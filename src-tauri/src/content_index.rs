//! Update indexes: user-added HTTPS indexes, hot reload. Borealis JSON indexes need a SHA-256 per
//! entry and may be ed25519-signed (trust on first use); Aurora XML indexes are handled in
//! [`crate::content_index_aurora`] (nested, unhashed, never "verified").
//! Nothing is fetched until the user adds an index and asks for an update; no default URLs exist.

use crate::content_auto::update_message;
use crate::content_dto::{ContentIndex, ImportPackRequest, MigrationReport, UpdateIndexesResult};
use crate::content_fetch_retry::{
    check_complete, parse_retry_after, FetchError, RetryPolicy, Retrying, Sleep, CODE_OTHER,
    CODE_REFUSED,
};
use crate::content_import::{slug, stem, write_pack_meta, Scratch};
use crate::content_index_aurora::{
    looks_like_xml, summarize, AuroraOutcome, MAX_FILE_BYTES, MAX_INDEX_BYTES,
};
use crate::content_packs::{progress, EventSink, PACK_META_FILE};
use crate::content_prefs::IndexRecord;
use crate::error::{ApiError, ApiResult};
use crate::ids::rfc3339;
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use borealis_content::IndexFile;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::SystemTime;

const MAX_REDIRECTS: usize = 5;
const FETCH_TIMEOUT_SECS: u64 = 30;
const SHA256_ATTRIBUTE: &str = "sha256";
const MAX_SIGNATURE_BYTES: usize = 4 * 1024;
const SIGNATURE_SUFFIX: &str = ".sig";
const RAW_GITHUB: &str = "https://raw.githubusercontent.com/";
pub(crate) const USER_AGENT: &str = concat!("Borealis/", env!("CARGO_PKG_VERSION"));

/// Detached signature document served at `<index url>.sig`: base64 key and signature over the
/// exact index bytes.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignatureFile {
    public_key: String,
    signature: String,
}

