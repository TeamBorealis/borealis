//! Public render entry points of the Borealis sheet: assemble the page builders of [`crate::layout`]
//! (PDF) and the HTML preview ([`crate::html_sheet`]) from one [`SheetData`].
//!
//! Page selection ([`PageSelection`]), shared by the PDF and the preview:
//! - `core`: page 1 (+ its overflow page) and, for 2024, the features/equipment pages; for 2014 the
//!   Details pages (full feature text).
//! - `inventory`: 2014 Inventory pages; for 2024 the features/equipment pages (they hold the gear).
//! - `story`: 2024 Story pages (only when there is story content); for 2014 the Details pages
//!   (they carry the story fields).
//! - `spellcasting`: the spell pages of casters (none for non-casters).
//!
//! The companion stat block (when the app supplies one) is its own page after the story pages,
//! selected with `story`.
//!
//! Cards (`layout::build_cards`) are appended after all other pages. `legacy_spellcasting` swaps the
//! spell pages for the fixed-capacity basic page (`layout::build_legacy_spells`).

use crate::base64;
use crate::html_sheet::{self, HtmlOptions, PortraitImage};
use crate::image::{decode_portrait, PdfImage};
use crate::layout::{self, Grid, LayoutOptions};
use crate::model_types::{Edition, SheetData};
use crate::pdf::{PageOp, PdfPage};
use crate::{
    card_model, sheet_data_for, PageSelection, SheetContent, SheetError, SheetOptions, SheetTheme,
};
use borealis_character::Character;
use borealis_engine::CharacterSnapshot;

/// Marker of one page in the HTML preview.
const HTML_PAGE_MARKER: &str = "<section class=\"page ";
/// Document language until the UI locale reaches the renderer.
const DEFAULT_LANG: &str = "en";
/// Portrait inset inside its frame, in points.
const PORTRAIT_INSET: f32 = 2.0;

fn layout_options(options: &SheetOptions) -> LayoutOptions {
    LayoutOptions {
        paper: options.paper,
        ink: options.theme == SheetTheme::Ink,
        fillable: options.form_fillable,
        compact: options.compact,
        swap_ability_positions: options.swap_ability_positions,
        lang: DEFAULT_LANG.to_string(),
    }
}

/// The allow-listed image MIME type matching the magic bytes, if any.
fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("image/png"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        _ => None,
    }
}

/// Decoded portrait bytes with their sniffed MIME type (`None` when absent or unsupported).
fn portrait_bytes(character: &Character) -> Option<(&'static str, Vec<u8>)> {
    let p = character.portrait.as_ref()?;
    let bytes = base64::decode(&p.data_base64)?;
    let mime = sniff_mime(&bytes)?;
    Some((mime, bytes))
}

/// Pages whose selection flags are on, in the order of the page plan (see module docs).
struct Plan {
    page_one: bool,
    main: bool,
    spells: bool,
    inventory: bool,
    story: bool,
    companion: bool,
}

fn plan(data: &SheetData, sel: PageSelection) -> Plan {
    let is24 = data.edition == Edition::Dnd2024;
    Plan {
        page_one: sel.core,
        main: if is24 {
            sel.core || sel.inventory
        } else {
            sel.core || sel.story
        },
        spells: sel.spellcasting,
        inventory: !is24 && sel.inventory,
        story: is24 && sel.story,
        companion: data.extras.companion.is_some() && sel.story,
    }
}

/// Embed the portrait into `page` inside `rect` (contain, centred).
fn place_portrait(
    page: &mut PdfPage,
    images: &mut Vec<PdfImage>,
    rect: (f32, f32, f32, f32),
    img: PdfImage,
) {
    let (x, y, w, h) = rect;
    let (bw, bh) = (w - 2.0 * PORTRAIT_INSET, h - 2.0 * PORTRAIT_INSET);
    let scale = (bw / img.width as f32).min(bh / img.height as f32);
    let (iw, ih) = (img.width as f32 * scale, img.height as f32 * scale);
    let index = images.len();
    images.push(img);
    page.ops.push(PageOp::Image {
        x: x + (w - iw) / 2.0,
        y: y + (h - ih) / 2.0,
        w: iw,
        h: ih,
        index,
    });
}

/// The sheet model with the companion stat block the app supplied.
fn data_with_companion(
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
    content: &SheetContent,
) -> SheetData {
    let mut data = sheet_data_for(character, snapshot, &content.spell_facts, options.edition);
    data.extras.companion = content.companion.clone();
    data
}

/// Render the PDF.
pub(crate) fn render_pdf(
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
    content: &SheetContent,
) -> Result<Vec<u8>, SheetError> {
    let data = data_with_companion(character, snapshot, options, content);
    let lopts = layout_options(options);
    let (pages, images) = build_pages(&data, character, snapshot, options, content)?;
    Ok(layout::write(pages, images, &data, &lopts))
}

