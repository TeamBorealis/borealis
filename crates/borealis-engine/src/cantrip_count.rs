//! Which cantrip selects count toward a class's cantrips known (decision D-21).
//!
//! SRD 5.1 Pact of the Tome: "choose three cantrips from any class's spell list ... They don't
//! count against your number of cantrips known." A class's cantrip maximum is its table count,
//! plus features that add an extra cantrip from the class's own list (2024 Thaumaturge,
//! Magician: `$(spellcasting:list), 0`). Content marks the difference only by the select's
//! list: a Spell select whose `supports` names spell levels and nothing else (`0`) is open to
//! every class's list, so its cantrips are listed with the class but counted apart. Selects
//! naming a list (`$(spellcasting:list),0`, `Wizard,0`) or a school or tag count as usual.

use borealis_content::SelectRule;

/// A `supports` term that only names spell levels: `0`, `(0||1)`.
fn is_level_term(term: &str) -> bool {
    let t = term.trim();
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace() || matches!(c, '(' | ')' | '|'))
}

/// Do the picks of this Spell select count toward the class's cantrips known?
#[must_use]
pub fn counts_toward_cantrips_known(rule: &SelectRule) -> bool {
    let Some(supports) = rule.supports.as_deref() else {
        return true;
    };
    !supports.split(',').all(is_level_term)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn select(supports: Option<&str>) -> SelectRule {
        SelectRule {
            ty: "Spell".into(),
            supports: supports.map(str::to_string),
            ..SelectRule::default()
        }
    }

    #[test]
    fn any_list_cantrips_do_not_count() {
        assert!(!counts_toward_cantrips_known(&select(Some("0"))));
        assert!(!counts_toward_cantrips_known(&select(Some(" (0||1) "))));
    }

    #[test]
    fn class_list_school_and_tag_cantrips_count() {
        for s in [
            "$(spellcasting:list),0",
            "$(spellcasting:list), 0",
            "Wizard,0",
            "0, Ritual",
            "",
        ] {
            assert!(counts_toward_cantrips_known(&select(Some(s))), "{s}");
        }
        assert!(counts_toward_cantrips_known(&select(None)));
    }
}
