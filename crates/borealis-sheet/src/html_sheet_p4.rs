//! Story page (2024 page 4) and the story column of the 2014 details page.

use super::parts::{area, card_top, esc, fld, h2};
use super::HtmlOptions;
use crate::companion::signed;
use crate::layout::COMPANION_ABILITIES;
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

/// Story sections in two columns (left, right). `personality` = with the traits, ideals, bonds
/// and flaws (the 2014 sheet prints them on page 1).
pub(super) fn columns(d: &SheetData, o: &HtmlOptions, personality: bool) -> (String, String) {
    let s = &d.story;
    let traits = if personality {
        format!(
            "{}{}{}{}",
            text_section("Personality traits", "story.traits", &s.traits, false),
            text_section("Ideals", "story.ideals", &s.ideals, false),
            text_section("Bonds", "story.bonds", &s.bonds, false),
            text_section("Flaws", "story.flaws", &s.flaws, false),
        )
    } else {
        String::new()
    };
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
    let (l, r) = columns(d, o, true);
    format!("<div class=\"two\"><div class=\"col\">{l}</div><div class=\"col\">{r}</div></div>")
}

/// 2014 details page: features left, story right (when it has content).
pub(super) fn details_2014(d: &SheetData, o: &HtmlOptions) -> String {
    let features = super::p2::features_column(d, o, true);
    if !d.story.has_content() {
        return format!("<div class=\"col\">{features}</div>");
    }
    let (l, r) = columns(d, o, false);
    format!(
        "<div class=\"two\"><div class=\"col\">{features}</div><div class=\"col\">{l}{r}</div></div>"
    )
}

/// Companion page: the stat block regions in two columns (statistics and skills left, the
/// creature's traits, actions and reactions right). `None` without a stat block.
pub(super) fn companion_page(d: &SheetData, o: &HtmlOptions) -> Option<String> {
    let c = d.extras.companion.as_ref()?;
    let (mut left, mut right) = (String::new(), String::new());
    for (i, title, text) in c.regions_for(false) {
        let col = if matches!(i, "companion.stats" | "companion.skills") {
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
        out.push_str("</div>");
    }
    out
}
