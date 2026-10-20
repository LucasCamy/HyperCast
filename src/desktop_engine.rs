use crate::resilience::StateMutex as Mutex;
use crate::{
    encoder::{Codec, HardwareEncoder,AmdProfile},
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
    sync::{Arc, mpsc, atomic::{AtomicU64, Ordering}},
    time::{Duration, Instant},
};
use tokio_stream::StreamExt;
use windows::{Win32::Graphics::Direct3D11::ID3D11Texture2D, core::Interface};

pub enum VideoFrame {
    Remote(BoxVideoFrame,Instant),
    Local(ID3D11Texture2D, u32, u32),
}
impl VideoFrame {
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Remote(frame,_) => (frame.buffer.width(), frame.buffer.height()),
            Self::Local(_, w, h) => (*w, *h),
        }
    }
    pub fn surface(&self) -> Option<(*mut std::ffi::c_void, u32)> {
        match self {
            Self::Remote(frame,_) => frame
                .buffer
                .as_native()
                .and_then(|native| native.d3d11_texture()),
            Self::Local(texture, _, _) => Some((texture.as_raw(), 0)),
        }
    }
    pub fn delivered_at(&self)->Option<Instant>{match self{Self::Remote(_,at)=>Some(*at),_=>None}}
    pub fn local(&self) -> bool {
        matches!(self, Self::Local(..))
    }
}

