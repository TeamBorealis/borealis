//! Pairing string (shown as text and QR on the desktop) and pairing-code generation.
//!
//! Format: `borealis-pair:v2?h=<ip:port>&c=<8 digits>&fp=<base64url SHA-256 of the cert DER>`.

use std::fmt;
use std::net::SocketAddr;

use crate::error::LinkError;
use crate::fingerprint::CertFingerprint;
use crate::random::random_bytes;

/// URI prefix of a v2 pairing string.
pub const PAIRING_URI_PREFIX: &str = "borealis-pair:v2?";
/// Number of digits in a pairing code.
pub const PAIRING_CODE_LEN: usize = 8;
const PAIRING_CODE_SPACE: u32 = 100_000_000; // 10^PAIRING_CODE_LEN

/// Uniform eight-digit pairing code from the OS CSPRNG (rejection sampling, no modulo bias).
///
/// # Errors
/// [`LinkError::Random`] when the OS has no secure randomness.
pub fn random_pairing_code() -> Result<String, LinkError> {
    let limit = u32::MAX - (u32::MAX % PAIRING_CODE_SPACE);
    loop {
        let n = u32::from_le_bytes(random_bytes::<4>()?);
        if n < limit {
            return Ok(format!("{:08}", n % PAIRING_CODE_SPACE));
        }
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
    fn codes_are_eight_uniform_digits() {
        for _ in 0..200 {
            let c = random_pairing_code().unwrap();
            assert_eq!(c.len(), PAIRING_CODE_LEN);
            assert!(c.bytes().all(|b| b.is_ascii_digit()));
        }
    }
}
