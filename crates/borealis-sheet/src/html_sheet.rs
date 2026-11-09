//! New Borealis sheet HTML preview, rendered from the typed [`SheetData`] model.
//!
//! A complete self-contained HTML5 document: inline CSS, art as inline SVG symbols, no scripts and
//! no external fetches. Every value element carries `data-field="<model_ids id>"`. Pages are
//! paper-sized boxes (screen: sheets on a neutral desk; print: `@page` with page breaks).

use crate::art::{self, ArtId, ArtSet};
use crate::card_model::Card;
use crate::layout::{self, Band, Grid, LayoutOptions, SpellPagePlan};
use crate::model_types::{Edition, SheetData};
use crate::{PageSelection, Paper};

#[path = "html_sheet_band.rs"]
mod band;
#[path = "html_sheet_cards.rs"]
mod cards;
#[path = "html_sheet_css.rs"]
mod css;
#[path = "html_sheet_legacy.rs"]
mod legacy;
#[path = "html_sheet_overflow.rs"]
mod overflow;
#[path = "html_sheet_p1.rs"]
mod p1;
#[path = "html_sheet_p2.rs"]
mod p2;
#[path = "html_sheet_p3.rs"]
mod p3;
#[path = "html_sheet_p4.rs"]
mod p4;
#[path = "html_sheet_parts.rs"]
mod parts;
#[cfg(test)]
#[path = "html_sheet_split_tests.rs"]
mod split_tests;
#[cfg(test)]
#[path = "html_sheet_tests.rs"]
mod tests;

use parts::{art as art_use, esc};

/// A portrait image to embed (only PNG, JPEG and WebP data URIs are emitted).
#[derive(Debug, Clone)]
pub(crate) struct PortraitImage {
    /// MIME type.
    pub mime: String,
    /// Standard base64 of the image bytes.
    pub base64: String,
}

impl PortraitImage {
    /// Whether the MIME type is allow-listed and the payload is plain base64.
    pub(crate) fn is_valid(&self) -> bool {
        matches!(
            self.mime.as_str(),
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) && self
            .base64
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
    }
}

/// Options of the HTML preview (mapped from `SheetOptions` by the caller).
#[derive(Debug, Clone)]
pub(crate) struct HtmlOptions {
    /// Monochrome ink mode (greys only, Ink art).
    pub ink: bool,
    /// Paper size.
    pub paper: Paper,
    /// Smaller type.
    pub compact: bool,
    /// Print the modifier in the circle and the score large.
    pub swap_ability_positions: bool,
    /// List unprepared spells too.
    pub include_non_prepared_spells: bool,
    /// Print the basic fixed-capacity spellcasting page instead of the full spell table.
    pub legacy_spellcasting: bool,
    /// Bold-labelled feature lists (on) or running text (off).
    pub style_formatting: bool,
    /// Which pages to include.
    pub pages: PageSelection,
    /// Document language (BCP 47).
    pub lang: String,
    /// Portrait image, when the character has one.
    pub portrait: Option<PortraitImage>,
    /// Companion portrait image, when the companion has one.
    pub companion_portrait: Option<PortraitImage>,
    /// Card pages (at most nine cards each), shown after the sheet pages.
    pub cards: Vec<Vec<Card>>,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        Self {
            ink: false,
            paper: Paper::Letter,
            compact: false,
            swap_ability_positions: false,
            include_non_prepared_spells: true,
            legacy_spellcasting: false,
            style_formatting: true,
            pages: PageSelection::default(),
            lang: "en".into(),
            portrait: None,
            companion_portrait: None,
            cards: Vec::new(),
        }
    }
}

/// Lang tags are restricted to ASCII letters, digits and `-`.
fn safe_lang(lang: &str) -> String {
    let ok = !lang.is_empty()
        && lang.len() <= 35
        && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok {
        lang.to_string()
    } else {
        "en".into()
    }
}

fn symbol(id: &str, set: ArtSet, piece: ArtId, par: &str) -> String {
    let p = art::piece(set, piece);
    let svg = art::to_svg(p);
    let inner = svg
        .find('>')
        .map_or("", |i| &svg[i + 1..])
        .trim_end_matches("</svg>");
    format!(
        "<symbol id=\"a-{id}\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"{par}\">{inner}</symbol>",
        p.width, p.height
    )
}

