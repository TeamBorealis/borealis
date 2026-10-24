//! The pinned certificate fingerprint and the device id derived from it.

use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::random::hex;

/// SHA-256 of a DER certificate. Serialized as unpadded base64url.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CertFingerprint(pub [u8; 32]);

impl CertFingerprint {
    /// Fingerprint of a DER-encoded certificate.
    pub fn of_der(der: &[u8]) -> Self {
        Self(Sha256::digest(der).into())
    }

    /// Unpadded base64url (43 chars).
    pub fn to_base64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }

    /// Parse unpadded base64url; must decode to exactly 32 bytes.
    pub fn from_base64url(s: &str) -> Option<Self> {
        let bytes = URL_SAFE_NO_PAD.decode(s.trim()).ok()?;
        Some(Self(bytes.try_into().ok()?))
    }

    /// Short human-comparable form (first 8 bytes, hex, grouped): `ab12 cd34 ...`.
    pub fn short_hex(&self) -> String {
        self.0[..8]
            .chunks(2)
            .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Constant-time equality.
    pub fn ct_eq(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

impl fmt::Debug for CertFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CertFingerprint({})", self.to_base64url())
    }
}

impl Serialize for CertFingerprint {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_base64url())
    }
}

impl<'de> Deserialize<'de> for CertFingerprint {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_base64url(&s)
            .ok_or_else(|| serde::de::Error::custom("fingerprint must be 32 bytes base64url"))
    }
}

/// Number of hex characters in a [`DeviceId`].
pub const DEVICE_ID_LEN: usize = 16;

/// Stable id of a device: the first 8 bytes of its certificate fingerprint, as 16 lowercase hex
/// characters. Always derived locally from the certificate a peer proved it holds, never taken
/// from what the peer claims; a new identity (reset, reinstall) is a new device.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct DeviceId(String);

impl DeviceId {
    /// The id of the device holding the certificate with this fingerprint.
    pub fn of(fingerprint: &CertFingerprint) -> Self {
        Self(hex(&fingerprint.0[..DEVICE_ID_LEN / 2]))
    }

    /// Parse a stored id (exactly 16 lowercase hex characters).
    pub fn parse(s: &str) -> Option<Self> {
        (s.len() == DEVICE_ID_LEN && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(s.to_string()))
    }

    /// The id text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom("device id must be 16 hex chars"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_serde_and_ct_eq() {
        let a = CertFingerprint::of_der(b"x");
        let json = serde_json::to_string(&a).unwrap();
        let back: CertFingerprint = serde_json::from_str(&json).unwrap();
        assert!(a.ct_eq(&back));
        assert!(!a.ct_eq(&CertFingerprint::of_der(b"y")));
        assert_eq!(a.short_hex().len(), 19);
    }

    #[test]
    fn device_id_is_derived_from_the_fingerprint() {
        let fp = CertFingerprint::of_der(b"cert");
        let id = DeviceId::of(&fp);
        assert_eq!(id.as_str().len(), DEVICE_ID_LEN);
        assert_eq!(DeviceId::parse(id.as_str()), Some(id.clone()));
        assert_ne!(id, DeviceId::of(&CertFingerprint::of_der(b"other")));
        for bad in [
            "",
            "ABCDEF0123456789",
            "0123",
            "0123456789abcdeg",
            "../../etc/passwd",
        ] {
            assert!(DeviceId::parse(bad).is_none(), "{bad}");
        }
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<DeviceId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<DeviceId>("\"nope\"").is_err());
    }
}
