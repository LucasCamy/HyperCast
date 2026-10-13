//! System sound only. Process loopback excludes this executable's own playback.
use anyhow::{Result, anyhow};
use livekit::{prelude::*, options::{AudioEncoding, TrackPublishOptions}, webrtc::{
    audio_frame::AudioFrame, audio_source::{AudioSourceOptions, RtcAudioSource, native::NativeAudioSource},
    audio_stream::native::{NativeAudioStream, NativeAudioStreamOptions},
}};
use std::{borrow::Cow, collections::VecDeque, ffi::{CStr, c_char, c_void}, ptr::NonNull,
    sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}, mpsc}, thread, time::Duration};
use tokio_stream::StreamExt;

unsafe extern "C" {
    fn hc_audio_capture_open(error: *mut c_char, size: usize) -> *mut c_void;
    fn hc_audio_capture_read(handle: *mut c_void, samples: *mut i16, capacity: u32, error: *mut c_char, size: usize) -> i32;
    fn hc_audio_capture_close(handle: *mut c_void);
    fn hc_audio_playout_open(error: *mut c_char, size: usize) -> *mut c_void;
    fn hc_audio_playout_write(handle: *mut c_void, samples: *const i16, frames: u32, error: *mut c_char, size: usize) -> i32;
    fn hc_audio_playout_close(handle: *mut c_void);
}
struct NativeHandle { pointer: NonNull<c_void>, capture: bool }
impl Drop for NativeHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was created and stays on this one COM thread.
        unsafe { if self.capture { hc_audio_capture_close(self.pointer.as_ptr()); }
            else { hc_audio_playout_close(self.pointer.as_ptr()); } }
    }
}
fn error(buffer: &[c_char]) -> anyhow::Error {
    // SAFETY: buffer is initially zero and the native error writer terminates it.
    anyhow!(unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned())
}

pub struct SystemAudioPublisher {
    room: Arc<Room>, sid: TrackSid, stop: Arc<AtomicBool>, thread: Option<thread::JoinHandle<()>>,
    pub frames: Arc<AtomicU64>, pub failure: Arc<Mutex<Option<String>>>,
}
impl SystemAudioPublisher {
    pub fn start(runtime: &tokio::runtime::Runtime, room: Arc<Room>) -> Result<Self> {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2, 20);
        let track = LocalAudioTrack::create_audio_track("system-audio", RtcAudioSource::Native(source.clone()));
        let stop = Arc::new(AtomicBool::new(false)); let frames = Arc::new(AtomicU64::new(0));
        let failure = Arc::new(Mutex::new(None));
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (activate_tx, activate_rx) = mpsc::sync_channel(1);
        let worker_stop = stop.clone(); let worker_frames = frames.clone(); let worker_failure = failure.clone();
        let handle = runtime.handle().clone();
        let worker = thread::spawn(move || {
            let mut message = [0 as c_char; 1024];
            // SAFETY: native handle owns WASAPI/COM and is freed on this thread.
            let pointer = unsafe { hc_audio_capture_open(message.as_mut_ptr(), message.len()) };
            let Some(pointer) = NonNull::new(pointer) else { let _ = ready_tx.send(Err(error(&message).to_string())); return; };
            let capture = NativeHandle { pointer, capture: true };
            let _ = ready_tx.send(Ok(()));
            if activate_rx.recv_timeout(Duration::from_secs(15)).is_err() { return; }
            let mut samples = vec![0i16; 96000]; let mut pending = VecDeque::with_capacity(2880);
            let mut reported_stereo = false;
            while !worker_stop.load(Ordering::Relaxed) {
                // SAFETY: output capacity exactly matches the allocated sample buffer.
                let n = unsafe { hc_audio_capture_read(capture.pointer.as_ptr(), samples.as_mut_ptr(), samples.len() as u32, message.as_mut_ptr(), message.len()) };
                if n < 0 { *worker_failure.lock().unwrap() = Some(error(&message).to_string()); break; }
                if !reported_stereo && samples[..n as usize * 2].chunks_exact(2).filter(|pair| (i32::from(pair[0])-i32::from(pair[1])).abs()>2).count() > 5 {
                    reported_stereo = true; crate::diagnostics::write("audio", "Captured distinct stereo channels", "");
                }
                pending.extend(samples[..n as usize * 2].iter().copied());
                // Discard old backlog: never send more than 30 ms of capture history.
                while pending.len() > 2880 { pending.pop_front(); }
                while pending.len() >= 960 && !worker_stop.load(Ordering::Relaxed) {
                    let data: Vec<i16> = pending.drain(..960).collect();
                    let frame = AudioFrame { data: Cow::Owned(data), sample_rate: 48000, num_channels: 2, samples_per_channel: 480 };
                    let pushed = handle.block_on(async { tokio::time::timeout(Duration::from_millis(100), source.capture_frame(&frame)).await });
                    match pushed {
                        Ok(Ok(())) => { worker_frames.fetch_add(1, Ordering::Relaxed); }
                        _ => { *worker_failure.lock().unwrap() = Some("Envio de áudio interrompido".into()); worker_stop.store(true, Ordering::Relaxed); break; }
                    }
                }
            }
            source.clear_buffer();
        });
        let initialization = ready_rx.recv_timeout(Duration::from_secs(6));
        if !matches!(initialization, Ok(Ok(()))) {
            stop.store(true, Ordering::Relaxed); drop(activate_tx); let _ = worker.join();
            return Err(anyhow!(match initialization { Ok(Err(text)) => text, _ => "Áudio do sistema não iniciou".into() }));
        }
        let publication = runtime.block_on(room.local_participant().publish_track(LocalTrack::Audio(track), TrackPublishOptions {
            source: TrackSource::ScreenshareAudio, audio_encoding: Some(AudioEncoding { max_bitrate: 128_000 }),
            dtx: false, red: true, simulcast: false, ..Default::default()
        }));
        match publication {
            Ok(publication) => {
                let _ = activate_tx.send(());
                crate::diagnostics::write("audio", "System audio published: 48000 Hz stereo, Opus 128 kbps; own process excluded", "");
                Ok(Self { room, sid: publication.sid(), stop, thread: Some(worker), frames, failure })
            }
            Err(error) => { stop.store(true, Ordering::Relaxed); drop(activate_tx); let _ = worker.join(); Err(error.into()) }
        }
    }
    pub fn stop(mut self, runtime: &tokio::runtime::Runtime) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.thread.take() { let _ = worker.join(); }
        if let Err(error) = runtime.block_on(self.room.local_participant().unpublish_track(&self.sid)) {
            crate::diagnostics::write("audio_unpublish_error", &error.to_string(), "");
        }
        crate::diagnostics::write("audio", &format!("System audio stopped; {} PCM frames sent", self.frames.load(Ordering::Relaxed)), "");
    }
}
impl Drop for SystemAudioPublisher {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); if let Some(worker) = self.thread.take() { let _ = worker.join(); } }
}

