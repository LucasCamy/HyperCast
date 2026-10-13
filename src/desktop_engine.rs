use crate::{
    encoder::{Codec, HardwareEncoder},
    network::{self, EncodedPublisher},
    pacing::FramePacer,
    screen_capture::{CaptureConfig, ScreenCapture},
    subscription::ViewMode,
};
use anyhow::Result;
use eframe::egui;
use livekit::{
    prelude::*,
    track::VideoQuality,
    webrtc::{video_frame::BoxVideoFrame, video_stream::native::NativeVideoStream},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use tokio_stream::StreamExt;
use windows::{Win32::Graphics::Direct3D11::ID3D11Texture2D, core::Interface};

pub enum VideoFrame {
    Remote(BoxVideoFrame),
    Local(ID3D11Texture2D, u32, u32),
}
impl VideoFrame {
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Remote(frame) => (frame.buffer.width(), frame.buffer.height()),
            Self::Local(_, w, h) => (*w, *h),
        }
    }
    pub fn surface(&self) -> Option<(*mut std::ffi::c_void, u32)> {
        match self {
            Self::Remote(frame) => frame
                .buffer
                .as_native()
                .and_then(|native| native.d3d11_texture()),
            Self::Local(texture, _, _) => Some((texture.as_raw(), 0)),
        }
    }
    pub fn local(&self) -> bool {
        matches!(self, Self::Local(..))
    }
}

