//! Consented model download (local-search-spec sec. 8, S3.2): stream to `models/.partial/<sha>.part`, resume with
//! HTTP `Range`, verify the compiled SHA-256 and size, atomic rename to `models/<file>`; on a checksum mismatch
//! delete the partial and try the other URL once, then fail with a plain error. Pause keeps the partial, cancel
//! deletes it. The only header a request carries beyond the HTTP client's minimum is `Range` (no identifiers).
//!
//! Nothing here runs on its own: the app calls [`download`] only after the user accepted the consent dialog.
//! The transport is a trait so tests run against a local server ([`http::HttpTransport`] is the real one).

pub mod http;
#[cfg(test)]
mod test_server;
#[cfg(test)]
mod tests;

use crate::models::{ModelFile, LICENSE_FILE_NAME, MODEL_LICENSE, MODEL_NOTICE, NOTICE_FILE_NAME};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Folder of partial downloads inside the models folder.
pub const PARTIAL_DIR: &str = ".partial";
const PARTIAL_SUFFIX: &str = ".part";
const COPY_CHUNK_BYTES: usize = 256 * 1024;
const HASH_CHUNK_BYTES: usize = 1 << 20;
const MAX_FILE_NAME: usize = 128;
const HTTP_OK: u16 = 200;
const HTTP_PARTIAL_CONTENT: u16 = 206;
const HTTP_RANGE_NOT_SATISFIABLE: u16 = 416;

/// What one HTTP GET answered (status line, the two length headers, the body stream).
pub struct Response {
    pub status: u16,
    /// `Content-Range` value, if any.
    pub content_range: Option<String>,
    /// `Content-Length`, if the server sent one.
    pub content_length: Option<u64>,
    pub body: Box<dyn Read + Send>,
}

/// A transport failure in plain words (no URL, no device data).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct TransportError(pub String);

/// The HTTP client the download uses.
pub trait Transport: Send + Sync {
    /// GET `url`. `from > 0` adds `Range: bytes=<from>-`, the only header a download sets.
    ///
    /// # Errors
    /// The request could not be sent or the response headers did not arrive.
    fn get(&self, url: &str, from: u64) -> Result<Response, TransportError>;
}

/// One file to download: its name under the models folder, compiled SHA-256 and size, and its URLs in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSpec {
    pub file_name: String,
    /// Lower-case hex SHA-256.
    pub sha256: String,
    /// Exact size: the server's length must equal it and the body is capped at it.
    pub bytes: u64,
    pub sources: Vec<String>,
}

impl DownloadSpec {
    /// The spec of a compiled model file, with the sources it may use today.
    #[must_use]
    pub fn for_file(file: &ModelFile) -> Self {
        Self {
            file_name: file.file_name.to_owned(),
            sha256: file.sha256.to_owned(),
            bytes: file.bytes,
            sources: file
                .download_sources()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }

    fn validate(&self) -> Result<(), DownloadError> {
        let sha_ok = self.sha256.len() == 64
            && self
                .sha256
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        if !is_safe_file_name(&self.file_name) || !sha_ok || self.bytes == 0 {
            return Err(DownloadError::Invalid);
        }
        Ok(())
    }
}

/// A plain file name: ASCII letters, digits, `.`, `_`, `-`; not starting with `.`; bounded length.
#[must_use]
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FILE_NAME
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The models folder (`<app-data>/models`): final files, partial downloads, licence files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDir {
    root: PathBuf,
}

impl ModelDir {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `models/<file>`.
    #[must_use]
    pub fn final_path(&self, spec: &DownloadSpec) -> PathBuf {
        self.root.join(&spec.file_name)
    }

    /// `models/.partial/<sha>.part`.
    #[must_use]
    pub fn partial_path(&self, spec: &DownloadSpec) -> PathBuf {
        self.partial_for(&spec.sha256)
    }

    fn partial_for(&self, sha256: &str) -> PathBuf {
        self.root
            .join(PARTIAL_DIR)
            .join(format!("{sha256}{PARTIAL_SUFFIX}"))
    }

