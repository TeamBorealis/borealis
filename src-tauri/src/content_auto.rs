//! Reference app "Automatic Updates": at startup (desktop only) the user's own update indexes are
//! checked in the background and verified updates are installed. Nothing here blocks startup:
//! the work runs on its own thread after a delay, uses the fetcher's existing timeouts, and
//! reports through `borealis://content-progress` plus `content_update_status`.

use crate::content_index::Fetcher;
use crate::content_packs::{progress, EventSink};
use crate::error::ApiResult;
use crate::ids::rfc3339;
use crate::settings_dto::{ContentUpdateState, ContentUpdateStatus};
use crate::state::AppState;
use std::time::{Duration, SystemTime};

/// Reference app waits this long after startup before checking.
pub const STARTUP_UPDATE_DELAY: Duration = Duration::from_secs(10);

/// Reference app's status-bar wording.
pub const MSG_CHECKING: &str = "Checking for content updates...";
/// Shown after at least one file was installed (Borealis hot-reloads, so no restart is needed).
pub const MSG_UPDATED: &str = "Your content files have been updated.";
/// Shown when nothing changed; the UI may render "Last checked ... at <checkedAt>".
pub const MSG_UP_TO_DATE: &str = "Your content files are up to date.";

/// Run `task` on its own thread (startup never waits for it).
pub fn spawn_startup_task<F>(task: F) -> std::thread::JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    std::thread::spawn(task)
}

impl AppState {
    /// The last automatic-update outcome.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn content_update_status(&self) -> ApiResult<ContentUpdateStatus> {
        Ok(self.content_update.lock()?.clone())
    }

    fn set_update_status(&self, status: ContentUpdateStatus) -> ContentUpdateStatus {
        if let Ok(mut s) = self.content_update.lock() {
            *s = status.clone();
        }
        status
    }

    /// Mark the startup update as scheduled (call before spawning the thread).
    pub fn mark_content_update_pending(&self) {
        let state = if self.settings().is_ok_and(|s| s.auto_content_update) {
            ContentUpdateState::Pending
        } else {
            ContentUpdateState::Disabled
        };
        self.set_update_status(ContentUpdateStatus {
            state,
            ..ContentUpdateStatus::default()
        });
    }

    /// Startup background work: load the secondary content folder, wait `delay`, then run the
    /// automatic content update. Meant for [`spawn_startup_task`].
    pub fn startup_content_tasks(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
        delay: Duration,
    ) -> ContentUpdateStatus {
        if self
            .settings()
            .is_ok_and(|s| s.secondary_content_dir.is_some())
        {
            if let Err(e) = self.sync_secondary_content(sink) {
                tracing::warn!("secondary content folder not loaded: {}", e.message);
            }
        }
        std::thread::sleep(delay);
        self.run_auto_content_update(fetcher, sink)
    }

    /// Check only the indexes the user added and install updates that pass the pinned-signature
    /// and SHA-256 checks (`update_indexes`). Off, or no indexes = nothing is fetched.
    pub fn run_auto_content_update(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
    ) -> ContentUpdateStatus {
        if !self.settings().is_ok_and(|s| s.auto_content_update) {
            return self.set_update_status(ContentUpdateStatus {
                state: ContentUpdateState::Disabled,
                ..ContentUpdateStatus::default()
            });
        }
        let total = self.content_prefs().indexes.len();
        if total == 0 {
            return self.set_update_status(ContentUpdateStatus {
                state: ContentUpdateState::Done,
                checked_at: Some(rfc3339(SystemTime::now())),
                ..ContentUpdateStatus::default()
            });
        }
        self.set_update_status(ContentUpdateStatus {
            state: ContentUpdateState::Running,
            message: MSG_CHECKING.into(),
            ..ContentUpdateStatus::default()
        });
        sink.content_progress(&progress("checking", 0, total, MSG_CHECKING));
        let status = match self.update_indexes(fetcher, sink) {
            Ok(r) => {
                let mut message = if r.updated > 0 {
                    MSG_UPDATED
                } else {
                    MSG_UP_TO_DATE
                }
                .to_string();
                if !r.failed.is_empty() {
                    message.push_str(&format!(" {} could not be updated.", r.failed.len()));
                }
                ContentUpdateStatus {
                    state: ContentUpdateState::Done,
                    message,
                    updated: r.updated,
                    failed: r.failed,
                    checked_at: Some(rfc3339(SystemTime::now())),
                }
            }
            Err(e) => {
                tracing::warn!("automatic content update failed: {}", e.message);
                ContentUpdateStatus {
                    state: ContentUpdateState::Failed,
                    message: format!("Checking for content updates failed: {}", e.message),
                    checked_at: Some(rfc3339(SystemTime::now())),
                    ..ContentUpdateStatus::default()
                }
            }
        };
        sink.content_progress(&progress("done", total, total, &status.message));
        self.set_update_status(status)
    }
}