fn defs(o: &HtmlOptions) -> String {
    let set = if o.ink { ArtSet::Ink } else { ArtSet::Colour };
    let frame = match o.paper {
        Paper::Letter => ArtId::FrameLetter,
        Paper::A4 => ArtId::FrameA4,
    };
    let mut out = String::from(
        "<svg width=\"0\" height=\"0\" style=\"position:absolute\" aria-hidden=\"true\"><defs>",
    );
    out.push_str(&symbol("frame", set, frame, "none"));
    out.push_str(&symbol("corner", set, ArtId::Corner, "xMidYMid meet"));
    out.push_str(&symbol(
        "header-rule",
        set,
        ArtId::HeaderRule,
        "xMinYMid meet",
    ));
    out.push_str(&symbol("divider", set, ArtId::Divider, "xMidYMid meet"));
    out.push_str(&symbol("ribbon", set, ArtId::BorealisRibbon, "none"));
    for (key, id) in [
        ("str", ArtId::SigilStr),
        ("dex", ArtId::SigilDex),
        ("con", ArtId::SigilCon),
        ("int", ArtId::SigilInt),
        ("wis", ArtId::SigilWis),
        ("cha", ArtId::SigilCha),
    ] {
        out.push_str(&symbol(&format!("sigil-{key}"), set, id, "xMidYMid meet"));
    }
    out.push_str("</defs></svg>");
    out
}

struct PageSpec {
    kind: &'static str,
    label: &'static str,
    subtitle: String,
    body: String,
    /// Page 1 only: identity line and meta carry the field ids.
    identity: Option<(String, Vec<(String, String)>)>,
    /// The page holds what one PDF page holds, so it keeps the paper height exactly (B-146).
    /// Pages the preview does not split yet grow with their content instead of cutting it.
    fixed: bool,
}

/// The PDF's page split that the preview follows (B-146): page 1's 2014 band and features
/// summary, and the rows of each spellcasting page.
pub(super) struct Split {
    /// The 2014 page-1 band (personality and equipment), `None` when page 1 keeps its Notes.
    pub band: Option<Band>,
    /// Feature names the 2014 page-1 summary lists.
    pub features_shown: usize,
    /// What each spellcasting page prints.
    pub spells: Vec<SpellPagePlan>,
    /// The PDF prints on one page what the preview prints on its one page of that kind:
    /// 2024 Features / 2014 Details, 2014 Inventory, Story, Companion.
    pub one_page: OnePage,
}

/// Which pages the preview prints whole but the PDF also holds on one page. Those keep the
/// paper height; one the PDF continues on further pages grows with its content instead (the
/// preview does not split it yet), so nothing is cut off.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct OnePage {
    pub main: bool,
    pub inventory: bool,
    pub story: bool,
    pub companion: bool,
}

/// Printed page numbers the notes point at.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Refs {
    /// The overflow page after page 1 (attacks and limited uses that did not fit).
    pub overflow: u32,
    /// The 2014 Details page (full feature text, personality continued).
    pub details: u32,
    /// The 2014 Inventory page, when printed.
    pub inventory: Option<u32>,
}

fn layout_options(o: &HtmlOptions) -> LayoutOptions {
    LayoutOptions {
        paper: o.paper,
        ink: o.ink,
        fillable: false,
        compact: o.compact,
        swap_ability_positions: o.swap_ability_positions,
        lang: o.lang.clone(),
    }
}

