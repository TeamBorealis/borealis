//! Pairing string (shown as text and QR on the desktop) and the pinned certificate fingerprint.
//!
//! Format: `borealis-pair:v2?h=<ip:port>&c=<8 digits>&fp=<base64url SHA-256 of the cert DER>`.

use std::fmt;
use std::net::SocketAddr;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// URI prefix of a v2 pairing string.
pub const PAIRING_URI_PREFIX: &str = "borealis-pair:v2?";
/// Number of digits in a pairing code.
pub const PAIRING_CODE_LEN: usize = 8;

/// SHA-256 of the host's DER certificate. Serialized as unpadded base64url.
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

/// Everything a phone needs to pair: address, one-time code and certificate pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingUri {
    /// Host address (`ip:port`).
    pub addr: SocketAddr,
    /// One-time pairing code (8 digits).
    pub code: String,
    /// Pinned certificate fingerprint.
    pub fingerprint: CertFingerprint,
}

/// Why a pairing string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid pairing string: {0}")]
pub struct PairingUriError(pub &'static str);

impl fmt::Display for PairingUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{PAIRING_URI_PREFIX}h={}&c={}&fp={}",
            self.addr,
            self.code,
            self.fingerprint.to_base64url()
        )
    }
}

impl std::str::FromStr for PairingUri {
    type Err = PairingUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let query = s
            .trim()
            .strip_prefix(PAIRING_URI_PREFIX)
            .ok_or(PairingUriError("not a borealis-pair:v2 string"))?;
        let (mut h, mut c, mut fp) = (None, None, None);
        for part in query.split('&') {
            let (k, v) = part
                .split_once('=')
                .ok_or(PairingUriError("malformed parameter"))?;
            let slot = match k {
                "h" => &mut h,
                "c" => &mut c,
                "fp" => &mut fp,
                _ => continue, // forward compatible: ignore unknown keys
            };
            if slot.replace(v).is_some() {
                return Err(PairingUriError("duplicate parameter"));
            }
        }
        let addr = h
            .and_then(|v| v.parse::<SocketAddr>().ok())
            .ok_or(PairingUriError("missing or bad host address"))?;
        let code = c
            .filter(|v| v.len() == PAIRING_CODE_LEN && v.bytes().all(|b| b.is_ascii_digit()))
            .ok_or(PairingUriError("missing or bad code"))?
            .to_string();
        let fingerprint = fp
            .and_then(CertFingerprint::from_base64url)
            .ok_or(PairingUriError("missing or bad fingerprint"))?;
        Ok(Self {
            addr,
            code,
            fingerprint,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PairingUri {
        PairingUri {
            addr: "192.168.1.20:41234".parse().unwrap(),
            code: "01234567".into(),
            fingerprint: CertFingerprint::of_der(b"cert"),
        }
    }

    #[test]
    fn round_trips() {
        let s = sample().to_string();
        assert!(s.starts_with("borealis-pair:v2?h=192.168.1.20:41234&c=01234567&fp="));
        assert_eq!(s.parse::<PairingUri>().unwrap(), sample());
    }

    #[test]
    fn rejects_bad_strings() {
        let fp = sample().fingerprint.to_base64url();
        for bad in [
            "borealis-pair:v1?h=1.2.3.4:5&c=01234567".to_string(),
            format!("borealis-pair:v2?h=1.2.3.4:5&c=0123456&fp={fp}"),
            format!("borealis-pair:v2?h=nohost&c=01234567&fp={fp}"),
            "borealis-pair:v2?h=1.2.3.4:5&c=01234567&fp=AAAA".to_string(),
            format!("borealis-pair:v2?h=1.2.3.4:5&c=01234567&c=01234567&fp={fp}"),
        ] {
            assert!(bad.parse::<PairingUri>().is_err(), "{bad}");
        }
    }

    #[test]
    fn fingerprint_serde_and_ct_eq() {
        let a = CertFingerprint::of_der(b"x");
        let json = serde_json::to_string(&a).unwrap();
        let back: CertFingerprint = serde_json::from_str(&json).unwrap();
        assert!(a.ct_eq(&back));
        assert!(!a.ct_eq(&CertFingerprint::of_der(b"y")));
        assert_eq!(a.short_hex().len(), 19);
    }
}
