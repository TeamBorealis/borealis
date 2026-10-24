//! A 5etools `data/` folder on the web: where it is, which revision it is at, and downloading it.
//!
//! Two kinds of address are accepted, both pasted by the user (no address ships with the app):
//! a GitHub repository (`https://github.com/<owner>/<repo>`, optionally `/tree/<ref>[/<path>]`,
//! or the `raw.githubusercontent.com` form), or any `https://` address of a `data/` folder.
//! For GitHub the branch is resolved to a commit with one unauthenticated API call (the public
//! ref list is the fallback when the API is rate limited), and the files are then read at that
//! commit from `raw.githubusercontent.com`. Only the files the importer reads are downloaded:
//! measured on the real repository before creatures were read, that was 79 files / 12 MB (2.6 MB
//! gzip-compressed; the `bestiary/` files add about 210 files / 11 MB), against a
//! 50 MB repository archive. A plain folder address is walked with the same file list, using the
//! `class/`, `spells/` and `bestiary/` index manifests. Every file is capped, names from manifests must be
//! plain file names (nothing can escape the folder), and only `https://` is ever fetched.
//! Deterministic: no AI is involved.

use crate::content_fetch_retry::FetchError;
use crate::content_fivetools_zip::{
    is_data_file, MAX_FILE_BYTES, MAX_TAKEN_FILES, MAX_UNPACKED_BYTES,
};
use crate::content_index::{looks_like_html, require_https, sha256_hex};
use crate::content_packs::{progress, EventSink};
use borealis_import_5etools::folder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

const GITHUB_WEB: [&str; 2] = ["https://github.com/", "https://www.github.com/"];
const GITHUB_API: &str = "https://api.github.com/repos/";
const RAW_GITHUB: &str = "https://raw.githubusercontent.com/";
/// The API answers a bare commit SHA for this media type.
const SHA_MEDIA_TYPE: &str = "application/vnd.github.sha";
const MAX_SHA_BYTES: usize = 1024;
const MAX_REFS_BYTES: usize = 8 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const SHA_LEN: usize = 40;
const HTTP_NOT_FOUND: u16 = 404;
const HTTP_GONE: u16 = 410;
/// Failed paths named in a download problem; the rest are counted.
const MAX_NAMED_FAILURES: usize = 12;
/// Parallel downloads (raw.githubusercontent.com serves small files; latency dominates).
const WORKERS: usize = 6;
/// The marker every 5etools `data/` folder has; also the first file fetched.
const MARKER: &str = "books.json";
/// Shown when the address has no `books.json`.
pub const NO_DATA_FOLDER: &str =
    "This address has no 5etools data folder (books.json was not found).";
/// Data-folder files the importer reads by fixed path (`borealis_import_5etools::folder::data_files`),
/// besides the `class/`, `spells/` and `bestiary/` files listed by the manifests: the importer's
/// own [`folder::ROOT_FILES`] plus its spell source lookup, so the download follows that one list.
pub fn root_files() -> impl Iterator<Item = &'static str> {
    folder::ROOT_FILES
        .iter()
        .copied()
        .chain([folder::SPELL_SOURCE_LOOKUP])
}
/// Manifests (`{"<key>": "<file name>"}`) of the sub-folders the importer reads.
pub const MANIFESTS: [&str; 5] = [
    "class/index.json",
    "class/fluff-index.json",
    "spells/index.json",
    "spells/fluff-index.json",
    "bestiary/index.json",
];

/// One HTTPS request.
#[derive(Debug, Clone, Copy)]
pub struct FetchRequest<'a> {
    pub url: &'a str,
    pub max_bytes: usize,
    /// `Accept` header.
    pub accept: Option<&'a str>,
    /// `If-None-Match` header (a stored ETag).
    pub if_none_match: Option<&'a str>,
}

impl<'a> FetchRequest<'a> {
    #[must_use]
    pub fn get(url: &'a str, max_bytes: usize) -> Self {
        Self {
            url,
            max_bytes,
            accept: None,
            if_none_match: None,
        }
    }
}

/// The answer to a [`FetchRequest`]. Any HTTP status is a response; only a network failure, a
/// refused redirect or an oversized body is an error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchResponse {
    pub status: u16,
    pub etag: Option<String>,
    /// Decoded body (empty unless the status is 200).
    pub body: Vec<u8>,
    /// GitHub said the request limit is used up.
    pub rate_limited: bool,
}

