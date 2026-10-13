//! Startup progress: the staged loading sequence shown by the splash screen.
//!
//! The Tauri-free core lives here (stages, percentages, messages, the replayable [`Startup`] tracker
//! and the minimum-hold rule) so it can be unit-tested with a recording emitter; the window and event
//! wiring is in `startup_boot`.

use crate::state::LoadStep;
use serde::Serialize;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// Event carrying a [`StartupProgress`] to the splash (desktop) or the startup overlay (mobile).
pub const EVENT_PROGRESS: &str = "startup-progress";
/// Event sent to the main webview once the backend state is managed and commands may be called.
pub const EVENT_DONE: &str = "startup-done";
/// Emitted by the splash webview once it is listening.
pub const EVENT_SPLASH_READY: &str = "splash-ready";
/// Emitted by the main webview once it is listening for startup events.
pub const EVENT_MAIN_LOADED: &str = "main-loaded";
/// Emitted by the main webview after its first real render.
pub const EVENT_MAIN_READY: &str = "main-ready";

/// The splash stays up at least this long so a fast start does not flicker.
pub const MIN_SPLASH: Duration = Duration::from_millis(1200);
/// How long the main window is waited for after startup finished before it is shown anyway.
pub const MAIN_READY_TIMEOUT: Duration = Duration::from_secs(5);

/// One step of the startup sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Creating the data folders.
    Directories,
    /// Reading the settings file.
    Settings,
    /// Loading one content pack (`custom` = a pack from the user library).
    Content {
        /// Pack folder name.
        name: String,
        /// Zero-based position among all packs being loaded.
        index: usize,
        /// Number of packs being loaded.
        total: usize,
        /// The pack comes from the user library rather than the bundle.
        custom: bool,
    },
    /// Building the rules engine over the loaded content.
    Engine,
    /// Verifying the AI profile (desktop).
    AiProfile,
    /// Everything loaded; the main window is about to open.
    Starting,
}

impl From<LoadStep> for Stage {
    fn from(step: LoadStep) -> Self {
        match step {
            LoadStep::Directories => Self::Directories,
            LoadStep::Settings => Self::Settings,
            LoadStep::Pack {
                name,
                index,
                total,
                custom,
            } => Self::Content {
                name,
                index,
                total,
                custom,
            },
            LoadStep::Engine => Self::Engine,
        }
    }
}

/// Payload of [`EVENT_PROGRESS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartupProgress {
    /// Plain-English status line.
    pub message: String,
    /// 0 to 100.
    pub percent: u8,
    /// Startup failed; `message` says what and `details` why.
    pub failed: bool,
    /// Running on a platform without a separate splash window (the UI shows an overlay instead).
    pub mobile: bool,
    /// Technical detail for the failure case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

const MOBILE: bool = cfg!(any(target_os = "android", target_os = "ios"));
const PCT_DIRECTORIES: u8 = 4;
const PCT_SETTINGS: u8 = 10;
const PCT_CONTENT_START: usize = 12;
const PCT_CONTENT_SPAN: usize = 66;
const PCT_ENGINE: u8 = 82;
const PCT_AI: u8 = 92;

/// The progress record announcing `stage`.
#[must_use]
pub fn progress_for(stage: &Stage) -> StartupProgress {
    let (message, percent) = match stage {
        Stage::Directories => ("Initializing directories".to_string(), PCT_DIRECTORIES),
        Stage::Settings => ("Loading settings".to_string(), PCT_SETTINGS),
        Stage::Content {
            name,
            index,
            total,
            custom,
        } => {
            let total = (*total).max(1);
            let pct = PCT_CONTENT_START + index * PCT_CONTENT_SPAN / total;
            let what = if *custom {
                "custom files"
            } else {
                "content packs"
            };
            (
                format!("Loading {what} ({} of {total}): {name}", index + 1),
                u8::try_from(pct).unwrap_or(PCT_ENGINE),
            )
        }
        Stage::Engine => ("Preparing the rules engine".to_string(), PCT_ENGINE),
        Stage::AiProfile => ("Verifying the AI profile".to_string(), PCT_AI),
        Stage::Starting => ("Starting Borealis".to_string(), 100),
    };
    StartupProgress {
        message,
        percent,
        failed: false,
        mobile: MOBILE,
        details: None,
    }
}

/// The record announcing a failed startup.
#[must_use]
pub fn failure(what: &str, details: &str, percent: u8) -> StartupProgress {
    StartupProgress {
        message: what.to_string(),
        percent,
        failed: true,
        mobile: MOBILE,
        details: Some(details.to_string()),
    }
}

/// How much longer to hold the splash given the time already spent: the rest of `min`, never more.
#[must_use]
pub fn hold_remaining(elapsed: Duration, min: Duration) -> Duration {
    min.saturating_sub(elapsed)
}

/// Delivers startup events to the UI.
pub trait StartupEmitter: Send + Sync {
    /// Broadcast a progress record.
    fn progress(&self, progress: &StartupProgress);
    /// Tell the main webview that commands may now be called.
    fn done(&self);
}

#[derive(Debug, Default)]
struct Inner {
    last: Option<StartupProgress>,
    done: bool,
    main_ready: bool,
}

/// Tracks startup so late-attaching webviews can be brought up to date (events are not queued).
pub struct Startup {
    emitter: Box<dyn StartupEmitter>,
    inner: Mutex<Inner>,
    main_ready_signal: Condvar,
}

