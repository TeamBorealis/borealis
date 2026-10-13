//! Challenge rating and the shared CR to XP table.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::EncounterError;

/// SRD 5.1 / 5.2.1: a CR 0 creature is worth "0 or 10" XP. [`Cr::xp`]
/// returns 0; callers wanting the alternative can use this constant.
pub const CR0_ALT_XP: u64 = 10;

/// Challenge rating 0, 1/8, 1/4, 1/2 or 1..=30.
///
/// Serialises as a string (`"0"`, `"1/8"`, `"5"`); deserialises from a string
/// or a bare integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cr {
    /// CR 0.
    Zero,
    /// CR 1/8.
    Eighth,
    /// CR 1/4.
    Quarter,
    /// CR 1/2.
    Half,
    /// CR 1 to 30.
    Whole(u8),
}

/// SRD 5.1 "Experience Points by Challenge Rating" (p. 258); identical to
/// SRD 5.2.1 (p. 256) for CR 0-27, which ends at 27 (5.1 continues to 30).
/// Index = CR 0..=30 (CR 0 stored as 0).
const WHOLE_XP: [u64; 31] = [
    0, 200, 450, 700, 1_100, 1_800, 2_300, 2_900, 3_900, 5_000, 5_900, 7_200, 8_400, 10_000,
    11_500, 13_000, 15_000, 18_000, 20_000, 22_000, 25_000, 33_000, 41_000, 50_000, 62_000, 75_000,
    90_000, 105_000, 120_000, 135_000, 155_000,
];

impl Cr {
    /// Build a whole-number CR; `None` outside 1..=30 (0 is [`Cr::Zero`]).
    pub fn whole(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Zero),
            1..=30 => Some(Self::Whole(n)),
            _ => None,
        }
    }

    /// XP value from the SRD table (CR 0 = 0, see [`CR0_ALT_XP`]).
    pub fn xp(self) -> u64 {
        match self {
            Self::Zero => 0,
            Self::Eighth => 25,
            Self::Quarter => 50,
            Self::Half => 100,
            Self::Whole(n) => WHOLE_XP[usize::from(n.min(30))],
        }
    }

    /// Value in eighths, for comparisons with character levels.
    pub fn eighths(self) -> u32 {
        match self {
            Self::Zero => 0,
            Self::Eighth => 1,
            Self::Quarter => 2,
            Self::Half => 4,
            Self::Whole(n) => u32::from(n) * 8,
        }
    }

    /// True when the CR is strictly higher than the given character level.
    pub fn exceeds_level(self, level: u8) -> bool {
        self.eighths() > u32::from(level) * 8
    }
}

impl FromStr for Cr {
    type Err = EncounterError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || EncounterError::InvalidCr(s.to_owned());
        match s.trim() {
            "1/8" => Ok(Self::Eighth),
            "1/4" => Ok(Self::Quarter),
            "1/2" => Ok(Self::Half),
            t => t.parse::<u8>().ok().and_then(Self::whole).ok_or_else(bad),
        }
    }
}

impl fmt::Display for Cr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => f.write_str("0"),
            Self::Eighth => f.write_str("1/8"),
            Self::Quarter => f.write_str("1/4"),
            Self::Half => f.write_str("1/2"),
            Self::Whole(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CrRepr {
    Text(String),
    Number(u64),
}

impl Serialize for Cr {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Cr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = match CrRepr::deserialize(d)? {
            CrRepr::Text(t) => t,
            CrRepr::Number(n) => n.to_string(),
        };
        text.parse().map_err(serde::de::Error::custom)
    }
}