/// HTTPS client for 5etools downloads (replaceable in tests).
pub trait RemoteFetcher: Send + Sync {
    /// One request; any HTTP status is an answer.
    ///
    /// # Errors
    /// A user-presentable reason.
    fn fetch(&self, request: &FetchRequest<'_>) -> Result<FetchResponse, String>;

    /// [`RemoteFetcher::fetch`] with a classified failure: an answer that may pass (408/425/429/
    /// 5xx, a rate-limited 403) is an error too, with its code and `Retry-After`. The default
    /// classifies the message; [`crate::content_index::HttpsFetcher`] answers from the response.
    ///
    /// # Errors
    /// The classified reason.
    fn fetch_classified(&self, request: &FetchRequest<'_>) -> Result<FetchResponse, FetchError> {
        let answer = self
            .fetch(request)
            .map_err(|m| FetchError::from_message(&m))?;
        crate::content_fivetools_fetch::classify_answer(answer, None)
    }
}

/// Where a 5etools data folder lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiveToolsLocation {
    /// A GitHub repository; `git_ref` `None` = the default branch, `path` `None` = find `data/`.
    GitHub {
        owner: String,
        repo: String,
        git_ref: Option<String>,
        path: Option<String>,
    },
    /// Any folder address, ending in `/`.
    Folder { base: String },
}

/// What the stored revision and per-file tags of a download are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileTag {
    pub etag: Option<String>,
    pub sha256: String,
}

const BAD_ADDRESS: &str = "Use a GitHub repository address (https://github.com/<owner>/<repo>) or the https:// address of a 5etools data/ folder.";

