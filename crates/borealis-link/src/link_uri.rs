//! The device-link QR payload (B2):
//! `borealis-link:v1?h=<ip:port>&fp=<cert sha256>&c=<secret>&n=<device name>&m=<send|receive>`.
//!
//! The device showing the QR listens; the scanning device connects to `h`, pins `fp` (TLS 1.3)
//! and proves it knows `c` over the channel ([`crate::auth`]). `m` is what the *showing* device
//! does, so a phone that scans a `receive` QR is the sender. Parsing is strict: every key exactly
//! once, no unknown keys, bounded length, no control characters in the name.

use std::fmt;
use std::net::SocketAddr;
use std::str::FromStr;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

use crate::error::LinkError;
use crate::fingerprint::CertFingerprint;
use crate::random::random_bytes;

/// URI prefix of a v1 device-link payload.
pub const LINK_URI_PREFIX: &str = "borealis-link:v1?";
/// Longest payload accepted (a QR at medium error correction holds this comfortably).
pub const MAX_LINK_URI_LEN: usize = 512;
/// Longest device name in a payload (characters).
pub const MAX_LINK_NAME_CHARS: usize = 64;
/// Bytes of the one-time QR secret.
pub const LINK_SECRET_LEN: usize = 16;

/// The one-time secret carried in the QR (128 bits). Never logged; `Debug` is redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkSecret([u8; LINK_SECRET_LEN]);

impl LinkSecret {
    /// A fresh secret from the OS CSPRNG.
    ///
    /// # Errors
    /// [`LinkError::Random`].
    pub fn generate() -> Result<Self, LinkError> {
        Ok(Self(random_bytes::<LINK_SECRET_LEN>()?))
    }

    /// The raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Unpadded base64url (22 characters).
    pub fn to_base64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }

    /// Parse unpadded base64url of exactly [`LINK_SECRET_LEN`] bytes.
    pub fn from_base64url(s: &str) -> Option<Self> {
        if s.len() != 22 {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(s).ok()?;
        Some(Self(bytes.try_into().ok()?))
    }
}

impl fmt::Debug for LinkSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LinkSecret(<redacted>)")
    }
}

/// What the device showing the QR does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkMode {
    /// It sends; the scanner receives.
    Send,
    /// It receives; the scanner sends.
    Receive,
}

impl LinkMode {
    /// The wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }

    /// The other side's role.
    pub fn opposite(self) -> Self {
        match self {
            Self::Send => Self::Receive,
            Self::Receive => Self::Send,
        }
    }
}

/// Everything a scanning device needs to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkUri {
    /// Listener address (`ip:port`).
    pub addr: SocketAddr,
    /// SHA-256 of the listener's certificate (pinned).
    pub fingerprint: CertFingerprint,
    /// One-time secret proven over the channel.
    pub secret: LinkSecret,
    /// The listener's device name (display only, never used as a path).
    pub name: String,
    /// What the listener does.
    pub mode: LinkMode,
}

/// Why a payload was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid device-link code: {0}")]
pub struct LinkUriError(pub &'static str);

impl fmt::Display for LinkUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{LINK_URI_PREFIX}h={}&fp={}&c={}&n={}&m={}",
            self.addr,
            self.fingerprint.to_base64url(),
            self.secret.to_base64url(),
            percent_encode(&self.name),
            self.mode.as_str()
        )
    }
}

fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=MAX_LINK_NAME_CHARS).contains(&n)
        && !name.chars().any(char::is_control)
        && name.trim() == name
}

fn valid_addr(addr: &SocketAddr) -> bool {
    let ip = addr.ip();
    addr.port() != 0 && !ip.is_unspecified() && !ip.is_multicast()
}

