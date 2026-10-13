//! Spellcasting page: cast strip, slot trackers and the spell table per casting class.

use super::parts::{esc, fld, h2, level_name, pip};
use super::HtmlOptions;
use crate::model_common::signed_text as signed;
use crate::model_ids as id;
use crate::model_types::{CasterData, SheetData, SpellRow};
use std::fmt::Write;

fn slot_row(
    total_id: &dyn Fn(usize) -> String,
    used_id: &dyn Fn(usize, i32) -> String,
    slots: &[i32; 9],
) -> String {
    let mut out = String::from("<div class=\"slots\">");
    for (i, n) in slots.iter().enumerate() {
        let lvl = i + 1;
        let _ = write!(
            out,
            "<div class=\"box{}\"><div class=\"lv\">{}</div><div class=\"trk\" style=\"justify-content:center;min-height:8pt\">",
            if *n > 0 { "" } else { " off" },
            fld(&total_id(lvl), "", &n.to_string())
        );
        for k in 1..=*n {
            let _ = write!(
                out,
                "<span class=\"dot\" data-field=\"{}\"></span>",
                used_id(lvl, k)
            );
        }
        out.push_str("</div></div>");
    }
    out.push_str("</div>");
    out
}

/// "C R M" flags from the facts.
fn crm(row: &SpellRow) -> String {
    let Some(f) = &row.facts else {
        return String::new();
    };
    let material = f
        .components
        .as_deref()
        .is_some_and(|c| c.split([',', '(']).any(|t| t.trim() == "M"));
    [(f.concentration, "C"), (f.ritual, "R"), (material, "M")]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join(" ")
}

fn spell_row(c: &CasterData, n: usize, idx: usize, row: &SpellRow) -> String {
    let facts = row.facts.clone().unwrap_or_default();
    let effect = facts
        .effect
        .clone()
        .or_else(|| row.damage.clone())
        .unwrap_or_default();
    let (state, book) = if row.level == 0 {
        ("na", false)
    } else if row.always_prepared {
        ("ex", false)
    } else if row.prepared || !c.prepares {
        ("on", false)
    } else {
        ("", true)
    };
    let source = row.source.clone().unwrap_or_else(|| c.class.clone());
    let f = |part: &str, cls: &str, v: &str| fld(&id::spell_row(n, idx, part), cls, v);
    format!(
        "<tr class=\"{}\"><td class=\"c\">{}</td><td class=\"sp\">{}<span class=\"sr\">{}</span></td><td>{}</td><td>{}</td><td class=\"c\"><b>{}</b></td><td>{}</td><td class=\"src\" dir=\"auto\">{}</td></tr>",
        if book { "book" } else { "" },
        pip(&id::spell_row(n, idx, "prep"), state),
        f("name", "", &row.name),
        f("level", "", &row.level.to_string()),
        f("time", "", facts.casting_time.as_deref().unwrap_or("")),
        f("range", "", facts.range.as_deref().unwrap_or("")),
        f("comp", "", &crm(row)),
        f("effect", "", &effect),
        esc(&source)
    )
}

