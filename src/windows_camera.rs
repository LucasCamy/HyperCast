use anyhow::{Result, anyhow, bail};
use livekit::{
    options::{TrackPublishOptions, VideoEncoding},
    prelude::*,
    webrtc::{
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
    },
};
use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

unsafe extern "C" {
    fn hc_camera_open(error: *mut i32) -> *mut c_void;
    fn hc_camera_read(camera: *mut c_void, data: *mut u8, size: u32) -> i32;
    fn hc_camera_close(camera: *mut c_void);
}

struct Device(NonNull<c_void>);

impl Drop for Device {
    fn drop(&mut self) {
        // This handle stays on the COM thread that opened it.
        unsafe { hc_camera_close(self.0.as_ptr()) };
    }
}

pub(crate) struct Camera {
    cancel: Arc<AtomicBool>,
    worker: JoinHandle<()>,
    sid: Option<TrackSid>,
    errors: std::sync::mpsc::Receiver<String>,
}

impl Camera {
    pub(crate) async fn open(room: &Room) -> Result<Self> {
        let source = NativeVideoSource::new(
            VideoResolution {
                width: WIDTH,
                height: HEIGHT,
            },
            false,
        );
        let track =
            LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(source.clone()));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (error_tx, errors) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("hypercast-camera".into())
            .spawn(move || {
                let mut ready = Some(ready_tx);
                if let Err(error) = capture(source, &worker_cancel, &mut ready) {
                    if let Some(ready) = ready {
                        let _ = ready.send(Err(error));
                    } else {
                        let _ = error_tx.send(error.to_string());
                    }
                }
            })?;
        let mut camera = Self {
            cancel,
            worker,
            sid: None,
            errors,
        };
        tokio::time::timeout(Duration::from_secs(5), ready_rx).await???;
        let publication = room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    video_encoding: Some(VideoEncoding {
                        max_bitrate: 2_500_000,
                        max_framerate: 30.0,
                    }),
                    simulcast: false,
                    ..Default::default()
                },
            )
            .await?;
        camera.sid = Some(publication.sid());
        Ok(camera)
    }

    pub(crate) fn failure(&self) -> Option<String> {
        self.errors.try_recv().ok().or_else(|| {
            self.worker
                .is_finished()
                .then(|| "Camera stopped delivering video".into())
        })
    }

    pub(crate) async fn stop(mut self, room: &Room) {
        self.cancel.store(true, Ordering::Release);
        if let Some(sid) = self.sid.take() {
            let _ = room.local_participant().unpublish_track(&sid).await;
        }
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

fn capture(
    source: NativeVideoSource,
    cancel: &AtomicBool,
    ready: &mut Option<tokio::sync::oneshot::Sender<Result<()>>>,
) -> Result<()> {
    let mut error = 0;
    // The buffer passed to read is allocated for the fixed NV12 output format.
    let device = Device(
        NonNull::new(unsafe { hc_camera_open(&mut error) })
            .ok_or_else(|| anyhow!("Could not open camera (0x{:08X})", error as u32))?,
    );
    let luma = (WIDTH * HEIGHT) as usize;
    let mut nv12 = vec![0; luma * 3 / 2];
    while !cancel.load(Ordering::Acquire) {
        let code =
            unsafe { hc_camera_read(device.0.as_ptr(), nv12.as_mut_ptr(), nv12.len() as u32) };
        if code < 0 {
            bail!("Camera capture failed (0x{:08X})", code as u32);
        }
        if code != 0 {
            continue;
        }
        let mut buffer = I420Buffer::new(WIDTH, HEIGHT);
        let (y, u, v) = buffer.data_mut();
        y.copy_from_slice(&nv12[..luma]);
        for ((u, v), uv) in u.iter_mut().zip(v).zip(nv12[luma..].chunks_exact(2)) {
            *u = uv[0];
            *v = uv[1];
        }
        source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer));
        if let Some(ready) = ready.take() {
            let _ = ready.send(Ok(()));
        }
    }
    Ok(())
}
