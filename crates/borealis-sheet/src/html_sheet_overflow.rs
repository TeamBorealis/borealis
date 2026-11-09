//! The overflow page after page 1: the attacks and limited uses page 1 has no rows for, as the
//! PDF's overflow page (`layout::page_overflow`). Ids are the items' own `atk.N.*` / `use.N.*`
//! ids (they are not on page 1).

use super::parts::{dot, fld, h2};
use crate::layout::{ATTACK_CAPACITY, USE_CAPACITY};
use crate::model_ids as id;
use crate::model_types::SheetData;
use std::fmt::Write;

fn attacks(d: &SheetData) -> String {
    if d.combat.attacks.len() <= ATTACK_CAPACITY {
        return String::new();
    }
    let mut out = String::from(
        "<table><tr><th>Name</th><th class=\"n\">Hit / DC</th><th>Damage &amp; type</th><th>Range</th><th>Notes</th></tr>",
    );
    for (i, a) in d.combat.attacks.iter().enumerate().skip(ATTACK_CAPACITY) {
        let n = i + 1;
        let _ = write!(
            out,
            "<tr><td><b>{}</b></td><td class=\"n\">{}</td><td>{}</td><td class=\"note\">{}</td><td class=\"note\">{}</td></tr>",
            fld(&id::atk(n, "name"), "", &a.name),
            fld(&id::atk(n, "bonus"), "", &a.bonus),
            fld(&id::atk(n, "damage"), "", &a.damage),
            fld(&id::atk(n, "range"), "", &a.range),
            fld(&id::atk(n, "notes"), "", &a.notes),
        );
    }
    out.push_str("</table>");
    out
}

fn uses(d: &SheetData) -> String {
    if d.limited_uses.len() <= USE_CAPACITY {
        return String::new();
    }
    let mut out = String::from("<table><tr><th>Feature</th><th>Uses</th><th>Recharge</th></tr>");
    for (i, u) in d.limited_uses.iter().enumerate().skip(USE_CAPACITY) {
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
            super::parts::esc(&u.note())
        );
    }
    out.push_str("</table>");
    out
}

/// Body of the overflow page.
pub(super) fn body(d: &SheetData) -> String {
    format!(
        "<div class=\"sec\">{}{}{}</div>",
        h2("Attacks & limited uses (continued)", ""),
        attacks(d),
        uses(d)
    )
}
