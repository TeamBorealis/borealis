//! Character sheet rendering from a derived [`CharacterSnapshot`] plus the [`Character`] details.
//!
//! One typed, layout-agnostic model ([`model_types::SheetData`]) feeds two renderers: self-contained
//! HTML for the in-app preview ([`render_html`]) and a pure-Rust PDF writer ([`render_pdf`]) that
//! embeds subset Noto fonts. The layout is original to Borealis.

mod art;
mod base64;
mod bidi;
mod card_model;
mod companion;
mod fonts;
mod html_sheet;
mod image;
mod layout;
#[cfg(test)]
mod layout_guard_tests;
mod model;
mod model_2014;
mod model_2024;
mod model_common;
#[cfg(test)]
mod model_fixtures;
pub mod model_ids;
#[cfg(test)]
mod model_tests;
mod model_text;
pub mod model_types;
mod os_fonts;
mod pdf;
mod pdf_draw;
mod pdf_fonts;
mod pdf_form;
mod pdf_tags;
mod portrait_resize;
mod shaping;
mod sheet_render;
mod spell_facts;
mod table_text;

pub use companion::{CompanionEntry, CompanionSheet, CompanionSkillLine};
pub use portrait_resize::{downscale_portrait, ResizedPortrait, MAX_PORTRAIT_EDGE_PX};
pub use spell_facts::{spell_effect, spell_facts};

use borealis_character::Character;
use borealis_engine::CharacterSnapshot;

/// Paper size of the printed sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paper {
    /// US Letter (8.5 x 11 in).
    #[default]
    Letter,
    /// ISO A4 (210 x 297 mm).
    A4,
}

impl Paper {
    /// Page size in PDF points (width, height).
    #[must_use]
    pub fn points(self) -> (f32, f32) {
        match self {
            Paper::Letter => (612.0, 792.0),
            Paper::A4 => (595.0, 842.0),
        }
    }
}

/// Which pages to include.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSelection {
    /// Page 1: core stats.
    pub core: bool,
    /// Page 2: spellcasting (omitted automatically for non-casters).
    pub spellcasting: bool,
    /// Page 3: inventory and currency.
    pub inventory: bool,
    /// Page 4: background, personality, notes and portrait.
    pub story: bool,
}

impl Default for PageSelection {
    fn default() -> Self {
        Self {
            core: true,
            spellcasting: true,
            inventory: true,
            story: true,
        }
    }
}

/// Which generated card groups to append to the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CardKinds {
    /// One card per known or prepared spell.
    pub spell: bool,
    /// One card per inventory line.
    pub item: bool,
    /// One card per attack.
    pub attack: bool,
    /// One card per displayed feature, trait or feat.
    pub feature: bool,
}

/// Card groups that start on a fresh page (spell cards have no such option).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardBreaks {
    /// Item cards.
    pub item: bool,
    /// Attack cards.
    pub attack: bool,
    /// Feature cards.
    pub feature: bool,
}

impl Default for CardBreaks {
    fn default() -> Self {
        Self {
            item: true,
            attack: false,
            feature: true,
        }
    }
}

/// Colour treatment of the exported sheet (light paper only; no dark variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SheetTheme {
    /// Full-colour light paper (default).
    #[default]
    Colour,
    /// Ink: colourless, monochrome ornaments for black-and-white printing.
    Ink,
}

/// Which rules edition's sheet to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SheetEdition {
    /// Follow the character's ruleset (default).
    #[default]
    Auto,
    /// The 2014 (SRD 5.1) sheet.
    Y2014,
    /// The 2024 (SRD 5.2.1) sheet.
    Y2024,
}

/// Rendering options (remembered per character by the caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetOptions {
    /// Colour or ink (monochrome) rendering. Honoured by the new layout.
    pub theme: SheetTheme,
    /// Sheet edition override; `Auto` follows the snapshot's ruleset. Honoured by the new layout: the typed model
    /// ([`sheet_data`]) always follows the snapshot ruleset, so the layout picks the page set from this.
    pub edition: SheetEdition,
    /// Paper size.
    pub paper: Paper,
    /// Pages to include.
    pub pages: PageSelection,
    /// Smaller type and tighter rows.
    pub compact: bool,
    /// Fillable form fields (default on); off flattens the PDF to static text. Honoured by the new layout.
    pub form_fillable: bool,
    /// Present features and traits as a bold-labelled list (on) or plain running text (off).
    pub style_formatting: bool,
    /// Print the modifier before the score in the ability table.
    pub swap_ability_positions: bool,
    /// List unprepared leveled spells of prepare-casters on the spellcasting page.
    pub include_non_prepared_spells: bool,
    /// Generated card groups.
    pub cards: CardKinds,
    /// Card groups that start on a new page.
    pub cards_new_page: CardBreaks,
    /// Use the single-page basic spellcasting layout.
    pub legacy_spellcasting: bool,
}

