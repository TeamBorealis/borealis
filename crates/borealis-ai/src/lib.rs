//! Local LLM CLI bridge (`claude -p` / `codex exec`): process spawning, streaming, tools.
//!
//! Desktop only. Mobile pairs with a desktop instance over LAN for AI features
//! (`host` feature on the desktop, `remote` feature on mobile).
//!
//! Layers, bottom to top:
//! * [`provider`]: find the CLIs and report versions.
//! * [`command`] + [`parse`] + [`session`]: build the argv, run the process, turn its stream
//!   into unified [`AiEvent`]s with timeouts, cancellation and size guards.
//! * [`task`]: versioned prompt templates, typed results, schema validation and the repair loop.
//! * [`protocol`], [`host`], [`remote`]: the LAN transport.
//!
//! No API keys are handled here; the CLIs use the user's own login. Prompts and model output
//! are never logged above `debug` level.

pub mod attachment;
pub mod child_env;
pub mod command;
pub mod error;
pub mod event;
pub mod parse;
pub mod profile;
pub mod provider;
pub mod retrieval;
pub mod session;
pub mod spawn;
pub mod task;
pub mod tempdirs;
pub mod trust;

#[cfg(any(feature = "host", feature = "remote"))]
pub mod protocol;

#[cfg(feature = "host")]
pub mod host;

#[cfg(feature = "remote")]
pub mod remote;

pub use attachment::{Attachment, AttachmentKind};
pub use error::{redact_paths, AiError};
pub use event::{AiEvent, ToolUseInfo, Usage};
pub use profile::{McpCommand, ProfileConfig, ProfileStatus};
pub use provider::{DetectConfig, ProviderInfo};
pub use session::{AiOutcome, AiRequest, AiSession, CancelToken, RunningSession, SessionOptions};
pub use tempdirs::sweep_stale_temp_dirs;
pub use trust::InputTrust;

use serde::{Deserialize, Serialize};

/// Supported local CLI backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// `claude -p`
    Claude,
    /// `codex exec`
    Codex,
}

impl Backend {
    /// Every supported backend, in detection preference order.
    pub const ALL: [Backend; 2] = [Backend::Claude, Backend::Codex];

    /// Executable name looked up on `PATH`.
    pub fn program(self) -> &'static str {
        match self {
            Backend::Claude => "claude",
            Backend::Codex => "codex",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_program_names() {
        assert_eq!(Backend::Claude.program(), "claude");
        assert_eq!(Backend::Codex.program(), "codex");
    }

    #[test]
    fn backend_serde_is_lowercase() {
        assert_eq!(serde_json::to_string(&Backend::Codex).unwrap(), "\"codex\"");
        let b: Backend = serde_json::from_str("\"claude\"").unwrap();
        assert_eq!(b, Backend::Claude);
    }
}
