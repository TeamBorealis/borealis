//! Shared fixtures for the golden tests. Aurora XML fixtures are migrated on the fly and are
//! never copied into the repository; tests are skipped (with a notice) when they are absent.
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

pub fn engine(v: Variant) -> Option<&'static Engine> {
    let cell = match v {
        Variant::Fixture => &FIXTURE,
        Variant::CorePack => &COREPACK,
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
        }
        let store = b.build();
        let engine = Engine::new(&store);
        Some((store, engine))
    })
    .as_ref()
    .map(|(_, e)| e)
}

/// Run a test body against both content variants (skips with a notice without fixtures).
pub fn each_variant(name: &str, body: impl Fn(&Engine, Variant)) {
    let mut ran = false;
    for v in [Variant::Fixture, Variant::CorePack] {
        match engine(v) {
            Some(e) => {
                body(e, v);
                ran = true;
            }
            None => eprintln!("SKIPPED {name}: fixtures/aurora-xml not found"),
        }
    }
    let _ = ran;
}

/// A character under construction with the same operations a user would perform.
pub struct Hero<'e> {
    pub e: &'e Engine,
    pub s: CharacterState,
}

pub const LEVEL1: &str = "ID_LEVEL_1";

impl<'e> Hero<'e> {
    pub fn new(e: &'e Engine, average_hp: bool) -> Self {
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