    /// Bytes already in the partial (0 when there is none or it is not a regular file).
    #[must_use]
    pub fn partial_bytes(&self, spec: &DownloadSpec) -> u64 {
        regular_file_len(&self.partial_path(spec)).unwrap_or(0)
    }

    /// The final file is present with the compiled size (the backend verifies the SHA-256 when it loads it).
    #[must_use]
    pub fn is_installed(&self, spec: &DownloadSpec) -> bool {
        regular_file_len(&self.final_path(spec)) == Some(spec.bytes)
    }

    /// Delete the given model files, their partials and the licence files ("Delete model", spec sec. 8). Only
    /// these names are touched; anything else in the folder stays. Returns how many files were removed.
    ///
    /// # Errors
    /// I/O errors other than "not found".
    pub fn delete(&self, specs: &[DownloadSpec]) -> io::Result<usize> {
        let mut removed = 0;
        for spec in specs.iter().filter(|s| s.validate().is_ok()) {
            removed += usize::from(remove_if_exists(&self.final_path(spec))?);
            removed += usize::from(remove_if_exists(&self.partial_path(spec))?);
        }
        for name in [LICENSE_FILE_NAME, NOTICE_FILE_NAME] {
            removed += usize::from(remove_if_exists(&self.root.join(name))?);
        }
        // The partial folder goes only when empty (remove_dir refuses otherwise).
        let _ = fs::remove_dir(self.root.join(PARTIAL_DIR));
        Ok(removed)
    }

    /// Write the model's LICENSE and NOTICE next to the file (Apache-2.0 sec. 4a/4b, EM6).
    ///
    /// # Errors
    /// The files could not be written.
    pub fn write_licence_files(&self) -> io::Result<()> {
        fs::write(self.root.join(LICENSE_FILE_NAME), MODEL_LICENSE)?;
        fs::write(self.root.join(NOTICE_FILE_NAME), MODEL_NOTICE)
    }
}

fn regular_file_len(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path)
        .ok()
        .filter(fs::Metadata::is_file)
        .map(|m| m.len())
}

/// `Ok(true)` when the file existed and is gone now.
fn remove_if_exists(path: &Path) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

const RUN: u8 = 0;
const PAUSE: u8 = 1;
const CANCEL: u8 = 2;

/// Pause / cancel switch of a running download, checked after every chunk (cheap to clone).
#[derive(Debug, Clone, Default)]
pub struct Control(Arc<AtomicU8>);

impl Control {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop at the next chunk and keep the partial.
    pub fn pause(&self) {
        let _ = self
            .0
            .compare_exchange(RUN, PAUSE, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Stop at the next chunk and delete the partial.
    pub fn cancel(&self) {
        self.0.store(CANCEL, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.0.load(Ordering::SeqCst) == PAUSE
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst) == CANCEL
    }
}

/// What the download is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Downloading,
    /// Hashing the complete file.
    Verifying,
}

/// Progress after each chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
}

/// How a download ended without an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Verified and in place.
    Installed(PathBuf),
    /// Paused: the partial is kept for a `Range` resume.
    Paused { have: u64 },
    /// Cancelled: the partial is deleted.
    Cancelled,
}

/// A download that did not finish. Messages are plain sentences for Settings (no URLs, no paths).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadError {
    /// No URL serves this device's file (a compiled entry without sources; guarded, never in a release).
    #[error("The model for this device is not available yet.")]
    NotAvailable,
    /// The compiled file table is malformed (never in a release; guarded anyway).
    #[error("The model file entry is invalid.")]
    Invalid,
    /// Every URL failed; the reason of the last one.
    #[error("The model could not be downloaded: {0}.")]
    Failed(String),
    /// The models folder could not be written.
    #[error("The model could not be saved on this device: {0}.")]
    Storage(String),
}

enum Attempt {
    Done(PathBuf),
    Paused(u64),
    Cancelled,
}