/// Measure the split with the PDF's own layout.
fn split(d: &SheetData, o: &HtmlOptions) -> Split {
    let lo = layout_options(o);
    let g = Grid::new(o.paper);
    let is14 = d.edition == Edition::Dnd2014;
    let spells = if o.pages.spellcasting && has_casting(d) && !o.legacy_spellcasting {
        layout::spell_plan(d, &lo, o.include_non_prepared_spells, &g)
    } else {
        Vec::new()
    };
    let band = if is14 && o.pages.core {
        layout::page_one_band(d, &lo, &g)
    } else {
        None
    };
    let features = layout::build_features(d, &lo, &g, 1, band.as_ref());
    let one_page = OnePage {
        main: features.main.len() <= 1,
        inventory: features.inventory.len() <= 1,
        story: layout::build_story(d, &lo, &g, 1).len() <= 1,
        companion: !layout::has_companion_page(d)
            || layout::build_companion(d, &lo, &g).pages.len() <= 1,
    };
    Split {
        band,
        features_shown: layout::summary_feature_count(d, &lo, &g),
        spells,
        one_page,
    }
}

fn plain_meta(d: &SheetData) -> Vec<(String, String)> {
    vec![
        ("Class".into(), esc(&d.identity.class_label)),
        ("Level".into(), d.identity.level.to_string()),
    ]
}

fn header(d: &SheetData, spec: &PageSpec) -> String {
    let (ident, meta, name) = match &spec.identity {
        Some((i, m)) => (
            i.clone(),
            m.clone(),
            parts::fld(crate::model_ids::CHAR_NAME, "", &d.identity.name),
        ),
        None => (
            esc(&spec.subtitle),
            plain_meta(d),
            format!("<span dir=\"auto\">{}</span>", esc(&d.identity.name)),
        ),
    };
    let meta: String = meta
        .iter()
        .map(|(l, v)| {
            format!("<div><span class=\"lbl\">{l}</span><span class=\"val\">{v}</span></div>")
        })
        .collect();
    format!(
        "<div class=\"head\">{}<div><div class=\"name\">{name}</div><div class=\"ident\">{ident}</div></div><div class=\"meta\">{meta}</div></div>{}",
        art_use("ribbon", "sky"),
        art_use("divider", "divider")
    )
}

fn page_html(d: &SheetData, o: &HtmlOptions, spec: &PageSpec, n: usize, total: usize) -> String {
    let paper = match o.paper {
        Paper::Letter => "letter",
        Paper::A4 => "a4",
    };
    let edition = match d.edition {
        Edition::Dnd2014 => "2014",
        Edition::Dnd2024 => "2024",
    };
    let a4 = if matches!(o.paper, Paper::A4) {
        " \u{b7} A4"
    } else {
        ""
    };
    let srd = if n == total {
        let version = if d.edition == Edition::Dnd2024 {
            "5.2.1"
        } else {
            "5.1"
        };
        format!(
            "<div class=\"attr\">Includes material from the System Reference Document {version} by Wizards of the Coast LLC, licensed under CC BY 4.0. Borealis is not affiliated with Wizards of the Coast.</div>"
        )
    } else {
        String::new()
    };
    let corners: String = ["tl", "tr", "bl", "br"]
        .iter()
        .map(|c| art_use("corner", &format!("corner c-{c}")))
        .collect();
    let note = d.footer_note();
    let flow = if spec.fixed { "" } else { " flow" };
    format!(
        "<section class=\"page {paper}{flow}\" data-page=\"{}\" aria-label=\"Page {n}: {}\">{}{corners}<div class=\"content\">{}{}{srd}</div>\
         <div class=\"foot\"><span>Borealis character sheet \u{b7} {edition} rules{a4}{note}</span><span>{n} / {total}</span></div></section>",
        spec.kind,
        spec.label,
        art_use("frame", "frame"),
        header(d, spec),
        spec.body
    )
}

fn has_casting(d: &SheetData) -> bool {
    !d.spellcasting.casters.is_empty()
}

/// Page numbers of the pages page 1 points at, in the order [`page_specs`] adds the pages.
fn refs(d: &SheetData, o: &HtmlOptions, sp: &Split) -> Refs {
    let sel = &o.pages;
    let is24 = d.edition == Edition::Dnd2024;
    let lead = u32::from(sel.core) * (1 + u32::from(layout::overflow_needed(d)));
    let details = lead + 1;
    let main = u32::from(if is24 {
        sel.core || sel.inventory
    } else {
        sel.core || sel.story
    });
    let spells = if !(sel.spellcasting && has_casting(d)) {
        0
    } else if o.legacy_spellcasting {
        d.spellcasting.casters.len()
    } else {
        sp.spells.len()
    };
    let inventory = (!is24 && sel.inventory).then(|| lead + main + spells as u32 + 1);
    Refs {
        overflow: 2,
        details,
        inventory,
    }
}

