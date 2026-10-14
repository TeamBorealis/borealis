//! The resource formula dialect of the engine (`play_rules::eval_expr`): integers, `+ - * /`,
//! parentheses, `ceil(a/b)`, `max(..)`, `min(..)`, `floor(..)`, `proficiency`,
//! `<ability>:modifier` and `<class> level`. Any other word evaluates to 0, so generated formulas
//! are normalised where the meaning is unambiguous and reported otherwise.

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];
const ABBR: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];
const FUNCTIONS: [&str; 4] = ["ceil", "max", "min", "floor"];
const SEPARATORS: [char; 7] = ['+', '-', '*', '/', '(', ')', ','];

/// The vocabulary, for messages to the model.
pub(crate) const VOCABULARY: &str = "integers, + - * /, parentheses, ceil(a/b), max(a, b), \
min(a, b), floor(a), proficiency, <ability>:modifier (e.g. dexterity:modifier) and \"<class> level\" \
(e.g. \"fighter level\")";

fn ability(word: &str) -> Option<&'static str> {
    ABILITIES
        .iter()
        .position(|a| *a == word)
        .or_else(|| ABBR.iter().position(|a| *a == word))
        .map(|i| ABILITIES[i])
}

/// The expression split into atoms (`false`) and separators (`true`), whitespace kept on atoms.
fn pieces(expr: &str) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    let mut atom = String::new();
    for c in expr.chars() {
        if SEPARATORS.contains(&c) {
            if !atom.is_empty() {
                out.push((false, std::mem::take(&mut atom)));
            }
            out.push((true, c.to_string()));
        } else {
            atom.push(c);
        }
    }
    if !atom.is_empty() {
        out.push((false, atom));
    }
    out
}

fn is_function(p: &[(bool, String)], i: usize) -> bool {
    FUNCTIONS.contains(&p[i].1.trim().to_lowercase().as_str())
        && p.get(i + 1).is_some_and(|(sep, s)| *sep && s == "(")
}

/// The engine reads this atom.
fn readable(atom: &str) -> bool {
    let a = atom.trim().to_lowercase();
    if a.parse::<i32>().is_ok() || a == "proficiency" || a == "proficiency bonus" {
        return true;
    }
    if let Some(ab) = a.strip_suffix(":modifier") {
        return ability(ab.trim()).is_some();
    }
    a.strip_suffix(" level")
        .is_some_and(|c| !c.trim().is_empty())
}

/// The engine form of an atom it cannot read, when the meaning is unambiguous. `class` is the
/// class the element belongs to (for a bare `level`).
fn rewrite(atom: &str, class: Option<&str>) -> Option<String> {
    let a = atom
        .trim()
        .to_lowercase()
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(ab) = ability(&a) {
        return Some(format!("{ab}:modifier"));
    }
    for suffix in [" modifier", " mod", " score modifier"] {
        if let Some(ab) = a.strip_suffix(suffix).and_then(ability) {
            return Some(format!("{ab}:modifier"));
        }
    }
    if matches!(a.as_str(), "pb" | "prof" | "proficiency modifier") {
        return Some("proficiency".to_string());
    }
    if matches!(a.as_str(), "level" | "class level" | "your level") {
        return class.map(|c| format!("{} level", c.trim().to_lowercase()));
    }
    None
}

/// The expression with every unambiguous atom rewritten, and the `(from, to)` rewrites.
pub(crate) fn normalise(expr: &str, class: Option<&str>) -> (String, Vec<(String, String)>) {
    let p = pieces(expr);
    let mut out = String::new();
    let mut changes = Vec::new();
    for (i, (sep, text)) in p.iter().enumerate() {
        let trimmed = text.trim();
        if *sep || trimmed.is_empty() || is_function(&p, i) || readable(trimmed) {
            out.push_str(text);
            continue;
        }
        match rewrite(trimmed, class) {
            Some(new) => {
                let lead = &text[..text.len() - text.trim_start().len()];
                let trail = &text[text.trim_end().len()..];
                out.push_str(&format!("{lead}{new}{trail}"));
                changes.push((trimmed.to_string(), new));
            }
            None => out.push_str(text),
        }
    }
    (out, changes)
}

