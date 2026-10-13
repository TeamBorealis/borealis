//! Currency: five denominations in copper base units, deposit and make-change withdrawals.

use crate::state::Coins;
use serde::{Deserialize, Serialize};

/// A coin denomination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Coin {
    /// Copper (1).
    Cp,
    /// Silver (10).
    Sp,
    /// Electrum (50).
    Ep,
    /// Gold (100).
    Gp,
    /// Platinum (1000).
    Pp,
}

impl Coin {
    /// All denominations from the lowest to the highest value.
    pub const ALL: [Coin; 5] = [Coin::Cp, Coin::Sp, Coin::Ep, Coin::Gp, Coin::Pp];

    /// Value in copper pieces.
    #[must_use]
    pub fn rate(self) -> u64 {
        match self {
            Coin::Cp => 1,
            Coin::Sp => 10,
            Coin::Ep => 50,
            Coin::Gp => 100,
            Coin::Pp => 1000,
        }
    }

    /// The next lower denomination and how many of it one coin breaks into.
    fn break_down(self) -> Option<(Coin, u64)> {
        let lower = match self {
            Coin::Cp => return None,
            Coin::Sp => Coin::Cp,
            Coin::Ep => Coin::Sp,
            Coin::Gp => Coin::Ep,
            Coin::Pp => Coin::Gp,
        };
        Some((lower, self.rate() / lower.rate()))
    }
}

impl Coins {
    /// Count of one denomination.
    #[must_use]
    pub fn get(&self, c: Coin) -> u64 {
        match c {
            Coin::Cp => self.cp,
            Coin::Sp => self.sp,
            Coin::Ep => self.ep,
            Coin::Gp => self.gp,
            Coin::Pp => self.pp,
        }
    }

    fn slot(&mut self, c: Coin) -> &mut u64 {
        match c {
            Coin::Cp => &mut self.cp,
            Coin::Sp => &mut self.sp,
            Coin::Ep => &mut self.ep,
            Coin::Gp => &mut self.gp,
            Coin::Pp => &mut self.pp,
        }
    }

    /// Total value in copper.
    #[must_use]
    pub fn total_cp(&self) -> u64 {
        Coin::ALL.iter().map(|c| self.get(*c) * c.rate()).sum()
    }

    /// Number of coins (each weighs 1/50 lb).
    #[must_use]
    pub fn count(&self) -> u64 {
        Coin::ALL.iter().map(|c| self.get(*c)).sum()
    }

    /// Value in gold pieces.
    #[must_use]
    pub fn gold_value(&self) -> f64 {
        self.total_cp() as f64 / 100.0
    }

    /// Add coins.
    #[must_use]
    pub fn deposit(&self, coin: Coin, n: u64) -> Coins {
        let mut next = *self;
        *next.slot(coin) += n;
        next
    }

    /// Remove `n` coins of a denomination, breaking larger coins down as needed.
    ///
    /// A higher coin is broken one step at a time (platinum to 10 gold, gold to 2 electrum, electrum
    /// to 5 silver, silver to 10 copper), always choosing the smallest single coin that covers the
    /// shortfall, otherwise the smallest one available. When no higher coin exists the remaining
    /// value is paid with lower coins and the change returned in copper. The total value always
    /// drops by exactly `n * rate`. Returns `None` when the total is insufficient.
    #[must_use]
    pub fn withdraw(&self, coin: Coin, n: u64) -> Option<Coins> {
        let need = n.checked_mul(coin.rate())?;
        if need > self.total_cp() {
            return None;
        }
        let mut c = *self;
        while c.get(coin) < n {
            let shortfall = (n - c.get(coin)) * coin.rate();
            let higher: Vec<Coin> = Coin::ALL
                .iter()
                .copied()
                .filter(|h| h.rate() > coin.rate() && c.get(*h) > 0)
                .collect();
            let pick = higher
                .iter()
                .copied()
                .find(|h| h.rate() >= shortfall)
                .or_else(|| higher.first().copied());
            let Some(h) = pick else { break };
            let (lower, pieces) = h.break_down()?;
            *c.slot(h) -= 1;
            *c.slot(lower) += pieces;
        }
        if c.get(coin) >= n {
            *c.slot(coin) -= n;
            return Some(c);
        }
        // pay the remainder with lower coins, giving change in copper
        let have = c.get(coin);
        *c.slot(coin) = 0;
        let mut owed = (n - have) * coin.rate();
        for lower in Coin::ALL
            .iter()
            .rev()
            .copied()
            .filter(|l| l.rate() < coin.rate())
        {
            while owed > 0 && c.get(lower) > 0 {
                *c.slot(lower) -= 1;
                owed = owed.saturating_sub(lower.rate());
            }
        }
        debug_assert_eq!(owed, 0);
        Some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coins(cp: u64, sp: u64, ep: u64, gp: u64, pp: u64) -> Coins {
        Coins { cp, sp, ep, gp, pp }
    }

    #[test]
    fn tv23_totals_and_weight_basis() {
        let c = coins(10, 50, 0, 100, 0);
        assert_eq!(c.count(), 160);
        assert_eq!(c.total_cp(), 10_510);
        assert!((c.gold_value() - 105.10).abs() < 1e-9);
    }

    #[test]
    fn withdraw_without_breaking() {
        let c = coins(0, 0, 0, 30, 1);
        let r = c.withdraw(Coin::Gp, 25).unwrap();
        assert_eq!((r.gp, r.pp), (5, 1));
    }

    #[test]
    fn withdraw_breaks_down_stepwise() {
        // 7 cp from 2 gp: gp -> 2 ep, ep -> 5 sp, sp -> 10 cp
        let r = coins(0, 0, 0, 2, 0).withdraw(Coin::Cp, 7).unwrap();
        assert_eq!(r, coins(3, 4, 1, 1, 0));
        assert_eq!(r.total_cp(), 193);
        // 12 gp from 1 pp + 5 gp: platinum covers the shortfall of 7 gp
        let r = coins(0, 0, 0, 5, 1).withdraw(Coin::Gp, 12).unwrap();
        assert_eq!(r.total_cp(), 500 + 1000 - 1200);
        assert_eq!((r.pp, r.gp), (0, 3));
        // 1 gp from 3 ep: no higher coin, pays from lower coins with copper change
        let r = coins(0, 0, 3, 0, 0).withdraw(Coin::Gp, 1).unwrap();
        assert_eq!(r.total_cp(), 50);
    }

    #[test]
    fn insufficient_funds_and_invariant() {
        assert!(coins(5, 0, 0, 0, 0).withdraw(Coin::Sp, 1).is_none());
        for (c, d, n) in [
            (coins(3, 4, 5, 6, 7), Coin::Pp, 6),
            (coins(0, 10, 0, 0, 0), Coin::Gp, 1),
            (coins(0, 0, 1, 0, 0), Coin::Sp, 3),
            (coins(99, 0, 0, 1, 0), Coin::Ep, 3),
        ] {
            let r = c.withdraw(d, n).unwrap();
            assert_eq!(r.total_cp(), c.total_cp() - n * d.rate());
        }
    }

    #[test]
    fn deposit_adds() {
        assert_eq!(Coins::default().deposit(Coin::Gp, 4).gp, 4);
    }
}
