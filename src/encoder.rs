//! Thread-confined NVENC/AMF wrapper. Synchronous GPU copy/encode keeps DXGI frame
//! ownership valid; only the compressed access unit is copied to host memory.
use crate::screen_capture::CapturedFrame;
use anyhow::{Result, anyhow, ensure};
use std::{
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
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
    fn hc_nvenc_close(handle: *mut c_void);
    fn hc_nvenc_preview(handle: *mut c_void) -> *mut c_void;
    fn hc_amf_open(
        device: *mut c_void,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        av1: u32,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
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
    fn hc_amf_close(handle: *mut c_void);
    fn hc_amf_preview(handle: *mut c_void) -> *mut c_void;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Codec {
    H265,
    Av1,
}

pub struct EncodedPacket<'a> {
    pub bytes: &'a [u8],
    pub timestamp_us: i64,
    pub keyframe: bool,
}

#[derive(Clone,Copy)]
enum Backend { Nvidia, Amd }

pub struct HardwareEncoder {
    backend:Backend,
    handle: NonNull<c_void>,
    output: Vec<u8>,
    poisoned: bool,
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
        let raw = unsafe {
            let open=match backend {Backend::Nvidia=>hc_nvenc_open,Backend::Amd=>hc_amf_open};
            open(
                device.as_raw(),
                width,
                height,
                fps,
                bitrate,
                matches!(codec, Codec::Av1) as u32,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        let handle = NonNull::new(raw).ok_or_else(|| anyhow!(error_text(&error)))?;
        Ok(Self {
            handle,backend,
            output: vec![0; 16 * 1024 * 1024],
            poisoned: false,
            _thread_affinity: PhantomData,
        })
    }

    pub fn reconfigure(&mut self, bitrate: u32, fps: u32) -> Result<()> {
        ensure!(!self.poisoned, "Encoder must be recreated after an error");
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
        self.poisoned = status != 0;
        ensure!(status == 0, "{}", error_text(&error));
        Ok(())
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
        Ok(EncodedPacket {
            bytes: &self.output[..length],
            timestamp_us,
            keyframe: key != 0,
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
