//! Story page (2024 page 4) and the story column of the 2014 details page.

use super::parts::{area, card_top, esc, fld, h2};
use super::HtmlOptions;
use crate::companion::signed;
use crate::layout::{Band, COMPANION_ABILITIES};
use crate::model_ids as id;
use crate::model_types::SheetData;
use std::fmt::Write;

fn portrait(d: &SheetData, o: &HtmlOptions) -> String {
    match &o.portrait {
        Some(p) if p.is_valid() => format!(
            "<img class=\"portrait\" alt=\"Character portrait\" src=\"data:{};base64,{}\">",
            esc(&p.mime),
            esc(&p.base64)
        ),
        _ if d.story.has_portrait => "<div class=\"portrait empty\">Portrait</div>".into(),
        _ => String::new(),
    }
}

fn appearance(d: &SheetData, o: &HtmlOptions) -> String {
    let s = &d.story;
    let mut out = format!(
        "<div class=\"sec\">{}<div class=\"story\">",
        h2("Appearance", "")
    );
    for (label, i, v) in [
        ("Gender", "story.gender", &s.gender),
        ("Age", "story.age", &s.age),
        ("Height", "story.height", &s.height),
        ("Weight", "story.weight", &s.weight),
        ("Eyes", "story.eyes", &s.eyes),
        ("Skin", "story.skin", &s.skin),
        ("Hair", "story.hair", &s.hair),
    ] {
        let _ = write!(
            out,
            "<div><div class=\"lbl\">{label}</div>{}</div>",
            fld(i, "f", v)
        );
    }
    let _ = write!(out, "</div>{}</div>", portrait(d, o));
    out
}

fn text_section(title: &str, i: &str, text: &str, grow: bool) -> String {
    format!(
        "<div class=\"sec{}\">{}{}</div>",
        if grow { " grow" } else { "" },
        h2(title, ""),
        area(i, text, !grow)
    )
}

/// The traits, ideals, bonds and flaws in full.
fn personality(d: &SheetData) -> String {
    let s = &d.story;
    format!(
        "{}{}{}{}",
        text_section("Personality traits", "story.traits", &s.traits, false),
        text_section("Ideals", "story.ideals", &s.ideals, false),
        text_section("Bonds", "story.bonds", &s.bonds, false),
        text_section("Flaws", "story.flaws", &s.flaws, false),
    )
}

/// The personality text page 1's band had no lines for (`story.<key>@2`, as the PDF's Details
/// page), under "<block> (continued)".
fn personality_rest(band: &Band) -> String {
    band.traits
        .iter()
        .filter_map(|t| {
            let rest = t.tail.as_deref()?;
            Some(text_section(
                &format!("{} (continued)", t.title),
                &format!("{}@2", t.id),
                rest,
                false,
            ))
        })
        .collect()
}

/// Story sections in two columns (left, right), with `traits` (the personality blocks or what
/// is left of them) under the appearance.
pub(super) fn columns(d: &SheetData, o: &HtmlOptions, traits: &str) -> (String, String) {
    let s = &d.story;
    let left = format!(
        "{}{traits}{}",
        appearance(d, o),
        text_section("Trinket", "story.trinket", &s.trinket, false),
    );
    let companion = format!(
        "<div class=\"sec\">{}<div class=\"txtbox\">{}</div></div>",
        h2("Companion", ""),
        fld(id::COMPANION, "", &d.extras.companion_name)
    );
    let right = format!(
        "{}{}{}{}{companion}",
        text_section("Backstory", id::STORY[12], &s.backstory, true),
        text_section("Organisation", "story.organization", &s.organization, false),
        text_section("Allies", "story.allies", &s.allies, false),
        text_section("Notes", "story.notes", &s.notes, true),
    );
    (left, right)
}

/// Whole 2024 story page body.
pub(super) fn page(d: &SheetData, o: &HtmlOptions) -> String {
    let (l, r) = columns(d, o, &personality(d));
    format!("<div class=\"two\"><div class=\"col\">{l}</div><div class=\"col\">{r}</div></div>")
}

/// 2014 details page: features left, story right. With page 1's `band` the personality blocks
/// are on page 1 and only their rest is here (the story column is left out when the story is
/// empty); without it (no room on page 1, or no page 1) they are here in full, as the PDF
/// prints them.
pub(super) fn details_2014(d: &SheetData, o: &HtmlOptions, band: Option<&Band>) -> String {
    let features = super::p2::features_column(d, o, true);
    if band.is_some() && !d.story.has_content() {
        return format!("<div class=\"col\">{features}</div>");
    }
    let traits = band.map_or_else(|| personality(d), personality_rest);
    let (l, r) = columns(d, o, &traits);
    format!(
        "<div class=\"two\"><div class=\"col\">{features}</div><div class=\"col\">{l}{r}</div></div>"
    )
}

/// The companion portrait box (empty frame when the picture is missing or not allow-listed).
fn companion_portrait(o: &HtmlOptions) -> String {
    match &o.companion_portrait {
        Some(p) if p.is_valid() => format!(
            "<img class=\"portrait companion-portrait\" alt=\"Companion portrait\" src=\"data:{};base64,{}\">",
            esc(&p.mime),
            esc(&p.base64)
        ),
        _ => "<div class=\"portrait companion-portrait empty\"></div>".into(),
    }
}

/// Companion page: the stat block regions in two columns (statistics, saving throws and skills left, the
/// creature's traits, actions and reactions right), the portrait box on top of the left column.
/// `None` without a stat block, even when a portrait is set.
pub(super) fn companion_page(d: &SheetData, o: &HtmlOptions) -> Option<String> {
    let c = d.extras.companion.as_ref()?;
    let portrait = if d.extras.has_companion_portrait {
        companion_portrait(o)
    } else {
        String::new()
    };
    let (mut left, mut right) = (portrait, String::new());
    for (i, title, text) in c.regions_for(false) {
        let col = if crate::CompanionSheet::in_left_column(i) {
            &mut left
        } else {
            &mut right
        };
        col.push_str(&text_section(title, i, &text, false));
    }
    Some(format!(
        "<div class=\"companion-scores\">{}</div><div class=\"two\"><div class=\"col\">{left}</div><div class=\"col\">{right}</div></div>",
        score_cards(&c.abilities, o)
    ))
}

/// The six scores as the main sheet's ability cards (sigil, name, modifier, score), two per row.
fn score_cards(scores: &[i32; 6], o: &HtmlOptions) -> String {
    let mut out = String::new();
    for (&(key, name), &score) in COMPANION_ABILITIES.iter().zip(scores) {
        out.push_str(&card_top(
            o,
            (key, name),
            (
                &format!("companion.{key}.mod"),
                &signed((score - 10).div_euclid(2)),
            ),
            (&format!("companion.{key}.score"), &score.to_string()),
        ));
        // `card_top` opens the card and its top row.
        out.push_str("</div></div>");
    }
    out
}