enum AttemptError {
    /// The complete file's SHA-256 is wrong: the partial is deleted and the other URL tried.
    Mismatch,
    /// This URL failed (network, HTTP status, wrong size); the other URL is tried, a good partial is kept.
    Source(String),
    /// Local disk failure: stop, another URL would not help.
    Local(io::Error),
}

impl From<io::Error> for AttemptError {
    fn from(e: io::Error) -> Self {
        Self::Local(e)
    }
}

const MISMATCH_TEXT: &str = "the downloaded file did not match its checksum";

/// Download `spec` into `dir` (see the module docs). Each URL is tried at most once per call.
///
/// # Errors
/// [`DownloadError::NotAvailable`] without URLs, [`DownloadError::Failed`] when every URL failed,
/// [`DownloadError::Storage`] on disk errors, [`DownloadError::Invalid`] for a malformed spec.
pub fn download(
    dir: &ModelDir,
    spec: &DownloadSpec,
    transport: &dyn Transport,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<Outcome, DownloadError> {
    spec.validate()?;
    if spec.sources.is_empty() {
        return Err(DownloadError::NotAvailable);
    }
    let storage = |e: io::Error| DownloadError::Storage(e.kind().to_string());
    fs::create_dir_all(dir.root.join(PARTIAL_DIR)).map_err(storage)?;
    if dir.is_installed(spec) && hash_matches(&dir.final_path(spec), spec).map_err(storage)? {
        dir.write_licence_files().map_err(storage)?;
        return Ok(Outcome::Installed(dir.final_path(spec)));
    }
    let mut last = String::new();
    for url in &spec.sources {
        if control.is_cancelled() {
            break;
        }
        match attempt(dir, spec, url, transport, control, progress) {
            Ok(Attempt::Done(path)) => {
                dir.write_licence_files().map_err(storage)?;
                return Ok(Outcome::Installed(path));
            }
            Ok(Attempt::Paused(have)) => return Ok(Outcome::Paused { have }),
            Ok(Attempt::Cancelled) => break,
            Err(AttemptError::Mismatch) => {
                remove_if_exists(&dir.partial_path(spec)).map_err(storage)?;
                MISMATCH_TEXT.clone_into(&mut last);
            }
            Err(AttemptError::Source(why)) => last = why,
            Err(AttemptError::Local(e)) => return Err(storage(e)),
        }
    }
    if control.is_cancelled() {
        remove_if_exists(&dir.partial_path(spec)).map_err(storage)?;
        return Ok(Outcome::Cancelled);
    }
    Err(DownloadError::Failed(last))
}

/// Length of a usable partial; a symlink, folder or oversize file is removed first (start over).
fn usable_partial(part: &Path, cap: u64) -> io::Result<u64> {
    match fs::symlink_metadata(part) {
        Ok(m) if m.is_file() && m.len() <= cap => Ok(m.len()),
        Ok(m) if m.is_dir() => fs::remove_dir_all(part).map(|()| 0),
        Ok(_) => fs::remove_file(part).map(|()| 0),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}

fn source(why: impl Into<String>) -> AttemptError {
    AttemptError::Source(why.into())
}

/// `bytes START-END/TOTAL` -> (START, TOTAL).
fn parse_content_range(value: &str) -> Option<(u64, u64)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let (start, _end) = range.split_once('-')?;
    Some((start.trim().parse().ok()?, total.trim().parse().ok()?))
}

fn attempt(
    dir: &ModelDir,
    spec: &DownloadSpec,
    url: &str,
    transport: &dyn Transport,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<Attempt, AttemptError> {
    let part = dir.partial_path(spec);
    let have = usable_partial(&part, spec.bytes)?;
    if have == spec.bytes {
        return finalize(dir, spec, control, progress);
    }
    let resp = transport.get(url, have).map_err(|e| source(e.0))?;
    let (mut file, start) = match resp.status {
        HTTP_PARTIAL_CONTENT if have > 0 => {
            let (start, total) = resp
                .content_range
                .as_deref()
                .and_then(parse_content_range)
                .ok_or_else(|| source("the server sent an unusable range"))?;
            if start != have || total != spec.bytes {
                return Err(source("the server resumed at the wrong place"));
            }
            (OpenOptions::new().append(true).open(&part)?, have)
        }
        HTTP_OK => {
            if resp.content_length.is_some_and(|n| n != spec.bytes) {
                return Err(source("the server sent a file of the wrong size"));
            }
            (File::create(&part)?, 0)
        }
        HTTP_RANGE_NOT_SATISFIABLE => {
            // The server holds fewer bytes than the compiled size: not our file.
            remove_if_exists(&part)?;
            return Err(source("the server sent a file of the wrong size"));
        }
        other => return Err(source(format!("the server answered HTTP {other}"))),
    };
    let mut body = resp.body;
    match copy_capped(&mut body, &mut file, start, spec.bytes, control, progress)? {
        Copied::Complete => {}
        Copied::Stopped => {
            file.sync_all()?;
            drop(file);
            return Ok(if control.is_cancelled() {
                Attempt::Cancelled
            } else {
                Attempt::Paused(dir.partial_bytes(spec))
            });
        }
        Copied::Short => {
            file.sync_all()?;
            return Err(source("the connection was interrupted"));
        }
        Copied::Oversize => {
            drop(file);
            remove_if_exists(&part)?;
            return Err(source("the server sent a file of the wrong size"));
        }
    }
    file.sync_all()?;
    drop(file);
    finalize(dir, spec, control, progress)
}

enum Copied {
    Complete,
    /// Pause or cancel was requested.
    Stopped,
    /// The body ended (or failed) before the compiled size.
    Short,
    /// The body is longer than the compiled size.
    Oversize,
}

/// Copy at most `total - start` bytes; one byte more marks the body as oversize.
fn copy_capped(
    body: &mut dyn Read,
    file: &mut File,
    start: u64,
    total: u64,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> io::Result<Copied> {
    let mut buf = vec![0u8; COPY_CHUNK_BYTES];
    let mut done = start;
    while done < total {
        let room = usize::try_from(total - done).map_or(buf.len(), |r| r.min(buf.len()));
        let n = match body.read(&mut buf[..room]) {
            Ok(0) | Err(_) => return Ok(Copied::Short),
            Ok(n) => n,
        };
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress(Progress {
            phase: Phase::Downloading,
            done,
            total,
        });
        if control.is_paused() || control.is_cancelled() {
            return Ok(Copied::Stopped);
        }
    }
    let mut extra = [0u8; 1];
    Ok(match body.read(&mut extra) {
        Ok(0) | Err(_) => Copied::Complete,
        Ok(_) => Copied::Oversize,
    })
}

/// Hash the complete partial; rename it into place when it matches.
fn finalize(
    dir: &ModelDir,
    spec: &DownloadSpec,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<Attempt, AttemptError> {
    let part = dir.partial_path(spec);
    let mut file = File::open(&part)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK_BYTES];
    let mut done = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        progress(Progress {
            phase: Phase::Verifying,
            done,
            total: spec.bytes,
        });
        if control.is_cancelled() {
            return Ok(Attempt::Cancelled);
        }
        if control.is_paused() {
            return Ok(Attempt::Paused(spec.bytes));
        }
    }
    drop(file);
    if done != spec.bytes || hex(&hasher.finalize()) != spec.sha256 {
        return Err(AttemptError::Mismatch);
    }
    let target = dir.final_path(spec);
    fs::rename(&part, &target)?;
    Ok(Attempt::Done(target))
}

fn hash_matches(path: &Path, spec: &DownloadSpec) -> io::Result<bool> {
    Ok(file_sha256(path)? == spec.sha256)
}

/// Lower-case hex SHA-256 of a whole file (the installed-file check here, and the llama loader before use).
///
/// # Errors
/// The file cannot be opened or read.
pub fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