impl Default for SheetOptions {
    fn default() -> Self {
        Self {
            theme: SheetTheme::default(),
            edition: SheetEdition::default(),
            paper: Paper::default(),
            pages: PageSelection::default(),
            compact: false,
            form_fillable: true,
            style_formatting: true,
            swap_ability_positions: false,
            include_non_prepared_spells: true,
            cards: CardKinds::default(),
            cards_new_page: CardBreaks::default(),
            legacy_spellcasting: false,
        }
    }
}

/// Text for one card, resolved from the content by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardText {
    /// Line under the title (level and school, category).
    pub subtitle: String,
    /// Plain-text description with the key facts.
    pub body: String,
    /// Where the card comes from (source book).
    pub source: String,
}

/// Content-derived card text the snapshot does not carry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SheetContent {
    /// Spell card text by spell id.
    pub spells: std::collections::HashMap<String, CardText>,
    /// Item card text by item element id.
    pub items: std::collections::HashMap<String, CardText>,
    /// Spell facts by spell id for the typed sheet model (pass to [`sheet_data`]).
    pub spell_facts: model_types::SpellFactsMap,
    /// The companion creature's stat block (printed as its own page when set).
    pub companion: Option<CompanionSheet>,
}

/// Rendering failures.
#[derive(Debug, thiserror::Error)]
pub enum SheetError {
    /// Every page was deselected or empty.
    #[error("the sheet has no pages to render with these options")]
    NoPages,
}

/// The typed, edition-aware sheet model: every value and form-field id of the sheet,
/// normalised and independent of layout. The edition follows the snapshot's ruleset.
/// `spell_facts` maps spell ids to content facts and may be empty.
#[must_use]
pub fn sheet_data(
    character: &Character,
    snapshot: &CharacterSnapshot,
    spell_facts: &model_types::SpellFactsMap,
) -> model_types::SheetData {
    sheet_data_for(character, snapshot, spell_facts, SheetEdition::Auto)
}

/// Like [`sheet_data`] with the edition forced (`Auto` follows the snapshot ruleset): the field
/// set and labels are those of the chosen edition even when the character is of the other one.
#[must_use]
pub fn sheet_data_for(
    character: &Character,
    snapshot: &CharacterSnapshot,
    spell_facts: &model_types::SpellFactsMap,
    edition: SheetEdition,
) -> model_types::SheetData {
    model::build_sheet_data(character, snapshot, spell_facts, edition)
}

/// Render the HTML preview (a complete, self-contained document).
///
/// # Errors
/// [`SheetError::NoPages`] when the options leave nothing to render.
pub fn render_html(
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
) -> Result<String, SheetError> {
    render_html_with(character, snapshot, options, &SheetContent::default())
}

/// [`render_html`] with content-derived card text.
///
/// # Errors
/// [`SheetError::NoPages`] when the options leave nothing to render.
pub fn render_html_with(
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
    content: &SheetContent,
) -> Result<String, SheetError> {
    sheet_render::render_html(character, snapshot, options, content)
}

/// Render the PDF (deterministic: no timestamps or random ids).
///
/// # Errors
/// [`SheetError::NoPages`] when the options leave nothing to render.
pub fn render_pdf(
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
) -> Result<Vec<u8>, SheetError> {
    render_pdf_with(character, snapshot, options, &SheetContent::default())
}

/// [`render_pdf`] with content-derived card text.
///
/// # Errors
/// [`SheetError::NoPages`] when the options leave nothing to render.
pub fn render_pdf_with(
    character: &Character,
    snapshot: &CharacterSnapshot,
    options: &SheetOptions,
    content: &SheetContent,
) -> Result<Vec<u8>, SheetError> {
    sheet_render::render_pdf(character, snapshot, options, content)
}
