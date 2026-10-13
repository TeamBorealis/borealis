//! Error type and secret/path hygiene helpers.

use std::time::Duration;

/// Which timer fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutKind {
    /// The whole session exceeded its wall-clock budget.
    Overall,
    /// No output arrived for too long.
    Idle,
}

/// Everything that can go wrong talking to a local AI CLI or running a task.
///
/// Messages are already passed through [`redact_paths`], so they are safe to show in a UI.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    /// No usable CLI was found for the requested backend.
    #[error("no `{0}` CLI found; install it or set its path in AI settings")]
    ProviderNotFound(&'static str),
    /// The process could not be started.
    #[error("failed to start the AI CLI: {0}")]
    Spawn(String),
    /// A timer fired and the process tree was killed.
    #[error("AI session timed out ({kind:?} after {after:?})")]
    Timeout {
        /// Which timer.
        kind: TimeoutKind,
        /// The configured limit.
        after: Duration,
    },
    /// The caller cancelled (or dropped the event receiver).
    #[error("AI session cancelled")]
    Cancelled,
    /// Output exceeded the configured size guard.
    #[error("AI output exceeded the limit of {limit} bytes")]
    OutputTooLarge {
        /// The configured limit in bytes.
        limit: usize,
    },
    /// The CLI reported a failure (auth, model, API error, non-zero exit...).
    #[error("AI CLI error: {message}")]
    Cli {
        /// Redacted message.
        message: String,
        /// Redacted tail of stderr, possibly empty.
        stderr_tail: String,
        /// Exit code if the process exited.
        exit_code: Option<i32>,
    },
    /// A scanned/image-only PDF was attached to a backend that cannot read PDFs natively
    /// (Codex): ask the user for page images or screenshots instead.
    #[error("`{name}` looks like a scanned PDF with no text layer; attach page images or screenshots instead")]
    NeedsImages {
        /// File name as supplied by the caller.
        name: String,
    },
    /// The stream ended without a final result.
    #[error("the AI CLI ended without producing a result")]
    NoResult,
    /// Model output could not be turned into valid typed output, even after repair attempts.
    #[error("AI output failed validation after {attempts} attempt(s): {}", errors.join("; "))]
    Validation {
        /// Total attempts made (first try plus repairs).
        attempts: u32,
        /// Issues from the final attempt.
        errors: Vec<String>,
    },
    /// Local I/O problem.
    #[error("I/O error: {0}")]
    Io(String),
    /// Bad caller input (e.g. unparsable schema).
    #[error("invalid request: {0}")]
    Invalid(String),
    /// Untrusted (imported or attached) input on a backend with an always-on shell tool, without
    /// the user's explicit consent. See [`crate::trust`].
    #[error("this request contains imported or attached content, and `{backend}` can run shell commands that read files on this computer if that content tells it to. Use Claude for imports, or confirm that you accept this risk for Codex")]
    UntrustedInputNeedsConsent {
        /// The refused CLI.
        backend: &'static str,
    },
    /// LAN transport problem (host or remote client).
    #[error("transport error: {0}")]
    Transport(String),
}

impl AiError {
    pub(crate) fn io(e: impl std::fmt::Display) -> Self {
        AiError::Io(redact_paths(&e.to_string()))
    }

    pub(crate) fn spawn(e: impl std::fmt::Display) -> Self {
        AiError::Spawn(redact_paths(&e.to_string()))
    }

    pub(crate) fn cli(message: &str, stderr_tail: &str, exit_code: Option<i32>) -> Self {
        let start = stderr_tail.len().saturating_sub(STDERR_IN_ERROR_MAX);
        let start = (start..=stderr_tail.len())
            .find(|&i| stderr_tail.is_char_boundary(i))
            .unwrap_or(0);
        AiError::Cli {
            message: redact_paths(message),
            stderr_tail: redact_paths(&stderr_tail[start..]),
            exit_code,
        }
    }
}

/// Stderr kept inside an error value (the full 16 KiB tail stays in `AiOutcome` on success).
const STDERR_IN_ERROR_MAX: usize = 2_000;

const USER_ROOTS: [&str; 3] = ["/Users/", "/home/", "C:\\Users\\"];

/// Replace user home directories in `text` with `~` so errors do not leak account names.
///
/// Handles the process's own `HOME`/`USERPROFILE` plus the generic `/Users/<name>`,
/// `/home/<name>` and `C:\Users\<name>` shapes.
pub fn redact_paths(text: &str) -> String {
    let mut out = text.to_string();
    for var in ["HOME", "USERPROFILE"] {
        if let Ok(home) = std::env::var(var) {
            if home.len() > 3 {
                out = out.replace(&home, "~");
            }
        }
    }
    for root in USER_ROOTS {
        out = redact_root(&out, root);
    }
    out
}

fn redact_root(text: &str, root: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(root) {
        out.push_str(&rest[..pos]);
        out.push('~');
        let after = &rest[pos + root.len()..];
        let end = after
            .find(['/', '\\', '"', '\'', ' ', '\n', ':'])
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_generic_user_dirs() {
        let s = redact_paths("cannot open /Users/alice/.claude/x.json: denied");
        assert_eq!(s, "cannot open ~/.claude/x.json: denied");
        let s = redact_paths("at C:\\Users\\Bob\\AppData and /home/carol/bin");
        assert!(!s.contains("Bob") && !s.contains("carol"), "{s}");
    }

    #[test]
    fn leaves_other_paths_alone() {
        assert_eq!(
            redact_paths("/usr/local/bin/claude"),
            "/usr/local/bin/claude"
        );
    }

    #[test]
    fn cli_error_keeps_only_the_stderr_tail() {
        let long = format!("{}END", "x".repeat(10_000));
        match AiError::cli("m", &long, None) {
            AiError::Cli { stderr_tail, .. } => {
                assert!(stderr_tail.len() <= STDERR_IN_ERROR_MAX && stderr_tail.ends_with("END"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn cli_error_is_redacted() {
        let e = AiError::cli("boom in /Users/dave/x", "tail /Users/dave/y", Some(2));
        let shown = e.to_string();
        assert!(!shown.contains("dave"));
        match e {
            AiError::Cli {
                stderr_tail,
                exit_code,
                ..
            } => {
                assert!(!stderr_tail.contains("dave"));
                assert_eq!(exit_code, Some(2));
            }
            _ => unreachable!(),
        }
    }
}
