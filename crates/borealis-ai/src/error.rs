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
    /// The AI account refuses every further call until the user acts (a usage limit was hit,
    /// or the CLI is signed out): the work stopped before it finished. See
    /// [`AiError::account_block`].
    #[error("{}", .0.user_message())]
    AccountBlocked(AccountBlock),
}

/// Why the AI account refuses every call until the user acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountBlockKind {
    /// A session, usage, rate or quota limit: calls pass again after it resets.
    UsageLimit,
    /// Not signed in, or the sign-in or API key is no longer valid.
    SignIn,
}

/// An AI CLI failure that every later call would repeat (see [`AiError::account_block`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBlock {
    /// What blocks the account.
    pub kind: AccountBlockKind,
    /// When the limit resets, as the CLI printed it ("1:20am (Europe/Paris)"), if it did.
    pub resets: Option<String>,
}

impl AccountBlock {
    /// One plain sentence for the user.
    #[must_use]
    pub fn user_message(&self) -> String {
        match (self.kind, self.resets.as_deref()) {
            (AccountBlockKind::UsageLimit, Some(at)) => format!(
                "Your AI usage limit was reached, so the work stopped before it finished and nothing was kept. Try again after the limit resets ({at})."
            ),
            (AccountBlockKind::UsageLimit, None) => "Your AI usage limit was reached, so the work stopped before it finished and nothing was kept. Try again after the limit resets.".to_string(),
            (AccountBlockKind::SignIn, _) => "The AI CLI is not signed in (or its sign-in expired), so the work stopped before it finished and nothing was kept. Sign in to the AI CLI again, then retry.".to_string(),
        }
    }
}

/// Words of a CLI failure saying a usage, session, rate or quota limit was hit.
const LIMIT_PHRASES: [&str; 12] = [
    "session limit",
    "usage limit",
    "weekly limit",
    "daily limit",
    "rate limit",
    "rate_limit",
    "quota",
    "credit balance is too low",
    "out of credits",
    "too many requests",
    "limit reached",
    "hit your limit",
];

/// Words of a CLI failure saying the CLI is not (or no longer) signed in.
const SIGN_IN_PHRASES: [&str; 10] = [
    "not logged in",
    "please run /login",
    "invalid api key",
    "authentication_error",
    "authentication failed",
    "oauth token has expired",
    "token could not be refreshed",
    "sign in again",
    "log in again",
    "unauthorized",
];

/// Longest reset time kept from a CLI message.
const RESETS_MAX_CHARS: usize = 60;

/// The account block a CLI failure message states, if any.
fn classify(text: &str) -> Option<AccountBlock> {
    let low = text.to_lowercase();
    let kind = if LIMIT_PHRASES.iter().any(|p| low.contains(p)) {
        AccountBlockKind::UsageLimit
    } else if SIGN_IN_PHRASES.iter().any(|p| low.contains(p)) {
        AccountBlockKind::SignIn
    } else {
        return None;
    };
    let resets = (kind == AccountBlockKind::UsageLimit)
        .then(|| reset_time(text))
        .flatten();
    Some(AccountBlock { kind, resets })
}

/// When a limit resets, as the message prints it: after "resets", "try again in/at/after", or
/// a unix time after a `|` ("usage limit reached|1760000000").
fn reset_time(text: &str) -> Option<String> {
    let low = text.to_lowercase();
    for key in [
        "resets at ",
        "resets ",
        "try again in ",
        "try again at ",
        "try again after ",
    ] {
        if let Some(i) = low.find(key) {
            let rest = &text[i + key.len()..];
            let end = rest.find(['\n', '\r']).unwrap_or(rest.len());
            let at: String = rest[..end]
                .trim()
                .trim_end_matches(['.', ','])
                .chars()
                .take(RESETS_MAX_CHARS)
                .collect();
            if !at.is_empty() {
                let lead = if key.starts_with("try again in") {
                    "in "
                } else {
                    ""
                };
                return Some(format!("{lead}{at}"));
            }
        }
    }
    let (_, tail) = text.split_once('|')?;
    let secs: i64 = tail.trim().parse().ok()?;
    Some(utc_time(secs))
}

/// `secs` since the unix epoch as "YYYY-MM-DD HH:MM UTC".
fn utc_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3_600,
        rem % 3_600 / 60
    )
}

impl AiError {
    /// The account block this failure states: a usage/session/rate/quota limit or a lost
    /// sign-in, which every later call would hit too (so a long task should stop, not retry).
    /// Read from the CLI's own failure message (its stderr only when the message is the bare
    /// exit status) or a remote host's error; any other failure is `None`.
    #[must_use]
    pub fn account_block(&self) -> Option<AccountBlock> {
        match self {
            AiError::AccountBlocked(b) => Some(b.clone()),
            AiError::Cli {
                message,
                stderr_tail,
                ..
            } => classify(message).or_else(|| {
                message
                    .starts_with(EXIT_STATUS_MESSAGE)
                    .then(|| classify(stderr_tail))
                    .flatten()
            }),
            AiError::Transport(m) => classify(m),
            _ => None,
        }
    }

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

/// How a CLI failure with no message of its own starts (`session.rs`).
pub(crate) const EXIT_STATUS_MESSAGE: &str = "the CLI exited with status";

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

    #[test]
    fn a_session_limit_is_an_account_block_with_its_reset_time() {
        let e = AiError::cli(
            "You've hit your session limit \u{b7} resets 1:20am (Europe/Paris)",
            "",
            Some(1),
        );
        let b = e.account_block().expect("a limit");
        assert_eq!(b.kind, AccountBlockKind::UsageLimit);
        assert_eq!(b.resets.as_deref(), Some("1:20am (Europe/Paris)"));
        let shown = AiError::AccountBlocked(b).to_string();
        assert!(
            shown.contains("usage limit") && shown.contains("1:20am"),
            "{shown}"
        );
    }

    #[test]
    fn a_unix_reset_time_and_a_try_again_delay_are_read() {
        let e = AiError::cli("Claude AI usage limit reached|1760000000", "", None);
        assert_eq!(
            e.account_block().and_then(|b| b.resets).as_deref(),
            Some("2025-10-09 08:53 UTC")
        );
        let e = AiError::Transport(
            "task failed (500): You've hit your usage limit. Try again in 2 hours.".into(),
        );
        assert_eq!(
            e.account_block().and_then(|b| b.resets).as_deref(),
            Some("in 2 hours")
        );
    }

    #[test]
    fn a_lost_sign_in_is_an_account_block() {
        let e = AiError::cli(
            "Your access token could not be refreshed because your refresh token was revoked. Please log out and sign in again.",
            "",
            Some(1),
        );
        let b = e.account_block().expect("a sign-in block");
        assert_eq!((b.kind, b.resets), (AccountBlockKind::SignIn, None));
        let e = AiError::cli("the CLI exited with status 1", "Invalid API key", Some(1));
        assert!(e.account_block().is_some());
    }

    #[test]
    fn ordinary_failures_are_no_account_block() {
        for e in [
            AiError::cli("the CLI exited with status 3", "boom", Some(3)),
            AiError::cli("API Error: 500 overloaded", "", Some(1)),
            AiError::cli("model not found", "rate limit retry noise", Some(1)),
            AiError::NoResult,
        ] {
            assert!(e.account_block().is_none(), "{e}");
        }
    }
}
