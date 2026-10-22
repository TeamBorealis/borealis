//! Garbage in (engine-test-audit-1, fuzz plan): random byte mutations of the SRD-only 5etools
//! fixtures never panic the importer. It returns `Ok` (with a report) or a typed `ImportError`.
//! Deterministic: a fixed xorshift seed picks the file, the mutation kind and the position, so
//! a failure names the case and replays exactly. No new dependency.

use borealis_import_5etools::{import_folder, ImportOptions};
use std::path::{Path, PathBuf};

const CASES: u64 = 120;
const MAX_FLIPS: usize = 8;
const MAX_INSERT: usize = 16;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn byte(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            json_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "json") {
            out.push(p);
        }
    }
}

/// One mutated copy of `text`: byte flips, a deleted span, inserted junk, or a truncation.
fn mutate(text: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = text.to_vec();
    if out.is_empty() {
        return out;
    }
    match rng.below(4) {
        0 => {
            for _ in 0..=rng.below(MAX_FLIPS) {
                let at = rng.below(out.len());
                out[at] = rng.byte();
            }
        }
        1 => {
            let at = rng.below(out.len());
            let len = 1 + rng.below(out.len() - at);
            out.drain(at..at + len.min(64));
        }
        2 => {
            let at = rng.below(out.len());
            let junk: Vec<u8> = (0..=rng.below(MAX_INSERT)).map(|_| rng.byte()).collect();
            out.splice(at..at, junk);
        }
        _ => out.truncate(rng.below(out.len())),
    }
    out
}

#[test]
fn mutated_5etools_files_never_panic_the_importer() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files = Vec::new();
    json_files(&fixtures.join("data"), &mut files);
    assert!(files.len() >= 10, "the fixtures hold their data files");
    let options = ImportOptions::default();
    let (mut ok, mut typed_errors) = (0, 0);
    for case in 0..CASES {
        let mut rng = Rng::new(case + 1);
        let source = &files[rng.below(files.len())];
        let rel = source.strip_prefix(&fixtures).expect("under fixtures");
        let original = std::fs::read(source).expect("read");
        // one mutated file per run keeps each import fast (a full fixture import takes ~2 s)
        let work = tempfile::tempdir().expect("temp dir");
        let target = work.path().join(rel);
        std::fs::create_dir_all(target.parent().expect("parent")).expect("create dir");
        std::fs::write(&target, mutate(&original, &mut rng)).expect("write mutation");
        let root = work.path().join("data");
        match std::panic::catch_unwind(|| import_folder(&root, &options)) {
            Ok(Ok(_)) => ok += 1,
            Ok(Err(_)) => typed_errors += 1,
            Err(_) => panic!("case {case}: the importer panicked on a mutation of {rel:?}"),
        }
    }
    eprintln!("garbage_in 5etools: {ok} imported, {typed_errors} typed errors of {CASES}");
}
