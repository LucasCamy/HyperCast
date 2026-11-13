//! Local UI validation helper: publish a moving H.264 camera from a second
//! participant. Needs a local SFU and a camera-only token in the environment.

#![cfg(feature = "livekit-network")]

use anyhow::Result;
use livekit::{
    options::{TrackPublishOptions, VideoCodec, VideoEncoding},
    prelude::*,
    webrtc::{
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
    },
};
use std::time::{Duration, Instant};

#[tokio::test]
#[ignore = "requires a local HyperCast SFU and a camera test token"]
async fn publish_synthetic_camera() -> Result<()> {
    let url = std::env::var("HYPERCAST_TEST_URL")?;
    let seconds: u64 = std::env::var("HYPERCAST_TEST_SECONDS")?.parse()?;
    let mut options = RoomOptions::default();
    options.auto_subscribe = false;
    let (room, _events) = Room::connect(
        &url,
        &std::env::var("HYPERCAST_TEST_CAMERA_TOKEN")?,
        options,
    )
    .await?;
    let (width, height) = (640, 360);
    let source = NativeVideoSource::new(VideoResolution { width, height }, false);
    let camera =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(source.clone()));
    room.local_participant()
        .publish_track(
            LocalTrack::Video(camera),
            TrackPublishOptions {
                source: TrackSource::Camera,
                video_codec: VideoCodec::H264,
                video_encoding: Some(VideoEncoding {
                    max_bitrate: 1_500_000,
                    max_framerate: 30.0,
                }),
                simulcast: false,
                ..Default::default()
            },
        )
        .await?;
    println!("synthetic camera published");
    let start = Instant::now();
    let mut timer = tokio::time::interval(Duration::from_millis(33));
    let mut tick = 0u32;
    while start.elapsed() < Duration::from_secs(seconds) {
        timer.tick().await;
        tick += 1;
        let mut frame = I420Buffer::new(width, height);
        let (y, u, v) = frame.data_mut();
        // A bright bar sweeping across a dark background.
        let bar = (tick * 8) % width;
        for row in 0..height {
            for col in 0..width {
                let inside = col.abs_diff(bar) < 40;
                y[(row * width + col) as usize] = if inside { 220 } else { 40 };
            }
        }
        u.fill(90);
        v.fill(170);
        source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, frame));
    }
    let _ = room.close().await;
    Ok(())
}