fn segment_ok(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn path_ok(path: &str) -> bool {
    path.split('/').all(segment_ok)
}

/// Parse a pasted address.
///
/// # Errors
/// A user-presentable reason (not `https://`, or not a usable address).
pub fn parse_location(url: &str) -> Result<FiveToolsLocation, String> {
    let url = url.trim();
    require_https(url).map_err(|_| "5etools addresses must start with https://".to_string())?;
    let clean = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .trim_end_matches('/');
    if let Some(rest) = GITHUB_WEB.iter().find_map(|p| clean.strip_prefix(p)) {
        return github_web(rest);
    }
    if let Some(rest) = clean.strip_prefix(RAW_GITHUB) {
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        return match parts.as_slice() {
            [owner, repo, git_ref, tail @ ..] => {
                github(owner, repo, Some(git_ref), tail.first().copied())
            }
            _ => Err(BAD_ADDRESS.into()),
        };
    }
    // A file of the folder (`.../data/books.json`) stands for the folder.
    let folder = match clean.rsplit_once('/') {
        Some((dir, file)) if file.ends_with(".json") && dir.len() > "https://".len() => dir,
        _ => clean,
    };
    Ok(FiveToolsLocation::Folder {
        base: format!("{folder}/"),
    })
}

fn github_web(rest: &str) -> Result<FiveToolsLocation, String> {
    let parts: Vec<&str> = rest.splitn(5, '/').collect();
    match parts.as_slice() {
        [owner, repo] => github(owner, repo, None, None),
        [owner, repo, "tree", git_ref] => github(owner, repo, Some(git_ref), None),
        [owner, repo, "tree", git_ref, path] => github(owner, repo, Some(git_ref), Some(path)),
        _ => Err(BAD_ADDRESS.into()),
    }
}

fn github(
    owner: &str,
    repo: &str,
    git_ref: Option<&str>,
    path: Option<&str>,
) -> Result<FiveToolsLocation, String> {
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let refs_ok = git_ref.is_none_or(segment_ok) && path.is_none_or(path_ok);
    if !segment_ok(owner) || !segment_ok(repo) || !refs_ok {
        return Err(BAD_ADDRESS.into());
    }
    Ok(FiveToolsLocation::GitHub {
        owner: owner.into(),
        repo: repo.into(),
        git_ref: git_ref.map(str::to_string),
        path: path.map(str::to_string),
    })
}

fn is_sha(s: &str) -> bool {
    s.len() == SHA_LEN && s.bytes().all(|b| b.is_ascii_hexdigit())
}

const RATE_LIMITED: &str =
    "GitHub is limiting requests from this network right now; try again in an hour.";

/// The commit `git_ref` (default branch when `None`) points at.
///
/// # Errors
/// A user-presentable reason.
pub fn resolve_commit(
    f: &dyn RemoteFetcher,
    owner: &str,
    repo: &str,
    git_ref: Option<&str>,
) -> Result<String, String> {
    if let Some(sha) = git_ref.filter(|r| is_sha(r)) {
        return Ok(sha.to_lowercase());
    }
    let url = format!(
        "{GITHUB_API}{owner}/{repo}/commits/{}",
        git_ref.unwrap_or("HEAD")
    );
    let api = f.fetch(&FetchRequest {
        accept: Some(SHA_MEDIA_TYPE),
        ..FetchRequest::get(&url, MAX_SHA_BYTES)
    });
    match api {
        Ok(r) if r.status == 200 => {
            let sha = String::from_utf8_lossy(&r.body).trim().to_lowercase();
            if is_sha(&sha) {
                return Ok(sha);
            }
        }
        Ok(r) if matches!(r.status, 404 | 422) => {
            return Err("GitHub does not know that repository or branch (private repositories cannot be used).".into());
        }
        Ok(r) => tracing::info!(
            status = r.status,
            limited = r.rate_limited,
            "GitHub API unavailable, using the ref list"
        ),
        Err(e) => tracing::info!("GitHub API failed ({e}), using the ref list"),
    }
    commit_from_refs(f, owner, repo, git_ref)
}

/// Fallback: the public git ref advertisement (`info/refs`), which has no API rate limit.
fn commit_from_refs(
    f: &dyn RemoteFetcher,
    owner: &str,
    repo: &str,
    git_ref: Option<&str>,
) -> Result<String, String> {
    let url = format!(
        "{}{owner}/{repo}.git/info/refs?service=git-upload-pack",
        GITHUB_WEB[0]
    );
    let r = f
        .fetch_classified(&FetchRequest::get(&url, MAX_REFS_BYTES))
        .map_err(|e| {
            if e.is_rate_limit() {
                format!("{RATE_LIMITED} [{}]", e.code)
            } else {
                format!("the GitHub branch list could not be read [{}]", e.code)
            }
        })?;
    if r.rate_limited || r.status == 429 {
        return Err(RATE_LIMITED.into());
    }
    if r.status != 200 {
        return Err(format!("GitHub answered {} for that repository.", r.status));
    }
    let refs = parse_ref_advertisement(&r.body);
    let wanted: Vec<String> = match git_ref {
        None => vec!["HEAD".into()],
        Some(name) => vec![
            format!("refs/heads/{name}"),
            format!("refs/tags/{name}^{{}}"),
            format!("refs/tags/{name}"),
        ],
    };
    wanted
        .iter()
        .find_map(|w| refs.get(w).cloned())
        .ok_or_else(|| "GitHub does not know that branch or tag.".into())
}

/// `ref name -> sha` from a smart-HTTP ref advertisement (pkt-lines).
#[must_use]
pub fn parse_ref_advertisement(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut at = 0;
    while at + 4 <= bytes.len() {
        let Some(len) = std::str::from_utf8(&bytes[at..at + 4])
            .ok()
            .and_then(|h| usize::from_str_radix(h, 16).ok())
        else {
            break;
        };
        if len < 4 {
            at += 4; // flush packet
            continue;
        }
        let Some(line) = bytes.get(at + 4..at + len) else {
            break;
        };
        at += len;
        let line = String::from_utf8_lossy(line);
        let line = line.split('\0').next().unwrap_or_default().trim_end();
        if let Some((sha, name)) = line.split_once(' ') {
            if is_sha(sha) {
                out.insert(name.to_string(), sha.to_lowercase());
            }
        }
    }
    out
}

/// The folder address of a GitHub location at `sha` (`path` `None` = repository root).
#[must_use]
pub fn raw_base(owner: &str, repo: &str, sha: &str, path: Option<&str>) -> String {
    match path {
        Some(p) => format!("{RAW_GITHUB}{owner}/{repo}/{sha}/{p}/"),
        None => format!("{RAW_GITHUB}{owner}/{repo}/{sha}/"),
    }
}

/// A finished folder download.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Download {
    /// Every file fetched (manifests included) with its ETag and SHA-256.
    pub files: BTreeMap<String, FileTag>,
    /// False when every file matched `previous` (nothing usable was written then).
    pub changed: bool,
    pub bytes: usize,
}

/// A stable digest of a download, its revision for folder addresses.
#[must_use]
pub fn files_revision(files: &BTreeMap<String, FileTag>) -> String {
    let text: String = files
        .iter()
        .map(|(p, t)| format!("{p}\0{}\n", t.sha256))
        .collect();
    sha256_hex(text.as_bytes())
}

