//! Aurora XML `.index` files in Update indexes (the files Aurora users paste, e.g. aurorabuilder's
//! `core.index`). Aurora indexes carry no per-file hashes, and a hash read from the same index would
//! add nothing beyond TLS, so the controls are: HTTPS only (also for every nested index, file and
//! redirect), the existing size limits and XML hardening, nested indexes followed to a bounded depth
//! with a cycle guard and a file cap, trust on first use of every file's SHA-256 (later changes are
//! applied but reported), and a report of hosts that were not seen before. Such indexes are never
//! "verified"; the UI asks for consent when one is added.
//!
//! A download that still fails after the retries for a passing reason (rate limit, server error,
//! timeout, dropped connection, short body; see [`crate::content_fetch_retry`]) holds the whole
//! index: nothing is installed or replaced, every failed address is reported with its code, and
//! the next Update tries again. A partial pack is installed only for failures that will not pass
//! (a missing file, a web page, a refused address), and is then reported as "N of M".
//!
//! All files of one top-level index are converted together as ONE pack through the folder-drop code
//! path ([`crate::content_preview::convert_set`]), so cross-file references and replacement order
//! match dropping the same repository as a folder.

use crate::content_dto::{MigrationIssue, MigrationReport};
use crate::content_import::{slug, stem, write_pack_meta, Scratch};
use crate::content_index::{looks_like_html, raw_file_url, require_https, sha256_hex, Fetcher};
use crate::content_packs::{progress, EventSink};
use crate::content_prefs::IndexRecord;
use crate::content_preview::convert_set;
use crate::ids::rfc3339;
use crate::state::AppState;
use borealis_content::{IndexFile, IssueCode};
use borealis_migrate::{convert_xml, Document};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::SystemTime;

/// Deepest chain of nested indexes followed below a top-level index.
pub const MAX_NESTED_DEPTH: usize = 8;
/// Most content files taken from one top-level index (nested ones included).
pub const MAX_INDEX_FILES: usize = 2_000;
/// Largest index file (top-level or nested).
pub const MAX_INDEX_BYTES: usize = 2 * 1024 * 1024;
/// Largest content file.
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
/// A listed address answered with an HTML page instead of the file.
const WEB_PAGE: &str = "the address is a web page, not the file itself";
/// Folder for files that live outside the top-level index's directory.
const OTHER_DIR: &str = "_other";

/// What one update of an Aurora index adds to the overall result.
#[derive(Debug, Default)]
pub struct AuroraOutcome {
    /// Files (and nested indexes) that could not be used.
    pub failed: Vec<String>,
    /// Files whose content changed since the first download (trust on first use).
    pub changed: Vec<String>,
    /// Hosts that appeared for the first time.
    pub new_hosts: Vec<String>,
    /// The conversion report of every pack converted in this update.
    pub reports: Vec<MigrationReport>,
}

/// A content file listed by an index tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub url: String,
    /// Where the file goes in the converted set (mirrors the repository layout).
    pub rel: String,
}

/// The files an index tree lists, plus every problem met while walking it.
#[derive(Debug, Default)]
pub struct Crawl {
    pub files: Vec<Listed>,
    /// Every fetched index URL (the top-level one first).
    pub indexes: Vec<String>,
    /// `(url, reason)` for entries that were refused or could not be read.
    pub problems: Vec<(String, String)>,
    /// Nested indexes that could not be read for a passing reason (their files are unknown).
    pub incomplete: Vec<String>,
    /// Nested indexes listed (read or not).
    pub nested: usize,
}

/// True when `bytes` look like an XML document rather than the Borealis JSON index.
#[must_use]
pub fn looks_like_xml(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    text.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
}

/// Parse an Aurora XML index.
///
/// # Errors
/// A short reason when the bytes are not an Aurora index.
pub fn parse_index(bytes: &[u8], url: &str) -> Result<IndexFile, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "the index is not UTF-8 text".to_string())?;
    let name = url.rsplit('/').next().unwrap_or("index.index");
    match convert_xml(text.trim_start_matches('\u{feff}'), name).document {
        Some(Document::Index(index)) => Ok(index),
        Some(Document::Content(_)) => Err("this is a content file, not an index".into()),
        None => Err("the index could not be read".into()),
    }
}

