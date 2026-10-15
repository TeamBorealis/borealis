//! Update indexes: user-added HTTPS indexes, hot reload. Borealis JSON indexes need a SHA-256 per
//! entry and may be ed25519-signed (trust on first use); Aurora XML indexes are handled in
//! [`crate::content_index_aurora`] (nested, unhashed, never "verified").
//! Nothing is fetched until the user adds an index and asks for an update; no default URLs exist.

use crate::content_auto::update_message;
use crate::content_dto::{ContentIndex, ImportPackRequest, UpdateIndexesResult};
use crate::content_import::{slug, stem, write_pack_meta, Scratch};
use crate::content_index_aurora::{looks_like_xml, AuroraOutcome, MAX_FILE_BYTES, MAX_INDEX_BYTES};
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
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut cause = e.source();
    while let Some(c) = cause {
        text.push_str(": ");
        text.push_str(&c.to_string());
        cause = c.source();
    }
    text
}

/// The real network client (desktop only).
#[derive(Debug, Clone, Copy, Default)]
pub struct HttpsFetcher;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Fetcher for HttpsFetcher {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        use std::io::Read;
        require_https(url).map_err(|e| e.message)?;
        let policy = reqwest::redirect::Policy::custom(|a| {
            match redirect_refusal(a.url().scheme(), a.previous().len()) {
                None => a.follow(),
                Some(why) => a.error(why),
            }
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
            .redirect(policy)
            .build()
            .map_err(|e| e.to_string())?;
        let response = client.get(url).send().map_err(|e| error_chain(&e))?;
        if !response.status().is_success() {
            return Err(format!("the server answered {}", response.status()));
        }
        let mut body = Vec::new();
        response
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        if body.len() > max_bytes {
            return Err("the file is too large".into());
        }
        Ok(body)
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl Fetcher for HttpsFetcher {
    fn get(&self, _: &str, _: usize) -> Result<Vec<u8>, String> {
        Err("downloads are not available on this device".into())
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
        prefs.save(&self.paths.prefs_file())?;
        Ok(prefs.indexes.iter().map(dto).collect())
    }

    /// Fetch every index, download changed files (SHA-256 verified; the index is refused if any entry lacks one),
    /// install them and hot-reload.
    ///
    /// # Errors
    /// `io_error` / `content_error`; per-file problems are reported in `failed`.
    pub fn update_indexes(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
    ) -> ApiResult<UpdateIndexesResult> {
        let _guard = self.content_lock()?;
        let mut prefs = self.content_prefs();
        normalize_index_urls(&mut prefs.indexes);
        let total = prefs.indexes.len();
        let mut outcome = AuroraOutcome::default();
        let mut staged: Vec<(PathBuf, String)> = Vec::new();
        let mut scratches = Vec::new();
        for n in 0..total {
            let previous = prefs.indexes[n].clone();
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
                Ok(r) => prefs.indexes[n] = r,
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
        sink.content_progress(&progress("done", total, total, &message));
        Ok(UpdateIndexesResult {
            updated,
            failed: outcome.failed,
            reloaded: updated > 0,
            changed: outcome.changed,
            new_hosts: outcome.new_hosts,
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
        let (mut record, failed) =
            self.check_json_index(fetcher, sink, url, &bytes, previous, staged, scratches)?;
        if !failed.is_empty() {
            record.problem = Some(format!(
                "{} file(s) were refused or could not be downloaded",
                failed.len()
            ));
        }
        outcome.failed.extend(failed);
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
    ) -> Result<(IndexRecord, Vec<String>), String> {
        let mut failed = Vec::new();
        let sig = fetcher
            .get(&format!("{url}{SIGNATURE_SUFFIX}"), MAX_SIGNATURE_BYTES)
            .ok();
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
                failed.push(entry.url.clone());
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
                Ok(Some(done)) => {
                    staged.push((done.0.join("out"), pack_id));
                    scratches.push(done);
                }
                Ok(None) => {}
                Err(()) => {
                    failed.push(entry.url.clone());
                    record.updates_available += 1;
                }
            }
        }
        Ok((record, failed))
    }

    /// `Ok(None)` = already current, `Err` = rejected (bad URL, hash mismatch, bad content).
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
    ) -> Result<Option<Scratch>, ()> {
        require_https(file_url).map_err(|_| ())?;
        let bytes = fetcher.get(file_url, MAX_FILE_BYTES).map_err(|_| ())?;
        let actual = sha256_hex(&bytes);
        if expected != actual {
            tracing::warn!("rejected {file_url}: SHA-256 mismatch");
            return Err(());
        }
        if installed == Some(actual.as_str()) {
            return Ok(None);
        }
        let scratch = Scratch::new(&self.paths.data).map_err(|_| ())?;
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
        .map_err(|_| ())?;
        if report.converted == 0 {
            return Err(());
        }
        write_pack_meta(
            &out_dir,
            "downloaded",
            &serde_json::json!({ "sha256": actual, "indexUrl": index_url, "fileUrl": file_url }),
        );
        Ok(Some(scratch))
    }
}
