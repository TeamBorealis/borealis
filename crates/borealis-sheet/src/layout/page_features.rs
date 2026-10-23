//! Features and gear: 2024 page 2 (class features, species traits, feats, equipment, attunement,
//! coins, carrying, treasure) and the 2014 Details and Inventory pages, each with its own
//! continuation pages and the "Feature details" pages that carry the full text of features that
//! were only summarised.
//!
//! Page order contract: [`FeaturePages::main`] is 2024 page 2 (+ continuation + feature details
//! pages) or the 2014 Details page (+ continuation); [`FeaturePages::inventory`] is empty for
//! 2024 and holds the 2014 Inventory page(s). For 2014 the caller inserts the spell pages
//! between `main` and `inventory`.

#[path = "features_flow.rs"]
pub(crate) mod features_flow;
#[path = "features_gear.rs"]
pub(crate) mod features_gear;
#[path = "features_pages.rs"]
pub(crate) mod features_pages;
#[path = "features_tables.rs"]
pub(crate) mod features_tables;
#[cfg(test)]
#[path = "features_tests.rs"]
mod features_tests;
#[path = "features_text.rs"]
pub(crate) mod features_text;

use super::grid::{Align, Grid, Txt};
use super::page_story::story_regions;
use super::{Band, LayoutOptions};
use crate::fonts::Style;
use crate::model_ids as id;
use crate::model_types::{Edition, EditionFields, FeatureGroup, SheetData};
use crate::pdf::PdfPage;
use features_flow::{
    feature_piece, fill_last, height, ContMark, Cx, Feat, Piece, HEAD_H, PORTRAIT_H, PORTRAIT_W,
};
use features_pages::{columns, draw_marks, new_canvas, Env, Seq};
use std::collections::{HashSet, VecDeque};

/// Pages of the features/gear family.
#[derive(Debug, Default)]
pub(crate) struct FeaturePages {
    /// 2024 page 2 and its continuation and details pages; 2014 Details page and continuation.
    pub main: Vec<PdfPage>,
    /// 2014 Inventory page(s); empty for 2024.
    pub inventory: Vec<PdfPage>,
    /// "continued on page N" notes of the inventory pages (their page numbers are only known
    /// once the spell pages are counted; see [`FeaturePages::numbered_inventory`]).
    pub inventory_marks: Vec<ContMark>,
}

impl FeaturePages {
    /// The inventory pages with their notes, the first printed as page `first`.
    pub fn numbered_inventory(mut self, o: &LayoutOptions, first: u32) -> Vec<PdfPage> {
        draw_marks(&mut self.inventory, &self.inventory_marks, o, |i| {
            first + i as u32
        });
        self.inventory
    }

    /// All pages in order (main, then inventory).
    #[cfg(test)]
    pub fn all(mut self) -> Vec<PdfPage> {
        self.main.append(&mut self.inventory);
        self.main
    }
}

/// Heading of the full-text blocks of summarised features.
const DETAILS_TITLE: &str = "Feature details";
/// Height of the deity row at the top of the 2024 start column.
const DEITY_H: f32 = 25.0;
/// Slack before a column counts as overfull.
const FIT_SLACK: f32 = 4.0;
/// Minimum blank lines of the "Other features" write-in area.
const OTHER_MIN_LINES: usize = 5;

/// Where a portrait goes on the 2014 Details page.
pub(crate) fn details_portrait_rect(g: &Grid, compact: bool) -> super::grid::Rect {
    let k = if compact { super::grid::COMPACT_K } else { 1.0 };
    let (x, _) = columns(g)[1];
    (x, g.body_top + HEAD_H * k, PORTRAIT_W, PORTRAIT_H)
}

/// The class a class or subclass feature is listed under.
fn class_key(d: &SheetData, i: usize) -> Option<String> {
    let f = &d.features[i];
    let classes = &d.identity.classes;
    let by_name = |n: &str| {
        classes
            .iter()
            .find(|c| c.class == n)
            .map(|c| c.class.clone())
    };
    f.class
        .as_deref()
        .and_then(|n| {
            by_name(n).or_else(|| {
                classes
                    .iter()
                    .find(|c| c.subclass.as_deref() == Some(n))
                    .map(|c| c.class.clone())
            })
        })
        .or_else(|| classes.first().map(|c| c.class.clone()))
}

