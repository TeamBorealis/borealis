//! Caps a receiver enforces on a peer it does not trust, and the cancel signal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use borealis_bundle::Limits;
use tokio::sync::Notify;

/// Transfer caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferLimits {
    /// Bundle reader caps (per-file and total uncompressed size, entries, ratio).
    pub bundle: Limits,
    /// Largest bundle file accepted on the wire.
    pub max_bundle_bytes: u64,
    /// Largest single item (sum of its files).
    pub max_item_bytes: u64,
    /// Largest control message.
    pub max_control_bytes: usize,
    /// Bundle bytes per data frame (under the transport's 1 MiB frame cap).
    pub chunk_bytes: usize,
    /// Longest the receiving user may look at the preview.
    pub decision_timeout: Duration,
    /// Keep-alive interval while waiting for the decision (under the 60 s idle timeout).
    pub keepalive: Duration,
}

impl Default for TransferLimits {
    fn default() -> Self {
        let bundle = Limits::default();
        Self {
            bundle,
            // Stored or deflated entries plus zip headers stay below the uncompressed total.
            max_bundle_bytes: bundle.max_total_bytes + (16 << 20),
            max_item_bytes: bundle.max_file_bytes * 4,
            max_control_bytes: 8 << 20,
            chunk_bytes: 512 * 1024,
            decision_timeout: Duration::from_secs(5 * 60),
            keepalive: Duration::from_secs(15),
        }
    }
}

/// A cancel signal shared between the UI command and a running transfer.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<(AtomicBool, Notify)>);

impl Cancel {
    /// A fresh, not yet cancelled signal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel (idempotent).
    pub fn cancel(&self) {
        self.0 .0.store(true, Ordering::SeqCst);
        self.0 .1.notify_waiters();
    }

    /// True once cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.0 .0.load(Ordering::SeqCst)
    }

    /// Resolves when cancelled.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.0 .1.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}
