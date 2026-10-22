//! Borealis Tauri shell: application state, services and the IPC command surface.
//!
//! Layout: `state` (content hub, sessions), `session`/`library`/`builder`/`app_api` (services as
//! `impl AppState`), `snapshot_map`/`extras`/`elements` (engine -> contract DTO mapping), `dto`/`dto_ext`
//! (contract types), `magic`/`equipment`/`sheet_api`/`compendium` (play services), `error` (typed `{code, message}` errors) and `commands` (Tauri wrappers).

#[cfg(desktop)]
pub mod ai_dto;
#[cfg(desktop)]
pub mod ai_extra;
#[cfg(desktop)]
pub mod ai_host;
#[cfg(desktop)]
pub mod ai_jobs;
pub mod ai_override;
// S5 acceptance digest: debug builds only (never in a release binary).
#[cfg(debug_assertions)]
pub mod acceptance_digest;
#[cfg(desktop)]
pub mod ai_profile;
#[cfg(desktop)]
pub mod ai_service;
#[cfg(desktop)]
pub mod ai_setup;
pub mod app_api;
pub mod app_logs;
pub mod app_update;
pub mod app_update_debug;
pub mod builder;
pub mod bundled_content;
pub mod commands;
#[cfg(debug_assertions)]
pub mod commands_acceptance;
#[cfg(desktop)]
pub mod commands_ai;
#[cfg(desktop)]
pub mod commands_ai_setup;
pub mod commands_app_update;
pub mod commands_content;
pub mod commands_gear;
#[cfg(desktop)]
pub mod commands_import;
pub mod commands_pick;
pub mod commands_play;
#[cfg(desktop)]
pub mod commands_report;
pub mod commands_settings;
pub mod commands_system;
pub mod companion;
pub mod companion_block;
pub mod compendium;
mod compendium_monster;
// The only module allowed `unsafe` (Windows console FFI); see src-tauri/Cargo.toml [lints].
#[allow(unsafe_code)]
pub mod console_attach;
pub mod content_auto;
pub mod content_dto;
pub mod content_fetch_retry;
pub mod content_fetch_tls;
pub mod content_fivetools_fetch;
pub mod content_fivetools_remote;
pub mod content_fivetools_url;
pub mod content_fivetools_zip;
pub mod content_import;
pub mod content_index;
pub mod content_index_aurora;
pub(crate) mod content_overrides;
pub mod content_packs;
pub mod content_prefs;
pub mod content_preview;
pub mod content_report;
pub mod data_dirs;
pub mod derive_cache;
#[cfg(desktop)]
pub mod drive;
pub mod dto;
pub mod dto_ext;
pub mod elements;
pub mod equipment;
pub mod error;
pub mod extras;
pub mod gear_facts;
pub mod gear_info;
pub mod homebrew_api;
pub mod ids;
#[cfg(desktop)]
pub mod import_bug_report;
pub mod import_check;
#[cfg(desktop)]
pub mod import_drafts;
#[cfg(desktop)]
pub mod import_dto;
#[cfg(desktop)]
pub mod import_notes;
#[cfg(desktop)]
pub mod import_review;
#[cfg(desktop)]
pub mod import_service;
#[cfg(desktop)]
pub mod import_sources;
pub mod item_categories;
pub mod library;
pub mod magic;
pub mod manage;
pub mod mcp_mode;
pub mod native_share;
pub mod picked_files;
pub mod picked_paths;
pub mod play_api;
pub mod play_dto;
pub mod renamed_ids;
pub mod sanitize;
pub mod secondary_content;
pub mod session;
pub mod settings;
pub mod settings_dto;
pub mod sheet_api;
pub mod snapshot_map;
pub mod startup;
pub mod startup_boot;
pub mod state;
pub mod zip_read;

