//! Features, equipment and inventory sections (2024 page 2, 2014 details and inventory pages).

use super::parts::{area, esc, fld, h2, pounds, sq};
use super::HtmlOptions;
use crate::model_ids as id;
use crate::model_types::{FeatureData, FeatureGroup, SheetData};
use std::fmt::Write;

/// Feature text longer than this prints its summary on the 2024 features page (space budget).
const LONG_TEXT: usize = 420;

fn feature_html(o: &HtmlOptions, full: bool, n: usize, f: &FeatureData) -> String {
    let tag = match (&f.class, f.level) {
        (Some(c), l) if l > 0 => format!("{c} {l}"),
        (Some(c), _) => c.clone(),
        (None, l) if l > 0 => format!("Level {l}"),
        _ => String::new(),
    };
    let tag = if tag.is_empty() {
        String::new()
    } else {
        format!("<span class=\"tag\" dir=\"auto\">{}</span>", esc(&tag))
    };
    let name = fld(&id::feat(n, "name"), "t", &f.name);
    let summarised = !full && f.text.chars().count() > LONG_TEXT;
    let (shown, tables) = if summarised {
        (f.summary.clone(), String::new())
    } else {
        prose_and_tables(n, f)
    };
    let text = fld(&id::feat(n, "text"), "txt", &shown);
    if o.style_formatting {
        format!("<div class=\"feat\">{name}{tag}<div>{text}</div>{tables}</div>")
    } else {
        format!("<div class=\"feat plain\">{name}. {text}{tables}</div>")
    }
}

/// The feature text before its first table, and the tables as HTML tables (header row, then
/// the body rows), each followed by the prose after it, as the PDF draws them (the later prose
/// in fields `feat.N.text.2`, `.3`...).
fn prose_and_tables(n: usize, f: &FeatureData) -> (String, String) {
    if f.tables.is_empty() {
        return (f.text.clone(), String::new());
    }
    let mut tables: Vec<_> = f.tables.iter().collect();
    tables.sort_by_key(|t| t.lines.start);
    let mut segments = crate::layout::prose_segments(&f.text, &tables).into_iter();
    let prose = segments.next().unwrap_or_default();
    let mut html = String::new();
    let mut part = 1;
    for (t, after) in tables.into_iter().zip(segments) {
        table_html(&mut html, t);
        if !after.trim().is_empty() {
            part += 1;
            let tid = format!("{}.{part}", id::feat(n, "text"));
            let _ = write!(html, "<div>{}</div>", fld(&tid, "txt", &after));
        }
    }
    (prose, html)
}

