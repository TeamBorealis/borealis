//! The spell lists a spellcasting list names. A class declares its list as `spellcasting.list.supports`: usually one
//! plain tag (`Wizard`), sometimes an expression (`Wizard,(Abjuration||Evocation)`, `(Cleric||Wizard)`). The tags of
//! the expression that are lists are what a spell carries to be on it; a spell's school, its level and a spell id are
//! matched from the spell itself (the engine's implicit spell tags), so they are not lists.

use borealis_content::expr::SupportsExpr;

/// The picker type whose rows are the spell lists loaded and drafted classes declare (`homebrew_reference_options`):
/// the spell form's Lists check boxes and the spellcasting section's Spell list suggestions. Not an element type.
pub const SPELL_LIST_PICKER: &str = "Spell List";

/// The eight schools (a school tag in a list expression filters by the spell's school, any case).
const SCHOOLS: [&str; 8] = [
    "Abjuration",
    "Conjuration",
    "Divination",
    "Enchantment",
    "Evocation",
    "Illusion",
    "Necromancy",
    "Transmutation",
];

/// A tag the engine gives every spell from its own setters (`universe::spell_implicit_supports`), not a list.
fn is_spell_fact(tag: &str) -> bool {
    tag.starts_with("ID_")
        || tag.starts_with('$')
        || tag.chars().all(|c| c.is_ascii_digit())
        || tag.eq_ignore_ascii_case("Ritual")
        || SCHOOLS.iter().any(|s| s.eq_ignore_ascii_case(tag))
}

/// The list tags a `spellcasting.list.supports` names, in written order, each once; empty for a spell id, an
/// expression that can't be read, or one that filters by spell facts only.
#[must_use]
pub fn list_tags(supports: &str) -> Vec<String> {
    let Ok(Some(expr)) = SupportsExpr::parse(supports) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for tag in expr.tags() {
        let tag = tag.trim();
        if !tag.is_empty() && !is_spell_fact(tag) && !out.iter().any(|t| t == tag) {
            out.push(tag.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::list_tags;

    #[test]
    fn a_plain_list_is_its_own_tag() {
        assert_eq!(list_tags("Wizard"), ["Wizard"]);
        assert_eq!(list_tags(" Fixture Sorcerer "), ["Fixture Sorcerer"]);
    }

    #[test]
    fn an_expression_names_its_list_tags_but_not_spell_facts() {
        assert_eq!(list_tags("Wizard,(Abjuration||Evocation)"), ["Wizard"]);
        assert_eq!(list_tags("(Cleric||Wizard)"), ["Cleric", "Wizard"]);
        assert_eq!(
            list_tags("(Wizard||Bard),(abjuration||Illusion),1,Ritual"),
            ["Wizard", "Bard"]
        );
        assert_eq!(list_tags("Wizard||Wizard"), ["Wizard"]);
    }

    #[test]
    fn ids_and_unreadable_text_name_no_list() {
        assert!(list_tags("ID_PHB_SPELL_COMMAND").is_empty());
        assert!(list_tags("(Abjuration||Evocation)").is_empty());
        assert!(list_tags("").is_empty());
        assert!(list_tags("(Wizard").is_empty());
    }
}