/// The atoms of `expr` the engine cannot read (it would count them as 0).
pub(crate) fn unreadable(expr: &str) -> Vec<String> {
    let p = pieces(expr);
    let mut out = Vec::new();
    if expr.trim().is_empty() {
        out.push("(empty)".to_string());
    }
    for (i, (sep, text)) in p.iter().enumerate() {
        let t = text.trim();
        if *sep || t.is_empty() || is_function(&p, i) || readable(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out
}

/// The save-DC dialect (`spell_rules::eval_formula` of the engine): `spell save DC (<class>)`,
/// or terms joined by `+`, each an integer, `proficiency` or an ability (`constitution` or
/// `constitution:modifier`). Anything else leaves the DC unknown.
pub(crate) const DC_VOCABULARY: &str =
    "\"spell save DC (<class>)\", or terms joined by + where each \
term is an integer, proficiency or an ability (e.g. \"8 + proficiency + constitution:modifier\")";

fn is_caster_dc(expr: &str) -> bool {
    expr.trim().to_lowercase().starts_with("spell save dc (")
}

fn dc_term_readable(term: &str) -> bool {
    let t = term.trim().to_lowercase();
    t.parse::<i32>().is_ok()
        || t == "proficiency"
        || t == "proficiency bonus"
        || ABILITIES.contains(&t.strip_suffix(":modifier").unwrap_or(&t))
}

/// The terms of a save-DC formula the engine cannot read (the DC would be missing).
pub(crate) fn dc_unreadable(expr: &str) -> Vec<String> {
    if expr.trim().is_empty() {
        return vec!["(empty)".to_string()];
    }
    if is_caster_dc(expr) {
        return Vec::new();
    }
    expr.split('+')
        .map(str::trim)
        .filter(|t| !dc_term_readable(t))
        .map(|t| {
            if t.is_empty() {
                "(empty term)".to_string()
            } else {
                t.to_string()
            }
        })
        .collect()
}

/// A save-DC formula with every unambiguous term rewritten ("con mod" -> constitution:modifier,
/// "PB" -> proficiency), and the `(from, to)` rewrites.
pub(crate) fn normalise_dc(expr: &str) -> (String, Vec<(String, String)>) {
    if is_caster_dc(expr) {
        return (expr.to_string(), Vec::new());
    }
    let mut changes = Vec::new();
    let terms: Vec<String> = expr
        .split('+')
        .map(|t| {
            let trimmed = t.trim();
            if dc_term_readable(trimmed) {
                return trimmed.to_string();
            }
            match rewrite(trimmed, None) {
                Some(new) => {
                    changes.push((trimmed.to_string(), new.clone()));
                    new
                }
                None => trimmed.to_string(),
            }
        })
        .collect();
    if changes.is_empty() {
        return (expr.to_string(), changes);
    }
    (terms.join(" + "), changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_dc_terms_are_checked_and_shorthands_rewritten() {
        assert!(dc_unreadable("8 + proficiency + constitution:modifier").is_empty());
        assert!(dc_unreadable("8+proficiency+wisdom").is_empty());
        assert!(dc_unreadable("spell save DC (Tidecaller)").is_empty());
        assert_eq!(dc_unreadable("8 + proficiency + con"), ["con"]);
        assert_eq!(dc_unreadable("10 + half your level"), ["half your level"]);
        assert_eq!(dc_unreadable(""), ["(empty)"]);
        let (e, ch) = normalise_dc("8 + PB + Con mod");
        assert_eq!(e, "8 + proficiency + constitution:modifier");
        assert_eq!(ch.len(), 2);
        assert!(dc_unreadable(&e).is_empty());
        assert_eq!(
            normalise_dc("8+proficiency+dexterity").0,
            "8+proficiency+dexterity"
        );
    }

    #[test]
    fn bare_abilities_and_shorthands_become_engine_atoms() {
        let (e, ch) = normalise("1 + wisdom", None);
        assert_eq!(e, "1 + wisdom:modifier");
        assert_eq!(ch.len(), 1);
        assert_eq!(
            normalise("max(1, Dex mod)", None).0,
            "max(1, dexterity:modifier)"
        );
        assert_eq!(normalise("PB*2", None).0, "proficiency*2");
        assert_eq!(
            normalise("ceil(level/2)", Some("Tidecaller")).0,
            "ceil(tidecaller level/2)"
        );
        // Already readable: untouched.
        let (e, ch) = normalise("charisma:modifier + proficiency", None);
        assert_eq!(e, "charisma:modifier + proficiency");
        assert!(ch.is_empty());
    }

    #[test]
    fn ambiguous_words_stay_and_are_reported() {
        let (e, ch) = normalise("level + 1", None);
        assert_eq!(e, "level + 1");
        assert!(ch.is_empty());
        assert_eq!(unreadable("level + 1"), ["level"]);
        assert_eq!(unreadable("half your speed"), ["half your speed"]);
        assert!(unreadable("max(1, wisdom:modifier) + tidecaller level").is_empty());
        assert_eq!(unreadable(""), ["(empty)"]);
    }
}
