//! Card pages of the HTML preview: a 3x3 grid per page, the same cards and ids as the PDF.

use super::parts::{esc, fld};
use crate::card_model::Card;

/// Field id of one card text, like the PDF (`card.3.title`, `card.3.title@2` on later pages).
fn id(slot: usize, part: &str, page: usize) -> String {
    if page == 0 {
        format!("card.{}.{part}", slot + 1)
    } else {
        format!("card.{}.{part}@{}", slot + 1, page + 1)
    }
}

fn card(c: &Card, slot: usize, page: usize) -> String {
    let facts: String = c
        .facts
        .iter()
        .enumerate()
        .map(|(i, (k, v))| {
            format!(
                "<div class=\"cfact\"><span class=\"lbl\">{}</span> {}</div>",
                esc(k),
                fld(&id(slot, &format!("fact{}", i + 1), page), "", v)
            )
        })
        .collect();
    format!(
        "<article class=\"card\"><h3>{}</h3><div class=\"csub\">{}</div>{facts}\
         <div class=\"cbody\">{}</div>\
         <div class=\"cfoot\">{}{}</div></article>",
        fld(&id(slot, "title", page), "", &c.title),
        fld(&id(slot, "subtitle", page), "", &c.subtitle),
        fld(&id(slot, "text", page), "", &c.body),
        fld(&id(slot, "left", page), "", &c.left_footer),
        fld(&id(slot, "right", page), "r", &c.right_footer),
    )
}

/// Body of one card page.
pub(super) fn body(cards: &[Card], page: usize) -> String {
    let items: String = cards
        .iter()
        .enumerate()
        .map(|(slot, c)| card(c, slot, page))
        .collect();
    format!("<div class=\"cards\">{items}</div>")
}
