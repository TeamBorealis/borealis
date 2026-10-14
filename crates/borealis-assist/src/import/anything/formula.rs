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

#[cfg(test)]
mod tests {
    use super::*;

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