/// Every page of the PDF (footers included), before tagging and serialisation.
pub(crate) fn build_pages(
    data: &SheetData,
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
    content: &SheetContent,
) -> Result<(Vec<PdfPage>, Vec<PdfImage>), SheetError> {
    let lopts = layout_options(options);
    let g = Grid::new(options.paper);
    let want = plan(data, options.pages);
    let card_pages = card_model::pages(character, snapshot, options, content);
    let mut pages: Vec<PdfPage> = Vec::new();
    let mut images: Vec<PdfImage> = Vec::new();

    // Page 1 points at later pages (2014 Details and Inventory), so it is built last and put
    // in front; `lead` counts it and its overflow page.
    let band = if want.page_one {
        layout::page_one_band(data, &lopts, &g)
    } else {
        None
    };
    let overflow = want.page_one && layout::overflow_needed(data);
    let lead = u32::from(want.page_one) + u32::from(overflow);
    let first_main = lead + 1;
    let mut features = layout::build_features(data, &lopts, &g, first_main, band.as_ref());
    let portrait = portrait_bytes(character).and_then(|(m, b)| decode_portrait(m, b));
    let mut main = if want.main {
        std::mem::take(&mut features.main)
    } else {
        Vec::new()
    };
    if let (Some(img), Some(p), Edition::Dnd2014) =
        (portrait.clone(), main.first_mut(), data.edition)
    {
        place_portrait(
            p,
            &mut images,
            layout::details_portrait_rect(&g, lopts.compact),
            img,
        );
    }
    pages.append(&mut main);
    if want.spells {
        let first = lead + pages.len() as u32 + 1;
        if options.legacy_spellcasting {
            pages.extend(layout::build_legacy_spells(
                data,
                &lopts,
                options.include_non_prepared_spells,
                &g,
            ));
        } else {
            pages.extend(layout::build_spells(
                data,
                &lopts,
                options.include_non_prepared_spells,
                &g,
                first,
            ));
        }
    }
    let mut inventory_page = None;
    if want.inventory {
        let first = lead + pages.len() as u32 + 1;
        let inv = features.numbered_inventory(&lopts, first);
        if !inv.is_empty() {
            inventory_page = Some(first);
        }
        pages.extend(inv);
    }
    if want.story {
        let first = lead + pages.len() as u32 + 1;
        let mut story = layout::build_story(data, &lopts, &g, first);
        if let (Some(img), Some(p)) = (portrait, story.first_mut()) {
            place_portrait(
                p,
                &mut images,
                layout::story_portrait_rect(&g, lopts.compact),
                img,
            );
        }
        pages.append(&mut story);
    }
    if want.companion {
        pages.extend(layout::build_companion(data, &lopts, &g));
    }
    pages.extend(layout::build_cards(&card_pages, data, &lopts, &g));
    if want.page_one {
        let refs = layout::PageRefs {
            continuation: layout::CONTINUATION_PAGE,
            band: layout::BandRefs {
                details: first_main,
                inventory: inventory_page,
            },
        };
        let mut front = vec![layout::build_page_one(data, &lopts, &g, refs, band.as_ref()).page];
        if overflow {
            front.push(layout::build_overflow(data, &lopts, &g));
        }
        pages.splice(0..0, front);
    }
    if pages.is_empty() {
        return Err(SheetError::NoPages);
    }
    layout::footers(&mut pages, &g, data, lopts.ink);
    Ok((pages, images))
}

/// Render the HTML preview.
pub(crate) fn render_html(
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
    content: &SheetContent,
) -> Result<String, SheetError> {
    let data = data_with_companion(character, snapshot, options, content);
    let portrait = portrait_bytes(character).map(|(mime, bytes)| PortraitImage {
        mime: mime.to_string(),
        base64: base64::encode(&bytes),
    });
    let opts = HtmlOptions {
        cards: card_model::pages(character, snapshot, options, content),
        ink: options.theme == SheetTheme::Ink,
        paper: options.paper,
        compact: options.compact,
        swap_ability_positions: options.swap_ability_positions,
        include_non_prepared_spells: options.include_non_prepared_spells,
        legacy_spellcasting: options.legacy_spellcasting,
        style_formatting: options.style_formatting,
        pages: options.pages,
        lang: DEFAULT_LANG.to_string(),
        portrait,
    };
    let html = html_sheet::render_sheet(&data, &opts);
    if html.contains(HTML_PAGE_MARKER) {
        Ok(html)
    } else {
        Err(SheetError::NoPages)
    }
}