/// The host of an `https://` URL, lower-cased.
#[must_use]
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// Directory of `url` including the trailing slash.
fn base_of(url: &str) -> &str {
    url.rfind('/').map_or(url, |i| &url[..=i])
}

/// The relative path a file gets in the converted set; `None` when it cannot be placed safely.
#[must_use]
pub fn relative_path(base: &str, url: &str) -> Option<String> {
    let url = url.split(['?', '#']).next().unwrap_or(url);
    let rest = match url.strip_prefix(base) {
        Some(r) => r.to_string(),
        None => format!("{OTHER_DIR}/{}", url.strip_prefix("https://")?),
    };
    let safe = |s: &&str| {
        !s.is_empty()
            && *s != "."
            && *s != ".."
            && !s
                .chars()
                .any(|c| c.is_control() || matches!(c, '\\' | ':' | '*' | '"' | '<' | '>' | '|'))
    };
    let parts: Vec<&str> = rest.split('/').collect();
    parts.iter().all(safe).then(|| parts.join("/"))
}

/// Walk an index tree depth-first from an already fetched top-level index.
pub fn crawl(fetcher: &dyn Fetcher, top_url: &str, top: &IndexFile) -> Crawl {
    let mut walk = Walk {
        fetcher,
        base: base_of(top_url).to_string(),
        crawl: Crawl::default(),
        seen_files: BTreeSet::new(),
        seen_indexes: BTreeSet::from([top_url.to_string()]),
        capped: false,
    };
    walk.crawl.indexes.push(top_url.to_string());
    walk.entries(top, &mut vec![top_url.to_string()]);
    walk.crawl
}

struct Walk<'a> {
    fetcher: &'a dyn Fetcher,
    base: String,
    crawl: Crawl,
    seen_files: BTreeSet<String>,
    seen_indexes: BTreeSet<String>,
    capped: bool,
}

impl Walk<'_> {
    fn problem(&mut self, url: &str, why: impl Into<String>) {
        self.crawl.problems.push((url.to_string(), why.into()));
    }

    fn entries(&mut self, index: &IndexFile, chain: &mut Vec<String>) {
        for entry in index.files.iter().filter(|e| !e.is_obsolete()) {
            let url = raw_file_url(&entry.url);
            let url = url.as_str();
            if require_https(url).is_err() {
                self.problem(
                    url,
                    format!("refused {url}: only https:// addresses are allowed"),
                );
            } else if entry.is_nested_index() {
                self.nested(url, chain);
            } else {
                self.file(url);
            }
        }
    }

    fn nested(&mut self, url: &str, chain: &mut Vec<String>) {
        if chain.iter().any(|c| c == url) {
            self.problem(url, format!("index cycle at {url}; not followed again"));
            return;
        }
        if chain.len() > MAX_NESTED_DEPTH {
            let why = format!("nested more than {MAX_NESTED_DEPTH} levels deep: {url}");
            self.problem(url, why);
            return;
        }
        if !self.seen_indexes.insert(url.to_string()) {
            return;
        }
        self.crawl.nested += 1;
        let parsed = match self.fetcher.get_classified(url, MAX_INDEX_BYTES) {
            Ok(bytes) if looks_like_html(&bytes) => Err(WEB_PAGE.into()),
            Ok(bytes) => parse_index(&bytes, url),
            Err(e) => {
                if e.is_transient() {
                    self.crawl.incomplete.push(url.to_string());
                }
                Err(e.reason())
            }
        };
        match parsed {
            Ok(index) => {
                self.crawl.indexes.push(url.to_string());
                chain.push(url.to_string());
                self.entries(&index, chain);
                chain.pop();
            }
            Err(why) => self.problem(url, why),
        }
    }

    fn file(&mut self, url: &str) {
        if !self.seen_files.insert(url.to_string()) {
            return;
        }
        if self.crawl.files.len() >= MAX_INDEX_FILES {
            if !self.capped {
                self.capped = true;
                let why =
                    format!("more than {MAX_INDEX_FILES} files listed; the rest were skipped");
                self.problem(url, why);
            }
            return;
        }
        match relative_path(&self.base, url) {
            Some(rel) if rel.to_lowercase().ends_with(".xml") => {
                self.crawl.files.push(Listed {
                    url: url.to_string(),
                    rel,
                });
            }
            Some(_) => self.problem(url, "not an .xml content file"),
            None => self.problem(url, "the address cannot be stored safely"),
        }
    }
}

