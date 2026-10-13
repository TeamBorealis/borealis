//! Input trust and the Codex consent gate.
//!
//! Codex always has a shell tool (it cannot be disabled; `--sandbox read-only` blocks writes, not
//! reads). Under prompt injection from imported content it could read any file the user can.
//! So a session whose input is [`InputTrust::Untrusted`] refuses to run on Codex unless the caller
//! passes explicit user consent. Claude runs with no shell and no file tools, so it is allowed.

use serde::{Deserialize, Serialize};

use crate::{AiError, Backend};

/// Where the prompt's content came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputTrust {
    /// Typed by the user in this app (chat, a homebrew description, a question).
    #[default]
    Trusted,
    /// Contains imported or attached material: PDFs, images, documents, pasted or imported text,
    /// files from other people.
    Untrusted,
}

impl InputTrust {
    /// The less trusted of the two.
    pub fn and(self, other: InputTrust) -> InputTrust {
        if self == InputTrust::Untrusted || other == InputTrust::Untrusted {
            InputTrust::Untrusted
        } else {
            InputTrust::Trusted
        }
    }

    /// `Untrusted` when any attachment is present, else `Trusted`.
    pub fn from_attachments(count: usize) -> InputTrust {
        if count > 0 {
            InputTrust::Untrusted
        } else {
            InputTrust::Trusted
        }
    }
}

/// Refuse untrusted input on Codex unless `allow_untrusted_with_codex` (explicit user consent).
///
/// # Errors
/// [`AiError::UntrustedInputNeedsConsent`].
pub fn check_backend(
    backend: Backend,
    trust: InputTrust,
    allow_untrusted_with_codex: bool,
) -> Result<(), AiError> {
    match (backend, trust, allow_untrusted_with_codex) {
        (Backend::Codex, InputTrust::Untrusted, false) => {
            Err(AiError::UntrustedInputNeedsConsent {
                backend: Backend::Codex.program(),
            })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_codex_with_untrusted_input_and_no_consent_is_refused() {
        use InputTrust::{Trusted, Untrusted};
        assert!(check_backend(Backend::Claude, Untrusted, false).is_ok());
        assert!(check_backend(Backend::Codex, Trusted, false).is_ok());
        assert!(check_backend(Backend::Codex, Untrusted, true).is_ok());
        let e = check_backend(Backend::Codex, Untrusted, false).unwrap_err();
        assert!(matches!(e, AiError::UntrustedInputNeedsConsent { .. }));
        assert!(e.to_string().contains("Claude"), "{e}");
    }

    #[test]
    fn trust_combines_to_the_weaker() {
        assert_eq!(
            InputTrust::Trusted.and(InputTrust::Untrusted),
            InputTrust::Untrusted
        );
        assert_eq!(
            InputTrust::Trusted.and(InputTrust::Trusted),
            InputTrust::Trusted
        );
        assert_eq!(InputTrust::from_attachments(1), InputTrust::Untrusted);
        assert_eq!(InputTrust::default(), InputTrust::Trusted);
    }
}
