//! Attunement restrictions: "requires attunement by a cleric, druid, or warlock".
//!
//! An item's `attunement` setter carries the printed restriction in its `addition` attribute
//! (Aurora XML and the 5etools importer both write it). The optional `requirements` attribute
//! holds the same restriction as a requirements expression (`[cleric:1]||[druid:1]`); without it
//! the restriction is read from the text: "by a/an X, Y, or Z [of <alignment> alignment]" where
//! each term is a class, a race or sub-race of the universe, or `spellcaster`. Text with any other
//! word stays a note: it is shown but not enforced.

use crate::eval::requirement_holds;
use crate::graph::Graph;
use crate::universe::{Cond, Universe};
use borealis_content::expr::RequirementsExpr;
use borealis_content::Element;

/// Name of the attunement setter.
const SETTER: &str = "attunement";
/// Requirement term of "a spellcaster" (any spellcasting block, `eval::ReqEnv::has_feature`).
const SPELLCASTER: &str = "[feature:spellcasting]";
/// Words of an alignment clause.
const ALIGNMENT_WORDS: [&str; 5] = ["lawful", "chaotic", "good", "evil", "neutral"];

/// The attunement restriction of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restriction {
    /// Printed text ("by a cleric, druid, or warlock").
    pub text: String,
    /// Requirements expression the restriction enforces; `None` = shown, not enforced.
    pub requirement: Option<String>,
}

/// The restriction of `el`, if its attunement setter names one.
#[must_use]
pub fn restriction(uni: &Universe, el: &Element) -> Option<Restriction> {
    let setter = el.setter(SETTER)?;
    let text = setter
        .attribute("addition")
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let explicit = setter
        .attribute("requirements")
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    if text.is_empty() && explicit.is_none() {
        return None;
    }
    let requirement = explicit.or_else(|| requirement_from_text(uni, &text));
    Some(Restriction { text, requirement })
}

/// True when `r` is enforced and the character of `graph` does not meet it.
#[must_use]
pub fn barred(graph: &Graph<'_>, r: &Restriction) -> bool {
    let Some(req) = &r.requirement else {
        return false;
    };
    let cond = Cond::from_parse(RequirementsExpr::parse(req), |e| e);
    !requirement_holds(&cond, &graph.env())
}

/// The restriction of an inventory line (its item, else its adorner) and whether the character
/// is barred by it (by either).
#[must_use]
pub fn line_restriction(
    uni: &Universe,
    graph: &Graph<'_>,
    item: &Element,
    adorner: Option<&Element>,
) -> (Option<Restriction>, bool) {
    let own = restriction(uni, item);
    let added = adorner.and_then(|a| restriction(uni, a));
    let is_barred = own.iter().chain(added.iter()).any(|r| barred(graph, r));
    (own.or(added), is_barred)
}

