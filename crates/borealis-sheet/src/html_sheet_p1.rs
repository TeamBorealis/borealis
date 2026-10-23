//! Page 1 (Character) of the HTML sheet, both editions.

use super::parts::{
    ability_card, area, blank, dot, esc, fld, h2, pounds, skill_mark, sq, thousands,
};
use super::HtmlOptions;
use crate::model_common::signed_text as signed;
use crate::model_ids as id;
use crate::model_types::{Edition, EditionFields, SheetData};
use std::fmt::Write;

fn is24(d: &SheetData) -> bool {
    matches!(d.edition_fields, EditionFields::Y2024(_))
}

fn speed_text(d: &SheetData) -> String {
    let parts: Vec<String> = d
        .combat
        .speeds
        .iter()
        .enumerate()
        .map(|(i, (k, v))| {
            if i == 0 {
                format!("{v} ft")
            } else {
                format!("{k} {v} ft")
            }
        })
        .collect();
    parts.join(", ")
}

fn defenses_text(d: &SheetData) -> String {
    let x = &d.extras;
    let group = |label: &str, items: &[String]| {
        (!items.is_empty()).then(|| format!("{label}: {}", items.join(", ")))
    };
    let groups: Vec<String> = [
        group("Resistances", &x.resistances),
        group("Immunities", &x.immunities),
        group("Vulnerabilities", &x.vulnerabilities),
        group("Condition immunities", &x.condition_immunities),
        group("Advantages", &x.advantages),
    ]
    .into_iter()
    .flatten()
    .collect();
    groups.join(" \u{b7} ")
}

fn hit_dice_text(d: &SheetData) -> String {
    let parts: Vec<String> = d
        .combat
        .hit_dice
        .iter()
        .map(|h| format!("{}d{}", h.count, h.die))
        .collect();
    parts.join(" + ")
}

/// Identity line and header meta of page 1 (the only page whose header carries field ids).
pub(super) fn identity(d: &SheetData) -> (String, Vec<(String, String)>) {
    let i = &d.identity;
    let first = match &d.edition_fields {
        EditionFields::Y2014(f) => format!(
            "<b>{}</b> {}",
            fld(id::RACE, "", &f.race),
            fld(id::SUBRACE, "paren", f.subrace.as_deref().unwrap_or(""))
        ),
        EditionFields::Y2024(f) => format!(
            "<b>{}</b> {}",
            fld(id::SPECIES, "", &f.species),
            fld(id::LINEAGE, "paren", f.lineage.as_deref().unwrap_or(""))
        ),
    };
    let html = format!(
        "{first} \u{b7} <b>{}</b> {} \u{b7} <b>{}</b> background",
        fld(id::CHAR_CLASS, "", &i.class_label),
        fld(id::CHAR_SUBCLASS, "paren", &i.subclass_label),
        fld(id::CHAR_BACKGROUND, "", &i.background)
    );
    let xp_next = i
        .next_level_xp
        .map_or(String::new(), |n| thousands(u64::from(n)));
    let xp = format!(
        "{}<span class=\"note\"> / </span>{}",
        fld(id::CHAR_XP, "", &thousands(u64::from(i.experience))),
        fld(id::CHAR_XP_NEXT, "", &xp_next)
    );
    let edition = match d.edition {
        Edition::Dnd2014 => "2014",
        Edition::Dnd2024 => "2024",
    };
    let meta = vec![
        (
            "Level".into(),
            fld(id::CHAR_LEVEL, "", &i.level.to_string()),
        ),
        ("XP".into(), xp),
        ("Player".into(), fld(id::CHAR_PLAYER, "", &i.player)),
        (
            "Alignment".into(),
            fld(id::CHAR_ALIGNMENT, "", &i.alignment),
        ),
        (
            "Rules".into(),
            format!("<span class=\"edition\">{edition}</span>"),
        ),
    ];
    (html, meta)
}

