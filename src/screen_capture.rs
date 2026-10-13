//! Direct DXGI texture access, with no CPU mapping or readback.
//! A frame belongs to duplication until release. An asynchronous encoder must
//! copy into its OWN GPU texture pool and fence that copy before releasing it.
use anyhow::{Context, Result, anyhow, bail};
use std::{
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1},
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
            },
            Dxgi::{
                Common::DXGI_MODE_ROTATION_IDENTITY, CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST,
                DXGI_ERROR_NOT_FOUND, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
                IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
            },
        },
    },
    core::Interface,
};

#[derive(Clone, Copy, Debug)]
pub struct CaptureConfig {
    pub adapter_index: u32,
    pub output_index: u32,
}

#[derive(Debug)]
pub struct Display {
    pub adapter_index: u32,
    pub output_index: u32,
    pub gpu_name: String,
    pub display_name: String,
    pub width: u32,
    pub height: u32,
}

/// Enumerate actual DXGI adapter/output pairs rather than assuming adapter zero.
pub fn displays() -> Result<Vec<Display>> {
    let mut displays = Vec::new();
    // SAFETY: windows-rs owns the returned COM objects and initializes outputs.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        for adapter_index in 0.. {
            let adapter = match factory.EnumAdapters1(adapter_index) {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(error.into()),
            };
            let gpu = adapter.GetDesc1()?;
            for output_index in 0.. {
                let output = match adapter.EnumOutputs(output_index) {
                    Ok(output) => output,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(error.into()),
                };
                let desc = output.GetDesc()?;
                if !desc.AttachedToDesktop.as_bool() {
                    continue;
                }
                let text = |buffer: &[u16]| {
                    String::from_utf16_lossy(
                        &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
                    )
                };
                displays.push(Display {
                    adapter_index,
                    output_index,
                    gpu_name: text(&gpu.Description),
                    display_name: text(&desc.DeviceName),
                    width: (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as u32,
                    height: (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as u32,
                });
            }
        }
    }
    Ok(displays)
}

pub struct ScreenCapture {
    config: CaptureConfig,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    // Immediate context and frame ownership remain on the capture thread.
    _thread_affinity: PhantomData<Rc<()>>,
}

pub struct CapturedFrame<'a> {
    capture: &'a mut ScreenCapture,
    texture: ID3D11Texture2D,
    pub info: DXGI_OUTDUPL_FRAME_INFO,
    pub acquired_at: Instant,
    pub acquire_duration: Duration,
    released: bool,
}

impl ScreenCapture {
    pub fn new(config: CaptureConfig) -> Result<Self> {
        // SAFETY: COM output pointers are initialized by windows-rs; selected
        // adapter owns the selected output, and the device uses that adapter.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            let adapter = factory
                .EnumAdapters1(config.adapter_index)
                .context("GPU adapter not found")?;
            let output: IDXGIOutput1 = adapter
                .EnumOutputs(config.output_index)
                .context("Monitor not found on this GPU")?
                .cast()?;
            let desc = output.GetDesc()?;
            if !desc.AttachedToDesktop.as_bool() {
                bail!("Monitor is not attached to desktop");
            }
            if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY {
                bail!("Rotated monitors need a GPU rotation pass (not implemented yet)");
            }
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .context("D3D11 hardware device creation failed")?;
            let device = device.ok_or_else(|| anyhow!("D3D11 returned no device"))?;
            let context = context.ok_or_else(|| anyhow!("D3D11 returned no context"))?;
            let duplication = output
                .DuplicateOutput(&device)
                .context("Desktop duplication unavailable; use an interactive Windows session")?;
            Ok(Self {
                config,
                device,
                context,
                duplication,
                _thread_affinity: PhantomData,
            })
        }
    }

    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }
    /// Actual duplication surface dimensions, independent of desktop scaling.
    pub fn dimensions(&self) -> (u32, u32) {
        let desc = unsafe { self.duplication.GetDesc() };
        (desc.ModeDesc.Width, desc.ModeDesc.Height)
    }
    pub fn context(&self) -> &ID3D11DeviceContext {
        &self.context
    }

    /// Poll at the pacer's tick. Timeout zero keeps acquisition from blocking.
    /// On display/session changes the duplication is recreated; no stale frame
    /// is returned. Device loss requires rebuilding the whole GPU pipeline.
    pub fn acquire(&mut self) -> Result<Option<CapturedFrame<'_>>> {
        let start = Instant::now();
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        // SAFETY: no frame is held: the mutable borrow prevents another acquire
        // until the previous CapturedFrame has released its frame.
        let result = unsafe {
            self.duplication
                .AcquireNextFrame(0, &mut info, &mut resource)
        };
        if let Err(error) = result {
            if error.code() == DXGI_ERROR_WAIT_TIMEOUT {
                return Ok(None);
            }
            if error.code() == DXGI_ERROR_ACCESS_LOST {
                // Recreate duplication on the SAME device, preserving encoder bindings.
                unsafe {
                    let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
                    let adapter = factory.EnumAdapters1(self.config.adapter_index)?;
                    let output: IDXGIOutput1 =
                        adapter.EnumOutputs(self.config.output_index)?.cast()?;
                    if output.GetDesc()?.Rotation != DXGI_MODE_ROTATION_IDENTITY {
                        bail!("Monitor rotation changed; restart after restoring landscape mode");
                    }
                    self.duplication = output
                        .DuplicateOutput(&self.device)
                        .context("Display changed; duplication recovery failed")?;
                }
                return Ok(None);
            }
            return Err(error)
                .context("AcquireNextFrame failed; rebuild GPU pipeline on device loss");
        }
        let texture = resource
            .ok_or_else(|| anyhow!("DXGI returned no texture"))
            .and_then(|r| r.cast::<ID3D11Texture2D>().map_err(Into::into));
        match texture {
            Ok(texture) => Ok(Some(CapturedFrame {
                capture: self,
                texture,
                info,
                acquired_at: Instant::now(),
                acquire_duration: start.elapsed(),
                released: false,
            })),
            Err(error) => {
                // SAFETY: AcquireNextFrame succeeded, so this path owes a ReleaseFrame.
                unsafe {
                    self.duplication.ReleaseFrame()?;
                }
                Err(error)
            }
        }
    }
}

impl CapturedFrame<'_> {
    /// Valid only until release. Do not retain a cloned COM handle after release:
    /// the OS invalidates the surface contents independently of COM refcount.
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    pub fn release(mut self) -> Result<()> {
        self.released = true;
        // SAFETY: this guard exclusively owns a successfully acquired frame.
        unsafe {
            self.capture
                .duplication
                .ReleaseFrame()
                .context("ReleaseFrame failed")
        }
    }
}

impl Drop for CapturedFrame<'_> {
    fn drop(&mut self) {
        if !self.released {
            // SAFETY: same exclusive frame ownership; Drop cannot report errors.
            unsafe {
                let _ = self.capture.duplication.ReleaseFrame();
            }
        }
    }
}
