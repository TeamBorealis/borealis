#![allow(dead_code)]
use borealis_content::{ContentFile, ContentStore, ContentStoreBuilder, PackOrigin};
use borealis_engine::Engine;
use borealis_migrate::{migrate_path, MigrateOptions};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

static ENGINE: OnceLock<Option<(ContentStore, Engine)>> = OnceLock::new();

/// Core pack plus the fixture SRD elements.
pub fn engine() -> Option<&'static Engine> {
    ENGINE
        .get_or_init(|| {
            let dir = fixtures_dir()?;
            let opts = MigrateOptions {
                origin: PackOrigin::Core,
                validate: false,
                ..MigrateOptions::default()
            };
            let out = migrate_path(&dir, &opts).ok()?;
            let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
            let mut b = ContentStoreBuilder::new().add_pack_dir(&core).ok()?;
            let mut files: Vec<_> = out
                .files
                .iter()
                .filter(|f| f.path.contains(".elements.") && f.path.ends_with(".json"))
                .collect();
            files.sort_by_key(|f| f.path.clone());
            for f in files {
                b = b.add_file(
                    f.path.clone(),
                    PackOrigin::Core,
                    ContentFile::from_json_str(&f.contents).ok()?,
                );
            }
            let store = b.build();
            let engine = Engine::new(&store);
            Some((store, engine))
        })
        .as_ref()
        .map(|(_, e)| e)
}
