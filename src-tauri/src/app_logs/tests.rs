use super::*;
use std::sync::{Arc, Mutex};

fn clock(days: Arc<Mutex<String>>) -> Box<dyn Fn() -> String + Send> {
    Box::new(move || days.lock().unwrap().clone())
}

fn names(dir: &Path) -> Vec<String> {
    log_files(dir)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn utc_day_formats_known_dates() {
    assert_eq!(utc_day(UNIX_EPOCH), "1970-01-01");
    let t = UNIX_EPOCH + std::time::Duration::from_secs(1_760_486_400);
    assert_eq!(utc_day(t), "2025-10-15");
    let leap = UNIX_EPOCH + std::time::Duration::from_secs(1_709_164_800);
    assert_eq!(utc_day(leap), "2024-02-29");
}

#[test]
fn redactor_replaces_home_in_every_spelling() {
    let r = Redactor::new(&["/Users/alon".into(), "C:\\Users\\Alon".into()]);
    assert_eq!(
        r.apply("open /Users/alon/Documents/x.json"),
        "open ~/Documents/x.json"
    );
    assert_eq!(r.apply("C:\\Users\\Alon\\AppData"), "~\\AppData");
    assert_eq!(r.apply("\"C:\\\\Users\\\\Alon\\\\x\""), "\"~\\\\x\"");
    assert_eq!(r.apply("C:/Users/Alon/x"), "~/x");
    assert_eq!(r.apply("nothing here"), "nothing here");
}

#[test]
fn redactor_ignores_a_root_home() {
    let r = Redactor::new(&["/".into(), String::new()]);
    assert_eq!(r.apply("/etc/hosts"), "/etc/hosts");
}

#[test]
fn redacting_writer_masks_lines_before_they_reach_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let day = Arc::new(Mutex::new("2026-10-15".to_string()));
    let rolling = RollingFile::new(dir.path(), 5, 1024, clock(day));
    let mut w = RedactingWriter::new(rolling, Redactor::new(&["/Users/alon".into()]));
    w.write_all(b"WARN failed /Users/alon/chars/a.json\n")
        .unwrap();
    w.flush().unwrap();
    let text = std::fs::read_to_string(dir.path().join("borealis-2026-10-15.log")).unwrap();
    assert_eq!(text, "WARN failed ~/chars/a.json\n");
}

#[test]
fn rotation_keeps_the_newest_five_files() {
    let dir = tempfile::tempdir().unwrap();
    let day = Arc::new(Mutex::new(String::new()));
    let mut w = RollingFile::new(dir.path(), MAX_LOG_FILES, 1024, clock(Arc::clone(&day)));
    for d in 1..=8 {
        *day.lock().unwrap() = format!("2026-10-{d:02}");
        w.write_all(b"line\n").unwrap();
    }
    let files = names(dir.path());
    assert_eq!(files.len(), 5);
    assert_eq!(files[0], "borealis-2026-10-04.log");
    assert_eq!(files[4], "borealis-2026-10-08.log");
}

#[test]
fn rotation_leaves_foreign_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "keep").unwrap();
    let day = Arc::new(Mutex::new("2026-10-01".to_string()));
    let mut w = RollingFile::new(dir.path(), 1, 1024, clock(day));
    w.write_all(b"x\n").unwrap();
    assert!(dir.path().join("notes.txt").is_file());
}

#[test]
fn a_file_is_truncated_at_the_size_cap() {
    let dir = tempfile::tempdir().unwrap();
    let day = Arc::new(Mutex::new("2026-10-15".to_string()));
    let mut w = RollingFile::new(dir.path(), 5, 100, clock(day));
    for i in 0..50 {
        w.write_all(format!("line number {i:03}\n").as_bytes())
            .unwrap();
    }
    let path = dir.path().join("borealis-2026-10-15.log");
    let size = std::fs::metadata(&path).unwrap().len();
    assert!(size <= 100, "size {size}");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("line number 049"));
    assert!(text.starts_with("[log file reached"));
}

#[test]
fn an_unwritable_directory_is_skipped_silently() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("logs");
    std::fs::write(&blocker, "a file where the folder should be").unwrap();
    let day = Arc::new(Mutex::new("2026-10-15".to_string()));
    let mut w = RollingFile::new(&blocker, 5, 100, clock(day));
    assert_eq!(w.write(b"hello\n").unwrap(), 6);
    assert!(w.flush().is_ok());
    // init on such a data dir must not panic either
    init(dir.path());
}

#[test]
fn recent_logs_returns_the_newest_lines_across_files_redacted() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("borealis-2026-10-14.log"), "a1\na2\na3\n").unwrap();
    std::fs::write(
        dir.path().join("borealis-2026-10-15.log"),
        "b1 /Users/alon/x\nb2\n",
    )
    .unwrap();
    let r = Redactor::new(&["/Users/alon".into()]);
    assert_eq!(recent_logs_with(dir.path(), 3, &r), "a3\nb1 ~/x\nb2");
    assert_eq!(recent_logs_with(dir.path(), 200, &r).lines().count(), 5);
    assert_eq!(recent_logs_with(&dir.path().join("none"), 5, &r), "");
}

#[test]
fn recent_logs_is_cut_to_the_byte_cap() {
    let dir = tempfile::tempdir().unwrap();
    let long = format!("{}\n", "x".repeat(MAX_EXCERPT_BYTES));
    std::fs::write(dir.path().join("borealis-2026-10-15.log"), long.repeat(3)).unwrap();
    let out = recent_logs_with(dir.path(), 200, &Redactor::default());
    assert!(out.len() <= MAX_EXCERPT_BYTES);
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn the_panic_hook_logs_message_and_location() {
    let capture = Capture::default();
    let sub = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(capture.clone())
        .finish();
    install_panic_hook();
    tracing::subscriber::with_default(sub, || {
        let _ = std::panic::catch_unwind(|| panic!("boom for the log"));
    });
    let text = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("panic at "), "{text}");
    assert!(text.contains("boom for the log"), "{text}");
    assert!(
        text.replace('\\', "/").contains("app_logs/tests.rs"),
        "{text}"
    );
}
