//! THE label normaliser of the golden suite (acceptance 3e). Every label compared between a
//! Borealis sheet and an expected sheet (feature, attack, resource and option names, damage
//! expressions) goes through [`label`] or [`damage`]; numbers are never touched.
//!
//! [`label`]:
//! 1. Unicode quotes and dashes become ASCII (`’` -> `'`, `–`/`—` -> `-`).
//! 2. Parenthetical notes are dropped: `Grappler (feat)` -> `Grappler`,
//!    `Channel Divinity (2/rest)` -> `Channel Divinity`.
//! 3. A trailing `: <note>` after a known feature head is kept (it is part of the SRD name, as in
//!    `Channel Divinity: Turn Undead`), but `Level N:` prefixes of 2024 headings are dropped.
//! 4. Case is folded; every run of characters other than letters and digits becomes one space;
//!    the result is trimmed.
//!
//! 5. A comma-inverted catalogue name is put back in reading order: `Crossbow, Light` ->
//!    `light crossbow` (the SRD 5.1 pack names items as Aurora does).
//! 6. A `Feature: ` prefix is dropped (`Feature: Shelter of the Faithful`).
//!
//! [`feature`]: [`label`], and the two options of the 2014 Ability Score Improvement select,
//! `Ability Score Improvement (N)` and `Feat (N)` (class features), both read as
//! `ability score improvement` (the SRD heading the expected sheet lists at every such level).
//!
//! [`resource`]: [`label`] of the part after the last `:` (`Magic Initiate: Bless (free cast)` ->
//! `bless`), so a qualified expected name meets the engine's resource or free-cast name.
//!
//! [`damage`]: a damage expression `1d8 + 3 slashing`, `1d8+3 Slashing`, `1d8 plus 3 slashing`
//! all become `1d8+3 slashing`: `plus` -> `+`, spaces around `+`/`-` removed, case folded,
//! `+0`/`-0` dropped, whitespace collapsed.

/// The normalised form of a label (see the module docs).
pub fn label(s: &str) -> String {
    let ascii = ascii_punct(s);
    let ascii = ascii
        .trim()
        .strip_prefix("Feature: ")
        .unwrap_or(ascii.trim())
        .to_string();
    let ascii = match ascii.split_once(", ") {
        Some((a, b)) if !a.contains(' ') && !b.contains(' ') && !b.contains(',') => {
            format!("{b} {a}")
        }
        _ => ascii,
    };
    let no_paren = drop_parens(&ascii);
    let no_level = drop_level_prefix(&no_paren);
    fold(&no_level)
}

/// A feature name (see the module docs): `ty` is the engine's element type.
pub fn feature(s: &str, ty: &str) -> String {
    let l = label(s);
    if ty == "Class Feature" && (l == "feat" || l == "ability score improvement") {
        return "ability score improvement".to_string();
    }
    l
}

/// A resource name (see the module docs).
pub fn resource(s: &str) -> String {
    label(s.rsplit(':').next().unwrap_or(s))
}

/// A name as the builder matches it: [`label`] without dropping notes or `Level N:`
/// (`Feat (4)` and `Feat (6)` stay apart).
pub fn name(s: &str) -> String {
    fold(&ascii_punct(s))
}

/// The normalised form of a damage expression (see the module docs).
pub fn damage(s: &str) -> String {
    let lower = ascii_punct(s).to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let joined = words
        .iter()
        .map(|w| if *w == "plus" { "+" } else { w })
        .collect::<Vec<_>>()
        .join(" ");
    let tight = joined
        .replace(" + ", "+")
        .replace(" +", "+")
        .replace("+ ", "+")
        .replace(" - ", "-")
        .replace("- ", "-");
    let tight = tight.replace("+0 ", " ").replace("-0 ", " ");
    let tight = tight
        .strip_suffix("+0")
        .or_else(|| tight.strip_suffix("-0"))
        .unwrap_or(&tight)
        .to_string();
    tight.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ascii_punct(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02bc}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            '\u{00a0}' => ' ',
            other => other,
        })
        .collect()
}

fn drop_parens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn drop_level_prefix(s: &str) -> String {
    let t = s.trim_start();
    let Some(rest) = t.strip_prefix("Level ") else {
        return s.to_string();
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    match rest[digits.len()..].strip_prefix(':') {
        Some(after) if !digits.is_empty() => after.to_string(),
        _ => s.to_string(),
    }
}

fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.extend(c.to_lowercase());
        } else if c != '\'' {
            gap = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_fold_case_punctuation_and_notes() {
        assert_eq!(label("Grappler (feat)"), "grappler");
        assert_eq!(label("Sleight Of Hand"), label("Sleight of Hand"));
        assert_eq!(label("Level 3: Champion"), "champion");
        assert_eq!(label("Thieves’ Cant"), "thieves cant");
        assert_eq!(
            label("Channel Divinity: Turn Undead"),
            "channel divinity turn undead"
        );
        assert_eq!(label("Crossbow, Light"), label("Light crossbow"));
        assert_eq!(
            label("Feature: Shelter of the Faithful"),
            "shelter of the faithful"
        );
        assert_eq!(
            feature("Feat (8)", "Class Feature"),
            "ability score improvement"
        );
        assert_eq!(resource("Magic Initiate: Bless (free cast)"), "bless");
    }

    #[test]
    fn damage_expressions_share_one_form() {
        assert_eq!(damage("1d8 + 3 Slashing"), "1d8+3 slashing");
        assert_eq!(damage("1d8 plus 3 slashing"), "1d8+3 slashing");
        assert_eq!(damage("2d6+5 slashing"), "2d6+5 slashing");
        assert_eq!(damage("1d10 + 0 fire"), "1d10 fire");
        assert_eq!(damage("1d4 - 1 piercing"), "1d4-1 piercing");
    }
}
