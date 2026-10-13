//! Headless receive diagnostic. WebRTC decodes; this module counts sink frames
//! without converting pixels to I420/RGBA or claiming a D3D11VA render path.
use crate::{
    network,
    subscription::{Demand, ViewMode, delivery_demand},
};
use anyhow::{Result, ensure};
use livekit::{
    prelude::*,
    track::VideoQuality,
    webrtc::video_stream::native::{NativeVideoStream, NativeVideoStreamOptions},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio_stream::StreamExt;

pub struct ReceiveOptions {
    pub mode: ViewMode,
    pub focus_identity: Option<String>,
    pub pause_after: Option<Duration>,
    pub duration: Duration,
}

#[derive(Default)]
struct Counters {
    frames: AtomicU64,
    width: AtomicU32,
    height: AtomicU32,
}
struct SinkTask {
    task: tokio::task::JoinHandle<()>,
    counters: Arc<Counters>,
}
impl Drop for SinkTask {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn refresh(room: &Room, options: &ReceiveOptions, paused: bool) {
    let mut screens: Vec<_> = room
        .remote_participants()
        .into_values()
        .flat_map(|participant| {
            let identity = participant.identity().to_string();
            participant
                .track_publications()
                .into_values()
                .map(move |publication| (identity.clone(), publication))
        })
        .filter(|(_, publication)| {
            publication.source() == TrackSource::Screenshare
                && publication.kind() == TrackKind::Video
        })
        .collect();
    screens
        .sort_by_key(|(identity, publication)| (identity.clone(), publication.sid().to_string()));
    // Explicit focus takes precedence. With no focus choose the first stable
    // participant identity, instead of relying on unordered HashMap iteration.
    let focus = options
        .focus_identity
        .as_deref()
        .or_else(|| screens.first().map(|(id, _)| id.as_str()));
    let mut subscribed = 0;
    for (identity, publication) in &screens {
        let wanted = delivery_demand(
            options.mode,
            !paused,
            Some(identity.as_str()) == focus,
            publication.simulcasted(),
        );
        let enabled = wanted != Demand::Paused && subscribed < 3;
        if enabled {
            subscribed += 1;
            if publication.is_subscribed() {
                publication.set_enabled(true);
            }
            publication.set_subscribed(true);
            if publication.simulcasted() {
                publication.set_video_quality(if wanted == Demand::Thumbnail {
                    VideoQuality::Low
                } else {
                    VideoQuality::High
                });
            }
        } else {
            if publication.is_subscribed() {
                publication.set_enabled(false);
            }
            publication.set_subscribed(false);
        }
    }
}

pub async fn receive(address: &str, token: &str, options: ReceiveOptions) -> Result<u64> {
    ensure!(
        !options.duration.is_zero(),
        "Receive duration must be positive"
    );
    livekit::webrtc::enable_zero_playout_delay()?;
    let (room, mut events) = network::connect(address, token).await?;
    let total = Arc::new(AtomicU64::new(0));
    let mut sinks: HashMap<TrackSid, SinkTask> = HashMap::new();
    let start = tokio::time::Instant::now();
    let deadline = start + options.duration;
    let mut paused = false;
    let mut refresh_timer = tokio::time::interval(Duration::from_millis(250));
    refresh_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    refresh(&room, &options, paused);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            _ = refresh_timer.tick() => {
                if !paused && options.pause_after.is_some_and(|after| start.elapsed() >= after) {
                    paused = true;
                    println!("Receiver paused: unsubscribing all screen tracks at {} frames", total.load(Ordering::Relaxed));
                    // Stop local decode consumption immediately; SFU unsubscribe
                    // is asynchronous and can leave a few packets in transit.
                    sinks.clear();
                    refresh(&room, &options, paused);
                }
            }
            event = events.recv() => {
                let Some(event) = event else { break; };
                match event {
                    RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), publication, .. } => {
                        let sid = publication.sid();
                        if paused || publication.source() != TrackSource::Screenshare || (sinks.len() >= 3 && !sinks.contains_key(&sid)) {
                            publication.set_subscribed(false);
                            continue;
                        }
                        let counters = Arc::new(Counters::default());
                        let observed = counters.clone();
                        let total = total.clone();
                        let mut stream = NativeVideoStream::with_options(track.rtc_track(),
                            NativeVideoStreamOptions { queue_size_frames: Some(1) });
                        println!("Receiving screen {} with a one-frame sink queue", sid);
                        let task = tokio::spawn(async move {
                            while let Some(frame) = stream.next().await {
                                observed.width.store(frame.buffer.width(), Ordering::Relaxed);
                                observed.height.store(frame.buffer.height(), Ordering::Relaxed);
                                observed.frames.fetch_add(1, Ordering::Relaxed);
                                total.fetch_add(1, Ordering::Relaxed);
                                // Frame buffer drops here; no pixel readback/conversion.
                            }
                        });
                        sinks.insert(sid, SinkTask { task, counters });
                    }
                    RoomEvent::TrackUnsubscribed { publication, .. } | RoomEvent::TrackUnpublished { publication, .. } => {
                        if let Some(sink) = sinks.remove(&publication.sid()) {
                            println!("Screen {} ended after {} decoded frames", publication.sid(), sink.counters.frames.load(Ordering::Relaxed));
                        }
                        refresh(&room, &options, paused);
                    }
                    RoomEvent::TrackPublished { .. } | RoomEvent::ParticipantConnected(_) | RoomEvent::ParticipantDisconnected(_) => {
                        refresh(&room, &options, paused);
                    }
                    RoomEvent::TrackSubscriptionFailed { error, .. } => {
                        sinks.clear();
                        room.close().await?;
                        anyhow::bail!("Video subscription failed: {error}");
                    }
                    _ => {}
                }
            }
        }
    }
    refresh(&room, &options, true);
    for (sid, sink) in &sinks {
        println!(
            "Screen {}: {} frames, {}x{}",
            sid,
            sink.counters.frames.load(Ordering::Relaxed),
            sink.counters.width.load(Ordering::Relaxed),
            sink.counters.height.load(Ordering::Relaxed)
        );
    }
    sinks.clear();
    room.close().await?;
    let count = total.load(Ordering::Relaxed);
    println!("Receiver total: {count} decoded frames");
    Ok(count)
}
