//! Companion page (S6): the companion creature's stat block, poured into the same two ruled
//! columns as the Story page. Printed when the app supplied a stat block
//! (`SheetData::extras.companion`) or a portrait (`has_companion_portrait`); a portrait alone
//! prints the page with just the portrait box.

use crate::layout::abilities::{header_with_ids, CardIds, HEADER_CARD_H};
use crate::layout::grid::{Canvas, Grid, Rect};
use crate::layout::page_features::features_flow::{
    fill, Cx, Piece, Region, PORTRAIT_H, PORTRAIT_W,
};
use crate::layout::page_features::features_pages::{columns, new_canvas, Env, Seq};
use crate::layout::LayoutOptions;
use crate::model_types::{AbilityData, SheetData};
use crate::pdf::PdfPage;
use std::collections::VecDeque;

/// Smallest ruled area of a section, in lines.
const MIN_LINES: usize = 2;

/// The statistics and skills go in the left column, the creature's traits, actions and
/// reactions in the right one.
fn queues(data: &SheetData) -> (VecDeque<Piece>, VecDeque<Piece>) {
    let mut left = VecDeque::new();
    let mut right = VecDeque::new();
    let Some(companion) = &data.extras.companion else {
        return (left, right);
    };
    for (id, title, text) in companion.regions_for(false) {
        let piece = Piece::Region(Region {
            id: id.to_string(),
            tip: title.to_string(),
            title: title.to_string(),
            text,
            min_lines: MIN_LINES,
            fill: false,
        });
        if matches!(id, "companion.stats" | "companion.skills") {
            left.push_back(piece);
        } else {
            right.push_back(piece);
        }
    }
    (left, right)
}

/// Gap between the score boxes, in points.
const BOX_GAP: f32 = 5.0;
/// Space between the score boxes and the text columns below them.
const BOXES_BELOW: f32 = 6.0;
/// Key and name of the six abilities in card order.
pub(crate) const ABILITIES: [(&str, &str); 6] = [
    ("str", "Strength"),
    ("dex", "Dexterity"),
    ("con", "Constitution"),
    ("int", "Intelligence"),
    ("wis", "Wisdom"),
    ("cha", "Charisma"),
];

/// The six scores as the main sheet's ability cards (two columns of three), from `top`.
/// Returns the y below them.
fn score_boxes(c: &mut Canvas, env: &Env, scores: [i32; 6], top: f32) -> f32 {
    let cols = columns(env.g);
    let mut y = top;
    for row in 0..3 {
        for (col, &(x, w)) in cols.iter().enumerate() {
            let i = row * 2 + col;
            let (key, name) = ABILITIES[i];
            let score = scores[i];
            let a = AbilityData {
                key,
                name: name.to_string(),
                score,
                modifier: (score - 10).div_euclid(2),
                save: 0,
                save_proficient: false,
            };
            c.card((x, y, w, HEADER_CARD_H), 5.0, Some(c.pal.paper));
            let ids = CardIds {
                score: format!("companion.{key}.score"),
                modifier: format!("companion.{key}.mod"),
            };
            header_with_ids(
                c,
                (x, y, w),
                &a,
                (env.o.swap_ability_positions, env.o.ink),
                &ids,
            );
        }
        y += HEADER_CARD_H + BOX_GAP;
    }
    y - BOX_GAP + BOXES_BELOW
}

/// The Companion pages and where the portrait picture goes on the first one.
pub(crate) struct Built {
    /// The pages (empty without a stat block or portrait).
    pub pages: Vec<PdfPage>,
    /// The portrait box on the first page, when a portrait was supplied.
    pub portrait: Option<Rect>,
}

/// Whether the Companion page is printed: a stat block or a portrait is present.
pub(crate) fn has_page(data: &SheetData) -> bool {
    data.extras.companion.is_some() || data.extras.has_companion_portrait
}

/// Draw the empty portrait frame (the picture is placed over it) at `(x, y)`.
fn portrait_box(c: &mut Canvas, (x, y): (f32, f32)) -> Rect {
    let rect = (x, y, PORTRAIT_W, PORTRAIT_H);
    c.artifact(|c| c.card(rect, 3.0, None));
    rect
}

/// Build the Companion page (and its continuation pages).
pub(crate) fn build(data: &SheetData, opts: &LayoutOptions, grid: &Grid) -> Built {
    if !has_page(data) {
        return Built {
            pages: Vec::new(),
            portrait: None,
        };
    }
    let env = Env {
        d: data,
        o: opts,
        g: grid,
    };
    let title = data
        .extras
        .companion
        .as_ref()
        .map_or_else(|| "Companion".to_string(), |c| c.title());
    let continued = format!("{title} (continued)");
    let mut seq = Seq::default();
    let (mut left, mut right) = queues(data);
    let (mut c, top) = new_canvas(&env, Some(&title));
    let [a, b] = columns(grid);
    let top = match &data.extras.companion {
        Some(companion) => score_boxes(&mut c, &env, companion.abilities, top),
        None => top,
    };
    let mut left_top = top;
    let portrait = data.extras.has_companion_portrait.then(|| {
        let rect = portrait_box(&mut c, (a.0, top));
        left_top = top + PORTRAIT_H + BOXES_BELOW;
        rect
    });
    {
        let mut cx = Cx {
            c: &mut c,
            ids: &mut seq.ids,
            d: data,
            ordinal: 1,
            page_idx: 0,
            refs: &mut seq.refs,
            landed: &mut seq.landed,
        };
        fill(&mut cx, &mut left, (a.0, left_top, a.1), grid.body_bottom);
        fill(&mut cx, &mut right, (b.0, top, b.1), grid.body_bottom);
    }
    seq.push(c);
    seq.flow_pages(&env, 1, (&continued, &continued), left, right);
    Built {
        pages: seq.pages,
        portrait,
    }
}