/// A feature table as an HTML table.
fn table_html(html: &mut String, t: &crate::model_types::TextTable) {
    html.push_str("<table class=\"ftab\"><tr>");
    for h in &t.header {
        let _ = write!(html, "<th scope=\"col\" dir=\"auto\">{}</th>", esc(h));
    }
    html.push_str("</tr>");
    for row in &t.rows {
        html.push_str("<tr>");
        for c in row {
            let _ = write!(html, "<td dir=\"auto\">{}</td>", esc(c));
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
}

fn feature_section(
    d: &SheetData,
    o: &HtmlOptions,
    full: bool,
    title: &str,
    groups: &[FeatureGroup],
) -> String {
    let items: Vec<String> = d
        .features
        .iter()
        .enumerate()
        .filter(|(_, f)| groups.contains(&f.group))
        .map(|(i, f)| feature_html(o, full, i + 1, f))
        .collect();
    if items.is_empty() {
        return String::new();
    }
    format!(
        "<div class=\"sec\">{}{}</div>",
        h2(title, ""),
        items.concat()
    )
}

/// Left column: class features, species/racial traits, feats, other features.
/// `full` prints complete feature text (2014 details page); otherwise long texts use the summary.
pub(super) fn features_column(d: &SheetData, o: &HtmlOptions, full: bool) -> String {
    use FeatureGroup::{Background, Class, Feat, Other, Species, Subclass};
    let is24 = matches!(
        d.edition_fields,
        crate::model_types::EditionFields::Y2024(_)
    );
    let species = if is24 {
        "Species traits"
    } else {
        "Racial traits"
    };
    let mut out = String::new();
    out.push_str(&feature_section(
        d,
        o,
        full,
        "Class features",
        &[Class, Subclass],
    ));
    out.push_str(&feature_section(d, o, full, species, &[Species]));
    out.push_str(&feature_section(d, o, full, "Feats", &[Feat]));
    out.push_str(&feature_section(
        d,
        o,
        full,
        "Background & other",
        &[Background, Other],
    ));
    let _ = write!(
        out,
        "<div class=\"sec grow\">{}{}</div>",
        h2("Other features", ""),
        area(
            "story.additional_features",
            &d.story.additional_features,
            false
        )
    );
    out
}

/// Equipment table with blank rows.
pub(super) fn equipment(d: &SheetData) -> String {
    let aside = format!("{} lb carried", pounds(d.gear.weight));
    let mut out = format!(
        "<div class=\"sec\">{}<table><tr><th>Item</th><th class=\"c\">Qty</th><th class=\"n\">lb</th><th>Notes</th></tr>",
        h2("Equipment", &aside)
    );
    for (i, it) in d.gear.items.iter().enumerate() {
        let n = i + 1;
        let mut notes: Vec<&str> = Vec::new();
        if !it.equipped.is_empty() {
            notes.push(&it.equipped);
        }
        if it.attuned {
            notes.push("attuned");
        } else if it.requires_attunement {
            notes.push("requires attunement");
        }
        let _ = write!(
            out,
            "<tr><td>{}</td><td class=\"c\">{}</td><td class=\"n\" style=\"font-weight:400\">{}</td><td class=\"note\">{}</td></tr>",
            fld(&id::equip(n, "name"), "", &it.name),
            fld(&id::equip(n, "qty"), "", &it.quantity.to_string()),
            fld(&id::equip(n, "weight"), "", &pounds(it.weight)),
            esc(&notes.join(", "))
        );
    }
    out.push_str(&"<tr class=\"empty\"><td></td><td></td><td></td><td></td></tr>".repeat(4));
    out.push_str("</table></div>");
    out
}

/// Attunement, coins, carrying, treasure and equipment notes.
pub(super) fn gear_extras(d: &SheetData) -> String {
    let g = &d.gear;
    let mut out = format!(
        "<div class=\"sec\">{}",
        h2(
            "Magic item attunement",
            &format!("{} / {}", g.attuned.len(), g.attunement_limit)
        )
    );
    for n in 1..=g.attunement_limit as usize {
        let name = g.attuned.get(n - 1).map_or("", String::as_str);
        let _ = write!(
            out,
            "<div class=\"trk\" style=\"margin-bottom:3pt\">{}<span class=\"f\" style=\"flex:1\" data-field=\"{}\" dir=\"auto\">{}</span></div>",
            sq(&id::attune(n), false),
            id::attune_name(n),
            esc(name)
        );
    }
    out.push_str("</div>");
    let _ = write!(
        out,
        "<div class=\"sec\">{}<div class=\"coins\">",
        h2("Coins", "")
    );
    for (label, (i, v)) in ["CP", "SP", "EP", "GP", "PP"]
        .iter()
        .zip(id::COINS.iter().zip(g.coins))
    {
        let _ = write!(
            out,
            "<div><div class=\"lbl\">{label}</div>{}</div>",
            fld(i, "f", &v.to_string())
        );
    }
    let _ = write!(
        out,
        "</div></div><div class=\"sec\">{}<div class=\"kv\"><div>Carried</div><div>{} lb</div><div>Capacity</div><div>{} lb</div></div></div>",
        h2("Carrying", ""),
        fld(id::CARRY_WEIGHT, "", &pounds(g.weight)),
        fld(id::CARRY_CAPACITY, "", &pounds(g.capacity)),
    );
    let _ = write!(
        out,
        "<div class=\"sec grow\">{}{}</div><div class=\"sec grow\">{}{}</div><div class=\"sec grow\">{}{}</div>",
        h2("Treasure", ""),
        area("story.treasure", &d.story.treasure, true),
        h2("Quest items", ""),
        area("story.quest", &d.story.quest, true),
        h2("Equipment notes", ""),
        area("story.equipment_notes", &d.story.equipment_notes, true),
    );
    out
}

/// 2024 page 2: features left, equipment and extras right.
pub(super) fn features_and_gear(d: &SheetData, o: &HtmlOptions) -> String {
    format!(
        "<div class=\"two\"><div class=\"col\">{}</div><div class=\"col\">{}{}</div></div>",
        features_column(d, o, false),
        equipment(d),
        gear_extras(d)
    )
}

/// 2014 inventory page: equipment left, extras right.
pub(super) fn inventory(d: &SheetData) -> String {
    format!(
        "<div class=\"two\"><div class=\"col\">{}</div><div class=\"col\">{}</div></div>",
        equipment(d),
        gear_extras(d)
    )
}