/// One line for the Indexes list: failures grouped by reason, most common first.
#[must_use]
pub fn summarize(problems: &[(String, String)]) -> Option<String> {
    let mut by_reason: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (url, why) in problems {
        by_reason
            .entry(why.as_str())
            .or_default()
            .push(url.as_str());
    }
    let mut groups: Vec<_> = by_reason.into_iter().collect();
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    let parts: Vec<String> = groups
        .iter()
        .map(|(why, urls)| match urls.len() {
            1 if why.contains(urls[0]) => (*why).to_string(),
            1 => format!("{why} ({})", urls[0]),
            n => format!("{why} for {n} files"),
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// Stable pack id for a top-level index URL.
fn pack_id_for(url: &str) -> String {
    let file = url.rsplit('/').next().unwrap_or(url);
    format!(
        "dl-{}-{}",
        slug(stem(file)),
        &sha256_hex(url.as_bytes())[..8]
    )
}

impl AppState {
    /// Update one Aurora XML index whose top-level bytes were just fetched.
    ///
    /// # Errors
    /// A reason when the index itself is unusable (the previous record is then kept).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_aurora_index(
        &self,
        fetcher: &dyn Fetcher,
        sink: &dyn EventSink,
        url: &str,
        bytes: &[u8],
        previous: &IndexRecord,
        staged: &mut Vec<(PathBuf, String)>,
        scratches: &mut Vec<Scratch>,
        outcome: &mut AuroraOutcome,
    ) -> Result<IndexRecord, String> {
        if previous.public_key.is_some() {
            return Err("the index is no longer signed".into());
        }
        let top = parse_index(bytes, url)?;
        let name = top
            .info
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| stem(url).to_string());
        let crawl = crawl(fetcher, url, &top);
        let mut record = IndexRecord {
            url: url.to_string(),
            name: name.clone(),
            last_checked: Some(rfc3339(SystemTime::now())),
            file_count: crawl.files.len(),
            requires_newer_app: top
                .min_app_version
                .as_deref()
                .is_some_and(crate::content_index::newer_than_app),
            ..IndexRecord::default()
        };
        if record.requires_newer_app {
            record.updates_available = record.file_count;
            record.file_hashes.clone_from(&previous.file_hashes);
            record.hosts.clone_from(&previous.hosts);
            return Ok(record);
        }
        let mut problems = crawl.problems.clone();
        let mut passing = crawl.incomplete.clone();
        let fetched = download_all(fetcher, sink, &crawl.files, &mut problems, &mut passing);
        record.hosts = note_hosts(url, &crawl, previous, outcome);
        // Entries that never became a file (refused, cycles) still count as listed.
        let listed = (crawl.files.len() + crawl.nested).max(problems.len());
        outcome
            .failed
            .extend(problems.iter().map(|(u, _)| u.clone()));
        if !passing.is_empty() {
            // Never install or replace a pack that misses files only because of the network.
            tracing::warn!(
                url,
                failed = problems.len(),
                listed,
                "index held: downloads failed for a passing reason"
            );
            record.problem = Some(format!(
                "{} of {listed} files could not be downloaded, so nothing was installed or \
                 changed; run Update again. {}",
                problems.len(),
                summarize(&problems).unwrap_or_default()
            ));
            record.updates_available = record.file_count.max(1);
            record.file_hashes.clone_from(&previous.file_hashes);
            let nothing = MigrationReport {
                file_name: url.to_string(),
                format: "xml".into(),
                converted: 0,
                skipped: 0,
                issues: Vec::new(),
            };
            outcome.reports.push(with_failures(nothing, &problems));
            return Ok(record);
        }
        let hashes: BTreeMap<String, String> = fetched
            .iter()
            .map(|(l, b)| (l.url.clone(), sha256_hex(b)))
            .collect();
        for (file, hash) in &hashes {
            if previous
                .file_hashes
                .get(file)
                .is_some_and(|old| old != hash)
            {
                tracing::warn!("index {url}: {file} changed since it was first downloaded");
                outcome.changed.push(file.clone());
            }
        }
        let pack_id = pack_id_for(url);
        let installed = self.paths.library().join(&pack_id).is_dir();
        let failed_files = crawl.files.len() - fetched.len();
        record.problem = summarize(&problems).map(|summary| {
            format!(
                "{} of {listed} files could not be used: {summary}",
                problems.len()
            )
        });
        record.updates_available = failed_files;
        let lost =
            !problems.is_empty() && previous.file_hashes.keys().any(|k| !hashes.contains_key(k));
        if installed && (lost || hashes == previous.file_hashes) {
            // Unchanged, or a partial download that would drop installed content: keep what is there.
            record.file_hashes.clone_from(&previous.file_hashes);
            return Ok(record);
        }
        if fetched.is_empty() {
            return Err(record
                .problem
                .clone()
                .unwrap_or_else(|| "the index lists no content files".into()));
        }
        let scratch = Scratch::new(&self.paths.data).map_err(|e| e.message)?;
        let (in_dir, out_dir) = (scratch.0.join("in"), scratch.0.join("out"));
        for (listed, body) in &fetched {
            let path = in_dir.join(&listed.rel);
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, body));
            if written.is_err() {
                return Err("the downloaded files could not be stored".into());
            }
        }
        sink.content_progress(&progress("converting", 0, 1, &name));
        let converted = convert_set(&in_dir, &out_dir, &pack_id, &name).map_err(|e| e.message)?;
        let found = self.findings(&converted, &out_dir).map_err(|e| e.message)?;
        outcome
            .reports
            .push(with_failures(found.report(url, &converted), &problems));
        if converted.converted == 0 {
            return Err("no elements could be converted from this index".into());
        }
        write_pack_meta(
            &out_dir,
            "downloaded",
            &serde_json::json!({ "indexUrl": url, "format": "aurora-index", "files": fetched.len() }),
        );
        staged.push((out_dir, pack_id));
        scratches.push(scratch);
        record.file_hashes = hashes;
        Ok(record)
    }
}

