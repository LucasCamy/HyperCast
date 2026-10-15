//! Thread-confined NVENC/AMF wrapper. Synchronous GPU copy/encode keeps DXGI frame
//! ownership valid; only the compressed access unit is copied to host memory.
use crate::screen_capture::CapturedFrame;
use anyhow::{Result, anyhow, ensure};
use std::{
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
    time::{Duration,Instant},
};
use windows::{
    Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D},
    core::Interface,
};

unsafe extern "C" {
    fn hc_nvenc_open(
        device: *mut c_void,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        av1: u32,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn hc_nvenc_encode(
        handle: *mut c_void,
        texture: *mut c_void,
        timestamp: u64,
        keyframe: u32,
        bytes: *mut u8,
        capacity: usize,
        length: *mut usize,
        is_key: *mut u32,
        error: *mut c_char,
        error_capacity: usize,
    ) -> i32;
    fn hc_nvenc_rates(
        handle: *mut c_void,
        bitrate: u32,
        fps: u32,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    #[cfg(test)]
    fn hc_nvenc_live_encoders()->u32;
    #[cfg(test)]
    fn hc_amf_live_encoders()->u32;
    fn hc_nvenc_size(handle:*mut c_void,w:*mut u32,h:*mut u32);
    fn hc_amf_size(handle:*mut c_void,w:*mut u32,h:*mut u32);
    fn hc_nvenc_close(handle: *mut c_void);
    fn hc_nvenc_preview(handle: *mut c_void) -> *mut c_void;
    fn hc_amf_encode(
        handle: *mut c_void,
        texture: *mut c_void,
        timestamp: u64,
        keyframe: u32,
        bytes: *mut u8,
        capacity: usize,
        length: *mut usize,
        is_key: *mut u32,
        error: *mut c_char,
        error_capacity: usize,
    ) -> i32;
    fn hc_amf_rates(
        handle: *mut c_void,
        bitrate: u32,
        fps: u32,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn hc_amf_open_profile(device:*mut c_void,width:u32,height:u32,fps:u32,bitrate:u32,av1:u32,profile:u32,error:*mut c_char,capacity:usize)->*mut c_void;
    fn hc_amf_feedback(handle:*mut c_void,qp:*mut i32,bitrate:*mut u32,vbv:*mut u32,fps:*mut u32,runtime:*mut u32);
    fn hc_amf_close(handle: *mut c_void);
    fn hc_amf_preview(handle: *mut c_void) -> *mut c_void;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Codec {
    H265,
    Av1,
}

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,clap::ValueEnum)]
#[repr(u32)]
pub enum AmdProfile { #[default] Fast=0, Text=1, Compatibility=2 }
impl AmdProfile {
    pub fn label(self)->&'static str {match self{Self::Fast=>"Fluidez",Self::Text=>"Texto · experimental",Self::Compatibility=>"Compatibilidade"}}
}
#[derive(Debug,Default)]
pub struct EncoderFeedback {pub qp:Option<u32>,pub bitrate:u32,pub vbv_bits:Option<u32>,pub fps:u32,pub vbv_runtime:Option<bool>}
pub struct EncodedPacket<'a> {
    pub bytes: &'a [u8],
    pub timestamp_us: i64,
    pub keyframe: bool,
    pub encode_time:Duration,
    pub qp:Option<u32>,
}

#[derive(Clone,Copy)]
enum Backend { Nvidia, Amd }

pub struct HardwareEncoder {
    backend:Backend,
    handle: NonNull<c_void>,
    output: Vec<u8>,
    poisoned: bool,
    requested_rate:(u32,u32),
    actual_profile:AmdProfile,
    _thread_affinity: PhantomData<Rc<()>>,
}

fn error_text(buffer: &[c_char]) -> String {
    // SAFETY: initialized to zero and native report() always writes a NUL.
    unsafe {
        CStr::from_ptr(buffer.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

impl HardwareEncoder {
    pub fn dimensions(&self)->(u32,u32){
        let (mut w,mut h)=(0,0);
        // SAFETY: handle is exclusively owned; bridge writes two bounded u32 values.
        unsafe {(match self.backend {Backend::Nvidia=>hc_nvenc_size,Backend::Amd=>hc_amf_size})(self.handle.as_ptr(),&mut w,&mut h);}
        (w,h)
    }
    pub fn profile(&self)->Option<AmdProfile>{matches!(self.backend,Backend::Amd).then_some(self.actual_profile)}
    pub fn backend_name(&self)->&'static str {match self.backend {Backend::Nvidia=>"NVENC",Backend::Amd=>"AMF"}}
    pub fn preview_texture(&self) -> Option<ID3D11Texture2D> {
        // SAFETY: native helper returns a new owned COM reference, never the
        // borrowed duplication surface. Its contents are synchronized in GPU.
        let raw = unsafe { match self.backend {Backend::Nvidia=>hc_nvenc_preview(self.handle.as_ptr()),Backend::Amd=>hc_amf_preview(self.handle.as_ptr())} };
        if raw.is_null() {
            None
        } else {
            Some(unsafe { ID3D11Texture2D::from_raw(raw) })
        }
    }
    pub fn new(
        device: &ID3D11Device,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        codec: Codec,
    ) -> Result<Self> {
        Self::new_with_profile(device,width,height,fps,bitrate,codec,AmdProfile::Fast)
    }
    pub fn new_with_profile(device:&ID3D11Device,width:u32,height:u32,fps:u32,bitrate:u32,codec:Codec,profile:AmdProfile)->Result<Self> {
        // SAFETY: the borrowed live device supports IDXGIDevice; returned COM
        // references retain the adapter through the descriptor query.
        let backend=unsafe {
            let dxgi:windows::Win32::Graphics::Dxgi::IDXGIDevice=device.cast()?;
            let desc=dxgi.GetAdapter()?.GetDesc()?;
            match desc.VendorId {0x10de=>Backend::Nvidia,0x1002=>Backend::Amd,_=>anyhow::bail!("GPU sem encoder suportado; use NVIDIA NVENC ou AMD AMF")}
        };
        let mut error = [0; 1024];
        // SAFETY: device remains alive during open; bridge retains its own COM
        // reference. Error output is bounded. Opaque handle is uniquely owned.
        let mut actual_profile=if codec==Codec::Av1{AmdProfile::Fast}else{profile};
        let mut raw = unsafe {match backend {
            Backend::Nvidia=>hc_nvenc_open(device.as_raw(),width,height,fps,bitrate,matches!(codec,Codec::Av1) as u32,error.as_mut_ptr(),error.len()),
            Backend::Amd=>hc_amf_open_profile(device.as_raw(),width,height,fps,bitrate,matches!(codec,Codec::Av1) as u32,if codec==Codec::Av1{AmdProfile::Fast as u32}else{profile as u32},error.as_mut_ptr(),error.len()),
        }};
        if raw.is_null() && matches!(backend,Backend::Amd) && actual_profile==AmdProfile::Text {
            actual_profile=AmdProfile::Fast;
            // SAFETY: the failed native constructor has already released its partial session.
            raw=unsafe{hc_amf_open_profile(device.as_raw(),width,height,fps,bitrate,matches!(codec,Codec::Av1) as u32,actual_profile as u32,error.as_mut_ptr(),error.len())};
        }
        let handle = NonNull::new(raw).ok_or_else(|| anyhow!(error_text(&error)))?;
        Ok(Self {
            handle,backend,actual_profile,
            output: vec![0; 16 * 1024 * 1024],
            poisoned: false,requested_rate:(bitrate,fps),
            _thread_affinity: PhantomData,
        })
    }

    pub fn feedback(&self)->EncoderFeedback {
        let mut feedback=EncoderFeedback{bitrate:self.requested_rate.0,fps:self.requested_rate.1,..Default::default()};
        if matches!(self.backend,Backend::Amd){
            let(mut qp,mut bitrate,mut vbv,mut fps,mut runtime)=(-1,0,0,0,0);
            // SAFETY: live uniquely owned native encoder; outputs point to initialized scalars.
            unsafe{hc_amf_feedback(self.handle.as_ptr(),&mut qp,&mut bitrate,&mut vbv,&mut fps,&mut runtime);}
            feedback.qp=(0..=51).contains(&qp).then_some(qp as u32);feedback.bitrate=bitrate;feedback.fps=fps;feedback.vbv_bits=Some(vbv);feedback.vbv_runtime=Some(runtime!=0);
        }
        feedback
    }
    /// False means a driver rejected this budget: pause until a new supported target.
    /// It must never turn a transient network limit into a destroyed encoder.
    pub fn reconfigure(&mut self, bitrate: u32, fps: u32) -> Result<bool> {
        ensure!(!self.poisoned, "Encoder must be recreated after an error");
        ensure!((64000..=45000000).contains(&bitrate) && (1..=240).contains(&fps),"Invalid rate target");
        if self.requested_rate==(bitrate,fps){return Ok(true);}
        let mut error = [0; 1024];
        // SAFETY: valid unique live handle, bounded writable error buffer.
        let status = unsafe {
            let rates=match self.backend {Backend::Nvidia=>hc_nvenc_rates,Backend::Amd=>hc_amf_rates};
            rates(
                self.handle.as_ptr(),
                bitrate,
                fps,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if status==2 && matches!(self.backend,Backend::Amd){self.requested_rate=(0,0);return Ok(false);}
        self.poisoned = status != 0;
        ensure!(status == 0, "{}", error_text(&error));
        self.requested_rate=(bitrate,fps);
        Ok(true)
    }

    pub fn encode<'a>(
        &'a mut self,
        frame: Option<&CapturedFrame<'_>>,
        timestamp_us: i64,
        force_keyframe: bool,
    ) -> Result<EncodedPacket<'a>> {
        self.encode_texture(frame.map(|f|f.texture()), timestamp_us, force_keyframe)
    }
    pub fn encode_texture<'a>(&'a mut self, frame: Option<&ID3D11Texture2D>, timestamp_us:i64, force_keyframe:bool)->Result<EncodedPacket<'a>> {
        ensure!(!self.poisoned, "Encoder must be recreated after an error");
        ensure!(timestamp_us >= 0, "Negative capture timestamp");
        let mut error = [0; 1024];
        let mut length = 0;
        let mut key = 0;
        // SAFETY: both COM texture and exclusive handle are live for this
        // synchronous call. Output/metadata pointers have stated capacities.
        let texture = frame
            .map(|f| f.as_raw())
            .unwrap_or(std::ptr::null_mut());
        let encode_start=Instant::now();
        let status = unsafe {
            let encode=match self.backend {Backend::Nvidia=>hc_nvenc_encode,Backend::Amd=>hc_amf_encode};
            encode(
                self.handle.as_ptr(),
                texture,
                timestamp_us as u64,
                force_keyframe as u32,
                self.output.as_mut_ptr(),
                self.output.len(),
                &mut length,
                &mut key,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        self.poisoned = status != 0;
        ensure!(status == 0, "{}", error_text(&error));
        ensure!(
            length <= self.output.len(),
            "Native encoder returned an invalid length"
        );
        let encode_time=encode_start.elapsed();
        let qp=self.feedback().qp;
        Ok(EncodedPacket {
            bytes: &self.output[..length],
            timestamp_us,
            keyframe: key != 0,encode_time,qp,
        })
    }
}

impl Drop for HardwareEncoder {
    fn drop(&mut self) {
        // SAFETY: owns exactly one handle; no output borrows survive this Drop.
        unsafe {
            match self.backend {Backend::Nvidia=>hc_nvenc_close(self.handle.as_ptr()),Backend::Amd=>hc_amf_close(self.handle.as_ptr())};
        }
    }
}

#[cfg(test)]
mod ultrawide_tests {
    use super::*;
    use windows::Win32::{Foundation::HMODULE,Graphics::{
        Direct3D::{D3D_DRIVER_TYPE_UNKNOWN,D3D_FEATURE_LEVEL_11_0},
        Direct3D11::*,Dxgi::{CreateDXGIFactory1,IDXGIFactory1,Common::*}}};
    #[test]
    #[ignore = "Requires NVIDIA GPU with AV1 and HEVC hardware encoding"]
    fn generated_ultrawide_gpu_lifecycle()->Result<()> {
        // Generated solid GPU textures only: no monitor acquisition or desktop readback.
        let (mut device,mut context)=(None,None);
        unsafe {
            let factory:IDXGIFactory1=CreateDXGIFactory1()?;let adapter=factory.EnumAdapters1(0)?;
            D3D11CreateDevice(&adapter,D3D_DRIVER_TYPE_UNKNOWN,HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),D3D11_SDK_VERSION,Some(&mut device),None,Some(&mut context))?;
        }
        let device=device.unwrap();let context=context.unwrap();
        let baseline=unsafe{hc_nvenc_live_encoders()+hc_amf_live_encoders()};
        for codec in [Codec::H265,Codec::Av1] {
            for _ in 0..3 {
                let desc=D3D11_TEXTURE2D_DESC{Width:4587,Height:1920,MipLevels:1,ArraySize:1,
                    Format:DXGI_FORMAT_B8G8R8A8_UNORM,SampleDesc:DXGI_SAMPLE_DESC{Count:1,Quality:0},
                    Usage:D3D11_USAGE_DEFAULT,BindFlags:(D3D11_BIND_RENDER_TARGET.0|D3D11_BIND_SHADER_RESOURCE.0) as u32,..Default::default()};
                let (mut texture,mut target)=(None,None);
                unsafe {device.CreateTexture2D(&desc,None,Some(&mut texture))?;device.CreateRenderTargetView(texture.as_ref().unwrap(),None,Some(&mut target))?;
                    context.ClearRenderTargetView(target.as_ref().unwrap(),&[0.2,0.5,0.8,1.0]);}
                // A red last source column detects accidental crop or stretching of native pixels.
                let column_desc=D3D11_TEXTURE2D_DESC{Width:1,..desc};
                let (mut column,mut column_target)=(None,None);
                unsafe {device.CreateTexture2D(&column_desc,None,Some(&mut column))?;
                    device.CreateRenderTargetView(column.as_ref().unwrap(),None,Some(&mut column_target))?;
                    context.ClearRenderTargetView(column_target.as_ref().unwrap(),&[1.0,0.0,0.0,1.0]);
                    context.CopySubresourceRegion(texture.as_ref().unwrap(),0,4586,0,0,column.as_ref().unwrap(),0,None);}
                let mut encoder=HardwareEncoder::new(&device,4587,1920,120,40_000_000,codec)?;
                assert_eq!(encoder.dimensions(),(4588,1920));
                for n in 0..3 {
                    let packet=encoder.encode_texture(texture.as_ref(),n*8333,n==0)?;
                    assert!(!packet.bytes.is_empty());
                }
                let preview=encoder.preview_texture().unwrap();
                let mut read_desc=Default::default();unsafe{preview.GetDesc(&mut read_desc);}
                read_desc.Usage=D3D11_USAGE_STAGING;read_desc.BindFlags=0;read_desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ.0 as u32;
                let mut staging=None;let mut mapped=D3D11_MAPPED_SUBRESOURCE::default();
                // Readback exists only in this generated-content test, never in the capture pipeline.
                let pixels=unsafe {
                    device.CreateTexture2D(&read_desc,None,Some(&mut staging))?;
                    let staging=staging.as_ref().unwrap();context.CopyResource(staging,&preview);
                    context.Map(staging,0,D3D11_MAP_READ,0,Some(&mut mapped))?;
                    if mapped.pData.is_null() || mapped.RowPitch<4588*4 {context.Unmap(staging,0);anyhow::bail!("Invalid synthetic mapped row");}
                    let row=mapped.pData.cast::<u32>();
                    let pixels=(row.add(4586).read_unaligned(),row.add(4587).read_unaligned());
                    context.Unmap(staging,0);pixels
                };
                assert_eq!(pixels,(0xffff0000,0xffff0000),"Native edge must be preserved and padded, not cropped or blurred");
                drop(preview);
                drop(encoder);
                assert_eq!(unsafe{hc_nvenc_live_encoders()+hc_amf_live_encoders()},baseline);
                assert!(HardwareEncoder::new(&device,8192,8192,120,40_000_000,codec).is_err());
                assert_eq!(unsafe{hc_nvenc_live_encoders()+hc_amf_live_encoders()},baseline);
                // A mismatched source poisons one owner; dropping it still releases its session.
                let mut bad=HardwareEncoder::new(&device,1920,1080,120,40_000_000,codec)?;
                assert!(bad.encode_texture(texture.as_ref(),0,true).is_err());drop(bad);
                assert_eq!(unsafe{hc_nvenc_live_encoders()+hc_amf_live_encoders()},baseline);
            }
        }
        Ok(())
    }
}