fn vitals(d: &SheetData) -> String {
    let c = &d.combat;
    let hp = |v: Option<i32>, i: &str| match v {
        Some(n) => format!("<span class=\"f\" data-field=\"{i}\">{n}</span>"),
        None => blank(i, ""),
    };
    let mut out = format!(
        "<div class=\"vitals\"><div class=\"box ac\"><div class=\"lbl\">Armor class</div><span class=\"big calc\" data-field=\"{}\">{}</span>{}",
        id::AC,
        c.armor_class,
        fld(id::AC_NOTE, "note hide", &c.ac_note)
    );
    if is24(d) {
        let _ = write!(
            out,
            "<div class=\"chip\" style=\"justify-content:center;margin-top:2pt\">{}<span class=\"note\">shield</span></div>",
            sq(id::AC_SHIELD, c.shield_equipped)
        );
    }
    let _ = write!(
        out,
        "</div><div class=\"box\"><div class=\"hp\"><div><div class=\"lbl\">HP max</div><span class=\"big calc\" data-field=\"{}\">{}</span></div>\
         <div><div class=\"lbl\">Current</div>{}</div><div><div class=\"lbl\">Temporary</div>{}</div></div></div>",
        id::HP_MAX,
        c.hp_max,
        hp(d.play.current_hp, id::HP_CURRENT),
        hp(d.play.temp_hp, id::HP_TEMP)
    );
    let _ = write!(
        out,
        "<div class=\"box\"><div class=\"lbl\">Hit dice <span class=\"note\">({})</span></div>",
        fld(id::HD_TOTAL, "", &hit_dice_text(d))
    );
    for h in &c.hit_dice {
        let _ = write!(
            out,
            "<div class=\"trk\" style=\"margin:2pt 0\"><span class=\"note\" data-field=\"{}\">d{} \u{d7}{}</span>",
            id::hd_total(h.die),
            h.die,
            h.count
        );
        for n in 1..=h.count {
            out.push_str(&dot(&id::hd_spent(h.die, n), "x"));
        }
        out.push_str("</div>");
    }
    out.push_str("<div class=\"lbl\">Death saves</div>");
    for (label, cls, f) in [
        ("Success", "g", id::death_success as fn(u32) -> String),
        ("Failure", "", id::death_failure),
    ] {
        let _ = write!(
            out,
            "<div class=\"trk\"><span class=\"note\" style=\"width:24pt\">{label}</span>"
        );
        for n in 1..=3 {
            out.push_str(&dot(&f(n), cls));
        }
        out.push_str("</div>");
    }
    out.push_str("</div></div>");
    out
}

fn strip(d: &SheetData) -> String {
    let boxed = |label: &str, inner: String| {
        format!("<div class=\"box\"><div class=\"lbl\">{label}</div>{inner}</div>")
    };
    let mid = |i: &str, v: &str| {
        format!(
            "<span class=\"mid\" data-field=\"{i}\" dir=\"auto\">{}</span>",
            esc(v)
        )
    };
    let mut boxes = Vec::new();
    if is24(d) {
        boxes.push(boxed(
            "Proficiency",
            mid(id::PROF_BONUS, &signed(d.proficiency_bonus)),
        ));
    }
    boxes.push(boxed(
        "Initiative",
        mid(id::INITIATIVE, &signed(d.combat.initiative)),
    ));
    boxes.push(boxed("Speed", mid(id::SPEED, &speed_text(d))));
    boxes.push(boxed(
        "Size",
        mid(id::CHAR_SIZE, d.identity.size.as_deref().unwrap_or("")),
    ));
    if is24(d) {
        if let Some(p) = d.passives.iter().find(|p| p.key == "perception") {
            boxes.push(boxed(
                "Passive Perc.",
                mid(&id::passive(&p.key), &p.value.to_string()),
            ));
        }
        boxes.push(boxed(
            "Heroic insp.",
            format!(
                "<span class=\"insp\" data-field=\"{}\"></span>",
                id::HEROIC_INSPIRATION
            ),
        ));
    }
    format!(
        "<div class=\"strip\" style=\"--n:{}\">{}</div>",
        boxes.len(),
        boxes.concat()
    )
}

