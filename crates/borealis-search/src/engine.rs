//! Index lifecycle (spec section 9): manifest, incremental per-pack sync, full rebuild on version change or
//! corruption, read-only opening for the MCP process.
//!
//! Layout under `<app-data>/search/v1/`: `manifest.json`, `rules/`, `lore/` (Tantivy directories). One process
//! (the app) opens it read-write; others use [`SearchEngine::open_read_only`].

use crate::analyzer::ANALYZER_VERSION;
use crate::chunk::PackInput;
use crate::error::{Result, SearchError};
use crate::index::{CorpusIndex, Hit, WriterBudget};
use crate::query::{SearchConfig, SearchRequest};
use crate::schema::SCHEMA_VERSION;
use crate::synonyms::Synonyms;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Version of Tantivy this index format was built with (major.minor; patch releases keep the format).
pub const TANTIVY_VERSION: &str = "0.26";
const MANIFEST_FILE: &str = "manifest.json";

/// The two keyword corpora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Corpus {
    /// Rules text, conditions, glossary.
    Rules,
    /// Every element with text.
    Lore,
}

impl Corpus {
    /// Directory / manifest name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Rules => "rules",
            Self::Lore => "lore",
        }
    }

    /// Both corpora.
    pub const ALL: [Self; 2] = [Self::Rules, Self::Lore];
}

/// Pack fingerprints of one corpus.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusState {
    /// `pack id -> fingerprint` of what the index currently holds.
    #[serde(default)]
    pub packs: BTreeMap<String, String>,
}

/// `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Schema version.
    pub schema_version: u32,
    /// Analyzer / stopword version.
    pub analyzer_version: u32,
    /// Synonym table version.
    pub synonyms_version: u32,
    /// Tantivy major.minor.
    pub tantivy_version: String,
    /// Per corpus state, keyed by [`Corpus::name`].
    #[serde(default)]
    pub corpora: BTreeMap<String, CorpusState>,
    /// Reserved for the sessions corpus (S2.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions_fingerprint: Option<String>,
    /// Reserved for the vector store (S3.3): `{model_sha, dims, prompt_version}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vectors: Option<serde_json::Value>,
}

impl Manifest {
    fn current(synonyms: &Synonyms) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            analyzer_version: ANALYZER_VERSION,
            synonyms_version: synonyms.version(),
            tantivy_version: TANTIVY_VERSION.to_string(),
            corpora: BTreeMap::new(),
            sessions_fingerprint: None,
            vectors: None,
        }
    }

    /// Why this manifest cannot be used with the running code, if it cannot.
    fn mismatch(&self, synonyms: &Synonyms) -> Option<String> {
        let now = Self::current(synonyms);
        let checks = [
            ("schema_version", self.schema_version == now.schema_version),
            (
                "analyzer_version",
                self.analyzer_version == now.analyzer_version,
            ),
            (
                "synonyms_version",
                self.synonyms_version == now.synonyms_version,
            ),
            (
                "tantivy_version",
                self.tantivy_version == now.tantivy_version,
            ),
        ];
        checks
            .iter()
            .find(|(_, ok)| !ok)
            .map(|(what, _)| (*what).to_string())
    }
}

/// What [`SearchEngine::open`] had to do beyond opening.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenReport {
    /// Corpora that were wiped (version change, open error or checksum failure) and must be re-synced, with the
    /// reason.
    pub rebuilt: Vec<(Corpus, String)>,
}

/// Outcome of one [`SearchEngine::sync`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Packs that were not in the index before.
    pub added: Vec<String>,
    /// Packs whose fingerprint changed (deleted and re-added).
    pub updated: Vec<String>,
    /// Packs no longer wanted (deleted).
    pub removed: Vec<String>,
    /// Packs left untouched.
    pub unchanged: Vec<String>,
    /// Chunks written.
    pub chunks_written: usize,
}

impl SyncReport {
    /// True when the index was not modified.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }
}

/// Both keyword indexes plus the manifest.
pub struct SearchEngine {
    root: PathBuf,
    manifest: Manifest,
    rules: CorpusIndex,
    lore: CorpusIndex,
    synonyms: Synonyms,
    budget: WriterBudget,
    writable: bool,
}