/// Verify `sig_bytes` against `index_bytes`; returns the base64 public key on success.
fn verify_signature(index_bytes: &[u8], sig_bytes: &[u8]) -> Result<String, String> {
    let file: SignatureFile =
        serde_json::from_slice(sig_bytes).map_err(|_| "malformed signature file".to_string())?;
    let key: [u8; 32] = STANDARD
        .decode(file.public_key.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("bad public key")?;
    let sig: [u8; 64] = STANDARD
        .decode(file.signature.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("bad signature")?;
    VerifyingKey::from_bytes(&key)
        .and_then(|k| k.verify(index_bytes, &Signature::from_bytes(&sig)))
        .map_err(|_| "signature does not match the index".to_string())?;
    Ok(STANDARD.encode(key))
}

/// Trust-on-first-use decision. `Ok(Some(key))` = verified with that key, `Ok(None)` = unsigned
/// index from a never-signed source.
fn check_trust(
    pinned: Option<&str>,
    index_bytes: &[u8],
    sig: Option<Vec<u8>>,
) -> Result<Option<String>, String> {
    match (pinned, sig) {
        (None, None) => Ok(None),
        (Some(_), None) => Err("the index is no longer signed".into()),
        (pinned, Some(sig_bytes)) => {
            let key = verify_signature(index_bytes, &sig_bytes)?;
            if pinned.is_some_and(|p| p != key) {
                return Err("the index signing key changed".into());
            }
            Ok(Some(key))
        }
    }
}

/// Downloads bytes over HTTPS (replaceable in tests).
pub trait Fetcher: Send + Sync {
    /// Fetch `url`, failing when the body exceeds `max_bytes`.
    ///
    /// # Errors
    /// A user-presentable message.
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String>;

    /// [`Fetcher::get`] with a classified failure (code, `Retry-After`). The default classifies
    /// the message; [`HttpsFetcher`] answers from the response itself.
    ///
    /// # Errors
    /// The classified reason.
    fn get_classified(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
        self.get(url, max_bytes)
            .map_err(|m| FetchError::from_message(&m))
    }

    /// [`Fetcher::get_classified`] as a conditional request: with the `etag` stored from an earlier
    /// download the server may answer "not modified". The default never does and reports no ETag,
    /// so a fetcher without conditional requests simply downloads everything.
    ///
    /// # Errors
    /// The classified reason.
    fn get_tagged(
        &self,
        url: &str,
        max_bytes: usize,
        etag: Option<&str>,
    ) -> Result<Tagged, FetchError> {
        let _ = etag;
        self.get_classified(url, max_bytes)
            .map(|bytes| Tagged::Body { bytes, etag: None })
    }
}

/// The answer to a conditional download ([`Fetcher::get_tagged`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tagged {
    /// The stored copy is still current (HTTP 304).
    NotModified,
    /// The file, with the ETag the server sent for it.
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

/// Reject anything but an `https://` URL with a host.
///
/// # Errors
/// `invalid_argument`.
pub fn require_https(url: &str) -> ApiResult<()> {
    let ok = url
        .strip_prefix("https://")
        .is_some_and(|rest| rest.split('/').next().is_some_and(|h| !h.is_empty()))
        && !url.chars().any(char::is_whitespace);
    if ok {
        Ok(())
    } else {
        Err(ApiError::invalid(
            "Index and file addresses must start with https://",
        ))
    }
}

/// The address that serves a file's raw bytes. A GitHub page link
/// (`https://github.com/<owner>/<repo>/blob/<branch>/<path>`, or `.../raw/...`) is what a browser
/// shows; it answers with an HTML page, so it is turned into its `raw.githubusercontent.com`
/// address. Every other address is returned unchanged (trimmed).
#[must_use]
pub fn raw_file_url(url: &str) -> String {
    let url = url.trim();
    let Some(rest) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("https://www.github.com/"))
    else {
        return url.to_string();
    };
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    let parts: Vec<&str> = path.splitn(4, '/').collect();
    match parts.as_slice() {
        [owner, repo, "blob" | "raw", tail] if !owner.is_empty() && !repo.is_empty() => {
            format!("{RAW_GITHUB}{owner}/{repo}/{tail}")
        }
        _ => url.to_string(),
    }
}

/// True when `bytes` are an HTML web page (what a link to a page, not a file, returns).
#[must_use]
pub fn looks_like_html(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let start = text
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(text.len());
    let head: Vec<u8> = text[start..]
        .iter()
        .take(16)
        .map(u8::to_ascii_lowercase)
        .collect();
    head.starts_with(b"<!doctype html") || head.starts_with(b"<html")
}

/// Shown when an index address answers with a web page instead of the index file.
pub const WEB_PAGE_NOT_INDEX: &str = "this address is a web page, not an index file; \
     use the address of the file itself (on GitHub: open the file and use its Raw button)";

/// Why a redirect must not be followed: anything but HTTPS, or too many hops.
#[must_use]
pub fn redirect_refusal(scheme: &str, hops_so_far: usize) -> Option<String> {
    if !scheme.eq_ignore_ascii_case("https") {
        Some(format!(
            "refused a redirect to a {scheme}: address (only https:// is allowed)"
        ))
    } else if hops_so_far >= MAX_REDIRECTS {
        Some("too many redirects".into())
    } else {
        None
    }
}

/// An error and its causes on one line (reqwest hides the redirect refusal in the source chain).
pub(crate) fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut cause = e.source();
    while let Some(c) = cause {
        text.push_str(": ");
        text.push_str(&c.to_string());
        cause = c.source();
    }
    text
}

/// The download client every content fetch uses: https only (redirects included, at most
/// [`MAX_REDIRECTS`] hops) and a timeout, with the platform's TLS setup.
///
/// # Errors
/// The reason the client could not be built.
pub(crate) fn https_client() -> Result<reqwest::blocking::Client, String> {
    let policy = reqwest::redirect::Policy::custom(|a| {
        match redirect_refusal(a.url().scheme(), a.previous().len()) {
            None => a.follow(),
            Some(why) => a.error(why),
        }
    });
    crate::content_fetch_tls::client_builder()?
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
        .redirect(policy)
        .build()
        .map_err(|e| error_chain(&e))
}