fn attacks(d: &SheetData) -> String {
    let mut out = format!(
        "<div class=\"sec\">{}<table><tr><th>Name</th><th class=\"n\">Hit / DC</th><th>Damage &amp; type</th><th>Range, properties{}</th></tr>",
        h2("Weapons & damage cantrips", ""),
        if is24(d) { ", mastery" } else { "" }
    );
    for (i, a) in d.combat.attacks.iter().enumerate() {
        let n = i + 1;
        let mut cells = String::new();
        let mut lead = false;
        let parts = [
            (id::atk(n, "range"), "", a.range.as_str()),
            (id::atk(n, "notes"), "", a.notes.as_str()),
            (
                id::atk(n, "mastery"),
                "ms",
                a.mastery.as_deref().unwrap_or(""),
            ),
        ];
        for (i, (fid, cls, v)) in parts.iter().enumerate() {
            if i == 2 && !is24(d) {
                break;
            }
            if lead && !v.is_empty() {
                cells.push_str(" \u{b7} ");
            }
            lead |= !v.is_empty();
            cells.push_str(&fld(fid, cls, v));
        }
        let _ = write!(
            out,
            "<tr><td><b>{}</b></td><td class=\"n\">{}</td><td>{}</td><td class=\"note\">{cells}</td></tr>",
            fld(&id::atk(n, "name"), "", &a.name),
            fld(&id::atk(n, "bonus"), "", &a.bonus),
            fld(&id::atk(n, "damage"), "", &a.damage),
        );
    }
    out.push_str("<tr class=\"empty\"><td></td><td></td><td></td><td></td></tr></table>");
    if let EditionFields::Y2024(f) = &d.edition_fields {
        if !f.weapon_masteries.is_empty() {
            out.push_str("<div class=\"note\">Weapon mastery: ");
            for (i, m) in f.weapon_masteries.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&fld(&id::mastery(i + 1), "", m));
            }
            out.push_str("</div>");
        }
    }
    out.push_str("</div>");
    out
}

fn senses(d: &SheetData) -> String {
    let mut rows = format!(
        "<div>Senses</div><div>{}</div><div>Defenses</div><div>{}</div><div>Deity</div><div>{}</div>",
        fld(id::SENSES, "", &d.extras.senses.join(" \u{b7} ")),
        fld(id::DEFENSES, "", &defenses_text(d)),
        fld(id::CHAR_DEITY, "", &d.identity.deity)
    );
    if let EditionFields::Y2024(f) = &d.edition_fields {
        let _ = write!(
            rows,
            "<div>Origin feat</div><div>{}</div>",
            fld(id::ORIGIN_FEAT, "", f.origin_feat.as_deref().unwrap_or(""))
        );
    }
    // 2024 prints Perception in the strip; 2014 prints it under the abilities.
    for p in d.passives.iter().filter(|p| p.key != "perception") {
        let _ = write!(
            rows,
            "<div>{}</div><div>{}</div>",
            esc(&p.label),
            fld(&id::passive(&p.key), "", &p.value.to_string())
        );
    }
    format!(
        "<div class=\"sec\">{}<div class=\"kv\">{rows}</div></div>",
        h2("Senses & defenses", "")
    )
}

fn proficiencies(d: &SheetData) -> String {
    let p = &d.proficiencies;
    let mut rows = String::new();
    if let EditionFields::Y2024(f) = &d.edition_fields {
        let t = f.armor_training;
        rows.push_str("<div>Armor training</div><div class=\"chips\">");
        for (name, on, i) in [
            ("Light", t.light, id::ARMOR_TRAINING[0]),
            ("Medium", t.medium, id::ARMOR_TRAINING[1]),
            ("Heavy", t.heavy, id::ARMOR_TRAINING[2]),
            ("Shields", t.shields, id::ARMOR_TRAINING[3]),
        ] {
            let _ = write!(rows, "<span class=\"chip\">{}{name}</span>", sq(i, on));
        }
        rows.push_str("</div>");
    }
    for (label, i, list) in [
        ("Armor", id::PROF_ARMOR, &p.armor),
        ("Weapons", id::PROF_WEAPONS, &p.weapons),
        ("Tools", id::PROF_TOOLS, &p.tools),
        ("Languages", id::PROF_LANGUAGES, &p.languages),
    ] {
        let _ = write!(
            rows,
            "<div>{label}</div><div>{}</div>",
            fld(i, "", &list.join(", "))
        );
    }
    format!(
        "<div class=\"sec\">{}<div class=\"kv\">{rows}</div></div>",
        h2("Training & proficiencies", "")
    )
}

