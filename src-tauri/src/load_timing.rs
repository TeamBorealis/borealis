//! PF1 (B-235): how long each content-loading step took, one compact info line per step.
//!
//! Lines look like `content load: user packs 812 ms (143 packs, 0 skipped)` and end with a
//! `content load: total ...` line, so a slow launch can be read from the log file (Settings >
//! About > logs, or `adb logcat -s RustStdoutStderr` on a phone).

use std::time::{Duration, Instant};

/// Times consecutive steps of one load.
#[derive(Debug)]
pub struct LoadTimer {
    started: Instant,
    last: Instant,
}

impl Default for LoadTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadTimer {
    /// Start timing now.
    #[must_use]
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            last: now,
        }
    }

    /// Log the step that just ended (`detail` may be empty) and start the next one.
    pub fn step(&mut self, label: &str, detail: &str) -> Duration {
        let now = Instant::now();
        let took = now - self.last;
        self.last = now;
        log_line(label, took, detail);
        took
    }

    /// Log the time since [`LoadTimer::new`].
    pub fn total(&self, detail: &str) -> Duration {
        let took = self.started.elapsed();
        log_line("total", took, detail);
        took
    }
}

fn log_line(label: &str, took: Duration, detail: &str) {
    let ms = took.as_millis();
    if detail.is_empty() {
        tracing::info!("content load: {label} {ms} ms");
    } else {
        tracing::info!("content load: {label} {ms} ms ({detail})");
    }
}
