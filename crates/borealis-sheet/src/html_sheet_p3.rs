//! Spellcasting pages: cast strip, slot trackers and the spell table per casting class, split
//! over pages as the PDF splits them ([`SpellPagePlan`], B-146).

use super::parts::{esc, fld, h2, level_name, pip};
use crate::layout::{SpellBlockPlan, SpellPagePlan};
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
    } else if c.other {
        // Aurora's "Other Spells" are listed unprepared (an empty mark), never as spellbook rows
        ("", false)
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

/// The head of a caster's block: heading, cast strip, pact slots and the slot tracker.
fn caster_head(n: usize, c: &CasterData) -> String {
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
    // a block without a casting ability (spells a feat picks in Aurora-format content) has no
    // modifier, DC or attack: the boxes stay blank to fill in, never a made-up 0
    let known = |v: String| {
        if c.ability_name.is_empty() {
            String::new()
        } else {
            v
        }
    };
    for (label, part, v) in [
        ("Modifier", "mod", known(signed(c.ability_modifier))),
        ("Save DC", "dc", known(c.dc.to_string())),
        ("Spell attack", "attack", known(signed(c.attack))),
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
    if c.has_slot_tracker() {
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
    out
}

const LEGEND: &str = "<span class=\"pip on\"></span> prepared &nbsp; <span class=\"pip ex\"></span> always prepared &nbsp; <span class=\"pip\"></span> in spellbook &nbsp; C concentration \u{b7} R ritual \u{b7} M material";

const COLUMNS: &str = "<tr><th class=\"c\" style=\"width:12pt\"></th><th>Spell</th><th>Time</th><th>Range</th><th class=\"c\">C R M</th><th>Effect</th><th>Source</th></tr>";

/// Row `idx` (1-based in `cantrips + spells`, the field id index) of a caster.
fn row_at(c: &CasterData, idx: usize) -> Option<&SpellRow> {
    let i = idx.checked_sub(1)?;
    c.cantrips
        .get(i)
        .or_else(|| c.spells.get(i.checked_sub(c.cantrips.len())?))
}

/// Level band of the table; `cont` on a page that goes on with the level of the page before.
fn level_head(c: &CasterData, level: u32, cont: bool) -> String {
    let label = if level == 0 {
        "Cantrips".to_string()
    } else {
        let slots = if cont || !c.has_slot_tracker() {
            String::new()
        } else {
            format!(
                " <span class=\"note\" style=\"font-weight:400\">\u{b7} {} slots</span>",
                c.slots[level as usize - 1]
            )
        };
        format!("{}{slots}", level_name(level))
    };
    let cont = if cont { " (cont.)" } else { "" };
    format!("<tr class=\"lvhead\"><td colspan=\"7\">{label}{cont}</td></tr>")
}

/// The part of a caster's spell table on one page. `last` is the level of the row printed last
/// before this page (it goes on as "(cont.)").
fn spell_table(n: usize, c: &CasterData, b: &SpellBlockPlan, last: &mut Option<u32>) -> String {
    let mut out = if b.head {
        format!(
            "<div class=\"sec\">{}<table class=\"spells\">{COLUMNS}",
            h2("Cantrips & spells", LEGEND)
        )
    } else {
        format!(
            "<div class=\"sec\"><div class=\"spcont\"><b dir=\"auto\">{}</b> <i>spells, continued</i></div><table class=\"spells\">{COLUMNS}",
            esc(&c.class)
        )
    };
    let mut fresh = true;
    for &idx in &b.rows {
        let Some(row) = row_at(c, idx) else {
            continue;
        };
        let same = *last == Some(row.level);
        if !same || fresh {
            out.push_str(&level_head(c, row.level, same));
        }
        fresh = false;
        *last = Some(row.level);
        out.push_str(&spell_row(c, n, idx, row));
    }
    out.push_str("</table></div>");
    out
}

/// The multiclass slot table.
fn pooled(d: &SheetData) -> String {
    let Some(p) = &d.spellcasting.pooled else {
        return String::new();
    };
    format!(
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
    )
}

/// Bodies of the spellcasting pages, one per page of `plan`; `first` is the printed number of
/// the first (for "Continued on page N").
pub(super) fn pages(d: &SheetData, plan: &[SpellPagePlan], first: u32) -> Vec<String> {
    let casters = &d.spellcasting.casters;
    let mut last: Vec<Option<u32>> = vec![None; casters.len()];
    let mut out = Vec::with_capacity(plan.len());
    for (i, page) in plan.iter().enumerate() {
        let mut body = if page.pooled {
            pooled(d)
        } else {
            String::new()
        };
        for b in &page.blocks {
            let Some(c) = b.caster.checked_sub(1).and_then(|i| casters.get(i)) else {
                continue;
            };
            if b.head {
                body.push_str(&caster_head(b.caster, c));
            }
            if b.head || !b.rows.is_empty() {
                body.push_str(&spell_table(b.caster, c, b, &mut last[b.caster - 1]));
            }
        }
        if i + 1 < plan.len() {
            let _ = write!(
                body,
                "<div class=\"note cont\">Continued on page {}</div>",
                first + i as u32 + 1
            );
        } else {
            let _ = write!(
                body,
                "<div class=\"sec grow more-spells\">{}<div class=\"lined\"></div></div>",
                h2("More spells", "")
            );
        }
        out.push(body);
    }
    out
}
