use eframe::egui;
use livekit::webrtc::video_frame::{BoxVideoFrame, I420Buffer, VideoFrame, VideoRotation};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) const LOCAL_CAMERA: &str = "/local-camera";
pub(crate) type PreviewFrame = (Arc<BoxVideoFrame>, Instant);

#[derive(Clone, Default)]
pub(crate) struct CameraPreview(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    visible: bool,
    latest: Option<PreviewFrame>,
}

impl CameraPreview {
    pub(crate) fn set_visible(&self, visible: bool) {
        let mut state = self.0.lock().unwrap();
        state.visible = visible;
        if !visible {
            state.latest = None;
        }
    }

    pub(crate) fn latest(&self) -> Option<PreviewFrame> {
        self.0.lock().unwrap().latest.clone()
    }

    pub(crate) fn capture(&self, buffer: &mut I420Buffer, ctx: &egui::Context) {
        let now = Instant::now();
        let mut state = self.0.lock().unwrap();
        if !state.visible
            || state
                .latest
                .as_ref()
                .is_some_and(|(_, at)| now.duration_since(*at) < Duration::from_millis(66))
        {
            return;
        }
        let buffer = buffer.scale(640, 360);
        let frame: BoxVideoFrame = VideoFrame::new(VideoRotation::VideoRotation0, Box::new(buffer));
        state.latest = Some((Arc::new(frame), now));
        drop(state);
        ctx.request_repaint();
    }
}

#[cfg(test)]
#[test]
fn hidden_preview_discards_frames_and_resumes_with_a_new_frame() {
    let preview = CameraPreview::default();
    let ctx = egui::Context::default();
    let mut frame = I420Buffer::new(1280, 720);
    preview.capture(&mut frame, &ctx);
    assert!(preview.latest().is_none());

    preview.set_visible(true);
    preview.capture(&mut frame, &ctx);
    let (first, _) = preview.latest().unwrap();
    assert_eq!((first.buffer.width(), first.buffer.height()), (640, 360));

    preview.set_visible(false);
    preview.capture(&mut frame, &ctx);
    assert!(preview.latest().is_none());

    preview.set_visible(true);
    assert!(preview.latest().is_none());
    preview.capture(&mut frame, &ctx);
    let (next, _) = preview.latest().unwrap();
    assert!(!Arc::ptr_eq(&first, &next));
}
