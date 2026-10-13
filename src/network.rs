//! Encoded access-unit publishing through WebRTC; no raw texture/RAM bridge.
use anyhow::{Context, Result, ensure};
use livekit::{
    options::{VideoCodec, VideoEncoderBackend, VideoEncoding},
    prelude::*,
    webrtc::{
        video_frame::{EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame},
        video_source::{RtcVideoSource, VideoResolution, native::NativeVideoSource},
    },
};
use std::time::Duration;

pub fn token_from_env() -> Result<String> {
    std::env::var("HYPERCAST_ROOM_TOKEN")
        .context("Set HYPERCAST_ROOM_TOKEN to a room JWT (never an API secret)")
}

fn validate_url(address: &str) -> Result<()> {
    let url = url::Url::parse(address)?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    ensure!(
        url.scheme() == "wss" || (url.scheme() == "ws" && loopback),
        "Use WSS; plaintext WS is allowed only on loopback"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are not supported"
    );
    Ok(())
}

async fn connect(
    address: &str,
    token: &str,
) -> Result<(Room, tokio::sync::mpsc::UnboundedReceiver<RoomEvent>)> {
    validate_url(address)?;
    let mut options = RoomOptions::default();
    options.auto_subscribe = false;
    options.adaptive_stream = true;
    options.dynacast = false;
    options.join_retries = 0;
    // The SDK's default single-PC mode produced conflicting ICE ufrags against
    // the pinned SFU during native publication. Dual transports avoid that path.
    options.single_peer_connection = false;
    options.connect_timeout = Duration::from_secs(10);
    Ok(Room::connect(address, token, options).await?)
}

pub async fn diagnostic(address: &str, token: &str, seconds: u32) -> Result<()> {
    let (room, mut events) = connect(address, token).await?;
    println!(
        "Connected to room {} as {} (auto-subscribe disabled)",
        room.name(),
        room.local_participant().identity()
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds as u64);
    while let Ok(Some(_event)) = tokio::time::timeout_at(deadline, events.recv()).await {
        // Do not log full SDK events: metadata/payloads may contain private data.
    }
    room.close().await?;
    Ok(())
}

pub struct EncodedPublisher {
    room: Room,
    source: NativeVideoSource,
    codec: EncodedVideoCodec,
    resolution: VideoResolution,
    last_timestamp: Option<i64>,
    seen_keyframe: bool,
    event_task: tokio::task::JoinHandle<()>,
}

impl EncodedPublisher {
    pub async fn open(
        address: &str,
        token: &str,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        av1: bool,
    ) -> Result<Self> {
        let (room, mut events) = connect(address, token).await?;
        let resolution = VideoResolution { width, height };
        let source = NativeVideoSource::new_encoded(resolution.clone());
        let track =
            LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(source.clone()));
        let publication = room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track),
                livekit::options::TrackPublishOptions {
                    source: TrackSource::Screenshare,
                    video_codec: if av1 {
                        VideoCodec::AV1
                    } else {
                        VideoCodec::H265
                    },
                    video_encoder: VideoEncoderBackend::PreEncoded,
                    simulcast: false,
                    video_encoding: Some(VideoEncoding {
                        max_bitrate: bitrate as u64,
                        max_framerate: fps as f64,
                    }),
                    ..Default::default()
                },
            )
            .await?;
        println!(
            "Published screen track {} in {}",
            publication.sid(),
            room.name()
        );
        let event_task = tokio::spawn(async move { while events.recv().await.is_some() {} });
        Ok(Self {
            room,
            source,
            resolution,
            codec: if av1 {
                EncodedVideoCodec::AV1
            } else {
                EncodedVideoCodec::H265
            },
            last_timestamp: None,
            seen_keyframe: false,
            event_task,
        })
    }

    pub fn take_keyframe_request(&self) -> bool {
        self.source.take_keyframe_request()
    }
    pub fn take_rate_control_request(
        &self,
    ) -> Option<livekit::webrtc::video_source::EncodedRateControl> {
        self.source.take_rate_control_request()
    }
    /// Complete access unit (HEVC Annex B / AV1 OBU), not individual RTP packets.
    pub fn push(&mut self, bytes: &[u8], timestamp_us: i64, keyframe: bool) -> Result<bool> {
        ensure!(
            self.room.connection_state() == ConnectionState::Connected,
            "Room disconnected/reconnecting; restart publication with a keyframe"
        );
        ensure!(
            !bytes.is_empty() && bytes.len() <= 16 * 1024 * 1024,
            "Invalid encoded access-unit size"
        );
        ensure!(
            timestamp_us >= 0 && self.last_timestamp.is_none_or(|last| timestamp_us > last),
            "Capture timestamps must increase monotonically"
        );
        ensure!(
            self.seen_keyframe || keyframe,
            "First access unit must be a keyframe"
        );
        let accepted = self.source.capture_encoded_frame(&EncodedVideoFrame {
            codec: self.codec,
            payload: bytes,
            timestamp_us,
            frame_type: if keyframe {
                EncodedFrameType::Key
            } else {
                EncodedFrameType::Delta
            },
            resolution: self.resolution.clone(),
            frame_metadata: None,
        });
        if accepted {
            self.last_timestamp = Some(timestamp_us);
            self.seen_keyframe |= keyframe;
        }
        Ok(accepted)
    }
    pub async fn close(self) -> Result<()> {
        let stats = self.room.get_stats().await?;
        for entry in stats.publisher_stats {
            if let livekit::webrtc::stats::RtcStats::OutboundRtp(rtp) = entry {
                println!(
                    "WebRTC outbound: {} packets, {} bytes, {} frames sent",
                    rtp.sent.packets_sent, rtp.sent.bytes_sent, rtp.outbound.frames_sent
                );
            }
        }
        self.event_task.abort();
        self.room.close().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_transport_requires_tls_outside_loopback() {
        assert!(validate_url("wss://screen.example.com").is_ok());
        assert!(validate_url("ws://127.0.0.1:8100").is_ok());
        assert!(validate_url("ws://192.168.15.19:8100").is_err());
        assert!(validate_url("https://screen.example.com").is_err());
        assert!(validate_url("wss://user:password@example.com").is_err());
    }
}
