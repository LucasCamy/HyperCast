use crate::{
    encoder::{Codec, HardwareEncoder},
    network::{self, EncodedPublisher},
    pacing::FramePacer,
    video_capture::{CaptureTarget,VideoCapture},
    subscription::ViewMode,
    system_audio::{AudioReceiver, SystemAudioPublisher},
};
use anyhow::Result;
use eframe::egui;
use livekit::{
    prelude::*,
    track::VideoQuality,
    webrtc::{video_frame::BoxVideoFrame, video_stream::native::{NativeVideoStream,NativeVideoStreamOptions}},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc, atomic::{AtomicU64, Ordering}},
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
    pub reception_resetting: bool,
    pub reception_restarts: u64,
    pub publishing: bool,
    pub can_publish: bool,
    pub identity: String,
    pub room: String,
    pub status: String,
    pub participants: Vec<(String, String)>,
    pub screens: Vec<String>,
    pub frames: HashMap<String, Arc<VideoFrame>>,
    /// One retained surface per interrupted screen, bounded to the three slots.
    pub frozen: HashMap<String, Instant>,
    pub received: u64,
    pub received_streams: HashMap<String,u64>,
    pub sent: u64,
    pub audio_status: String,
    pub active_target:Option<CaptureTarget>,
    pub source_error:String,
}
#[derive(Clone)]
pub struct Layout {
    pub self_preview: bool,
    pub mode: ViewMode,
    pub focus: Option<String>,
    pub paused: bool,
    pub volume: u32,
}
pub enum Command {
    RestartReception,
    Layout(Layout),
    Share {
        adapter: u32,
        output: u32,
        codec: Codec,
    },
    ShareWindow { adapter:u32, hwnd:usize, pid:u32, codec:Codec },
    ChangeSource(CaptureTarget),
    StopShare,
    SystemAudio(bool),
    Disconnect,
}
pub struct Engine {
    pub commands: mpsc::Sender<Command>,
    pub shared: Arc<Mutex<Shared>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
enum Access {
    Token(String),
    Invite { code: String, name: String, request_id: String, session:String, by_room:bool },
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
    selection: (CaptureTarget, Codec),
    capture: VideoCapture,
    dimensions:(u32,u32),
    has_frame:bool,
    device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    encoder: Option<HardwareEncoder>,
    publisher: Option<EncodedPublisher>,
    start: Instant,
    count: u64,
    paused: bool,
    fps: u32,
    audio: Option<SystemAudioPublisher>,
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
        target: CaptureTarget,
        codec: Codec,
        runtime: &tokio::runtime::Runtime,
        room: &Arc<Room>,
    ) -> Result<Self> {
        let capture = VideoCapture::new(target)?;
        let device=capture.device().clone();
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
        crate::diagnostics::write("encoder",encoder.backend_name(),"");
        // No Desktop Duplication frame is held while waiting for publication acknowledgment.
        let (send_width,send_height)=encoder.dimensions();
        crate::diagnostics::write("resolution",&format!("source={width}x{height} output={send_width}x{send_height}"),"");
        let publisher = runtime.block_on(EncodedPublisher::in_room(
            room.clone(),
            send_width,
            send_height,
            120,
            40_000_000,
            codec == Codec::Av1,
        ))?;
        Ok(Self {
            audio: None,
            selection: (target, codec),
            dimensions:(width,height),device,has_frame:false,
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
        if _room.connection_state()!=ConnectionState::Connected {
            if let Some(p)=&mut self.publisher {p.restart_keyframe();}
            return Ok(());
        }
        let frame = self.capture.acquire()?;
        if let Some(frame)=&frame {
            let mut desc=Default::default();unsafe{frame.texture().GetDesc(&mut desc);}
            if (desc.Width,desc.Height)!=self.dimensions {
                self.encoder=Some(HardwareEncoder::new(&self.device,desc.Width,desc.Height,120,40_000_000,self.selection.1)?);
                self.dimensions=(desc.Width,desc.Height);
                if let Some(p)=&mut self.publisher {let (w,h)=self.encoder.as_ref().unwrap().dimensions();p.set_resolution(w,h);}
            }
        }
        if !self.has_frame && frame.is_none() {
            return Ok(());
        }
        self.has_frame |= frame.is_some();
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
                    encoder.encode_texture(frame.as_ref().map(|f|f.texture()), self.start.elapsed().as_micros() as i64, key)?;
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
    fn set_audio(&mut self, enabled: bool, runtime: &tokio::runtime::Runtime, room: &Arc<Room>) -> Result<()> {
        if !enabled { if let Some(audio) = self.audio.take() { audio.stop(runtime); } }
        else if self.audio.is_none() { self.audio = Some(SystemAudioPublisher::start_for_process(runtime, room.clone(),self.selection.0.process())?); }
        Ok(())
    }
    fn change_source(&mut self,target:CaptureTarget,runtime:&tokio::runtime::Runtime,room:&Arc<Room>,audio_requested:bool)->Result<()> {
        if target==self.selection.0 {return Ok(());}
        // Prepare and validate first; failures leave the original source intact.
        let capture=VideoCapture::new(target)?;
        let dimensions=capture.dimensions();let device=capture.device().clone();
        let encoder=HardwareEncoder::new(&device,dimensions.0,dimensions.1,120,40_000_000,self.selection.1)?;
        let send_size=encoder.dimensions();
        let change_audio=self.selection.0.process()!=target.process();
        if change_audio {if let Some(audio)=&self.audio{audio.set_process(target.process())?;}}
        self.capture=capture;self.device=device;self.encoder=Some(encoder);self.dimensions=dimensions;self.selection.0=target;self.has_frame=false;
        if let Some(p)=&mut self.publisher {p.set_resolution(send_size.0,send_size.1);}
        crate::diagnostics::write("source_switch",&format!("{}x{} application={}",dimensions.0,dimensions.1,target.process().is_some()),"");
        if change_audio && self.audio.is_none() {self.set_audio(audio_requested,runtime,room)?;}
        Ok(())
    }
    fn audio_status(&self)->String{if self.audio.is_none(){"Áudio desativado".into()}else if self.selection.0.process().is_some(){"Enviando áudio do aplicativo".into()}else{"Enviando áudio do sistema".into()}}
    fn stop(mut self, runtime: &tokio::runtime::Runtime) {
        if let Some(audio) = self.audio.take() { audio.stop(runtime); }
        if let Some(publisher) = self.publisher {
            if let Err(error) = runtime.block_on(publisher.stop()) {
                crate::diagnostics::write("unpublish_error", &format!("{error:#}"), "");
            } else {
                crate::diagnostics::write("publication", "Unpublish completed", "");
            }
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
    let local_visible=layout.self_preview && room.local_participant().track_publications().into_values()
        .any(|p|p.source()==TrackSource::Screenshare && p.kind()==TrackKind::Video);
    let focus = layout
        .focus
        .as_deref()
        .filter(|id|screens.iter().any(|(owner,_)|owner==id) || (local_visible && *id==room.local_participant().identity().as_str()))
        .or_else(|| screens.first().map(|(id, _)| id.as_str()));
    let mut count = 0;
    for (identity, publication) in &screens {
        let enabled = !layout.paused
            && count < 3
            && (layout.mode == ViewMode::Grid || Some(identity.as_str()) == focus);
        if count<3 {crate::subscription::keep_receiver(publication,enabled);count+=1;}
        else if publication.is_desired(){publication.set_subscribed(false);}
        if publication.simulcasted() && enabled {
            publication.set_video_quality(VideoQuality::High);
        }
    }
    for participant in room.remote_participants().into_values() {
        let identity = participant.identity().to_string();
        let has_screen = screens.iter().any(|(owner, _)| owner == &identity);
        for publication in participant.track_publications().into_values() {
            if publication.source() != TrackSource::ScreenshareAudio { continue; }
            let enabled = has_screen && !layout.paused && layout.volume > 0
                && (layout.mode == ViewMode::Grid || Some(identity.as_str()) == focus);
            crate::subscription::keep_receiver(&publication,enabled);
        }
    }
}


fn attach_video(runtime:&tokio::runtime::Runtime, track:RemoteVideoTrack, owner_id:&str,
    state:&Arc<Mutex<Shared>>,ctx:&egui::Context)->Sink {
    let identity = owner_id.to_owned();
    let owner = identity.clone();
    let state = state.clone();
    let ctx = ctx.clone();
    let enabled_track=track.rtc_track();
    let mut stream = NativeVideoStream::with_options(enabled_track.clone(),NativeVideoStreamOptions{queue_size_frames:Some(1)});
    let task = runtime.spawn(async move {
        while let Some(frame) = stream.next().await {
            if !enabled_track.enabled(){continue;}
            let mut state = state.lock().unwrap();
            // A new live screen takes precedence over a departed
            // participant's frozen surface, keeping the GPU bound.
            if state.frames.len() >= 3 && !state.frames.contains_key(&identity) {
                if let Some(oldest) = state.frozen.iter().min_by_key(|(_, at)| *at).map(|(id, _)| id.clone()) {
                    state.frozen.remove(&oldest);
                    state.frames.remove(&oldest);
                }
            }
            if state.frames.len() < 3
                || state.frames.contains_key(&identity)
            {
                if state.frozen.remove(&identity).is_some() {
                    crate::diagnostics::write("recovery_view", "Resumed retained screen", "");
                }
                state.frames.insert(
                    identity.clone(),
                    Arc::new(VideoFrame::Remote(frame)),
                );
                state.received += 1;
                if state.received_streams.len()>=3 && !state.received_streams.contains_key(&identity) {
                    if let Some(old)=state.received_streams.keys().next().cloned(){state.received_streams.remove(&old);}
                }
                let sequence=state.received;state.received_streams.insert(identity.clone(),sequence);
            }
            drop(state);
            ctx.request_repaint();
        }
    });
    Sink { owner, task }
}

impl Engine {
    pub fn finished(&self)->bool { self.thread.as_ref().is_none_or(|thread|thread.is_finished()) }
    pub fn connect(address: String, token: String, ctx: egui::Context, parent: usize, relay_only: bool) -> Self {
        Self::connect_access(address, Access::Token(token), ctx, parent, relay_only)
    }
    pub fn connect_invite(address: String, code: String, name: String, request_id: String, session:String, ctx: egui::Context, parent: usize, relay_only: bool) -> Self {
        Self::connect_access(address, Access::Invite { code, name, request_id, session, by_room:false }, ctx, parent, relay_only)
    }
    pub fn connect_room(address:String,room:String,name:String,request_id:String,session:String,ctx:egui::Context,parent:usize,relay_only:bool)->Self {
        Self::connect_access(address,Access::Invite{code:room,name,request_id,session,by_room:true},ctx,parent,relay_only)
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
            let (token,room_title) = match access {
                Access::Token(token) => (token,None),
                Access::Invite { code, name, request_id, session, by_room } => {
                    match if by_room {crate::invitation::join_room(&address,&code,&name,&request_id,&session)}else{crate::invitation::join_details(&address, &code, &name, &request_id, &session)} {
                        Ok(joined) => (joined.token,joined.title),
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
                    state.room = room_title.or_else(||serde_json::from_str::<serde_json::Value>(&room.metadata()).ok()
                        .and_then(|value|value["hypercast_name"].as_str().filter(|name|name.chars().count()<=64).map(str::to_owned)))
                        .unwrap_or_else(||room.name());
                    state.status = "Conectado".into();
                    state.identity = room.local_participant().identity().to_string();
                    state.can_publish = room
                        .local_participant()
                        .permission()
                        .is_some_and(|grant| grant.can_publish);
                }
                let mut layout = Layout {
                    self_preview: true,
                    volume: 80,
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
                let mut recovery: Option<((CaptureTarget, Codec), Instant)> = None;
                let mut recovery_attempts = 0u8;
                let mut reconnect_attempt = 0u32;
                #[cfg(debug_assertions)]
                let mut injected_capture_loss = false;
                let mut sinks = HashMap::<TrackSid, Sink>::new();
                let mut audio_sinks = HashMap::<TrackSid, AudioReceiver>::new();
                let mut audio_requested = false;
                let audio_volume = Arc::new(AtomicU64::new(80));
                let audio_received = Arc::new(AtomicU64::new(0));
                let mut pacer = FramePacer::new(120)?;
                update_subscriptions(&room, &layout);
                let mut reception_restart:Option<Instant>=None;
                let mut reception_tick=Instant::now();
                let mut reception_trace=Instant::now();
                let trace_reception=std::env::var_os("HYPERCAST_SMOKE_SECONDS").is_some();
                let mut info_tick = Instant::now();
                let mut diagnostic_tick = Instant::now();
                'session: loop {
                    if trace_reception && reception_trace.elapsed()>=Duration::from_secs(1) {
                        let shared=state.lock().unwrap();println!("Receive progress: {}",serde_json::to_string(&shared.received_streams).unwrap_or_default());
                        reception_trace=Instant::now();
                    }
                    if reception_restart.is_some_and(|until|Instant::now()>=until) {
                        reception_restart=None;update_subscriptions(&room,&layout);
                        state.lock().unwrap().reception_resetting=false;
                        crate::diagnostics::write("reception","Receive settings restored; waiting for fresh frames","");
                    }
                    if reception_restart.is_none() && reception_tick.elapsed()>=Duration::from_millis(250) {
                        update_subscriptions(&room,&layout);
                        for participant in room.remote_participants().into_values() {
                            for p in participant.track_publications().into_values() {
                                if !p.is_desired(){continue;}
                                match p.track() {
                                    Some(RemoteTrack::Video(track)) if p.source()==TrackSource::Screenshare && sinks.get(&p.sid()).is_none_or(|s|s.task.is_finished()) && (sinks.contains_key(&p.sid()) || sinks.len()<3) => {
                                        if let Some(old)=sinks.remove(&p.sid()){old.abort();}
                                        sinks.insert(p.sid(),attach_video(&runtime,track,&participant.identity().to_string(),&state,&ctx));
                                    },
                                    Some(RemoteTrack::Audio(track)) if p.source()==TrackSource::ScreenshareAudio && p.is_enabled() && !audio_sinks.contains_key(&p.sid()) && audio_sinks.len()<3 => {
                                        audio_sinks.insert(p.sid(),AudioReceiver::start(&runtime,track,audio_volume.clone(),audio_received.clone()));
                                    },
                                    _=>{}
                                }
                            }
                        }
                        let active_audio:Vec<_>=room.remote_participants().into_values().flat_map(|p|p.track_publications().into_values())
                            .filter(|p|p.source()==TrackSource::ScreenshareAudio && p.is_desired() && p.is_enabled()).map(|p|p.sid()).collect();
                        audio_sinks.retain(|sid,_|active_audio.contains(sid));
                        reception_tick=Instant::now();
                    }
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
                        if reception_restart.is_none(){update_subscriptions(&room, &layout);}
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
                            Command::RestartReception if reception_restart.is_none() && room.connection_state()==ConnectionState::Connected => {
                                for (_,sink) in sinks.drain(){sink.abort();let _=runtime.block_on(sink.task);}
                                audio_sinks.clear();
                                crate::subscription::reset_receivers(&room);
                                reception_restart=Some(Instant::now()+Duration::from_millis(700));
                                let mut shared=state.lock().unwrap();shared.reception_resetting=true;shared.reception_restarts+=1;
                                crate::diagnostics::write("reception","Manual receive restart; local publication preserved","");
                            }
                            Command::Layout(next) => {
                                audio_volume.store(next.volume.min(100) as u64, Ordering::Relaxed);
                                crate::diagnostics::write("layout", &format!("mode={:?} paused={} volume={} self_preview={}", next.mode, next.paused, next.volume, next.self_preview), "");
                                if !next.self_preview { state.lock().unwrap().frames.remove(&room.local_participant().identity().to_string()); }
                                manual_layout = next;
                                layout = manual_layout.clone();
                                layout.paused |= window_hidden;
                                if reception_restart.is_none(){update_subscriptions(&room, &layout);}
                            }
                            Command::Share { adapter, output, codec } if cast.is_none() => {
                                let target=CaptureTarget::Monitor{adapter,output};
                                recovery = None;
                                recovery_attempts = 0;
                                crate::diagnostics::write("publication", &format!("adapter={adapter} output={output} codec={codec:?}"), "");
                                match Cast::new(target, codec, &runtime, &room) {
                                    Ok(mut value) => {
                                        if let Err(error) = value.set_audio(audio_requested, &runtime, &room) {
                                            state.lock().unwrap().audio_status = format!("Áudio: {error}");
                                            crate::diagnostics::write("audio_error", &format!("{error:#}"), &token);
                                        } else {
                                            state.lock().unwrap().audio_status = value.audio_status();
                                        }
                                        cast = Some(value);
                                        let mut state = state.lock().unwrap();
                                        state.active_target=Some(target);
                                        state.publishing = true;
                                        state.status = "Preparando compartilhamento…".into();
                                    }
                                    Err(error) => {
                                        crate::diagnostics::write("publication_error", &format!("{error:#}"), &token);
                                        let message = error.to_string();
                                        state.lock().unwrap().status = if message.contains("already has this screen track") {
                                            "A publicação anterior ainda está sendo encerrada. Aguarde e tente novamente; se persistir, reconecte à sala.".into()
                                        } else if message.contains("LimitExceeded") {
                                            "A sala atingiu o limite de telas. Você pode assistir e compartilhar quando alguém parar.".into()
                                        } else {
                                            error.to_string().replace(&token, "[token]")
                                        };
                                    }
                                }
                            }
                            Command::ShareWindow{adapter,hwnd,pid,codec} if cast.is_none()=>{
                                recovery=None;recovery_attempts=0;
                                let target=CaptureTarget::Window{adapter,hwnd,pid};
                                match Cast::new(target,codec,&runtime,&room){
                                    Ok(mut value)=>{
                                        if let Err(e)=value.set_audio(audio_requested,&runtime,&room){state.lock().unwrap().audio_status=format!("Áudio: {e}");}
                                        let audio_status=if state.lock().unwrap().audio_status.starts_with("Áudio:"){None}else{Some(value.audio_status())};
                                        cast=Some(value);let mut s=state.lock().unwrap();s.publishing=true;s.active_target=Some(target);if let Some(status)=audio_status{s.audio_status=status;}
                                    }
                                    Err(e)=>{crate::diagnostics::write("publication_error",&format!("{e:#}"),&token);state.lock().unwrap().status=format!("Janela: {e}").replace(&token,"[token]");}
                                }
                            }
                            Command::ChangeSource(target)=>{
                                if let Some(c)=&mut cast {
                                    let result=c.change_source(target,&runtime,&room,audio_requested);
                                    let mut s=state.lock().unwrap();s.active_target=Some(c.selection.0);s.audio_status=c.audio_status();
                                    s.frames.remove(&room.local_participant().identity().to_string());
                                    if let Err(e)=&result {crate::diagnostics::write("source_error",&format!("{e:#}"),&token);}
                                    s.source_error=result.err().map(|e|format!("Troca de origem: {e}").replace(&token,"[token]")).unwrap_or_default();
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
                                state.audio_status = "Áudio do sistema parado".into();
                                state.status = "Conectado".into();state.source_error.clear();state.active_target=None;
                                state
                                    .frames
                                    .remove(&room.local_participant().identity().to_string());
                            }
                            Command::SystemAudio(enabled) => {
                                audio_requested = enabled;
                                if let Some(value) = &mut cast {
                                    state.lock().unwrap().audio_status = match value.set_audio(enabled, &runtime, &room) {
                                        Ok(()) => value.audio_status(),
                                        Err(error) => { crate::diagnostics::write("audio_error", &format!("{error:#}"), &token); format!("Áudio: {error}") }
                                    };
                                }
                            }
                            _ => {}
                        }
                    }
                    for _ in 0..64 {
                        let Ok(event) = events.try_recv() else {
                            break;
                        };
                        match event {
                            RoomEvent::TrackSubscribed { track: RemoteTrack::Audio(_), publication, .. } => {
                                if reception_restart.is_some() || !publication.is_desired(){continue;}
                                update_subscriptions(&room,&layout);
                                audio_sinks.remove(&publication.sid());
                                let Some(RemoteTrack::Audio(track))=publication.track() else{continue;};
                                if publication.source()==TrackSource::ScreenshareAudio && publication.is_enabled() && audio_sinks.len()<3 {
                                    audio_sinks.insert(publication.sid(),AudioReceiver::start(&runtime,track,audio_volume.clone(),audio_received.clone()));
                                }
                            }
                            RoomEvent::TrackSubscribed {
                                track: RemoteTrack::Video(_),
                                publication,
                                participant,
                            } => {
                                if reception_restart.is_some() || !publication.is_desired(){continue;}
                                let Some(RemoteTrack::Video(track))=publication.track() else{continue;};
                                if let Some(previous) = sinks.remove(&publication.sid()) {
                                    previous.task.abort();
                                }
                                if sinks.len() >= 3 {
                                    publication.set_subscribed(false);
                                    continue;
                                }
                                sinks.insert(publication.sid(),attach_video(&runtime,track,&participant.identity().to_string(),&state,&ctx));
                            }
                            RoomEvent::TrackUnsubscribed { publication, .. } => {
                                // An old unsubscribe can be queued after a new subscribe for the same SID.
                                if publication.is_subscribed(){continue;}
                                audio_sinks.remove(&publication.sid());
                                if let Some(task) = sinks.remove(&publication.sid()) {
                                    task.abort();
                                }
                                // Retain the last GPU surface as a static paused
                                // thumbnail. It consumes no RTP or decoder work.
                                if reception_restart.is_none(){update_subscriptions(&room, &layout);}
                            }
                            RoomEvent::TrackUnpublished {
                                publication,
                                participant,
                            } => {
                                audio_sinks.remove(&publication.sid());
                                if let Some(task) = sinks.remove(&publication.sid()) {
                                    task.abort();
                                }
                                if publication.kind() == TrackKind::Video {
                                    let mut shared = state.lock().unwrap();
                                    let identity = participant.identity().to_string();
                                    if !shared.frozen.contains_key(&identity) { shared.frames.remove(&identity); }
                                }
                                if reception_restart.is_none(){update_subscriptions(&room, &layout);}
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
                                let mut shared = state.lock().unwrap();
                                if shared.frames.contains_key(&identity) {
                                    shared.frozen.entry(identity).or_insert_with(Instant::now);
                                    crate::diagnostics::write("recovery_view", "Retained screen during participant interruption", "");
                                }
                                drop(shared);
                                if reception_restart.is_none(){update_subscriptions(&room, &layout);}
                            }
                            RoomEvent::TrackPublished { .. }
                            | RoomEvent::ParticipantConnected(_) => {
                                update_subscriptions(&room, &layout)
                            }
                            RoomEvent::ReconnectAttempt { attempt, maximum } => {
                                reconnect_attempt = attempt;
                                state.lock().unwrap().status = format!("Reconectando… tentativa {attempt}/{maximum}");
                                crate::diagnostics::write("connection", &format!("Reconnect attempt {attempt}/{maximum}"), "");
                            }
                            RoomEvent::Reconnecting => {
                                state.lock().unwrap().status=format!("Reconectando… tentativa {}/3", reconnect_attempt.max(1));
                                if let Some(c)=&mut cast {if let Some(p)=&mut c.publisher {p.restart_keyframe();}}
                                crate::diagnostics::write("connection","Reconnecting; publication retained","");
                            }
                            RoomEvent::Reconnected => {
                                reconnect_attempt = 0;
                                if let Some(c)=&mut cast {
                                    c.paused=false;c.fps=120;
                                    if let Some(e)=&mut c.encoder{e.reconfigure(40_000_000,120)?;}
                                    if let Some(p)=&mut c.publisher {p.restart_keyframe();}
                                }
                                if reception_restart.is_none(){update_subscriptions(&room,&layout);}
                                state.lock().unwrap().status="Reconectado".into();
                                crate::diagnostics::write("connection","Reconnected; requesting keyframe","");
                            }
                            RoomEvent::Disconnected { .. } => {
                                crate::diagnostics::write("connection", "SDK disconnected", "");
                                if reconnect_attempt > 0 {
                                    state.lock().unwrap().status = format!("Conexão encerrada após tentativa {reconnect_attempt}/3. Entre novamente.");
                                }
                                break 'session;
                            }
                            _ => {}
                        }
                    }
                    if recovery.as_ref().is_some_and(|(_, at)| Instant::now() >= *at) {
                        let (selection, _) = recovery.take().unwrap();
                        recovery_attempts += 1;
                        crate::diagnostics::write("recovery", &format!("Rebuilding GPU pipeline, attempt {recovery_attempts}/3"), "");
                        match Cast::new(selection.0, selection.1, &runtime, &room) {
                            Ok(mut value) => {
                                if let Err(error) = value.set_audio(audio_requested, &runtime, &room) {
                                    state.lock().unwrap().audio_status = format!("Áudio: {error}");
                                }
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
                        let audio_failure = value.audio.as_ref().and_then(|audio| audio.failure.lock().unwrap().take());
                        if let Some(message) = audio_failure {
                            let _ = value.set_audio(false, &runtime, &room);
                            state.lock().unwrap().audio_status = format!("Áudio: {message}");
                            crate::diagnostics::write("audio_error", &message, "");
                        }
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
                                state.status = if room.connection_state()!=ConnectionState::Connected {
                                    format!("Reconectando… tentativa {}/3", reconnect_attempt.max(1))
                                } else if value.paused {
                                    "Compartilhamento ativo • envio pausado".into()
                                } else {
                                    let (w,h)=value.encoder.as_ref().map(|e|e.dimensions()).unwrap_or(value.dimensions);
                                    format!("{} · {w}×{h}",if value.selection.0.process().is_some(){"Compartilhando janela do aplicativo"}else{"Compartilhando sua tela"})
                                }
                                .into();
                            }
                            if let Some(texture) = value
                                .encoder
                                .as_ref()
                                .filter(|_| layout.self_preview && !value.paused && value.count % 8 == 0)
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
                        // A remote full restart changes participant/track SIDs. Keep its
                        // last surface by identity for a bounded grace period. A manual
                        // track stop still clears immediately; departures expire below.
                        let mut shared = state.lock().unwrap();
                        let expired: Vec<_> = shared.frozen.iter()
                            .filter(|(_, at)| at.elapsed() >= Duration::from_secs(60))
                            .map(|(id, _)| id.clone()).collect();
                        for id in expired { shared.frozen.remove(&id); shared.frames.remove(&id); }
                        let previous_people = shared.participants.clone();
                        shared.screens = room
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
                        shared.participants = room
                            .remote_participants()
                            .into_values()
                            .map(|p| (p.identity().to_string(), p.name()))
                            .collect();
                        for (id, name) in previous_people {
                            if shared.frozen.contains_key(&id) && !shared.participants.iter().any(|(owner, _)| owner == &id) {
                                shared.participants.push((id, name));
                            }
                        }
                        let frozen: Vec<_> = shared.frozen.keys().cloned().collect();
                        for id in frozen { if !shared.screens.contains(&id) { shared.screens.push(id); } }
                        drop(shared);
                        info_tick = Instant::now();
                        ctx.request_repaint();
                    }
                    if diagnostic_tick.elapsed() >= Duration::from_secs(5) {
                        let shared = state.lock().unwrap();
                        crate::diagnostics::write("media", &format!("sent={} received={} receivers={} publishing={}", shared.sent, shared.received, sinks.len(), shared.publishing), "");
                        crate::diagnostics::write("audio_stats", &format!("sent={} received={} receivers={}", cast.as_ref().and_then(|value| value.audio.as_ref()).map_or(0, |audio| audio.frames.load(Ordering::Relaxed)), audio_received.load(Ordering::Relaxed), audio_sinks.len()), "");
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
                audio_sinks.clear();
                for (_, task) in sinks {
                    task.abort();
                    let _ = runtime.block_on(task.task);
                }
                match runtime.block_on(room.close()) {
                    Ok(()) | Err(livekit::RoomError::AlreadyClosed) => {}
                    Err(error) => return Err(error.into()),
                }
                Ok(())
            })();
            let mut state = state.lock().unwrap();
            crate::diagnostics::write("session_summary", &format!("sent={} received={}", state.sent, state.received), "");
            state.connected = false;
            state.publishing = false;
            state.frames.clear();
            state.frozen.clear();
            state.status = result
                .err()
                .map(|e| e.to_string().replace(&token, "[token]"))
                .unwrap_or_else(|| if state.status.starts_with("Conexão encerrada após tentativa") { state.status.clone() } else { "Desconectado".into() });
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
