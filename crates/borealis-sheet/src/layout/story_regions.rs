//! Story write-in regions: which story field becomes which ruled region, per page.

use crate::layout::page_features::features_flow::{AppKind, Piece, Region};
use crate::model_types::SheetData;
use std::collections::VecDeque;

/// Text of a story id.
fn text_of(d: &SheetData, id: &str) -> String {
    let s = &d.story;
    match id {
        "story.traits" => &s.traits,
        "story.ideals" => &s.ideals,
        "story.bonds" => &s.bonds,
        "story.flaws" => &s.flaws,
        "story.trinket" => &s.trinket,
        "story.backstory" => &s.backstory,
        "story.organization" => &s.organization,
        "story.allies" => &s.allies,
        "story.treasure" => &s.treasure,
        "story.quest" => &s.quest,
        "story.additional_features" => &s.additional_features,
        "story.notes" => &s.notes,
        "xtra.companion" => &d.extras.companion_name,
        _ => return String::new(),
    }
    .clone()
}

/// A ruled region for story field `id`.
pub(crate) fn region(d: &SheetData, id: &str, title: &str, min_lines: usize, fill: bool) -> Piece {
    Piece::Region(Region {
        id: id.to_string(),
        tip: title.to_string(),
        title: title.to_string(),
        text: text_of(d, id),
        min_lines,
        fill,
    })
}

fn all(d: &SheetData, items: &[(&str, &str, usize, bool)]) -> VecDeque<Piece> {
    items
        .iter()
        .map(|(id, title, n, fill)| region(d, id, title, *n, *fill))
        .collect()
}

/// 2024 Story page: `(left, right)` queues.
pub(crate) fn story_2024(d: &SheetData) -> (VecDeque<Piece>, VecDeque<Piece>) {
    let mut left: VecDeque<Piece> = VecDeque::new();
    left.push_back(Piece::Appearance(AppKind::Story));
    left.extend(all(
        d,
        &[
            ("story.traits", "Personality traits", 5, false),
            ("story.ideals", "Ideals", 4, false),
            ("story.bonds", "Bonds", 4, false),
            ("story.flaws", "Flaws", 4, false),
            ("story.trinket", "Trinket", 3, false),
        ],
    ));
    let right = all(
        d,
        &[
            ("story.backstory", "Backstory", 14, false),
            ("story.organization", "Organisation", 2, false),
            ("story.allies", "Allies", 5, false),
            ("xtra.companion", "Companion", 2, false),
            ("story.notes", "Notes", 6, true),
        ],
    );
    (left, right)
}

/// 2014 Details page right column.
pub(crate) fn details_2014(d: &SheetData) -> VecDeque<Piece> {
    let mut q: VecDeque<Piece> = VecDeque::new();
    q.push_back(Piece::Appearance(AppKind::Details));
    q.extend(all(
        d,
        &[
            ("story.traits", "Personality traits", 2, false),
            ("story.ideals", "Ideals", 1, false),
            ("story.bonds", "Bonds", 1, false),
            ("story.flaws", "Flaws", 1, false),
            ("story.trinket", "Trinket", 1, false),
            ("story.backstory", "Backstory", 6, false),
            ("story.organization", "Organisation", 1, false),
            ("story.allies", "Allies", 2, false),
            ("story.treasure", "Treasure", 2, false),
            ("story.quest", "Quest items", 1, false),
            ("xtra.companion", "Companion", 1, false),
            ("story.notes", "Notes", 2, true),
        ],
    ));
    q
}