impl FromStr for LinkUri {
    type Err = LinkUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > MAX_LINK_URI_LEN {
            return Err(LinkUriError("too long"));
        }
        let query = s
            .strip_prefix(LINK_URI_PREFIX)
            .ok_or(LinkUriError("not a borealis-link:v1 code"))?;
        let mut slots: [Option<&str>; 5] = [None; 5];
        for part in query.split('&') {
            let (k, v) = part
                .split_once('=')
                .ok_or(LinkUriError("malformed parameter"))?;
            let i = ["h", "fp", "c", "n", "m"]
                .iter()
                .position(|key| *key == k)
                .ok_or(LinkUriError("unknown parameter"))?;
            if slots[i].replace(v).is_some() {
                return Err(LinkUriError("duplicate parameter"));
            }
        }
        let [h, fp, c, n, m] = slots;
        let addr = h
            .and_then(|v| v.parse::<SocketAddr>().ok())
            .filter(valid_addr)
            .ok_or(LinkUriError("missing or bad host address"))?;
        let fingerprint = fp
            .filter(|v| v.len() == 43)
            .and_then(CertFingerprint::from_base64url)
            .ok_or(LinkUriError("missing or bad fingerprint"))?;
        let secret = c
            .and_then(LinkSecret::from_base64url)
            .ok_or(LinkUriError("missing or bad secret"))?;
        let name = n
            .and_then(percent_decode)
            .filter(|v| valid_name(v))
            .ok_or(LinkUriError("missing or bad device name"))?;
        let mode = match m {
            Some("send") => LinkMode::Send,
            Some("receive") => LinkMode::Receive,
            _ => return Err(LinkUriError("missing or bad mode")),
        };
        Ok(Self {
            addr,
            fingerprint,
            secret,
            name,
            mode,
        })
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

/// RFC 3986 percent-encoding of everything but unreserved characters.
pub fn percent_encode(s: &str) -> String {
    use std::fmt::Write as _;
    s.bytes()
        .fold(String::with_capacity(s.len()), |mut out, b| {
            if is_unreserved(b) {
                out.push(char::from(b));
            } else {
                let _ = write!(out, "%{b:02X}");
            }
            out
        })
}

/// Strict percent-decoding: only unreserved characters may appear raw, every `%` is followed by
/// two hex digits, and the result must be UTF-8.
pub fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes.get(i + 1..i + 3)?;
                let text = std::str::from_utf8(hex).ok()?;
                out.push(u8::from_str_radix(text, 16).ok()?);
                i += 3;
            }
            b if is_unreserved(b) => {
                out.push(b);
                i += 1;
            }
            _ => return None,
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LinkUri {
        LinkUri {
            addr: "192.168.1.20:41234".parse().unwrap(),
            fingerprint: CertFingerprint::of_der(b"cert"),
            secret: LinkSecret::generate().unwrap(),
            name: "Alon's iPhone & Co ✓".into(),
            mode: LinkMode::Receive,
        }
    }

    #[test]
    fn round_trips() {
        let uri = sample();
        let s = uri.to_string();
        assert!(s.starts_with("borealis-link:v1?h=192.168.1.20:41234&fp="));
        assert!(s.ends_with("&m=receive"));
        assert!(s.len() < MAX_LINK_URI_LEN);
        assert_eq!(s.parse::<LinkUri>().unwrap(), uri);
        let v6 = LinkUri {
            addr: "[fe80::1]:9".parse().unwrap(),
            mode: LinkMode::Send,
            ..uri
        };
        assert_eq!(v6.to_string().parse::<LinkUri>().unwrap(), v6);
    }

    #[test]
    fn secret_debug_is_redacted() {
        let uri = sample();
        let dbg = format!("{uri:?}");
        assert!(!dbg.contains(&uri.secret.to_base64url()));
    }

    #[test]
    fn rejects_malformed_payloads() {
        let good = sample().to_string();
        let fp = sample().fingerprint.to_base64url();
        let c = LinkSecret::generate().unwrap().to_base64url();
        let base = |h: &str, f: &str, c: &str, n: &str, m: &str| {
            format!("borealis-link:v1?h={h}&fp={f}&c={c}&n={n}&m={m}")
        };
        let bad = [
            good.replacen("v1", "v2", 1),
            format!(" {good}"),
            format!("{good}&x=1"),
            format!("{good}&m=send"),
            good.replace("&m=receive", ""),
            base("1.2.3.4:0", &fp, &c, "a", "send"),
            base("0.0.0.0:5", &fp, &c, "a", "send"),
            base("224.0.0.1:5", &fp, &c, "a", "send"),
            base("nohost", &fp, &c, "a", "send"),
            base("1.2.3.4:5", "AAAA", &c, "a", "send"),
            base("1.2.3.4:5", &fp, "short", "a", "send"),
            base("1.2.3.4:5", &fp, &format!("{c}A"), "a", "send"),
            base("1.2.3.4:5", &fp, &c, "", "send"),
            base("1.2.3.4:5", &fp, &c, "a%0Ab", "send"),
            base("1.2.3.4:5", &fp, &c, "a b", "send"),
            base("1.2.3.4:5", &fp, &c, "%2", "send"),
            base("1.2.3.4:5", &fp, &c, "%FF%FE", "send"),
            base("1.2.3.4:5", &fp, &c, "%20pad", "send"),
            base("1.2.3.4:5", &fp, &c, &"x".repeat(65), "send"),
            base("1.2.3.4:5", &fp, &c, "a", "SEND"),
            base("1.2.3.4:5", &fp, &c, "a", "sync"),
            format!("{good}{}", "&".repeat(600)),
        ];
        for s in bad {
            assert!(s.parse::<LinkUri>().is_err(), "{s}");
        }
    }

    #[test]
    fn percent_coding_is_strict_and_reversible() {
        for s in ["plain", "a b/c?d=e&f", "Ünïcödé ✓", "%"] {
            assert_eq!(percent_decode(&percent_encode(s)).as_deref(), Some(s));
        }
        assert!(percent_decode("a+b").is_none());
        assert!(percent_decode("%zz").is_none());
    }
}