fn conditions(d: &SheetData) -> String {
    let mut out = format!(
        "<div class=\"sec\">{}<div class=\"chips\">",
        h2("Conditions", "tick while active")
    );
    for c in d.play.conditions {
        let _ = write!(
            out,
            "<span class=\"chip\">{}{}</span>",
            sq(&id::cond(c), false),
            esc(c)
        );
    }
    out.push_str(
        "</div><div class=\"trk\" style=\"margin-top:4pt\"><b class=\"lbl\">Exhaustion</b>",
    );
    for n in 1..=d.play.exhaustion.max_levels {
        out.push_str(&dot(&id::exh(n), "x"));
    }
    let hint = if d.play.exhaustion.linear {
        "each level: \u{2212}2 to d20 tests, \u{2212}5 ft speed; 6 = death"
    } else {
        "levels add penalties (disadvantage, speed halved ...); 6 = death"
    };
    let _ = write!(out, "<span class=\"note\">&nbsp;{hint}</span></div></div>");
    out
}

fn limited_uses(d: &SheetData) -> String {
    if d.limited_uses.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "<div class=\"sec\">{}<table><tr><th>Feature</th><th>Uses</th><th>Recharge</th></tr>",
        h2("Limited uses", "tick when expended")
    );
    for (i, u) in d.limited_uses.iter().enumerate() {
        let n = i + 1;
        let _ = write!(
            out,
            "<tr><td>{}</td><td><span class=\"trk\">",
            fld(&id::use_name(n), "", &u.name)
        );
        for m in 1..=u.max {
            out.push_str(&dot(&id::use_box(n, m), "g"));
        }
        let _ = write!(
            out,
            "</span></td><td class=\"note\">{}</td></tr>",
            esc(&u.note())
        );
    }
    out.push_str("</table></div>");
    out
}

fn features_summary(d: &SheetData) -> String {
    let mut out = format!(
        "<div class=\"sec\">{}",
        h2("Features", "summary; full text on the details page")
    );
    for f in &d.features {
        let _ = write!(
            out,
            "<div class=\"feat\"><span class=\"t\" dir=\"auto\">{}</span> <span dir=\"auto\">{}</span></div>",
            esc(&f.name),
            esc(&f.summary)
        );
    }
    out.push_str("</div>");
    out
}

/// Inventory lines in the 2014 page-1 equipment summary (Aurora prints the first 15).
const EQUIPMENT_LINES_2014: usize = 15;

/// 2014 page-1 equipment summary: carried weight, coins and the first inventory lines. Printed
/// text; the fields are on the inventory page.
fn equipment_2014(d: &SheetData) -> String {
    let g = &d.gear;
    let mut out = format!(
        "<div class=\"sec\">{}<div class=\"coins\">",
        h2("Equipment", &format!("{} lb carried", pounds(g.weight)))
    );
    for (label, v) in ["CP", "SP", "EP", "GP", "PP"].iter().zip(g.coins) {
        let _ = write!(
            out,
            "<div><div class=\"lbl\">{label}</div><span class=\"f\">{}</span></div>",
            thousands(v)
        );
    }
    out.push_str("</div><table>");
    for it in g.items.iter().take(EQUIPMENT_LINES_2014) {
        let _ = write!(
            out,
            "<tr><td class=\"c\">{}</td><td dir=\"auto\">{}</td></tr>",
            it.quantity,
            esc(&it.name)
        );
    }
    let more = g.items.len().saturating_sub(EQUIPMENT_LINES_2014);
    if more > 0 {
        let _ = write!(
            out,
            "<tr><td></td><td class=\"note\">+{more} more on the inventory page</td></tr>"
        );
    }
    out.push_str("</table></div>");
    out
}