struct Fetched {
    tag: FileTag,
    /// `None` = 304 Not Modified (the stored tag still holds).
    body: Option<Vec<u8>>,
}

/// One file of the folder: `Ok(None)` when it does not exist (404/410). Any other answer but
/// 200 (or 304 to a conditional request) is a failure with its code.
fn fetch_file(
    f: &dyn RemoteFetcher,
    base: &str,
    rel: &str,
    max: usize,
    etag: Option<&str>,
) -> Result<Option<Fetched>, FetchError> {
    let url = format!("{base}{rel}");
    let r = f.fetch_classified(&FetchRequest {
        if_none_match: etag,
        ..FetchRequest::get(&url, max)
    })?;
    match r.status {
        200 => Ok(Some(Fetched {
            tag: FileTag {
                etag: r.etag,
                sha256: sha256_hex(&r.body),
            },
            body: Some(r.body),
        })),
        304 if etag.is_some() => Ok(Some(Fetched {
            tag: FileTag {
                etag: etag.map(str::to_string),
                sha256: String::new(),
            },
            body: None,
        })),
        HTTP_NOT_FOUND | HTTP_GONE => Ok(None),
        s => Err(FetchError::http(s, "", None)),
    }
}

/// A manifest's file names, as `<dir>/<name>`; every name must be a plain file name.
fn manifest_files(dir: &str, body: &[u8]) -> Result<Vec<String>, String> {
    let map: BTreeMap<String, serde_json::Value> = serde_json::from_slice(body)
        .map_err(|_| format!("{dir}: the file list could not be read"))?;
    let mut out = Vec::new();
    for value in map.values() {
        let Some(name) = value.as_str() else { continue };
        let rel = format!("{dir}/{name}");
        if !segment_ok(name) || !is_data_file(&rel) {
            return Err(format!("{dir}: refused the unsafe file name \"{name}\""));
        }
        if !out.contains(&rel) {
            out.push(rel);
        }
    }
    Ok(out)
}

/// Files of a download that could not be had, out of how many, and how many were not tried
/// after the first failure. Its message names paths and codes only, never file contents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadFailure {
    /// `(path, why)` in download order.
    pub failed: Vec<(String, FetchError)>,
    pub total: usize,
    pub not_tried: usize,
}

impl DownloadFailure {
    fn one(rel: &str, e: FetchError, total: usize) -> Self {
        Self {
            failed: vec![(rel.to_string(), e)],
            total,
            not_tried: 0,
        }
    }

    /// The problem shown in the 5etools tab and the error of the stage or Update.
    #[must_use]
    pub fn message(&self) -> String {
        let mut causes: Vec<&str> = Vec::new();
        for (_, e) in &self.failed {
            let cause = cause_of(e);
            if !causes.contains(&cause) {
                causes.push(cause);
            }
        }
        let named: Vec<String> = self
            .failed
            .iter()
            .take(MAX_NAMED_FAILURES)
            .map(|(rel, e)| format!("{rel} [{}]", e.code))
            .collect();
        let mut text = format!(
            "Could not download the 5etools data: {} of {} files could not be downloaded ({}). Nothing was installed or changed; try again later. Failed: {}",
            self.failed.len(),
            self.total.max(self.failed.len()),
            causes.join(", "),
            named.join(", ")
        );
        let unnamed = self.failed.len().saturating_sub(MAX_NAMED_FAILURES);
        if unnamed > 0 {
            text.push_str(&format!(" and {unnamed} more"));
        }
        text.push('.');
        if self.not_tried > 0 {
            text.push_str(&format!(
                " {} more were not tried after the failure.",
                self.not_tried
            ));
        }
        text
    }
}

/// A few words on why a file failed, from its code.
fn cause_of(e: &FetchError) -> &'static str {
    match (e.status(), e.code.as_str()) {
        (Some(429), _) | (_, crate::content_fetch_retry::CODE_RATE_LIMITED) => {
            "the server is limiting requests"
        }
        (Some(408), _) | (_, crate::content_fetch_retry::CODE_TIMEOUT) => {
            "the server did not answer in time"
        }
        (Some(HTTP_NOT_FOUND | HTTP_GONE), _) => "a file the folder lists is missing",
        (Some(500..=599), _) => "the server had an error",
        (_, crate::content_fetch_retry::CODE_NETWORK) => "the connection failed",
        (_, crate::content_fetch_retry::CODE_TRUNCATED) => "a download ended early",
        (_, crate::content_fetch_retry::CODE_TOO_LARGE) => "a file is too large",
        _ => "the server refused a file",
    }
}