fn read_manifest(root: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(root.join(MANIFEST_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_manifest(root: &Path, manifest: &Manifest) -> Result<()> {
    let tmp = root.join(format!("{MANIFEST_FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(manifest)?)?;
    std::fs::rename(&tmp, root.join(MANIFEST_FILE))?;
    Ok(())
}

fn wipe(dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn open_checked(dir: &Path) -> Result<CorpusIndex> {
    let index = CorpusIndex::open_or_create(dir)?;
    index.verify()?;
    Ok(index)
}

impl SearchEngine {
    /// Open (creating when needed) the indexes under `root` for the app process. A manifest with another
    /// schema/analyzer/synonym/Tantivy version wipes both corpora; an index that fails to open or fails its
    /// checksums is wiped and recreated empty (its manifest entry is cleared), so the next [`sync`](Self::sync)
    /// rebuilds it.
    ///
    /// # Errors
    /// I/O errors that even a wipe cannot fix.
    pub fn open(root: &Path, budget: WriterBudget) -> Result<(Self, OpenReport)> {
        std::fs::create_dir_all(root)?;
        let synonyms = Synonyms::bundled();
        let mut report = OpenReport::default();
        let mut manifest = match read_manifest(root) {
            Some(m) => match m.mismatch(&synonyms) {
                None => m,
                Some(what) => {
                    for c in Corpus::ALL {
                        wipe(&root.join(c.name()))?;
                        report.rebuilt.push((c, format!("{what} changed")));
                    }
                    Manifest::current(&synonyms)
                }
            },
            None => {
                // No manifest: whatever is on disk is of unknown provenance.
                for c in Corpus::ALL {
                    if root.join(c.name()).exists() {
                        wipe(&root.join(c.name()))?;
                        report
                            .rebuilt
                            .push((c, "manifest missing or unreadable".into()));
                    }
                }
                Manifest::current(&synonyms)
            }
        };
        let rules = Self::open_corpus(root, Corpus::Rules, &mut manifest, &mut report)?;
        let lore = Self::open_corpus(root, Corpus::Lore, &mut manifest, &mut report)?;
        write_manifest(root, &manifest)?;
        let engine = Self {
            root: root.to_path_buf(),
            manifest,
            rules,
            lore,
            synonyms,
            budget,
            writable: true,
        };
        Ok((engine, report))
    }

    fn open_corpus(
        root: &Path,
        corpus: Corpus,
        manifest: &mut Manifest,
        report: &mut OpenReport,
    ) -> Result<CorpusIndex> {
        let dir = root.join(corpus.name());
        match open_checked(&dir) {
            Ok(index) => Ok(index),
            Err(e) => {
                tracing::warn!(corpus = corpus.name(), error = %e, "search index damaged, rebuilding");
                wipe(&dir)?;
                manifest.corpora.remove(corpus.name());
                report.rebuilt.push((corpus, e.to_string()));
                open_checked(&dir)
            }
        }
    }

    /// Open the indexes read-only (MCP process): no writer, no manifest writes, no wiping.
    ///
    /// # Errors
    /// [`SearchError::NotBuilt`] when the app has not built the indexes, [`SearchError::Stale`] when they were
    /// built by other versions.
    pub fn open_read_only(root: &Path) -> Result<Self> {
        let synonyms = Synonyms::bundled();
        let manifest = read_manifest(root).ok_or(SearchError::NotBuilt)?;
        if let Some(what) = manifest.mismatch(&synonyms) {
            return Err(SearchError::Stale(what));
        }
        Ok(Self {
            root: root.to_path_buf(),
            rules: CorpusIndex::open_read_only(&root.join(Corpus::Rules.name()))?,
            lore: CorpusIndex::open_read_only(&root.join(Corpus::Lore.name()))?,
            manifest,
            synonyms,
            budget: WriterBudget::MOBILE,
            writable: false,
        })
    }

    /// The manifest as last written.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The index of one corpus.
    #[must_use]
    pub fn corpus_index(&self, corpus: Corpus) -> &CorpusIndex {
        match corpus {
            Corpus::Rules => &self.rules,
            Corpus::Lore => &self.lore,
        }
    }

    /// The synonym table in use.
    #[must_use]
    pub fn synonyms(&self) -> &Synonyms {
        &self.synonyms
    }

    /// Bring one corpus in line with `wanted`: packs missing from the index are added, packs whose fingerprint
    /// changed are deleted and re-added, packs absent from `wanted` are deleted; unchanged packs are not touched.
    /// One commit, then the manifest is rewritten atomically (a crash in between only causes a harmless
    /// re-replace on the next run).
    ///
    /// # Errors
    /// Index or I/O errors, or a read-only engine.
    pub fn sync(&mut self, corpus: Corpus, wanted: &[PackInput]) -> Result<SyncReport> {
        if !self.writable {
            return Err(SearchError::Index("engine opened read-only".into()));
        }
        let current = self
            .manifest
            .corpora
            .get(corpus.name())
            .cloned()
            .unwrap_or_default();
        let mut report = SyncReport::default();
        let mut replace: Vec<PackInput> = Vec::new();
        for pack in wanted {
            match current.packs.get(&pack.pack) {
                None => report.added.push(pack.pack.clone()),
                Some(fp) if *fp != pack.fingerprint => report.updated.push(pack.pack.clone()),
                Some(_) => {
                    report.unchanged.push(pack.pack.clone());
                    continue;
                }
            }
            report.chunks_written += pack.chunks.len();
            replace.push(pack.clone());
        }
        report.removed = current
            .packs
            .keys()
            .filter(|id| !wanted.iter().any(|p| &p.pack == *id))
            .cloned()
            .collect();
        if report.is_noop() {
            return Ok(report);
        }
        self.corpus_index(corpus)
            .apply(&report.removed, &replace, self.budget)?;
        let state = self
            .manifest
            .corpora
            .entry(corpus.name().to_string())
            .or_default();
        for id in &report.removed {
            state.packs.remove(id);
        }
        for pack in &replace {
            state
                .packs
                .insert(pack.pack.clone(), pack.fingerprint.clone());
        }
        write_manifest(&self.root, &self.manifest)?;
        Ok(report)
    }

    /// Search one corpus.
    ///
    /// # Errors
    /// Index errors.
    pub fn search(
        &self,
        corpus: Corpus,
        req: &SearchRequest,
        cfg: &SearchConfig,
    ) -> Result<Vec<Hit>> {
        self.corpus_index(corpus).search(req, cfg, &self.synonyms)
    }
}