use state::{AppState, LoadStep};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Resolve where the bundled content lives (bundled resources, or the source tree in dev builds).
///
/// Android resources are APK assets, not files: the packs are extracted into the app data folder
/// (re-extracted when the app version or the bundled bytes change). A failure is logged and
/// returned as the diagnostic; the caller then runs with an empty content store.
#[cfg(target_os = "android")]
fn content_root(app: &AppHandle) -> (PathBuf, Option<String>) {
    let target = match app.path().app_data_dir() {
        Ok(d) => d.join("bundled-content"),
        Err(e) => return (PathBuf::new(), Some(format!("no app data folder: {e}"))),
    };
    let outcome = bundled_content::sync(
        &bundled_content::ApkAssets,
        "content",
        &state::BUNDLED_PACKS,
        &target,
        env!("CARGO_PKG_VERSION"),
    );
    match outcome {
        Ok(o) => {
            tracing::info!("bundled content: {o:?}");
            (target, None)
        }
        Err(e) => {
            tracing::error!("bundled content could not be extracted: {e}");
            (
                target,
                Some(format!("Bundled content could not be extracted: {e}")),
            )
        }
    }
}

/// Resolve where the bundled content lives (bundled resources, or the source tree in dev builds).
#[cfg(not(target_os = "android"))]
fn content_root(app: &AppHandle) -> (PathBuf, Option<String>) {
    let bundled = app
        .path()
        .resource_dir()
        .map(|d| d.join("content"))
        .unwrap_or_default();
    if bundled.join("core").join("pack.json").is_file() {
        return (bundled, None);
    }
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content");
        if dev.join("core").join("pack.json").is_file() {
            return (dev, None);
        }
    }
    (bundled, None)
}

/// Build the app state; if bundled content cannot be loaded, log it and start with an empty store
/// carrying the diagnostic (surfaced as a warning) instead of aborting the launch.
fn build_state(
    paths: state::AppPaths,
    extract_error: Option<String>,
    on_step: &mut dyn FnMut(LoadStep),
) -> Result<AppState, String> {
    match AppState::new_with_progress(paths.clone(), on_step) {
        Ok(state) => Ok(state),
        Err(e) => {
            let reason = extract_error.unwrap_or(e.message);
            tracing::error!("starting without content: {reason}");
            AppState::new_degraded(paths, &reason).map_err(|e| e.message)
        }
    }
}

/// Resolve the bundled AI profile assets (resources, or the crate's copy in dev builds).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn profile_assets(app: &AppHandle) -> PathBuf {
    let bundled = app
        .path()
        .resource_dir()
        .map(|d| d.join("ai-profile"))
        .unwrap_or_default();
    if bundled.join("claude").is_dir() {
        return bundled;
    }
    if cfg!(debug_assertions) {
        return borealis_ai::profile::default_assets_dir();
    }
    bundled
}

