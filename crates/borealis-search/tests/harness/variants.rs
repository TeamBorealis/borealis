//! Variant definitions (V0..V4, ablations) and the shared search fixture over the bundled SRD corpora.
use super::data::{canon, EvalQuery};
use super::Ranker;
use crate::common;
use borealis_content::ContentStore;
use borealis_search::{
    Chunk, Corpus, Edition, EditionScope, PackInput, SearchConfig, SearchEngine, SearchRequest,
    Tweaks, Weights, WriterBudget,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::str::FromStr;

/// Name occurrences in the V2b combined field (tweak 1 weight).
const BM25F_NAME_REPEATS: usize = 4;
/// Heading occurrences in the V2b combined field.
const BM25F_HEADING_REPEATS: usize = 2;
pub const TWEAK_COUNT: u8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantId {
    /// Current `borealis-ai` BM25 (provided by the example, which can link `borealis-ai`).
    V0,
    /// Tantivy multi-field BM25, no tweaks.
    V1,
    /// V1 plus tweaks 1-7.
    V2,
    /// V2 with tweak 1 replaced by a true-BM25F style combined field.
    V2b,
    /// V2 minus tweak `k` (1-7).
    Ablation(u8),
    /// Exactly the tweaks in the bit set (bit `k - 1` = tweak `k`), written `T3,4,6` (used by the selection search).
    Custom(u8),
    /// Like `Custom` on the combined-field (true BM25F emulation) index, written `B3,4,6,7`.
    Bm25f(u8),
    /// BMX re-scorer (S1.4). Hook only.
    V3,
    /// Hybrid with embeddings (S3.3). Hook only.
    V4,
}

impl VariantId {
    pub fn name(self) -> String {
        match self {
            Self::V0 => "V0".into(),
            Self::V1 => "V1".into(),
            Self::V2 => "V2".into(),
            Self::V2b => "V2b".into(),
            Self::Ablation(k) => format!("V2-t{k}"),
            Self::Custom(mask) => custom_name(mask, 'T'),
            Self::Bm25f(mask) => custom_name(mask, 'B'),
            Self::V3 => "V3".into(),
            Self::V4 => "V4".into(),
        }
    }

    /// Every variant that runs today, in report order.
    pub fn runnable() -> Vec<Self> {
        let mut out = vec![Self::V0, Self::V1, Self::V2, Self::V2b];
        out.extend((1..=TWEAK_COUNT).map(Self::Ablation));
        out
    }

    /// The tweak set of a Tantivy variant.
    pub fn tweaks(self) -> Option<Tweaks> {
        match self {
            Self::V1 => Some(Tweaks::none()),
            Self::V2 | Self::V2b => Some(Tweaks::ALL),
            Self::Ablation(k) => Some(without(k)),
            Self::Custom(mask) | Self::Bm25f(mask) => Some(from_mask(mask)),
            Self::V0 | Self::V3 | Self::V4 => None,
        }
    }
}

impl FromStr for VariantId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "V0" => Ok(Self::V0),
            "V1" => Ok(Self::V1),
            "V2" => Ok(Self::V2),
            "V2b" => Ok(Self::V2b),
            "V3" => Ok(Self::V3),
            "V4" => Ok(Self::V4),
            other if other.starts_with('T') => parse_custom(&other[1..]).map(Self::Custom),
            other if other.starts_with('B') => parse_custom(&other[1..]).map(Self::Bm25f),
            other => other
                .strip_prefix("V2-t")
                .and_then(|k| k.parse::<u8>().ok())
                .filter(|k| (1..=TWEAK_COUNT).contains(k))
                .map(Self::Ablation)
                .ok_or_else(|| format!("unknown variant {other:?}")),
        }
    }
}

fn custom_name(mask: u8, tag: char) -> String {
    let list: Vec<String> = (1..=TWEAK_COUNT)
        .filter(|k| mask & (1 << (k - 1)) != 0)
        .map(|k| k.to_string())
        .collect();
    format!("{tag}{}", list.join(","))
}

fn parse_custom(list: &str) -> Result<u8, String> {
    let mut mask = 0u8;
    for part in list.split(',').filter(|p| !p.is_empty()) {
        let k: u8 = part.parse().map_err(|_| format!("bad tweak {part:?}"))?;
        if !(1..=TWEAK_COUNT).contains(&k) {
            return Err(format!("tweak {k} out of range"));
        }
        mask |= 1 << (k - 1);
    }
    Ok(mask)
}

/// The tweak set with exactly the tweaks of `mask` on.
pub fn from_mask(mask: u8) -> Tweaks {
    let on = |k: u8| mask & (1 << (k - 1)) != 0;
    Tweaks {
        field_weights: on(1),
        exact_name: on(2),
        prefix: on(3),
        typo: on(4),
        phrase: on(5),
        synonyms: on(6),
        edition: on(7),
    }
}

/// All tweaks on except number `k`.
pub fn without(k: u8) -> Tweaks {
    let mut t = Tweaks::ALL;
    match k {
        1 => t.field_weights = false,
        2 => t.exact_name = false,
        3 => t.prefix = false,
        4 => t.typo = false,
        5 => t.phrase = false,
        6 => t.synonyms = false,
        7 => t.edition = false,
        _ => {}
    }
    t
}

