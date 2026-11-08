//! Web pages the app opens in the system browser (decision U15, GitHub #9): the Help menu, the
//! title-bar globe, the update notice and the sections' guide buttons (U18). The `open_link`
//! command opens an address only when it is exactly one of these, so the web view can never make
//! the app open anything else.

use crate::app_update::DOWNLOAD_PAGE;

/// The project website (title-bar globe).
pub const WEBSITE: &str = "https://borealisbuilder.com/";
/// Help > News: the website's changelog.
pub const NEWS: &str = "https://borealisbuilder.com/changelog";
/// Help > Releases: the GitHub Releases page.
pub const RELEASES: &str = "https://github.com/TeamBorealis/borealis/releases";
/// Help > Support Page: the Borealis Discord invite.
pub const SUPPORT: &str = "https://discord.gg/r4K4D3AJpZ";
/// Help > Privacy Policy (B-86): the website's privacy page, on every platform.
pub const PRIVACY: &str = "https://borealisbuilder.com/privacy";

/// The website guides (decision U18, B-234): each section's help button opens the guide that
/// explains it, on every platform including store builds. New Character flyout.
pub const GUIDE_CREATE_CHARACTER: &str = "https://borealisbuilder.com/guides/create-a-character";
/// Guide for the Additional Content page's Aurora content import.
pub const GUIDE_IMPORT_AURORA: &str = "https://borealisbuilder.com/guides/import-aurora-content";
/// Guide for the 5etools-format import.
pub const GUIDE_IMPORT_5ETOOLS: &str = "https://borealisbuilder.com/guides/import-5etools-data";
/// Guide for Send / Receive from device and Export to bundle.
pub const GUIDE_MOVE_CHARACTERS: &str =
    "https://borealisbuilder.com/guides/move-characters-between-devices";
/// Guide for the character sheet's preview and PDF save.
pub const GUIDE_PRINT_SHEET: &str = "https://borealisbuilder.com/guides/print-character-sheet";

/// Every address the app may open, exactly as written (`ui/src/lib/links.ts` lists the same).
pub const ALLOWED: [&str; 11] = [
    WEBSITE,
    NEWS,
    RELEASES,
    SUPPORT,
    PRIVACY,
    DOWNLOAD_PAGE,
    GUIDE_CREATE_CHARACTER,
    GUIDE_IMPORT_AURORA,
    GUIDE_IMPORT_5ETOOLS,
    GUIDE_MOVE_CHARACTERS,
    GUIDE_PRINT_SHEET,
];

/// The allowed address equal to `url`, or `None`. Callers open the returned constant, never the
/// caller's text.
#[must_use]
pub fn allowed(url: &str) -> Option<&'static str> {
    ALLOWED.iter().copied().find(|a| *a == url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTS: [&str; 3] = [
        "https://borealisbuilder.com/",
        "https://github.com/TeamBorealis/",
        "https://discord.gg/",
    ];

    #[test]
    fn every_link_is_https_on_a_project_host() {
        for url in ALLOWED {
            assert!(
                HOSTS.iter().any(|h| url.starts_with(h)),
                "{url} is not on a project host"
            );
            assert!(!url.chars().any(char::is_whitespace), "{url}");
        }
    }

    #[test]
    fn the_privacy_policy_is_allowed() {
        assert_eq!(
            allowed("https://borealisbuilder.com/privacy"),
            Some(PRIVACY)
        );
    }

    #[test]
    fn the_five_guides_are_allowed() {
        for (url, constant) in [
            (
                "https://borealisbuilder.com/guides/create-a-character",
                GUIDE_CREATE_CHARACTER,
            ),
            (
                "https://borealisbuilder.com/guides/import-aurora-content",
                GUIDE_IMPORT_AURORA,
            ),
            (
                "https://borealisbuilder.com/guides/import-5etools-data",
                GUIDE_IMPORT_5ETOOLS,
            ),
            (
                "https://borealisbuilder.com/guides/move-characters-between-devices",
                GUIDE_MOVE_CHARACTERS,
            ),
            (
                "https://borealisbuilder.com/guides/print-character-sheet",
                GUIDE_PRINT_SHEET,
            ),
        ] {
            assert_eq!(allowed(url), Some(constant));
        }
        assert_eq!(allowed("https://borealisbuilder.com/guides/"), None);
        assert_eq!(allowed("https://borealisbuilder.com/guides/other"), None);
    }

    #[test]
    fn only_exact_listed_addresses_are_allowed() {
        for url in ALLOWED {
            assert_eq!(allowed(url), Some(url));
        }
        for bad in [
            "",
            "https://example.com/",
            "https://borealisbuilder.com.evil.example/",
            "https://borealisbuilder.com/changelog/../../x",
            " https://borealisbuilder.com/",
            "https://borealisbuilder.com/ ",
            "HTTPS://BOREALISBUILDER.COM/",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://discord.gg/other",
        ] {
            assert_eq!(allowed(bad), None, "{bad:?} must be refused");
        }
    }

    #[test]
    fn the_ui_names_the_same_links() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/lib/links.ts");
        let ts = std::fs::read_to_string(&path).expect("ui/src/lib/links.ts");
        let mut quoted: Vec<&str> = ts
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|s| s.starts_with("https://"))
            .collect();
        quoted.sort_unstable();
        quoted.dedup();
        let mut ours = ALLOWED.to_vec();
        ours.sort_unstable();
        assert_eq!(quoted, ours);
    }
}
