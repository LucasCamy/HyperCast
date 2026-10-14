use super::media::{AudioCapture, Capture, Codec, Encoder, Playout, Source};
use crate::{
    network::{self, EncodedPublisher},
    subscription::{Demand, ViewMode, delivery_demand},
};
use anyhow::Result;
use eframe::egui;
use livekit::{
    prelude::*,
    track::VideoQuality,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::{AudioSourceOptions, RtcAudioSource, native::NativeAudioSource},
        audio_stream::native::{NativeAudioStream, NativeAudioStreamOptions},
        video_frame::BoxVideoFrame,
        video_stream::native::{NativeVideoStream, NativeVideoStreamOptions},
    },
};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct Screen {
    pub sid: String,
    pub owner: String,
    pub name: String,
}
#[derive(Default)]
pub struct State {
    pub connected: bool,
    pub reconnecting: bool,
    pub sharing: bool,
    pub selecting: bool,
    pub can_publish: bool,
    pub status: String,
    pub participants: Vec<(String, String)>,
    pub screens: Vec<Screen>,
    pub frames: HashMap<String, (u64, Arc<BoxVideoFrame>)>,
    pub sent: u64,
    pub received: u64,
    pub audio_received: u64,
}
#[derive(Clone)]
pub struct Layout {
    pub mode: ViewMode,
    pub focus: Option<String>,
    pub paused: bool,
    pub volume: u32,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            mode: ViewMode::Spotlight,
            focus: None,
            paused: false,
            volume: 80,
        }
    }
}
pub enum Command {
    Layout(Layout),
    Share(Source, Codec, Encoder),
    Stop,
    CancelSelection,
    Audio(bool),
    Disconnect,
}
pub struct Engine {
    pub state: Arc<Mutex<State>>,
    pub tx: tokio::sync::mpsc::UnboundedSender<Command>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Engine {
    pub fn start(address: String, token: String, ctx: egui::Context) -> Self {
        let state = Arc::new(Mutex::new(State {
            status: "Conectando…".into(),
            ..Default::default()
        }));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let shared = state.clone();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let result = runtime.block_on(session(address, token, rx, shared.clone(), ctx.clone()));
            if let Err(error) = result {
                let mut s = shared.lock().unwrap();
                s.status = error.to_string();
                s.connected = false;
                s.sharing = false;
            }
            ctx.request_repaint();
            runtime.shutdown_timeout(Duration::from_secs(2));
        });
        Self {
            state,
            tx,
            thread: Some(thread),
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Disconnect);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
struct Sink(tokio::task::JoinHandle<()>);
impl Drop for Sink {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn refresh(
    room: &Room,
    layout: &Layout,
    shared: &Arc<Mutex<State>>,
    mute_playback: bool,
) -> HashMap<String, String> {
    let mut screens: Vec<_> = room
        .remote_participants()
        .into_values()
        .flat_map(|p| {
            let owner = p.identity().to_string();
            let name = p.name().to_owned();
            p.track_publications()
                .into_values()
                .filter(|t| t.source() == TrackSource::Screenshare)
                .map(move |publication| {
                    (
                        Screen {
                            sid: publication.sid().to_string(),
                            owner: owner.clone(),
                            name: name.clone(),
                        },
                        publication,
                    )
                })
        })
        .collect();
    screens.sort_by(|a, b| a.0.owner.cmp(&b.0.owner));
    screens.truncate(3);
    let focus = layout
        .focus
        .as_deref()
        .filter(|id| screens.iter().any(|s| s.0.owner == *id))
        .or_else(|| screens.first().map(|s| s.0.owner.as_str()));
    let mut active = HashMap::new();
    for (screen, publication) in &screens {
        let wanted = delivery_demand(
            layout.mode,
            !layout.paused,
            Some(screen.owner.as_str()) == focus,
            publication.simulcasted(),
        );
        let enabled = wanted != Demand::Paused;
        if publication.is_subscribed() {
            publication.set_enabled(enabled);
        }
        publication.set_subscribed(enabled);
        if enabled {
            active.insert(screen.sid.clone(), screen.owner.clone());
            if publication.simulcasted() {
                publication.set_video_quality(if wanted == Demand::Thumbnail {
                    VideoQuality::Low
                } else {
                    VideoQuality::High
                });
            }
        }
    }
    for participant in room.remote_participants().into_values() {
        let owner = participant.identity().to_string();
        for publication in participant.track_publications().into_values() {
            if publication.source() != TrackSource::ScreenshareAudio {
                continue;
            }
            let enabled = !mute_playback
                && !layout.paused
                && layout.volume > 0
                && active.values().any(|id| id == &owner);
            if publication.is_subscribed() {
                publication.set_enabled(enabled);
            }
            publication.set_subscribed(enabled);
        }
    }
    let mut state = shared.lock().unwrap();
    state.screens = screens.into_iter().map(|s| s.0).collect();
    state.participants = room
        .remote_participants()
        .into_values()
        .map(|p| (p.identity().to_string(), p.name().to_owned()))
        .collect();
    state.participants.sort_by(|a, b| a.0.cmp(&b.0));
    let present: Vec<String> = state.screens.iter().map(|s| s.sid.clone()).collect();
    state.frames.retain(|sid, _| present.contains(sid));
    active
}
struct AudioCast {
    capture: AudioCapture,
    source: NativeAudioSource,
    sid: TrackSid,
    track_id: String,
    pending: Vec<i16>,
}
impl AudioCast {
    async fn open(room: &Room) -> Result<Self> {
        let capture = AudioCapture::open()?;
        let source = NativeAudioSource::new(
            AudioSourceOptions {
                echo_cancellation: false,
                noise_suppression: false,
                auto_gain_control: false,
            },
            48000,
            2,
            30,
        );
        let track = LocalAudioTrack::create_audio_track(
            "system-audio",
            RtcAudioSource::Native(source.clone()),
        );
        let track_id = track.rtc_track().id();
        let publication = room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                livekit::options::TrackPublishOptions {
                    source: TrackSource::ScreenshareAudio,
                    audio_encoding: Some(livekit::options::AudioEncoding {
                        max_bitrate: 128000,
                    }),
                    dtx: false,
                    red: true,
                    simulcast: false,
                    ..Default::default()
                },
            )
            .await?;
        Ok(Self {
            capture,
            source,
            sid: publication.sid(),
            track_id,
            pending: Vec::new(),
        })
    }
    async fn tick(&mut self) -> Result<()> {
        if let Some(samples) = self.capture.poll()? {
            self.pending.extend(samples);
        }
        if self.pending.len() > 2880 {
            let extra = self.pending.len() - 2880;
            self.pending.drain(..extra);
        }
        while self.pending.len() >= 960 {
            let data: Vec<i16> = self.pending.drain(..960).collect();
            self.source
                .capture_frame(&AudioFrame {
                    data: Cow::Owned(data),
                    sample_rate: 48000,
                    num_channels: 2,
                    samples_per_channel: 480,
                })
                .await?;
        }
        Ok(())
    }
    async fn stop(self, room: &Room) {
        let sid = room
            .local_participant()
            .track_publications()
            .into_values()
            .find(|p| {
                p.track()
                    .is_some_and(|t| t.rtc_track().id() == self.track_id)
            })
            .map(|p| p.sid())
            .unwrap_or(self.sid);
        let _ = room.local_participant().unpublish_track(&sid).await;
    }
}

async fn session(
    address: String,
    token: String,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Command>,
    shared: Arc<Mutex<State>>,
    ctx: egui::Context,
) -> Result<()> {
    gstreamer::init()?;
    livekit::webrtc::enable_zero_playout_delay()?;
    let connection = tokio::select! {result=network::connect(&address,&token)=>result?,_ = rx.recv()=>return Ok(())};
    let (room, mut events) = connection;
    let room = Arc::new(room);
    {
        let mut s = shared.lock().unwrap();
        s.connected = true;
        s.can_publish = room
            .local_participant()
            .permission()
            .is_some_and(|grant| grant.can_publish);
        s.status = "Conectado".into();
    }
    let volume = Arc::new(AtomicU32::new(80));
    let mut layout = Layout::default();
    let mut video_sinks = HashMap::<String, Sink>::new();
    let mut audio_sinks = HashMap::<String, Sink>::new();
    let mut capture: Option<Capture> = None;
    let mut publisher: Option<EncodedPublisher> = None;
    let mut audio: Option<AudioCast> = None;
    let mut wants_audio = false;
    let mut pending: Option<tokio::task::JoinHandle<Result<(Capture, Codec)>>> = None;
    let mut current_codec = Codec::Hevc;
    let started = Instant::now();
    let mut ticks = tokio::time::interval(Duration::from_millis(8));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_refresh = Instant::now() - Duration::from_secs(1);
    let mut active = HashMap::new();
    let outcome=async {
        loop {
            tokio::select!{
                command=rx.recv()=>match command.unwrap_or(Command::Disconnect){
                    Command::Disconnect=>break,
                    Command::Layout(value)=>{layout=value;volume.store(layout.volume,Ordering::Relaxed);last_refresh=Instant::now()-Duration::from_secs(1);},
                    Command::Share(source,codec,encoder)=>{
                        if pending.is_none() && room.local_participant().permission().is_some_and(|grant|grant.can_publish){
                            pending=Some(tokio::spawn(async move{let cap=Capture::open(source,codec,encoder).await?;Ok((cap,codec))}));
                            let mut s=shared.lock().unwrap();s.selecting=true;s.status="Selecione a tela ou janela no diálogo do sistema".into();
                        }
                    },
                    Command::CancelSelection=>{
                        if let Some(task)=pending.take(){task.abort();let _=task.await;}
                        let mut s=shared.lock().unwrap();s.selecting=false;s.status=if s.sharing{"Seleção cancelada; origem anterior mantida"}else{"Seleção cancelada"}.into();
                    },
                    Command::Stop=>{
                        if let Some(task)=pending.take(){task.abort();let _=task.await;}
                        if let Some(mut cap)=capture.take(){cap.stop().await;}
                        if let Some(p)=publisher.take(){let _=p.stop().await;}
                        if let Some(a)=audio.take(){a.stop(&room).await;}
                        let mut s=shared.lock().unwrap();s.sharing=false;s.selecting=false;s.status="Compartilhamento encerrado".into();
                    },
                    Command::Audio(enabled)=>{wants_audio=enabled;if !enabled {if let Some(a)=audio.take(){a.stop(&room).await;}}},
                },
                event=events.recv()=>match event {
                    None=>break,
                    Some(RoomEvent::Disconnected{..})=>{let mut s=shared.lock().unwrap();s.connected=false;s.reconnecting=false;s.status="Conexão encerrada após as tentativas de reconexão".into();break;},
                    Some(RoomEvent::Reconnecting)=>{let mut s=shared.lock().unwrap();s.reconnecting=true;s.status="Reconectando… mantendo o último quadro".into();},
                    Some(RoomEvent::Reconnected)=>{if let Some(p)=&mut publisher{p.restart_keyframe();}if let Some(c)=&capture{c.request_keyframe();}let mut s=shared.lock().unwrap();s.reconnecting=false;s.status="Conectado".into();},
                    Some(RoomEvent::TrackSubscribed{track:RemoteTrack::Video(track),publication,..})=>{
                        let sid=publication.sid().to_string();
                        if publication.source()==TrackSource::Screenshare && video_sinks.len()<3 {
                            let mut stream=NativeVideoStream::with_options(track.rtc_track(),NativeVideoStreamOptions{queue_size_frames:Some(1)});
                            let state=shared.clone();let repaint=ctx.clone();let key=sid.clone();let mut sequence=0;
                            video_sinks.insert(sid,Sink(tokio::spawn(async move{
                                while let Some(frame)=stream.next().await {let w=frame.buffer.width();let h=frame.buffer.height();if w==0||h==0||w>3840||h>2160{continue;}
                                    sequence+=1;let mut s=state.lock().unwrap();s.received+=1;s.frames.insert(key.clone(),(sequence,Arc::new(frame)));drop(s);repaint.request_repaint();}
                            })));
                        }
                    },
                    Some(RoomEvent::TrackSubscribed{track:RemoteTrack::Audio(track),publication,..})=>{
                        if publication.source()==TrackSource::ScreenshareAudio && audio_sinks.len()<3{
                            let key=publication.sid().to_string();let gain=volume.clone();let state=shared.clone();
                            let mut stream=NativeAudioStream::with_options(track.rtc_track(),48000,2,NativeAudioStreamOptions{queue_size_frames:Some(3)});
                            audio_sinks.insert(key,Sink(tokio::spawn(async move {
                                let Ok(output)=Playout::open()else{return;};
                                while let Some(frame)=stream.next().await {if frame.sample_rate==48000&&frame.num_channels==2{let gain=if std::env::var("HYPERCAST_AUDIO_TEST_SILENT_OUTPUT").as_deref()==Ok("1"){0}else{gain.load(Ordering::Relaxed)};
                                        let mut s=state.lock().unwrap();s.audio_received+=1;drop(s);
                                        if let Err(e)=output.push(&frame.data,gain){state.lock().unwrap().status=e.to_string();break;}}}
                            })));
                        }
                    },
                    Some(RoomEvent::TrackUnsubscribed{publication,..})=>{video_sinks.remove(&publication.sid().to_string());audio_sinks.remove(&publication.sid().to_string());},
                    _=>{last_refresh=Instant::now()-Duration::from_secs(1);},
                },
                _=ticks.tick()=>{
                    if pending.as_ref().is_some_and(|t|t.is_finished()) {
                        let result=pending.take().unwrap().await?;
                        shared.lock().unwrap().selecting=false;
                        match result {
                            Ok((cap,codec))=>{if let Some(mut old)=capture.take(){old.stop().await;}current_codec=codec;cap.request_keyframe();
                                {let mut s=shared.lock().unwrap();s.sharing=true;s.status=format!("Captura Linux · {} · até {} FPS",cap.backend,cap.fps);}capture=Some(cap);
                                if let Some(p)=&mut publisher{p.restart_keyframe();}
                            },
                            Err(error)=>{shared.lock().unwrap().status=error.to_string();},
                        }
                    }
                    if room.connection_state()==ConnectionState::Connected && last_refresh.elapsed()>Duration::from_millis(250){
                        active=refresh(&room,&layout,&shared,audio.is_some());video_sinks.retain(|sid,_|active.contains_key(sid));
                        if audio.is_some()||layout.paused||layout.volume==0{audio_sinks.clear();}
                        last_refresh=Instant::now();
                    }
                    if let Some(cap)=&mut capture {
                        match cap.poll(){
                            Ok(Some(packet))=>{
                                if publisher.is_none(){
                                    if !packet.keyframe{cap.request_keyframe();continue;}
                                    match EncodedPublisher::in_room(room.clone(),packet.width,packet.height,cap.fps,if cap.fps==30{12_000_000}else{40_000_000},current_codec==Codec::Av1).await{
                                        Ok(p)=>{publisher=Some(p);shared.lock().unwrap().sharing=true;},
                                        Err(error)=>{shared.lock().unwrap().status=format!("Publicação rejeitada: {error}");cap.stop().await;capture=None;shared.lock().unwrap().sharing=false;continue;}
                                    }
                                }
                                let p=publisher.as_mut().unwrap();
                                if p.take_keyframe_request(){cap.request_keyframe();if !packet.keyframe{continue;}}
                                p.set_resolution_if_changed(packet.width,packet.height);
                                if let Some(rate)=p.take_rate_control_request(){cap.bitrate(rate.target_bitrate_bps as u32);}
                                if p.push(&packet.bytes,started.elapsed().as_micros() as i64,packet.keyframe)?{shared.lock().unwrap().sent+=1;}
                            },
                            Ok(None)=>{},
                            Err(error)=>{shared.lock().unwrap().status=error.to_string();cap.stop().await;capture=None;if let Some(p)=publisher.take(){let _=p.stop().await;}if let Some(a)=audio.take(){a.stop(&room).await;}
                                shared.lock().unwrap().sharing=false;},
                        }
                    }
                    if wants_audio && publisher.is_some()&&audio.is_none(){
                        volume.store(0,Ordering::Relaxed);audio_sinks.clear();
                        active=refresh(&room,&layout,&shared,true);
                        match AudioCast::open(&room).await{Ok(a)=>{audio=Some(a);volume.store(0,Ordering::Relaxed);audio_sinks.clear();},Err(e)=>{shared.lock().unwrap().status=e.to_string();wants_audio=false;}}
                    }
                    if let Some(a)=&mut audio {if let Err(e)=a.tick().await{shared.lock().unwrap().status=e.to_string();if let Some(a)=audio.take(){a.stop(&room).await;}wants_audio=false;}}
                    if audio.is_none(){volume.store(layout.volume,Ordering::Relaxed);}
                }
            }
            ctx.request_repaint();
        }
        Ok::<(),anyhow::Error>(())
    }.await;
    if let Some(task) = pending.take() {
        task.abort();
        let _ = task.await;
    }
    if let Some(mut cap) = capture.take() {
        cap.stop().await;
    }
    if let Some(a) = audio.take() {
        a.stop(&room).await;
    }
    if let Some(p) = publisher.take() {
        let _ = p.stop().await;
    }
    video_sinks.clear();
    audio_sinks.clear();
    let _ = room.close().await;
    drop(room);
    {
        let mut s = shared.lock().unwrap();
        s.connected = false;
        s.sharing = false;
        s.selecting = false;
        s.frames.clear();
        s.screens.clear();
        s.participants.clear();
    }
    ctx.request_repaint();
    outcome
}
