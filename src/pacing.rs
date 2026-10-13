use anyhow::Result;
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, INFINITE,
            SetWaitableTimer, TIMER_ALL_ACCESS, WaitForSingleObject,
        },
    },
    core::PCWSTR,
};

/// Absolute deadlines, skipping missed ticks instead of accumulating a queue.
pub struct FramePacer {
    interval: Duration,
    next: Instant,
    #[cfg(windows)]
    timer: HANDLE,
}

impl FramePacer {
    /// Adjust the period without resetting the next deadline. Resetting the
    /// timer on each WebRTC rate update would create extra immediate ticks.
    pub fn set_fps(&mut self, fps: u32) -> Result<()> {
        anyhow::ensure!((1..=240).contains(&fps), "FPS must be 1..=240");
        self.interval = Duration::from_secs_f64(1.0 / fps as f64);
        self.next = self.next.min(Instant::now() + self.interval);
        Ok(())
    }
    pub fn new(fps: u32) -> Result<Self> {
        anyhow::ensure!((1..=240).contains(&fps), "FPS must be 1..=240");
        // SAFETY: unnamed process-private kernel object, owned by this pacer.
        // High resolution timers require Windows 10 1803 or newer.
        #[cfg(windows)]
        let timer = unsafe {
            CreateWaitableTimerExW(
                None,
                PCWSTR::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )?
        };
        Ok(Self {
            interval: Duration::from_secs_f64(1.0 / fps as f64),
            next: Instant::now(),
            #[cfg(windows)]
            timer,
        })
    }

    pub fn wait(&mut self) -> Result<()> {
        let now = Instant::now();
        if self.next > now {
            #[cfg(not(windows))]
            std::thread::sleep(self.next - now);
            #[cfg(windows)]
            unsafe {
                // SAFETY: relative due time in negative 100 ns units; valid handle,
                // no APC callback, and one wait on this capture thread.
                let due = -(((self.next - now).as_nanos() / 100).max(1) as i64);
                SetWaitableTimer(self.timer, &due, 0, None, None, false)?;
                anyhow::ensure!(
                    WaitForSingleObject(self.timer, INFINITE) == WAIT_OBJECT_0,
                    "High resolution timer wait failed"
                );
            }
        }
        let now = Instant::now();
        while self.next <= now {
            self.next += self.interval;
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for FramePacer {
    fn drop(&mut self) {
        // SAFETY: this pacer owns the handle and has no outstanding wait.
        unsafe {
            let _ = CloseHandle(self.timer);
        }
    }
}
