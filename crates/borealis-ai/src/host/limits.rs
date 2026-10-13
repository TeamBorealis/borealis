//! Rate limiting and the pairing-code state machine (per-IP lockout plus a global ceiling).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::lock;

const RATE_WINDOW: Duration = Duration::from_secs(60);
const LIMITER_GC_THRESHOLD: usize = 1024;

/// Fixed-window counter per key.
pub(super) struct RateLimiter {
    limit: u32,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub(super) fn new(limit: u32) -> Self {
        Self {
            limit,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `false` when the key is over its limit for the current window.
    pub(super) fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut hits = lock(&self.hits);
        if hits.len() > LIMITER_GC_THRESHOLD {
            hits.retain(|_, (start, _)| now.duration_since(*start) < RATE_WINDOW);
        }
        let entry = hits.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= RATE_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.limit
    }
}

/// Outcome of a pairing guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Guess {
    /// Correct code from a sender that is not locked out: the code is consumed.
    Accepted,
    /// Wrong code, sender locked out, or the window expired.
    Rejected,
}

/// An open pairing window.
pub(super) struct PairingState {
    code: String,
    expires: Instant,
    failures_total: u32,
    failures_by_ip: HashMap<IpAddr, u32>,
}

/// Per-sender and global failure caps for one code.
#[derive(Debug, Clone, Copy)]
pub(super) struct GuessLimits {
    pub per_ip: u32,
    pub total: u32,
}

impl PairingState {
    pub(super) fn new(code: String, ttl: Duration) -> Self {
        Self {
            code,
            expires: Instant::now() + ttl,
            failures_total: 0,
            failures_by_ip: HashMap::new(),
        }
    }

    /// Check a guess. Wrong guesses lock out only the sending IP after `per_ip` misses; the code
    /// itself dies after `total` misses from all senders (bounds brute force with many IPs).
    /// Returns the outcome and whether the window must be closed.
    pub(super) fn guess(&mut self, ip: IpAddr, code: &str, lim: GuessLimits) -> (Guess, bool) {
        if self.expires <= Instant::now() {
            return (Guess::Rejected, true);
        }
        let misses = self.failures_by_ip.entry(ip).or_insert(0);
        if *misses >= lim.per_ip {
            return (Guess::Rejected, false);
        }
        if ct_eq(&self.code, code) {
            return (Guess::Accepted, true);
        }
        *misses += 1;
        self.failures_total += 1;
        (Guess::Rejected, self.failures_total >= lim.total)
    }
}

/// Constant-time string equality (length is not secret).
pub(super) fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIM: GuessLimits = GuessLimits {
        per_ip: 2,
        total: 5,
    };

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([10, 0, 0, n])
    }

    #[test]
    fn rate_limiter_blocks_after_limit_per_key() {
        let l = RateLimiter::new(2);
        assert!(l.allow("a") && l.allow("a"));
        assert!(!l.allow("a"));
        assert!(l.allow("b"));
    }

    #[test]
    fn constant_time_compare() {
        assert!(ct_eq("12345678", "12345678"));
        assert!(!ct_eq("12345678", "12345679"));
        assert!(!ct_eq("1234567", "12345678"));
    }

    #[test]
    fn misses_lock_out_only_the_sender() {
        let mut p = PairingState::new("11112222".into(), Duration::from_secs(60));
        assert_eq!(p.guess(ip(1), "0", LIM), (Guess::Rejected, false));
        assert_eq!(p.guess(ip(1), "0", LIM), (Guess::Rejected, false));
        assert_eq!(p.guess(ip(1), "11112222", LIM), (Guess::Rejected, false));
        assert_eq!(p.guess(ip(2), "11112222", LIM), (Guess::Accepted, true));
    }

    #[test]
    fn global_ceiling_destroys_the_code() {
        let mut p = PairingState::new("11112222".into(), Duration::from_secs(60));
        for n in 1..=4 {
            assert_eq!(p.guess(ip(n), "0", LIM), (Guess::Rejected, false));
        }
        assert_eq!(p.guess(ip(9), "0", LIM), (Guess::Rejected, true));
    }

    #[test]
    fn expired_window_closes() {
        let mut p = PairingState::new("11112222".into(), Duration::ZERO);
        assert_eq!(p.guess(ip(1), "11112222", LIM), (Guess::Rejected, true));
    }
}
