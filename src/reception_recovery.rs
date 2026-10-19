//! Conservative reception recovery. RTP silence is never itself a failure.
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub observed: u32,
    pub mapped: u32,
    pub recovering: u32,
    pub exhausted: u32,
    pub actions: u64,
    pub reader_stalls: u64,
    pub decoder_stalls: u64,
    pub unmapped: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Counters {
    pub bytes: u64,
    pub encoded: u64,
    pub decoded: u64,
    pub delivered: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Reattach,
    Refresh,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Decoder,
    Reader,
}
#[derive(Default)]
pub struct Watch {
    last: Option<(Instant, Counters)>,
    stalled: Duration,
    stable: Duration,
    attempts: u8,
    next: Option<Instant>,
    pub recovering: bool,
}
impl Watch {
    pub fn suspend(&mut self) {
        self.last = None;
        self.stalled = Duration::ZERO;
        self.stable = Duration::ZERO;
        self.recovering = false;
    }
    pub fn exhausted(&self) -> bool {
        self.recovering && self.attempts >= 3
    }
    pub fn observe(&mut self, now: Instant, c: Counters) -> Option<(Action, Reason)> {
        let Some((at, old)) = self.last.replace((now, c)) else {
            return None;
        };
        let dt = now.saturating_duration_since(at);
        if dt > Duration::from_secs(10)
            || c.bytes < old.bytes
            || c.encoded < old.encoded
            || c.decoded < old.decoded
            || c.delivered < old.delivered
        {
            self.stalled = Duration::ZERO;
            self.stable = Duration::ZERO;
            return None;
        }
        if c.delivered > old.delivered {
            self.recovering = false;
            self.stalled = Duration::ZERO;
            self.stable += dt;
            if self.stable >= Duration::from_secs(15) {
                self.attempts = 0;
                self.next = None;
            }
            return None;
        }
        self.stable = Duration::ZERO;
        // Padding, probes and retransmissions do not prove complete frames arrived.
        let reason = if c.decoded > old.decoded {
            Reason::Reader
        } else if c.bytes > old.bytes && c.encoded > old.encoded {
            Reason::Decoder
        } else {
            self.stalled = Duration::ZERO;
            return None;
        };
        self.stalled += dt;
        if self.stalled < Duration::from_secs(8)
            || self.attempts >= 3
            || self.next.is_some_and(|at| now < at)
        {
            return None;
        }
        self.stalled = Duration::ZERO;
        self.recovering = true;
        self.attempts += 1;
        self.next = Some(now + Duration::from_secs(5 << (self.attempts - 1)));
        Some((
            if self.attempts == 1 {
                Action::Reattach
            } else {
                Action::Refresh
            },
            reason,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_padding_pauses_and_counter_reset_do_not_trigger() {
        let start = Instant::now();
        let mut w = Watch::default();
        for i in 0..40 {
            assert!(
                w.observe(
                    start + Duration::from_secs(i * 2),
                    Counters {
                        bytes: i * 10,
                        ..Default::default()
                    }
                )
                .is_none()
            );
        }
        assert_eq!(w.attempts, 0);
        w.suspend();
        assert!(
            w.observe(
                start + Duration::from_secs(81),
                Counters {
                    bytes: 1,
                    encoded: 1,
                    decoded: 1,
                    delivered: 1
                }
            )
            .is_none()
        );
        assert!(
            w.observe(start + Duration::from_secs(83), Counters::default())
                .is_none()
        );
        assert_eq!(w.attempts, 0);
    }
    #[test]
    fn recovering_one_stream_is_bounded_and_needs_sustained_progress_to_rearm() {
        let start = Instant::now();
        let (mut broken, mut healthy) = (Watch::default(), Watch::default());
        let mut actions = vec![];
        for i in 0..70 {
            let now = start + Duration::from_secs(i * 2);
            let c = Counters {
                bytes: i * 100,
                encoded: i,
                decoded: i,
                delivered: 0,
            };
            if let Some(a) = broken.observe(now, c) {
                actions.push(a);
            }
            assert!(
                healthy
                    .observe(now, Counters { delivered: i, ..c })
                    .is_none()
            );
        }
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0], (Action::Reattach, Reason::Reader));
        assert_eq!(actions[1].0, Action::Refresh);
        assert!(broken.exhausted());
        broken.suspend();
        for i in 70..80 {
            broken.observe(
                start + Duration::from_secs(i * 2),
                Counters {
                    bytes: i * 100,
                    encoded: i,
                    decoded: i,
                    delivered: i,
                },
            );
        }
        assert_eq!(broken.attempts, 0);
        assert!(!broken.recovering);
    }
    #[test]
    fn focus_flapping_and_missing_samples_never_replenish_attempts() {
        let start = Instant::now();
        let mut w = Watch::default();
        let mut count = 0;
        for episode in 0..8 {
            w.suspend();
            for step in 0..7 {
                let i = episode * 20 + step * 2;
                if w.observe(
                    start + Duration::from_secs(i),
                    Counters {
                        bytes: i * 100,
                        encoded: i,
                        decoded: i,
                        ..Default::default()
                    },
                )
                .is_some()
                {
                    count += 1;
                }
            }
        }
        assert_eq!(count, 3);
        assert_eq!(w.attempts, 3);
    }
    #[test]
    fn decoder_evidence_and_unknown_intervals_are_conservative() {
        let start = Instant::now();
        let mut w = Watch::default();
        let mut actions = vec![];
        for i in 0..7 {
            if let Some(a) = w.observe(
                start + Duration::from_secs(i * 2),
                Counters {
                    bytes: i * 100,
                    encoded: i,
                    ..Default::default()
                },
            ) {
                actions.push(a);
            }
        }
        assert_eq!(actions, vec![(Action::Reattach, Reason::Decoder)]);
        w.suspend();
        assert!(
            w.observe(
                start + Duration::from_secs(40),
                Counters {
                    bytes: 999,
                    encoded: 99,
                    ..Default::default()
                }
            )
            .is_none()
        );
        assert_eq!(w.attempts, 1);
    }
}

#[cfg(feature = "desktop")]
pub mod runtime {
    use super::*;
    use livekit::{prelude::*, webrtc::stats::RtcStats};
    use std::{collections::HashMap, sync::Arc};
    struct Target {
        sid: String,
        rtc: String,
        owner: String,
        publication: RemoteTrackPublication,
    }
    struct Sample {
        at: Instant,
        targets: Vec<(String, String)>,
        rows: HashMap<String, Counters>,
        network: Option<crate::video_metrics::VideoNetwork>,
    }
    pub struct Repair {
        pub sid: String,
        pub action: Action,
        pub reason: Reason,
    }
    pub struct Monitor {
        watches: HashMap<String, Watch>,
        holds: HashMap<String, Instant>,
        owners: HashMap<String, String>,
        task: Option<tokio::task::JoinHandle<Option<Sample>>>,
        last_poll: Option<Instant>,
        pub network: Option<crate::video_metrics::VideoNetwork>,
        pub summary: Summary,
    }
    impl Default for Monitor {
        fn default() -> Self {
            Self {
                watches: HashMap::new(),
                holds: HashMap::new(),
                owners: HashMap::new(),
                task: None,
                last_poll: None,
                network: None,
                summary: Summary::default(),
            }
        }
    }
    impl Drop for Monitor {
        fn drop(&mut self) {
            if let Some(task) = self.task.take() {
                task.abort();
            }
        }
    }
    impl Monitor {
        pub fn held(&self, sid: &str) -> bool {
            self.holds
                .get(sid)
                .is_some_and(|until| Instant::now() < *until)
        }
        pub fn recovering(&self) -> Vec<(String, String, bool)> {
            self.watches
                .iter()
                .filter(|(_, w)| w.recovering)
                .map(|(sid, w)| {
                    (
                        sid.clone(),
                        self.owners.get(sid).cloned().unwrap_or_default(),
                        w.exhausted(),
                    )
                })
                .collect()
        }
        /// Interrupt evidence collection without replenishing per-stream retry budgets.
        pub fn suspend(&mut self) {
            if let Some(task) = self.task.take() {
                task.abort();
            }
            for watch in self.watches.values_mut() {
                watch.suspend();
            }
            self.holds.clear();
            self.last_poll = None;
            self.network = None;
            self.summary.recovering = 0;
            self.summary.exhausted = 0;
            self.summary.mapped = 0;
        }
        pub fn reset(&mut self) {
            if let Some(t) = self.task.take() {
                t.abort();
            }
            self.watches.clear();
            self.holds.clear();
            self.owners.clear();
            self.last_poll = None;
            self.network = None;
            self.summary.observed = 0;
            self.summary.mapped = 0;
            self.summary.recovering = 0;
            self.summary.exhausted = 0;
        }
        pub async fn tick(
            &mut self,
            room: &Arc<Room>,
            allowed: bool,
            delivered: impl Fn(&str, &str) -> u64,
        ) -> Vec<Repair> {
            let now = Instant::now();
            self.holds.retain(|_, until| now < *until);
            let mut targets = vec![];
            for p in room.remote_participants().into_values() {
                for publication in p.track_publications().into_values() {
                    if publication.source() == TrackSource::Screenshare && publication.is_desired()
                    {
                        if let Some(RemoteTrack::Video(track)) = publication.track() {
                            targets.push(Target {
                                sid: publication.sid().to_string(),
                                rtc: track.rtc_track().id(),
                                owner: p.identity().to_string(),
                                publication,
                            });
                        }
                    }
                }
            }
            targets.sort_by(|a, b| a.sid.cmp(&b.sid));
            targets.truncate(3);
            self.watches
                .retain(|sid, _| targets.iter().any(|t| &t.sid == sid));
            self.owners
                .retain(|sid, _| targets.iter().any(|t| &t.sid == sid));
            let mut repairs = vec![];
            let mut sample = None;
            if self.task.as_ref().is_some_and(|t| t.is_finished()) {
                sample = self.task.take().unwrap().await.ok().flatten();
                if sample.is_none() {
                    for w in self.watches.values_mut() {
                        w.suspend();
                    }
                    self.network = None;
                }
            }
            if let Some(sample) = sample {
                self.network = sample.network;
                self.summary.mapped = 0;
                for t in &targets {
                    self.owners.insert(t.sid.clone(), t.owner.clone());
                    let w = self.watches.entry(t.sid.clone()).or_default();
                    let active = allowed
                        && room.connection_state() == ConnectionState::Connected
                        && t.publication.is_enabled()
                        && !t.publication.is_muted()
                        && !self.holds.contains_key(&t.sid);
                    let mapped = !t.rtc.is_empty()
                        && targets.iter().filter(|other| other.rtc == t.rtc).count() == 1
                        && sample
                            .targets
                            .iter()
                            .filter(|(_, rtc)| *rtc == t.rtc)
                            .count()
                            == 1
                        && sample
                            .targets
                            .iter()
                            .any(|(sid, rtc)| *sid == t.sid && *rtc == t.rtc);
                    if !active {
                        if !self.holds.contains_key(&t.sid) {
                            w.suspend();
                        }
                        continue;
                    }
                    if let Some(mut counters) = sample.rows.get(&t.rtc).copied().filter(|_| mapped)
                    {
                        self.summary.mapped += 1;
                        counters.delivered = delivered(&t.sid, &t.owner);
                        if let Some((action, reason)) = w.observe(sample.at, counters) {
                            self.summary.actions = self.summary.actions.saturating_add(1);
                            match reason {
                                Reason::Reader => {
                                    self.summary.reader_stalls =
                                        self.summary.reader_stalls.saturating_add(1)
                                }
                                Reason::Decoder => {
                                    self.summary.decoder_stalls =
                                        self.summary.decoder_stalls.saturating_add(1)
                                }
                            }
                            if action == Action::Refresh {
                                self.holds
                                    .insert(t.sid.clone(), now + Duration::from_millis(700));
                                t.publication.set_enabled(false);
                            }
                            repairs.push(Repair {
                                sid: t.sid.clone(),
                                action,
                                reason,
                            });
                        }
                    } else {
                        w.suspend();
                        self.summary.unmapped = self.summary.unmapped.saturating_add(1);
                    }
                }
            }
            for t in &targets {
                if !allowed
                    || room.connection_state() != ConnectionState::Connected
                    || (!t.publication.is_enabled() && !self.held(&t.sid))
                    || t.publication.is_muted()
                {
                    if let Some(w) = self.watches.get_mut(&t.sid) {
                        w.suspend();
                    }
                }
            }
            if self.task.is_none()
                && room.connection_state() == ConnectionState::Connected
                && self
                    .last_poll
                    .is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_secs(5))
            {
                let snapshot: Vec<_> = targets
                    .iter()
                    .map(|t| (t.sid.clone(), t.rtc.clone()))
                    .collect();
                let observed = room.clone();
                self.last_poll = Some(now);
                self.task = Some(tokio::spawn(async move {
                    let stats = tokio::time::timeout(Duration::from_secs(3), observed.get_stats())
                        .await
                        .ok()?
                        .ok()?;
                    #[cfg(windows)]
                    {
                        for line in crate::network::video_stats_lines(
                            &stats.publisher_stats,
                            &stats.subscriber_stats,
                        ) {
                            crate::diagnostics::write("video_rtp", &line, "");
                        }
                        crate::network::print_ice_path("publisher", &stats.publisher_stats);
                        crate::network::print_ice_path("subscriber", &stats.subscriber_stats);
                    }
                    let mut rows: HashMap<String, Counters> = HashMap::new();
                    for entry in &stats.subscriber_stats {
                        if let RtcStats::InboundRtp(r) = entry {
                            if r.stream.kind == "video"
                                && snapshot
                                    .iter()
                                    .any(|(_, id)| id == &r.inbound.track_identifier)
                            {
                                let c = rows.entry(r.inbound.track_identifier.clone()).or_default();
                                c.bytes = c.bytes.saturating_add(r.inbound.bytes_received);
                                c.encoded = c.encoded.saturating_add(r.inbound.frames_received);
                                c.decoded =
                                    c.decoded.saturating_add(r.inbound.frames_decoded as u64);
                            }
                        }
                    }
                    Some(Sample {
                        at: Instant::now(),
                        targets: snapshot,
                        rows,
                        network: crate::network::video_network_sample(&stats.subscriber_stats),
                    })
                }));
            }
            self.summary.observed = targets.len() as u32;
            self.summary.recovering = self.watches.values().filter(|w| w.recovering).count() as u32;
            self.summary.exhausted = self.watches.values().filter(|w| w.exhausted()).count() as u32;
            repairs
        }
    }
}