/// The download client shared by every content fetch of the process (one connection pool, so a
/// few hundred files of one index reuse their connections instead of opening one each).
///
/// # Errors
/// The reason the client could not be built (not cached; the next call tries again).
pub(crate) fn shared_client() -> Result<reqwest::blocking::Client, String> {
    static CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let built = https_client()?;
    Ok(CLIENT.get_or_init(|| built).clone())
}

/// A transport failure, classified (redirect refusals stay permanent and keep their wording).
pub(crate) fn send_error(e: &reqwest::Error) -> FetchError {
    let text = error_chain(e);
    if e.is_redirect() || text.contains("redirect") {
        FetchError::new(CODE_REFUSED, text)
    } else if e.is_timeout() {
        FetchError::timeout(&text)
    } else {
        FetchError::network(&text)
    }
}

/// The real network client. Desktop verifies TLS with the OS trust store; phones use the Mozilla
/// root set compiled into the app ([`crate::content_fetch_tls`]). The rules are the same on every
/// platform: https only (redirects included), a timeout, a size cap, and a body that must be as
/// long as announced. Retries are not done here but by [`Retrying`] around it.
#[derive(Debug, Clone, Copy, Default)]
pub struct HttpsFetcher;

impl Fetcher for HttpsFetcher {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        self.get_classified(url, max_bytes).map_err(|e| e.wire())
    }

    fn get_classified(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
        match self.get_tagged(url, max_bytes, None)? {
            Tagged::Body { bytes, .. } => Ok(bytes),
            Tagged::NotModified => Err(FetchError::new(CODE_OTHER, "the server answered 304")),
        }
    }

    fn get_tagged(
        &self,
        url: &str,
        max_bytes: usize,
        etag: Option<&str>,
    ) -> Result<Tagged, FetchError> {
        use std::io::Read;
        require_https(url).map_err(|e| FetchError::new(CODE_REFUSED, e.message))?;
        let client = shared_client().map_err(|m| FetchError::new(CODE_OTHER, m))?;
        let mut call = client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT);
        if let Some(tag) = etag {
            call = call.header(reqwest::header::IF_NONE_MATCH, tag);
        }
        let response = call.send().map_err(|e| send_error(&e))?;
        let status = response.status();
        if etag.is_some() && status.as_u16() == 304 {
            return Ok(Tagged::NotModified);
        }
        if !status.is_success() {
            let header = |name| {
                response
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            };
            let retry_after = header(reqwest::header::RETRY_AFTER.as_str())
                .as_deref()
                .and_then(parse_retry_after);
            if status.as_u16() == 403 && header("x-ratelimit-remaining").as_deref() == Some("0") {
                return Err(FetchError::rate_limited(retry_after));
            }
            let reason = status.canonical_reason().unwrap_or_default();
            return Err(FetchError::http(status.as_u16(), reason, retry_after));
        }
        let new_tag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let announced = response.content_length();
        let mut body = Vec::new();
        response
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| FetchError::truncated(&e.to_string()))?;
        if body.len() > max_bytes {
            return Err(FetchError::too_large());
        }
        check_complete(body.len(), announced)?;
        Ok(Tagged::Body {
            bytes: body,
            etag: new_tag,
        })
    }
}

/// Lower-case hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn version_parts(v: &str) -> Vec<u32> {
    v.split('.')
        .map(|p| p.trim().parse().unwrap_or(0))
        .collect()
}

pub(crate) fn newer_than_app(min: &str) -> bool {
    version_parts(min) > version_parts(env!("CARGO_PKG_VERSION"))
}

fn dto(r: &IndexRecord) -> ContentIndex {
    ContentIndex {
        url: r.url.clone(),
        name: r.name.clone(),
        last_checked: r.last_checked.clone(),
        file_count: r.file_count,
        updates_available: r.updates_available,
        requires_newer_app: r.requires_newer_app,
        verified: r.verified,
        problem: r.problem.clone(),
    }
}

