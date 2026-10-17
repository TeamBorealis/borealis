//! Shared fixtures for the golden tests. Aurora XML fixtures are migrated on the fly and are
//! never copied into the repository; their variants are skipped (with a notice) when they are
//! absent. Every test also runs on the bundled, licence-clean `content/core` + `content/srd-5.1`
//! packs (same element ids), so public CI exercises the vectors. Under CI (`CI` set and not
//! `0`/`false`) a test that runs no variant, or a missing bundled pack, fails instead of passing.
//! Every full derive of a test that uses these helpers is checked against the settle invariants
//! (`borealis_engine::invariants`).
#![allow(dead_code)]

use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, PackOrigin};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, Choice, SlotKey};
use borealis_engine::Engine;
use borealis_migrate::{migrate_path, MigrateOptions};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Which core definitions the content uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// Everything migrated from the legacy fixtures (core files included).
    Fixture,
    /// Our core pack plus the fixture SRD elements.
    CorePack,
    /// Our core pack plus the bundled SRD 5.1 pack: in the repository, always available.
    Bundled,
}

/// Every variant, in run order.
pub const VARIANTS: [Variant; 3] = [Variant::Fixture, Variant::CorePack, Variant::Bundled];

/// True on a CI runner (`CI` set to anything but empty, `0` or `false`).
pub fn on_ci() -> bool {
    std::env::var("CI").map(|v| ci_flag(&v)).unwrap_or(false)
}

/// How a `CI` value reads: anything but empty, `0` or `false` (any case) means CI.
pub fn ci_flag(value: &str) -> bool {
    let v = value.trim();
    !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
}

pub fn fixtures_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BOREALIS_AURORA_FIXTURES") {
        return Some(PathBuf::from(p));
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join("fixtures").join("aurora-xml");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn core_pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core")
}

pub fn srd51_pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/srd-5.1")
}

fn migrated() -> Option<Vec<(String, ContentFile)>> {
    let dir = fixtures_dir()?;
    let opts = MigrateOptions {
        origin: PackOrigin::Core,
        validate: false,
        ..MigrateOptions::default()
    };
    let out = migrate_path(&dir, &opts).ok()?;
    let mut files = Vec::new();
    for f in &out.files {
        if f.path == "pack.json" || !f.path.ends_with(".json") {
            continue;
        }
        files.push((
            f.path.clone(),
            ContentFile::from_json_str(&f.contents).ok()?,
        ));
    }
    // the core files first, like the pack order
    files.sort_by_key(|(n, _)| (!n.contains(".core."), n.clone()));
    Some(files)
}

static FIXTURE: OnceLock<Option<(ContentStore, Engine)>> = OnceLock::new();
static COREPACK: OnceLock<Option<(ContentStore, Engine)>> = OnceLock::new();
static BUNDLED: OnceLock<Option<(ContentStore, Engine)>> = OnceLock::new();

/// The bundled packs (core + SRD 5.1), `None` when either directory is missing.
fn bundled() -> Option<(ContentStore, Engine)> {
    let (core, srd) = (core_pack_dir(), srd51_pack_dir());
    if !core.is_dir() || !srd.is_dir() {
        return None;
    }
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core pack loads")
        .add_pack_dir(&srd)
        .expect("SRD 5.1 pack loads")
        .build();
    let engine = Engine::new(&store);
    Some((store, engine))
}

pub fn engine(v: Variant) -> Option<&'static Engine> {
    let cell = match v {
        Variant::Fixture => &FIXTURE,
        Variant::CorePack => &COREPACK,
        Variant::Bundled => return BUNDLED.get_or_init(bundled).as_ref().map(|(_, e)| e),
    };
    cell.get_or_init(|| {
        let files = migrated()?;
        let mut b = ContentStoreBuilder::new();
        match v {
            Variant::Fixture => {
                for (n, f) in files {
                    b = b.add_file(n, PackOrigin::Core, f);
                }
            }
            Variant::CorePack => {
                b = b.add_pack_dir(&core_pack_dir()).expect("core pack loads");
                for (n, f) in files.into_iter().filter(|(n, _)| n.contains(".elements.")) {
                    b = b.add_file(n, PackOrigin::Core, f);
                }
            }
            Variant::Bundled => unreachable!("bundled packs load in `bundled`"),
        }
        let store = b.build();
        let engine = Engine::new(&store);
        Some((store, engine))
    })
    .as_ref()
    .map(|(_, e)| e)
}