pub struct AudioReceiver { task: tokio::task::JoinHandle<()>, stop: Arc<AtomicBool>, worker: Option<thread::JoinHandle<()>> }
impl AudioReceiver {
    pub fn start(runtime: &tokio::runtime::Runtime, track: RemoteAudioTrack, volume: Arc<AtomicU64>, count: Arc<AtomicU64>) -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::<Vec<i16>>::with_capacity(3)));
        let stop = Arc::new(AtomicBool::new(false));
        let buffer = queue.clone(); let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            let mut message = [0 as c_char; 1024];
            // SAFETY: the playout handle is used only by its creating COM thread.
            let pointer = unsafe { hc_audio_playout_open(message.as_mut_ptr(), message.len()) };
            let Some(pointer) = NonNull::new(pointer) else { crate::diagnostics::write("audio_playback_error", &error(&message).to_string(), ""); return; };
            let playout = NativeHandle { pointer, capture: false };
            let silent_test = std::env::var("HYPERCAST_AUDIO_TEST_SILENT_OUTPUT").as_deref() == Ok("1");
            let mut reported_playback = false;
            while !worker_stop.load(Ordering::Relaxed) {
                let data = buffer.lock().unwrap().pop_front();
                let Some(mut data) = data else { thread::sleep(Duration::from_millis(2)); continue; };
                let gain = if silent_test { 0 } else { volume.load(Ordering::Relaxed).min(100) };
                for sample in &mut data { *sample = (i64::from(*sample) * gain as i64 / 100) as i16; }
                let deadline = std::time::Instant::now() + Duration::from_millis(20);
                loop {
                    // SAFETY: data contains exactly stereo PCM pairs, at most 20 ms.
                    let result = unsafe { hc_audio_playout_write(playout.pointer.as_ptr(), data.as_ptr(), (data.len()/2) as u32, message.as_mut_ptr(), message.len()) };
                    if result < 0 { crate::diagnostics::write("audio_playback_error", &error(&message).to_string(), ""); return; }
                    if result == 1 && !reported_playback { reported_playback = true; crate::diagnostics::write("audio", "WASAPI playback running", ""); }
                    if result == 1 || std::time::Instant::now() >= deadline || worker_stop.load(Ordering::Relaxed) { break; }
                    thread::sleep(Duration::from_millis(2));
                }
            }
        });
        let mut stream = NativeAudioStream::with_options(track.rtc_track(), 48000, 2,
            NativeAudioStreamOptions { queue_size_frames: Some(3) });
        let task = runtime.spawn(async move {
            let mut reported_signal = false;
            let mut reported_stereo = false;
            while let Some(frame) = stream.next().await {
                if frame.sample_rate != 48000 || frame.num_channels != 2 || frame.data.len() > 1920 || frame.data.len() % 2 != 0 { continue; }
                if !reported_signal && frame.data.iter().any(|sample| *sample != 0) {
                    reported_signal = true;
                    crate::diagnostics::write("audio", "Decoded non-silent system audio", "");
                }
                if !reported_stereo && frame.data.chunks_exact(2).filter(|pair| (i32::from(pair[0])-i32::from(pair[1])).abs()>2).count() > 5 {
                    reported_stereo = true;
                    crate::diagnostics::write("audio", "Decoded distinct stereo channels", "");
                }
                let mut queue = queue.lock().unwrap(); if queue.len() == 3 { queue.pop_front(); }
                queue.push_back(frame.data.into_owned()); count.fetch_add(1, Ordering::Relaxed);
            }
        });
        Self { task, stop, worker: Some(worker) }
    }
}
impl Drop for AudioReceiver {
    fn drop(&mut self) { self.task.abort(); self.stop.store(true, Ordering::Relaxed); if let Some(worker) = self.worker.take() { let _ = worker.join(); } }
}