/// Stored index addresses in their raw-file form (records added before GitHub page links were
/// converted), without duplicates.
fn normalize_index_urls(indexes: &mut Vec<IndexRecord>) {
    let mut seen = std::collections::BTreeSet::new();
    let normalized: Vec<IndexRecord> = indexes
        .drain(..)
        .map(|r| {
            let url = raw_file_url(&r.url);
            if url == r.url {
                r
            } else {
                let name = if r.name == r.url { url.clone() } else { r.name };
                IndexRecord { url, name, ..r }
            }
        })
        .filter(|r| seen.insert(r.url.clone()))
        .collect();
    *indexes = normalized;
}

fn installed_hash(state: &AppState, pack_id: &str) -> Option<String> {
    let text =
        std::fs::read_to_string(state.paths.library().join(pack_id).join(PACK_META_FILE)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["sha256"].as_str().map(str::to_string)
}

impl AppState {
    /// The user's indexes.
    #[must_use]
    pub fn list_indexes(&self) -> Vec<ContentIndex> {
        self.content_prefs().indexes.iter().map(dto).collect()
    }

    /// Remember an index URL (no network access).
    ///
    /// # Errors
    /// `invalid_argument` unless the URL is HTTPS.
    pub fn add_index(&self, url: &str) -> ApiResult<Vec<ContentIndex>> {
        let url = raw_file_url(url);
        let url = url.as_str();
        require_https(url)?;
        let _guard = self.content_lock()?;
        let mut prefs = self.content_prefs();
        if !prefs.indexes.iter().any(|i| i.url == url) {
            prefs.indexes.push(IndexRecord {
                url: url.to_string(),
                name: url.to_string(),
                ..IndexRecord::default()
            });
            prefs.save(&self.paths.prefs_file())?;
        }
        Ok(prefs.indexes.iter().map(dto).collect())
    }

    /// Forget an index (installed packs stay).
    ///
    /// # Errors
    /// `io_error`.
    pub fn remove_index(&self, url: &str) -> ApiResult<Vec<ContentIndex>> {
        let _guard = self.content_lock()?;
        let mut prefs = self.content_prefs();
        prefs.indexes.retain(|i| i.url != url.trim());
        let cache = self
            .paths
            .data
            .join(crate::content_index_aurora::CACHE_DIR)
            .join(crate::content_index_aurora::pack_id_for(url.trim()));
        // Only a saving is lost when the local copies stay behind.
        let _ = std::fs::remove_dir_all(cache);
        prefs.save(&self.paths.prefs_file())?;
        Ok(prefs.indexes.iter().map(dto).collect())
    }

    /// Fetch every index, download changed files (SHA-256 verified; the index is refused if any entry lacks one),
    /// install them and hot-reload. Downloads that fail for a passing reason are retried
    /// ([`RetryPolicy::default`]).
    ///
    /// # Errors
    /// `io_error` / `content_error`; per-file problems are reported in `failed`.
    pub fn update_indexes(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
    ) -> ApiResult<UpdateIndexesResult> {
        self.update_indexes_with(fetcher, sink, RetryPolicy::default(), &std::thread::sleep)
    }

    /// [`AppState::update_indexes`] with an explicit retry policy and pause function (tests).
    ///
    /// # Errors
    /// As [`AppState::update_indexes`].
    pub fn update_indexes_with(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
        policy: RetryPolicy,
        sleep: Sleep<'_>,
    ) -> ApiResult<UpdateIndexesResult> {
        let retrying = Retrying::new(fetcher, policy, sleep);
        let fetcher: &dyn Fetcher = &retrying;
        let _guard = self.content_lock()?;
        let mut prefs = self.content_prefs();
        normalize_index_urls(&mut prefs.indexes);
        let total = prefs.indexes.len();
        let mut outcome = AuroraOutcome::default();
        let mut staged: Vec<(PathBuf, String)> = Vec::new();
        let mut scratches = Vec::new();
        tracing::info!(indexes = total, "content index update started");
        for n in 0..total {
            let previous = prefs.indexes[n].clone();
            let staged_before = staged.len();
            sink.content_progress(&progress("checking", n, total, &previous.url));
            let record = self.check_index(
                fetcher,
                sink,
                &previous,
                &mut staged,
                &mut scratches,
                &mut outcome,
            );
            match record {
                Ok(r) => {
                    tracing::info!(
                        url = %previous.url,
                        updated = staged.len() - staged_before,
                        problem = r.problem.as_deref().unwrap_or("none"),
                        "content index checked"
                    );
                    prefs.indexes[n] = r;
                }
                Err(why) => {
                    tracing::warn!("index {}: {why}", previous.url);
                    prefs.indexes[n].problem = Some(why);
                    outcome.failed.push(previous.url);
                }
            }
        }
        prefs.save(&self.paths.prefs_file())?;
        let updated = staged.len();
        if updated > 0 {
            sink.content_progress(&progress(
                "reloading",
                updated,
                updated,
                "Reloading content",
            ));
            self.install_packs(&staged, sink, "indexes-updated")?;
        }
        let message = update_message(updated, outcome.failed.len());
        tracing::info!(
            updated,
            failed = outcome.failed.len(),
            "content index update finished"
        );
        sink.content_progress(&progress("done", total, total, &message));
        Ok(UpdateIndexesResult {
            updated,
            failed: outcome.failed,
            reloaded: updated > 0,
            changed: outcome.changed,
            new_hosts: outcome.new_hosts,
            reports: outcome.reports,
        })
    }

    /// Fetch one index and stage what changed; `Err` = the index itself is unusable (reason).
    fn check_index(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
        previous: &IndexRecord,
        staged: &mut Vec<(PathBuf, String)>,
        scratches: &mut Vec<Scratch>,
        outcome: &mut AuroraOutcome,
    ) -> Result<IndexRecord, String> {
        let url = previous.url.as_str();
        require_https(url).map_err(|e| e.message)?;
        let bytes = fetcher.get(url, MAX_INDEX_BYTES)?;
        if looks_like_html(&bytes) {
            return Err(WEB_PAGE_NOT_INDEX.into());
        }
        if looks_like_xml(&bytes) {
            return self.check_aurora_index(
                fetcher, sink, url, &bytes, previous, staged, scratches, outcome,
            );
        }
        let (mut record, failed) = self.check_json_index(
            fetcher,
            sink,
            url,
            &bytes,
            previous,
            staged,
            scratches,
            &mut outcome.reports,
        )?;
        if let Some(summary) = summarize(&failed) {
            record.problem = Some(format!(
                "{} of {} file(s) were refused or could not be downloaded: {summary}",
                failed.len(),
                record.file_count
            ));
        }
        outcome.failed.extend(failed.into_iter().map(|(u, _)| u));
        Ok(record)
    }

    /// The signed / hashed Borealis JSON index (unchanged behaviour).
    #[allow(clippy::too_many_arguments)]
    fn check_json_index(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
        url: &str,
        bytes: &[u8],
        previous: &IndexRecord,
        staged: &mut Vec<(PathBuf, String)>,
        scratches: &mut Vec<Scratch>,
        reports: &mut Vec<MigrationReport>,
    ) -> Result<(IndexRecord, Vec<(String, String)>), String> {
        let mut failed = Vec::new();
        let sig = match fetcher
            .get_classified(&format!("{url}{SIGNATURE_SUFFIX}"), MAX_SIGNATURE_BYTES)
        {
            Ok(sig) => Some(sig),
            // A signature that could not be read now must not look like "no longer signed".
            Err(e) if e.is_transient() => {
                return Err(format!(
                    "the signature could not be downloaded: {}",
                    e.reason()
                ))
            }
            Err(_) => None,
        };
        let public_key = check_trust(previous.public_key.as_deref(), bytes, sig)?;
        let text = std::str::from_utf8(bytes).map_err(|_| "the index is not UTF-8 text")?;
        let index = IndexFile::from_json_str(text)
            .map_err(|e| format!("the index could not be read: {e}"))?;
        let name = index.info.name.clone().unwrap_or_else(|| url.to_string());
        let mut record = IndexRecord {
            url: url.to_string(),
            name: name.clone(),
            last_checked: Some(rfc3339(SystemTime::now())),
            requires_newer_app: index.min_app_version.as_deref().is_some_and(newer_than_app),
            verified: public_key.is_some(),
            public_key,
            ..IndexRecord::default()
        };
        let entries: Vec<_> = index
            .files
            .iter()
            .filter(|f| !f.is_obsolete() && !f.is_nested_index())
            .collect();
        record.file_count = entries.len();
        if record.requires_newer_app {
            record.updates_available = entries.len();
            return Ok((record, failed));
        }
        for (i, entry) in entries.iter().enumerate() {
            let pack_id = format!("dl-{}-{}", slug(&name), slug(stem(&entry.name)));
            let Some(expected) = entry
                .extra
                .attributes
                .get(SHA256_ATTRIBUTE)
                .map(|h| h.to_lowercase())
                .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            else {
                tracing::warn!("rejected {}: the index gives no valid sha256", entry.url);
                failed.push((entry.url.clone(), "the index gives no valid sha256".into()));
                record.updates_available += 1;
                continue;
            };
            let installed = installed_hash(self, &pack_id);
            if installed.as_deref() == Some(expected.as_str()) {
                continue;
            }
            sink.content_progress(&progress("downloading", i, entries.len(), &entry.name));
            let outcome = self.download_entry(
                fetcher,
                entry.url.as_str(),
                &entry.name,
                &pack_id,
                &expected,
                installed.as_deref(),
                url,
            );
            match outcome {
                Ok(Some((done, report))) => {
                    staged.push((done.0.join("out"), pack_id));
                    scratches.push(done);
                    reports.push(report);
                }
                Ok(None) => {}
                Err(why) => {
                    tracing::warn!(url = %entry.url, "not updated: {why}");
                    failed.push((entry.url.clone(), why));
                    record.updates_available += 1;
                }
            }
        }
        Ok((record, failed))
    }

    /// `Ok(None)` = already current, `Err` = why it was rejected (bad URL, download failure with
    /// its code, hash mismatch, bad content).
    #[allow(clippy::too_many_arguments)]
    fn download_entry(
        &self,
        fetcher: &dyn Fetcher,
        file_url: &str,
        file_name: &str,
        pack_id: &str,
        expected: &str,
        installed: Option<&str>,
        index_url: &str,
    ) -> Result<Option<(Scratch, MigrationReport)>, String> {
        require_https(file_url).map_err(|e| e.message)?;
        let bytes = fetcher
            .get_classified(file_url, MAX_FILE_BYTES)
            .map_err(|e| e.reason())?;
        let actual = sha256_hex(&bytes);
        if expected != actual {
            tracing::warn!("rejected {file_url}: SHA-256 mismatch");
            return Err("the SHA-256 does not match the index".into());
        }
        if installed == Some(actual.as_str()) {
            return Ok(None);
        }
        let scratch = Scratch::new(&self.paths.data).map_err(|e| e.message)?;
        let out_dir = scratch.0.join("out");
        let request = ImportPackRequest {
            file_name: file_name.to_string(),
            content_base64: String::new(),
            format: None,
        };
        let xml = file_name.to_lowercase().ends_with(".xml");
        let report = if xml {
            Self::convert_xml(&scratch.0, &out_dir, &request, pack_id, &bytes)
        } else {
            Self::convert_json(&out_dir, &request, pack_id, &bytes)
        }
        .map_err(|e| e.message)?;
        if report.converted == 0 {
            return Err("no elements could be converted from this file".into());
        }
        write_pack_meta(
            &out_dir,
            "downloaded",
            &serde_json::json!({ "sha256": actual, "indexUrl": index_url, "fileUrl": file_url }),
        );
        Ok(Some((scratch, report)))
    }
}
