//! .NET style `Version` strings (2 to 4 numeric components).

use std::cmp::Ordering;
use std::fmt;

/// Parsed dotted version. Ordering is numeric per component; a missing component sorts
/// before a present one (as in .NET `Version`, where `1.0 < 1.0.0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    parts: Vec<u32>,
}

/// A string that is not a valid version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid version {0:?}: expected 2 to 4 dot-separated integers")]
pub struct VersionError(pub String);

impl Version {
    /// Parse a version with 2 to 4 components (`update/@version`).
    ///
    /// # Errors
    /// [`VersionError`] when the text is not 2 to 4 non-negative integers.
    pub fn parse(text: &str) -> Result<Self, VersionError> {
        Self::parse_range(text, 2)
    }

    /// Parse the root `app` attribute: like [`Version::parse`] but a single component is
    /// accepted and read as `<n>.0`.
    ///
    /// # Errors
    /// [`VersionError`] when the text is not 1 to 4 non-negative integers.
    pub fn parse_app(text: &str) -> Result<Self, VersionError> {
        let mut v = Self::parse_range(text, 1)?;
        if v.parts.len() == 1 {
            v.parts.push(0);
        }
        Ok(v)
    }

    fn parse_range(text: &str, min: usize) -> Result<Self, VersionError> {
        let err = || VersionError(text.to_string());
        let parts = text
            .trim()
            .split('.')
            .map(|p| {
                if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(err());
                }
                p.parse::<u32>().map_err(|_| err())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() < min || parts.len() > 4 {
            return Err(err());
        }
        Ok(Self { parts })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts.cmp(&other.parts)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text: Vec<String> = self.parts.iter().map(u32::to_string).collect();
        f.write_str(&text.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares_numerically() {
        let a = Version::parse("1.2.10").unwrap();
        let b = Version::parse("1.2.9").unwrap();
        assert!(a > b);
        assert_eq!(a.to_string(), "1.2.10");
        assert!(Version::parse("1.0").unwrap() < Version::parse("1.0.0").unwrap());
    }

    #[test]
    fn app_version_accepts_single_component() {
        assert_eq!(Version::parse_app("1").unwrap().to_string(), "1.0");
        assert!(Version::parse("1").is_err());
    }

    #[test]
    fn rejects_junk() {
        for bad in ["", "a.b", "1.", "1.2.3.4.5", "1.2-beta", "-1.0", "1..2"] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }
}
