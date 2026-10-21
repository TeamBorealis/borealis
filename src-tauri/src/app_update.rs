//! "New version available" notice: asks GitHub for the latest Borealis release and reports whether
//! it is newer than this build. Sends no user data (one GET, a `User-Agent` and nothing else), never
//! blocks startup, and stays silent when offline or rate limited. There is no auto-updater: the
//! notice only points at the download page. Turned off by `checkAppUpdates`.
//!
//! Desktop runs the request here. Mobile has no HTTPS client in the backend, so there the command
//! answers `unsupported` and the UI makes the same request with `fetch()` (see `ui/src/state/appUpdate.ts`).

use crate::content_index::Fetcher;
use crate::error::ApiResult;
use crate::ids::rfc3339;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// Latest published (non-draft, non-prerelease) release of the project.
pub const RELEASES_API: &str = "https://api.github.com/repos/alon-mini/borealis/releases/latest";
/// The official download page (the website lists every platform).
pub const DOWNLOAD_PAGE: &str = "https://borealisbuilder.com/#download";
/// Release pages must live under this prefix to be offered as a link.
const RELEASE_PAGE_PREFIX: &str = "https://github.com/alon-mini/borealis/";
/// The release JSON is a few kilobytes to a few dozen; anything bigger is not what we asked for.
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REDIRECTS: usize = 3;
/// Release notes longer than this (in characters) are cut, so a huge body never reaches the UI.
pub const MAX_NOTES_CHARS: usize = 4000;
/// Wait after startup before asking, so launch is never slowed.
pub const STARTUP_CHECK_DELAY: Duration = Duration::from_secs(12);
/// This build's version.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the check stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppUpdateState {
    /// Not asked yet.
    Idle,
    /// Setting is off: nothing was requested.
    Disabled,
    /// GitHub answered (see `newer`).
    Done,
    /// Offline, rate limited or an unusable answer; shown only when the user clicks "Check now".
    Failed,
    /// This build cannot make the request; the UI does it with `fetch()`.
    Unsupported,
}

/// `app_update_status` / `check_app_update` result; also sent on `borealis://app-update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateStatus {
    pub state: AppUpdateState,
    /// This build's version.
    pub current: String,
    /// Latest release version without a leading `v`.
    pub latest: Option<String>,
    /// `latest` is newer than `current`.
    pub newer: bool,
    /// Where to download: the website download page.
    pub url: Option<String>,
    /// The GitHub release page, when GitHub gave a usable one.
    pub release_url: Option<String>,
    /// The release's notes (GitHub's markdown `body`), trimmed and capped; `None` when empty or absent.
    pub notes: Option<String>,
    /// RFC 3339 time of the last answer.
    pub checked_at: Option<String>,
}

