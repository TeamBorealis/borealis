//! The compendium's monster entry (backlog B-05): AC / HP / CR facts and the stat block's
//! sections (traits, actions, reactions, legendary actions, lair actions, regional effects) as
//! description HTML for the description pane.

use crate::dto_ext::ElementFact;
use borealis_content::monster::{Feature, LairSection, LegendaryActions, RestKind, Usage};
use borealis_content::{Element, MonsterData};

const MONSTER_TYPE: &str = "Monster";

fn monster(el: &Element) -> Option<MonsterData> {
    (el.ty == MONSTER_TYPE)
        .then(|| MonsterData::from_element(el).ok())
        .flatten()
}

fn fact(label: &str, value: String) -> Option<ElementFact> {
    let value = value.trim();
    (!value.is_empty()).then(|| ElementFact {
        label: label.into(),
        value: value.into(),
    })
}

/// A typed stat block's armor class and hit points as printed (`13 (natural armor)`,
/// `11 (2d8+2)`), or the formula text that stands in for one (decision IM12a), then its
/// challenge rating with its XP (`1/4 (50 XP)`); a creature without one (decision IM12) shows
/// the note that stands in for it, or an em dash, and no XP.
pub(crate) fn monster_facts(el: &Element) -> Vec<ElementFact> {
    let Some(m) = monster(el) else {
        return Vec::new();
    };
    let challenge = match m.xp.filter(|_| m.cr.is_some()) {
        Some(xp) => format!("{} ({xp} XP)", m.challenge_text()),
        None => m.challenge_text(),
    };
    [
        fact("Armor Class", m.armor_class_text()),
        fact("Hit Points", m.hit_points_text()),
        fact("Challenge", challenge),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The stat block's sections as description HTML (every text escaped), or `None` for an element
/// that is not a readable monster.
pub(crate) fn monster_html(el: &Element) -> Option<String> {
    let m = monster(el)?;
    let mut out = String::new();
    features(&mut out, "Traits", &m.traits);
    features(&mut out, "Actions", &m.actions);
    features(&mut out, "Bonus actions", &m.bonus_actions);
    features(&mut out, "Reactions", &m.reactions);
    if let Some(l) = &m.legendary {
        legendary(&mut out, l);
    }
    if let Some(s) = &m.lair_actions {
        lair(&mut out, "Lair actions", s);
    }
    if let Some(s) = &m.regional_effects {
        lair(&mut out, "Regional effects", s);
    }
    Some(out)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// One text with its line breaks kept (spell lists hold one list per line).
fn lines(text: &str) -> String {
    text.trim()
        .lines()
        .map(|l| escape(l.trim()))
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Paragraphs separated by a blank line.
fn paragraphs(out: &mut String, text: &str) {
    for p in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        out.push_str(&format!("<p>{}</p>", lines(p)));
    }
}

fn usage_text(u: &Usage) -> String {
    match u {
        Usage::Recharge { min, max } if min == max => format!("Recharge {min}"),
        Usage::Recharge { min, max } => format!("Recharge {min}\u{2013}{max}"),
        Usage::PerDay(n) => format!("{n}/Day"),
        Usage::PerDayInLair { normal, lair } => format!("{normal}/Day, or {lair}/Day in Lair"),
        Usage::OnRest(RestKind::ShortOrLong) => "Recharges after a Short or Long Rest".into(),
        Usage::OnRest(RestKind::Long) => "Recharges after a Long Rest".into(),
    }
}

/// `Name (Recharge 5–6).` with the usage, the condition and a legendary cost in parentheses.
fn feature_name(f: &Feature) -> String {
    let mut notes: Vec<String> = Vec::new();
    if let Some(c) = f.condition.as_deref().filter(|c| !c.trim().is_empty()) {
        notes.push(c.trim().to_string());
    }
    if let Some(u) = &f.usage {
        notes.push(usage_text(u));
    }
    match f.cost {
        Some(n) if n > 1 => notes.push(format!("Costs {n} Actions")),
        _ => {}
    }
    let mut name = f.name.trim().to_string();
    if !notes.is_empty() {
        name.push_str(&format!(" ({})", notes.join("; ")));
    }
    if !name.ends_with('.') {
        name.push('.');
    }
    name
}

fn feature(out: &mut String, f: &Feature) {
    out.push_str(&format!(
        "<p><b><i>{}</i></b> {}</p>",
        escape(&feature_name(f)),
        lines(&f.text)
    ));
}

fn features(out: &mut String, title: &str, list: &[Feature]) {
    if list.is_empty() {
        return;
    }
    out.push_str(&format!("<h4>{title}</h4>"));
    for f in list {
        feature(out, f);
    }
}

fn legendary(out: &mut String, l: &LegendaryActions) {
    if l.actions.is_empty() && l.preamble.trim().is_empty() {
        return;
    }
    out.push_str("<h4>Legendary actions</h4>");
    paragraphs(out, &l.preamble);
    for f in &l.actions {
        feature(out, f);
    }
}

fn lair(out: &mut String, title: &str, s: &LairSection) {
    if s.options.is_empty() && s.preamble.trim().is_empty() && s.closing.trim().is_empty() {
        return;
    }
    out.push_str(&format!("<h4>{title}</h4>"));
    paragraphs(out, &s.preamble);
    if !s.options.is_empty() {
        out.push_str("<ul>");
        for o in &s.options {
            match o.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                Some(n) => out.push_str(&format!(
                    "<li><b><i>{}.</i></b> {}</li>",
                    escape(n.trim_end_matches('.')),
                    lines(&o.text)
                )),
                None => out.push_str(&format!("<li>{}</li>", lines(&o.text))),
            }
        }
        out.push_str("</ul>");
    }
    paragraphs(out, &s.closing);
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::monster::LairOption;
    use serde_json::json;

    fn stat_block(extra: serde_json::Value) -> Element {
        let mut data = json!({
            "ruleset": "dnd2014", "size": ["medium"], "creature_type": "beast", "alignment": "unaligned",
            "ac": [{ "value": 13, "source": "natural armor" }], "hp": { "average": 11, "formula": "2d8+2" },
            "speed": { "walk": 30 }, "abilities": { "str": 12, "dex": 14, "con": 12, "int": 3, "wis": 12, "cha": 6 },
            "senses": { "passive_perception": 11 }, "cr": "1/4", "xp": 50,
            "traits": [{ "name": "Keen Smell", "text": "It notices <odd> scents." }],
            "actions": [{ "name": "Bite", "text": "Melee Weapon Attack: +4 to hit." }],
        });
        if let (Some(d), Some(e)) = (data.as_object_mut(), extra.as_object()) {
            d.extend(e.clone());
        }
        let mut el = Element::new("ID_TEST_BEAST", "Test Beast", "Monster", "Test");
        el.data = data
            .as_object()
            .map(|m| m.clone().into_iter().collect())
            .unwrap_or_default();
        el
    }

    #[test]
    fn facts_show_ac_hp_and_cr_with_xp() {
        let facts = monster_facts(&stat_block(json!({})));
        let pairs: Vec<(&str, &str)> = facts
            .iter()
            .map(|f| (f.label.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("Armor Class", "13 (natural armor)"),
                ("Hit Points", "11 (2d8+2)"),
                ("Challenge", "1/4 (50 XP)")
            ]
        );
    }

    #[test]
    fn facts_use_the_formula_and_note_text_without_numbers() {
        let el = stat_block(json!({
            "ac": [], "ac_note": "11 + the level of the spell", "hp": null, "hp_note": "40 + 10 for each spell level above 4th",
            "cr": null, "cr_note": "scales with the spell's level", "xp": null,
        }));
        let values: Vec<String> = monster_facts(&el).into_iter().map(|f| f.value).collect();
        assert_eq!(
            values,
            [
                "11 + the level of the spell",
                "40 + 10 for each spell level above 4th",
                "scales with the spell's level"
            ]
        );
    }

    #[test]
    fn html_lists_every_section_in_stat_block_order_and_escapes_text() {
        let mut el = stat_block(json!({
            "reactions": [{ "name": "Parry", "text": "Adds 2 to its AC." }],
            "bonus_actions": [{ "name": "Dash", "text": "It moves.", "usage": { "recharge": { "min": 5, "max": 6 } } }],
            "legendary": { "per_round": 3, "preamble": "It can take 3 legendary actions.", "actions": [
                { "name": "Detect", "text": "It looks around." }, { "name": "Swipe", "text": "It swipes.", "cost": 2 } ] },
        }));
        let lair = LairSection {
            preamble: "On initiative count 20 the lair acts.".into(),
            options: vec![LairOption {
                name: Some("Tremor".into()),
                text: "The ground shakes.".into(),
            }],
            closing: String::new(),
        };
        let regional = LairSection {
            preamble: String::new(),
            options: vec![LairOption {
                name: None,
                text: "Water within 1 mile turns cold.".into(),
            }],
            closing: "The effects end when it dies.".into(),
        };
        el.data.insert(
            "lair_actions".into(),
            serde_json::to_value(&lair).expect("lair"),
        );
        el.data.insert(
            "regional_effects".into(),
            serde_json::to_value(&regional).expect("regional"),
        );
        let html = monster_html(&el).expect("a monster");
        let order = [
            "<h4>Traits</h4>",
            "<h4>Actions</h4>",
            "<h4>Bonus actions</h4>",
            "<h4>Reactions</h4>",
            "<h4>Legendary actions</h4>",
            "<h4>Lair actions</h4>",
            "<h4>Regional effects</h4>",
        ];
        let at: Vec<usize> = order
            .iter()
            .map(|h| {
                html.find(h)
                    .unwrap_or_else(|| panic!("{h} missing in {html}"))
            })
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{html}");
        assert!(html.contains("<p><b><i>Keen Smell.</i></b> It notices &lt;odd&gt; scents.</p>"));
        assert!(html.contains("<b><i>Dash (Recharge 5\u{2013}6).</i></b>"));
        assert!(html.contains("<b><i>Swipe (Costs 2 Actions).</i></b>"));
        assert!(html.contains("<li><b><i>Tremor.</i></b> The ground shakes.</li>"));
        assert!(html.contains(
            "<li>Water within 1 mile turns cold.</li></ul><p>The effects end when it dies.</p>"
        ));
    }

    #[test]
    fn other_elements_have_no_stat_block() {
        let el = Element::new("ID_X", "X", "Feat", "Test");
        assert!(monster_html(&el).is_none());
        assert!(monster_facts(&el).is_empty());
    }
}
