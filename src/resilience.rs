//! Failure boundaries for desktop workers. Native faults still require process isolation.
use std::sync::{LockResult, Mutex, MutexGuard, TryLockError, TryLockResult};
use std::time::Duration;

/// Only for replaceable UI snapshots; never use for SDK/GPU resource locks.
pub struct StateMutex<T: Default>(Mutex<T>, std::sync::atomic::AtomicBool);
impl<T: Default> StateMutex<T> {
    pub fn new(value: T) -> Self {
        Self(Mutex::new(value), std::sync::atomic::AtomicBool::new(false))
    }
    pub fn was_reset(&self) -> bool {
        self.1.load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        Ok(match self.0.lock() {
            Ok(value) => value,
            Err(error) => {
                let mut value = error.into_inner();
                *value = T::default();
                self.1.store(true, std::sync::atomic::Ordering::Release);
                self.0.clear_poison();
                value
            }
        })
    }
    pub fn try_lock(&self) -> TryLockResult<MutexGuard<'_, T>> {
        match self.0.try_lock() {
            Ok(value) => Ok(value),
            Err(TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
            Err(TryLockError::Poisoned(error)) => {
                let mut value = error.into_inner();
                *value = T::default();
                self.1.store(true, std::sync::atomic::Ordering::Release);
                self.0.clear_poison();
                Ok(value)
            }
        }
    }
}
impl<T: Default> Default for StateMutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Discard the whole worker on panic; do not reuse partially changed native state.
/// Do not expose panic payloads: they may contain credentials or user data.
pub fn guard(work: impl FnOnce()) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).is_ok()
}
pub fn worker_failure() -> &'static str {
    crate::i18n::label(
        "A sessão encontrou uma falha interna. Tente conectar novamente.",
        "The session encountered an internal failure. Try connecting again.",
    )
}

/// Three total attempts, including the first. The caller must make this future cancelable.
pub async fn retry<T, E, F, Fut>(
    mut work: F,
    retryable: impl Fn(&E) -> bool,
    mut waiting: impl FnMut(u32, Duration),
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 1;
    loop {
        match work().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempt >= 3 || !retryable(&error) {
                    return Err(error);
                }
                let delay = Duration::from_secs(1 << (attempt - 1));
                attempt += 1;
                waiting(attempt, delay);
                tokio::time::sleep(delay).await;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_panic_discards_poisoned_snapshot_and_can_restart() {
        let state = StateMutex::new(vec![1]);
        assert!(!guard(|| {
            let mut s = state.lock().unwrap();
            s.push(2);
            panic!("injected");
        }));
        assert!(state.try_lock().unwrap().is_empty());
        assert!(state.was_reset());
        assert!(guard(|| state.lock().unwrap().push(3)));
        assert_eq!(*state.lock().unwrap(), vec![3]);
    }
    #[test]
    fn terminal_error_is_not_repeated_and_cancel_interrupts_backoff() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut count = 0;
            let result = retry(
                || {
                    count += 1;
                    std::future::ready(Err::<(), _>(false))
                },
                |e| *e,
                |_, _| panic!("permanent error retried"),
            )
            .await;
            assert_eq!(result, Err(false));
            assert_eq!(count, 1);
            let cancel = crate::room_transition::Cancellation::default();
            let stop = cancel.clone();
            let mut attempts = 0;
            let result = cancel
                .run(retry(
                    || {
                        attempts += 1;
                        std::future::ready(Err::<(), _>(true))
                    },
                    |e| *e,
                    move |_, _| stop.cancel(),
                ))
                .await;
            assert!(result.is_none());
            assert_eq!(attempts, 1);
        });
    }
    #[test]
    fn transient_errors_are_bounded_and_success_returns() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut count = 0;
            let result = retry(
                || {
                    count += 1;
                    std::future::ready(if count == 2 { Ok(7) } else { Err(()) })
                },
                |_| true,
                |_, _| {},
            )
            .await;
            assert_eq!(result, Ok(7));
            assert_eq!(count, 2);
            count = 0;
            let result = retry(
                || {
                    count += 1;
                    std::future::ready(Err::<(), _>(()))
                },
                |_| true,
                |_, _| {},
            )
            .await;
            assert!(result.is_err());
            assert_eq!(count, 3);
        });
    }
}
