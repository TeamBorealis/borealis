//! Local log files (decision UP3): rolling daily files in `<app data>/logs/`, newest five kept, each
//! capped at about 5 MB. Nothing is sent anywhere; the logs only leave the device when the user shares
//! them (mobile) or ticks "Include recent logs" in a bug report (BR1).
//!
//! Privacy: lines carry ids, counts, URLs of content indexes and error reasons, never source text,
//! notes or character content. The user's home directory is replaced by `~` in every line.
//! Everything here is best-effort: an unwritable folder silently disables file logging.

use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// Folder (inside the app data dir) that holds the log files.
pub const LOGS_DIR: &str = "logs";
/// Log files kept; older days are deleted.
pub const MAX_LOG_FILES: usize = 5;
/// A day's file is truncated when it would grow past this.
pub const MAX_LOG_FILE_BYTES: u64 = 5 * 1024 * 1024;
/// Lines of the newest logs included in a bug report or shared from a phone.
pub const EXCERPT_LINES: usize = 200;
/// Largest excerpt returned, whatever the line lengths.
pub const MAX_EXCERPT_BYTES: usize = 100 * 1024;
const FILE_PREFIX: &str = "borealis-";
const FILE_SUFFIX: &str = ".log";
const TRUNCATED_MARK: &[u8] = b"[log file reached its size limit and was truncated]\n";
/// INFO for the app's own crates, WARN for every dependency.
const DEFAULT_FILTER: &str = "warn,borealis_lib=info,borealis_ai=info,borealis_assist=info,\
    borealis_content=info,borealis_engine=info,borealis_character=info,borealis_migrate=info,\
    borealis_sheet=info,borealis_import_5etools=info";

/// Keeps the background writer alive (and flushes it) for the life of the process.
static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// UTC calendar day `YYYY-MM-DD` of `now`.
fn utc_day(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn is_log_file(name: &str) -> bool {
    name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX)
}

/// Log files in `dir`, oldest first (the names sort by date).
fn log_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_str().is_some_and(is_log_file))
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

/// A file per UTC day, `borealis-YYYY-MM-DD.log`, truncated at [`MAX_LOG_FILE_BYTES`].
pub struct RollingFile {
    dir: PathBuf,
    clock: Box<dyn Fn() -> String + Send>,
    max_files: usize,
    max_bytes: u64,
    day: String,
    file: Option<File>,
    written: u64,
}

impl RollingFile {
    /// A writer into `dir` (created on the first write); `clock` names the current day.
    #[must_use]
    pub fn new(
        dir: &Path,
        max_files: usize,
        max_bytes: u64,
        clock: Box<dyn Fn() -> String + Send>,
    ) -> Self {
        Self {
            dir: dir.to_path_buf(),
            clock,
            max_files,
            max_bytes,
            day: String::new(),
            file: None,
            written: 0,
        }
    }

    fn open_day(&mut self, day: String) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{FILE_PREFIX}{day}{FILE_SUFFIX}"));
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        self.written = file.metadata().map_or(0, |m| m.len());
        self.file = Some(file);
        self.day = day;
        self.prune();
        Ok(())
    }

    fn prune(&self) {
        let files = log_files(&self.dir);
        let excess = files.len().saturating_sub(self.max_files);
        for old in files.iter().take(excess) {
            let _ = std::fs::remove_file(old);
        }
    }

    fn truncate(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(TRUNCATED_MARK)?;
            self.written = TRUNCATED_MARK.len() as u64;
        }
        Ok(())
    }

    fn try_write(&mut self, buf: &[u8]) -> io::Result<()> {
        let day = (self.clock)();
        if self.file.is_none() || day != self.day {
            self.open_day(day)?;
        }
        if self.written + buf.len() as u64 > self.max_bytes {
            self.truncate()?;
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(buf)?;
            self.written += buf.len() as u64;
        }
        Ok(())
    }
}

impl Write for RollingFile {
    /// Never fails: a log line that cannot be written is dropped.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.try_write(buf).is_err() {
            self.file = None;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
        Ok(())
    }
}

/// Replaces the user's home directory with `~`.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    needles: Vec<String>,
}

impl Redactor {
    /// Redact each of `homes` (and its forward-slash, backslash and JSON-escaped spellings).
    #[must_use]
    pub fn new(homes: &[String]) -> Self {
        let mut needles: Vec<String> = Vec::new();
        for home in homes {
            let trimmed = home.trim_end_matches(['/', '\\']);
            if trimmed.len() < 2 {
                continue;
            }
            let slashes = trimmed.replace('\\', "/");
            let backslashes = trimmed.replace('/', "\\");
            let escaped = backslashes.replace('\\', "\\\\");
            for form in [trimmed.to_string(), slashes, backslashes, escaped] {
                if !needles.contains(&form) {
                    needles.push(form);
                }
            }
        }
        needles.sort_by_key(|n| std::cmp::Reverse(n.len()));
        Self { needles }
    }