/// Run a test body against every available content variant. The Aurora fixture variants are
/// skipped with a notice when `fixtures/aurora-xml` is absent; the bundled variant always runs.
/// Under CI a missing bundled pack, or a run of no variant at all, fails the test.
pub fn each_variant(name: &str, body: impl Fn(&Engine, Variant)) {
    borealis_engine::invariants::enable_for_thread();
    let mut ran = Vec::new();
    for v in VARIANTS {
        match engine(v) {
            Some(e) => {
                body(e, v);
                ran.push(v);
            }
            None => {
                let what = match v {
                    Variant::Bundled => "content/core + content/srd-5.1 not found",
                    _ => "fixtures/aurora-xml not found",
                };
                assert!(
                    !(on_ci() && v == Variant::Bundled),
                    "{name}: {what}; CI must run the bundled vectors"
                );
                eprintln!("SKIPPED {name} ({v:?}): {what}");
            }
        }
    }
    assert!(
        !(on_ci() && ran.is_empty()),
        "{name}: no content variant ran under CI"
    );
}

/// A character under construction with the same operations a user would perform.
pub struct Hero<'e> {
    pub e: &'e Engine,
    pub s: CharacterState,
}

pub const LEVEL1: &str = "ID_LEVEL_1";

impl<'e> Hero<'e> {
    pub fn new(e: &'e Engine, average_hp: bool) -> Self {
        borealis_engine::invariants::enable_for_thread();
        let mut s = commands::new_character(e, borealis_content::Ruleset::Dnd2014);
        s.options
            .retain(|o| o != "ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP");
        if average_hp {
            s.options.push("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP".into());
        }
        Self { e, s }
    }

    pub fn snap(&self) -> CharacterSnapshot {
        self.e.derive(&self.s)
    }

    pub fn base(&mut self, scores: [i32; 6]) -> &mut Self {
        self.s.abilities.base = scores;
        self
    }

    /// Choose `id` in the slot `name`/`ordinal` of `owner`.
    pub fn pick(&mut self, owner: &str, name: &str, ordinal: u32, id: &str) -> &mut Self {
        let snap = self.snap();
        let key = slot_key(&snap, owner, name, ordinal).unwrap_or_else(|| {
            let open: Vec<_> = snap
                .slots
                .iter()
                .map(|s| format!("{}:{}#{}", s.owner.id, s.name, s.key.ordinal))
                .collect();
            panic!("no open slot {owner}/{name}#{ordinal}; open: {open:?}")
        });
        self.pick_key(&key, id)
    }

    pub fn pick_key(&mut self, key: &SlotKey, id: &str) -> &mut Self {
        self.s = commands::select(self.e, &self.s, key, Choice::Element(id.to_string()))
            .unwrap_or_else(|err| panic!("select {id}: {err}"))
            .state;
        self
    }

    /// Choose `id` in the first open slot of type `ty` that offers it.
    pub fn pick_any(&mut self, ty: &str, id: &str) -> &mut Self {
        let snap = self.snap();
        let key = snap
            .slots
            .iter()
            .find(|s| s.ty == ty && s.chosen.is_none() && s.options.iter().any(|o| o.id == id))
            .map(|s| s.key.clone())
            .unwrap_or_else(|| panic!("no open {ty} slot offering {id}"));
        self.pick_key(&key, id)
    }

    pub fn race_class(&mut self, race: &str, class: &str) -> &mut Self {
        self.pick(LEVEL1, "Race", 1, race)
            .pick(LEVEL1, "Class", 1, class)
    }

    pub fn background(&mut self, id: &str) -> &mut Self {
        self.pick(LEVEL1, "Background", 1, id)
    }

    pub fn level_to(&mut self, n: u32) -> &mut Self {
        while self.s.level() < n {
            self.s = commands::level_up_main(self.e, &self.s, None)
                .expect("level up")
                .state;
        }
        self
    }

    pub fn give(&mut self, item: &str) -> String {
        self.s = commands::add_item(self.e, &self.s, item, 1)
            .expect("add item")
            .state;
        self.s.inventory.items.last().expect("item").uid.clone()
    }

    pub fn equip(&mut self, item: &str) -> String {
        let uid = self.give(item);
        self.s = commands::equip(self.e, &self.s, &uid, None)
            .expect("equip")
            .state;
        uid
    }
}

pub const RACE_HUMAN: &str = "ID_RACE_HUMAN";
pub const RACE_DWARF: &str = "ID_SRD_RACE_DWARF";
pub const SUBRACE_HILL: &str = "ID_SUB_RACE_HILL_DWARF";
pub const ACOLYTE: &str = "ID_BACKGROUND_ACOLYTE";

pub fn class(name: &str) -> String {
    format!("ID_WOTC_PHB_CLASS_{}", name.to_uppercase())
}

pub fn skill(name: &str) -> String {
    format!(
        "ID_PROFICIENCY_SKILL_{}",
        name.to_uppercase().replace(' ', "")
    )
}