impl AppUpdateStatus {
    fn bare(state: AppUpdateState) -> Self {
        Self {
            state,
            current: CURRENT_VERSION.to_string(),
            latest: None,
            newer: false,
            url: None,
            release_url: None,
            notes: None,
            checked_at: None,
        }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `X.Y.Z` (an optional leading `v`, missing parts count as 0). `None` for a pre-release
/// (`1.2.0-rc1`) or anything that is not numeric.
#[must_use]
pub fn parse_version(raw: &str) -> Option<[u64; 3]> {
    let s = raw.trim();
    let s = s
        .strip_prefix('v')
        .or_else(|| s.strip_prefix('V'))
        .unwrap_or(s);
    let s = s.split('+').next().unwrap_or(s);
    if s.contains('-') || s.is_empty() {
        return None;
    }
    let mut out = [0u64; 3];
    let mut parts = s.split('.');
    for slot in &mut out {
        if let Some(p) = parts.next() {
            *slot = p.parse().ok()?;
        }
    }
    parts.next().is_none().then_some(out)
}

/// Whether `latest` is a release newer than `current`.
#[must_use]
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// The release body trimmed and cut to `MAX_NOTES_CHARS` characters (an ellipsis marks a cut);
/// `None` when it is absent or blank.
#[must_use]
pub fn clean_notes(body: Option<&str>) -> Option<String> {
    let text = body?.replace("\r\n", "\n");
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_NOTES_CHARS {
        return Some(text.to_string());
    }
    let cut: String = text.chars().take(MAX_NOTES_CHARS).collect();
    Some(format!("{}...", cut.trim_end()))
}

/// Turn GitHub's answer into a status (`Done`), or say why it cannot be used.
///
/// # Errors
/// A short reason for the debug log.
pub fn evaluate(body: &[u8], current: &str, now: &str) -> Result<AppUpdateStatus, String> {
    let rel: Release =
        serde_json::from_slice(body).map_err(|e| format!("unreadable answer: {e}"))?;
    if rel.draft || rel.prerelease {
        return Err("latest release is a draft or pre-release".into());
    }
    let latest = parse_version(&rel.tag_name).ok_or_else(|| "unusable release tag".to_string())?;
    let release_url = rel
        .html_url
        .filter(|u| u.starts_with(RELEASE_PAGE_PREFIX) && !u.chars().any(char::is_whitespace));
    Ok(AppUpdateStatus {
        state: AppUpdateState::Done,
        current: current.to_string(),
        latest: Some(format!("{}.{}.{}", latest[0], latest[1], latest[2])),
        newer: is_newer(&rel.tag_name, current),
        url: Some(DOWNLOAD_PAGE.to_string()),
        release_url,
        notes: clean_notes(rel.body.as_deref()),
        checked_at: Some(now.to_string()),
    })
}

/// Ask GitHub (one request). Offline, rate limited or unusable answers give `Failed`, logged at
/// debug level only.
pub fn check_now(fetcher: &dyn Fetcher, now: SystemTime) -> AppUpdateStatus {
    let stamp = rfc3339(now);
    let result = fetcher
        .get(RELEASES_API, MAX_RESPONSE_BYTES)
        .and_then(|body| evaluate(&body, CURRENT_VERSION, &stamp));
    result.unwrap_or_else(|why| {
        tracing::debug!("app update check skipped: {why}");
        AppUpdateStatus {
            checked_at: Some(stamp),
            ..AppUpdateStatus::bare(AppUpdateState::Failed)
        }
    })
}

/// The startup check: makes no request at all when the setting is off.
pub fn check_if_enabled(enabled: bool, fetcher: &dyn Fetcher, now: SystemTime) -> AppUpdateStatus {
    if enabled {
        check_now(fetcher, now)
    } else {
        AppUpdateStatus::bare(AppUpdateState::Disabled)
    }
}

/// The last outcome, shared by the startup task and the commands.
#[derive(Default)]
pub struct AppUpdateCell(Mutex<Option<AppUpdateStatus>>);

impl AppUpdateCell {
    /// The stored status (`Idle` before any check).
    #[must_use]
    pub fn get(&self) -> AppUpdateStatus {
        self.0
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| AppUpdateStatus::bare(AppUpdateState::Idle))
    }

    /// Replace the stored status and return it.
    pub fn set(&self, status: AppUpdateStatus) -> AppUpdateStatus {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(status.clone());
        }
        status
    }
}

/// The process-wide cell.
pub fn cell() -> &'static AppUpdateCell {
    static CELL: OnceLock<AppUpdateCell> = OnceLock::new();
    CELL.get_or_init(AppUpdateCell::default)
}

/// Status the commands return on a build that cannot make the request.
#[must_use]
pub fn unsupported() -> AppUpdateStatus {
    AppUpdateStatus::bare(AppUpdateState::Unsupported)
}

/// Pure helper for the commands: the stored status, or `Unsupported` on mobile.
///
/// # Errors
/// Never today; kept as `ApiResult` for the command layer.
pub fn current_status() -> ApiResult<AppUpdateStatus> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(unsupported())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Ok(cell().get())
    }
}

