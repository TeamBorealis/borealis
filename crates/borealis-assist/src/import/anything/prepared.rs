//! The outline's prepared-spells formula as printed ("Charisma modifier (minimum of one
//! spell)", "your Wisdom modifier + your druid level") in the terms the caster builder reads
//! (`mod`, `level`, `half`, `third`, numbers joined by `+`). A printed formula the builder could
//! not read fell back to `mod + level`, one spell per level too many for a modifier-only caster.

const ABILITIES: [&str; 12] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
    "str",
    "dex",
    "con",
    "int",
    "wis",
    "cha",
];

/// Text without its parentheticals ("(minimum of one)", "(rounded down)").
fn without_parentheticals(s: &str) -> String {
    let mut depth = 0usize;
    s.chars()
        .filter(|c| match c {
            '(' => {
                depth += 1;
                false
            }
            ')' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect()
}

fn term(t: &str) -> Option<String> {
    let words: Vec<&str> = t
        .split(|c: char| !c.is_alphanumeric() && c != '/')
        .filter(|w| !w.is_empty())
        .collect();
    let has = |w: &str| words.contains(&w);
    if has("mod") || has("modifier") || has("ability") || ABILITIES.iter().any(|a| has(a)) {
        return Some("mod".into());
    }
    if has("half") || t.contains("/2") {
        return Some("half".into());
    }
    if has("third") || t.contains("/3") {
        return Some("third".into());
    }
    if has("level") || has("levels") {
        return Some("level".into());
    }
    words
        .iter()
        .find_map(|w| w.parse::<i32>().ok())
        .map(|n| n.to_string())
}

/// The builder's terms for a printed formula; the text unchanged when no term is read.
pub(crate) fn canonical(formula: &str) -> String {
    let clean = without_parentheticals(&formula.to_lowercase()).replace(" plus ", " + ");
    let terms: Vec<String> = clean.split('+').filter_map(term).collect();
    if terms.is_empty() {
        formula.to_string()
    } else {
        terms.join(" + ")
    }
}

#[cfg(test)]
mod tests {
    use super::canonical;

    #[test]
    fn printed_formulas_become_builder_terms() {
        assert_eq!(canonical("Charisma modifier (minimum of one spell)"), "mod");
        assert_eq!(
            canonical("your Wisdom modifier + your warden level"),
            "mod + level"
        );
        assert_eq!(
            canonical("half your warden level, rounded down, plus your Intelligence modifier"),
            "half + mod"
        );
        assert_eq!(canonical("mod + third"), "mod + third");
        assert_eq!(canonical("mod + 1"), "mod + 1");
        assert_eq!(canonical("see the table"), "see the table");
    }
}