fn page_specs(d: &SheetData, o: &HtmlOptions) -> Vec<PageSpec> {
    let sel = &o.pages;
    let is24 = d.edition == Edition::Dnd2024;
    let sp = split(d, o);
    let refs = refs(d, o, &sp);
    let mut v = Vec::new();
    let spec = |kind, label, subtitle: &str, body, fixed| PageSpec {
        kind,
        label,
        subtitle: subtitle.into(),
        body,
        identity: None,
        fixed,
    };
    if sel.core {
        let mut p = spec(
            "character",
            "Character",
            "",
            p1::body(d, o, &sp, refs),
            true,
        );
        p.identity = Some(p1::identity(d));
        v.push(p);
        if layout::overflow_needed(d) {
            v.push(spec(
                "overflow",
                "Attacks and limited uses",
                "Attacks & limited uses (continued)",
                overflow::body(d),
                true,
            ));
        }
    }
    if is24 {
        if sel.core || sel.inventory {
            v.push(spec(
                "features",
                "Features and equipment",
                "Features, traits and equipment",
                p2::features_and_gear(d, o),
                sp.one_page.main,
            ));
        }
    } else if sel.core || sel.story {
        v.push(spec(
            "details",
            "Details",
            "Details and features",
            p4::details_2014(d, o, sp.band.as_ref()),
            sp.one_page.main,
        ));
    }
    if sel.spellcasting && has_casting(d) && o.legacy_spellcasting {
        for (i, c) in d.spellcasting.casters.iter().enumerate() {
            v.push(spec(
                "spellcasting-basic",
                "Spellcasting (basic)",
                "Spellcasting (basic)",
                legacy::body(o, i + 1, c),
                false,
            ));
        }
    } else if sel.spellcasting && has_casting(d) {
        let first = v.len() as u32 + 1;
        for (i, body) in p3::pages(d, &sp.spells, first).into_iter().enumerate() {
            let subtitle = if i == 0 {
                "Spellcasting"
            } else {
                "Spellcasting (continued)"
            };
            v.push(spec("spellcasting", "Spellcasting", subtitle, body, true));
        }
    }
    if !is24 && sel.inventory {
        v.push(spec(
            "inventory",
            "Inventory",
            "Inventory",
            p2::inventory(d),
            sp.one_page.inventory,
        ));
    }
    if sel.story && crate::layout::has_story_page(d) {
        v.push(spec(
            "story",
            "Story",
            "Story",
            p4::page(d, o),
            sp.one_page.story,
        ));
    }
    if let (true, Some(body)) = (sel.story, p4::companion_page(d, o)) {
        let title = d
            .extras
            .companion
            .as_ref()
            .map_or_else(|| "Companion".to_string(), |c| c.title());
        v.push(spec(
            "companion",
            "Companion",
            &title,
            body,
            sp.one_page.companion,
        ));
    }
    for (i, page) in o.cards.iter().enumerate() {
        v.push(spec("cards", "Cards", "Cards", cards::body(page, i), true));
    }
    v
}

/// Render the complete HTML document for `data`.
pub(crate) fn render_sheet(data: &SheetData, opts: &HtmlOptions) -> String {
    let (size, _) = match opts.paper {
        Paper::Letter => ("letter", ()),
        Paper::A4 => ("A4", ()),
    };
    let specs = page_specs(data, opts);
    let total = specs.len();
    let pages: String = specs
        .iter()
        .enumerate()
        .map(|(i, s)| page_html(data, opts, s, i + 1, total))
        .collect();
    format!(
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title>\
         <style>@page{{size:{size};margin:0}}{}{}</style></head>\
         <body class=\"sheet{}\">{}<div class=\"desk\">{pages}</div></body></html>",
        safe_lang(&opts.lang),
        esc(&data.identity.name),
        css::tokens(opts.ink),
        css::CSS,
        if opts.compact { " compact" } else { "" },
        defs(opts)
    )
}