/// Download the importer's files of the folder at `base` into `dest` (created if needed).
/// With `previous` tags, files are requested conditionally; when everything matches, the result
/// says `changed: false`. When anything changed, every file is in `dest`.
///
/// A root file the server does not have (404/410) is left out, as the importer skips a root file
/// a folder lacks; a file a manifest lists is required. Any other failure (after the retries of
/// `f`) fails the whole download: see [`DownloadFailure::message`].
///
/// # Errors
/// A user-presentable reason; `dest` may then hold partial files (the caller removes it).
pub fn download_folder(
    f: &dyn RemoteFetcher,
    base: &str,
    dest: &Path,
    previous: Option<&BTreeMap<String, FileTag>>,
    sink: &dyn EventSink,
) -> Result<Download, String> {
    let failed =
        |rel: &str, e: FetchError, total: usize| DownloadFailure::one(rel, e, total).message();
    let marker = fetch_file(f, base, MARKER, MAX_FILE_BYTES, None)
        .map_err(|e| failed(MARKER, e, 1))?
        .and_then(|m| m.body.map(|b| (m.tag, b)))
        .ok_or(NO_DATA_FOLDER)?;
    if looks_like_html(&marker.1) {
        return Err("This address is a web page, not a 5etools data folder.".into());
    }
    let mut files: BTreeMap<String, FileTag> = BTreeMap::new();
    let mut wanted: Vec<String> = root_files()
        .filter(|r| *r != MARKER)
        .map(str::to_string)
        .collect();
    let mut required: BTreeSet<String> = BTreeSet::new();
    for manifest in MANIFESTS {
        let dir = manifest.split('/').next().unwrap_or_default();
        let got = fetch_file(f, base, manifest, MAX_MANIFEST_BYTES, None)
            .map_err(|e| failed(manifest, e, 1 + MANIFESTS.len()))?;
        if let Some(m) = got {
            let body = m.body.unwrap_or_default();
            for rel in manifest_files(dir, &body)? {
                required.insert(rel.clone());
                wanted.push(rel);
            }
            files.insert(manifest.to_string(), m.tag);
        }
    }
    if wanted.len() + 1 > MAX_TAKEN_FILES {
        return Err("This folder lists too many files.".into());
    }
    write_file(dest, MARKER, &marker.1)?;
    files.insert(MARKER.into(), marker.0);
    let total = wanted.len();
    let in_download = |e: DownloadFailure| {
        DownloadFailure {
            total: total + files.len(),
            ..e
        }
        .message()
    };
    let fetched = fetch_all(f, base, &wanted, previous, &required, sink).map_err(in_download)?;
    let mut bytes = marker.1.len();
    let mut stale = Vec::new();
    for (rel, got) in wanted.iter().zip(fetched) {
        let Some(got) = got else { continue };
        match got.body {
            Some(body) => {
                bytes += body.len();
                if bytes > MAX_UNPACKED_BYTES {
                    return Err("This folder is too large to import.".into());
                }
                write_file(dest, rel, &body)?;
                files.insert(rel.clone(), got.tag);
            }
            None => {
                let kept = previous
                    .and_then(|p| p.get(rel))
                    .cloned()
                    .unwrap_or(got.tag);
                files.insert(rel.clone(), kept);
                stale.push(rel.clone());
            }
        }
    }
    let changed = previous.is_none_or(|p| !same_content(p, &files));
    if changed && !stale.is_empty() {
        let all: BTreeSet<String> = stale.iter().cloned().collect();
        let again = fetch_all(f, base, &stale, None, &all, sink).map_err(|e| {
            DownloadFailure {
                total: files.len(),
                ..e
            }
            .message()
        })?;
        for (rel, got) in stale.iter().zip(again) {
            let got = got.and_then(|g| g.body.map(|b| (g.tag, b)));
            let (tag, body) =
                got.ok_or_else(|| format!("{rel} disappeared during the download"))?;
            bytes += body.len();
            if bytes > MAX_UNPACKED_BYTES {
                return Err("This folder is too large to import.".into());
            }
            write_file(dest, rel, &body)?;
            files.insert(rel.clone(), tag);
        }
    }
    sink.import_progress(&progress("downloading", total, total, "Downloaded"));
    Ok(Download {
        files,
        changed,
        bytes,
    })
}