fn species_hint(d: &SheetData) -> Option<String> {
    match &d.edition_fields {
        EditionFields::Y2014(f) => Some(f.race.clone()),
        EditionFields::Y2024(f) => Some(f.species.clone()),
    }
    .filter(|s| !s.is_empty())
}

/// Feature `i` as pieces: its block, then the tables of its full text that fit width `w` (their
/// rows leave the text). `group` is the heading a continuation repeats.
fn feature_pieces(
    d: &SheetData,
    i: usize,
    summary: bool,
    (k, w): (f32, f32),
    group: &str,
) -> Vec<Piece> {
    let mut f = feature_piece(d, i, summary);
    f.group = group.to_string();
    if summary && f.summarised {
        return vec![Piece::Feature(f)];
    }
    let (text, tables) = features_tables::split_off_tables(&d.features[i], k, w);
    let after_table = |text: String| Feat {
        text,
        after_table: true,
        ..f.clone()
    };
    let mut out = Vec::with_capacity(1 + 2 * tables.len());
    for (mut t, after) in tables {
        t.group = group.to_string();
        out.push(Piece::Table(t));
        if !after.trim().is_empty() {
            out.push(Piece::Feature(after_table(after)));
        }
    }
    out.insert(0, Piece::Feature(Feat { text, ..f }));
    out
}

/// The feature queue in reading order: class groups, species, feats, background and other, then
/// the "other features" write-in area. `summary` lists the features shown as a summary; `kw` is
/// the scale and column width (for the tables of feature text).
fn feature_queue(d: &SheetData, summary: &HashSet<usize>, kw: (f32, f32)) -> VecDeque<Piece> {
    let mut q = VecDeque::new();
    let block = |i: usize, group: &str| feature_pieces(d, i, summary.contains(&i), kw, group);
    let of = |g: &[FeatureGroup]| -> Vec<usize> {
        (0..d.features.len())
            .filter(|&i| g.contains(&d.features[i].group))
            .collect()
    };
    let class_idx = of(&[FeatureGroup::Class, FeatureGroup::Subclass]);
    let classes = &d.identity.classes;
    let many = classes.len() > 1;
    if classes.is_empty() {
        if !class_idx.is_empty() {
            q.push_back(Piece::Heading {
                title: "Class features".into(),
                hint: None,
            });
            q.extend(class_idx.iter().flat_map(|&i| block(i, "Class features")));
        }
    } else {
        for c in classes {
            let own: Vec<usize> = class_idx
                .iter()
                .copied()
                .filter(|&i| class_key(d, i).as_deref() == Some(c.class.as_str()))
                .collect();
            if own.is_empty() {
                continue;
            }
            let title = if many {
                format!("{} features", c.class)
            } else {
                "Class features".to_string()
            };
            q.push_back(Piece::Heading {
                title: title.clone(),
                hint: Some(format!("{} {}", c.class, c.level)),
            });
            q.extend(own.into_iter().flat_map(|i| block(i, &title)));
        }
    }
    let groups: [(&str, &[FeatureGroup], Option<String>); 3] = [
        (
            if d.edition == Edition::Dnd2014 {
                "Racial traits"
            } else {
                "Species traits"
            },
            &[FeatureGroup::Species],
            species_hint(d),
        ),
        ("Feats", &[FeatureGroup::Feat], None),
        (
            "Background & other",
            &[FeatureGroup::Background, FeatureGroup::Other],
            None,
        ),
    ];
    for (title, g, hint) in groups {
        let idx = of(g);
        if idx.is_empty() {
            continue;
        }
        q.push_back(Piece::Heading {
            title: title.into(),
            hint,
        });
        q.extend(idx.into_iter().flat_map(|i| block(i, title)));
    }
    let title = if d.edition == Edition::Dnd2014 {
        "Additional features"
    } else {
        "Other features"
    };
    q.push_back(story_regions::region(
        d,
        "story.additional_features",
        title,
        OTHER_MIN_LINES,
        true,
    ));
    q
}

