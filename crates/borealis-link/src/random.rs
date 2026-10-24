//! CSPRNG helpers, hex, hashing and constant-time comparison.

use sha2::{Digest, Sha256};

use crate::error::LinkError;

/// `N` bytes from the OS CSPRNG.
///
/// # Errors
/// [`LinkError::Random`] when the OS has no secure randomness.
pub fn random_bytes<const N: usize>() -> Result<[u8; N], LinkError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| LinkError::Random(e.to_string()))?;
    Ok(b)
}

/// A random 256-bit bearer token as 64 lowercase hex characters.
///
/// # Errors
/// As [`random_bytes`].
pub fn random_token() -> Result<String, LinkError> {
    Ok(hex(&random_bytes::<32>()?))
}

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Lowercase hex SHA-256 of a string (stored instead of a token).
pub fn sha256_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

/// Constant-time string equality (length is not secret).
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_compare() {
        assert!(ct_eq("12345678", "12345678"));
        assert!(!ct_eq("12345678", "12345679"));
        assert!(!ct_eq("1234567", "12345678"));
    }

    #[test]
    fn tokens_are_64_hex_chars_and_unique() {
        let a = random_token().unwrap();
        let b = random_token().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn hashing_is_stable_hex() {
        assert_eq!(sha256_hex("abc").len(), 64);
        assert_eq!(sha256_hex("abc"), sha256_hex("abc"));
        assert_eq!(hex(&[0, 255]), "00ff");
    }
}
