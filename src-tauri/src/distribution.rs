//! Store builds (user, 2026-11-01): the App Store and Google Play forbid an app pointing its users at
//! updates outside the store. A store build never runs the "new version available" check, never
//! shows its notice, and the UI hides the Help menu's Releases link (`settings.storeBuild`).
//!
//! iOS is always a store build. Android is one when built with `BOREALIS_DISTRIBUTION=play` (read at
//! build time; the sideloaded APK on GitHub is built without it). macOS is one in the Mac App Store
//! flavour (cargo feature `mas`, MAS1); the DMG is not. Windows and Linux never are.

/// The value of the build-time `BOREALIS_DISTRIBUTION` that marks an Android build for Google Play
/// (`option_env!`: cargo rebuilds the crate when the variable changes).
pub const PLAY: &str = "play";

/// This build has the AI CLI integration (AI commands, "Import anything", `--mcp`, the drive CLI and
/// the LAN AI host). Phones and the Mac App Store build do not: build.rs sets `cfg(ai_cli)` for the
/// other desktop builds only. The UI reads it as `settings.aiAvailable`.
pub const HAS_AI_CLI: bool = cfg!(ai_cli);

/// Where this build is distributed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution {
    /// Downloaded from the website or GitHub (desktop, the sideloaded APK): may offer updates.
    Direct,
    /// Installed from an app store: updates come from the store only.
    Store,
}

/// The distribution of a build for `target_os` (`std::env::consts::OS`) whose
/// `BOREALIS_DISTRIBUTION` was `build_value` at build time, built with the `mas` feature when
/// `mac_app_store` (it only counts on macOS).
#[must_use]
pub fn distribution_for(
    target_os: &str,
    build_value: Option<&str>,
    mac_app_store: bool,
) -> Distribution {
    match target_os {
        "ios" => Distribution::Store,
        "android" if build_value.is_some_and(|v| v.trim().eq_ignore_ascii_case(PLAY)) => {
            Distribution::Store
        }
        "macos" if mac_app_store => Distribution::Store,
        _ => Distribution::Direct,
    }
}

/// This build's distribution.
#[must_use]
pub fn current() -> Distribution {
    distribution_for(
        std::env::consts::OS,
        option_env!("BOREALIS_DISTRIBUTION"),
        cfg!(feature = "mas"),
    )
}

/// This build comes from an app store (no update check, no Releases link).
#[must_use]
pub fn is_store_build() -> bool {
    current() == Distribution::Store
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_is_always_a_store_build() {
        assert_eq!(distribution_for("ios", None, false), Distribution::Store);
        assert_eq!(
            distribution_for("ios", Some("github"), false),
            Distribution::Store
        );
    }

    #[test]
    fn android_is_a_store_build_only_with_the_play_switch() {
        assert_eq!(
            distribution_for("android", Some("play"), false),
            Distribution::Store
        );
        assert_eq!(
            distribution_for("android", Some(" Play "), false),
            Distribution::Store
        );
        assert_eq!(
            distribution_for("android", None, false),
            Distribution::Direct
        );
        assert_eq!(
            distribution_for("android", Some(""), false),
            Distribution::Direct
        );
        assert_eq!(
            distribution_for("android", Some("github"), false),
            Distribution::Direct
        );
    }

    #[test]
    fn desktop_ignores_the_play_switch() {
        for os in ["macos", "windows", "linux"] {
            assert_eq!(distribution_for(os, None, false), Distribution::Direct);
            assert_eq!(
                distribution_for(os, Some("play"), false),
                Distribution::Direct
            );
        }
    }

    #[test]
    fn the_mac_app_store_flavour_is_a_store_build_on_macos_only() {
        assert_eq!(distribution_for("macos", None, true), Distribution::Store);
        assert_eq!(
            distribution_for("macos", Some("github"), true),
            Distribution::Store
        );
        for os in ["windows", "linux"] {
            assert_eq!(distribution_for(os, None, true), Distribution::Direct);
        }
    }

    #[test]
    fn this_host_build_is_a_store_build_only_in_the_mac_app_store_flavour() {
        assert_eq!(
            is_store_build(),
            cfg!(all(target_os = "macos", feature = "mas"))
        );
    }

    #[test]
    fn the_ai_cli_is_compiled_into_desktop_builds_without_the_mas_feature() {
        assert_eq!(
            HAS_AI_CLI,
            !cfg!(any(
                target_os = "android",
                target_os = "ios",
                feature = "mas"
            ))
        );
    }
}