fn queue_height(q: &VecDeque<Piece>, k: f32, w: f32) -> f32 {
    q.iter().map(|p| height(p, k, w)).sum()
}

/// Features to show as a summary so the start column fits on one page: longest savings first.
fn choose_summaries(d: &SheetData, k: f32, w: f32, col_h: f32) -> HashSet<usize> {
    let mut chosen = HashSet::new();
    loop {
        if queue_height(&feature_queue(d, &chosen, (k, w)), k, w) <= col_h - FIT_SLACK {
            return chosen;
        }
        let best = (0..d.features.len())
            .filter(|i| !chosen.contains(i) && d.features[*i].summary != d.features[*i].text)
            .filter(|i| !d.features[*i].summary.is_empty())
            .map(|i| {
                let full = height(&Piece::Feature(feature_piece(d, i, false)), k, w);
                let short = height(&Piece::Feature(feature_piece(d, i, true)), k, w);
                (i, full - short)
            })
            .filter(|(_, saving)| *saving > 0.5)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, _)) => {
                chosen.insert(i);
            }
            None => return chosen,
        }
    }
}

fn detail_queue(d: &SheetData, summarised: &HashSet<usize>, kw: (f32, f32)) -> VecDeque<Piece> {
    let mut idx: Vec<usize> = summarised.iter().copied().collect();
    idx.sort_unstable();
    let mut q = VecDeque::new();
    if idx.is_empty() {
        return q;
    }
    q.push_back(Piece::Heading {
        title: DETAILS_TITLE.into(),
        hint: None,
    });
    q.extend(idx.into_iter().flat_map(|i| {
        let mut pieces = feature_pieces(d, i, false, kw, DETAILS_TITLE);
        if let Some(Piece::Feature(f)) = pieces.first_mut() {
            f.detail = true;
            f.pips = 0;
        }
        pieces
    }));
    q
}

/// Build the features and gear pages. `first_page_no` is the printed number of the first page
/// returned (used for the "full text on page N" notes).
/// `band` is page 1's 2014 band (the personality text that goes on on the Details page).
pub(crate) fn build(
    data: &SheetData,
    opts: &LayoutOptions,
    grid: &Grid,
    first_page_no: u32,
    band: Option<&Band>,
) -> FeaturePages {
    build_with_limits(data, opts, grid, first_page_no, band).0
}

/// [`build`] plus the width limit of every text op, per page (main pages, then inventory).
pub(crate) fn build_with_limits(
    data: &SheetData,
    opts: &LayoutOptions,
    grid: &Grid,
    first_page_no: u32,
    band: Option<&Band>,
) -> (FeaturePages, Vec<Vec<(usize, f32)>>) {
    let env = Env {
        d: data,
        o: opts,
        g: grid,
    };
    match data.edition {
        Edition::Dnd2024 => build_2024(&env, first_page_no),
        Edition::Dnd2014 => build_2014(&env, first_page_no, band),
    }
}

fn build_2024(env: &Env, first_page_no: u32) -> (FeaturePages, Vec<Vec<(usize, f32)>>) {
    let (d, g) = (env.d, env.g);
    let k = if env.o.compact {
        super::grid::COMPACT_K
    } else {
        1.0
    };
    let [a, b] = columns(g);
    let mut seq = Seq::default();
    let (mut c, top) = new_canvas(env, None);
    let y0 = top + DEITY_H * k;
    let summarised = choose_summaries(d, k, a.1, g.body_bottom - y0);
    let mut left = feature_queue(d, &summarised, (k, a.1));
    let right;
    {
        let mut cx = Cx {
            c: &mut c,
            ids: &mut seq.ids,
            d,
            ordinal: 1,
            page_idx: 0,
            refs: &mut seq.refs,
            landed: &mut seq.landed,
        };
        deity_row(&mut cx, (a.0, top, a.1));
        let marks = &mut seq.marks;
        fill_last(&mut cx, &mut left, (a.0, y0, a.1), g.body_bottom, marks);
        right = features_gear::column_2024(&mut cx, (b.0, top, b.1), g.body_bottom, marks);
    }
    seq.push(c);
    // The full-text blocks join the shorter column's queue, so they use the room left on the
    // continuation pages instead of starting a page of their own.
    let mut right = right;
    let shorter = if queue_height(&left, k, a.1) <= queue_height(&right, k, b.1) {
        &mut left
    } else {
        &mut right
    };
    shorter.extend(detail_queue(d, &summarised, (k, a.1)));
    let cont = "Features & gear (continued)";
    seq.flow_pages(env, 1, (cont, cont), left, right);
    seq.resolve_refs(env, |i| first_page_no + i as u32);
    seq.resolve_marks(env.o, |i| first_page_no + i as u32);
    (
        FeaturePages {
            main: seq.pages,
            inventory: Vec::new(),
            inventory_marks: Vec::new(),
        },
        seq.limits,
    )
}

