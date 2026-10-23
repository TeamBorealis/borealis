//! WebView2 browser arguments for the Windows WebDriver smoke (debug builds only).
//!
//! wry always hands WebView2 its own browser arguments, which override both the
//! `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` environment variable and the registry policy, so msedgedriver
//! never finds a remote-debugging port. Debug builds read that variable themselves and append it to wry's
//! defaults for every window; release builds are unchanged.

/// The environment variable WebView2 documents for extra browser arguments.
pub const ENV_VAR: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";

/// wry's default WebView2 arguments (mini menu and SmartScreen off, autoplay allowed), kept when extending.
const WRY_DEFAULT_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// wry's defaults plus the extra arguments, or `None` when there are none.
#[must_use]
pub fn with_extra(extra: Option<&str>) -> Option<String> {
    let extra = extra.map(str::trim).filter(|e| !e.is_empty())?;
    Some(format!("{WRY_DEFAULT_ARGS} {extra}"))
}

/// Debug Windows builds: give every configured window the extra arguments from [`ENV_VAR`].
pub fn apply_env<R: tauri::Runtime>(context: &mut tauri::Context<R>) {
    if !cfg!(all(windows, debug_assertions)) {
        return;
    }
    let Some(args) = with_extra(std::env::var(ENV_VAR).ok().as_deref()) else {
        return;
    };
    for window in &mut context.config_mut().app.windows {
        window.additional_browser_args = Some(args.clone());
    }
}

/// The arguments the configured windows use, for windows built in code (WebView2 needs them to match).
#[must_use]
pub fn configured(config: &tauri::Config) -> Option<String> {
    config
        .app
        .windows
        .iter()
        .find_map(|w| w.additional_browser_args.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_arguments_extend_the_wry_defaults() {
        assert_eq!(
            with_extra(Some(" --remote-debugging-port=9222 ")).as_deref(),
            Some(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
                 --autoplay-policy=no-user-gesture-required --remote-debugging-port=9222"
            )
        );
    }

    #[test]
    fn no_or_blank_extra_arguments_leave_the_defaults_to_wry() {
        assert_eq!(with_extra(None), None);
        assert_eq!(with_extra(Some("  ")), None);
    }
}