impl Startup {
    /// A tracker emitting through `emitter`.
    #[must_use]
    pub fn new(emitter: Box<dyn StartupEmitter>) -> Self {
        Self {
            emitter,
            inner: Mutex::new(Inner::default()),
            main_ready_signal: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Announce `stage`.
    pub fn report(&self, stage: &Stage) {
        self.publish(progress_for(stage));
    }

    /// Announce a failure; the splash switches to its error state.
    pub fn fail(&self, what: &str, details: &str) {
        let percent = self.lock().last.as_ref().map_or(0, |p| p.percent);
        self.publish(failure(what, details, percent));
    }

    fn publish(&self, progress: StartupProgress) {
        self.lock().last = Some(progress.clone());
        self.emitter.progress(&progress);
    }

    /// Mark startup finished and tell the main webview.
    pub fn complete(&self) {
        self.lock().done = true;
        self.emitter.done();
    }

    /// Startup finished successfully.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.lock().done
    }

    /// Re-send the latest progress (and the completion) to a webview that attached late.
    pub fn replay(&self) {
        let (last, done) = {
            let inner = self.lock();
            (inner.last.clone(), inner.done)
        };
        if let Some(p) = last {
            self.emitter.progress(&p);
        }
        if done {
            self.emitter.done();
        }
    }

    /// The main webview rendered.
    pub fn mark_main_ready(&self) {
        self.lock().main_ready = true;
        self.main_ready_signal.notify_all();
    }

    /// Wait until the main webview rendered; false on timeout.
    #[must_use]
    pub fn wait_main_ready(&self, timeout: Duration) -> bool {
        let guard = self.lock();
        let (guard, _) = self
            .main_ready_signal
            .wait_timeout_while(guard, timeout, |i| !i.main_ready)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.main_ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<String>>);

    struct Rec(Arc<Recorder>);

    impl StartupEmitter for Rec {
        fn progress(&self, p: &StartupProgress) {
            let tag = if p.failed { "fail" } else { "progress" };
            self.0
                 .0
                .lock()
                .unwrap()
                .push(format!("{tag} {} {}", p.percent, p.message));
        }
        fn done(&self) {
            self.0 .0.lock().unwrap().push("done".to_string());
        }
    }

    fn tracker() -> (Startup, Arc<Recorder>) {
        let rec = Arc::new(Recorder::default());
        (Startup::new(Box::new(Rec(Arc::clone(&rec)))), rec)
    }

    fn pack(name: &str, index: usize, total: usize, custom: bool) -> Stage {
        Stage::Content {
            name: name.into(),
            index,
            total,
            custom,
        }
    }

    #[test]
    fn full_sequence_is_monotone_and_ends_at_one_hundred() {
        let stages = [
            Stage::Directories,
            Stage::Settings,
            pack("core", 0, 3, false),
            pack("srd-5.1", 1, 3, false),
            pack("mine", 2, 3, true),
            Stage::Engine,
            Stage::AiProfile,
            Stage::Starting,
        ];
        let pct: Vec<u8> = stages.iter().map(|s| progress_for(s).percent).collect();
        assert!(pct.windows(2).all(|w| w[0] < w[1]), "{pct:?}");
        assert_eq!(pct.last(), Some(&100));
        assert!(pct[0] > 0);
    }

    #[test]
    fn messages_are_plain_english() {
        assert_eq!(
            progress_for(&Stage::Directories).message,
            "Initializing directories"
        );
        assert_eq!(
            progress_for(&pack("srd-5.1", 1, 5, false)).message,
            "Loading content packs (2 of 5): srd-5.1"
        );
        assert_eq!(
            progress_for(&pack("mine", 0, 1, true)).message,
            "Loading custom files (1 of 1): mine"
        );
        assert_eq!(progress_for(&Stage::Starting).message, "Starting Borealis");
    }

    #[test]
    fn many_packs_stay_below_the_engine_step() {
        let last = progress_for(&pack("p", 49, 50, false)).percent;
        assert!(last < PCT_ENGINE);
    }

    #[test]
    fn hold_never_exceeds_the_minimum_and_is_zero_when_slow() {
        assert_eq!(hold_remaining(Duration::ZERO, MIN_SPLASH), MIN_SPLASH);
        assert_eq!(
            hold_remaining(Duration::from_millis(300), MIN_SPLASH),
            Duration::from_millis(900)
        );
        assert_eq!(
            hold_remaining(Duration::from_secs(4), MIN_SPLASH),
            Duration::ZERO
        );
    }

    #[test]
    fn report_emits_in_order_and_complete_signals_done() {
        let (s, rec) = tracker();
        s.report(&Stage::Directories);
        s.report(&Stage::Starting);
        assert!(!s.is_done());
        s.complete();
        assert!(s.is_done());
        let log = rec.0.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "progress 4 Initializing directories",
                "progress 100 Starting Borealis",
                "done"
            ]
        );
    }

    #[test]
    fn replay_resends_latest_progress_and_done_only_once_finished() {
        let (s, rec) = tracker();
        s.replay();
        assert!(rec.0.lock().unwrap().is_empty());
        s.report(&Stage::Settings);
        s.replay();
        s.complete();
        s.replay();
        let log = rec.0.lock().unwrap().clone();
        assert_eq!(log[1], "progress 10 Loading settings");
        assert_eq!(&log[2..], ["done", "progress 10 Loading settings", "done"]);
    }

    #[test]
    fn failure_keeps_the_last_percent_and_carries_details() {
        let (s, rec) = tracker();
        s.report(&Stage::Settings);
        s.fail("Borealis could not load its data", "disk full");
        let log = rec.0.lock().unwrap().clone();
        assert_eq!(log[1], "fail 10 Borealis could not load its data");
        assert!(!s.is_done());
    }

    #[test]
    fn main_ready_wait_times_out_then_succeeds() {
        let (s, _) = tracker();
        assert!(!s.wait_main_ready(Duration::from_millis(10)));
        s.mark_main_ready();
        assert!(s.wait_main_ready(Duration::from_millis(10)));
    }
}
