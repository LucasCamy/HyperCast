use anyhow::{Context, Result, ensure};
use gst::prelude::*;
use gstreamer as gst;
use livekit::{
    options::{TrackPublishOptions, VideoCodec, VideoEncoding},
    prelude::*,
    webrtc::{
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
    },
};
use std::time::{Duration, Instant};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

pub(super) struct Camera {
    pipeline: gst::Pipeline,
    sink: gstreamer_app::AppSink,
    source: NativeVideoSource,
    sid: Option<TrackSid>,
    last_frame: Instant,
}

impl Camera {
    pub(super) async fn open(room: &Room) -> Result<Self> {
        let mut camera = Self::start_capture("v4l2src").await?;
        let track = LocalVideoTrack::create_video_track(
            "camera",
            RtcVideoSource::Native(camera.source.clone()),
        );
        let publication = room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    video_codec: VideoCodec::H264,
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

    async fn start_capture(input: &str) -> Result<Self> {
        gst::init()?;
        let pipeline = gst::parse::launch(
            format!("{input} ! decodebin ! videoconvert ! videoscale ! videorate ! video/x-raw,format=I420,width={WIDTH},height={HEIGHT},framerate=30/1 ! appsink name=camera max-buffers=1 drop=true sync=false").as_str(),
        )?.downcast::<gst::Pipeline>().ok().context("Expected a camera pipeline")?;
        let sink = pipeline
            .by_name("camera")
            .context("Missing camera sink")?
            .downcast::<gstreamer_app::AppSink>()
            .ok()
            .context("Invalid camera sink")?;
        let source = NativeVideoSource::new(
            VideoResolution {
                width: WIDTH,
                height: HEIGHT,
            },
            false,
        );
        let mut camera = Self {
            pipeline,
            sink,
            source,
            sid: None,
            last_frame: Instant::now(),
        };
        camera.pipeline.set_state(gst::State::Playing)?;
        let first_sample = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(sample) = camera.sink.try_pull_sample(gst::ClockTime::ZERO) {
                    return Ok(sample);
                }
                if let Some(message) = camera
                    .pipeline
                    .bus()
                    .and_then(|bus| bus.pop_filtered(&[gst::MessageType::Error]))
                {
                    if let gst::MessageView::Error(error) = message.view() {
                        return Err(anyhow::anyhow!("{}", error.error()));
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("Camera did not deliver a frame")??;
        camera.push_sample(first_sample)?;
        Ok(camera)
    }

    pub(super) fn tick(&mut self) -> Result<()> {
        let Some(sample) = self.sink.try_pull_sample(gst::ClockTime::ZERO) else {
            ensure!(
                self.last_frame.elapsed() < Duration::from_secs(5),
                "Camera stopped delivering video"
            );
            return Ok(());
        };
        self.push_sample(sample)
    }

    fn push_sample(&mut self, sample: gst::Sample) -> Result<()> {
        let info =
            gstreamer_video::VideoInfo::from_caps(sample.caps().context("Missing camera format")?)?;
        let buffer = sample.buffer_owned().context("Missing camera frame")?;
        let mapped = gstreamer_video::VideoFrame::from_buffer_readable(buffer, &info)
            .map_err(|_| anyhow::anyhow!("Could not map camera frame"))?;
        let mut frame = I420Buffer::new(WIDTH, HEIGHT);
        let (y, u, v) = frame.data_mut();
        for (index, destination) in [y, u, v].into_iter().enumerate() {
            let width = if index == 0 { WIDTH } else { WIDTH / 2 } as usize;
            let stride = usize::try_from(info.stride()[index])?;
            let plane = mapped.plane_data(index as u32)?;
            ensure!(stride >= width, "Invalid camera frame stride");
            for (row, output) in destination.chunks_exact_mut(width).enumerate() {
                let start = row * stride;
                output.copy_from_slice(
                    plane
                        .get(start..start + width)
                        .context("Incomplete camera frame")?,
                );
            }
        }
        self.source
            .capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, frame));
        self.last_frame = Instant::now();
        Ok(())
    }

    pub(super) async fn stop(mut self, room: &Room) {
        let _ = self.pipeline.set_state(gst::State::Null);
        if let Some(sid) = self.sid.take() {
            let _ = room.local_participant().unpublish_track(&sid).await;
        }
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
#[tokio::test]
async fn simulated_webcam_delivers_frames_through_camera_capture() -> Result<()> {
    let mut camera = Camera::start_capture("videotestsrc is-live=true pattern=ball").await?;
    let first_frame = camera.last_frame;
    tokio::time::timeout(Duration::from_secs(2), async {
        while camera.last_frame == first_frame {
            tokio::time::sleep(Duration::from_millis(20)).await;
            camera.tick()?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}
