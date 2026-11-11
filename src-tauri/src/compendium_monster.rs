//! The compendium's monster entry (backlog B-05, P2b): the whole stat block as description HTML for the
//! description pane (also the creator's live preview and Try it's creature): the size, type and alignment line, then
//! armor class, hit points, initiative and speed, the six scores with their modifiers, saving throws, skills, damage
//! and condition lines, senses, languages and challenge, each line as the pane prints a fact ("Speed. 30 ft."), then
//! the sections (traits, actions, reactions, legendary actions, lair actions, regional effects). Aurora has no monster
//! view (audit 10 N14), so the entry follows the description pane's own look.

use crate::companion_stat_lines::monster_lines;
use crate::dto_ext::ElementFact;
use borealis_content::monster::{Feature, LairSection, LegendaryActions, RestKind, Usage};
use borealis_content::monster_types::Ability;
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

/// `Small humanoid (goblinoid), neutral evil`; a swarm reads `Medium swarm of Tiny beasts`.
fn type_line(m: &MonsterData) -> String {
    let size = m
        .size
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    let ty = m.creature_type.to_string();
    let mut head = match m.swarm_of {
        Some(each) => format!("{size} swarm of {} {ty}s", capitalize(each.as_str())),
        None => format!("{size} {ty}"),
    };
    let tags: Vec<&str> = m
        .tags
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect();
    if !tags.is_empty() {
        head.push_str(&format!(" ({})", tags.join(", ")));
    }
    let alignment = m.alignment.trim();
    let line = if alignment.is_empty() {
        head
    } else {
        format!("{head}, {alignment}")
    };
    capitalize(line.trim())
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// `+2`, `-1`, `+0`.
fn signed(n: i32) -> String {
    if n < 0 {
        n.to_string()
    } else {
        format!("+{n}")
    }
}

/// `30 ft., climb 30 ft., fly 60 ft. (hover)` and the speed's note.
fn speed_text(m: &MonsterData) -> String {
    let s = &m.speed;
    let mut parts: Vec<String> = s.walk.map(|w| format!("{w} ft.")).into_iter().collect();
    for (name, v) in [
        ("burrow", s.burrow),
        ("climb", s.climb),
        ("fly", s.fly),
        ("swim", s.swim),
    ] {
        if let Some(v) = v {
            let hover = if name == "fly" && s.hover {
                " (hover)"
            } else {
                ""
            };
            parts.push(format!("{name} {v} ft.{hover}"));
        }
    }
    if let Some(note) = s.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        parts.push(note.to_string());
    }
    parts.join(", ")
}

const ABILITY_ORDER: [(Ability, &str); 6] = [
    (Ability::Str, "STR"),
    (Ability::Dex, "DEX"),
    (Ability::Con, "CON"),
    (Ability::Int, "INT"),
    (Ability::Wis, "WIS"),
    (Ability::Cha, "CHA"),
];

