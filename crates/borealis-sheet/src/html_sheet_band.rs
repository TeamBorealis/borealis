//! The 2014 page-1 band of the HTML sheet: the equipment summary beside the personality blocks,
//! split as the PDF splits them ([`crate::layout::Band`]): the same ruled lines per personality
//! block with the rest on the Details page, and the same equipment lines with "+N more on
//! page X" (B-146).

use super::parts::{esc, fld, h2, pounds, thousands};
use super::Refs;
use crate::layout::Band;
use crate::model_ids as id;
use crate::model_types::SheetData;
use std::fmt::Write;

/// Equipment and personality side by side; `--pitch` is the band's line distance.
pub(super) fn band(d: &SheetData, b: &Band, refs: Refs) -> String {
    format!(
        "<div class=\"two band\" style=\"--pitch:{:.2}pt\"><div class=\"col\">{}</div><div class=\"col\">{}</div></div>",
        b.pitch,
        equipment(d, b, refs),
        personality(b, refs)
    )
}

/// One summary line: the quantity and item fields of line `n`.
fn line(n: usize, qty: &str, name: &str) -> String {
    format!(
        "<tr><td class=\"c\">{}</td><td>{}</td></tr>",
        fld(&id::summary_equip(n, "qty"), "", qty),
        fld(&id::summary_equip(n, "name"), "", name)
    )
}

/// Carried weight, coins and the first inventory lines, as fields of their own (`summary.*`;
/// the Inventory page has the `coin.*` and `equip.*` ones).
fn equipment(d: &SheetData, b: &Band, refs: Refs) -> String {
    let g = &d.gear;
    let mut out = format!(
        "<div class=\"sec\">{}<div class=\"coins\">",
        h2("Equipment", &format!("{} lb carried", pounds(g.weight)))
    );
    for ((label, v), i) in ["CP", "SP", "EP", "GP", "PP"]
        .iter()
        .zip(g.coins)
        .zip(id::SUMMARY_COINS)
    {
        let _ = write!(
            out,
            "<div><div class=\"lbl\">{label}</div>{}</div>",
            fld(i, "f", &thousands(v))
        );
    }
    out.push_str("</div><table class=\"sum\"><tr><th class=\"c\">Qty</th><th>Item</th></tr>");
    let (shown, hidden) = b.equipment_cut(g.items.len());
    for (i, it) in g.items.iter().take(shown).enumerate() {
        out.push_str(&line(i + 1, &it.quantity.to_string(), &it.name));
    }
    if hidden == 0 {
        // Lines past the last item are blank write-ins, as the PDF's.
        for n in shown + 1..=b.rows {
            out.push_str(&line(n, "", ""));
        }
    } else {
        let page = refs
            .inventory
            .map_or(String::new(), |p| format!(" on page {p}"));
        let _ = write!(
            out,
            "<tr><td></td><td class=\"note more\">+{hidden} more{page}</td></tr>"
        );
    }
    out.push_str("</table></div>");
    out
}

/// The four personality blocks: the lines page 1 has for each, holding the text as the PDF
/// wraps it (one printed line per ruled line), and "continued on page N" when the rest of a
/// block is on the Details page.
fn personality(b: &Band, refs: Refs) -> String {
    let mut out = format!("<div class=\"sec\">{}", h2("Personality", ""));
    for t in &b.traits {
        let more = if t.tail.is_some() {
            format!(
                "<span class=\"note\">continued on page {}</span>",
                refs.details
            )
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "<div class=\"lbl blk\">{}{more}</div><div class=\"lined\" data-field=\"{}\" dir=\"auto\" style=\"height:{:.2}pt\">{}</div>",
            esc(t.title),
            t.id,
            t.lines as f32 * b.pitch,
            esc(&t.head_lines.join("\n"))
        );
    }
    out.push_str("</div>");
    out
}
