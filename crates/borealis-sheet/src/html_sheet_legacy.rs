//! The basic ("legacy") spellcasting page of the HTML preview: one page per caster with the same
//! row plan (capacities, redistribution, columns) and "+N more" note as the PDF page
//! (`layout::legacy_plan`).

use super::parts::{fld, h2, pip};
use super::HtmlOptions;
use crate::layout::{legacy_avail, legacy_dropped_note, legacy_level_title, legacy_plan, Grid};
use crate::model_common::signed_text as signed;
use crate::model_types::{CasterData, SpellRow};
use std::fmt::Write;

fn field_id(caster: usize, level: usize, row: usize, part: &str) -> String {
    format!("legacy.{caster}.{level}.{row}.{part}")
}

/// Pip state of a row, as on the full spell table.
fn pip_state(c: &CasterData, row: Option<&SpellRow>) -> &'static str {
    match row {
        None => "",
        Some(r) if r.level == 0 => "na",
        Some(r) if r.always_prepared => "ex",
        Some(r) if r.prepared || !c.prepares => "on",
        Some(_) => "",
    }
}

fn strip(c: &CasterData) -> String {
    let mut out = String::from("<div class=\"cast\">");
    for (label, value) in [
        ("Class", c.class.clone()),
        ("Ability", c.ability_name.clone()),
        ("Spell save DC", c.dc.to_string()),
        ("Spell attack", signed(c.attack)),
    ] {
        let _ = write!(
            out,
            "<div class=\"box\"><div class=\"lbl\">{label}</div><span class=\"mid\">{}</span></div>",
            super::esc(&value)
        );
    }
    out.push_str("</div>");
    out
}

/// Body of the basic page of caster `n` (1-based).
pub(super) fn body(o: &HtmlOptions, n: usize, c: &CasterData) -> String {
    let avail = legacy_avail(&Grid::new(o.paper), o.compact);
    let p = legacy_plan(c, o.include_non_prepared_spells, avail);
    let mut out = format!(
        "<div class=\"sec\">{}</div>{}",
        h2("Spellcasting (basic)", &super::esc(&c.class)),
        strip(c)
    );
    out.push_str("<div class=\"lg\">");
    for levels in &p.columns {
        out.push_str("<div>");
        for &seg in levels {
            let level = seg.level;
            let title = legacy_level_title(seg);
            let slots = match c.slots.get(level.wrapping_sub(1)) {
                Some(&k) if level > 0 && !c.slots_pooled && !seg.cont && k > 0 => {
                    format!("<span>{k} {}</span>", if k == 1 { "slot" } else { "slots" })
                }
                _ => String::new(),
            };
            let _ = write!(out, "<h3>{title}{slots}</h3>");
            for i in seg.first..seg.first + seg.count {
                let row = p.by[level].get(i).copied();
                let _ = write!(
                    out,
                    "<div class=\"lgr\">{}{}</div>",
                    pip(&field_id(n, level, i + 1, "prep"), pip_state(c, row)),
                    fld(
                        &field_id(n, level, i + 1, "name"),
                        "",
                        row.map_or("", |r| r.name.as_str())
                    )
                );
            }
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    let note = legacy_dropped_note(p.dropped);
    if !note.is_empty() {
        let _ = write!(out, "<div class=\"lgn\">{}</div>", super::esc(&note));
    }
    out
}