/// GitHub fetcher: HTTPS only, a short timeout, a size cap, `User-Agent: Borealis/<version>` and
/// no other headers, cookies or credentials.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct GithubFetcher;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Fetcher for GithubFetcher {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        use std::io::Read;
        crate::content_index::require_https(url).map_err(|e| e.message)?;
        let policy = reqwest::redirect::Policy::custom(|a| {
            if a.url().scheme() != "https" {
                a.error("redirect to a non-https address")
            } else if a.previous().len() >= MAX_REDIRECTS {
                a.error("too many redirects")
            } else {
                a.follow()
            }
        });
        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("Borealis/{CURRENT_VERSION}"))
            .timeout(REQUEST_TIMEOUT)
            .redirect(policy)
            .build()
            .map_err(|e| e.to_string())?;
        let response = client.get(url).send().map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("the server answered {}", response.status()));
        }
        let mut body = Vec::new();
        response
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        if body.len() > max_bytes {
            return Err("answer too large".into());
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        calls: AtomicUsize,
        answer: Result<Vec<u8>, String>,
    }
    impl Fake {
        fn new(answer: Result<Vec<u8>, String>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                answer,
            }
        }
    }
    impl Fetcher for Fake {
        fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, String> {
            assert_eq!(url, RELEASES_API);
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answer.clone()
        }
    }

    fn release(tag: &str, draft: bool, pre: bool) -> Vec<u8> {
        format!(
            r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{pre},"html_url":"https://github.com/alon-mini/borealis/releases/tag/{tag}","assets":[]}}"#
        )
        .into_bytes()
    }

    #[test]
    fn versions_compare_numerically_with_or_without_v() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(is_newer("2", "1.9.9"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
    }

    #[test]
    fn prerelease_and_garbage_tags_are_ignored() {
        assert_eq!(parse_version("v1.2.0-rc1"), None);
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("v1.2.3+build5"), Some([1, 2, 3]));
        assert!(!is_newer("v9.0.0-beta.1", "1.0.0"));
    }

    #[test]
    fn newer_release_is_reported_with_the_download_page() {
        let f = Fake::new(Ok(release("v9.0.0", false, false)));
        let s = check_now(&f, SystemTime::UNIX_EPOCH);
        assert_eq!(s.state, AppUpdateState::Done);
        assert!(s.newer);
        assert_eq!(s.latest.as_deref(), Some("9.0.0"));
        assert_eq!(s.url.as_deref(), Some(DOWNLOAD_PAGE));
        assert!(s.release_url.unwrap().ends_with("/releases/tag/v9.0.0"));
    }

    #[test]
    fn same_version_is_not_newer() {
        let tag = format!("v{CURRENT_VERSION}");
        let s = check_now(
            &Fake::new(Ok(release(&tag, false, false))),
            SystemTime::UNIX_EPOCH,
        );
        assert_eq!(s.state, AppUpdateState::Done);
        assert!(!s.newer);
    }

    #[test]
    fn drafts_and_prereleases_are_ignored() {
        for (d, p) in [(true, false), (false, true)] {
            let s = check_now(
                &Fake::new(Ok(release("v9.0.0", d, p))),
                SystemTime::UNIX_EPOCH,
            );
            assert_eq!(s.state, AppUpdateState::Failed);
            assert!(!s.newer);
        }
    }

    #[test]
    fn foreign_release_links_are_dropped() {
        let body = br#"{"tag_name":"v9.0.0","html_url":"https://evil.example/x"}"#.to_vec();
        let s = evaluate(&body, "1.0.0", "t").unwrap();
        assert_eq!(s.release_url, None);
        assert_eq!(s.url.as_deref(), Some(DOWNLOAD_PAGE));
    }

    #[test]
    fn release_notes_are_carried_trimmed_and_capped() {
        let with = |body: &str| {
            let json = serde_json::json!({"tag_name": "v9.0.0", "body": body}).to_string();
            evaluate(json.as_bytes(), "1.0.0", "t").unwrap().notes
        };
        assert_eq!(
            with("  ## New\r\n- a fix  \n").as_deref(),
            Some("## New\n- a fix")
        );
        assert_eq!(with(""), None);
        assert_eq!(with(" \n\t "), None);
        let long = with(&"\u{e9}".repeat(MAX_NOTES_CHARS + 50)).unwrap();
        assert_eq!(long.chars().count(), MAX_NOTES_CHARS + 3);
        assert!(long.ends_with("..."));
        let exact = "x".repeat(MAX_NOTES_CHARS);
        assert_eq!(with(&exact).as_deref(), Some(exact.as_str()));
        let absent = br#"{"tag_name":"v9.0.0"}"#;
        assert_eq!(evaluate(absent, "1.0.0", "t").unwrap().notes, None);
        let null = br#"{"tag_name":"v9.0.0","body":null}"#;
        assert_eq!(evaluate(null, "1.0.0", "t").unwrap().notes, None);
    }

    #[test]
    fn setting_off_makes_no_request() {
        let f = Fake::new(Ok(release("v9.0.0", false, false)));
        let s = check_if_enabled(false, &f, SystemTime::UNIX_EPOCH);
        assert_eq!(s.state, AppUpdateState::Disabled);
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
        let s = check_if_enabled(true, &f, SystemTime::UNIX_EPOCH);
        assert_eq!(s.state, AppUpdateState::Done);
        assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn offline_or_rate_limited_is_a_quiet_failure() {
        for err in ["dns error", "the server answered 403 Forbidden"] {
            let s = check_now(&Fake::new(Err(err.into())), SystemTime::UNIX_EPOCH);
            assert_eq!(s.state, AppUpdateState::Failed);
            assert!(!s.newer && s.latest.is_none() && s.url.is_none());
        }
        let s = check_now(&Fake::new(Ok(b"<html>".to_vec())), SystemTime::UNIX_EPOCH);
        assert_eq!(s.state, AppUpdateState::Failed);
    }

    #[test]
    fn cell_starts_idle_and_keeps_the_last_status() {
        let c = AppUpdateCell::default();
        assert_eq!(c.get().state, AppUpdateState::Idle);
        c.set(AppUpdateStatus::bare(AppUpdateState::Disabled));
        assert_eq!(c.get().state, AppUpdateState::Disabled);
    }
}