#[derive(Default)]
pub struct Shared {
    pub connected: bool,
    pub reconnecting:bool,
    pub quality:crate::video_metrics::QualitySnapshot,
    pub upload:crate::bandwidth::UploadProfile,
    pub encoder_stats:String,
    pub reception_resetting: bool,
    pub reception_restarts: u64,
    /// Recovering streams; true means the automatic retry budget is exhausted.
    pub recovering: HashMap<String,bool>,
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
    #[cfg(test)]
    pub audio_received:u64,
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
    RateMode(crate::adaptive_upload::Mode),
    #[cfg(test)]
    StallReader(String),
    #[cfg(test)]
    FailWorker,
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
    AmdProfile(AmdProfile),
    UploadProfile(crate::bandwidth::UploadProfile),
    StreamSettings(crate::resolution::StreamSettings),
    Disconnect,
}
pub struct Engine {
    pub commands: mpsc::Sender<Command>,
    pub shared: Arc<Mutex<Shared>>,
    thread: Option<std::thread::JoinHandle<()>>,
    cancel:crate::room_transition::Cancellation,
}
enum Access {
    Token(String),
    Invite { code: String, name: String, request_id: String, session:String, by_room:bool },
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.cancel.cancel();
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
    profile:AmdProfile,
    upload:crate::bandwidth::UploadProfile,
    metrics:crate::video_metrics::EncodeWindow,
    evidence:crate::evidence::Evidence,
    last_rate:(u64,u32),
    rate_control:crate::adaptive_upload::Controller,
    sent_bytes:u64,
    rate_blocked:bool,
    device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    encoder: Option<HardwareEncoder>,
    publisher: Option<EncodedPublisher>,
    start: Instant,
    count: u64,
    paused: bool,
    fps: u32,
    settings:crate::resolution::StreamSettings,
    audio: Option<SystemAudioPublisher>,
}
struct Sink {
    #[cfg(test)]
    stall: Arc<std::sync::atomic::AtomicBool>,
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
        profile:AmdProfile,
        upload:crate::bandwidth::UploadProfile,
        settings:crate::resolution::StreamSettings,
        rate_mode:crate::adaptive_upload::Mode,
        stopping:&crate::room_transition::Cancellation,
        evidence:&crate::evidence::Evidence,
    ) -> Result<Self> {
        let capture = VideoCapture::new(target)?;
        let device=capture.device().clone();
        let (width, height) = capture.dimensions();
        crate::diagnostics::write("capture", &format!("{width}x{height} target_fps={} codec={codec:?}",settings.fps), "");
        let encoder = HardwareEncoder::new_with_limits(
            capture.device(),
            width,
            height,
            settings.fps,
            upload.bps(),
            codec,profile,settings.size,
        )?;
        if rate_mode==crate::adaptive_upload::Mode::Automatic{if let Some(reason)=encoder.capabilities().automatic_rejection(){anyhow::bail!(reason.label());}}
        crate::diagnostics::write("encoder",encoder.backend_name(),"");
        // No Desktop Duplication frame is held while waiting for publication acknowledgment.
        let (send_width,send_height)=encoder.dimensions();
        crate::diagnostics::write("bandwidth",&format!("video_limit_bps={}",upload.bps()),"");
        crate::diagnostics::write("resolution",&format!("source={width}x{height} output={send_width}x{send_height}"),"");
        let publisher = runtime.block_on(stopping.run(EncodedPublisher::in_room(
            room.clone(),
            send_width,
            send_height,
            settings.fps,
            upload.bps(),
            codec == Codec::Av1,
        ))).ok_or_else(||anyhow::anyhow!(crate::i18n::label("Publicação cancelada","Publication cancelled")))??;
        Ok(Self {
            audio: None,profile,upload,rate_control:crate::adaptive_upload::Controller::new(rate_mode,upload.bps()),metrics:Default::default(),last_rate:(upload.bps() as u64,settings.fps),sent_bytes:0,rate_blocked:false,
            selection: (target, codec),
            dimensions:(width,height),device,has_frame:false,
            evidence:evidence.clone(),capture,
            encoder: Some(encoder),
            publisher: Some(publisher),
            start: Instant::now(),
            count: 0,
            paused: false,
            fps: settings.fps,settings,
        })
    }
    fn tick(&mut self, _runtime: &tokio::runtime::Runtime, _room: &Arc<Room>) -> Result<()> {
        if _room.connection_state()!=ConnectionState::Connected {
            if let Some(p)=&mut self.publisher {p.restart_keyframe();}
            return Ok(());
        }
        let capture_started=Instant::now();
        let acquired=self.capture.acquire();
        self.evidence.record(crate::evidence::Stage::CaptureAcquire,capture_started.elapsed());
        let frame=acquired?;
        if let Some(frame)=&frame {
            let mut desc=Default::default();unsafe{frame.texture().GetDesc(&mut desc);}
            if (desc.Width,desc.Height)!=self.dimensions {
                self.encoder=Some(HardwareEncoder::new_with_limits(&self.device,desc.Width,desc.Height,self.settings.fps,self.upload.bps(),self.selection.1,self.profile,self.settings.size)?);
                if self.last_rate.0>0 {if let Some(encoder)=&mut self.encoder{self.rate_blocked=!encoder.reconfigure(self.upload.active_rate(self.last_rate.0),self.fps)?;self.paused=self.rate_blocked;}}
                self.dimensions=(desc.Width,desc.Height);
                if let Some(p)=&mut self.publisher {let (w,h)=self.encoder.as_ref().unwrap().dimensions();p.set_resolution(w,h);}
            }
        }
        if !self.has_frame && frame.is_none() {
            return Ok(());
        }
        self.has_frame |= frame.is_some();
        if let (Some(encoder), Some(publisher)) = (&mut self.encoder, &mut self.publisher) {
            let key_requested=publisher.take_keyframe_request();
            let mut key=self.count==0 || key_requested;
            if let Some(rate)=publisher.take_rate_control_request() {
                let was_paused=self.paused;
                // WebRTC SetRates reflects its measured input cadence as well as
                // allocation. Feeding that FPS back into this external source's
                // pacer ratchets transient slowdowns down permanently. Adapt
                // bitrate (including zero/pause), keeping the user's cadence.
                let fps=self.settings.fps;
                let target=self.rate_control.sample(Instant::now(),rate.target_bitrate_bps);
                self.last_rate=(target as u64,fps);self.paused=target==0;
                self.rate_blocked=false;
                if !self.paused {
                    let accepted=encoder.reconfigure(target,fps)?;
                    self.rate_blocked=!accepted;self.paused=!accepted;
                    if accepted{self.fps=fps;}
                }
                if was_paused && !self.paused {key=true;publisher.restart_keyframe();}
            }
            if !self.paused {
                let packet =
                    encoder.encode_texture(frame.as_ref().map(|f|f.texture()), self.start.elapsed().as_micros() as i64, key)?;
                self.evidence.record(crate::evidence::Stage::EncodeCall,packet.encode_time);
                self.metrics.record(packet.encode_time,packet.bytes.len(),packet.keyframe,key,packet.qp);
                let submit_started=Instant::now();
                let pushed=publisher.push(packet.bytes, packet.timestamp_us, packet.keyframe);
                self.evidence.record(crate::evidence::Stage::SendSubmit,submit_started.elapsed());
                if pushed? {
                    self.count += 1;
                    self.sent_bytes+=packet.bytes.len() as u64;
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
        let mut encoder=HardwareEncoder::new_with_limits(&device,dimensions.0,dimensions.1,self.settings.fps,self.upload.bps(),self.selection.1,self.profile,self.settings.size)?;
        if self.last_rate.0>0 {self.rate_blocked=!encoder.reconfigure(self.upload.active_rate(self.last_rate.0),self.fps)?;self.paused=self.rate_blocked;}
        let send_size=encoder.dimensions();
        let change_audio=self.selection.0.process()!=target.process();
        if change_audio {if let Some(audio)=&self.audio{audio.set_process(target.process())?;}}
        self.capture=capture;self.device=device;self.encoder=Some(encoder);self.dimensions=dimensions;self.selection.0=target;self.has_frame=false;
        if let Some(p)=&mut self.publisher {p.set_resolution(send_size.0,send_size.1);}
        self.evidence.event(crate::evidence::EventKind::SourceChanged);
        crate::diagnostics::write("source_switch",&format!("{}x{} application={}",dimensions.0,dimensions.1,target.process().is_some()),"");
        if change_audio && self.audio.is_none() {self.set_audio(audio_requested,runtime,room)?;}
        Ok(())
    }
    fn audio_status(&self)->String{if self.audio.is_none(){crate::i18n::label("Áudio desativado","Audio disabled").into()}else if self.selection.0.process().is_some(){crate::i18n::label("Enviando áudio do aplicativo","Sending app audio").into()}else{crate::i18n::label("Enviando áudio do sistema","Sending system audio").into()}}
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

fn update_subscriptions(room: &Room, layout: &Layout, receive_monitor: &crate::reception_recovery::runtime::Monitor) {
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
            && !receive_monitor.held(&publication.sid().to_string())
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
    #[cfg(test)]
    let stall=Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(test)]
    let reader_stall=stall.clone();
    let task = runtime.spawn(async move {
        while let Some(frame) = stream.next().await {
            if !enabled_track.enabled(){continue;}
            #[cfg(test)]
            if reader_stall.load(Ordering::Relaxed){continue;}
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
                    Arc::new(VideoFrame::Remote(frame,Instant::now())),
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
    Sink { owner, task, #[cfg(test)] stall }
}

impl Engine {
    pub fn request_exit(&self){self.cancel.cancel();}
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
            status: crate::i18n::label("Conectando…","Connecting…").into(),
            ..Default::default()
        }));
        let evidence=shared.lock().unwrap().quality.evidence.clone();evidence.event(crate::evidence::EventKind::Connecting);
        let state = shared.clone();
        let cancel=crate::room_transition::Cancellation::default();let stopping=cancel.clone();
        let failure_state=shared.clone();let failure_ctx=ctx.clone();
        let thread = std::thread::spawn(move || {
            let healthy=crate::resilience::guard(|| {
            let Ok(runtime)=tokio::runtime::Runtime::new() else {state.lock().unwrap().status=crate::i18n::label("Não foi possível iniciar a conexão","Could not start the connection").into();return;};
            let admission=runtime.block_on(stopping.run(async {
                match access {
                    Access::Token(token)=>Ok((token,None)),
                    Access::Invite{code,name,request_id,session,by_room}=>{
                        let joined=crate::invitation::join_async(&address,&code,&name,&request_id,&session,by_room).await?;
                        Ok::<_,anyhow::Error>((joined.token,joined.title))
                    }
                }
            }));
            let (token,room_title)=match admission {
                Some(Ok(value))=>value,
                Some(Err(error))=>{evidence.event(crate::evidence::EventKind::SessionFailed);state.lock().unwrap().status=error.to_string();ctx.request_repaint();runtime.shutdown_timeout(Duration::from_secs(2));return;},
                None=>{state.lock().unwrap().status=crate::i18n::label("Conexão cancelada","Connection cancelled").into();ctx.request_repaint();runtime.shutdown_timeout(Duration::from_secs(2));return;}
            };
            let result = (|| -> Result<()> {
                let _runtime_guard = runtime.enter();
                livekit::webrtc::enable_zero_playout_delay()?;
                let (room, mut events) = match runtime.block_on(stopping.run(network::connect_desktop(&address, &token, relay_only,|attempt,_|{evidence.event(crate::evidence::EventKind::ConnectRetry);state.lock().unwrap().status=crate::i18n_format!("Tentando conectar novamente… {attempt}/3","Retrying connection… {attempt}/3");ctx.request_repaint();}))){Some(result)=>result?,None=>return Ok(())};
                let room = Arc::new(room);
                crate::diagnostics::write("connection", "Room connected", "");
                {
                    let mut state = state.lock().unwrap();
                    state.connected = true;
                    state.room = room_title.or_else(||serde_json::from_str::<serde_json::Value>(&room.metadata()).ok()
                        .and_then(|value|value["hypercast_name"].as_str().filter(|name|name.chars().count()<=64).map(str::to_owned)))
                        .unwrap_or_else(||room.name());
                    state.status = crate::i18n::label("Conectado","Connected").into();
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
                let mut amd_profile=AmdProfile::Fast;
                let mut upload=crate::bandwidth::UploadProfile::default();
                let mut rate_mode=crate::adaptive_upload::Mode::Manual;
                let mut settings=crate::resolution::StreamSettings::default();
                let audio_volume = Arc::new(AtomicU64::new(80));
                let audio_received = Arc::new(AtomicU64::new(0));
                let mut pacer = FramePacer::new(120)?;
                let mut receive_monitor=crate::reception_recovery::runtime::Monitor::default();
                update_subscriptions(&room, &layout, &receive_monitor);
                let mut reception_restart:Option<Instant>=None;
                let mut reception_tick=Instant::now();
                let mut reception_trace=Instant::now();
                let trace_reception=std::env::var_os("HYPERCAST_SMOKE_SECONDS").is_some();
                let mut info_tick = Instant::now();
                let mut diagnostic_tick = Instant::now();
                let(mut send_meter,mut receive_meter)=(crate::video_metrics::TrafficMeter::default(),crate::video_metrics::TrafficMeter::default());
                let mut send_origin=None;
                'session: loop {
                    if state.was_reset(){break 'session;}
                    if stopping.is_cancelled(){break 'session;}
                    if trace_reception && reception_trace.elapsed()>=Duration::from_secs(1) {
                        let shared=state.lock().unwrap();println!("Receive progress: {}",serde_json::to_string(&shared.received_streams).unwrap_or_default());
                        println!("Receive throughput: {:.1}",shared.quality.receive.map_or(0.0,|r|r.fps));
                        reception_trace=Instant::now();
                    }
                    if reception_restart.is_some_and(|until|Instant::now()>=until) {
                        reception_restart=None;update_subscriptions(&room,&layout,&receive_monitor);
                        state.lock().unwrap().reception_resetting=false;
                        crate::diagnostics::write("reception","Receive settings restored; waiting for fresh frames","");
                    }
                    if reception_restart.is_none() && reception_tick.elapsed()>=Duration::from_millis(250) {
                        let repairs=runtime.block_on(receive_monitor.tick(&room,!layout.paused,|_,owner|state.lock().unwrap().received_streams.get(owner).copied().unwrap_or(0)));
                        for repair in repairs {
                            if let Some(sid)=sinks.keys().find(|sid|sid.to_string()==repair.sid).cloned(){
                                if let Some(old)=sinks.remove(&sid){old.abort();runtime.block_on(async{let _=old.task.await;});}
                            }
                            crate::diagnostics::write("receive_recovery",&format!("action={:?} reason={:?} total={}",repair.action,repair.reason,receive_monitor.summary.actions),"");
                        }
                        {let mut s=state.lock().unwrap();#[cfg(test)] {s.audio_received=audio_received.load(Ordering::Relaxed);}
                            s.quality.network=receive_monitor.network;s.quality.automatic_recovery=receive_monitor.summary;s.recovering=receive_monitor.recovering().into_iter().map(|(_,owner,exhausted)|(owner,exhausted)).collect();}
                        update_subscriptions(&room,&layout,&receive_monitor);
                        for participant in room.remote_participants().into_values() {
                            for p in participant.track_publications().into_values() {
                                if !p.is_desired(){continue;}
                                match p.track() {
                                    Some(RemoteTrack::Video(track)) if p.source()==TrackSource::Screenshare && !receive_monitor.held(&p.sid().to_string()) && sinks.get(&p.sid()).is_none_or(|s|s.task.is_finished()) && (sinks.contains_key(&p.sid()) || sinks.len()<3) => {
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
                        if reception_restart.is_none(){update_subscriptions(&room, &layout, &receive_monitor);}
                        if std::env::var("HYPERCAST_SMOKE_SECONDS").is_ok() {
                            println!(
                                "GUI viewport: {} at {} frames",
                                if hidden { "minimized" } else { "restored" },
                                state.lock().unwrap().received
                            );
                        }
                    }
                    while let Ok(command) = commands.try_recv() {
                        if stopping.is_cancelled(){break 'session;}
                        match command {
                            #[cfg(test)] Command::FailWorker=>panic!("injected worker failure"),
                            Command::StreamSettings(value)=>{if cast.is_none()&&recovery.is_none()&&value.valid(){settings=value;}},
                            Command::AmdProfile(profile)=>{if cast.is_none(){amd_profile=profile;}},
                            Command::RateMode(mode)=>{if cast.is_none()&&recovery.is_none(){rate_mode=mode;}},
                            Command::UploadProfile(profile)=>{if cast.is_none()&&recovery.is_none(){upload=profile;}},
                            Command::Disconnect => {
                                crate::diagnostics::write("connection", "User disconnected", "");
                                break 'session;
                            }
                            #[cfg(test)]
                            Command::StallReader(owner)=>{if let Some(sink)=sinks.values().find(|s|s.owner==owner){sink.stall.store(true,Ordering::Relaxed);}},
                            Command::RestartReception if reception_restart.is_none() && room.connection_state()==ConnectionState::Connected => {
                                evidence.event(crate::evidence::EventKind::ManualReceptionReset);
                            receive_monitor.reset();
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
                                if reception_restart.is_none(){update_subscriptions(&room, &layout, &receive_monitor);}
                            }
                            Command::Share { adapter, output, codec } if cast.is_none() => {
                                let target=CaptureTarget::Monitor{adapter,output};
                                recovery = None;
                                recovery_attempts = 0;
                                crate::diagnostics::write("publication", &format!("adapter={adapter} output={output} codec={codec:?}"), "");
                                match Cast::new(target, codec, &runtime, &room,amd_profile,upload,settings,rate_mode,&stopping,&evidence) {
                                    Ok(mut value) => {
                                        if let Err(error) = value.set_audio(audio_requested, &runtime, &room) {
                                            state.lock().unwrap().audio_status = crate::i18n_format!("Áudio: {error}","Audio: {error}");
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
                                            crate::i18n::label("A publicação anterior ainda está sendo encerrada. Aguarde e tente novamente; se persistir, reconecte à sala.","The previous publication is still closing. Wait and try again; if it persists, reconnect to the room.").into()
                                        } else if message.contains("LimitExceeded") {
                                            crate::i18n::label("A sala atingiu o limite de telas. Você pode assistir e compartilhar quando alguém parar.","The room reached its stream limit. You can watch and share when someone stops.").into()
                                        } else {
                                            error.to_string().replace(&token, "[token]")
                                        };
                                    }
                                }
                            }
                            Command::ShareWindow{adapter,hwnd,pid,codec} if cast.is_none()=>{
                                recovery=None;recovery_attempts=0;
                                let target=CaptureTarget::Window{adapter,hwnd,pid};
                                match Cast::new(target,codec,&runtime,&room,amd_profile,upload,settings,rate_mode,&stopping,&evidence){
                                    Ok(mut value)=>{
                                        if let Err(e)=value.set_audio(audio_requested,&runtime,&room){state.lock().unwrap().audio_status=crate::i18n_format!("Áudio: {e}","Audio: {e}");}
                                        let audio_status=if state.lock().unwrap().audio_status.starts_with(crate::i18n::label("Áudio:","Audio:")){None}else{Some(value.audio_status())};
                                        cast=Some(value);let mut s=state.lock().unwrap();s.publishing=true;s.active_target=Some(target);if let Some(status)=audio_status{s.audio_status=status;}
                                    }
                                    Err(e)=>{crate::diagnostics::write("publication_error",&format!("{e:#}"),&token);state.lock().unwrap().status=crate::i18n_format!("Janela: {e}","Window: {e}").replace(&token,"[token]");}
                                }
                            }
                            Command::ChangeSource(target)=>{
                                if let Some(c)=&mut cast {
                                    let result=c.change_source(target,&runtime,&room,audio_requested);
                                    let mut s=state.lock().unwrap();s.active_target=Some(c.selection.0);s.audio_status=c.audio_status();
                                    s.frames.remove(&room.local_participant().identity().to_string());
                                    if let Err(e)=&result {crate::diagnostics::write("source_error",&format!("{e:#}"),&token);}
                                    s.source_error=result.err().map(|e|crate::i18n_format!("Troca de origem: {e}","Source change: {e}").replace(&token,"[token]")).unwrap_or_default();
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
                                state.audio_status = crate::i18n::label("Áudio do sistema parado","System audio stopped").into();
                                state.status = crate::i18n::label("Conectado","Connected").into();state.source_error.clear();state.active_target=None;
                                state
                                    .frames
                                    .remove(&room.local_participant().identity().to_string());
                            }
                            Command::SystemAudio(enabled) => {
                                audio_requested = enabled;
                                if let Some(value) = &mut cast {
                                    state.lock().unwrap().audio_status = match value.set_audio(enabled, &runtime, &room) {
                                        Ok(()) => value.audio_status(),
                                        Err(error) => { crate::diagnostics::write("audio_error", &format!("{error:#}"), &token); crate::i18n_format!("Áudio: {error}","Audio: {error}") }
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
                                update_subscriptions(&room,&layout,&receive_monitor);
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
                                if reception_restart.is_some() || !publication.is_desired() || receive_monitor.held(&publication.sid().to_string()){continue;}
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
                                if reception_restart.is_none(){update_subscriptions(&room, &layout, &receive_monitor);}
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
                                if reception_restart.is_none(){update_subscriptions(&room, &layout, &receive_monitor);}
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
                                if reception_restart.is_none(){update_subscriptions(&room, &layout, &receive_monitor);}
                            }
                            RoomEvent::TrackPublished { .. }
                            | RoomEvent::ParticipantConnected(_) => {
                                update_subscriptions(&room, &layout, &receive_monitor)
                            }
                            RoomEvent::ReconnectAttempt { attempt, maximum } => {
                                reconnect_attempt = attempt;
                                state.lock().unwrap().status = format!("Reconectando… tentativa {attempt}/{maximum}");
                                crate::diagnostics::write("connection", &format!("Reconnect attempt {attempt}/{maximum}"), "");
                            }
                            RoomEvent::Reconnecting => {
                                receive_monitor.suspend();
                                state.lock().unwrap().reconnecting=true;
                                state.lock().unwrap().status=crate::i18n_format!("Reconectando… tentativa {}/3","Reconnecting… attempt {}/3", reconnect_attempt.max(1));
                                if let Some(c)=&mut cast {if let Some(p)=&mut c.publisher {p.restart_keyframe();}}
                                crate::diagnostics::write("connection","Reconnecting; publication retained","");
                            }
                            RoomEvent::Reconnected => {
                                state.lock().unwrap().reconnecting=false;
                                reconnect_attempt = 0;
                                let mut encoder_error=None;
                                if let Some(c)=&mut cast {
                                    // A new WebRTC encoder needs a bootstrap frame after reconnect.
                                    if c.last_rate.0==0{c.paused=true;}else if let Some(e)=&mut c.encoder{match e.reconfigure(c.upload.active_rate(c.last_rate.0),c.fps){
                                        Ok(applied)=>{c.rate_blocked=!applied;c.paused=c.rate_blocked;},
                                        Err(error)=>encoder_error=Some(error),
                                    }}
                                    if let Some(p)=&mut c.publisher {p.restart_keyframe();}
                                }
                                if let Some(error)=encoder_error {
                                    if let Some(c)=cast.take(){c.stop(&runtime);}
                                    let mut s=state.lock().unwrap();s.publishing=false;
                                    s.source_error=crate::i18n_format!("Encoder: {error}. Tente compartilhar novamente.","Encoder: {error}. Try sharing again.").replace(&token,"[token]");
                                }
                                if reception_restart.is_none(){update_subscriptions(&room,&layout,&receive_monitor);}
                                state.lock().unwrap().status="Reconectado".into();
                                crate::diagnostics::write("connection","Reconnected; requesting keyframe","");
                            }
                            RoomEvent::Disconnected { .. } => {
                                crate::diagnostics::write("connection", "SDK disconnected", "");
                                if reconnect_attempt > 0 {
                                    state.lock().unwrap().status = crate::i18n_format!("Conexão encerrada após tentativa {reconnect_attempt}/3. Entre novamente.","Connection ended after attempt {reconnect_attempt}/3. Join again.");
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
                        match Cast::new(selection.0, selection.1, &runtime, &room,amd_profile,upload,settings,rate_mode,&stopping,&evidence) {
                            Ok(mut value) => {
                                if let Err(error) = value.set_audio(audio_requested, &runtime, &room) {
                                    state.lock().unwrap().audio_status = crate::i18n_format!("Áudio: {error}","Audio: {error}");
                                }
                                cast = Some(value);
                                state.lock().unwrap().publishing = true;
                            }
                            Err(error) => {
                                crate::diagnostics::write("recovery_error", &format!("{error:#}"), &token);
                                if recovery_attempts < 3 {
                                    recovery = Some((selection, Instant::now() + Duration::from_secs(1)));
                                } else {
                                    state.lock().unwrap().status = crate::i18n_format!("Recuperação interrompida: {error}","Recovery interrupted: {error}").replace(&token, "[token]");
                                }
                            }
                        }
                    }
                    if let Some(value) = &mut cast {
                        let audio_failure = value.audio.as_ref().and_then(|audio| audio.failure.lock().unwrap().take());
                        if let Some(message) = audio_failure {
                            let _ = value.set_audio(false, &runtime, &room);
                            state.lock().unwrap().audio_status = crate::i18n_format!("Áudio: {message}","Audio: {message}");
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
                            evidence.event(crate::evidence::EventKind::PublicationFailed);
                            let message = format!("{error:#}");
                            crate::diagnostics::write("capture_error", &message, &token);
                            let recoverable = recoverable_capture_error(&message);
                            if recoverable && recovery_attempts < 3 {
                                recovery = Some((value.selection, Instant::now() + Duration::from_secs(1)));
                            }
                            state.lock().unwrap().status = if recovery.is_some() {
                                crate::i18n::label("Tela mudou; recuperando compartilhamento…","Screen changed; recovering stream…").into()
                            } else {
                                crate::i18n_format!("Compartilhamento: {error}","Stream: {error}").replace(&token, "[token]")
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
                                    crate::i18n_format!("Reconectando… tentativa {}/3","Reconnecting… attempt {}/3", reconnect_attempt.max(1))
                                } else if value.rate_blocked {
                                    crate::i18n::label("Envio aguardando uma taxa aceita pelo driver AMD","Waiting for a rate accepted by the AMD driver").into()
                                } else if value.paused {
                                    crate::i18n::label("Compartilhamento ativo • envio pausado","Stream active • sending paused").into()
                                } else {
                                    let (w,h)=value.encoder.as_ref().map(|e|e.dimensions()).unwrap_or(value.dimensions);
                                    format!("{} · {w}×{h}",if value.selection.0.process().is_some(){crate::i18n::label("Compartilhando janela do aplicativo","Sharing an app window")}else{crate::i18n::label("Compartilhando sua tela","Sharing your screen")})
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
                    {
                        let now=Instant::now();let mut shared=state.lock().unwrap();
                        if let Some(value)=&cast {
                            shared.upload=value.upload;shared.quality.encode_caps=value.encoder.as_ref().map(|e|e.capabilities());
                            shared.quality.adaptive_upload=Some(value.rate_control.snapshot());
                            if send_origin!=Some(value.start){send_origin=Some(value.start);send_meter=Default::default();shared.quality.send=None;shared.quality.applied_encoder=None;shared.quality.encode_window=None;}
                            if let Some(sample)=send_meter.sample(now,value.count,value.sent_bytes){shared.quality.send=Some(sample);}
                            shared.quality.resolution=value.encoder.as_ref().map(|e|e.dimensions());
                            shared.quality.encoder_backend=value.encoder.as_ref().map(|e|e.backend_name());
                            shared.quality.codec=Some(if value.selection.1==Codec::Av1{"AV1"}else{"HEVC"});
                            shared.quality.send_paused=value.paused;shared.quality.encoder_rate_rejected=value.rate_blocked;
                        }else{shared.quality.encode_window=None;shared.quality.encode_caps=None;shared.quality.adaptive_upload=None;shared.quality.encoder_rate_rejected=false;send_origin=None;send_meter=Default::default();shared.quality.send=None;shared.quality.resolution=None;shared.quality.send_paused=false;shared.quality.encoder_backend=None;shared.quality.codec=None;shared.quality.applied_encoder=None;}
                        if let Some(sample)=receive_meter.sample(now,shared.received,0){shared.quality.receive=Some(sample);}
                        shared.quality.observe(shared.connected,shared.reconnecting,shared.publishing);
                    }
                    if diagnostic_tick.elapsed() >= Duration::from_secs(5) {
                        if let Some(value)=&mut cast {
                            if let Some(encoder)=&value.encoder{
                                let feedback=encoder.feedback();
                                state.lock().unwrap().quality.adaptive_upload=Some(value.rate_control.snapshot());
                                state.lock().unwrap().quality.applied_encoder=(feedback.bitrate>0&&feedback.fps>0).then_some((feedback.bitrate,feedback.fps));
                                state.lock().unwrap().quality.encode_window=value.metrics.quality();
                                let summary=format!("backend={} profile={:?} applied_profile={:?} requested_bps={} applied_bps={} fps={} vbv_bits={:?} vbv_runtime={:?} rate_blocked={} {}",
                                    encoder.backend_name(),value.profile,encoder.profile(),value.last_rate.0,feedback.bitrate,feedback.fps,feedback.vbv_bits,feedback.vbv_runtime,value.rate_blocked,value.metrics.summary_and_reset());
                                crate::diagnostics::write("encoder_window",&summary,"");state.lock().unwrap().encoder_stats=summary;
                            }
                        }

                        let shared = state.lock().unwrap();
                        crate::diagnostics::write("media", &format!("sent={} received={} receivers={} publishing={}", shared.sent, shared.received, sinks.len(), shared.publishing), "");
                        crate::diagnostics::write("audio_stats", &format!("sent={} received={} receivers={}", cast.as_ref().and_then(|value| value.audio.as_ref()).map_or(0, |audio| audio.frames.load(Ordering::Relaxed)), audio_received.load(Ordering::Relaxed), audio_sinks.len()), "");
                        drop(shared);
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
                anyhow::ensure!(!state.was_reset(),crate::resilience::worker_failure());
                Ok(())
            })();
            let mut state = state.lock().unwrap();
            crate::diagnostics::write("session_summary", &format!("sent={} received={}", state.sent, state.received), "");
            evidence.event(crate::evidence::EventKind::Disconnected);
            state.connected = false;
            state.reconnecting=false;state.reception_resetting=false;state.can_publish=false;
            state.publishing = false;
            state.frames.clear();
            state.frozen.clear();
            if result.is_err(){evidence.event(crate::evidence::EventKind::SessionFailed);}
            state.status = result
                .err()
                .map(|e| e.to_string().replace(&token, "[token]"))
                .unwrap_or_else(|| if state.status.starts_with(crate::i18n::label("Conexão encerrada após tentativa","Connection ended after attempt")) { state.status.clone() } else { crate::i18n::label("Desconectado","Disconnected").into() });
            crate::diagnostics::write("session_end", &state.status, &token);
            drop(state);
            ctx.request_repaint();
            runtime.shutdown_timeout(Duration::from_secs(2));
            });
            if !healthy {let mut s=failure_state.lock().unwrap();*s=Shared::default();s.status=crate::resilience::worker_failure().into();drop(s);failure_ctx.request_repaint();}
        });
        Self {
            commands: tx,
            shared, cancel,
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
