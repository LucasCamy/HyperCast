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
use std::{sync::Arc, time::Duration};

struct DiagnosticLog;
impl log::Log for DiagnosticLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let message = record.args().to_string();
            let first = message.lines().next().unwrap_or("");
            let lower = first.to_lowercase();
            if !["token", "password", "authorization", "pwd="]
                .iter()
                .any(|word| lower.contains(word))
            {
                eprintln!(
                    "SDK diagnostic: {}",
                    first.chars().take(500).collect::<String>()
                );
            }
        }
    }
    fn flush(&self) {}
}
static DIAGNOSTIC_LOG: DiagnosticLog = DiagnosticLog;

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

pub(crate) async fn connect(
    address: &str,
    token: &str,
) -> Result<(Room, tokio::sync::mpsc::UnboundedReceiver<RoomEvent>)> {
    connect_with_policy(address, token, std::env::var("HYPERCAST_RELAY_ONLY").as_deref() == Ok("1")).await
}

pub(crate) async fn connect_with_policy(
    address: &str, token: &str, relay_only: bool,
) -> Result<(Room, tokio::sync::mpsc::UnboundedReceiver<RoomEvent>)> {
    if std::env::var("HYPERCAST_TRACE_CODEC").as_deref() == Ok("1")
        && log::set_logger(&DIAGNOSTIC_LOG).is_ok()
    {
        log::set_max_level(log::LevelFilter::Warn);
    }
    validate_url(address)?;
    let mut options = RoomOptions::default();
    options.auto_subscribe = false;
    options.adaptive_stream = true;
    options.dynacast = false;
    options.join_retries = 0;
    options.reconnect_attempts = 3;
    options.reconnect_min_delay = Duration::from_secs(2);
    // The SDK's default single-PC mode produced conflicting ICE ufrags against
    // the pinned SFU during native publication. Dual transports avoid that path.
    options.single_peer_connection = false;
    options.connect_timeout = Duration::from_secs(10);
    if relay_only {
        options.rtc_config.ice_transport_type = livekit::webrtc::peer_connection_factory::IceTransportsType::Relay;
    }
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

/// Exercise synchronous SFU admission without capturing any screen pixels.
pub async fn admission_probe(address: &str, token: &str, reject_first: bool) -> Result<()> {
    let (room, mut events) = connect(address, token).await?;
    let room = Arc::new(room);
    let event_task = tokio::spawn(async move { while events.recv().await.is_some() {} });
    let first = EncodedPublisher::in_room(room.clone(), 2560, 1440, 120, 40_000_000, true).await;
    if reject_first {
        let error = first
            .err()
            .context("SFU incorrectly admitted a forbidden publisher")?;
        ensure!(
            error.to_string().contains("LimitExceeded"),
            "Expected quota rejection, received: {error}"
        );
        println!("PASS: SFU rejected forbidden publisher before publication");
    } else {
        let first = first?;
        let duplicate =
            EncodedPublisher::in_room(room.clone(), 2560, 1440, 120, 40_000_000, true).await;
        let error = duplicate
            .err()
            .context("SFU incorrectly admitted a second screen")?;
        ensure!(
            error.to_string().contains("LimitExceeded"),
            "Expected quota rejection, received: {error}"
        );
        println!("PASS: SFU rejected duplicate screen before publication");
        first.stop().await?;
    }
    event_task.abort();
    room.close().await?;
    Ok(())
}

pub struct EncodedPublisher {
    room: Arc<Room>,
    publication: LocalTrackPublication,
    track_id:String,
    source: NativeVideoSource,
    codec: EncodedVideoCodec,
    resolution: VideoResolution,
    last_timestamp: Option<i64>,
    seen_keyframe: bool,
    event_task: Option<tokio::task::JoinHandle<()>>,
}

/// Keep a pending publication without capturing or transmitting screen pixels.
pub async fn admission_hold(address: &str, token: &str, seconds: u32, release_after: Option<u32>) -> Result<()> {
    let (room, mut events) = connect(address, token).await?;
    let room = Arc::new(room);
    let event_task = tokio::spawn(async move { while events.recv().await.is_some() {} });
    let publisher = EncodedPublisher::in_room(room.clone(), 1920, 1080, 120, 40_000_000, true).await?;
    println!("Admission hold: publication accepted without pixel capture");
    let before_release = release_after.unwrap_or(seconds).min(seconds);
    tokio::time::sleep(Duration::from_secs(before_release as u64)).await;
    publisher.stop().await?;
    println!("Admission hold: unpublished, remaining connected");
    tokio::time::sleep(Duration::from_secs((seconds-before_release) as u64)).await;
    room.close().await?;
    event_task.abort();
    Ok(())
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
        let mut publisher = Self::in_room(Arc::new(room), width, height, fps, bitrate, av1).await?;
        publisher.event_task = Some(tokio::spawn(async move {
            while events.recv().await.is_some() {}
        }));
        Ok(publisher)
    }

    pub async fn in_room(
        room: Arc<Room>,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        av1: bool,
    ) -> Result<Self> {
        let resolution = VideoResolution { width, height };
        let source = NativeVideoSource::new_encoded(resolution.clone());
        let track =
            LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(source.clone()));
        let track_id=track.rtc_track().id();
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
        Ok(Self {
            room,
            publication,
            track_id,
            source,
            resolution,
            codec: if av1 {
                EncodedVideoCodec::AV1
            } else {
                EncodedVideoCodec::H265
            },
            last_timestamp: None,
            seen_keyframe: false,
            event_task: None,
        })
    }

    pub fn take_keyframe_request(&self) -> bool {
        self.source.take_keyframe_request() || !self.seen_keyframe
    }
    pub fn restart_keyframe(&mut self){self.seen_keyframe=false;}
    pub fn set_resolution_if_changed(&mut self,width:u32,height:u32){
        if self.resolution.width!=width || self.resolution.height!=height {self.set_resolution(width,height);}
    }
    pub fn set_resolution(&mut self,width:u32,height:u32){
        self.resolution=VideoResolution{width,height};self.restart_keyframe();
    }
    pub fn take_rate_control_request(
        &self,
    ) -> Option<livekit::webrtc::video_source::EncodedRateControl> {
        self.source.take_rate_control_request()
    }
    /// Complete access unit (HEVC Annex B / AV1 OBU), not individual RTP packets.
    pub fn push(&mut self, bytes: &[u8], timestamp_us: i64, keyframe: bool) -> Result<bool> {
        match self.room.connection_state(){
            ConnectionState::Reconnecting=>{self.restart_keyframe();return Ok(false);}
            ConnectionState::Disconnected=>anyhow::bail!("Room disconnected"),
            ConnectionState::Connected=>{}
        }
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
        print_ice_path("publisher", &stats.publisher_stats);
        for entry in stats.publisher_stats {
            if let livekit::webrtc::stats::RtcStats::OutboundRtp(rtp) = entry {
                println!(
                    "WebRTC outbound: {} packets, {} bytes, {} frames sent",
                    rtp.sent.packets_sent, rtp.sent.bytes_sent, rtp.outbound.frames_sent
                );
            }
        }
        if let Some(task) = self.event_task {
            task.abort();
        }
        self.room.close().await?;
        Ok(())
    }
    pub async fn stop(self) -> Result<()> {
        if let Some(task) = self.event_task {
            task.abort();
        }
        if self.room.connection_state() == ConnectionState::Disconnected {
            return Ok(()); // SDK teardown releases the closed publication graph.
        }
        // A full SDK reconnect can replace the publication SID while retaining the RTC track.
        let sid=self.room.local_participant().track_publications().into_values()
            .find(|p|p.track().is_some_and(|t|t.rtc_track().id()==self.track_id))
            .map(|p|p.sid()).unwrap_or_else(||self.publication.sid());
        self.room.local_participant().unpublish_track(&sid).await?;
        Ok(())
    }
}

