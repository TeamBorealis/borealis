//! Web pages the app opens in the system browser (decision U15, GitHub #9): the Help menu, the
//! title-bar globe and the update notice. The `open_link` command opens an address only when it is
//! exactly one of these, so the web view can never make the app open anything else.

use crate::app_update::DOWNLOAD_PAGE;

/// The project website (title-bar globe).
pub const WEBSITE: &str = "https://borealisbuilder.com/";
/// Help > News: the website's changelog.
pub const NEWS: &str = "https://borealisbuilder.com/changelog";
/// Help > Releases: the GitHub Releases page.
pub const RELEASES: &str = "https://github.com/TeamBorealis/borealis/releases";
/// Help > Support Page: the Borealis Discord invite.
pub const SUPPORT: &str = "https://discord.gg/r4K4D3AJpZ";

/// Every address the app may open, exactly as written (`ui/src/lib/links.ts` lists the same).
pub const ALLOWED: [&str; 5] = [WEBSITE, NEWS, RELEASES, SUPPORT, DOWNLOAD_PAGE];

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
