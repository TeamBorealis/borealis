//! Drive layer (AI15): Borealis driven end to end by an external program, through the CLI
//! (`borealis <tool> --library <dir>`) and the MCP write mode (`borealis --mcp --allow-write
//! --library <dir>`). Both front ends call [`Drive::call`] with the same tool names and JSON
//! arguments; every operation is a typed request handled by the same [`AppState`] functions the
//! Tauri commands call, so the engine validates each action exactly as it does for the UI.
//!
//! Scope: one library directory (the app's data-folder layout: `characters/`, `library/`,
//! settings). Every filesystem path in an argument must resolve inside it; the only network
//! access is the explicit content-import tool for an index URL. Changes are saved after every
//! successful edit (as the app's autosave does). Deterministic: no AI anywhere.

mod character;
pub mod cli;
mod content;
mod gear;
pub mod mcp;
pub mod specs;
mod view;

use crate::error::ApiError;
use crate::state::{AppPaths, AppState};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

/// A failed operation: a stable code (`invalid_argument`, `not_found`, `requirement_failed`,
/// `outside_library`, `unknown_tool`, ...) and a message for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveError {
    pub code: String,
    pub message: String,
}

impl DriveError {
    #[must_use]
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_argument", message)
    }

    /// True for errors in the request itself (usage), as opposed to a refused action.
    #[must_use]
    pub fn is_usage(&self) -> bool {
        self.code == "unknown_tool" || self.code == "usage"
    }
}

impl std::fmt::Display for DriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<ApiError> for DriveError {
    fn from(e: ApiError) -> Self {
        let code = serde_json::to_value(e.code)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "error".into());
        Self {
            code,
            message: e.message,
        }
    }
}

pub type DriveResult<T> = Result<T, DriveError>;

/// The headless app over one library directory.
pub struct Drive {
    app: AppState,
    root: PathBuf,
}

impl Drive {
    /// Open (creating it when missing) the library `library` with bundled content at
    /// `content_root`.
    ///
    /// # Errors
    /// When the folder cannot be created or the content cannot load.
    pub fn open(library: &Path, content_root: PathBuf) -> DriveResult<Self> {
        std::fs::create_dir_all(library)
            .map_err(|e| DriveError::new("io_error", format!("cannot create the library: {e}")))?;
        let root = library
            .canonicalize()
            .map_err(|e| DriveError::new("io_error", format!("cannot open the library: {e}")))?;
        let app = AppState::new(AppPaths {
            content_root,
            data: root.clone(),
        })?;
        Ok(Self { app, root })
    }

    /// The app state (tests and the front ends read through it).
    #[must_use]
    pub fn app(&self) -> &AppState {
        &self.app
    }

    /// The canonical library directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a path argument (relative paths are relative to the library) and refuse anything
    /// outside the library, `..` components and symlinks leading out included. The file itself
    /// need not exist.
    ///
    /// # Errors
    /// `outside_library` or `invalid_argument`.
    pub fn scoped_path(&self, raw: &str) -> DriveResult<PathBuf> {
        if raw.trim().is_empty() {
            return Err(DriveError::invalid("the path is empty"));
        }
        let path = Path::new(raw);
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(outside(raw));
        }
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let mut existing = joined.as_path();
        let mut rest = Vec::new();
        while !existing.exists() {
            let (Some(name), Some(parent)) = (existing.file_name(), existing.parent()) else {
                return Err(outside(raw));
            };
            rest.push(name.to_os_string());
            existing = parent;
        }
        let mut resolved = existing.canonicalize().map_err(|_| outside(raw))?;
        if !resolved.starts_with(&self.root) {
            return Err(outside(raw));
        }
        for part in rest.iter().rev() {
            resolved.push(part);
        }
        Ok(resolved)
    }

    /// Run one tool by name with JSON arguments (an object).
    ///
    /// # Errors
    /// `unknown_tool`, `invalid_argument` for bad arguments, or the engine's refusal.
    pub fn call(&self, name: &str, args: &Value) -> DriveResult<Value> {
        if !args.is_object() {
            return Err(DriveError::invalid("arguments must be a JSON object"));
        }
        if let Some(out) = character::dispatch(self, name, args) {
            return out;
        }
        if let Some(out) = gear::dispatch(self, name, args) {
            return out;
        }
        if let Some(out) = content::dispatch(self, name, args) {
            return out;
        }
        Err(DriveError::new(
            "unknown_tool",
            format!("unknown tool: {name} (see `borealis help`)"),
        ))
    }

    /// Open the character when it is not open yet (tools address saved characters by id).
    fn ensure_open(&self, id: &str) -> DriveResult<()> {
        self.app.load_character(id)?;
        Ok(())
    }

    /// Save an open character after a successful change.
    fn persist(&self, id: &str) -> DriveResult<crate::dto::CharacterSnapshot> {
        Ok(self.app.save_character(id)?)
    }

    /// Run one edit (possibly several engine calls) on a character: saved when it succeeds; on a
    /// refusal the open copy is discarded, so the character stays exactly as last saved.
    fn edit(
        &self,
        id: &str,
        f: impl FnOnce(&AppState) -> Result<(), ApiError>,
    ) -> DriveResult<crate::dto::CharacterSnapshot> {
        self.ensure_open(id)?;
        if let Err(e) = f(&self.app) {
            // Best effort: the refusal is what the caller needs to see.
            let _ = self.app.close_character(id, true);
            return Err(e.into());
        }
        self.persist(id)
    }
}

fn outside(raw: &str) -> DriveError {
    DriveError::new(
        "outside_library",
        format!("refused: {raw} is outside the library folder"),
    )
}

/// Deserialize tool arguments into their typed request.
fn parse<T: DeserializeOwned>(args: &Value) -> DriveResult<T> {
    serde_json::from_value(args.clone()).map_err(|e| DriveError::invalid(format!("{e}")))
}

/// `"2014"`/`"2024"` (or the long `dnd2014`/`dnd2024` forms) to a ruleset.
fn ruleset(text: &str) -> DriveResult<borealis_content::Ruleset> {
    match text.trim().to_ascii_lowercase().as_str() {
        "2014" | "dnd2014" => Ok(borealis_content::Ruleset::Dnd2014),
        "2024" | "dnd2024" => Ok(borealis_content::Ruleset::Dnd2024),
        other => Err(DriveError::invalid(format!(
            "unknown ruleset {other}; use 2014 or 2024"
        ))),
    }
}

/// Deserialize a snake_case enum from its wire string.
fn wire_enum<T: DeserializeOwned>(text: &str, what: &str) -> DriveResult<T> {
    serde_json::from_value(Value::String(text.to_string()))
        .map_err(|_| DriveError::invalid(format!("unknown {what}: {text}")))
}

#[cfg(test)]
mod tests;
