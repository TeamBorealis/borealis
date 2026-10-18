//! Content for the `.dnd5e` truth check: A-core (the bundled packs plus the corpus's
//! `aurorabuilder__elements`) and, per character, the community repos that define the ids the
//! character needs. Repos are converted the way the app converts a dropped folder
//! (`migrate_path` -> `write_output` -> pack directory loaded on top of the bundled packs).

use borealis_content::{ContentStore, ContentStoreBuilder, PackOrigin};
use borealis_engine::Engine;
use borealis_migrate::{migrate_path, write_output, MigrateOptions};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// The A-core community repo (Aurora's own public element repository).
pub const CORE_REPO: &str = "aurorabuilder__elements";
const BASE_PACKS: &[&str] = &[
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];
/// A repo whose element ids are mostly A-core ids is a copy of the core (a mirror, or an app's
/// vendored system content), not something a user installs on top of it.
const MIRROR_SHARE: f64 = 0.25;

/// Element id -> repos defining it (`<element ... id="...">` in any `.xml` of the repo).
pub struct RepoIndex {
    pub defs: HashMap<String, BTreeSet<String>>,
    pub mirrors: BTreeSet<String>,
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Every file under `dir` (sorted, dot-entries skipped).
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, &mut out);
    out
}

/// The `id` attribute of every `<element ...>` start tag in `text`.
pub fn element_ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("<element") {
        rest = &rest[at + "<element".len()..];
        if !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if let Some(i) = tag.find(" id=\"").or_else(|| tag.find("\tid=\"")) {
            let v = &tag[i + 5..];
            if let Some(j) = v.find('"') {
                out.push(v[..j].to_string());
            }
        }
    }
    out
}

impl RepoIndex {
    pub fn build(repos: &Path) -> Self {
        let mut defs: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut names: Vec<String> = std::fs::read_dir(repos)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().to_str().map(String::from))
                    .filter(|n| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        for repo in &names {
            for f in files_under(&repos.join(repo)) {
                let is_xml = f
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("xml"));
                if !is_xml {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&f) else {
                    continue;
                };
                for id in element_ids(&String::from_utf8_lossy(&bytes)) {
                    defs.entry(id).or_default().insert(repo.clone());
                }
            }
        }
        let mut total: BTreeMap<&str, usize> = BTreeMap::new();
        let mut shared: BTreeMap<&str, usize> = BTreeMap::new();
        for repos_of in defs.values() {
            let in_core = repos_of.contains(CORE_REPO);
            for r in repos_of {
                *total.entry(r.as_str()).or_default() += 1;
                if in_core {
                    *shared.entry(r.as_str()).or_default() += 1;
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let mirrors = total
            .iter()
            .filter(|(r, n)| {
                **r != CORE_REPO
                    && shared.get(*r).copied().unwrap_or(0) as f64 / **n as f64 > MIRROR_SHARE
            })
            .map(|(r, _)| (*r).to_string())
            .collect();
        Self { defs, mirrors }
    }

    /// Greedy cover of `missing` ids by non-mirror repos (most ids first, then name).
    /// Returns the chosen repos and the ids no installable repo defines.
    pub fn cover(&self, missing: &BTreeSet<String>) -> (Vec<String>, BTreeSet<String>) {
        let mut left: BTreeSet<&String> = missing.iter().collect();
        let mut chosen = Vec::new();
        loop {
            let mut gain: BTreeMap<&str, usize> = BTreeMap::new();
            for id in &left {
                for r in self.defs.get(*id).into_iter().flatten() {
                    if r != CORE_REPO && !self.mirrors.contains(r) {
                        *gain.entry(r.as_str()).or_default() += 1;
                    }
                }
            }
            let Some((best, _)) = gain
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            else {
                break;
            };
            let best = (*best).to_string();
            left.retain(|id| !self.defs.get(*id).is_some_and(|rs| rs.contains(&best)));
            chosen.push(best);
        }
        chosen.sort();
        (chosen, left.into_iter().cloned().collect())
    }
}

fn pack_id(repo: &str) -> String {
    let s: String = repo
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("dl-{}", s.trim_matches('-'))
}

/// Converted packs and engines, cached per repo and per repo combination.
pub struct Library {
    repos: PathBuf,
    tmp: tempfile::TempDir,
    packs: HashMap<String, Option<PathBuf>>,
    engines: HashMap<Vec<String>, Option<(ContentStore, Engine)>>,
}

impl Library {
    pub fn new(repos: &Path) -> Self {
        Self {
            repos: repos.to_path_buf(),
            tmp: tempfile::tempdir().expect("tempdir"),
            packs: HashMap::new(),
            engines: HashMap::new(),
        }
    }

    /// Convert `repo` as the folder import does (cached); `None` when it does not convert.
    pub fn pack(&mut self, repo: &str) -> Option<PathBuf> {
        if let Some(p) = self.packs.get(repo) {
            return p.clone();
        }
        let id = pack_id(repo);
        let opts = MigrateOptions {
            pack_id: Some(id.clone()),
            pack_name: Some(repo.to_string()),
            origin: PackOrigin::Custom,
            ..MigrateOptions::default()
        };
        let out = migrate_path(&self.repos.join(repo), &opts)
            .map_err(|e| eprintln!("[{repo}] migrate failed: {e}"))
            .ok()
            .and_then(|out| {
                let dir = self.tmp.path().join(&id);
                write_output(&dir, &out).ok().map(|()| dir)
            });
        self.packs.insert(repo.to_string(), out.clone());
        out
    }

    /// The store and engine of bundled packs + A-core + `extra` repos (in name order).
    pub fn engine(&mut self, extra: &[String]) -> Option<&(ContentStore, Engine)> {
        let mut key: Vec<String> = vec![CORE_REPO.to_string()];
        key.extend(extra.iter().cloned());
        if !self.engines.contains_key(&key) {
            let built = self.load(&key);
            self.engines.insert(key.clone(), built);
        }
        self.engines.get(&key).and_then(Option::as_ref)
    }

    fn load(&mut self, repos: &[String]) -> Option<(ContentStore, Engine)> {
        let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for p in BASE_PACKS {
            b = b.add_pack_dir(&content.join(p)).expect("bundled pack");
        }
        for repo in repos {
            let dir = self.pack(repo)?;
            match b.clone().add_pack_dir(&dir) {
                Ok(next) => b = next,
                Err(e) => {
                    eprintln!("[{repo}] pack load failed (the app would roll back): {e}");
                    return None;
                }
            }
        }
        let store = b.build();
        let engine = Engine::new(&store);
        Some((store, engine))
    }
}
