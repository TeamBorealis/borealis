//! Rate limiting and the pairing-code state machine (per-IP lockout plus a global ceiling).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::random::ct_eq;

const RATE_WINDOW: Duration = Duration::from_secs(60);
const LIMITER_GC_THRESHOLD: usize = 1024;

/// Fixed-window counter per key.
pub struct RateLimiter {
    limit: u32,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    /// At most `limit` hits per key per minute.
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `false` when the key is over its limit for the current window.
    pub fn allow(&self, key: &str) -> bool {
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
pub enum Guess {
    /// Correct code from a sender that is not locked out: the code is consumed.
    Accepted,
    /// Wrong code, sender locked out, or the window expired.
    Rejected,
}

/// An open pairing window.
pub struct PairingState {
    code: String,
    expires: Instant,
    failures_total: u32,
    failures_by_ip: HashMap<IpAddr, u32>,
}

/// Per-sender and global failure caps for one code.
#[derive(Debug, Clone, Copy)]
pub struct GuessLimits {
    /// Wrong guesses one source IP may make before it is locked out of the code.
    pub per_ip: u32,
    /// Wrong guesses from everyone before the code is destroyed.
    pub total: u32,
}

impl PairingState {
    /// A window for `code` that closes after `ttl`.
    pub fn new(code: String, ttl: Duration) -> Self {
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
    pub fn guess(&mut self, ip: IpAddr, code: &str, lim: GuessLimits) -> (Guess, bool) {
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

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
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