/// Desktop startup: install/verify/repair the Borealis AI profile and enable it for all sessions.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn setup_ai_profile(app: &AppHandle, state: &AppState, ai: &ai_service::AiService) {
    let (Ok(dir), Ok(exe)) = (
        app.path().app_data_dir().map(|d| d.join("ai-profile")),
        std::env::current_exe(),
    ) else {
        return;
    };
    let outcome = ai_profile::ensure_profile(&profile_assets(app), &dir, env!("CARGO_PKG_VERSION"))
        .map_err(|e| e.to_string());
    match &outcome {
        Ok(ai_profile::ProfileOutcome::Repaired(files)) => {
            tracing::warn!("AI profile was modified and has been restored: {files:?}");
        }
        Ok(o) => tracing::info!("AI profile: {o:?}"),
        Err(e) => tracing::error!("AI profile unavailable: {e}"),
    }
    ai.setup.set_profile_outcome(&outcome);
    if outcome.is_ok() {
        ai.set_profile(ai_profile::ProfileBase {
            dir,
            exe,
            content_dirs: vec![state.paths.content_root.clone(), state.paths.library()],
        });
    }
}

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Windows and Linux "Open with" / double-click pass the file as an argument (Linux desktop entry
    // `Exec=borealis %F`); macOS uses RunEvent::Opened. The UI drains the queue on mount; non-content
    // arguments are ignored by `push_opened_file`.
    #[cfg(any(windows, target_os = "linux"))]
    for arg in std::env::args_os().skip(1) {
        content_preview::push_opened_file(std::path::Path::new(&arg));
    }
    let builder = tauri::Builder::default();
    // Opt-in import bug reports: previews wait here until the user presses Send.
    #[cfg(desktop)]
    let builder = builder.manage(std::sync::Arc::new(
        import_bug_report::ReportOutbox::default(),
    ));
    // Phones: `open_download_page` hands the page to the system browser through the opener plugin's
    // Rust API. No capability grants the web view an opener permission, and link interception is off.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder = builder.plugin(
        tauri_plugin_opener::Builder::new()
            .open_js_links_on_click(false)
            .build(),
    );
    // Android: the in-app Kotlin share plugin behind "Share logs" (see `native_share`).
    #[cfg(target_os = "android")]
    let builder = builder.plugin(native_share::init());
    builder
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            startup_boot::on_window_event(window, event);
            #[cfg(desktop)]
            picked_files::on_window_event(window, event);
        })
        .setup(|app| Ok(startup_boot::install(app)?))
        // AI commands carry `#[cfg(desktop)]` on their own line: mobile has no AI integration. The
        // contract test reads this list as text and skips attribute lines, so it still sees every command.
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::get_settings,
            commands::save_settings,
            commands::list_characters,
            commands::create_character,
            commands::load_character,
            commands::get_snapshot,
            commands::save_character,
            commands::save_character_as,
            commands::close_character,
            commands::delete_character,
            commands::duplicate_character,
            commands::update_character_meta,
            commands::set_portrait,
            commands::import_character,
            commands::recheck_missing_content,
            commands::export_character,
            commands::get_slot_options,
            commands::apply_selection,
            commands::clear_selection,
            commands::set_xp,
            commands::level_up,
            commands::level_down,
            commands::add_multiclass,
            commands::set_hp_roll,
            commands::set_ability_method,
            commands::set_base_score,
            commands::roll_abilities,
            commands::assign_standard_array,
            commands::reset_abilities,
            commands::set_character_options,
            commands::get_ruleset_profile,
            commands_gear::replace_spell,
            commands_gear::set_spell_prepared,
            commands_gear::explain_stat,
            commands_gear::search_items,
            commands_gear::list_item_categories,
            commands_gear::add_item,
            commands_gear::remove_item,
            commands_gear::equip_item,
            commands_gear::unequip_item,
            commands_gear::set_attuned,
            commands_gear::set_rolled_max,
            commands_gear::reroll_max,
            commands_gear::store_item,
            commands_gear::move_item,
            commands_gear::update_item_details,
            commands_gear::extract_pack,
            commands_gear::adjust_coins,
            commands_gear::convert_coins,
            commands_gear::set_storage_names,
            commands_gear::render_sheet,
            commands_gear::export_sheet,
            commands_gear::search_compendium,
            commands_gear::get_element,
            commands_gear::compendium_facets,
            commands_content::update_details,
            commands_content::upsert_attack,
            commands_content::remove_attack,
            commands_content::move_attack,
            commands_content::update_companion,
            commands_content::preview_source_restrictions,
            commands_content::set_source_restrictions,
            commands_content::set_allow_cross_edition,
            commands_content::get_compatibility_report,
            commands_content::list_packs,
            commands_content::import_pack,
            commands_content::preview_import_pack,
            commands_content::install_import_preview,
            commands_content::cancel_import_preview,
            commands_content::take_opened_files,
            commands_content::set_pack_enabled,
            commands_content::remove_pack,
            commands_content::list_sources,
            commands_content::list_indexes,
            commands_content::add_index,
            commands_content::remove_index,
            commands_content::update_indexes,
            commands_content::clear_downloaded_content,
            commands_content::scan_5etools_folder,
            commands_content::stage_5etools_zip,
            commands_content::stage_5etools_url,
            commands_content::list_5etools_urls,
            commands_content::remove_5etools_url,
            commands_content::update_5etools_url,
            commands_content::import_5etools,
            #[cfg(desktop)]
            commands_ai::ai_status,
            #[cfg(desktop)]
            commands_ai::ai_start_task,
            #[cfg(desktop)]
            commands_ai::ai_cancel_task,
            #[cfg(desktop)]
            commands_ai::apply_plan,
            #[cfg(desktop)]
            commands_ai::encounter_evaluate,
            #[cfg(desktop)]
            commands_ai::encounter_suggest,
            #[cfg(desktop)]
            commands_ai::ai_host_enable,
            #[cfg(desktop)]
            commands_ai::ai_host_disable,
            #[cfg(desktop)]
            commands_ai::ai_host_status,
            #[cfg(desktop)]
            commands_ai::ai_host_begin_pairing,
            #[cfg(desktop)]
            commands_ai::ai_host_revoke,
            #[cfg(desktop)]
            commands_ai_setup::ai_setup_status,
            #[cfg(desktop)]
            commands_ai_setup::ai_setup_confirm_install,
            #[cfg(desktop)]
            commands_ai_setup::ai_setup_install,
            #[cfg(desktop)]
            commands_ai_setup::ai_setup_login,
            #[cfg(desktop)]
            commands_ai_setup::ai_setup_complete,
            #[cfg(desktop)]
            commands_import::import_start,
            #[cfg(desktop)]
            commands_import::import_get_draft,
            #[cfg(desktop)]
            commands_import::import_choose_parent,
            #[cfg(desktop)]
            commands_import::import_revise,
            #[cfg(desktop)]
            commands_import::import_update_element,
            #[cfg(desktop)]
            commands_import::import_relocate_element,
            #[cfg(desktop)]
            commands_import::import_regenerate_element,
            #[cfg(desktop)]
            commands_import::import_accept_element,
            #[cfg(desktop)]
            commands_import::import_set_text_only,
            #[cfg(desktop)]
            commands_import::import_finalize,
            #[cfg(desktop)]
            commands_import::import_discard,
            #[cfg(desktop)]
            commands_import::ai_validate_homebrew,
            #[cfg(desktop)]
            commands_import::ai_add_homebrew_to_pack,
            #[cfg(desktop)]
            commands_report::import_report_preview,
            #[cfg(desktop)]
            commands_report::import_report_send,
            commands_play::preview_convert_to_2024,
            commands_play::apply_convert_to_2024,
            commands_play::spend_resource,
            commands_play::restore_resource,
            commands_play::short_rest,
            commands_play::long_rest,
            commands_play::set_toggle,
            commands_play::set_exhaustion,
            commands_play::set_heroic_inspiration,
            commands_play::choose_weapon_masteries,
            commands_play::set_wild_shape_forms,
            commands_play::choose_equipment_package,
            commands_pick::pick_path,
            commands_pick::take_picked_files,
            #[cfg(desktop)]
            commands_pick::set_ai_override,
            commands_settings::get_data_dirs,
            commands_settings::set_documents_dir,
            commands_settings::set_secondary_content_dir,
            commands_settings::unlink_secondary_content_dir,
            commands_settings::restore_default_settings,
            commands_settings::clear_caches,
            commands_settings::content_update_status,
            commands_app_update::app_update_status,
            commands_app_update::check_app_update,
            commands_app_update::open_download_page,
            commands_system::open_app_folder,
            commands_system::get_recent_logs,
            commands_system::share_logs,
            commands_system::restart_app,
            #[cfg(debug_assertions)]
            commands_acceptance::acceptance_digest,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Borealis")
        .run(|app, event| {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if let tauri::RunEvent::Opened { urls } = &event {
                commands_content::handle_opened(app, urls);
            }
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_version_is_semver_like() {
        assert_eq!(super::commands::app_version().split('.').count(), 3);
    }
}