/// The requirements expression of a printed restriction, or `None` when a word is not known.
#[must_use]
pub fn requirement_from_text(uni: &Universe, text: &str) -> Option<String> {
    let t = text
        .trim()
        .trim_end_matches(['.', ')'])
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let t = t.strip_prefix("by ").unwrap_or(&t);
    let (subjects, alignment) = split_alignment(t);
    let (prefix, subjects) = leading_alignment(&subjects);
    let alignment = match (alignment, prefix) {
        (Some(a), None) | (None, Some(a)) => Some(alignment_term(uni, &a)?),
        (None, None) => None,
        (Some(_), Some(_)) => return None,
    };
    let subjects = subject_terms(uni, &subjects)?;
    match (subjects, alignment) {
        (Some(s), Some(a)) => Some(format!("({s})&&({a})")),
        (Some(s), None) => Some(s),
        (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

/// Split "X of <words> alignment" into ("X", Some("<words>")).
fn split_alignment(t: &str) -> (String, Option<String>) {
    match t
        .strip_suffix(" alignment")
        .and_then(|s| s.rsplit_once(" of "))
    {
        Some((subjects, words)) => (subjects.trim().to_string(), Some(words.trim().to_string())),
        None => (t.to_string(), None),
    }
}

/// "an evil cleric or paladin" -> (Some("evil"), "cleric or paladin").
fn leading_alignment(t: &str) -> (Option<String>, String) {
    let rest = strip_article(t);
    let mut words = Vec::new();
    let mut tail = rest;
    while let Some((w, more)) = tail.split_once(' ') {
        if !is_alignment_word(w) {
            break;
        }
        words.push(w);
        tail = more;
    }
    if words.is_empty() {
        (None, t.to_string())
    } else {
        (Some(words.join(" ")), tail.to_string())
    }
}

fn is_alignment_word(w: &str) -> bool {
    let w = w.strip_prefix("non-").unwrap_or(w);
    ALIGNMENT_WORDS.contains(&w)
}

fn strip_article(t: &str) -> &str {
    let t = t.trim();
    t.strip_prefix("a ")
        .or_else(|| t.strip_prefix("an "))
        .unwrap_or(t)
        .trim()
}

/// The alignments of the universe whose name fits `words` ("good", "non-evil", "neutral
/// good"), as an `||` of their ids, or no alignment chosen at all.
fn alignment_term(uni: &Universe, words: &str) -> Option<String> {
    if !words.split(' ').all(is_alignment_word) {
        return None;
    }
    let (negated, want) = match words.strip_prefix("non-") {
        Some(w) => (true, w),
        None => (false, words),
    };
    let all: Vec<&str> = uni.by_type("Alignment").map(|a| a.el.id.as_str()).collect();
    let ids: Vec<&str> = uni
        .by_type("Alignment")
        .filter(|a| a.el.name.to_lowercase().contains(want) != negated)
        .map(|a| a.el.id.as_str())
        .collect();
    // Without a chosen alignment the clause is not enforced (nothing to judge yet).
    (!ids.is_empty()).then(|| format!("{}||!({})", ids.join("||"), all.join("||")))
}

/// The `||` of the subject terms; `Some(None)` = "a creature" (no subject restriction),
/// `None` = an unknown word.
fn subject_terms(uni: &Universe, subjects: &str) -> Option<Option<String>> {
    let s = strip_article(subjects);
    if s.is_empty() || s == "creature" {
        return Some(None);
    }
    let mut terms = Vec::new();
    for part in s.split(',').flat_map(|p| p.split(" or ")) {
        let part = part.trim();
        let part = part.strip_prefix("or ").unwrap_or(part);
        let word = strip_article(part);
        if word.is_empty() {
            continue;
        }
        terms.push(subject_term(uni, word)?);
    }
    (!terms.is_empty()).then(|| Some(terms.join("||")))
}

/// One subject: `spellcaster`, a class name or a race / sub-race name (singular or plural).
fn subject_term(uni: &Universe, word: &str) -> Option<String> {
    let singular = word.strip_suffix('s').filter(|_| word.len() > 3);
    for w in std::iter::once(word).chain(singular) {
        if w == "spellcaster" {
            return Some(SPELLCASTER.to_string());
        }
        let named = |ty: &str| {
            uni.by_type(ty)
                .find(|e| e.el.name.trim().eq_ignore_ascii_case(w))
                .map(|e| e.el.name.trim().to_string())
        };
        if let Some(class) = named("Class") {
            return Some(format!("[{}:1]", class.to_lowercase()));
        }
        if let Some(race) = named("Race").or_else(|| named("Sub Race")) {
            return Some(format!("[feature:{race}]"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_alignment_clauses() {
        assert_eq!(
            split_alignment("an elf or half-elf of neutral good alignment"),
            (
                "an elf or half-elf".to_string(),
                Some("neutral good".to_string())
            )
        );
        assert_eq!(
            leading_alignment("an evil cleric or paladin"),
            (Some("evil".to_string()), "cleric or paladin".to_string())
        );
        assert_eq!(
            leading_alignment("a cleric"),
            (None, "a cleric".to_string())
        );
        assert!(is_alignment_word("non-good") && !is_alignment_word("creature"));
    }
}