/// Report the selected media route without logging endpoint addresses or ICE credentials.
pub fn print_ice_path(label: &str, stats: &[livekit::webrtc::stats::RtcStats]) {
    use livekit::webrtc::stats::RtcStats;
    for entry in stats {
        let RtcStats::Transport(transport) = entry else {
            continue;
        };
        let pair = stats.iter().find_map(|entry| match entry {
            RtcStats::CandidatePair(pair)
                if pair.rtc.id == transport.transport.selected_candidate_pair_id =>
            {
                Some(pair)
            }
            _ => None,
        });
        let Some(pair) = pair else { continue };
        let remote = stats.iter().find_map(|entry| match entry {
            RtcStats::RemoteCandidate(remote)
                if remote.rtc.id == pair.candidate_pair.remote_candidate_id =>
            {
                Some(remote)
            }
            _ => None,
        });
        let Some(remote) = remote else { continue };
        let local = stats.iter().find_map(|entry| match entry {
            RtcStats::LocalCandidate(candidate) if candidate.rtc.id == pair.candidate_pair.local_candidate_id => Some(candidate),
            _ => None,
        });
        let relay = local.is_some_and(|c| c.local_candidate.candidate_type == Some(livekit::webrtc::stats::IceCandidateType::Relay));
        let relay_protocol = local.and_then(|c| c.local_candidate.relay_protocol);
        let protocol = match remote.remote_candidate.protocol.as_str() {
            "udp" => "udp",
            "tcp" => "tcp",
            _ => "other",
        };
        let scope = address_scope(&remote.remote_candidate.address);
        let rtt = pair.candidate_pair.current_round_trip_time * 1000.0;
        if rtt.is_finite() && rtt >= 0.0 {
            println!("WebRTC ICE {label}: {protocol}, remote {scope}, network RTT {rtt:.2} ms");
            println!("WebRTC route {label}: local relay={relay}, relay transport={relay_protocol:?}");
            #[cfg(all(windows, feature = "desktop"))]
            crate::diagnostics::write("route", &format!("{label}: local relay={relay}, relay transport={relay_protocol:?}"), "");
            #[cfg(all(windows, feature = "desktop"))]
            crate::diagnostics::write("ice", &format!("{label}: {protocol}, remote {scope}, network RTT {rtt:.2} ms"), "");
        }
    }
}

fn address_scope(address: &str) -> &'static str {
    use std::net::IpAddr;
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) if ip.is_loopback() => "loopback",
        Ok(IpAddr::V4(ip))
            if ip.is_private()
                || ip.is_link_local()
                || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1])) =>
        {
            "private"
        }
        Ok(IpAddr::V6(ip)) if ip.is_loopback() => "loopback",
        Ok(IpAddr::V6(ip)) if ip.is_unique_local() || ip.is_unicast_link_local() => "private",
        Ok(ip) if !ip.is_unspecified() && !ip.is_multicast() => "non-private",
        _ => "unknown",
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
    #[test]
    fn route_scope_identifies_vpn_and_carrier_nat_without_exposing_address() {
        assert_eq!(address_scope("100.121.252.74"), "private");
        assert_eq!(address_scope("192.168.15.19"), "private");
        assert_eq!(address_scope("fd7a:115c:a1e0::1"), "private");
        assert_eq!(address_scope("127.0.0.1"), "loopback");
        assert_eq!(address_scope("8.8.8.8"), "non-private");
        assert_eq!(address_scope("unavailable"), "unknown");
    }
}