/// Shared indexes: the combined SRD corpus (Lore elements plus rules-text sections in one index, so IDF is shared
/// the way an "All" search would see it) and the V2b combined-field copy of it.
pub struct Fixture {
    engine: Rc<SearchEngine>,
    bm25f: Rc<SearchEngine>,
    /// Canonical key -> (title, plain text) of every indexed document.
    pub docs: HashMap<String, (String, String)>,
    pub store: ContentStore,
    _dir: tempfile::TempDir,
}

fn is_rules_text(pack: &PackInput) -> bool {
    pack.pack.ends_with("-rules-text")
}

fn bm25f_chunk(c: &Chunk) -> Chunk {
    let mut out = c.clone();
    let mut body = String::new();
    for _ in 0..BM25F_NAME_REPEATS {
        body.push_str(&c.name);
        body.push(' ');
    }
    for _ in 0..BM25F_HEADING_REPEATS {
        body.push_str(&c.headings);
        body.push(' ');
    }
    body.push_str(&c.body);
    out.body = body;
    out
}

impl Fixture {
    pub fn build() -> Self {
        let store = common::srd_store();
        let (rules, lore) = common::srd_inputs(&store);
        let mut packs: Vec<PackInput> = lore;
        packs.extend(rules.into_iter().filter(is_rules_text));
        let mut docs: HashMap<String, (String, String)> = HashMap::new();
        for chunk in packs.iter().flat_map(|p| &p.chunks) {
            let key = canon(&chunk.doc_key);
            let is_first = chunk.doc_key == key || chunk.doc_key.ends_with("#1");
            if is_first || !docs.contains_key(key) {
                docs.insert(key.to_string(), (chunk.name.clone(), chunk.body.clone()));
            }
        }
        let bm25f_packs: Vec<PackInput> = packs
            .iter()
            .map(|p| PackInput::new(p.pack.clone(), p.chunks.iter().map(bm25f_chunk).collect()))
            .collect();
        let dir = tempfile::tempdir().expect("temp dir");
        let engine = open_synced(&dir.path().join("plain"), &packs);
        let bm25f = open_synced(&dir.path().join("bm25f"), &bm25f_packs);
        Self {
            engine: Rc::new(engine),
            bm25f: Rc::new(bm25f),
            docs,
            store,
            _dir: dir,
        }
    }

    /// Every canonical key in the corpus.
    pub fn keys(&self) -> HashSet<&str> {
        self.docs.keys().map(String::as_str).collect()
    }

    /// A Tantivy ranker with explicit tweaks and weights (the field-weight sweep).
    pub fn ranker_custom(&self, label: &str, tweaks: Tweaks, weights: Weights) -> Box<dyn Ranker> {
        Box::new(TantivyRanker {
            label: label.to_string(),
            engine: Rc::clone(&self.engine),
            cfg: SearchConfig { tweaks, weights },
        })
    }

    /// The ranker of a Tantivy variant. `V0` (needs `borealis-ai`) and the S1.4/S3.3 hooks are `Err`.
    pub fn ranker(&self, id: VariantId) -> Result<Box<dyn Ranker>, String> {
        let tweaks = match id {
            VariantId::V0 => return Err("V0 is provided by the example (links borealis-ai)".into()),
            VariantId::V3 => return Err("V3 (BMX) is work package S1.4".into()),
            VariantId::V4 => return Err("V4 (embeddings) is work package S3.3".into()),
            other => other.tweaks().expect("tantivy variant"),
        };
        let (engine, weights) = if matches!(id, VariantId::V2b | VariantId::Bm25f(_)) {
            (
                &self.bm25f,
                Weights {
                    name: 0.0,
                    headings: 0.0,
                    body: 1.0,
                    ..Weights::default()
                },
            )
        } else {
            (&self.engine, Weights::default())
        };
        Ok(Box::new(TantivyRanker {
            label: id.name(),
            engine: Rc::clone(engine),
            cfg: SearchConfig { tweaks, weights },
        }))
    }
}

fn open_synced(path: &std::path::Path, packs: &[PackInput]) -> SearchEngine {
    let (mut engine, _) = SearchEngine::open(path, WriterBudget::DESKTOP).expect("open index");
    engine.sync(Corpus::Lore, packs).expect("sync");
    engine
}

struct TantivyRanker {
    label: String,
    engine: Rc<SearchEngine>,
    cfg: SearchConfig,
}

/// Edition of a query (edition-filtered queries and the few that name one).
pub fn edition_of(q: &EvalQuery) -> Option<Edition> {
    match q.ruleset.as_deref() {
        Some("2014") => Some(Edition::Y2014),
        Some("2024") => Some(Edition::Y2024),
        _ => None,
    }
}

impl Ranker for TantivyRanker {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn rank(&self, q: &EvalQuery, k: usize) -> Vec<String> {
        let mut req = SearchRequest::new(q.text.clone());
        req.limit = k;
        if let Some(edition) = edition_of(q) {
            req.edition = EditionScope::Only(edition);
        }
        let hits = self
            .engine
            .search(Corpus::Lore, &req, &self.cfg)
            .expect("search");
        let mut seen = HashSet::new();
        hits.into_iter()
            .map(|h| canon(&h.doc_key).to_string())
            .filter(|key| seen.insert(key.clone()))
            .take(k)
            .collect()
    }
}
