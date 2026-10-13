//! Golden structural snapshots: a compact, stable text summary of the PDF for
//! {2014, 2024} x {Letter, A4} x {colour, ink} x {fillable, flatten}. Update with
//! `BOREALIS_UPDATE_GOLDEN=1 cargo test -p borealis-sheet --test golden_structure`.

mod common;
mod support;

use borealis_sheet::{render_pdf, Paper, SheetEdition, SheetOptions, SheetTheme};
use lopdf::content::Content;
use lopdf::{Document, Object};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

fn num(o: &Object) -> String {
    match o {
        Object::Integer(i) => i.to_string(),
        Object::Real(r) => format!("{r:.3}"),
        _ => "?".into(),
    }
}

/// Summary of one page: marked-content roles, text-show operators, colours.
fn page_summary(
    doc: &Document,
    page: lopdf::ObjectId,
) -> (BTreeMap<String, usize>, usize, BTreeSet<String>) {
    let bytes = doc.get_page_content(page);
    let content = Content::decode(&bytes).expect("content decodes");
    let (mut roles, mut texts, mut colours) = (BTreeMap::new(), 0, BTreeSet::new());
    for op in &content.operations {
        match op.operator.as_str() {
            "BDC" => {
                let tag = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .unwrap_or(b"?");
                *roles
                    .entry(String::from_utf8_lossy(tag).into_owned())
                    .or_insert(0) += 1;
            }
            "Tj" | "TJ" | "'" | "\"" => texts += 1,
            "rg" | "RG" | "g" | "G" | "k" | "K" => {
                let vals: Vec<String> = op.operands.iter().map(num).collect();
                colours.insert(format!("{} {}", vals.join(" "), op.operator));
            }
            _ => {}
        }
    }
    (roles, texts, colours)
}

pub fn summarize(pdf: &[u8]) -> String {
    let doc = Document::load_mem(pdf).expect("parses");
    let pages = doc.get_pages();
    let mut out = format!("pages: {}\n", pages.len());
    let mut all = BTreeSet::new();
    for (n, id) in &pages {
        let (roles, texts, colours) = page_summary(&doc, *id);
        let names = support::page_widgets(&doc, *id);
        let _ = writeln!(out, "page {n}");
        let roles: Vec<String> = roles.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let _ = writeln!(out, "  roles: {}", roles.join(" "));
        let _ = writeln!(out, "  text-ops: {texts}");
        let _ = writeln!(
            out,
            "  colours ({}): {}",
            colours.len(),
            colours.iter().cloned().collect::<Vec<_>>().join("; ")
        );
        let _ = writeln!(out, "  fields ({}):", names.len());
        for name in &names {
            let _ = writeln!(out, "    {name}");
        }
        all.extend(names);
    }
    let _ = writeln!(out, "document fields: {}", all.len());
    out
}

fn case_name(y2024: bool, a4: bool, ink: bool, fill: bool) -> String {
    format!(
        "{}_{}_{}_{}.txt",
        if y2024 { "2024" } else { "2014" },
        if a4 { "a4" } else { "letter" },
        if ink { "ink" } else { "colour" },
        if fill { "fillable" } else { "flatten" }
    )
}

#[test]
fn structural_goldens_match_and_are_deterministic() {
    let mut failures = Vec::new();
    for y2024 in [false, true] {
        let (c, s) = if y2024 {
            common::wizard_2024()
        } else {
            common::wizard_2014()
        };
        for a4 in [false, true] {
            for ink in [false, true] {
                for fill in [true, false] {
                    let o = SheetOptions {
                        edition: if y2024 {
                            SheetEdition::Y2024
                        } else {
                            SheetEdition::Y2014
                        },
                        paper: if a4 { Paper::A4 } else { Paper::Letter },
                        theme: if ink {
                            SheetTheme::Ink
                        } else {
                            SheetTheme::Colour
                        },
                        form_fillable: fill,
                        ..SheetOptions::default()
                    };
                    let first = summarize(&render_pdf(&c, &s, &o).unwrap());
                    let second = summarize(&render_pdf(&c, &s, &o).unwrap());
                    let name = case_name(y2024, a4, ink, fill);
                    assert_eq!(first, second, "{name}: summary is not deterministic");
                    if let Err(e) = support::check_text(&name, &first) {
                        failures.push(e);
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
