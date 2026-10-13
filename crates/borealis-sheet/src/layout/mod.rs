//! New Borealis sheet layout: builds `pdf::PdfDocument` pages from `model_types::SheetData`.
//! `crate::sheet_render` assembles the pages; this module holds the grid, chrome and page builders.

mod abilities;
mod art_place;
mod chrome;
mod grid;
mod legacy_plan;
mod lists;
mod page_cards;
mod page_core;
mod page_features;
mod page_legacy_spells;
mod page_overflow;
mod page_spells;
mod page_story;
mod tagging;
mod vitals;

#[cfg(test)]
mod tests;

use crate::image::PdfImage;
use crate::model_types::SheetData;
use crate::pdf::{FormMode, PdfDocument, PdfPage};
use crate::Paper;

/// Layout options (`SheetOptions` maps onto this).
#[derive(Debug, Clone)]
pub(crate) struct LayoutOptions {
    pub paper: Paper,
    /// Monochrome ink mode: greys only, intricate monochrome art.
    pub ink: bool,
    /// Keep form fields editable (otherwise flatten).
    pub fillable: bool,
    /// Same sections at smaller type.
    pub compact: bool,
    /// Big number is the score and the circle holds the modifier.
    pub swap_ability_positions: bool,
    /// BCP 47 language tag of the content (`/Lang`).
    pub lang: String,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            paper: Paper::Letter,
            ink: false,
            fillable: true,
            compact: false,
            swap_ability_positions: false,
            lang: "en".to_string(),
        }
    }
}

/// Page number printed in "+N more on page X" by [`build_document`] (the overflow page).
pub(crate) const CONTINUATION_PAGE: u32 = 2;

pub(crate) use chrome::footers;
pub(crate) use grid::Grid;
pub(crate) use legacy_plan::{
    dropped_note as legacy_dropped_note, level_title as legacy_level_title, plan as legacy_plan,
};
pub(crate) use page_cards::build as build_cards;
pub(crate) use page_core::build as build_page_one;
pub(crate) use page_features::{build as build_features, details_portrait_rect};
pub(crate) use page_legacy_spells::{build as build_legacy_spells, columns_avail as legacy_avail};
pub(crate) use page_overflow::{build as build_overflow, needed as overflow_needed};
pub(crate) use page_spells::build as build_spells;
pub(crate) use page_story::{build as build_story, portrait_rect as story_portrait_rect};

/// Build a one-page document (page 1 only; tests).
#[cfg(test)]
pub(crate) fn build_document(data: &SheetData, opts: &LayoutOptions) -> PdfDocument<'static> {
    let g = grid::Grid::new(opts.paper);
    let mut pages = vec![page_core::build(data, opts, &g, CONTINUATION_PAGE).page];
    chrome::footers(&mut pages, &g, data, opts.ink);
    document(pages, Vec::new(), data, opts)
}

pub(crate) use tagging::tag_pages;

/// Tag `pages` and serialise them: the one place that turns finished layout pages into a PDF.
pub(crate) fn write(
    mut pages: Vec<PdfPage>,
    images: Vec<PdfImage>,
    data: &SheetData,
    opts: &LayoutOptions,
) -> Vec<u8> {
    let mut doc = document(Vec::new(), images, data, opts);
    let builder = tag_pages(&mut pages, doc.mode);
    doc.pages = pages;
    doc.structure = Some(&builder);
    crate::pdf::write_document(&doc)
}

/// Wrap finished pages (footers already added) into a document.
pub(crate) fn document(
    pages: Vec<PdfPage>,
    images: Vec<PdfImage>,
    data: &SheetData,
    opts: &LayoutOptions,
) -> PdfDocument<'static> {
    let (page_w, page_h) = opts.paper.points();
    PdfDocument {
        page_w,
        page_h,
        pages,
        images,
        title: format!("{} \u{2013} Borealis character sheet", data.identity.name),
        lang: Some(opts.lang.clone()),
        mode: if opts.fillable {
            FormMode::Fillable
        } else {
            FormMode::Flatten
        },
        structure: None,
    }
}