/// `Dex +4, Wis +2`: the saving throws the stat block lists.
fn saves_text(m: &MonsterData) -> String {
    ABILITY_ORDER
        .iter()
        .filter_map(|(a, _)| {
            m.saves
                .get(a)
                .map(|b| format!("{} {}", capitalize(a.as_str()), signed(*b)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `Perception +3, Stealth +4`, then the skills' notes.
fn skills_text(m: &MonsterData) -> String {
    m.skills
        .iter()
        .map(|(skill, b)| {
            let name = skill
                .as_str()
                .split(' ')
                .map(|w| {
                    if w == "of" {
                        w.to_string()
                    } else {
                        capitalize(w)
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("{name} {}", signed(*b))
        })
        .chain(
            m.skill_notes
                .iter()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
        )
        .collect::<Vec<_>>()
        .join(", ")
}

/// The challenge rating with its XP (`1/4 (50 XP)`); a creature without one (decision IM12) shows the note that
/// stands in for it, or an em dash, and no XP.
fn challenge_text(m: &MonsterData) -> String {
    match m.xp.filter(|_| m.cr.is_some()) {
        Some(xp) => format!("{} ({xp} XP)", m.challenge_text()),
        None => m.challenge_text(),
    }
}

/// The stat block's lines above the ability scores (armor class, hit points, initiative, speed) and below them
/// (saving throws to challenge), each a label and its text; a line with nothing to say is left out. Armor class and
/// hit points that are a formula show that text (decision IM12a).
pub(crate) fn stat_lines(m: &MonsterData) -> (Vec<ElementFact>, Vec<ElementFact>) {
    let initiative = m
        .initiative
        .as_ref()
        .map(|i| format!("{} ({})", signed(i.modifier), i.score))
        .unwrap_or_default();
    let top = [
        fact("Armor Class", m.armor_class_text()),
        fact("Hit Points", m.hit_points_text()),
        fact("Initiative", initiative),
        fact("Speed", speed_text(m)),
    ];
    let lines = monster_lines(m);
    let mut senses = lines.senses.clone();
    if !senses.is_empty() {
        senses.push_str(", ");
    }
    senses.push_str(&format!(
        "passive Perception {}",
        m.senses.passive_perception
    ));
    let mut bottom: Vec<Option<ElementFact>> = vec![
        fact("Saving Throws", saves_text(m)),
        fact("Skills", skills_text(m)),
    ];
    bottom.extend(lines.defenses.into_iter().map(|d| fact(&d.name, d.text)));
    bottom.extend([
        fact("Senses", senses),
        fact("Languages", lines.languages),
        fact("Challenge", challenge_text(m)),
    ]);
    (
        top.into_iter().flatten().collect(),
        bottom.into_iter().flatten().collect(),
    )
}

fn line_html(out: &mut String, f: &ElementFact) {
    out.push_str(&format!(
        "<p><i>{}.</i> {}</p>",
        escape(&f.label),
        escape(&f.value)
    ));
}

/// The six scores with their modifiers, in one table.
fn scores_html(out: &mut String, m: &MonsterData) {
    out.push_str("<table><thead><tr>");
    for (_, abbr) in ABILITY_ORDER {
        out.push_str(&format!("<th>{abbr}</th>"));
    }
    out.push_str("</tr></thead><tbody><tr>");
    for (a, _) in ABILITY_ORDER {
        out.push_str(&format!(
            "<td>{} ({})</td>",
            m.abilities.score(a),
            signed(m.modifier(a))
        ));
    }
    out.push_str("</tr></tbody></table>");
}

/// The stat block's header: the type line, the lines and the scores (module docs).
fn header_html(out: &mut String, m: &MonsterData) {
    out.push_str(&format!("<p><i>{}</i></p>", escape(&type_line(m))));
    let (top, bottom) = stat_lines(m);
    for f in &top {
        line_html(out, f);
    }
    scores_html(out, m);
    for f in &bottom {
        line_html(out, f);
    }
}

/// The stat block's sections as description HTML (every text escaped), or `None` for an element
/// that is not a readable monster.
pub(crate) fn monster_html(el: &Element) -> Option<String> {
    let m = monster(el)?;
    let mut out = String::new();
    header_html(&mut out, &m);
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

    fn lines(el: &Element) -> Vec<(String, String)> {
        let m = monster(el).expect("a monster");
        let (top, bottom) = stat_lines(&m);
        top.into_iter()
            .chain(bottom)
            .map(|f| (f.label, f.value))
            .collect()
    }

    #[test]
    fn the_stat_block_lines_come_in_stat_block_order() {
        let el = stat_block(json!({
            "speed": { "walk": 30, "climb": 20, "fly": 60, "hover": true },
            "saves": { "dex": 4, "wis": 3 }, "skills": { "perception": 3, "sleight of hand": 4 },
            "damage_resistances": [{ "types": ["cold", "fire"] }], "condition_immunities": ["charmed"],
            "senses": { "darkvision": 60, "passive_perception": 13 }, "languages": ["Common"], "telepathy": 30,
        }));
        let got = lines(&el);
        let got: Vec<(&str, &str)> = got.iter().map(|(l, v)| (l.as_str(), v.as_str())).collect();
        assert_eq!(
            got,
            [
                ("Armor Class", "13 (natural armor)"),
                ("Hit Points", "11 (2d8+2)"),
                ("Speed", "30 ft., climb 20 ft., fly 60 ft. (hover)"),
                ("Saving Throws", "Dex +4, Wis +3"),
                ("Skills", "Perception +3, Sleight of Hand +4"),
                ("Damage Resistances", "Cold, Fire"),
                ("Condition Immunities", "Charmed"),
                ("Senses", "Darkvision 60 ft., passive Perception 13"),
                ("Languages", "Common, telepathy 30 ft."),
                ("Challenge", "1/4 (50 XP)"),
            ]
        );
        let html = monster_html(&el).expect("a monster");
        assert!(
            html.starts_with("<p><i>Medium beast, unaligned</i></p><p><i>Armor Class.</i> 13 (natural armor)</p>"),
            "{html}"
        );
        assert!(
            html.contains(
                "<th>STR</th><th>DEX</th><th>CON</th><th>INT</th><th>WIS</th><th>CHA</th>"
            ),
            "{html}"
        );
        assert!(
            html.contains("<td>12 (+1)</td><td>14 (+2)</td><td>12 (+1)</td><td>3 (-4)</td><td>12 (+1)</td><td>6 (-2)</td>"),
            "{html}"
        );
        let speed = html.find("Speed.").expect("speed");
        let scores = html.find("<table>").expect("scores");
        let saves = html.find("Saving Throws.").expect("saves");
        let traits = html.find("<h4>Traits</h4>").expect("traits");
        assert!(speed < scores && scores < saves && saves < traits, "{html}");
    }

    #[test]
    fn a_2024_block_has_its_initiative_and_a_swarm_its_parts() {
        let el = stat_block(json!({
            "ruleset": "dnd2024", "size": ["medium"], "swarm_of": "tiny", "tags": ["shapechanger"],
            "alignment": "neutral", "initiative": { "modifier": 2, "score": 12 },
        }));
        assert!(lines(&el).contains(&("Initiative".to_string(), "+2 (12)".to_string())));
        let html = monster_html(&el).expect("a monster");
        assert!(
            html.starts_with("<p><i>Medium swarm of Tiny beasts (shapechanger), neutral</i></p>"),
            "{html}"
        );
        assert!(
            lines(&el).contains(&("Languages".to_string(), "\u{2014}".to_string())),
            "no languages read as a dash"
        );
    }

    #[test]
    fn lines_use_the_formula_and_note_text_without_numbers() {
        let el = stat_block(json!({
            "ac": [], "ac_note": "11 + the level of the spell", "hp": null, "hp_note": "40 + 10 for each spell level above 4th",
            "cr": null, "cr_note": "scales with the spell's level", "xp": null,
        }));
        let got = lines(&el);
        let value = |label: &str| got.iter().find(|(l, _)| l == label).map(|(_, v)| v.clone());
        assert_eq!(
            value("Armor Class").as_deref(),
            Some("11 + the level of the spell")
        );
        assert_eq!(
            value("Hit Points").as_deref(),
            Some("40 + 10 for each spell level above 4th")
        );
        assert_eq!(
            value("Challenge").as_deref(),
            Some("scales with the spell's level")
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
    }
}
