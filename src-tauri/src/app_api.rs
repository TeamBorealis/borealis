//! App-level services: version and settings.

use crate::error::ApiResult;
use crate::picked_paths::PickPurpose;
use crate::settings::AppSettings;
use crate::state::AppState;
use serde_json::Value;
use std::path::Path;

impl AppState {
    /// Current settings.
    ///
    /// # Errors
    /// `internal` if the settings lock is poisoned.
    pub fn get_settings(&self) -> ApiResult<AppSettings> {
        self.settings()
    }

    /// Merge a partial settings patch, persist it and rebuild the engine when rules settings change.
    ///
    /// # Errors
    /// `invalid_argument` for invalid values, `io_error` if the file cannot be written.
    pub fn save_settings(&self, patch: &Value) -> ApiResult<AppSettings> {
        let current = self.settings()?;
        let next = current.patched(patch)?;
        next.save(&self.paths.settings_file())?;
        if next.engine_settings() != current.engine_settings() {
            let hub = self.hub()?.with_settings(&next);
            self.set_hub(hub)?;
        }
        self.set_settings(next.clone())?;
        Ok(next)
    }

    /// Set `ai.overridePath` from an `ai-override` pick: the token is spent, the program must
    /// validate (`claude`/`codex`, executable, not in a temp or the data folder) and `confirm`
    /// (the native dialog in the command) must accept the exact path. A declined confirmation
    /// returns the settings unchanged.
    ///
    /// # Errors
    /// `invalid_argument` for a bad token or a program that fails validation; `io_error` if the
    /// settings cannot be written.
    pub fn set_ai_override(
        &self,
        token: &str,
        data_dir: Option<&Path>,
        confirm: impl FnOnce(&Path) -> bool,
    ) -> ApiResult<AppSettings> {
        let picked = self.picked.take(token, PickPurpose::AiOverride)?;
        let roots = crate::ai_override::forbidden_roots(data_dir.or(Some(&self.paths.data)));
        let path = crate::ai_override::validate(&picked, &roots)?;
        if !confirm(&path) {
            return self.settings();
        }
        let mut next = self.settings()?;
        next.ai.override_path = Some(path.to_string_lossy().into_owned());
        next.save(&self.paths.settings_file())?;
        self.set_settings(next.clone())?;
        Ok(next)
    }
}
