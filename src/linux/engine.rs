use crate::resilience::StateMutex as Mutex;
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
        Arc,
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
#[derive(Default,Clone)]
pub struct State {
    pub connected: bool,
    pub quality:crate::video_metrics::QualitySnapshot,
    pub reception_resetting: bool,
    pub reception_restarts: u64,
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
    #[cfg(test)]
    FailWorker,
    #[cfg(test)]
    FailCapture,
    RestartReception,
    StreamSettings(crate::resolution::StreamSettings),
    Layout(Layout),
    Share(Source, Codec, Encoder,crate::bandwidth::UploadProfile),
    Stop,
    CancelSelection,
    Audio(bool),
    Disconnect,
}
pub struct Engine {
    pub state: Arc<Mutex<State>>,
    pub tx: tokio::sync::mpsc::UnboundedSender<Command>,
    thread: Option<std::thread::JoinHandle<()>>,
    cancel:crate::room_transition::Cancellation,
}
impl Engine {
    #[cfg(test)]
    pub fn fixture(state:Arc<Mutex<State>>)->Self {let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();Self{state,tx,thread:None,cancel:Default::default()}}

    pub fn finished(&self)->bool{self.thread.as_ref().is_none_or(|t|t.is_finished())}
    pub fn request_exit(&self){self.cancel.cancel();}
    pub fn start(address: String, token: String, ctx: egui::Context) -> Self {
        let state = Arc::new(Mutex::new(State {
            status: crate::i18n::label("Conectando…","Connecting…").into(),
            ..Default::default()
        }));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let shared = state.clone();
        let cancel=crate::room_transition::Cancellation::default();let stopping=cancel.clone();
        let failure_state=state.clone();let failure_ctx=ctx.clone();
        let thread = std::thread::spawn(move || {
            let healthy=crate::resilience::guard(|| {
            let Ok(runtime)=tokio::runtime::Runtime::new() else {shared.lock().unwrap().status=crate::resilience::worker_failure().into();ctx.request_repaint();return;};
            let result = runtime.block_on(session(address, token, rx, shared.clone(), ctx.clone(),stopping));
            if let Err(error) = result {
                let mut s = shared.lock().unwrap();
                s.status = error.to_string();
                s.connected = false;
                s.sharing = false;s.selecting=false;s.reconnecting=false;s.reception_resetting=false;s.can_publish=false;
            }
            ctx.request_repaint();
            runtime.shutdown_timeout(Duration::from_secs(2));
            });
            if !healthy {let mut s=failure_state.lock().unwrap();*s=State::default();s.status=crate::resilience::worker_failure().into();drop(s);failure_ctx.request_repaint();}
        });
        Self {
            state,
            tx, cancel,
            thread: Some(thread),
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.cancel.cancel();
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

fn attach_video(track:RemoteVideoTrack,sid:&str,shared:&Arc<Mutex<State>>,ctx:&egui::Context)->Sink {
    let enabled_track=track.rtc_track();
    let mut stream=NativeVideoStream::with_options(enabled_track.clone(),NativeVideoStreamOptions{queue_size_frames:Some(1)});
    let state=shared.clone();let repaint=ctx.clone();let key=sid.to_owned();
    Sink(tokio::spawn(async move{
        while let Some(frame)=stream.next().await {if !enabled_track.enabled(){continue;}let w=frame.buffer.width();let h=frame.buffer.height();if !crate::resolution::valid(w,h){continue;}
            let mut s=state.lock().unwrap();s.received+=1;let sequence=s.received;s.frames.insert(key.clone(),(sequence,Arc::new(frame)));drop(s);repaint.request_repaint();}
    }))
}
fn attach_audio(track:RemoteAudioTrack,volume:&Arc<AtomicU32>,shared:&Arc<Mutex<State>>)->Sink {
    let gain=volume.clone();let state=shared.clone();
    let mut stream=NativeAudioStream::with_options(track.rtc_track(),48000,2,NativeAudioStreamOptions{queue_size_frames:Some(3)});
    Sink(tokio::spawn(async move {
        let Ok(output)=Playout::open()else{return;};
        while let Some(frame)=stream.next().await {if frame.sample_rate==48000&&frame.num_channels==2{let gain=if std::env::var("HYPERCAST_AUDIO_TEST_SILENT_OUTPUT").as_deref()==Ok("1"){0}else{gain.load(Ordering::Relaxed)};
                let mut s=state.lock().unwrap();s.audio_received+=1;drop(s);
                if let Err(e)=output.push(&frame.data,gain){state.lock().unwrap().status=e.to_string();break;}}}
    }))
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
        crate::subscription::keep_receiver(publication,enabled);
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
            crate::subscription::keep_receiver(&publication,enabled);
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
    stopping:crate::room_transition::Cancellation,
) -> Result<()> {
    gstreamer::init()?;
    livekit::webrtc::enable_zero_playout_delay()?;
    let connection = match stopping.run(network::connect_desktop(&address,&token,std::env::var("HYPERCAST_RELAY_ONLY").as_deref()==Ok("1"),|attempt,_|{shared.lock().unwrap().status=crate::i18n_format!("Tentando conectar novamente… {attempt}/3","Retrying connection… {attempt}/3");ctx.request_repaint();})).await {Some(result)=>result?,None=>return Ok(())};
    let (room, mut events) = connection;
    let mut room = Arc::new(room);
    {
        let mut s = shared.lock().unwrap();
        s.connected = true;
        s.can_publish = room
            .local_participant()
            .permission()
            .is_some_and(|grant| grant.can_publish);
        s.status = crate::i18n::label("Conectado","Connected").into();
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
    let mut ever_published=false;
    let mut settings=crate::resolution::StreamSettings::default();
    let mut publication_format:Option<(Codec,u32,u32)>=None;
    let mut last_video_bps:Option<u64>=None;
    let mut retiring:Option<tokio::task::JoinHandle<()>>=None;
    let mut transmit_paused=false;
    let started = Instant::now();
    let mut ticks = tokio::time::interval(Duration::from_millis(8));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_refresh = Instant::now() - Duration::from_secs(1);
    let mut reception_restart:Option<Instant>=None;
    let(mut send_meter,mut receive_meter)=(crate::video_metrics::TrafficMeter::default(),crate::video_metrics::TrafficMeter::default());
    let mut sent_bytes=0u64;
    let outcome=async {
        loop {
            anyhow::ensure!(!shared.was_reset(),crate::resilience::worker_failure());
            tokio::select!{
                biased;
                _=stopping.cancelled()=>break,
                command=rx.recv()=>match command.unwrap_or(Command::Disconnect){
                    #[cfg(test)] Command::FailWorker=>panic!("injected worker failure"),
                    #[cfg(test)] Command::FailCapture=>{pending=Some(tokio::spawn(async{panic!("injected capture task failure")}));shared.lock().unwrap().selecting=true;},
                    Command::Disconnect=>break,
                    Command::StreamSettings(value)=>{if capture.is_none()&&pending.is_none()&&value.valid(){settings=value;}},
                    Command::RestartReception=>{
                        if reception_restart.is_none() && room.connection_state()==ConnectionState::Connected {
                            for (_,mut sink) in video_sinks.drain(){sink.0.abort();let _=(&mut sink.0).await;}
                            for (_,mut sink) in audio_sinks.drain(){sink.0.abort();let _=(&mut sink.0).await;}
                            crate::subscription::reset_receivers(&room);
                            reception_restart=Some(Instant::now()+Duration::from_millis(700));
                            let mut s=shared.lock().unwrap();s.reception_resetting=true;s.reception_restarts+=1;
                        }
                    },
                    Command::Layout(value)=>{layout=value;volume.store(layout.volume,Ordering::Relaxed);last_refresh=Instant::now()-Duration::from_secs(1);},
                    Command::Share(source,codec,encoder,upload)=>{
                        if pending.is_none() && retiring.is_none() && room.local_participant().permission().is_some_and(|grant|grant.can_publish){
                            pending=Some(tokio::spawn(async move{let cap=Capture::open_with_settings(source,codec,encoder,upload,settings).await?;Ok((cap,codec))}));
                            let mut s=shared.lock().unwrap();s.selecting=true;s.status=crate::i18n::label("Selecione a tela ou janela no diálogo do sistema","Select a screen or window in the system dialog").into();
                        }
                    },
                    Command::CancelSelection=>{
                        if let Some(task)=pending.take(){task.abort();let _=task.await;}
                        let mut s=shared.lock().unwrap();s.selecting=false;s.status=if s.sharing{crate::i18n::label("Seleção cancelada; origem anterior mantida","Selection cancelled; previous source kept")}else{crate::i18n::label("Seleção cancelada","Selection cancelled")}.into();
                    },
                    Command::Stop=>{
                        if let Some(task)=pending.take(){task.abort();let _=task.await;}
                        if let Some(mut cap)=capture.take(){cap.stop().await;}
                        if let Some(p)=publisher.take(){let _=p.stop().await;}
                        publication_format=None;last_video_bps=None;transmit_paused=false;
                        if let Some(a)=audio.take(){a.stop(&room).await;}
                        let mut s=shared.lock().unwrap();s.sharing=false;s.selecting=false;s.status="Compartilhamento encerrado".into();
                    },
                    Command::Audio(enabled)=>{wants_audio=enabled;if !enabled {if let Some(a)=audio.take(){a.stop(&room).await;}}},
                },
                event=events.recv()=>match event {
                    None=>break,
                    Some(RoomEvent::Disconnected{..})=>{let mut s=shared.lock().unwrap();s.connected=false;s.reconnecting=false;s.status=crate::i18n::label("Conexão encerrada após as tentativas de reconexão","Connection ended after the reconnection attempts").into();break;},
                    Some(RoomEvent::Reconnecting)=>{let mut s=shared.lock().unwrap();s.reconnecting=true;s.status=crate::i18n::label("Reconectando… mantendo o último quadro","Reconnecting… keeping the last frame").into();},
                    Some(RoomEvent::Reconnected)=>{if let Some(p)=&mut publisher{p.restart_keyframe();}if let Some(c)=&capture{c.request_keyframe();}let mut s=shared.lock().unwrap();s.reconnecting=false;s.status=crate::i18n::label("Conectado","Connected").into();},
                    Some(RoomEvent::TrackSubscribed{track:RemoteTrack::Video(_),publication,..})=>{
                        if reception_restart.is_some() || !publication.is_desired(){continue;}
                        let Some(RemoteTrack::Video(track))=publication.track() else{continue;};
                        let sid=publication.sid().to_string();
                        eprintln!("Linux reception: stage=subscribed sid={sid}");
                        if publication.source()==TrackSource::Screenshare && (video_sinks.contains_key(&sid)||video_sinks.len()<3) {
                            video_sinks.insert(sid.clone(),attach_video(track,&sid,&shared,&ctx));
                        }
                    },
                    Some(RoomEvent::TrackSubscribed{track:RemoteTrack::Audio(_),publication,..})=>{
                        if reception_restart.is_some() || !publication.is_desired(){continue;}
                        let Some(RemoteTrack::Audio(track))=publication.track() else{continue;};
                        if publication.source()==TrackSource::ScreenshareAudio && !layout.paused && layout.volume>0 && audio.is_none() && (audio_sinks.contains_key(&publication.sid().to_string())||audio_sinks.len()<3){
                            audio_sinks.insert(publication.sid().to_string(),attach_audio(track,&volume,&shared));
                        }
                    },
                    Some(RoomEvent::TrackUnsubscribed{publication,..})=>{if publication.is_subscribed(){continue;}video_sinks.remove(&publication.sid().to_string());audio_sinks.remove(&publication.sid().to_string());},
                    _=>{last_refresh=Instant::now()-Duration::from_secs(1);},
                },
                _=ticks.tick()=>{
                    if retiring.as_ref().is_some_and(|task|task.is_finished()) {
                        if retiring.take().unwrap().await.is_err(){shared.lock().unwrap().status=crate::i18n::label("A origem anterior falhou ao encerrar; você pode tentar novamente.","The previous source failed to stop; you can try again.").into();}
                        shared.lock().unwrap().selecting=pending.is_some();
                    }
                    if pending.as_ref().is_some_and(|t|t.is_finished()) {
                        let result=pending.take().unwrap().await.unwrap_or_else(|_|Err(anyhow::anyhow!(crate::i18n::label("A captura falhou internamente. Selecione a origem para tentar novamente.","Capture failed internally. Select the source to try again."))));
                        shared.lock().unwrap().selecting=false;
                        match result {
                            Ok((cap,codec))=>{
                                let next_format=(codec,cap.fps,cap.bitrate_limit);
                                if (ever_published && current_codec!=codec) || publication_format.is_some_and(|previous|previous!=next_format) {
                                    eprintln!("Linux publish: stage=reconfigure codec={codec:?} fps={} bitrate={}",cap.fps,cap.bitrate_limit);
                                    // A fresh Linux media connection avoids retaining the previous
                                    // codec/transport after the sole video transceiver is stopped.
                                    {let mut s=shared.lock().unwrap();s.reconnecting=true;s.status=crate::i18n::label("Reconectando para alterar o formato da transmissão…","Reconnecting to change the stream format…").into();}
                                    if let Some(a)=audio.take(){a.stop(&room).await;}
                                    if let Some(p)=publisher.take(){let _=p.stop().await;}
                                    for (_,mut sink) in video_sinks.drain(){sink.0.abort();let _=(&mut sink.0).await;}
                                    for (_,mut sink) in audio_sinks.drain(){sink.0.abort();let _=(&mut sink.0).await;}
                                    let current_token=room.token();
                                    room.close().await?;
                                    let (next_room,next_events)=match stopping.run(network::connect(&address,&current_token)).await{Some(result)=>result?,None=>break};
                                    room=Arc::new(next_room);events=next_events;
                                    reception_restart=None;last_refresh=Instant::now()-Duration::from_secs(1);
                                    {let mut s=shared.lock().unwrap();s.reconnecting=false;s.reception_resetting=false;}
                                    publication_format=None;last_video_bps=None;transmit_paused=false;
                                }
                                if let Some(bps)=last_video_bps.filter(|bps|*bps>0){cap.bitrate(bps.min(cap.bitrate_limit as u64) as u32);}
                                current_codec=codec;
                                {let mut s=shared.lock().unwrap();s.sharing=true;s.status=crate::i18n_format!("Captura Linux · {} · até {} FPS","Linux capture · {} · up to {} FPS",cap.backend,cap.fps);s.quality.send=None;}
                                if let Some(mut old)=capture.replace(cap) {
                                    retiring=Some(tokio::spawn(async move{
                                        eprintln!("Linux capture: stage=retire_previous_start");
                                        old.stop().await;
                                        eprintln!("Linux capture: stage=retire_previous_done");
                                    }));
                                }
                                shared.lock().unwrap().selecting=retiring.is_some();
                                send_meter=Default::default();
                                eprintln!("Linux capture: stage=source_switched codec={codec:?}");
                                if let Some(p)=&mut publisher{p.restart_keyframe();}
                            },
                            Err(error)=>{shared.lock().unwrap().status=error.to_string();},
                        }
                    }
                    if reception_restart.is_some_and(|until|Instant::now()>=until){
                        reception_restart=None;shared.lock().unwrap().reception_resetting=false;
                        last_refresh=Instant::now()-Duration::from_secs(1);
                    }
                    if reception_restart.is_none() && room.connection_state()==ConnectionState::Connected && last_refresh.elapsed()>Duration::from_millis(250){
                        let _=refresh(&room,&layout,&shared,audio.is_some());
                        for participant in room.remote_participants().into_values(){
                            for p in participant.track_publications().into_values(){
                                if !p.is_desired(){continue;}
                                let sid=p.sid().to_string();
                                match p.track(){
                                    Some(RemoteTrack::Video(track)) if p.source()==TrackSource::Screenshare && video_sinks.get(&sid).is_none_or(|s|s.0.is_finished()) && (video_sinks.contains_key(&sid)||video_sinks.len()<3)=>{
                                        video_sinks.insert(sid.clone(),attach_video(track,&sid,&shared,&ctx));
                                    },
                                    Some(RemoteTrack::Audio(track)) if p.source()==TrackSource::ScreenshareAudio && p.is_enabled() && audio.is_none() && !layout.paused && layout.volume>0 && !audio_sinks.contains_key(&sid) && audio_sinks.len()<3=>{
                                        audio_sinks.insert(sid,attach_audio(track,&volume,&shared));
                                    },
                                    _=>{}
                                }
                            }
                        }
                        let active_audio:Vec<_>=room.remote_participants().into_values().flat_map(|p|p.track_publications().into_values())
                            .filter(|p|p.source()==TrackSource::ScreenshareAudio && p.is_desired() && p.is_enabled()).map(|p|p.sid().to_string()).collect();
                        audio_sinks.retain(|sid,_|active_audio.contains(sid));
                        if audio.is_some()||layout.paused||layout.volume==0{audio_sinks.clear();}
                        last_refresh=Instant::now();
                    }
                    if let Some(cap)=&mut capture {
                        if let Some(p)=&mut publisher {
                            if let Some(rate)=p.take_rate_control_request(){
                                if last_video_bps!=Some(rate.target_bitrate_bps){eprintln!("Linux publish rate: target_bps={} paused={}",rate.target_bitrate_bps,rate.target_bitrate_bps==0);}
                                last_video_bps=Some(rate.target_bitrate_bps);
                                let was_paused=transmit_paused;transmit_paused=rate.target_bitrate_bps==0;
                                if !transmit_paused {cap.bitrate(rate.target_bitrate_bps.min(cap.bitrate_limit as u64) as u32);}
                                if was_paused&&!transmit_paused{p.restart_keyframe();cap.request_keyframe();}
                            }
                        }else{transmit_paused=false;}
                        match cap.poll(){
                            Ok(Some(_)) if transmit_paused=>{},
                            Ok(Some(packet))=>{
                                if publisher.is_none(){
                                    if !packet.keyframe{cap.request_keyframe();continue;}
                                    match EncodedPublisher::in_room(room.clone(),packet.width,packet.height,cap.fps,cap.bitrate_limit,current_codec==Codec::Av1).await{
                                        Ok(p)=>{ever_published=true;publisher=Some(p);publication_format=Some((current_codec,cap.fps,cap.bitrate_limit));shared.lock().unwrap().sharing=true;},
                                        Err(error)=>{shared.lock().unwrap().status=crate::i18n_format!("Publicação rejeitada: {error}","Publication rejected: {error}");cap.stop().await;capture=None;shared.lock().unwrap().sharing=false;continue;}
                                    }
                                }
                                let p=publisher.as_mut().unwrap();
                                p.set_resolution_if_changed(packet.width,packet.height);
                                if p.take_keyframe_request(){cap.request_keyframe();if !packet.keyframe{continue;}}
                                match p.push(&packet.bytes,started.elapsed().as_micros() as i64,packet.keyframe){
                                    Ok(true)=>{let mut state=shared.lock().unwrap();state.sent+=1;sent_bytes+=packet.bytes.len() as u64;state.quality.resolution=Some((packet.width,packet.height));},
                                    Ok(false)=>{},
                                    Err(error)=>{
                                        cap.stop().await;capture=None;
                                        if let Some(p)=publisher.take(){let _=p.stop().await;}
                                        if let Some(a)=audio.take(){a.stop(&room).await;}
                                        publication_format=None;last_video_bps=None;transmit_paused=false;
                                        let mut s=shared.lock().unwrap();s.sharing=false;
                                        s.status=crate::i18n_format!("Envio interrompido: {error}. Tente compartilhar novamente.","Sending stopped: {error}. Try sharing again.");
                                    }
                                }
                            },
                            Ok(None)=>{},
                            Err(error)=>{shared.lock().unwrap().status=error.to_string();cap.stop().await;capture=None;if let Some(p)=publisher.take(){let _=p.stop().await;}if let Some(a)=audio.take(){a.stop(&room).await;}
                                publication_format=None;last_video_bps=None;transmit_paused=false;
                                shared.lock().unwrap().sharing=false;},
                        }
                    }
                    if wants_audio && publisher.is_some()&&audio.is_none(){
                        volume.store(0,Ordering::Relaxed);audio_sinks.clear();
                        if reception_restart.is_none(){let _=refresh(&room,&layout,&shared,true);}
                        match AudioCast::open(&room).await{Ok(a)=>{audio=Some(a);volume.store(0,Ordering::Relaxed);audio_sinks.clear();},Err(e)=>{shared.lock().unwrap().status=e.to_string();wants_audio=false;}}
                    }
                    if let Some(a)=&mut audio {if let Err(e)=a.tick().await{shared.lock().unwrap().status=e.to_string();if let Some(a)=audio.take(){a.stop(&room).await;}wants_audio=false;}}
                    if audio.is_none(){volume.store(layout.volume,Ordering::Relaxed);}
                }
            }
            {
                let now=Instant::now();let mut state=shared.lock().unwrap();
                if capture.is_some(){if let Some(sample)=send_meter.sample(now,state.sent,sent_bytes){state.quality.send=Some(sample);}}
                else{send_meter=Default::default();state.quality.send=None;state.quality.resolution=None;}
                state.quality.send_paused=capture.is_some()&&transmit_paused;
                if let Some(sample)=receive_meter.sample(now,state.received,0){state.quality.receive=Some(sample);}
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
    if let Some(task)=retiring.take(){let _=task.await;}
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

#[cfg(test)]
mod reception_tests {
    use super::*;
    #[test]
    #[ignore="Needs two authorized tokens for one isolated room and an NVENC GPU"]
    fn publisher_source_and_codec_switches()->Result<()> {
        let address=std::env::var("HYPERCAST_URL")?;
        let publisher=Engine::start(address.clone(),std::env::var("HYPERCAST_PUBLISH_TOKEN")?,egui::Context::default());
        let viewer=Engine::start(address,std::env::var("HYPERCAST_ROOM_TOKEN")?,egui::Context::default());
        let deadline=Instant::now()+Duration::from_secs(15);
        while !publisher.state.lock().unwrap().connected || !viewer.state.lock().unwrap().connected {
            anyhow::ensure!(Instant::now()<deadline,"Switch test clients did not connect");
            std::thread::sleep(Duration::from_millis(25));
        }
        publisher.tx.send(Command::FailCapture)?;
        let until=Instant::now()+Duration::from_secs(5);
        loop {
            let s=publisher.state.lock().unwrap();
            if !s.selecting && s.status==crate::i18n::label("A captura falhou internamente. Selecione a origem para tentar novamente.","Capture failed internally. Select the source to try again."){anyhow::ensure!(s.connected,"Capture failure disconnected the room");break;}
            anyhow::ensure!(Instant::now()<until,"Capture task failure did not become retryable");drop(s);std::thread::sleep(Duration::from_millis(10));
        }
        let mut previous_sid=None;
        for (step,(w,h,codec,encoder)) in [
            (640,360,Codec::Av1,Encoder::Auto),
            (800,600,Codec::Av1,Encoder::Auto),
            (640,360,Codec::Hevc,Encoder::Auto),
            (800,600,Codec::Hevc,Encoder::Software),
            (640,360,Codec::Hevc,Encoder::Auto),
            (640,360,Codec::Av1,Encoder::Auto),
        ].into_iter().enumerate() {
            if step>=3 {
                publisher.tx.send(Command::Stop)?;
                let until=Instant::now()+Duration::from_secs(5);
                while publisher.state.lock().unwrap().sharing{anyhow::ensure!(Instant::now()<until,"Stop did not finish");std::thread::sleep(Duration::from_millis(10));}
            }
            let before=viewer.state.lock().unwrap().received;
            publisher.tx.send(Command::Share(Source::Synthetic(w,h),codec,encoder,crate::bandwidth::UploadProfile::Economic))?;
            let deadline=Instant::now()+Duration::from_secs(15);
            let sid=loop {
                let ready={let s=publisher.state.lock().unwrap();s.sharing&&!s.selecting};
                let fresh={let s=viewer.state.lock().unwrap();s.screens.iter().find_map(|screen|s.frames.get(&screen.sid).filter(|_|step<2 || previous_sid.as_ref()!=Some(&screen.sid)).filter(|(sequence,frame)|*sequence>before+3 && frame.buffer.width()==w && frame.buffer.height()==h).map(|_|screen.sid.clone()))};
                if ready{if let Some(sid)=fresh{break sid;}}
                if Instant::now()>=deadline {let p=publisher.state.lock().unwrap();let v=viewer.state.lock().unwrap();anyhow::bail!("Switch {step} did not deliver fresh {w}x{h} frames: publisher={}, sent={}, viewer={}, received={}, screens={}",p.status,p.sent,v.status,v.received,v.screens.len());}
                std::thread::sleep(Duration::from_millis(25));
            };
            if let Some(previous)=previous_sid {
                if step==1{anyhow::ensure!(sid==previous,"Compatible source change replaced the publication");}
                else{anyhow::ensure!(sid!=previous,"Codec/FPS change kept an incompatible publication");}
            }
            previous_sid=Some(sid);
            eprintln!("PASS: switch {step}: {w}x{h} {codec:?} {encoder:?}");
        }
        drop(publisher);drop(viewer);
        anyhow::ensure!(super::super::media::decoder_count()==0,"Switch test retained decoders");
        Ok(())
    }
    #[test]
    #[ignore="Needs an isolated authorized room with two synthetic publishers"]
    fn receiver_switch_and_refresh_cycles()->Result<()> {
        let address=std::env::var("HYPERCAST_URL")?;
        let token=std::env::var("HYPERCAST_ROOM_TOKEN")?;
        let engine=Engine::start(address,token,egui::Context::default());
        let deadline=Instant::now()+Duration::from_secs(15);
        while engine.state.lock().unwrap().screens.len()<2 {
            anyhow::ensure!(Instant::now()<deadline,"Synthetic publishers did not become ready");
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut layout=Layout::default();
        for cycle in 0..3 {
            for step in 0..20 {
                layout.focus=Some(format!("hc-screen-{}",step%2+1));
                engine.tx.send(Command::Layout(layout.clone()))?;
                std::thread::sleep(Duration::from_millis(60));
            }
            let owner=format!("hc-screen-{}",cycle%2+1);
            layout.focus=Some(owner.clone());engine.tx.send(Command::Layout(layout.clone()))?;
            std::thread::sleep(Duration::from_millis(300));
            let sequence=|engine:&Engine|{
                let state=engine.state.lock().unwrap();
                state.screens.iter().find(|s|s.owner==owner).and_then(|s|state.frames.get(&s.sid)).map(|f|f.0).unwrap_or(0)
            };
            let before=sequence(&engine);engine.tx.send(Command::RestartReception)?;
            let deadline=Instant::now()+Duration::from_secs(10);
            loop {
                let ready={let state=engine.state.lock().unwrap();state.connected && !state.reception_resetting && state.reception_restarts==cycle+1};
                if ready && sequence(&engine)>before {break;}
                anyhow::ensure!(Instant::now()<deadline,"Receive refresh did not produce a fresh selected frame");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        drop(engine);
        anyhow::ensure!(super::super::media::decoder_count()==0,"Receive shutdown retained decoders");
        Ok(())
    }
}
