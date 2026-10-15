use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;

/// Deliberate interruption of an execution job.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum Interruption {
    /// The caller canceled this job or superseded its design revision.
    #[error("execution canceled")]
    Canceled,
    /// The execution deadline elapsed.
    #[error("execution timed out")]
    TimedOut,
}

#[derive(Default)]
struct Cancellation {
    canceled: AtomicBool,
    changed: Notify,
}

/// Cooperative native-solver cancellation and deadline shared with external process adapters.
#[derive(Clone, Default)]
pub struct RunControl {
    cancellation: Arc<Cancellation>,
    deadline: Option<Instant>,
}

impl RunControl {
    /// Create an execution with a monotonic bounded duration.
    pub fn bounded(timeout: Duration) -> Self {
        Self {
            cancellation: Arc::default(),
            deadline: Some(
                Instant::now()
                    .checked_add(timeout)
                    .unwrap_or_else(Instant::now),
            ),
        }
    }
    /// Cancel this execution and wake event-driven adapters.
    pub fn cancel(&self) {
        self.cancellation.canceled.store(true, Ordering::Release);
        self.cancellation.changed.notify_waiters();
    }
    /// Monotonic deadline for an adapter's timeout mechanism.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Check before each bounded unit of native solver work.
    pub fn checkpoint(&self) -> Result<(), Interruption> {
        if self.cancellation.canceled.load(Ordering::Acquire) {
            Err(Interruption::Canceled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Err(Interruption::TimedOut)
        } else {
            Ok(())
        }
    }
    /// Wait for a cancellation event without polling or sleep-and-retry.
    pub async fn cancelled(&self) {
        let changed = self.cancellation.changed.notified();
        let mut changed = std::pin::pin!(changed);
        changed.as_mut().enable();
        if self.cancellation.canceled.load(Ordering::Acquire) {
            return;
        }
        changed.await;
    }
}
