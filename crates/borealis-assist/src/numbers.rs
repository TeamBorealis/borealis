//! Number guard: prose may only contain numbers the engine produced.
//!
//! The model is told not to invent numbers, but nothing relies on that. Every number found in a
//! piece of generated prose must also appear in the engine-computed data the prose is about
//! (a stat trace, a delta table, ...). Signed numbers (`+2`, `-1`) must match the sign of an
//! engine value; unsigned ones match by magnitude.

use std::collections::BTreeSet;

/// A number found in prose.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Found {
    /// `Some('+')` or `Some('-')` when written with an explicit sign.
    pub sign: Option<char>,
    /// Canonical magnitude: no leading zeros, no trailing fractional zeros.
    pub magnitude: String,
}

impl std::fmt::Display for Found {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(s) = self.sign {
            write!(f, "{s}")?;
        }
        write!(f, "{}", self.magnitude)
    }
}

const WORDS: [(&str, &str); 25] = [
    ("zero", "0"),
    ("two", "2"),
    ("three", "3"),
    ("four", "4"),
    ("five", "5"),
    ("six", "6"),
    ("seven", "7"),
    ("eight", "8"),
    ("nine", "9"),
    ("ten", "10"),
    ("eleven", "11"),
    ("twelve", "12"),
    ("thirteen", "13"),
    ("fourteen", "14"),
    ("fifteen", "15"),
    ("sixteen", "16"),
    ("seventeen", "17"),
    ("eighteen", "18"),
    ("nineteen", "19"),
    ("twenty", "20"),
    ("thirty", "30"),
    ("forty", "40"),
    ("fifty", "50"),
    ("sixty", "60"),
    ("hundred", "100"),
];

fn canonical(int: &str, frac: &str) -> String {
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        int.to_string()
    } else {
        format!("{int}.{frac}")
    }
}

/// Every number in `text`: digit runs (with an optional decimal part) and spelled-out numbers
/// from "two" to "hundred" ("one" is skipped: it is mostly a pronoun).
#[must_use]
pub fn extract_numbers(text: &str) -> Vec<Found> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let int: String = chars[start..i].iter().collect();
        let mut frac = String::new();
        if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
            let fs = i + 1;
            i = fs;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            frac = chars[fs..i].iter().collect();
        }
        let sign = match start.checked_sub(1).map(|p| chars[p]) {
            Some('+') if sign_position_ok(&chars, start - 1) => Some('+'),
            Some('-' | '\u{2212}') if sign_position_ok(&chars, start - 1) => Some('-'),
            _ => None,
        };
        out.push(Found {
            sign,
            magnitude: canonical(&int, &frac),
        });
    }
    let lower = text.to_lowercase();
    for word in lower.split(|c: char| !c.is_alphabetic()) {
        if let Some((_, digits)) = WORDS.iter().find(|(w, _)| *w == word) {
            out.push(Found {
                sign: None,
                magnitude: (*digits).to_string(),
            });
        }
    }
    out
}

/// A sign belongs to the number only when it is not glued to a preceding word or digit
/// (`3-5` and `well-8` are ranges or compounds).
fn sign_position_ok(chars: &[char], pos: usize) -> bool {
    pos == 0 || !chars[pos - 1].is_alphanumeric()
}

/// The set of numbers a piece of prose may mention.
#[derive(Debug, Clone, Default)]
pub struct NumberGuard {
    magnitudes: BTreeSet<String>,
    signed: BTreeSet<(char, String)>,
}

impl NumberGuard {
    /// An empty guard (nothing is allowed).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow an engine-computed integer (as `v`, and with its own sign).
    pub fn allow_int(&mut self, v: i64) {
        self.magnitudes.insert(v.unsigned_abs().to_string());
        let sign = if v < 0 { '-' } else { '+' };
        self.signed.insert((sign, v.unsigned_abs().to_string()));
    }

    /// Allow every number written in `text` (labels, reasons, element names, ...).
    pub fn allow_text(&mut self, text: &str) {
        for f in extract_numbers(text) {
            if let Some(s) = f.sign {
                self.signed.insert((s, f.magnitude.clone()));
            }
            self.magnitudes.insert(f.magnitude);
        }
    }

    /// Numbers of `prose` that are not covered by the allowed set (empty means the prose is clean).
    #[must_use]
    pub fn unknown(&self, prose: &str) -> Vec<String> {
        let mut bad: Vec<String> = extract_numbers(prose)
            .into_iter()
            .filter(|f| {
                let magnitude_ok = self.magnitudes.contains(&f.magnitude);
                match f.sign {
                    Some(s) => !(magnitude_ok && self.signed.contains(&(s, f.magnitude.clone()))),
                    None => !magnitude_ok,
                }
            })
            .map(|f| f.to_string())
            .collect();
        bad.sort();
        bad.dedup();
        bad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_digits_decimals_signs_and_words() {
        let f = extract_numbers(
            "AC is 17 (+2 from the shield, -1 penalty, 3-5 range), 0.50 and sixteen.",
        );
        let shown: Vec<String> = f.iter().map(ToString::to_string).collect();
        assert_eq!(shown[..6], ["17", "+2", "-1", "3", "5", "0.5"]);
        assert!(shown.contains(&"16".to_string()));
    }

    #[test]
    fn signs_must_match_engine_values() {
        let mut g = NumberGuard::new();
        g.allow_int(16);
        g.allow_int(-1);
        assert!(g.unknown("16 and -1 and 1").is_empty());
        assert_eq!(g.unknown("+1 bonus"), vec!["+1"]);
        assert_eq!(g.unknown("it is 17"), vec!["17"]);
    }

    #[test]
    fn text_numbers_are_allowed() {
        let mut g = NumberGuard::new();
        g.allow_text("Fighter level 3, +2 shield");
        assert!(g.unknown("level 3 with +2").is_empty());
        assert_eq!(g.unknown("-2"), vec!["-2"]);
    }
}