    /// The current user's home directory.
    #[must_use]
    pub fn for_current_user() -> Self {
        let homes: Vec<String> = ["HOME", "USERPROFILE"]
            .iter()
            .filter_map(std::env::var_os)
            .filter_map(|v| v.into_string().ok())
            .collect();
        Self::new(&homes)
    }

    /// `text` with every home directory replaced by `~`.
    #[must_use]
    pub fn apply<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut out = Cow::Borrowed(text);
        for needle in &self.needles {
            if out.contains(needle.as_str()) {
                out = Cow::Owned(out.replace(needle.as_str(), "~"));
            }
        }
        out
    }
}

/// Redacts what is written to the inner writer.
pub struct RedactingWriter<W: Write> {
    inner: W,
    redactor: Redactor,
}

impl<W: Write> RedactingWriter<W> {
    /// Wrap `inner`.
    pub fn new(inner: W, redactor: Redactor) -> Self {
        Self { inner, redactor }
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.inner
            .write_all(self.redactor.apply(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The text of a panic: message and source location, never a backtrace.
fn panic_text(
    payload: &(dyn std::any::Any + Send),
    location: Option<&std::panic::Location<'_>>,
) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "no message".to_string());
    let place = location.map_or_else(
        || "unknown location".to_string(),
        |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
    );
    format!("panic at {place}: {message}")
}

/// Log panics (message and location) before the previous hook runs.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(target: "borealis_lib::panic", "{}", panic_text(info.payload(), info.location()));
        previous(info);
    }));
}

/// The log filter: `RUST_LOG` in debug builds, the defaults otherwise.
fn filter() -> EnvFilter {
    #[cfg(debug_assertions)]
    if let Ok(env) = EnvFilter::try_from_default_env() {
        return env;
    }
    EnvFilter::new(DEFAULT_FILTER)
}

/// Install the subscriber and the panic hook. Safe to call more than once; never panics.
pub fn init(data_dir: &Path) {
    if GUARD.get().is_some() {
        return;
    }
    let dir = data_dir.join(LOGS_DIR);
    let rolling = RollingFile::new(
        &dir,
        MAX_LOG_FILES,
        MAX_LOG_FILE_BYTES,
        Box::new(|| utc_day(SystemTime::now())),
    );
    let redacting = RedactingWriter::new(rolling, Redactor::for_current_user());
    let (writer, guard) = tracing_appender::non_blocking(redacting);
    let file_layer = fmt::layer().with_ansi(false).with_writer(writer);
    let stderr_layer = cfg!(debug_assertions).then(|| fmt::layer().with_writer(io::stderr));
    let installed = tracing_subscriber::registry()
        .with(filter())
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .is_ok();
    if installed {
        let _ = GUARD.set(guard);
        install_panic_hook();
    }
}

/// One INFO line that opens every session in the log.
pub fn log_startup() {
    let platform = if cfg!(any(target_os = "android", target_os = "ios")) {
        "mobile"
    } else {
        "desktop"
    };
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        platform,
        "Borealis started"
    );
}

/// The last `max_lines` log lines of `dir` (oldest first), cut to [`MAX_EXCERPT_BYTES`] and
/// redacted again. Empty when there are no logs.
#[must_use]
pub fn recent_logs(dir: &Path, max_lines: usize) -> String {
    let redactor = Redactor::for_current_user();
    recent_logs_with(dir, max_lines, &redactor)
}

fn recent_logs_with(dir: &Path, max_lines: usize, redactor: &Redactor) -> String {
    let mut lines: Vec<String> = Vec::new();
    for file in log_files(dir).iter().rev() {
        if lines.len() >= max_lines {
            break;
        }
        let Ok(bytes) = std::fs::read(file) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let mut mine: Vec<String> = text.lines().map(str::to_string).collect();
        mine.append(&mut lines);
        lines = mine;
    }
    let skip = lines.len().saturating_sub(max_lines);
    let mut out = redactor.apply(&lines[skip..].join("\n")).into_owned();
    if out.len() > MAX_EXCERPT_BYTES {
        let mut start = out.len() - MAX_EXCERPT_BYTES;
        while !out.is_char_boundary(start) {
            start += 1;
        }
        out = out[start..].to_string();
    }
    out
}

#[cfg(test)]
mod tests;
