//! Codec-independent smoothing under the WebRTC congestion-control ceiling.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Manual,
    Automatic,
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => crate::i18n::label("Manual", "Manual"),
            Self::Automatic => crate::i18n::label("Automático", "Automatic"),
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub mode: Mode,
    pub ceiling: u32,
    pub network: Option<u64>,
    pub target: u32,
    pub reductions: u64,
    pub increases: u64,
}
pub struct Controller {
    recent: VecDeque<(Instant, u32)>,
    state: Snapshot,
    last_sample: Option<Instant>,
    stable_since: Option<Instant>,
    last_growth: Option<Instant>,
}
impl Controller {
    pub fn new(mode: Mode, ceiling: u32) -> Self {
        let ceiling = ceiling.clamp(64_000, 40_000_000);
        Self {
            recent: VecDeque::new(),
            state: Snapshot {
                mode,
                ceiling,
                target: ceiling,
                ..Default::default()
            },
            last_sample: None,
            stable_since: None,
            last_growth: None,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state
    }
    /// Call only for a new WebRTC rate sample. No timer invents bandwidth.
    /// 0..64 kbps is a pause: an encoder's minimum must never exceed WebRTC's grant.
    pub fn sample(&mut self, now: Instant, network: u64) -> u32 {
        if self
            .last_sample
            .is_some_and(|at| now.saturating_duration_since(at) > Duration::from_secs(3))
        {
            self.stable_since = None;
            self.last_growth = None;
            self.recent.clear();
        }
        self.last_sample = Some(now);
        self.state.network = Some(network);
        let limit = network.min(self.state.ceiling as u64) as u32;
        if self.state.mode == Mode::Automatic {
            self.recent
                .retain(|(at, _)| now.saturating_duration_since(*at) <= Duration::from_secs(10));
            if let Some((at, low)) = self
                .recent
                .back_mut()
                .filter(|(at, _)| now.saturating_duration_since(*at) < Duration::from_secs(1))
            {
                let _ = at;
                *low = (*low).min(limit);
            } else {
                self.recent.push_back((now, limit));
            }
        }
        let growth_limit = self
            .recent
            .iter()
            .map(|(_, limit)| *limit)
            .min()
            .unwrap_or(limit)
            .min(limit);
        let old = self.state.target;
        let target = if limit < 64_000 {
            self.stable_since = None;
            self.last_growth = None;
            self.recent.clear();
            0
        } else if self.state.mode == Mode::Manual {
            limit
        } else if old == 0 {
            self.stable_since = Some(now);
            self.last_growth = Some(now);
            limit.min(1_000_000)
        } else if limit < old {
            self.stable_since = None;
            self.last_growth = None;
            // React immediately to a hard network limit, retaining 10% headroom.
            (limit as u64 * 9 / 10).max(64_000) as u32
        } else {
            let since = *self.stable_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= Duration::from_secs(10)
                && self
                    .last_growth
                    .is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_secs(5))
            {
                self.last_growth = Some(now);
                old.saturating_add((old / 10).clamp(64_000, 2_000_000))
                    .min(growth_limit)
                    .max(old.min(limit))
            } else {
                old.min(limit)
            }
        };
        if target < old {
            self.state.reductions = self.state.reductions.saturating_add(1);
        } else if target > old {
            self.state.increases = self.state.increases.saturating_add(1);
        }
        self.state.target = target;
        target
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_bandwidth_recovers_gradually_and_never_exceeds_any_ceiling() {
        let start = Instant::now();
        for ceiling in [12_000_000, 25_000_000, 40_000_000] {
            let mut c = Controller::new(Mode::Automatic, ceiling);
            assert_eq!(c.sample(start, u64::MAX), ceiling);
            assert_eq!(
                c.sample(start + Duration::from_secs(1), 4_000_000),
                3_600_000
            );
            for second in 2..12 {
                assert_eq!(
                    c.sample(start + Duration::from_secs(second), ceiling as u64),
                    3_600_000
                );
            }
            let mut old = 3_600_000;
            for second in 12..100 {
                let target = c.sample(start + Duration::from_secs(second), ceiling as u64);
                assert!(target >= old && target <= ceiling);
                assert!(target - old <= 2_000_000);
                old = target;
            }
            assert!(old > 3_600_000);
            assert_eq!(c.snapshot().reductions, 1);
        }
    }
    #[test]
    fn zero_and_tiny_grants_pause_and_resume_without_overshooting() {
        for mode in [Mode::Manual, Mode::Automatic] {
            let mut c = Controller::new(mode, 25_000_000);
            let now = Instant::now();
            for (step, grant) in [0, 1, 63_999, 64_000, 65_000, 500_000, u64::MAX]
                .into_iter()
                .enumerate()
            {
                let target = c.sample(now + Duration::from_secs(step as u64), grant);
                assert!(target as u64 <= grant);
                assert!(target <= 25_000_000);
                if grant < 64_000 {
                    assert_eq!(target, 0);
                }
            }
        }
    }
    #[test]
    fn sparse_feedback_does_not_raise_bitrate_and_recent_low_grants_bound_growth() {
        let now = Instant::now();
        let mut c = Controller::new(Mode::Automatic, 25_000_000);
        c.sample(now, 4_000_000);
        let low = c.snapshot().target;
        for second in [5, 10, 15, 20, 25] {
            assert_eq!(c.sample(now + Duration::from_secs(second), 25_000_000), low);
        }
        for second in 26..100 {
            let grant = if second % 2 == 0 {
                4_000_000
            } else {
                3_000_000
            };
            assert!(c.sample(now + Duration::from_secs(second), grant) <= grant as u32);
        }
        assert_eq!(c.snapshot().reductions, 2);
        assert!(c.snapshot().target <= 3_000_000);
        assert!(c.recent.len() <= 11);
    }
    #[test]
    fn manual_preserves_immediate_network_control() {
        let mut c = Controller::new(Mode::Manual, 12_000_000);
        let now = Instant::now();
        for grant in [9_000_000, 3_000_000, 12_000_000, 0, 40_000_000] {
            assert_eq!(c.sample(now, grant), grant.min(12_000_000) as u32);
        }
    }
}
