//! Element-level mapping helpers shared by the builder and (later) the compendium.

use crate::dto::{ElementRef, ElementSummary, Ruleset};
use crate::ids::ui_type;
use borealis_content::{ContentStore, Element};

const SHORT_DESCRIPTION_CHARS: usize = 160;

/// Reference form of an element.
#[must_use]
pub fn element_ref(el: &Element) -> ElementRef {
    ElementRef {
        id: el.id.clone(),
        name: el.name.clone(),
        ty: ui_type(&el.ty).to_string(),
        source: el.source.clone(),
    }
}

/// Plain text of an HTML-ish description: tags removed, whitespace collapsed.
#[must_use]
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{}...", cut.trim_end())
}

/// Picker row for an element.
#[must_use]
pub fn summary(_store: &ContentStore, el: &Element) -> ElementSummary {
    let r = element_ref(el);
    let prerequisites = Some(el.prerequisite.trim())
        .filter(|p| !p.is_empty())
        .map(plain_text);
    let rulesets = match el.ruleset {
        Some(Ruleset::Dnd2014) => vec![Ruleset::Dnd2014],
        Some(Ruleset::Dnd2024) => vec![Ruleset::Dnd2024],
        _ => vec![Ruleset::Dnd2014, Ruleset::Dnd2024],
    };
    let is_spell = el.ty == "Spell";
    ElementSummary {
        short_description: truncate(&plain_text(&el.description), SHORT_DESCRIPTION_CHARS),
        id: r.id,
        name: r.name,
        ty: r.ty,
        source_name: r.source.clone(),
        source: r.source,
        prerequisites,
        rulesets,
        level: is_spell
            .then(|| el.setter_value("level").and_then(|v| v.trim().parse().ok()))
            .flatten(),
        school: is_spell
            .then(|| el.setter_value("school").map(str::to_string))
            .flatten(),
        spell: crate::gear_facts::spell_facts(el),
        armor: crate::gear_facts::armor_facts(el),
        weapon: crate::gear_facts::weapon_facts(el),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_strips_tags_and_collapses_space() {
        assert_eq!(
            plain_text("<p>Hello  <b>brave</b>\n world</p>"),
            "Hello brave world"
        );
    }

    #[test]
    fn truncate_adds_ellipsis_only_when_needed() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdefgh", 4), "abcd...");
    }
}