/// 2014 page-1 personality blocks (design §4).
fn personality_2014(d: &SheetData) -> String {
    let s = &d.story;
    let mut out = format!("<div class=\"sec\">{}", h2("Personality", ""));
    for (title, i, text) in [
        ("Personality traits", "story.traits", &s.traits),
        ("Ideals", "story.ideals", &s.ideals),
        ("Bonds", "story.bonds", &s.bonds),
        ("Flaws", "story.flaws", &s.flaws),
    ] {
        let _ = write!(
            out,
            "<div class=\"lbl\">{title}</div>{}",
            area(i, text, true)
        );
    }
    out.push_str("</div>");
    out
}

fn skill_list_2014(d: &SheetData) -> String {
    let mut skills: Vec<_> = d.skills.iter().collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = format!("<div class=\"sec\">{}", h2("Skills", ""));
    for s in skills {
        let _ = write!(
            out,
            "<div class=\"r s14\">{}<span>{}</span><span class=\"src\">{}</span>{}</div>",
            skill_mark(s),
            esc(&s.name),
            s.ability.to_uppercase(),
            fld(&id::skill(&s.key), "v", &signed(s.total))
        );
    }
    out.push_str("</div>");
    out
}

fn body_2024(d: &SheetData, o: &HtmlOptions) -> String {
    let mut out = String::from("<div class=\"body\"><div class=\"abil\">");
    for a in &d.abilities {
        let skills: Vec<_> = d.skills.iter().filter(|s| s.ability == a.key).collect();
        out.push_str(&ability_card(o, a, &skills, &signed));
    }
    out.push_str(
        "<div class=\"tagline\"><span class=\"pip on\"></span> proficient &nbsp; <span class=\"pip ex\"></span> expertise</div></div>",
    );
    let _ = write!(
        out,
        "<div class=\"main\">{}{}{}{}{}{}{}<div class=\"sec grow\">{}<div class=\"lined\"></div></div></div></div>",
        vitals(d),
        strip(d),
        attacks(d),
        senses(d),
        proficiencies(d),
        conditions(d),
        limited_uses(d),
        h2("Notes", ""),
    );
    out
}

fn body_2014(d: &SheetData, o: &HtmlOptions) -> String {
    let mut out = String::from("<div class=\"body d14\"><div class=\"abil\">");
    for a in &d.abilities {
        out.push_str(&ability_card(o, a, &[], &signed));
    }
    let perc = d.passives.iter().find(|p| p.key == "perception");
    let _ = write!(
        out,
        "<div class=\"box\" style=\"display:flex;align-items:center;gap:6pt\"><span class=\"insp\" style=\"margin:2pt 4pt\" data-field=\"{}\"></span><div><div class=\"lbl\">Inspiration</div><div class=\"note\">advantage on one roll</div></div></div>\
         <div class=\"box\"><div class=\"lbl\">Proficiency bonus</div><span class=\"mid\" data-field=\"{}\">{}</span></div>\
         <div class=\"box\"><div class=\"lbl\">Passive Wisdom (Perception)</div><span class=\"mid\" data-field=\"{}\">{}</span></div></div>",
        id::INSPIRATION,
        id::PROF_BONUS,
        signed(d.proficiency_bonus),
        id::passive("perception"),
        perc.map_or(String::new(), |p| p.value.to_string())
    );
    let _ = write!(
        out,
        "<div class=\"main\">{}{}<div class=\"sub\">{}<div class=\"col\">{}{}{}{}</div></div>{}{}<div class=\"two\"><div class=\"col\">{}</div><div class=\"col\">{}</div></div></div></div>",
        vitals(d),
        strip(d),
        skill_list_2014(d),
        attacks(d),
        senses(d),
        proficiencies(d),
        conditions(d),
        limited_uses(d),
        features_summary(d),
        equipment_2014(d),
        personality_2014(d),
    );
    out
}

/// Body of page 1 for the sheet's edition.
pub(super) fn body(d: &SheetData, o: &HtmlOptions) -> String {
    if is24(d) {
        body_2024(d, o)
    } else {
        body_2014(d, o)
    }
}