#[cfg(feature = "desktop")]
pub fn status_label(exhausted: bool) -> &'static str {
    if exhausted {
        crate::i18n::label(
            "Sem progresso - use Atualizar recepção",
            "No progress - use Refresh reception",
        )
    } else {
        crate::i18n::label("Recuperando - último quadro", "Recovering - last frame")
    }
}

#[cfg(all(test, feature = "desktop", any(windows, target_os = "linux")))]
mod native_tests {
    use super::*;
    #[cfg(windows)]
    use crate::desktop_engine::{Command, Engine, Layout};
    #[cfg(target_os = "linux")]
    use crate::linux::engine::{Command, Engine, Layout};
    use anyhow::Result;
    fn wait(mut ready: impl FnMut() -> bool, seconds: u64, message: &str) -> Result<()> {
        let until = Instant::now() + Duration::from_secs(seconds);
        while !ready() {
            anyhow::ensure!(Instant::now() < until, "{message}");
            std::thread::sleep(Duration::from_millis(40));
        }
        Ok(())
    }
    #[test]
    #[ignore = "Needs an isolated room with two synthetic moving publishers"]
    fn automatic_reception_recovers_only_stalled_stream() -> Result<()> {
        let address = std::env::var("HYPERCAST_URL")?;
        let token = std::env::var("HYPERCAST_ROOM_TOKEN")?;
        #[cfg(windows)]
        let engine = Engine::connect(address, token, eframe::egui::Context::default(), 0, false);
        #[cfg(target_os = "linux")]
        let engine = Engine::start(address, token, eframe::egui::Context::default());
        #[cfg(windows)]
        let (shared, commands) = (&engine.shared, &engine.commands);
        #[cfg(target_os = "linux")]
        let (shared, commands) = (&engine.state, &engine.tx);
        #[cfg(windows)]
        let mut layout = Layout {
            self_preview: false,
            mode: crate::subscription::ViewMode::Grid,
            focus: None,
            paused: false,
            volume: 80,
        };
        #[cfg(target_os = "linux")]
        let mut layout = Layout::default();
        layout.mode = crate::subscription::ViewMode::Grid;
        commands.send(Command::Layout(layout.clone()))?;
        let sequences = || {
            let s = shared.lock().unwrap();
            #[cfg(windows)]
            let values = s.received_streams.clone();
            #[cfg(target_os = "linux")]
            let values = s
                .frames
                .iter()
                .map(|(k, v)| (k.clone(), v.0))
                .collect::<std::collections::HashMap<_, _>>();
            values
        };
        wait(
            || {
                let s = shared.lock().unwrap();
                s.connected && s.quality.automatic_recovery.observed == 2
            },
            20,
            "Two observed streams missing",
        )?;
        wait(|| sequences().len() == 2, 10, "Two decoded streams missing")?;
        // Early samples can legitimately predate RTP negotiation; only a usable
        // exact mapping establishes readiness, never the cumulative miss count.
        wait(
            || shared.lock().unwrap().quality.automatic_recovery.mapped == 2,
            15,
            "RTP mapping unavailable; automatic repair must abstain",
        )?;
        let audio_before = shared.lock().unwrap().audio_received;
        let before = sequences();
        let keys: Vec<_> = before.keys().cloned().collect();
        let broken = keys[0].clone();
        let healthy = keys[1].clone();
        commands.send(Command::StallReader(broken.clone()))?;
        std::thread::sleep(Duration::from_secs(1));
        let frozen = sequences()[&broken];
        std::thread::sleep(Duration::from_secs(2));
        let during = sequences();
        anyhow::ensure!(
            during[&broken] == frozen,
            "Injected reader did not hold last frame"
        );
        anyhow::ensure!(
            during[&healthy] > before[&healthy],
            "Other stream stopped during fault"
        );
        wait(
            || shared.lock().unwrap().quality.automatic_recovery.actions >= 1,
            25,
            "Stalled reader was not detected",
        )?;
        wait(
            || sequences().get(&broken).is_some_and(|seq| *seq > frozen),
            10,
            "Repaired reader did not produce a new frame",
        )?;
        {
            let s = shared.lock().unwrap();
            anyhow::ensure!(
                s.connected && s.reception_restarts == 0,
                "Automatic repair reset the room or manual reception"
            );
            anyhow::ensure!(
                s.quality.automatic_recovery.actions == 1,
                "Recovered reader retried unnecessarily"
            );
        }
        anyhow::ensure!(
            shared.lock().unwrap().audio_received > audio_before,
            "Audio stopped during reader repair"
        );
        let recovered = sequences();
        anyhow::ensure!(
            recovered.len() == 2 && recovered[&healthy] > during[&healthy],
            "Healthy stream was replaced or interrupted"
        );
        layout.paused = true;
        commands.send(Command::Layout(layout.clone()))?;
        std::thread::sleep(Duration::from_secs(11));
        anyhow::ensure!(
            shared.lock().unwrap().quality.automatic_recovery.actions == 1,
            "Paused reception triggered automatic repair"
        );
        let paused = sequences()[&broken];
        layout.paused = false;
        commands.send(Command::Layout(layout))?;
        wait(
            || sequences().get(&broken).is_some_and(|seq| *seq > paused),
            10,
            "Resume did not produce a new frame",
        )?;
        // A second fault in the same episode exercises the video-only refresh path.
        let before_refresh = sequences();
        let audio_before = shared.lock().unwrap().audio_received;
        commands.send(Command::StallReader(broken.clone()))?;
        wait(
            || shared.lock().unwrap().quality.automatic_recovery.actions == 2,
            25,
            "Second reader fault did not refresh the video track",
        )?;
        wait(
            || {
                sequences()
                    .get(&broken)
                    .is_some_and(|seq| *seq > before_refresh[&broken])
            },
            10,
            "Video-only refresh did not resume frames",
        )?;
        anyhow::ensure!(
            sequences()[&healthy] > before_refresh[&healthy],
            "Refresh stopped the healthy stream"
        );
        anyhow::ensure!(
            shared.lock().unwrap().audio_received > audio_before,
            "Video refresh stopped audio"
        );
        anyhow::ensure!(
            shared.lock().unwrap().reception_restarts == 0,
            "Video refresh reset all reception"
        );
        drop(engine);
        #[cfg(windows)]
        wait(
            || livekit::webrtc::native::d3d11_live_surfaces() == 0,
            3,
            "Automatic recovery retained GPU surfaces",
        )?;
        #[cfg(target_os = "linux")]
        anyhow::ensure!(
            crate::linux::decoder_count() == 0,
            "Automatic recovery retained decoders"
        );
        eprintln!(
            "PASS: retained frame, reader replacement, video-only refresh, uninterrupted second stream/audio, paused reception and shutdown"
        );
        Ok(())
    }
}