fn deity_row(cx: &mut Cx, (x, y, w): (f32, f32, f32)) {
    let k = cx.c.k;
    cx.c.label(x + 1.0, y + 5.0, Align::Start, "DEITY");
    let t = Txt::new(8.0 * k, Style::Sans, cx.c.pal.ink);
    let fid = cx.ids.make(id::CHAR_DEITY, cx.ordinal);
    let value = cx.d.identity.deity.clone();
    cx.c.tinted_field(
        &fid,
        "Deity",
        (x, y + 7.0, w.min(170.0), 13.0 * k),
        y + 7.0 + 10.0 * k,
        &value,
        &t,
    );
}

fn build_2014(
    env: &Env,
    first_page_no: u32,
    band: Option<&Band>,
) -> (FeaturePages, Vec<Vec<(usize, f32)>>) {
    let (d, g) = (env.d, env.g);
    let [a, b] = columns(g);
    // Details page.
    let mut details = Seq::default();
    let (mut c, top) = new_canvas(env, None);
    let k = if env.o.compact {
        super::grid::COMPACT_K
    } else {
        1.0
    };
    let mut left = feature_queue(d, &HashSet::new(), (k, a.1));
    let mut right = story_regions::details_2014(d, band);
    {
        let mut cx = Cx {
            c: &mut c,
            ids: &mut details.ids,
            d,
            ordinal: 1,
            page_idx: 0,
            refs: &mut details.refs,
            landed: &mut details.landed,
        };
        let marks = &mut details.marks;
        fill_last(&mut cx, &mut left, (a.0, top, a.1), g.body_bottom, marks);
        fill_last(&mut cx, &mut right, (b.0, top, b.1), g.body_bottom, marks);
    }
    details.push(c);
    let cont = "Details (continued)";
    details.flow_pages(env, 1, (cont, cont), left, right);
    details.resolve_marks(env.o, |i| first_page_no + i as u32);
    // Inventory page.
    let mut inv = Seq::default();
    let (mut c, top) = new_canvas(env, None);
    let head = format!("{} lb carried", features_flow::trim_num(d.gear.weight));
    let (mut eq, mut side);
    {
        let mut cx = Cx {
            c: &mut c,
            ids: &mut inv.ids,
            d,
            ordinal: 1,
            page_idx: 0,
            refs: &mut inv.refs,
            landed: &mut inv.landed,
        };
        eq = features_gear::equipment_queue(&cx, 0, "Equipment", Some(head));
        let marks = &mut inv.marks;
        // Rows that do not fit go on in the right column (no "continued on page" note here).
        let y = features_flow::fill(&mut cx, &mut eq, (a.0, top, a.1), g.body_bottom);
        if eq.is_empty() {
            features_flow::blank_rows(cx.c, (a.0, y, a.1), g.body_bottom);
        }
        let rows = std::mem::take(&mut eq);
        side = features_gear::side_2014(&mut cx, (b.0, top, b.1), g.body_bottom, marks, rows);
    }
    inv.push(c);
    features_gear::continue_equipment(&mut side);
    let cont = "Inventory (continued)";
    inv.flow_pages(env, 1, (cont, cont), eq, side);
    let mut limits = details.limits;
    limits.extend(inv.limits);
    (
        FeaturePages {
            main: details.pages,
            inventory: inv.pages,
            inventory_marks: inv.marks,
        },
        limits,
    )
}
