//! The typed error crossing the IPC boundary: serialised as `{ code, message, details? }`.

use borealis_character::{FormatError, ImportError};
use borealis_content::ContentError;
use borealis_engine::commands::CommandError;
use serde::Serialize;
use serde_json::Value;

/// Machine-readable error class (matches `ApiErrorCode` in the UI contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The character, slot, element or file does not exist.
    NotFound,
    /// Malformed or out-of-range input.
    InvalidArgument,
    /// A point buy (or similar) budget would be exceeded.
    BudgetExceeded,
    /// A game rule or prerequisite refuses the change.
    RequirementFailed,
    /// The AI provider is not available.
    AiUnavailable,
    /// Imported or attached content would run on Codex (shell tool); the user must consent first.
    ConsentRequired,
    /// Content packs failed to load or validate.
    ContentError,
    /// Reading or writing a file failed.
    IoError,
    /// A bug or poisoned state; details are logged, not shown.
    Internal,
}

/// Error returned by every command.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct ApiError {
    /// Error class.
    pub code: ErrorCode,
    /// User-presentable English message (never contains paths or stack traces).
    pub message: String,
    /// Optional structured details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// Result alias used by the services and commands.
pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    /// Imported or attached content would reach a CLI with a shell tool; the user must agree first.
    pub fn consent_required(backend: &str) -> Self {
        let mut e = Self::new(
            ErrorCode::ConsentRequired,
            format!(
                "This content comes from outside Borealis, and {backend} can run commands that read files on this computer if the content tells it to. Use Claude for imports, or continue with {backend} if you accept that risk."
            ),
        );
        e.details = Some(serde_json::json!({ "backend": backend }));
        e
    }

    /// Map an AI engine error (consent refusals get their own code).
    pub fn from_ai(e: &borealis_ai::AiError) -> Self {
        match e {
            borealis_ai::AiError::UntrustedInputNeedsConsent { backend } => {
                Self::consent_required(backend)
            }
            other => Self::new(ErrorCode::AiUnavailable, other.to_string()),
        }
    }

    /// Build an error.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Missing character, slot or file.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// Bad input.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    /// Rule refusal.
    pub fn requirement(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::RequirementFailed, message)
    }

    /// Unexpected failure: the cause is logged, the UI sees a generic message.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        tracing::error!("internal error: {cause}");
        Self::new(ErrorCode::Internal, "An internal error occurred.")
    }

    /// A file operation failed; the path stays in the log only.
    pub fn io(what: &str, cause: impl std::fmt::Display) -> Self {
        tracing::warn!("io error while {what}: {cause}");
        Self::new(ErrorCode::IoError, format!("Could not {what}."))
    }
}

impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        let message = e.to_string();
        let code = match &e {
            CommandError::UnknownSlot | CommandError::UnknownElement(_) => ErrorCode::NotFound,
            CommandError::Ability(m) if m.contains("budget") => ErrorCode::BudgetExceeded,
            CommandError::Ability(_) => ErrorCode::InvalidArgument,
            CommandError::NotAllowed(_)
            | CommandError::Level(_)
            | CommandError::Inventory(_)
            | CommandError::Money(_)
            | CommandError::Spell(_)
            | CommandError::Version(_) => ErrorCode::RequirementFailed,
        };
        Self::new(code, capitalise(&message))
    }
}

impl From<FormatError> for ApiError {
    fn from(e: FormatError) -> Self {
        match e {
            FormatError::Io { source, .. } => Self::io("read or write the character file", source),
            other => Self::invalid(format!("Invalid character file: {other}")),
        }
    }
}

impl From<ImportError> for ApiError {
    fn from(e: ImportError) -> Self {
        Self::invalid(format!("Cannot import this file: {e}"))
    }
}

impl From<ContentError> for ApiError {
    fn from(e: ContentError) -> Self {
        tracing::warn!("content error: {e}");
        Self::new(ErrorCode::ContentError, "Content could not be loaded.")
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        Self::io("access the file", e)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::invalid(format!("Invalid data: {e}"))
    }
}

impl<T> From<std::sync::PoisonError<T>> for ApiError {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        Self::internal(format!("poisoned lock: {e}"))
    }
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}
