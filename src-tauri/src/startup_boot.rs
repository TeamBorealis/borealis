//! Startup wiring: the splash window (desktop), the background loader thread and the progress events.
//!
//! Setup returns immediately so the event loop can paint the splash; the heavy work (state, content
//! packs, AI profile) runs on a thread and reports through [`Startup`]. The main window starts hidden
//! and is only shown once loading finished, the splash was up for [`MIN_SPLASH`] and the main webview
//! rendered. Mobile has one window: the UI shows the same splash as an overlay until `startup-done`.

use crate::startup::{
    hold_remaining, Stage, Startup, StartupEmitter, StartupProgress, EVENT_DONE, EVENT_MAIN_LOADED,
    EVENT_MAIN_READY, EVENT_PROGRESS, EVENT_SPLASH_READY, MAIN_READY_TIMEOUT, MIN_SPLASH,
};
use crate::state::{AppPaths, AppState};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Listener, Manager};

/// Label of the desktop splash window.
pub const SPLASH_LABEL: &str = "splash";
/// Splash window size (650 x 350).
const SPLASH_SIZE: (f64, f64) = (650.0, 350.0);
/// Splash background (matches `--window-bg`), so the first frame is not white.
const SPLASH_BG: tauri::window::Color = tauri::window::Color(16, 16, 16, 255);

struct TauriEmitter(AppHandle);

impl StartupEmitter for TauriEmitter {
    fn progress(&self, progress: &StartupProgress) {
        let _ = self.0.emit(EVENT_PROGRESS, progress);
    }
    fn done(&self) {
        let _ = self.0.emit(EVENT_DONE, ());
    }
}

/// Install the startup flow: open the splash, wire the readiness events and start loading.
///
/// # Errors
/// If the splash window cannot be created.
pub fn install(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle().clone();
    if let Ok(data) = handle.path().app_data_dir() {
        crate::app_logs::init(&data);
    }
    crate::app_logs::log_startup();
    let startup = Arc::new(Startup::new(Box::new(TauriEmitter(handle.clone()))));
    app.manage(Arc::clone(&startup));
    for event in [EVENT_SPLASH_READY, EVENT_MAIN_LOADED] {
        let s = Arc::clone(&startup);
        app.listen(event, move |_| s.replay());
    }
    let s = Arc::clone(&startup);
    app.listen(EVENT_MAIN_READY, move |_| s.mark_main_ready());
    open_windows(&handle)?;
    let began = Instant::now();
    std::thread::Builder::new()
        .name("borealis-startup".into())
        .spawn(move || boot(&handle, &startup, began))
        .map_err(|e| tauri::Error::Anyhow(e.into()))?;
    Ok(())
}

/// Desktop: the undecorated splash opens first. Mobile: the single window is shown right away.
fn open_windows(handle: &AppHandle) -> tauri::Result<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut splash = tauri::WebviewWindowBuilder::new(
            handle,
            SPLASH_LABEL,
            tauri::WebviewUrl::App("index.html#/splash".into()),
        );
        // WebView2 shares one browser process per data directory, so the splash takes the main window's arguments.
        if let Some(args) = crate::webview_args::configured(handle.config()) {
            splash = splash.additional_browser_args(&args);
        }
        splash
            .title("Borealis")
            .inner_size(SPLASH_SIZE.0, SPLASH_SIZE.1)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .decorations(false)
            .shadow(true)
            .center()
            .background_color(SPLASH_BG)
            .build()?;
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if let Some(main) = handle.get_webview_window("main") {
        let _ = (SPLASH_SIZE, SPLASH_BG);
        main.show()?;
    }
    Ok(())
}

fn boot(handle: &AppHandle, startup: &Startup, began: Instant) {
    match load(handle, startup) {
        Ok(()) => {
            startup.report(&Stage::Starting);
            std::thread::sleep(hold_remaining(began.elapsed(), MIN_SPLASH));
            startup.complete();
            reveal_main(handle, startup);
        }
        Err(details) => {
            tracing::error!("startup failed: {details}");
            startup.fail("Borealis could not load its data.", &details);
        }
    }
}

/// Build the application state off the main thread and publish it to the commands.
fn load(handle: &AppHandle, startup: &Startup) -> Result<(), String> {
    startup.report(&Stage::Directories);
    let data = handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let (content_root, extract_error) = crate::content_root(handle);
    let paths = AppPaths {
        content_root,
        data: data.clone(),
    };
    let state = crate::build_state(paths, extract_error, &mut |step| {
        startup.report(&Stage::from(step));
    })?;
    // Mobile has no AI integration: no service, no profile, no stale-temp sweep.
    #[cfg(desktop)]
    let ai = {
        let ai = Arc::new(crate::ai_service::AiService::default());
        ai.set_data_dir(data);
        crate::ai_service::spawn_startup_sweep(borealis_ai::sweep_stale_temp_dirs);
        startup.report(&Stage::AiProfile);
        crate::setup_ai_profile(handle, &state, &ai);
        ai
    };
    #[cfg(mobile)]
    let _ = data;
    handle.manage::<AppState>(state);
    // S5 acceptance (debug builds only): answer a pending digest request once the content is loaded.
    #[cfg(debug_assertions)]
    {
        let handle = handle.clone();
        std::thread::spawn(move || {
            crate::acceptance_digest::answer_request(&handle.state::<AppState>());
        });
    }
    #[cfg(desktop)]
    handle.manage(ai);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    crate::commands_settings::spawn_startup_content_tasks(handle);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    crate::commands_app_update::spawn_startup_update_check(handle);
    Ok(())
}

/// Desktop: show the main window and close the splash. Mobile has nothing to swap.
fn reveal_main(handle: &AppHandle, startup: &Startup) {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if !startup.wait_main_ready(MAIN_READY_TIMEOUT) {
            tracing::warn!("main window did not report ready; showing it anyway");
        }
        if let Some(main) = handle.get_webview_window("main") {
            let _ = main.show();
            let _ = main.set_focus();
        }
        if let Some(splash) = handle.get_webview_window(SPLASH_LABEL) {
            let _ = splash.close();
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let _ = (handle, startup, MAIN_READY_TIMEOUT);
}

/// Closing the splash while startup is unfinished (error state, or the user gave up) quits the app.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != SPLASH_LABEL || !matches!(event, tauri::WindowEvent::Destroyed) {
        return;
    }
    let finished = window
        .try_state::<Arc<Startup>>()
        .is_some_and(|s| s.is_done());
    if !finished {
        window.app_handle().exit(0);
    }
}
