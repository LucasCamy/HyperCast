//! Thread-confined capture sources; application windows never fall back to desktop.
use crate::screen_capture::{CaptureConfig, CapturedFrame, ScreenCapture};
use anyhow::{Result, anyhow, ensure};
use std::{ffi::{c_char,c_void,CStr}, marker::PhantomData, ptr::NonNull, rc::Rc};
use windows::{core::Interface,Win32::Graphics::Direct3D11::{ID3D11Device,ID3D11Texture2D}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureTarget {
    Monitor { adapter:u32, output:u32 },
    Window { adapter:u32, hwnd:usize, pid:u32 },
}
impl CaptureTarget {
    pub fn process(self)->Option<u32>{match self {Self::Window{pid,..}=>Some(pid),_=>None}}
}
#[derive(Clone)]
pub struct ApplicationWindow {pub hwnd:usize,pub pid:u32,pub title:String}
unsafe extern "C" {
    fn hc_window_open(hwnd:usize,pid:u32,adapter:u32,error:*mut c_char,capacity:usize)->*mut c_void;
    fn hc_window_device(handle:*mut c_void)->*mut c_void;
    fn hc_window_size(handle:*mut c_void,width:*mut u32,height:*mut u32);
    fn hc_window_frame(handle:*mut c_void,texture:*mut *mut c_void,error:*mut c_char,capacity:usize)->i32;
    fn hc_window_close(handle:*mut c_void);
    fn hc_window_list(callback:unsafe extern "C" fn(usize,u32,*const u16,*mut c_void),data:*mut c_void);
}
pub fn windows()->Vec<ApplicationWindow>{
    unsafe extern "C" fn collect(hwnd:usize,pid:u32,title:*const u16,data:*mut c_void){
        // SAFETY: synchronous native enumeration supplies a NUL-terminated 512-unit array
        // and the exclusive Vec borrowed below. Never let a Rust panic cross the ABI.
        let mut len=0;while len<511 && unsafe{*title.add(len)}!=0 {len+=1;}
        let title=String::from_utf16_lossy(unsafe{std::slice::from_raw_parts(title,len)});
        unsafe{&mut *data.cast::<Vec<ApplicationWindow>>()}.push(ApplicationWindow{hwnd,pid,title});
    }
    let mut list=Vec::<ApplicationWindow>::new();
    unsafe{hc_window_list(collect,(&mut list as *mut Vec<ApplicationWindow>).cast());}
    list.sort_by(|a,b|a.title.to_lowercase().cmp(&b.title.to_lowercase()));list
}
pub struct WindowCapture {handle:NonNull<c_void>,device:ID3D11Device,_affinity:PhantomData<Rc<()>>}
impl WindowCapture {
    fn new(adapter:u32,hwnd:usize,pid:u32)->Result<Self>{
        let mut error=[0;1024];
        let handle=NonNull::new(unsafe{hc_window_open(hwnd,pid,adapter,error.as_mut_ptr(),error.len())})
            .ok_or_else(||anyhow!(unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy().into_owned()))?;
        // SAFETY: live capture returns a new owned COM device reference.
        let device=unsafe{ID3D11Device::from_raw(hc_window_device(handle.as_ptr()))};
        Ok(Self{handle,device,_affinity:PhantomData})
    }
    fn acquire(&mut self)->Result<Option<ID3D11Texture2D>>{
        let mut texture=std::ptr::null_mut();let mut error=[0;1024];
        let status=unsafe{hc_window_frame(self.handle.as_ptr(),&mut texture,error.as_mut_ptr(),error.len())};
        ensure!(status==0,"{}",unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy());
        Ok(if texture.is_null(){None}else{Some(unsafe{ID3D11Texture2D::from_raw(texture)})})
    }
}
impl Drop for WindowCapture {fn drop(&mut self){unsafe{hc_window_close(self.handle.as_ptr());}}}
pub enum VideoCapture {Monitor(ScreenCapture),Window(WindowCapture)}
pub enum CaptureFrame<'a>{Monitor(CapturedFrame<'a>),Window(ID3D11Texture2D)}
impl CaptureFrame<'_>{
    pub fn texture(&self)->&ID3D11Texture2D{match self{Self::Monitor(f)=>f.texture(),Self::Window(t)=>t}}
    pub fn release(self)->Result<()>{match self{Self::Monitor(f)=>f.release(),Self::Window(_)=>Ok(())}}
}
impl VideoCapture {
    pub fn new(target:CaptureTarget)->Result<Self>{Ok(match target{
        CaptureTarget::Monitor{adapter,output}=>Self::Monitor(ScreenCapture::new(CaptureConfig{adapter_index:adapter,output_index:output})?),
        CaptureTarget::Window{adapter,hwnd,pid}=>Self::Window(WindowCapture::new(adapter,hwnd,pid)?),
    })}
    pub fn device(&self)->&ID3D11Device{match self{Self::Monitor(c)=>c.device(),Self::Window(c)=>&c.device}}
    pub fn dimensions(&self)->(u32,u32){match self{Self::Monitor(c)=>c.dimensions(),Self::Window(c)=>{let(mut w,mut h)=(0,0);unsafe{hc_window_size(c.handle.as_ptr(),&mut w,&mut h);}(w,h)}}}
    pub fn acquire(&mut self)->Result<Option<CaptureFrame<'_>>>{match self{Self::Monitor(c)=>Ok(c.acquire()?.map(CaptureFrame::Monitor)),Self::Window(c)=>Ok(c.acquire()?.map(CaptureFrame::Window))}}
}
