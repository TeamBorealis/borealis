//! Character sheet preview and export services.

use crate::compendium::facts;
use crate::dto::ExportResult;
use crate::dto_ext::{SheetOptionsDto, SheetPreview};
use crate::elements::plain_text;
use crate::error::{ApiError, ApiResult};
use crate::ids::{file_stem, rfc3339};
use crate::settings::{SheetEditionSetting, SheetSettings, SheetThemeSetting};
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_engine::CharacterSnapshot;
use borealis_sheet::{
    render_html_with, render_pdf_with, CardBreaks, CardKinds, CardText, SheetContent, SheetEdition,
    SheetError, SheetOptions, SheetTheme,
};
use std::collections::HashSet;
use std::time::SystemTime;

/// Sheet options for the saved CHARACTER SHEET settings.
#[must_use]
pub fn sheet_options(s: &SheetSettings) -> SheetOptions {
    SheetOptions {
        theme: match s.theme {
            SheetThemeSetting::Colour => SheetTheme::Colour,
            SheetThemeSetting::Ink => SheetTheme::Ink,
        },
        edition: match s.edition {
            SheetEditionSetting::Auto => SheetEdition::Auto,
            SheetEditionSetting::Y2014 => SheetEdition::Y2014,
            SheetEditionSetting::Y2024 => SheetEdition::Y2024,
        },
        form_fillable: s.form_fillable,
        style_formatting: s.style_formatting,
        swap_ability_positions: s.swap_ability_positions,
        include_non_prepared_spells: s.include_non_prepared_spells,
        cards: CardKinds {
            spell: s.cards.spell,
            item: s.cards.item,
            attack: s.cards.attack,
            feature: s.cards.feature,
        },
        cards_new_page: CardBreaks {
            item: s.cards_new_page.item,
            attack: s.cards_new_page.attack,
            feature: s.cards_new_page.feature,
        },
        legacy_spellcasting: s.legacy_spellcasting_page,
        ..SheetOptions::default()
    }
}

const FACT_SKIP: [&str; 3] = ["Type", "Source", "Category"];

fn card_text(el: &borealis_content::Element) -> CardText {
    let level_school = ["Level", "School", "Category"];
    let all = facts(el);
    let pick = |labels: &[&str]| -> Vec<String> {
        all.iter()
            .filter(|f| labels.contains(&f.label.as_str()))
            .map(|f| f.value.clone())
            .collect()
    };
    let details: Vec<String> = all
        .iter()
        .filter(|f| {
            !level_school.contains(&f.label.as_str()) && !FACT_SKIP.contains(&f.label.as_str())
        })
        .map(|f| format!("{}: {}", f.label, f.value))
        .collect();
    let text = plain_text(&el.description);
    let mut body = details.join(". ");
    if !text.is_empty() {
        if !body.is_empty() {
            body.push_str(". ");
        }
        body.push_str(&text);
    }
    CardText {
        subtitle: pick(&level_school).join(", "),
        body,
        source: el.source.clone(),
    }
}

/// Content text for the spell and item cards of a snapshot.
fn sheet_content(
    store: &borealis_content::ContentStore,
    snap: &CharacterSnapshot,
    companion: Option<borealis_sheet::CompanionSheet>,
    companion_portrait: Option<borealis_character::Portrait>,
) -> SheetContent {
    let mut out = SheetContent {
        companion,
        companion_portrait,
        ..SheetContent::default()
    };
    let mut seen = HashSet::new();
    for sp in snap.spellcasting.iter().flat_map(|c| &c.spells) {
        if let (true, Some(el)) = (seen.insert(sp.id.clone()), store.element(&sp.id)) {
            out.spells.insert(sp.id.clone(), card_text(el));
            out.spell_facts
                .insert(sp.id.clone(), borealis_sheet::spell_facts(el));
        }
    }
    for item in &snap.inventory.items {
        if let (true, Some(el)) = (
            seen.insert(item.item.id.clone()),
            store.element(&item.item.id),
        ) {
            out.items.insert(item.item.id.clone(), card_text(el));
        }
    }
    out
}

/// The companion's stat block, when a creature is chosen.
fn companion_sheet(
    ctx: &crate::session::Ctx<'_>,
    id: &str,
    session: &crate::state::Session,
    snap: &CharacterSnapshot,
) -> Option<borealis_sheet::CompanionSheet> {
    let input = crate::snapshot_map::MapInput {
        id,
        character: &session.character,
        dirty: session.dirty,
        display_path: String::new(),
        hub: ctx.hub,
        settings: ctx.settings,
        rolls: &session.rolls,
        warnings: &[],
    };
    crate::companion::companion(&input, snap).and_then(|c| crate::companion::sheet_block(&c))
}

