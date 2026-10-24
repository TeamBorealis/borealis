//! Typed codes for devices without a camera (PC <-> PC): `42-ember-lantern`.
//!
//! The number is the **nameplate**: it is advertised over mDNS so the typing device can find the
//! listener, and is not secret. The two words (16 bits) are the password of a SPAKE2 exchange
//! ([`crate::auth`]), which allows exactly one online guess per code: the listener kills the code
//! after any attempt that does not end in a valid key confirmation. Codes expire after 5 minutes.

use std::fmt;
use std::str::FromStr;

use crate::error::LinkError;
use crate::random::random_bytes;
use crate::wordlist::{index_of, WORDS};

/// Smallest nameplate.
pub const MIN_NAMEPLATE: u16 = 1;
/// Largest nameplate generated and accepted.
pub const MAX_NAMEPLATE: u16 = 99;

/// A typed code. `Debug` is redacted (the words are the password).
#[derive(Clone, PartialEq, Eq)]
pub struct TypedCode {
    nameplate: u16,
    words: [u8; 2],
}

/// Why a typed code was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid code: {0}")]
pub struct TypedCodeError(pub &'static str);

impl TypedCode {
    /// A fresh code from the OS CSPRNG.
    ///
    /// # Errors
    /// [`LinkError::Random`].
    pub fn generate() -> Result<Self, LinkError> {
        let span = MAX_NAMEPLATE - MIN_NAMEPLATE + 1;
        let limit = u16::MAX - (u16::MAX % span);
        let nameplate = loop {
            let n = u16::from_le_bytes(random_bytes::<2>()?);
            if n < limit {
                break MIN_NAMEPLATE + n % span;
            }
        };
        Ok(Self {
            nameplate,
            words: random_bytes::<2>()?,
        })
    }

    /// The nameplate (public, used for discovery).
    pub fn nameplate(&self) -> u16 {
        self.nameplate
    }

    /// The canonical text, also the SPAKE2 password.
    pub fn password(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for TypedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}",
            self.nameplate,
            WORDS[usize::from(self.words[0])],
            WORDS[usize::from(self.words[1])]
        )
    }
}

impl fmt::Debug for TypedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypedCode({}-<redacted>)", self.nameplate)
    }
}

impl FromStr for TypedCode {
    type Err = TypedCodeError;

    /// Lenient about case, surrounding space and separators (`42 Ember lantern`), strict about
    /// content: a nameplate in range and two words from the list.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > 64 {
            return Err(TypedCodeError("too long"));
        }
        let lower = s.trim().to_ascii_lowercase();
        let parts: Vec<&str> = lower
            .split(|c: char| c == '-' || c.is_whitespace())
            .filter(|p| !p.is_empty())
            .collect();
        let [num, w1, w2] = parts.as_slice() else {
            return Err(TypedCodeError("expected a number and two words"));
        };
        let nameplate = Some(*num)
            .filter(|n| n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|n| n.parse::<u16>().ok())
            .filter(|n| (MIN_NAMEPLATE..=MAX_NAMEPLATE).contains(n))
            .ok_or(TypedCodeError("bad number"))?;
        let a = index_of(w1).ok_or(TypedCodeError("unknown word"))?;
        let b = index_of(w2).ok_or(TypedCodeError("unknown word"))?;
        Ok(Self {
            nameplate,
            words: [a, b],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_codes_round_trip() {
        for _ in 0..200 {
            let c = TypedCode::generate().unwrap();
            assert!((MIN_NAMEPLATE..=MAX_NAMEPLATE).contains(&c.nameplate()));
            assert_eq!(c.to_string().parse::<TypedCode>().unwrap(), c);
        }
    }

    #[test]
    fn parsing_is_lenient_on_form_strict_on_content() {
        let c: TypedCode = "42-ember-lantern".parse().unwrap();
        assert_eq!(c.to_string(), "42-ember-lantern");
        assert_eq!(" 42 Ember  LANTERN ".parse::<TypedCode>().unwrap(), c);
        for bad in [
            "",
            "42-ember",
            "42-ember-lantern-oak",
            "0-ember-lantern",
            "100-ember-lantern",
            "4a-ember-lantern",
            "42-ember-lanterns",
            "42-émber-lantern",
            "ember-42-lantern",
        ] {
            assert!(bad.parse::<TypedCode>().is_err(), "{bad}");
        }
    }

    #[test]
    fn debug_hides_the_words() {
        let c: TypedCode = "42-ember-lantern".parse().unwrap();
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("ember") && dbg.contains("42"));
    }
}