/// Same files with the same contents (ETag-only answers keep the stored hash).
fn same_content(a: &BTreeMap<String, FileTag>, b: &BTreeMap<String, FileTag>) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((pa, ta), (pb, tb))| pa == pb && ta.sha256 == tb.sha256)
}

fn write_file(dest: &Path, rel: &str, body: &[u8]) -> Result<(), String> {
    if !is_data_file(rel) && !MANIFESTS.contains(&rel) {
        return Err(format!("refused the file name \"{rel}\""));
    }
    let path = rel
        .split('/')
        .fold(dest.to_path_buf(), |p, seg| p.join(seg));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not save the download: {e}"))?;
    }
    std::fs::write(&path, body).map_err(|e| format!("could not save the download: {e}"))
}

type Slot = Mutex<Option<Result<Option<Fetched>, FetchError>>>;

/// Fetch `rels` with a few parallel workers, in order (conditionally when `tags` has an ETag);
/// progress is reported per file. A `required` file that is not there is a failure. After the
/// first failure no new file is started; every failed path is returned with its reason.
fn fetch_all(
    f: &dyn RemoteFetcher,
    base: &str,
    rels: &[String],
    tags: Option<&BTreeMap<String, FileTag>>,
    required: &BTreeSet<String>,
    sink: &dyn EventSink,
) -> Result<Vec<Option<Fetched>>, DownloadFailure> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let slots: Vec<Slot> = rels.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(rels.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(rel) = rels.get(i) else { break };
                let etag = tags
                    .and_then(|t| t.get(rel))
                    .and_then(|t| t.etag.as_deref());
                let got = match fetch_file(f, base, rel, MAX_FILE_BYTES, etag) {
                    Ok(None) if required.contains(rel) => {
                        Err(FetchError::http(HTTP_NOT_FOUND, "Not Found", None))
                    }
                    other => other,
                };
                let failed = got.is_err();
                *slots[i].lock().unwrap_or_else(PoisonError::into_inner) = Some(got);
                let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                sink.import_progress(&progress("downloading", n, rels.len(), rel));
                if failed {
                    next.store(rels.len(), Ordering::SeqCst); // stop early
                }
            });
        }
    });
    let mut out = Vec::with_capacity(rels.len());
    let mut failure = DownloadFailure {
        total: rels.len(),
        ..DownloadFailure::default()
    };
    for (rel, slot) in rels.iter().zip(slots) {
        match slot.into_inner().unwrap_or_else(PoisonError::into_inner) {
            Some(Ok(got)) => out.push(got),
            Some(Err(e)) => failure.failed.push((rel.clone(), e)),
            None => failure.not_tried += 1,
        }
    }
    if failure.failed.is_empty() {
        Ok(out)
    } else {
        Err(failure)
    }
}

/// Decode a gzip body (RFC 1952), refusing more than `max` output bytes.
///
/// # Errors
/// A damaged stream or too much output.
pub fn gunzip(bytes: &[u8], max: usize) -> Result<Vec<u8>, String> {
    const FEXTRA: u8 = 4;
    const FNAME: u8 = 8;
    const FCOMMENT: u8 = 16;
    const FHCRC: u8 = 2;
    let bad = || "the server sent a damaged compressed file".to_string();
    if bytes.len() < 18 || bytes[0] != 0x1f || bytes[1] != 0x8b || bytes[2] != 8 {
        return Err(bad());
    }
    let flags = bytes[3];
    let mut at = 10;
    if flags & FEXTRA != 0 {
        let n = usize::from(u16::from_le_bytes([
            *bytes.get(at).ok_or_else(bad)?,
            *bytes.get(at + 1).ok_or_else(bad)?,
        ]));
        at += 2 + n;
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            let end = bytes
                .get(at..)
                .and_then(|b| b.iter().position(|c| *c == 0))
                .ok_or_else(bad)?;
            at += end + 1;
        }
    }
    if flags & FHCRC != 0 {
        at += 2;
    }
    let body = bytes
        .get(at..bytes.len().saturating_sub(8))
        .ok_or_else(bad)?;
    let out = miniz_oxide::inflate::decompress_to_vec_with_limit(body, max)
        .map_err(|_| "the file is too large or damaged".to_string())?;
    let tail = &bytes[bytes.len() - 8..];
    let size = u32::from_le_bytes([tail[4], tail[5], tail[6], tail[7]]);
    if u64::from(size) != (out.len() as u64 & 0xffff_ffff) {
        return Err(bad());
    }
    Ok(out)
}

#[cfg(test)]
#[path = "content_fivetools_remote_tests.rs"]
mod tests;