/// The companion's portrait picture; a stored picture that no longer validates is left out.
fn companion_portrait(session: &crate::state::Session) -> Option<borealis_character::Portrait> {
    let url = crate::extras::ui_extras(&session.character).companion_portrait?;
    crate::extras::portrait_from_data_url(&url).ok()
}

const PAGE_MARKER: &str = "<section class=\"page ";

/// The rendered sheet file, before it is saved or sent to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetFile {
    /// Suggested file name.
    pub file_name: String,
    /// Media type.
    pub mime: &'static str,
    /// File bytes.
    pub bytes: Vec<u8>,
}

impl SheetFile {
    /// The file as an `ExportResult` carrying its bytes (no filesystem access).
    #[must_use]
    pub fn into_base64(self) -> ExportResult {
        ExportResult {
            file_name: self.file_name,
            mime: self.mime.into(),
            path: None,
            data_base64: Some(STANDARD.encode(self.bytes)),
        }
    }
}

fn sheet_error(e: &SheetError) -> ApiError {
    match e {
        SheetError::NoPages => ApiError::invalid("The sheet has no pages to render."),
    }
}

fn is_html(options: &SheetOptionsDto) -> ApiResult<bool> {
    match options.format.as_deref() {
        None | Some("pdf") => Ok(false),
        Some("html") => Ok(true),
        Some(_) => Err(ApiError::invalid("Sheet format must be pdf or html.")),
    }
}

impl AppState {
    /// Render the HTML preview of a character's sheet.
    ///
    /// # Errors
    /// `not_found` if the character is not open, `invalid_argument` when nothing can be rendered.
    pub fn render_sheet(&self, id: &str, _options: &SheetOptionsDto) -> ApiResult<SheetPreview> {
        let settings = self.settings()?.sheet;
        self.read(id, |ctx, s| {
            let snap = ctx.hub.engine.derive(&s.character.build);
            let content = sheet_content(
                &ctx.hub.store,
                &snap,
                companion_sheet(ctx, id, s, &snap),
                companion_portrait(s),
            );
            let html = render_html_with(&s.character, &snap, &sheet_options(&settings), &content)
                .map_err(|e| sheet_error(&e))?;
            Ok(SheetPreview {
                page_count: html.matches(PAGE_MARKER).count().max(1),
                html,
                generated_at: rfc3339(SystemTime::now()),
            })
        })
    }

    /// Render the sheet as a PDF (default) or HTML file.
    ///
    /// # Errors
    /// `not_found` if the character is not open, `invalid_argument` for a bad format.
    pub fn sheet_file(&self, id: &str, options: &SheetOptionsDto) -> ApiResult<SheetFile> {
        let html = is_html(options)?;
        let settings = self.settings()?.sheet;
        self.read(id, |ctx, s| {
            let snap = ctx.hub.engine.derive(&s.character.build);
            let content = sheet_content(
                &ctx.hub.store,
                &snap,
                companion_sheet(ctx, id, s, &snap),
                companion_portrait(s),
            );
            let render_options = sheet_options(&settings);
            let stem = file_stem(&s.character.meta.name);
            if html {
                let doc = render_html_with(&s.character, &snap, &render_options, &content)
                    .map_err(|e| sheet_error(&e))?;
                return Ok(SheetFile {
                    file_name: format!("{stem}.html"),
                    mime: "text/html",
                    bytes: doc.into_bytes(),
                });
            }
            let bytes = render_pdf_with(&s.character, &snap, &render_options, &content)
                .map_err(|e| sheet_error(&e))?;
            Ok(SheetFile {
                file_name: format!("{stem}.pdf"),
                mime: "application/pdf",
                bytes,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_theme_and_edition_reach_the_render_options() {
        let stored: SheetSettings =
            serde_json::from_str(r#"{"theme":"ink","edition":"2014","formFillable":false}"#)
                .unwrap();
        let o = sheet_options(&stored);
        assert_eq!(o.theme, SheetTheme::Ink);
        assert_eq!(o.edition, SheetEdition::Y2014);
        assert!(!o.form_fillable);
        // An old settings file without the new keys gets the defaults.
        let o = sheet_options(&serde_json::from_str::<SheetSettings>("{}").unwrap());
        assert_eq!(
            (o.theme, o.edition),
            (SheetTheme::Colour, SheetEdition::Auto)
        );
        assert!(!o.form_fillable, "flattened by default");
    }
}
