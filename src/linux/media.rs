use anyhow::{Context, Result, ensure};
use ashpd::desktop::{
    Session,
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
};
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSink, AppSrc};
use std::{
    os::fd::{AsRawFd, OwnedFd},
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    Hevc,
    Av1,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoder {
    Auto,
    Software,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Portal,
    X11,
    Test,
}

fn enum_property(element: &gst::Element, name: &str, value: &str) -> bool {
    let Some(property) = element.find_property(name) else {
        return false;
    };
    let Some(class) = gst::glib::EnumClass::with_type(property.value_type()) else {
        return false;
    };
    if class.value_by_nick(value).is_none() {
        return false;
    }
    element.set_property_from_str(name, value);
    true
}

struct PortalLease(Option<Session<Screencast>>);
impl PortalLease {
    async fn close(&mut self) {
        if let Some(session) = self.0.take() {
            let _ = session.close().await;
        }
    }
}
impl Drop for PortalLease {
    fn drop(&mut self) {
        if let Some(session) = self.0.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = session.close().await;
                });
            }
        }
    }
}

pub struct Packet {
    pub bytes: Vec<u8>,
    pub timestamp_us: i64,
    pub keyframe: bool,
    pub width: u32,
    pub height: u32,
}
pub struct Capture {
    pipeline: gst::Pipeline,
    sink: AppSink,
    encoder: gst::Element,
    lease: Option<PortalLease>,
    _fd: Option<OwnedFd>,
    pub backend: String,
    pub fps: u32,
    pub bitrate_limit:u32,
    ready: Option<Packet>,
}
impl Capture {
    pub async fn open(source: Source, codec: Codec, encoder: Encoder) -> Result<Self> {
        Self::open_with_profile(source,codec,encoder,crate::bandwidth::UploadProfile::Maximum).await
    }
    pub async fn open_with_profile(source:Source,codec:Codec,encoder:Encoder,upload:crate::bandwidth::UploadProfile)->Result<Self>{
        eprintln!("Linux capture: stage=init source={source:?} codec={codec:?} encoder={encoder:?}");
        gst::init()?;
        let (description, lease, fd) = match source {
            Source::Portal => {
                eprintln!("Linux capture: stage=portal_connect");
                let portal=Screencast::new().await.context("Portal de captura indisponível; confira xdg-desktop-portal e o backend do desktop")?;
                let mut lease = PortalLease(Some(portal.create_session(Default::default()).await?));
                let setup = async {
                    let sources = portal.available_source_types().await?;
                    let requested = (SourceType::Monitor | SourceType::Window) & sources;
                    ensure!(
                        !requested.is_empty(),
                        "Este portal não oferece captura de tela/janela"
                    );
                    let cursor = if portal
                        .available_cursor_modes()
                        .await?
                        .contains(CursorMode::Embedded)
                    {
                        CursorMode::Embedded
                    } else {
                        CursorMode::Hidden
                    };
                    portal
                        .select_sources(
                            lease.0.as_ref().unwrap(),
                            SelectSourcesOptions::default()
                                .set_sources(requested)
                                .set_multiple(false)
                                .set_cursor_mode(cursor),
                        )
                        .await?
                        .response()?;
                    eprintln!("Linux capture: stage=portal_select");
                    let response = portal
                        .start(lease.0.as_ref().unwrap(), None, Default::default())
                        .await?
                        .response()?;
                    eprintln!("Linux capture: stage=portal_selected");
                    let node = response
                        .streams()
                        .first()
                        .context("Nenhuma origem selecionada")?
                        .pipe_wire_node_id();
                    let fd = portal
                        .open_pipe_wire_remote(lease.0.as_ref().unwrap(), Default::default())
                        .await?;
                    eprintln!("Linux capture: stage=pipewire_remote_open");
                    Ok::<_, anyhow::Error>((node, fd))
                }
                .await;
                match setup {
                    Ok((node, fd)) => (
                        format!(
                            "pipewiresrc fd={} path={} do-timestamp=true",
                            fd.as_raw_fd(),
                            node
                        ),
                        Some(lease),
                        Some(fd),
                    ),
                    Err(e) => {
                        lease.close().await;
                        return Err(e);
                    }
                }
            }
            Source::X11 => {
                ensure!(
                    std::env::var("WAYLAND_DISPLAY").is_err(),
                    "No Wayland, escolha o seletor do sistema"
                );
                ensure!(std::env::var("DISPLAY").is_ok(), "Sessão X11 indisponível");
                (
                    "ximagesrc use-damage=false show-pointer=true do-timestamp=true".into(),
                    None,
                    None,
                )
            }
            Source::Test => {
                let size=std::env::var("HYPERCAST_LINUX_TEST_SIZE").unwrap_or_else(|_|"640x360".into());
                let (w,h)=size.split_once('x').context("Invalid synthetic size")?;
                let (w,h)=(w.parse::<u32>()?,h.parse::<u32>()?);
                ensure!(crate::resolution::valid(w,h),"Invalid synthetic size");
                (format!("videotestsrc is-live=true pattern=ball ! video/x-raw,width={w},height={h}"),None,None)
            },
        };
        Self::open_pipeline(description,lease,fd,codec,encoder,upload).await
    }
    async fn open_pipeline(description:String,lease:Option<PortalLease>,fd:Option<OwnedFd>,codec:Codec,encoder:Encoder,upload:crate::bandwidth::UploadProfile)->Result<Self>{
        let fps = if encoder == Encoder::Software {
            30
        } else {
            120
        };
        let bitrate_limit=if encoder==Encoder::Software{upload.bps().min(12_000_000)}else{upload.bps()};
        let kbps=bitrate_limit/1000;
        eprintln!("Linux capture: stage=encoder_discovery");
        let chain=match (encoder,codec) {
            (Encoder::Software,Codec::Hevc)=>"x265enc name=encoder tune=zerolatency speed-preset=ultrafast bitrate=12000 key-int-max=30 option-string=bframes=0:rc-lookahead=0:pools=1:frame-threads=1".to_string(),
            (Encoder::Software,Codec::Av1)=>anyhow::bail!("O modo compatível usa HEVC; AV1 exige encoder de hardware"),
            (Encoder::Auto,codec)=>{
                let candidates=if codec==Codec::Av1{["nvav1enc","nvcudaav1enc"]}else{["nvcudah265enc","nvh265enc"]};
                let nvidia=candidates.into_iter().find(|name|gst::ElementFactory::find(name).is_some());
                let va=if codec==Codec::Av1{"vaav1enc"}else{"vah265enc"};
                if let Some(nvidia)=nvidia{
                    format!("{nvidia} name=encoder bitrate={kbps} gop-size=120 rc-mode=cbr")
                }else if gst::ElementFactory::find(va).is_some(){
                    format!("{va} name=encoder bitrate={kbps} key-int-max=120 rate-control=cbr")
                }else{anyhow::bail!("Encoder de hardware indisponível. Instale os plugins nvcodec/VA-API ou selecione HEVC compatível (30 FPS)");}
            }
        };
        let raw_format = if encoder == Encoder::Software {
            "I420"
        } else {
            "NV12"
        };
        let raw="video/x-raw";
        let parser = if codec == Codec::Av1 {
            "av1parse ! video/x-av1,stream-format=obu-stream,alignment=tu"
        } else {
            "h265parse config-interval=-1 ! video/x-h265,stream-format=byte-stream,alignment=au"
        };
        // Only encoded access units enter Rust RAM. The initial portable source
        // conversion can negotiate CPU buffers; DMA-BUF zero-copy is not guaranteed.
        eprintln!("Linux capture: stage=pipeline_create backend={}",chain.split_whitespace().next().unwrap_or("encoder"));
        let pipeline=gst::parse::launch(&format!("{description} ! queue max-size-buffers=1 max-size-bytes=0 max-size-time=0 leaky=downstream ! identity name=source_bounds ! videorate drop-only=true max-rate={fps} ! videoconvertscale ! capsfilter name=scaled ! {raw},format={raw_format},framerate={fps}/1 ! {chain} ! {parser} ! appsink name=encoded sync=false max-buffers=2 drop=true"))?
            .downcast::<gst::Pipeline>().map_err(|_|anyhow::anyhow!("Invalid capture pipeline"))?;
        eprintln!("Linux capture: stage=pipeline_created");
        // KWin screen casts offer only 2..4 buffers; pipewiresrc 1.0.5
        // defaults to a minimum of eight. Copy before downstream queues keep
        // references so a small compositor pool can recycle immediately.
        for source in pipeline.iterate_sources().into_iter() {
            let source=source.map_err(|_|anyhow::anyhow!("Capture source enumeration failed"))?;
            if source.factory().is_some_and(|factory|factory.name()=="pipewiresrc") {
                source.set_property("min-buffers",2_i32);
                source.set_property("max-buffers",4_i32);
                source.set_property("always-copy",true);
                eprintln!("Linux capture: stage=pipewire_buffers min=2 max=4 always_copy=true");
            }
        }
        let sink = pipeline
            .by_name("encoded")
            .unwrap()
            .downcast::<AppSink>()
            .unwrap();
        let encoder_element = pipeline.by_name("encoder").unwrap();
        let supported=encoder_element.static_pad("sink").context("Encoder input missing")?.query_caps(None);
        // Negotiate bounds before starting PipeWire. Changing capsfilter from a
        // downstream CAPS probe sends RECONFIGURE upstream and can reconnect or
        // deadlock the PipeWire stream while its state transition is in flight.
        // videoconvertscale fixates these ranges to the nearest supported size,
        // preserving aspect ratio through square pixels and even YUV dimensions.
        let bounds=gst::Caps::builder("video/x-raw")
            .field("format",raw_format)
            .field("width",gst::IntRange::<i32>::with_step(2,8192,2))
            .field("height",gst::IntRange::<i32>::with_step(2,8192,2))
            .field("pixel-aspect-ratio",gst::Fraction::new(1,1)).build();
        let bounds=bounds.intersect(&supported);
        ensure!(!bounds.is_empty(),"No compatible encoder resolution");
        pipeline.by_name("scaled").unwrap().set_property("caps",bounds);
        pipeline.by_name("source_bounds").unwrap().static_pad("src").unwrap().add_probe(gst::PadProbeType::EVENT_DOWNSTREAM,move |pad,info| {
            let Some(gst::PadProbeData::Event(event))=&info.data else{return gst::PadProbeReturn::Ok;};
            let gst::EventView::Caps(event)=event.view() else{return gst::PadProbeReturn::Ok;};
            let size=event.caps().structure(0).and_then(|s|Some((s.get::<i32>("width").ok()?,s.get::<i32>("height").ok()?)));
            let Some((w,h))=size.filter(|(w,h)|*w>=2 && *h>=2 && crate::resolution::valid(*w as u32,*h as u32)) else {
                if let Some(element)=pad.parent_element(){gst::element_error!(element,gst::StreamError::Format,["Capture exceeds 8192 per axis or 32 megapixels"]);}
                return gst::PadProbeReturn::Drop;
            };
            let format=event.caps().structure(0).and_then(|s|s.get::<&str>("format").ok()).unwrap_or("unknown");
            let memory=event.caps().features(0).map(|features|features.to_string()).unwrap_or_default();
            eprintln!("Linux capture: stage=source_caps width={w} height={h} format={format} memory={memory}");
            gst::PadProbeReturn::Ok
        });
        pipeline.by_name("scaled").unwrap().static_pad("src").unwrap().add_probe(gst::PadProbeType::EVENT_DOWNSTREAM,|_,info| {
            if let Some(gst::PadProbeData::Event(event))=&info.data {
                if let gst::EventView::Caps(event)=event.view() {
                    if let Some(s)=event.caps().structure(0) {
                        if let (Ok(w),Ok(h))=(s.get::<i32>("width"),s.get::<i32>("height")) {
                            eprintln!("Linux capture: stage=output_caps width={w} height={h}");
                        }
                    }
                }
            }
            gst::PadProbeReturn::Ok
        });
        if chain.starts_with("nv") {
            ensure!(
                enum_property(&encoder_element, "preset", "p1"),
                "Encoder NVENC antigo; use a versão AppImage com o plugin moderno"
            );
            enum_property(&encoder_element, "tune", "ultra-low-latency");
            for name in ["zerolatency", "zero-reorder-delay"] {
                if encoder_element.find_property(name).is_some() {
                    encoder_element.set_property(name, true);
                }
            }
            if encoder_element.find_property("bframes").is_some() {
                encoder_element.set_property("bframes", 0_u32);
            }
            if encoder_element.find_property("rc-lookahead").is_some() {
                encoder_element.set_property("rc-lookahead", 0_u32);
            }
        }
        let mut result = Self {
            pipeline,
            sink,
            encoder: encoder_element,
            lease,
            _fd: fd,
            backend: chain.split_whitespace().next().unwrap_or("encoder").into(),
            fps,
            bitrate_limit,
            ready: None,
        };
        eprintln!("Linux capture: stage=pipeline_start");
        if let Err(e) = result.pipeline.set_state(gst::State::Playing) {
            eprintln!("Linux capture: stage=pipeline_state_failed error={e}");
            // Drain the capture bus before stopping so driver/format failures
            // aren't lost behind the generic state-change error.
            if let Err(detail)=result.poll(){eprintln!("Linux capture: stage=pipeline_error error={detail:#}");}
            result.stop().await;
            return Err(e.into());
        }
        eprintln!("Linux capture: stage=waiting_first_packet");
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let readiness = async {
            loop {
                if let Some(packet) = result.poll()? {
                    eprintln!("Linux capture: stage=first_packet width={} height={} bytes={}",packet.width,packet.height,packet.bytes.len());
                    result.ready = Some(packet);
                    return Ok::<_, anyhow::Error>(());
                }
                ensure!(
                    std::time::Instant::now() < deadline,
                    "A origem/encoder não produziu vídeo; confira o driver e a seleção"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        .await;
        if let Err(error) = readiness {
            eprintln!("Linux capture: stage=start_failed error={error:#}");
            result.stop().await;
            return Err(error);
        }
        Ok(result)
    }
    pub fn poll(&mut self) -> Result<Option<Packet>> {
        if let Some(packet) = self.ready.take() {
            return Ok(Some(packet));
        }
        if let Some(message) = self
            .pipeline
            .bus()
            .unwrap()
            .pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos])
        {
            match message.view() {
                gst::MessageView::Error(e) => anyhow::bail!("Captura/encoder: {}", e.error()),
                _ => anyhow::bail!("A origem de captura foi encerrada"),
            }
        }
        let Some(sample) = self.sink.try_pull_sample(gst::ClockTime::ZERO) else {
            return Ok(None);
        };
        let buffer = sample.buffer().context("Empty capture buffer")?;
        let caps = sample.caps().context("Missing encoded caps")?;
        let structure = caps.structure(0).context("Missing encoded structure")?;
        let width = structure.get::<i32>("width")?;
        let height = structure.get::<i32>("height")?;
        ensure!(
            (2..=8192).contains(&width) && (2..=8192).contains(&height) && i64::from(width)*i64::from(height)<=33_554_432,
            "Origem excede 8192 pixels por eixo ou 32 megapixels"
        );
        let data = buffer.map_readable()?;
        ensure!(data.len() <= 16 * 1024 * 1024, "Encoded frame too large");
        Ok(Some(Packet {
            bytes: data.to_vec(),
            timestamp_us: buffer
                .pts()
                .context("Missing capture timestamp")?
                .useconds() as i64,
            keyframe: !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT),
            width: width as u32,
            height: height as u32,
        }))
    }
    pub fn request_keyframe(&self) {
        let _ = self.pipeline.send_event(
            gstreamer_video::UpstreamForceKeyUnitEvent::builder()
                .all_headers(true)
                .build(),
        );
    }
    pub fn bitrate(&self, bits: u32) {
        if self.encoder.find_property("bitrate").is_some() {
            self.encoder
                .set_property("bitrate", bits.clamp(64_000, self.bitrate_limit) / 1000);
        }
    }
    pub async fn stop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        if let Some(lease) = &mut self.lease {
            lease.close().await;
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod pipewire_tests {
    use super::*;

    #[test]
    #[ignore = "requires the isolated synthetic PipeWire test daemon"]
    fn pipewire_first_encoded_frames() -> Result<()> {
        gst::init()?;
        let socket=std::os::unix::net::UnixStream::connect(std::env::var("HYPERCAST_TEST_PIPEWIRE_SOCKET")?)?;
        let fd:OwnedFd=socket.into();
        let size=std::env::var("HYPERCAST_TEST_PIPEWIRE_SIZE").unwrap_or_else(|_|"3440x1440".into());
        let (w,h)=size.split_once('x').context("Invalid test size")?;
        let (w,h)=(w.parse::<u32>()?,h.parse::<u32>()?);
        ensure!(crate::resolution::valid(w,h),"Invalid test size");
        let encoder=if std::env::var("HYPERCAST_TEST_PIPEWIRE_ENCODER").as_deref()==Ok("GPU"){Encoder::Auto}else{Encoder::Software};
        let codec=if std::env::var("HYPERCAST_TEST_PIPEWIRE_CODEC").as_deref()==Ok("av1"){Codec::Av1}else{Codec::Hevc};
        tokio::runtime::Runtime::new()?.block_on(async move {
            let description=format!("pipewiresrc fd={} target-object=HyperCastTest client-name=HyperCastProbe do-timestamp=true ! video/x-raw,width={w},height={h}",fd.as_raw_fd());
            let mut capture=Capture::open_pipeline(description,None,Some(fd),codec,encoder,crate::bandwidth::UploadProfile::Economic).await?;
            let mut frames=0;
            let deadline=std::time::Instant::now()+Duration::from_secs(15);
            while frames<10 && std::time::Instant::now()<deadline {
                if let Some(packet)=capture.poll()? {
                    ensure!(packet.width.abs_diff(w)<=1 && packet.height.abs_diff(h)<=1,"Unexpected synthetic size: {}x{}",packet.width,packet.height);
                    frames+=1;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            capture.stop().await;
            ensure!(frames>=10,"PipeWire produced {frames}/10 encoded frames");
            eprintln!("PASS: PipeWire FD -> production capture -> {codec:?} {encoder:?} {w}x{h}: {frames} frames");
            Ok(())
        })
    }
}

pub struct AudioCapture {
    pipeline: gst::Pipeline,
    sink: AppSink,
}
impl AudioCapture {
    pub fn open() -> Result<Self> {
        Self::with_source("pulsesrc device=@DEFAULT_MONITOR@ buffer-time=30000 latency-time=10000")
    }
    fn with_source(source: &str) -> Result<Self> {
        // @DEFAULT_MONITOR@ is a playback monitor, never a microphone.
        let pipeline=gst::parse::launch(&format!("{source} ! audioconvert ! audioresample ! audio/x-raw,format=S16LE,rate=48000,channels=2,layout=interleaved ! appsink name=pcm sync=false max-buffers=3 drop=true"))?.downcast::<gst::Pipeline>().map_err(|_|anyhow::anyhow!("Invalid audio pipeline"))?;
        let sink = pipeline
            .by_name("pcm")
            .unwrap()
            .downcast::<AppSink>()
            .unwrap();
        let result = Self { pipeline, sink };
        result.pipeline.set_state(gst::State::Playing)?;
        Ok(result)
    }
    pub fn poll(&self) -> Result<Option<Vec<i16>>> {
        if let Some(e) = self
            .pipeline
            .bus()
            .unwrap()
            .pop_filtered(&[gst::MessageType::Error])
        {
            if let gst::MessageView::Error(e) = e.view() {
                anyhow::bail!("Áudio: {}", e.error());
            }
        }
        let Some(sample) = self.sink.try_pull_sample(gst::ClockTime::ZERO) else {
            return Ok(None);
        };
        let map = sample
            .buffer()
            .context("Empty audio buffer")?
            .map_readable()?;
        ensure!(
            map.len() <= 19200 && map.len() % 4 == 0,
            "Invalid stereo audio buffer"
        );
        Ok(Some(
            map.chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect(),
        ))
    }
}
impl Drop for AudioCapture {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
pub struct Playout {
    pipeline: gst::Pipeline,
    source: AppSrc,
}
impl Playout {
    pub fn open() -> Result<Self> {
        let pipeline=gst::parse::launch("appsrc name=pcm is-live=true format=time do-timestamp=true block=false max-bytes=11520 ! audio/x-raw,format=S16LE,rate=48000,channels=2,layout=interleaved ! queue max-size-buffers=3 max-size-bytes=0 max-size-time=0 leaky=downstream ! pulsesink sync=false buffer-time=30000 latency-time=10000")?.downcast::<gst::Pipeline>().map_err(|_|anyhow::anyhow!("Invalid playout pipeline"))?;
        let source = pipeline
            .by_name("pcm")
            .unwrap()
            .downcast::<AppSrc>()
            .unwrap();
        let result = Self { pipeline, source };
        result.pipeline.set_state(gst::State::Playing)?;
        Ok(result)
    }
    pub fn push(&self, samples: &[i16], volume: u32) -> Result<()> {
        if let Some(message) = self
            .pipeline
            .bus()
            .unwrap()
            .pop_filtered(&[gst::MessageType::Error])
        {
            if let gst::MessageView::Error(e) = message.view() {
                anyhow::bail!("Reprodução de áudio: {}", e.error());
            }
        }
        if self.source.current_level_bytes() > 11520 {
            return Ok(());
        }
        ensure!(samples.len() <= 1920, "Audio frame too large");
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|s| ((i32::from(*s) * volume.min(100) as i32 / 100) as i16).to_le_bytes())
            .collect();
        self.source
            .push_buffer(gst::Buffer::from_mut_slice(bytes))?;
        Ok(())
    }
}
impl Drop for Playout {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

unsafe extern "C" {
    fn hc_linux_live_decoders() -> u32;
    fn hc_linux_decode_test(data: *const u8, length: usize, width: i32, height: i32) -> i32;
}
pub fn decoder_count() -> u32 {
    unsafe { hc_linux_live_decoders() }
}

pub async fn media_self_test() -> Result<()> {
    gst::init()?;
    let malformed = [0_u8; 16];
    // SAFETY: bounded static bytes; the native test constructs and destroys its
    // decoder synchronously even when decoding rejects the packet.
    let status = unsafe { hc_linux_decode_test(malformed.as_ptr(), malformed.len(), 640, 360) };
    ensure!(
        status < 0 && decoder_count() == 0,
        "Failed native decode retained resources"
    );

    for _ in 0..3 {
        let mut capture = Capture::open(Source::Test, Codec::Hevc, Encoder::Software).await?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut frames = 0;
        let mut key = false;
        while std::time::Instant::now() < deadline && frames < 5 {
            if let Some(packet) = capture.poll()? {
                if packet.keyframe {
                    // SAFETY: the bounded slice remains live during synchronous decode;
                    // C++ owns and releases all temporary decoder resources.
                    let status = unsafe {
                        hc_linux_decode_test(
                            packet.bytes.as_ptr(),
                            packet.bytes.len(),
                            packet.width as i32,
                            packet.height as i32,
                        )
                    };
                    ensure!(
                        status == 0,
                        "Native HEVC decode/lifecycle test failed: {status}"
                    );
                }
                key |= packet.keyframe;
                frames += 1;
                ensure!(crate::resolution::valid(packet.width,packet.height),"Invalid capture dimensions");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        capture.stop().await;
        ensure!(
            frames >= 5 && key,
            "Synthetic HEVC pipeline did not produce keyframes"
        );
    }
    ensure!(decoder_count() == 0, "Decoder owner leak");
    println!(
        "PASS: repeated synthetic capture, HEVC encode/decode, keyframe, native destruction and pipeline shutdown"
    );
    Ok(())
}

pub async fn publish_self_test(address: &str, seconds: u32) -> Result<()> {
    use livekit::{
        prelude::*,
        webrtc::{
            audio_frame::AudioFrame,
            audio_source::{AudioSourceOptions, RtcAudioSource, native::NativeAudioSource},
        },
    };
    use std::{borrow::Cow, sync::Arc};
    let gpu = std::env::var("HYPERCAST_LINUX_TEST_GPU").as_deref() == Ok("1");
    let codec = if std::env::var("HYPERCAST_LINUX_TEST_CODEC").as_deref() == Ok("av1") {
        Codec::Av1
    } else {
        Codec::Hevc
    };
    let encoder = if gpu {
        Encoder::Auto
    } else {
        Encoder::Software
    };
    let upload=match std::env::var("HYPERCAST_LINUX_TEST_UPLOAD_MBPS"){
        Ok(value)=>crate::bandwidth::UploadProfile::from_mbps(value.parse()?).context("Invalid synthetic video budget")?,
        Err(_)=>crate::bandwidth::UploadProfile::Maximum,
    };
    let mut capture = Capture::open_with_profile(Source::Test, codec, encoder,upload).await?;
    ensure!(capture.encoder.property::<u32>("bitrate")<=capture.bitrate_limit/1000,"Encoder ignored video budget");
    println!("Linux video budget: {} bps",capture.bitrate_limit);
    let fps = capture.fps;
    let (room, mut events) =
        crate::network::connect(address, &crate::network::token_from_env()?).await?;
    let room = Arc::new(room);
    let event_task = tokio::spawn(async move { while events.recv().await.is_some() {} });
    let result = async {
        let mut publisher = crate::network::EncodedPublisher::in_room(
            room.clone(),
            640,
            360,
            fps,
            capture.bitrate_limit,
            codec == Codec::Av1,
        )
        .await?;
        let audio = AudioCapture::with_source(
            "audiotestsrc is-live=true wave=sine freq=997 samplesperbuffer=480",
        )?;
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
            "synthetic-audio",
            RtcAudioSource::Native(source.clone()),
        );
        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                livekit::options::TrackPublishOptions {
                    source: TrackSource::ScreenshareAudio,
                    audio_encoding: Some(livekit::options::AudioEncoding {
                        max_bitrate: 128000,
                    }),
                    dtx: false,
                    simulcast: false,
                    ..Default::default()
                },
            )
            .await?;
        let start = std::time::Instant::now();
        let mut frames = 0;
        let mut audio_frames = 0;
        let workload = async {
            while start.elapsed() < Duration::from_secs(seconds as u64) {
                if let Some(packet) = capture.poll()? {
                    if publisher.take_keyframe_request() {
                        capture.request_keyframe();
                        if !packet.keyframe {
                            continue;
                        }
                    }
                    publisher.set_resolution_if_changed(packet.width,packet.height);
                    if publisher.push(&packet.bytes, packet.timestamp_us, packet.keyframe)? {
                        frames += 1;
                    }
                }
                if let Some(samples) = audio.poll()? {
                    for data in samples.chunks(960) {
                        source
                            .capture_frame(&AudioFrame {
                                data: Cow::Borrowed(data),
                                sample_rate: 48000,
                                num_channels: 2,
                                samples_per_channel: (data.len() / 2) as u32,
                            })
                            .await?;
                        audio_frames += 1;
                    }
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            ensure!(
                frames > 0 && audio_frames > 0,
                "Synthetic video/audio publication failed"
            );
            println!("Linux synthetic publisher: {frames} frames, {audio_frames} audio frames");
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let close = publisher.stop().await;
        workload?;
        close?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    capture.stop().await;
    let _ = room.close().await;
    event_task.abort();
    let _ = event_task.await;
    result
}
