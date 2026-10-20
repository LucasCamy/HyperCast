//! Bounded, content-free session evidence. CPU boundary timings, never photon latency.
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
const BINS: usize = 32;
#[derive(Clone, Copy)]
#[repr(usize)]
pub enum Stage {
    CaptureAcquire,
    EncodeCall,
    SendSubmit,
    SourceToEncoded,
    EncoderInputToEncoded,
    RenderPrepare,
    PresentCall,
    ReaderToRender,
}
impl Stage {
    const ALL: [Self; 8] = [
        Self::CaptureAcquire,
        Self::EncodeCall,
        Self::SendSubmit,
        Self::SourceToEncoded,
        Self::EncoderInputToEncoded,
        Self::RenderPrepare,
        Self::PresentCall,
        Self::ReaderToRender,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::CaptureAcquire => "capture_acquire_call",
            Self::EncodeCall => "encode_call",
            Self::SendSubmit => "encoded_packet_submit",
            Self::SourceToEncoded => "gst_source_to_encoded_sink",
            Self::EncoderInputToEncoded => "gst_encoder_input_to_encoded_sink",
            Self::RenderPrepare => "render_prepare_cpu",
            Self::PresentCall => "present_call",
            Self::ReaderToRender => "reader_to_render_start",
        }
    }
}
#[derive(Default)]
struct Histogram {
    bins: [AtomicU64; BINS],
    count: AtomicU64,
    nanos: AtomicU64,
    maximum_us: AtomicU64,
    last_us: AtomicU64,
}
impl Histogram {
    fn record(&self, d: Duration, at: u64) {
        let us = d.as_micros().min(u64::MAX as u128) as u64;
        let bin = if us <= 1 {
            0
        } else {
            (64 - (us - 1).leading_zeros() as usize).min(BINS - 1)
        };
        self.bins[bin].fetch_add(1, Ordering::Relaxed);
        self.nanos
            .fetch_add(d.as_nanos().min(u64::MAX as u128) as u64, Ordering::Relaxed);
        self.maximum_us.fetch_max(us, Ordering::Relaxed);
        self.last_us.store(at.saturating_add(1), Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }
    fn report(&self, now: u64) -> serde_json::Value {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return serde_json::Value::Null;
        }
        let bins = self.bins.each_ref().map(|v| v.load(Ordering::Relaxed));
        let total = bins.iter().sum::<u64>();
        let percentile = |p: u64| {
            let wanted = total.saturating_mul(p).div_ceil(100).max(1);
            let mut n = 0;
            for (i, c) in bins.iter().enumerate() {
                n += c;
                if n >= wanted {
                    return if i == BINS - 1 {
                        self.maximum_us.load(Ordering::Relaxed) as f64 / 1000.0
                    } else {
                        (1_u64 << i) as f64 / 1000.0
                    };
                }
            }
            0.0
        };
        serde_json::json!({"samples":count,"mean_ms":self.nanos.load(Ordering::Relaxed) as f64/count as f64/1_000_000.0,"p50_upper_ms":percentile(50),"p95_upper_ms":percentile(95),"max_ms":self.maximum_us.load(Ordering::Relaxed) as f64/1000.0,"last_sample_age_ms":now.saturating_sub(self.last_us.load(Ordering::Relaxed).saturating_sub(1)) as f64/1000.0})
    }
}
#[derive(Clone, Copy)]
pub enum EventKind {
    Connecting,
    ConnectRetry,
    SessionFailed,
    Connected,
    Reconnecting,
    ConnectionRestored,
    Disconnected,
    PublicationStarted,
    PublicationStopped,
    PublicationFailed,
    SourceChanged,
    SendPaused,
    SendResumed,
    RateRejected,
    RateAccepted,
    BitrateChanged,
    ReceiverRecovery,
    ManualReceptionReset,
}
impl EventKind {
    fn name(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::ConnectRetry => "connect_retry",
            Self::SessionFailed => "session_failed",
            Self::Connected => "connected",
            Self::Reconnecting => "reconnecting",
            Self::ConnectionRestored => "connection_restored",
            Self::Disconnected => "disconnected",
            Self::PublicationStarted => "publication_started",
            Self::PublicationStopped => "publication_stopped",
            Self::PublicationFailed => "publication_failed",
            Self::SourceChanged => "source_changed",
            Self::SendPaused => "send_paused",
            Self::SendResumed => "send_resumed",
            Self::RateRejected => "rate_rejected",
            Self::RateAccepted => "rate_accepted",
            Self::BitrateChanged => "bitrate_target_changed",
            Self::ReceiverRecovery => "receiver_recovery",
            Self::ManualReceptionReset => "manual_reception_reset",
        }
    }
}
#[derive(Default, Clone, Copy)]
pub struct Observation {
    pub connected: bool,
    pub reconnecting: bool,
    pub publishing: bool,
    pub paused: bool,
    pub rejected: bool,
    pub bitrate: u32,
    pub repairs: u64,
}
#[derive(Default)]
struct History {
    rows: VecDeque<(u64, EventKind, Option<u32>)>,
    previous: Observation,
}
struct Inner {
    start: Instant,
    timings: [Histogram; 8],
    history: Mutex<History>,
    last_observe: AtomicU64,
    correlation_misses: AtomicU64,
}
#[derive(Clone)]
pub struct Evidence(Arc<Inner>);
impl Default for Evidence {
    fn default() -> Self {
        Self(Arc::new(Inner {
            start: Instant::now(),
            timings: std::array::from_fn(|_| Default::default()),
            history: Mutex::new(Default::default()),
            last_observe: AtomicU64::new(0),
            correlation_misses: AtomicU64::new(0),
        }))
    }
}
impl Evidence {
    fn now(&self) -> u64 {
        self.0.start.elapsed().as_micros().min(u64::MAX as u128) as u64
    }
    pub fn record(&self, stage: Stage, duration: Duration) {
        self.0.timings[stage as usize].record(duration, self.now());
    }
    fn push(history: &mut History, at: u64, kind: EventKind, value: Option<u32>) {
        if history.rows.len() == 64 {
            history.rows.pop_front();
        }
        history.rows.push_back((at / 1000, kind, value));
    }
    pub fn event(&self, kind: EventKind) {
        if let Ok(mut h) = self.0.history.lock() {
            Self::push(&mut h, self.now(), kind, None);
        }
    }
    pub fn observe(&self, next: Observation) {
        let now = self.now();
        let old = self.0.last_observe.load(Ordering::Relaxed);
        if old != 0 && now.saturating_sub(old) < 250_000 {
            return;
        }
        self.0.last_observe.store(now.max(1), Ordering::Relaxed);
        if let Ok(mut h) = self.0.history.lock() {
            let p = h.previous;
            h.previous = next;
            for (changed, on, yes, no) in [
                (
                    p.connected != next.connected,
                    next.connected,
                    EventKind::Connected,
                    EventKind::Disconnected,
                ),
                (
                    p.reconnecting != next.reconnecting && (next.reconnecting || next.connected),
                    next.reconnecting,
                    EventKind::Reconnecting,
                    EventKind::ConnectionRestored,
                ),
                (
                    p.publishing != next.publishing,
                    next.publishing,
                    EventKind::PublicationStarted,
                    EventKind::PublicationStopped,
                ),
                (
                    p.paused != next.paused && (next.paused || next.publishing),
                    next.paused,
                    EventKind::SendPaused,
                    EventKind::SendResumed,
                ),
                (
                    p.rejected != next.rejected && (next.rejected || next.publishing),
                    next.rejected,
                    EventKind::RateRejected,
                    EventKind::RateAccepted,
                ),
            ] {
                if changed {
                    Self::push(&mut h, now, if on { yes } else { no }, None);
                }
            }
            if p.bitrate != next.bitrate {
                Self::push(
                    &mut h,
                    now,
                    EventKind::BitrateChanged,
                    Some(next.bitrate.min(45_000_000)),
                );
            }
            if next.repairs > p.repairs {
                Self::push(
                    &mut h,
                    now,
                    EventKind::ReceiverRecovery,
                    Some(next.repairs.min(u32::MAX as u64) as u32),
                );
            }
        }
    }
    pub fn missing_correlation(&self) {
        self.0.correlation_misses.fetch_add(1, Ordering::Relaxed);
    }
    pub fn report(&self) -> serde_json::Value {
        let now = self.now();
        let stages = Stage::ALL
            .into_iter()
            .map(|s| (s.name().into(), self.0.timings[s as usize].report(now)))
            .collect::<serde_json::Map<_, _>>();
        let events = self
            .0
            .history
            .lock()
            .map(|h| {
                h.rows
                    .iter()
                    .map(
                        |(ms, k, v)| serde_json::json!({"elapsed_ms":ms,"kind":k.name(),"value":v}),
                    )
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        serde_json::json!({"scope":"current_session_cumulative_cpu_boundaries","histogram":"power_of_two_microseconds_upper_bounds","concurrent_snapshot_approximate":true,"stages":stages,"correlation_misses":self.0.correlation_misses.load(Ordering::Relaxed),"events":events,"limits":{"events":64,"histogram_bins_per_stage":32,"pending_pts_per_boundary":64},"physical_scanout_measured":false})
    }
}
impl std::fmt::Debug for Evidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Evidence(bounded)")
    }
}
/// Stores timestamps only, never frames. Exact PTS matching; absent matches stay unknown.
#[derive(Default)]
pub struct Correlation {
    source: VecDeque<(u64, Option<Instant>)>,
    input: VecDeque<(u64, Option<Instant>)>,
}
impl Correlation {
    pub fn input(&mut self, encoder: bool, pts: u64, at: Instant) {
        let q = if encoder {
            &mut self.input
        } else {
            &mut self.source
        };
        if let Some((_, at)) = q.iter_mut().find(|(key, _)| *key == pts) {
            *at = None;
            return;
        }
        if q.len() == 64 {
            q.pop_front();
        }
        q.push_back((pts, Some(at)));
    }
    pub fn output(&mut self, pts: u64, at: Instant, e: &Evidence) {
        for (q, stage) in [
            (&mut self.source, Stage::SourceToEncoded),
            (&mut self.input, Stage::EncoderInputToEncoded),
        ] {
            if let Some(index) = q.iter().position(|(key, _)| *key == pts) {
                if let Some((_, Some(start))) = q.remove(index) {
                    e.record(stage, at.saturating_duration_since(start));
                } else {
                    e.missing_correlation();
                }
            } else {
                e.0.correlation_misses.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_is_bounded_and_missing_stages_stay_null() {
        let e = Evidence::default();
        for _ in 0..1000 {
            e.event(EventKind::SourceChanged);
            e.record(Stage::EncodeCall, Duration::from_micros(1500));
        }
        let r = e.report();
        assert_eq!(r["events"].as_array().unwrap().len(), 64);
        assert_eq!(r["stages"]["encode_call"]["samples"], 1000);
        assert_eq!(r["stages"]["encode_call"]["p95_upper_ms"], 2.048);
        assert!(r["stages"]["capture_acquire_call"].is_null());
        assert!(r.to_string().len() < 16384);
    }
    #[test]
    fn pts_matching_never_substitutes_a_different_frame() {
        let mut c = Correlation::default();
        let e = Evidence::default();
        let now = Instant::now();
        for pts in 0..1000 {
            c.input(false, pts, now);
            c.input(true, pts, now);
        }
        assert_eq!(c.source.len(), 64);
        c.output(0, now, &e);
        assert!(e.report()["stages"]["gst_source_to_encoded_sink"].is_null());
        c.output(999, now + Duration::from_millis(3), &e);
        assert_eq!(
            e.report()["stages"]["gst_encoder_input_to_encoded_sink"]["mean_ms"],
            3.0
        );
        assert_eq!(e.report()["correlation_misses"], 2);
        c.input(false, 1001, now);
        c.input(false, 1001, now + Duration::from_millis(1));
        c.output(1001, now + Duration::from_millis(3), &e);
        assert_eq!(
            e.report()["stages"]["gst_source_to_encoded_sink"]["samples"],
            1
        );
        assert_eq!(e.report()["correlation_misses"], 4);
    }
    #[test]
    #[ignore = "Measures local timing instrumentation overhead; no capture or network"]
    fn measure_instrumentation_overhead() {
        let e = Evidence::default();
        let c = Mutex::new(Correlation::default());
        let start = Instant::now();
        for pts in 0..100_000 {
            let now = Instant::now();
            c.lock().unwrap().input(false, pts, now);
            c.lock().unwrap().input(true, pts, now);
            c.lock().unwrap().output(pts, now, &e);
            e.record(Stage::CaptureAcquire, Duration::from_micros(1));
            e.record(Stage::EncodeCall, Duration::from_micros(1));
            e.record(Stage::SendSubmit, Duration::from_micros(1));
        }
        let us = start.elapsed().as_secs_f64() * 1_000_000.0 / 100_000.0;
        println!(
            "Evidence overhead: {us:.3} us per five-stage synthetic frame; fraction_120fps_budget={:.6}",
            us / 8333.333
        );
        assert!(
            us < 83.333,
            "Instrumentation exceeded 1% of a 120 FPS frame budget on this host"
        );
    }
}