#[derive(Default)]
pub struct Shared {
    pub connected: bool,
    pub publishing: bool,
    pub can_publish: bool,
    pub identity: String,
    pub room: String,
    pub status: String,
    pub participants: Vec<(String, String)>,
    pub screens: Vec<String>,
    pub frames: HashMap<String, Arc<VideoFrame>>,
    pub received: u64,
    pub sent: u64,
}
#[derive(Clone)]
pub struct Layout {
    pub mode: ViewMode,
    pub focus: Option<String>,
    pub paused: bool,
}
pub enum Command {
    Layout(Layout),
    Share {
        adapter: u32,
        output: u32,
        codec: Codec,
    },
    StopShare,
    Disconnect,
}
pub struct Engine {
    pub commands: mpsc::Sender<Command>,
    pub shared: Arc<Mutex<Shared>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
enum Access {
    Token(String),
    Invite { code: String, name: String, request_id: String },
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Disconnect);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Cast {
    selection: (u32, u32, Codec),
    capture: ScreenCapture,
    encoder: Option<HardwareEncoder>,
    publisher: Option<EncodedPublisher>,
    start: Instant,
    count: u64,
    paused: bool,
    fps: u32,
}
struct Sink {
    owner: String,
    task: tokio::task::JoinHandle<()>,
}
fn recoverable_capture_error(message: &str) -> bool {
    message.contains("rebuild GPU pipeline") || message.contains("duplication recovery failed")
}
impl Sink {
    fn abort(&self) {
        self.task.abort();
    }
}
impl Cast {
    fn new(
        adapter: u32,
        output: u32,
        codec: Codec,
        runtime: &tokio::runtime::Runtime,
        room: &Arc<Room>,
    ) -> Result<Self> {
        let capture = ScreenCapture::new(CaptureConfig {
            adapter_index: adapter,
            output_index: output,
        })?;
        let (width, height) = capture.dimensions();
        crate::diagnostics::write("capture", &format!("{width}x{height} target_fps=120 codec={codec:?}"), "");
        let encoder = HardwareEncoder::new(
            capture.device(),
            width,
            height,
            120,
            40_000_000,
            codec,
        )?;
        // No Desktop Duplication frame is held while waiting for publication acknowledgment.
        let publisher = runtime.block_on(EncodedPublisher::in_room(
            room.clone(),
            width,
            height,
            120,
            40_000_000,
            codec == Codec::Av1,
        ))?;
        Ok(Self {
            selection: (adapter, output, codec),
            capture,
            encoder: Some(encoder),
            publisher: Some(publisher),
            start: Instant::now(),
            count: 0,
            paused: false,
            fps: 120,
        })
    }
    fn tick(&mut self, _runtime: &tokio::runtime::Runtime, _room: &Arc<Room>) -> Result<()> {
        let frame = self.capture.acquire()?;
        if self.count == 0 && frame.is_none() {
            return Ok(());
        }
        if let (Some(encoder), Some(publisher)) = (&mut self.encoder, &mut self.publisher) {
            let mut key = self.count == 0 || publisher.take_keyframe_request();
            if let Some(rate) = publisher.take_rate_control_request() {
                self.paused = rate.target_bitrate_bps == 0;
                if !self.paused {
                    self.fps = (rate.framerate_fps as u32).clamp(1, 120);
                    encoder.reconfigure(
                        rate.target_bitrate_bps.clamp(64_000, 40_000_000) as u32,
                        self.fps,
                    )?;
                    key = true;
                }
            }
            if !self.paused {
                let packet =
                    encoder.encode(frame.as_ref(), self.start.elapsed().as_micros() as i64, key)?;
                if publisher.push(packet.bytes, packet.timestamp_us, packet.keyframe)? {
                    self.count += 1;
                }
            }
        }
        if let Some(frame) = frame {
            frame.release()?;
        }
        Ok(())
    }
    fn stop(self, runtime: &tokio::runtime::Runtime) {
        if let Some(publisher) = self.publisher {
            let _ = runtime.block_on(publisher.stop());
        }
    }
}

fn update_subscriptions(room: &Room, layout: &Layout) {
    let mut screens = Vec::new();
    for participant in room.remote_participants().into_values() {
        for publication in participant.track_publications().into_values() {
            if publication.source() == TrackSource::Screenshare
                && publication.kind() == TrackKind::Video
            {
                screens.push((participant.identity().to_string(), publication));
            }
        }
    }
    screens.sort_by_key(|(id, _)| id.clone());
    let focus = layout
        .focus
        .as_deref()
        .or_else(|| screens.first().map(|(id, _)| id.as_str()));
    let mut count = 0;
    for (identity, publication) in &screens {
        let enabled = !layout.paused
            && count < 3
            && (layout.mode == ViewMode::Grid || Some(identity.as_str()) == focus);
        if enabled {
            count += 1;
            publication.set_subscribed(true);
            if publication.is_subscribed() {
                publication.set_enabled(true);
            }
        } else {
            if publication.is_subscribed() {
                publication.set_enabled(false);
            }
            publication.set_subscribed(false);
        }
        if publication.simulcasted() && enabled {
            publication.set_video_quality(VideoQuality::High);
        }
    }
}

impl Engine {
    pub fn connect(address: String, token: String, ctx: egui::Context, parent: usize, relay_only: bool) -> Self {
        Self::connect_access(address, Access::Token(token), ctx, parent, relay_only)
    }
    pub fn connect_invite(address: String, code: String, name: String, request_id: String, ctx: egui::Context, parent: usize, relay_only: bool) -> Self {
        Self::connect_access(address, Access::Invite { code, name, request_id }, ctx, parent, relay_only)
    }
    fn connect_access(address: String, access: Access, ctx: egui::Context, parent: usize, relay_only: bool) -> Self {
        crate::diagnostics::write("connection", "Starting room connection", "");
        let (tx, commands) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared {
            status: "Conectando…".into(),
            ..Default::default()
        }));
        let state = shared.clone();
        let thread = std::thread::spawn(move || {
            let token = match access {
                Access::Token(token) => token,
                Access::Invite { code, name, request_id } => {
                    match crate::invitation::join(&address, &code, &name, &request_id) {
                        Ok(token) => token,
                        Err(error) => {
                            let message = error.to_string().replace(&code, "[invite]");
                            crate::diagnostics::write("invitation_error", &message, &code);
                            state.lock().unwrap().status = message;
                            ctx.request_repaint();
                            return;
                        }
                    }
                }
            };
            let result = (|| -> Result<()> {
                let runtime = tokio::runtime::Runtime::new()?;
                let _runtime_guard = runtime.enter();
                livekit::webrtc::enable_zero_playout_delay()?;
                let (room, mut events) = runtime.block_on(network::connect_with_policy(&address, &token, relay_only))?;
                let room = Arc::new(room);
                crate::diagnostics::write("connection", "Room connected", "");
                {
                    let mut state = state.lock().unwrap();
                    state.connected = true;
                    state.room = room.name();
                    state.status = "Conectado".into();
                    state.identity = room.local_participant().identity().to_string();
                    state.can_publish = room
                        .local_participant()
                        .permission()
                        .is_some_and(|grant| grant.can_publish);
                }
                let mut layout = Layout {
                    mode: ViewMode::Spotlight,
                    focus: None,
                    paused: false,
                };
                if std::env::var("HYPERCAST_VIEW_MODE").as_deref() == Ok("grid") {
                    layout.mode = ViewMode::Grid;
                }
                layout.focus = std::env::var("HYPERCAST_FOCUS").ok();
                let mut manual_layout = layout.clone();
                let mut window_hidden = false;
                let mut cast: Option<Cast> = None;
                let mut recovery: Option<((u32, u32, Codec), Instant)> = None;
                let mut recovery_attempts = 0u8;
                #[cfg(debug_assertions)]
                let mut injected_capture_loss = false;
                let mut sinks = HashMap::<TrackSid, Sink>::new();
                let mut pacer = FramePacer::new(120)?;
                update_subscriptions(&room, &layout);
                let mut info_tick = Instant::now();
                let mut diagnostic_tick = Instant::now();
                'session: loop {
                    // OS state is polled independently of painting: a minimized
                    // eframe window may stop invoking App::ui entirely.
                    let hidden = unsafe {
                        windows::Win32::UI::WindowsAndMessaging::IsIconic(
                            windows::Win32::Foundation::HWND(parent as *mut _),
                        )
                    }
                    .as_bool();
                    if hidden != window_hidden {
                        window_hidden = hidden;
                        layout = manual_layout.clone();
                        layout.paused |= hidden;
                        update_subscriptions(&room, &layout);
                        if std::env::var("HYPERCAST_SMOKE_SECONDS").is_ok() {
                            println!(
                                "GUI viewport: {} at {} frames",
                                if hidden { "minimized" } else { "restored" },
                                state.lock().unwrap().received
                            );
                        }
                    }
                    while let Ok(command) = commands.try_recv() {
                        match command {
                            Command::Disconnect => {
                                crate::diagnostics::write("connection", "User disconnected", "");
                                break 'session;
                            }
                            Command::Layout(next) => {
                                crate::diagnostics::write("layout", &format!("mode={:?} paused={}", next.mode, next.paused), "");
                                manual_layout = next;
                                layout = manual_layout.clone();
                                layout.paused |= window_hidden;
                                update_subscriptions(&room, &layout);
                            }
                            Command::Share {
                                adapter,
                                output,
                                codec,
                            } if cast.is_none() => {
                                recovery = None;
                                recovery_attempts = 0;
                                crate::diagnostics::write("publication", &format!("adapter={adapter} output={output} codec={codec:?}"), "");
                                match Cast::new(adapter, output, codec, &runtime, &room) {
                                    Ok(value) => {
                                        cast = Some(value);
                                        let mut state = state.lock().unwrap();
                                        state.publishing = true;
                                        state.status = "Preparando compartilhamento…".into();
                                    }
                                    Err(error) => {
                                        crate::diagnostics::write("publication_error", &format!("{error:#}"), &token);
                                        state.lock().unwrap().status = if error.to_string().contains("LimitExceeded") {
                                            "A sala atingiu o limite de telas. Você pode assistir e compartilhar quando alguém parar.".into()
                                        } else {
                                            error.to_string().replace(&token, "[token]")
                                        };
                                    }
                                }
                            }
                            Command::StopShare => {
                                recovery = None;
                                crate::diagnostics::write("publication", "Stopped", "");
                                if let Some(value) = cast.take() {
                                    value.stop(&runtime);
                                }
                                let mut state = state.lock().unwrap();
                                state.publishing = false;
                                state.status = "Conectado".into();
                                state
                                    .frames
                                    .remove(&room.local_participant().identity().to_string());
                            }
                            _ => {}
                        }
                    }
                    for _ in 0..64 {
                        let Ok(event) = events.try_recv() else {
                            break;
                        };
                        match event {
                            RoomEvent::TrackSubscribed {
                                track: RemoteTrack::Video(track),
                                publication,
                                participant,
                            } => {
                                if let Some(previous) = sinks.remove(&publication.sid()) {
                                    previous.task.abort();
                                }
                                if layout.paused || sinks.len() >= 3 {
                                    publication.set_subscribed(false);
                                    continue;
                                }
                                let identity = participant.identity().to_string();
                                let owner = identity.clone();
                                let sid = publication.sid();
                                let state = state.clone();
                                let ctx = ctx.clone();
                                let mut stream = NativeVideoStream::new(track.rtc_track());
                                let task = runtime.spawn(async move {
                                    while let Some(frame) = stream.next().await {
                                        let mut state = state.lock().unwrap();
                                        if state.frames.len() < 3
                                            || state.frames.contains_key(&identity)
                                        {
                                            state.frames.insert(
                                                identity.clone(),
                                                Arc::new(VideoFrame::Remote(frame)),
                                            );
                                            state.received += 1;
                                        }
                                        drop(state);
                                        ctx.request_repaint();
                                    }
                                });
                                sinks.insert(sid, Sink { owner, task });
                            }
                            RoomEvent::TrackUnsubscribed { publication, .. } => {
                                if let Some(task) = sinks.remove(&publication.sid()) {
                                    task.abort();
                                }
                                // Retain the last GPU surface as a static paused
                                // thumbnail. It consumes no RTP or decoder work.
                                update_subscriptions(&room, &layout);
                            }
                            RoomEvent::TrackUnpublished {
                                publication,
                                participant,
                            } => {
                                if let Some(task) = sinks.remove(&publication.sid()) {
                                    task.abort();
                                }
                                state
                                    .lock()
                                    .unwrap()
                                    .frames
                                    .remove(&participant.identity().to_string());
                                update_subscriptions(&room, &layout);
                            }
                            RoomEvent::ParticipantDisconnected(participant) => {
                                let identity = participant.identity().to_string();
                                sinks.retain(|_, sink| {
                                    if sink.owner == identity {
                                        sink.abort();
                                        false
                                    } else {
                                        true
                                    }
                                });
                                state
                                    .lock()
                                    .unwrap()
                                    .frames
                                    .remove(&participant.identity().to_string());
                                update_subscriptions(&room, &layout);
                            }
                            RoomEvent::TrackPublished { .. }
                            | RoomEvent::ParticipantConnected(_) => {
                                update_subscriptions(&room, &layout)
                            }
                            RoomEvent::Disconnected { .. } => {
                                crate::diagnostics::write("connection", "SDK disconnected", "");
                                break 'session;
                            }
                            _ => {}
                        }
                    }
                    if recovery.as_ref().is_some_and(|(_, at)| Instant::now() >= *at) {
                        let (selection, _) = recovery.take().unwrap();
                        recovery_attempts += 1;
                        crate::diagnostics::write("recovery", &format!("Rebuilding GPU pipeline, attempt {recovery_attempts}/3"), "");
                        match Cast::new(selection.0, selection.1, selection.2, &runtime, &room) {
                            Ok(value) => {
                                cast = Some(value);
                                state.lock().unwrap().publishing = true;
                            }
                            Err(error) => {
                                crate::diagnostics::write("recovery_error", &format!("{error:#}"), &token);
                                if recovery_attempts < 3 {
                                    recovery = Some((selection, Instant::now() + Duration::from_secs(1)));
                                } else {
                                    state.lock().unwrap().status = format!("Recuperação interrompida: {error}").replace(&token, "[token]");
                                }
                            }
                        }
                    }
                    if let Some(value) = &mut cast {
                        let tick_result = value.tick(&runtime, &room);
                        #[cfg(debug_assertions)]
                        let tick_result = if !injected_capture_loss && value.count > 120
                            && std::env::var("HYPERCAST_SMOKE_SECONDS").is_ok()
                            && std::env::var("HYPERCAST_SMOKE_CAPTURE_LOSS").as_deref() == Ok("1")
                        {
                            injected_capture_loss = true;
                            Err(anyhow::anyhow!("Injected capture loss; rebuild GPU pipeline"))
                        } else { tick_result };
                        if let Err(error) = tick_result {
                            let message = format!("{error:#}");
                            crate::diagnostics::write("capture_error", &message, &token);
                            let recoverable = recoverable_capture_error(&message);
                            if recoverable && recovery_attempts < 3 {
                                recovery = Some((value.selection, Instant::now() + Duration::from_secs(1)));
                            }
                            state.lock().unwrap().status = if recovery.is_some() {
                                "Tela mudou; recuperando compartilhamento…".into()
                            } else {
                                format!("Compartilhamento: {error}").replace(&token, "[token]")
                            };
                            if let Some(value) = cast.take() {
                                value.stop(&runtime);
                            }
                            let mut state = state.lock().unwrap();
                            state.publishing = false;
                            state
                                .frames
                                .remove(&room.local_participant().identity().to_string());
                        } else {
                            let mut state = state.lock().unwrap();
                            state.sent = value.count;
                            if value.publisher.is_some() {
                                state.status = if value.paused {
                                    "Compartilhamento ativo • envio pausado"
                                } else {
                                    "Compartilhando sua tela"
                                }
                                .into();
                            }
                            if let Some(texture) = value
                                .encoder
                                .as_ref()
                                .filter(|_| !value.paused && value.count % 8 == 0)
                                .and_then(|encoder| encoder.preview_texture())
                            {
                                let mut desc = Default::default();
                                unsafe {
                                    texture.GetDesc(&mut desc);
                                }
                                state.frames.insert(
                                    room.local_participant().identity().to_string(),
                                    Arc::new(VideoFrame::Local(texture, desc.Width, desc.Height)),
                                );
                            }
                        }
                    }
                    if info_tick.elapsed() > Duration::from_millis(500) {
                        state.lock().unwrap().screens = room
                            .remote_participants()
                            .into_values()
                            .filter(|p| {
                                p.track_publications().into_values().any(|track| {
                                    track.source() == TrackSource::Screenshare
                                        && track.kind() == TrackKind::Video
                                })
                            })
                            .map(|p| p.identity().to_string())
                            .collect();
                        state.lock().unwrap().participants = room
                            .remote_participants()
                            .into_values()
                            .map(|p| (p.identity().to_string(), p.name()))
                            .collect();
                        info_tick = Instant::now();
                        ctx.request_repaint();
                    }
                    if diagnostic_tick.elapsed() >= Duration::from_secs(5) {
                        let shared = state.lock().unwrap();
                        crate::diagnostics::write("media", &format!("sent={} received={} receivers={} publishing={}", shared.sent, shared.received, sinks.len(), shared.publishing), "");
                        drop(shared);
                        let diagnostic_room = room.clone();
                        runtime.spawn(async move {
                            if let Ok(stats) = diagnostic_room.get_stats().await {
                                network::print_ice_path("publisher", &stats.publisher_stats);
                                network::print_ice_path("subscriber", &stats.subscriber_stats);
                            }
                        });
                        diagnostic_tick = Instant::now();
                    }
                    let fps = cast
                        .as_ref()
                        .filter(|cast| !cast.paused)
                        .map(|cast| cast.fps)
                        .unwrap_or(if sinks.is_empty() { 10 } else { 30 });
                    pacer.set_fps(fps)?;
                    pacer.wait()?;
                }
                if let Some(value) = cast {
                    value.stop(&runtime);
                }
                for (_, task) in sinks {
                    task.abort();
                    let _ = runtime.block_on(task.task);
                }
                runtime.block_on(room.close())?;
                Ok(())
            })();
            let mut state = state.lock().unwrap();
            crate::diagnostics::write("session_summary", &format!("sent={} received={}", state.sent, state.received), "");
            state.connected = false;
            state.publishing = false;
            state.frames.clear();
            state.status = result
                .err()
                .map(|e| e.to_string().replace(&token, "[token]"))
                .unwrap_or_else(|| "Desconectado".into());
            crate::diagnostics::write("session_end", &state.status, &token);
            drop(state);
            ctx.request_repaint();
        });
        Self {
            commands: tx,
            shared,
            thread: Some(thread),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebuilds_changed_surfaces_but_not_unsupported_formats_or_codecs() {
        assert!(recoverable_capture_error("Capture dimensions changed: expected 1920x1080, received 2560x1440; rebuild GPU pipeline"));
        assert!(recoverable_capture_error("AcquireNextFrame failed; rebuild GPU pipeline on device loss"));
        assert!(!recoverable_capture_error("Unsupported capture texture layout: format=10"));
        assert!(!recoverable_capture_error("NVENC codec AV1 is unsupported"));
    }
}
