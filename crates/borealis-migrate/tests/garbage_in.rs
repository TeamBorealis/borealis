//! Garbage in (engine-test-audit-1, fuzz plan): random byte mutations of a small synthetic
//! Aurora XML file never panic the migrator. It returns `Ok` (with a report) or a typed
//! `MigrateError`. Deterministic: a fixed xorshift seed picks the mutation, so a failure names
//! the case and replays exactly. No new dependency, synthetic content only.

use borealis_content::PackOrigin;
use borealis_migrate::{migrate_path, MigrateOptions};

const CASES: u64 = 300;
const MAX_FLIPS: usize = 8;
const MAX_INSERT: usize = 16;
const MAX_DELETE: usize = 64;

/// Classes, a select with requirements, supports, stats, a spell and an item: one of each
/// construct the rule translator reads.
const SEED_XML: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<elements>
  <info><name>Synthetic</name><update version="1.0.0"><file name="s.xml" url="https://e.invalid/s.xml" /></update></info>
  <element name="Synth Class" type="Class" source="Synthetic" id="ID_SYNTH_CLASS_A">
    <supports>Synth Archetype</supports>
    <description><p>A synthetic class.</p></description>
    <setters><set name="hd">d8</set><set name="short">Short</set></setters>
    <rules>
      <stat name="hp:level" value="1" />
      <stat name="level" value="1" bonus="base" />
      <grant type="Proficiency" id="ID_SYNTH_PROF" />
      <grant type="Class Feature" id="ID_SYNTH_FEATURE" level="2" />
      <select type="Feat" name="Pick" supports="Synth Pick" level="3" requirements="[level:3]" />
      <select type="Spell" name="Spell Known" supports="Synth List" number="2" />
      <stat name="strength:modifier" value="strength:score" />
    </rules>
    <multiclass id="ID_SYNTH_MULTI" prerequisite="[strength:13]">
      <setters><set name="hd">d8</set></setters>
      <rules><grant type="Proficiency" id="ID_SYNTH_PROF2" /></rules>
    </multiclass>
  </element>
  <element name="Synth Feature" type="Class Feature" source="Synthetic" id="ID_SYNTH_FEATURE">
    <requirements>ID_SYNTH_CLASS_A</requirements>
    <rules><stat name="speed" value="10" /></rules>
  </element>
  <element name="Synth Spell" type="Spell" source="Synthetic" id="ID_SYNTH_SPELL">
    <supports>Synth List, Wizard</supports>
    <setters><set name="level">1</set><set name="school">Evocation</set><set name="time">1 action</set></setters>
  </element>
  <element name="Synth Item" type="Item" source="Synthetic" id="ID_SYNTH_ITEM">
    <setters><set name="cost" currency="gp">5</set><set name="weight" lb="2">2 lb.</set></setters>
    <rules><stat name="ac" value="1" /></rules>
  </element>
</elements>
"#;

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

/// One mutated copy: byte flips, a deleted span, inserted junk, or a truncation.
fn mutate(text: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = text.to_vec();
    match rng.below(4) {
        0 => {
            for _ in 0..=rng.below(MAX_FLIPS) {
                let at = rng.below(out.len());
                out[at] = rng.byte();
            }
        }
        1 => {
            let at = rng.below(out.len());
            let len = 1 + rng.below(MAX_DELETE);
            out.drain(at..(at + len).min(text.len()));
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
fn mutated_aurora_xml_never_panics_the_migrator() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("synthetic.xml");
    let options = MigrateOptions {
        origin: PackOrigin::Custom,
        ..MigrateOptions::default()
    };
    std::fs::write(&file, SEED_XML).expect("write seed");
    let clean = migrate_path(dir.path(), &options).expect("the seed file migrates");
    assert!(!clean.files.is_empty(), "the seed file produces content");
    let (mut ok, mut typed_errors) = (0, 0);
    for case in 0..CASES {
        let mut rng = Rng::new(case + 1);
        std::fs::write(&file, mutate(SEED_XML.as_bytes(), &mut rng)).expect("write mutation");
        match std::panic::catch_unwind(|| migrate_path(dir.path(), &options)) {
            Ok(Ok(_)) => ok += 1,
            Ok(Err(_)) => typed_errors += 1,
            Err(_) => panic!("case {case}: the migrator panicked on a mutation"),
        }
    }
    eprintln!("garbage_in aurora: {ok} migrated, {typed_errors} typed errors of {CASES}");
}
