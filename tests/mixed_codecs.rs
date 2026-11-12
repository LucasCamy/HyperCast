//! Exercise codec negotiation with two video sources on one publisher connection.
//! Supply a 320x180 Annex B HEVC keyframe (VPS/SPS/PPS included) and two
//! local test tokens for the same room. The publisher token must allow
//! screen_share and camera; the viewer only needs permission to subscribe.

#![cfg(feature = "livekit-network")]

use anyhow::{Result, ensure};
use hypercast::network::EncodedPublisher;
use livekit::{
    options::{TrackPublishOptions, VideoCodec, VideoEncoding},
    prelude::*,
    webrtc::{
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
    },
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::test]
#[ignore = "requires local HyperCast SFU, generated HEVC access unit, and test room tokens"]
async fn screen_hevc_and_camera_h264_share_one_publisher() -> Result<()> {
    let url = std::env::var("HYPERCAST_TEST_URL")?;
    let screen_bytes = std::fs::read(std::env::var("HYPERCAST_TEST_HEVC_FRAME")?)?;
    let mut options = RoomOptions::default();
    options.auto_subscribe = false;
    options.single_peer_connection = false;
    let (viewer, mut events) = Room::connect(
        &url,
        &std::env::var("HYPERCAST_TEST_VIEWER_TOKEN")?,
        options.clone(),
    )
    .await?;
    let (room, _events) = Room::connect(
        &url,
        &std::env::var("HYPERCAST_TEST_PUBLISHER_TOKEN")?,
        options,
    )
    .await?;
    let room = Arc::new(room);
    let mut screen =
        EncodedPublisher::in_room(room.clone(), 320, 180, 15, 1_000_000, false).await?;
    let start = Instant::now();
    let mut timer = tokio::time::interval(Duration::from_millis(66));
    let screen_task = tokio::spawn(async move {
        loop {
            timer.tick().await;
            screen
                .push(&screen_bytes, start.elapsed().as_micros() as i64, true)
                .unwrap();
        }
    });

    async fn wait_for_codec(
        events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
        source: TrackSource,
        expected: &str,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(event) = events.recv().await {
                if let RoomEvent::TrackPublished { publication, .. } = event {
                    if publication.source() == source {
                        let actual = publication.mime_type();
                        ensure!(
                            actual.eq_ignore_ascii_case(expected),
                            "{source:?}: expected {expected}, got {actual}"
                        );
                        return Ok(());
                    }
                }
            }
            anyhow::bail!("spectator disconnected")
        })
        .await?
    }
    wait_for_codec(&mut events, TrackSource::Screenshare, "video/H265").await?;
    let source = NativeVideoSource::new(
        VideoResolution {
            width: 320,
            height: 180,
        },
        false,
    );
    let camera =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(source.clone()));
    room.local_participant()
        .publish_track(
            LocalTrack::Video(camera),
            TrackPublishOptions {
                source: TrackSource::Camera,
                video_codec: VideoCodec::H264,
                video_encoding: Some(VideoEncoding {
                    max_bitrate: 1_000_000,
                    max_framerate: 15.0,
                }),
                simulcast: false,
                ..Default::default()
            },
        )
        .await?;
    let camera_task = tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_millis(66));
        loop {
            timer.tick().await;
            let mut frame = I420Buffer::new(320, 180);
            let (y, u, v) = frame.data_mut();
            y.fill(160);
            u.fill(128);
            v.fill(128);
            source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, frame));
        }
    });
    let got_camera = wait_for_codec(&mut events, TrackSource::Camera, "video/H264").await;
    camera_task.abort();
    screen_task.abort();
    let _ = room.close().await;
    let _ = viewer.close().await;
    got_camera
}