fn caster_block(d: &SheetData, o: &HtmlOptions, n: usize, c: &CasterData) -> String {
    let prepared = match c.prepare_limit {
        Some(l) if c.prepares => format!("{} / {l}", c.prepared_count),
        _ if c.prepares => c.prepared_count.to_string(),
        _ => String::new(),
    };
    let mut out = format!(
        "<div class=\"sec\">{}</div><div class=\"cast\"><div class=\"box l\"><div class=\"lbl\">Casting ability</div><span class=\"mid\" data-field=\"{}\">{}</span></div>",
        h2(
            "Spellcasting",
            &fld(&id::spell_head(n, "class"), "", &c.class)
        ),
        id::spell_head(n, "ability"),
        esc(&c.ability_name)
    );
    for (label, part, v) in [
        ("Modifier", "mod", signed(c.ability_modifier)),
        ("Save DC", "dc", c.dc.to_string()),
        ("Spell attack", "attack", signed(c.attack)),
        ("Prepared", "prepared", prepared),
    ] {
        let _ = write!(
            out,
            "<div class=\"box\"><div class=\"lbl\">{label}</div><span class=\"mid\" data-field=\"{}\">{}</span></div>",
            id::spell_head(n, part),
            esc(&v)
        );
    }
    out.push_str("</div>");
    if let Some(p) = c.pact {
        let _ = write!(
            out,
            "<div class=\"box\"><span class=\"lbl\">Pact magic</span> <b data-field=\"{}\">{}</b> slots of level <b data-field=\"{}\">{}</b> <span class=\"trk\" style=\"display:inline-flex\">",
            id::spell_pact(n, "total"),
            p.count,
            id::spell_pact(n, "level"),
            p.level
        );
        for k in 1..=p.count {
            let _ = write!(
                out,
                "<span class=\"dot\" data-field=\"{}\"></span>",
                id::spell_pact_used(n, k)
            );
        }
        out.push_str("</span></div>");
    }
    if !c.slots_pooled {
        let _ = write!(
            out,
            "<div class=\"sec\">{}{}</div>",
            h2("Spell slots", "tick when expended"),
            slot_row(
                &|l| id::spell_slot_total(n, l),
                &|l, k| id::spell_slot_used(n, l, k),
                &c.slots
            )
        );
    }
    out.push_str(&spell_table(d, o, n, c));
    out
}

fn spell_table(d: &SheetData, o: &HtmlOptions, n: usize, c: &CasterData) -> String {
    let _ = d;
    let legend = "<span class=\"pip on\"></span> prepared &nbsp; <span class=\"pip ex\"></span> always prepared &nbsp; <span class=\"pip\"></span> in spellbook &nbsp; C concentration \u{b7} R ritual \u{b7} M material";
    let mut out = format!(
        "<div class=\"sec\">{}<table><tr><th class=\"c\" style=\"width:12pt\"></th><th>Spell</th><th>Time</th><th>Range</th><th class=\"c\">C R M</th><th>Effect</th><th>Source</th></tr>",
        h2("Cantrips & spells", legend)
    );
    let mut rows: Vec<(usize, &SpellRow)> = c
        .cantrips
        .iter()
        .enumerate()
        .map(|(i, r)| (i + 1, r))
        .collect();
    let base = c.cantrips.len();
    rows.extend(
        c.spells
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                !c.prepares || o.include_non_prepared_spells || s.prepared || s.always_prepared
            })
            .map(|(i, r)| (base + i + 1, r)),
    );
    let mut last = None;
    for (idx, row) in rows {
        if last != Some(row.level) {
            last = Some(row.level);
            let label = if row.level == 0 {
                "Cantrips".to_string()
            } else {
                let slots = if c.slots_pooled {
                    String::new()
                } else {
                    format!(
                        " <span class=\"note\" style=\"font-weight:400\">\u{b7} {} slots</span>",
                        c.slots[row.level as usize - 1]
                    )
                };
                format!("{}{slots}", level_name(row.level))
            };
            let _ = write!(
                out,
                "<tr class=\"lvhead\"><td colspan=\"7\">{label}</td></tr>"
            );
        }
        out.push_str(&spell_row(c, n, idx, row));
    }
    out.push_str("</table></div>");
    out
}

/// Whole spellcasting page body.
pub(super) fn body(d: &SheetData, o: &HtmlOptions) -> String {
    let mut out = String::new();
    if let Some(p) = &d.spellcasting.pooled {
        let _ = write!(
            out,
            "<div class=\"sec\">{}{}</div>",
            h2(
                "Multiclass spell slots",
                &format!(
                    "caster level <span class=\"fv\" data-field=\"{}\">{}</span>",
                    id::POOL_LEVEL,
                    p.caster_level
                )
            ),
            slot_row(&id::pool_slot_total, &id::pool_slot_used, &p.slots)
        );
    }
    for (i, c) in d.spellcasting.casters.iter().enumerate() {
        out.push_str(&caster_block(d, o, i + 1, c));
    }
    let _ = write!(
        out,
        "<div class=\"sec grow\">{}<div class=\"lined\"></div></div>",
        h2("More spells", "")
    );
    out
}
