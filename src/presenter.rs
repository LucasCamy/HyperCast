use anyhow::{Result, anyhow, ensure};
use std::{
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};
unsafe extern "C" {
    fn hc_presenter_open(parent: *mut c_void) -> *mut c_void;
    fn hc_presenter_frame(
        handle: *mut c_void,
        texture: *mut c_void,
        slice: u32,
        fw: u32,
        fh: u32,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        error: *mut c_char,
        cap: usize,
    ) -> i32;
    fn hc_presenter_hide(handle: *mut c_void);
    fn hc_presenter_close(handle: *mut c_void);
}
pub struct Presenter {
    handle: NonNull<c_void>,
    _ui_thread: PhantomData<Rc<()>>,
    visible: bool,
}
impl Presenter {
    /// Parent is the live app window. Constructor and all calls stay on its UI thread.
    pub unsafe fn new(parent: *mut c_void) -> Result<Self> {
        let handle = NonNull::new(unsafe { hc_presenter_open(parent) })
            .ok_or_else(|| anyhow!("Cannot create DirectX presenter"))?;
        Ok(Self {
            handle,
            _ui_thread: PhantomData,
            visible: false,
        })
    }
    /// Borrowed texture must stay alive through the GPU fence inside this call.
    pub unsafe fn frame(
        &mut self,
        texture: *mut c_void,
        slice: u32,
        width: u32,
        height: u32,
        rect: [i32; 4],
    ) -> Result<()> {
        let mut error = [0; 1024];
        let status = unsafe {
            hc_presenter_frame(
                self.handle.as_ptr(),
                texture,
                slice,
                width,
                height,
                rect[0],
                rect[1],
                rect[2].max(0) as u32,
                rect[3].max(0) as u32,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        ensure!(status == 0, "{}", unsafe {
            CStr::from_ptr(error.as_ptr()).to_string_lossy()
        });
        self.visible = true;
        Ok(())
    }
    pub fn hide(&mut self) {
        self.visible = false;
        unsafe {
            hc_presenter_hide(self.handle.as_ptr());
        }
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
}
impl Drop for Presenter {
    fn drop(&mut self) {
        unsafe {
            hc_presenter_close(self.handle.as_ptr());
        }
    }
}
