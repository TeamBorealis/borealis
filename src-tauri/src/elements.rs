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
    plain_prefix(html, usize::MAX)
}

/// The first `limit` characters of [`plain_text`], made without the rest (a long description
/// is cut to a short one for every picker row).
fn plain_prefix(html: &str, limit: usize) -> String {
    // the text is never longer than the html (a tag becomes one space); a char is 1-4 bytes
    let mut out = String::with_capacity(html.len().min(limit.saturating_mul(4)));
    let mut n = 0;
    let mut in_tag = false;
    // whitespace seen after some text: one space before the next character
    let mut gap = false;
    for c in html.chars() {
        let c = match c {
            '<' => {
                in_tag = true;
                continue;
            }
            '>' if in_tag => {
                in_tag = false;
                ' '
            }
            _ if in_tag => continue,
            c => c,
        };
        if c.is_whitespace() {
            gap |= n > 0;
            continue;
        }
        for c in [' ', c].into_iter().skip(usize::from(!gap)) {
            if n == limit {
                return out;
            }
            out.push(c);
            n += 1;
        }
        gap = false;
    }
    out
}

/// [`plain_text`] cut to `max` characters by [`truncate`].
fn short_text(html: &str, max: usize) -> String {
    truncate(&plain_prefix(html, max.saturating_add(1)), max)
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
        short_description: short_text(&el.description, SHORT_DESCRIPTION_CHARS),
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

    /// The text the earlier full-string version made (tags to spaces, then collapsed).
    fn reference(html: &str) -> String {
        let mut out = String::new();
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

    #[test]
    fn plain_text_and_its_short_form_match_the_full_string_version() {
        let cases = [
            "",
            "   ",
            "<p></p>",
            "a",
            " a ",
            "<p>Hello  <b>brave</b>\n world</p>",
            "x>y <i>z</i>> <<b>>q",
            "tab\tand\u{a0}nbsp\u{2003}em <br/>end ",
            "<p>unterminated <b tag",
            "\u{e9}t\u{e9} \u{1f600} <i>caf\u{e9}</i>",
        ];
        let long = "<p>word </p>".repeat(80);
        for html in cases.iter().copied().chain([long.as_str()]) {
            let want = reference(html);
            assert_eq!(plain_text(html), want, "{html:?}");
            for max in [0, 1, 2, 3, 5, 7, 160] {
                assert_eq!(
                    short_text(html, max),
                    truncate(&want, max),
                    "{html:?} {max}"
                );
            }
        }
    }

    #[test]
    fn truncate_adds_ellipsis_only_when_needed() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdefgh", 4), "abcd...");
    }
}