/// `report` with one error per file that could not be used (its address, and its reason with the
/// code) ahead of the conversion's own issues.
fn with_failures(report: MigrationReport, problems: &[(String, String)]) -> MigrationReport {
    let failed = problems.iter().map(|(file, why)| MigrationIssue {
        severity: "error",
        code: IssueCode::IndexEntryMissing,
        message: format!("not imported: {why}"),
        element: None,
        element_type: None,
        file: Some(file.clone()),
        line: None,
    });
    let issues = failed.chain(report.issues).collect();
    MigrationReport { issues, ..report }
}

/// Download every listed file; failures go to `problems`, and the addresses of those that may
/// pass also to `passing`.
fn download_all(
    fetcher: &dyn Fetcher,
    sink: &dyn EventSink,
    files: &[Listed],
    problems: &mut Vec<(String, String)>,
    passing: &mut Vec<String>,
) -> Vec<(Listed, Vec<u8>)> {
    let mut fetched = Vec::with_capacity(files.len());
    for (i, listed) in files.iter().enumerate() {
        if i % 25 == 0 {
            let label = listed.rel.rsplit('/').next().unwrap_or(&listed.rel);
            sink.content_progress(&progress("downloading", i, files.len(), label));
        }
        match fetcher.get_classified(&listed.url, MAX_FILE_BYTES) {
            Ok(body) if looks_like_html(&body) => {
                problems.push((listed.url.clone(), WEB_PAGE.into()));
            }
            Ok(body) => fetched.push((listed.clone(), body)),
            Err(e) => {
                if e.is_transient() {
                    passing.push(listed.url.clone());
                }
                problems.push((listed.url.clone(), e.reason()));
            }
        }
    }
    fetched
}

/// Hosts of this index tree; hosts not seen before (other than the index's own) are reported.
fn note_hosts(
    url: &str,
    crawl: &Crawl,
    previous: &IndexRecord,
    outcome: &mut AuroraOutcome,
) -> Vec<String> {
    let own = host_of(url);
    let hosts: BTreeSet<String> = crawl
        .indexes
        .iter()
        .chain(crawl.files.iter().map(|l| &l.url))
        .map(|u| host_of(u))
        .collect();
    for host in &hosts {
        if *host != own && !previous.hosts.contains(host) {
            tracing::warn!("index {url} now also downloads from {host}");
            outcome.new_hosts.push(host.clone());
        }
    }
    hosts.into_iter().collect()
}

#[cfg(test)]
#[path = "content_index_partial_tests.rs"]
mod partial_tests;
