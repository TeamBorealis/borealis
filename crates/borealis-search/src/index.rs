//! One Tantivy index (one corpus): write path (per-pack replace/remove, one commit) and read path (search).

use crate::analyzer::{self, normalize_name, prefix_grams};
use crate::chunk::{Chunk, EditionTag, PackInput};
use crate::error::{Result, SearchError};
use crate::query::{build_query, Edition, EditionScope, SearchConfig, SearchRequest};
use crate::schema::{self, Fields};
use crate::synonyms::Synonyms;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tantivy::collector::{Count, TopDocs};
use tantivy::directory::MmapDirectory;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

/// Longest snippet returned in a [`Hit`], in characters.
pub const SNIPPET_CHARS: usize = 220;
/// Candidate pool factor: the pool is `(offset + limit) * factor`, at least [`MIN_POOL`].
const POOL_FACTOR: usize = 4;
const MIN_POOL: usize = 100;

/// Writer resources (spec section 2, risk 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterBudget {
    /// Indexing threads.
    pub threads: usize,
    /// Total heap in bytes (Tantivy needs 15 MB per thread at least).
    pub heap_bytes: usize,
}

impl WriterBudget {
    /// Desktop: 2 threads / 64 MB.
    pub const DESKTOP: Self = Self {
        threads: 2,
        heap_bytes: 64_000_000,
    };
    /// Mobile: 1 thread / 20 MB.
    pub const MOBILE: Self = Self {
        threads: 1,
        heap_bytes: 20_000_000,
    };
}

/// Whether this handle may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The app process: may take the writer lock.
    ReadWrite,
    /// The MCP process: never creates a writer; the reader reloads before each query.
    ReadOnly,
}

/// One search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Citation id.
    pub doc_key: String,
    /// Title or element name.
    pub title: String,
    /// Heading path or element type.
    pub path: String,
    /// Element type.
    pub ty: String,
    /// `2014`, `2024` or `none`.
    pub ruleset: String,
    /// Pack id.
    pub pack: String,
    /// Source name.
    pub source: String,
    /// Plain-text excerpt (at most [`SNIPPET_CHARS`] characters), never HTML.
    pub snippet: String,
    /// `[start, end)` UTF-8 byte offsets of the matches inside `snippet`.
    pub highlights: Vec<(usize, usize)>,
    /// Final score (after the preferred-edition multiplier).
    pub score: f32,
}

/// One stored chunk read back by its exact `doc_key` (the `get_passage` MCP tool).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    /// The key it was found under.
    pub doc_key: String,
    /// Title or element name.
    pub title: String,
    /// Heading path or element type.
    pub path: String,
    /// Element type.
    pub ty: String,
    /// `2014`, `2024` or `none`.
    pub ruleset: String,
    /// Pack id.
    pub pack: String,
    /// Source name.
    pub source: String,
    /// Campaign id (sessions only).
    pub campaign: Option<String>,
    /// The full plain text of the chunk.
    pub body: String,
}

/// An opened corpus index.
pub struct CorpusIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
    mode: Mode,
    dir: PathBuf,
}

fn open_dir(dir: &Path) -> Result<MmapDirectory> {
    Ok(MmapDirectory::open(dir)?)
}

impl CorpusIndex {
    /// Open the index in `dir`, creating it when missing (read-write).
    ///
    /// # Errors
    /// I/O errors, or a Tantivy error when the directory holds an incompatible or damaged index (the caller wipes
    /// and rebuilds).
    pub fn open_or_create(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let (schema, fields) = schema::build();
        let index = Index::open_or_create(open_dir(dir)?, schema)?;
        Self::finish(index, fields, Mode::ReadWrite, dir)
    }

    /// Open an existing index without ever creating a writer (`search` reloads first).
    ///
    /// # Errors
    /// [`SearchError::NotBuilt`] when `dir` holds no index.
    pub fn open_read_only(dir: &Path) -> Result<Self> {
        if !dir.is_dir() || !Index::exists(&open_dir(dir)?).unwrap_or(false) {
            return Err(SearchError::NotBuilt);
        }
        let (_, fields) = schema::build();
        let index = Index::open_in_dir(dir)?;
        Self::finish(index, fields, Mode::ReadOnly, dir)
    }

    /// Both modes reload by hand (`ReloadPolicy::Manual`): [`apply`](Self::apply), the only writer, reloads
    /// right after its commit, and a read-only handle reloads before each query. Tantivy's
    /// `OnCommitWithDelay` would start a meta-file watcher thread whose first poll reloads the reader on a
    /// background thread holding its own reference to the index, so the segment files stayed open (mapped)
    /// for a moment after this handle was dropped. Windows refuses to remove a folder whose files are still
    /// open, which broke wiping a damaged index right after its checksum failed (`DirectoryNotEmpty`). With
    /// the manual policy no thread outlives the handle: dropping it closes every file.
    fn finish(index: Index, fields: Fields, mode: Mode, dir: &Path) -> Result<Self> {
        analyzer::register(index.tokenizers());
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            index,
            reader,
            fields,
            mode,
            dir: dir.to_path_buf(),
        })
    }

    /// The directory of this index.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Verify every index file against its checksum (detects truncated or bit-rotten files).
    ///
    /// # Errors
    /// [`SearchError::Index`] naming the damaged files.
    pub fn verify(&self) -> Result<()> {
        let damaged = self.index.validate_checksum()?;
        if damaged.is_empty() {
            return Ok(());
        }
        let mut names: Vec<String> = damaged.iter().map(|p| p.display().to_string()).collect();
        names.sort();
        Err(SearchError::Index(format!(
            "checksum mismatch in {}",
            names.join(", ")
        )))
    }

    /// Reload the reader so it sees the latest commit (called automatically before each read-only query).
    ///
    /// # Errors
    /// Tantivy reload errors.
    pub fn reload(&self) -> Result<()> {
        self.reader.reload()?;
        Ok(())
    }

    /// Number of live documents.
    #[must_use]
    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// Number of live documents of one pack.
    ///
    /// # Errors
    /// Tantivy search errors.
    pub fn pack_doc_count(&self, pack: &str) -> Result<usize> {
        let q = TermQuery::new(
            Term::from_field_text(self.fields.pack, pack),
            IndexRecordOption::Basic,
        );
        Ok(self.reader.searcher().search(&q, &Count)?)
    }

    /// Size on disk of the index files, in bytes.
    #[must_use]
    pub fn disk_bytes(&self) -> u64 {
        std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(std::result::Result::ok)
                    .filter_map(|e| e.metadata().ok())
                    .filter(std::fs::Metadata::is_file)
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    fn document(&self, c: &Chunk) -> TantivyDocument {
        let f = &self.fields;
        let mut doc = TantivyDocument::default();
        doc.add_text(f.doc_key, &c.doc_key);
        doc.add_text(f.name, &c.name);
        doc.add_text(f.headings, &c.headings);
        doc.add_text(f.body, &c.body);
        if c.is_primary {
            doc.add_text(f.name_exact, normalize_name(&c.name));
            doc.add_text(f.name_prefix, prefix_grams(&c.name));
            doc.add_text(f.primary, "1");
        }
        doc.add_text(f.ruleset, c.edition.as_str());
        doc.add_text(f.ty, &c.ty);
        doc.add_text(f.pack, &c.pack);
        doc.add_text(f.source, &c.source);
        if let Some(campaign) = &c.campaign {
            doc.add_text(f.campaign, campaign);
        }
        doc.add_text(f.content_hash, c.content_hash());
        doc
    }

    /// Delete every document of the packs in `remove`, delete and re-add the packs in `replace`, all under one
    /// commit. Other packs are not touched.
    ///
    /// # Errors
    /// [`SearchError::Index`] when this handle is read-only or Tantivy fails.
    pub fn apply(
        &self,
        remove: &[String],
        replace: &[PackInput],
        budget: WriterBudget,
    ) -> Result<()> {
        if self.mode == Mode::ReadOnly {
            return Err(SearchError::Index("index opened read-only".into()));
        }
        if remove.is_empty() && replace.is_empty() {
            return Ok(());
        }
        let mut writer: IndexWriter = self
            .index
            .writer_with_num_threads(budget.threads, budget.heap_bytes)?;
        for pack in remove.iter().chain(replace.iter().map(|p| &p.pack)) {
            writer.delete_term(Term::from_field_text(self.fields.pack, pack));
        }
        for input in replace {
            for chunk in &input.chunks {
                writer.add_document(self.document(chunk))?;
            }
        }
        writer.commit()?;
        // Join the merge threads the commit may have started, so no background merge still holds segment
        // files once this returns (the folder can then be removed at once, even on Windows).
        writer.wait_merging_threads()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Search with `cfg`. Hits are ordered by descending score, ties by `doc_key`.
    ///
    /// # Errors
    /// Tantivy errors.
    pub fn search(
        &self,
        req: &SearchRequest,
        cfg: &SearchConfig,
        synonyms: &Synonyms,
    ) -> Result<Vec<Hit>> {
        if self.mode == Mode::ReadOnly {
            self.reader.reload()?;
        }
        let Some(query) = build_query(&self.fields, self.index.tokenizers(), req, cfg, synonyms)
        else {
            return Ok(Vec::new());
        };
        let searcher = self.reader.searcher();
        let wanted = req.offset.saturating_add(req.limit);
        let pool = wanted.saturating_mul(POOL_FACTOR).max(MIN_POOL);
        let top = searcher.search(&query, &TopDocs::with_limit(pool).order_by_score())?;
        let mut snippets = SnippetGenerator::create(&searcher, &*query, self.fields.body)?;
        snippets.set_max_num_chars(SNIPPET_CHARS);
        let preferred = preferred_tag(req, cfg);
        let mut hits = Vec::with_capacity(top.len());
        for (score, address) in top {
            let doc: TantivyDocument = searcher.doc(address)?;
            let mut hit = self.hit_from(&doc, &snippets, score);
            if preferred == Some(EditionTag::from_stored(&hit.ruleset)) {
                hit.score *= cfg.weights.preferred_edition;
            }
            hits.push(hit);
        }
        hits.sort_by(compare_hits);
        if req.collapse {
            hits = collapse(hits);
        }
        Ok(hits.into_iter().skip(req.offset).take(req.limit).collect())
    }

    /// The chunk stored under exactly `doc_key`, if any.
    ///
    /// # Errors
    /// Tantivy errors.
    pub fn passage(&self, doc_key: &str) -> Result<Option<Passage>> {
        if self.mode == Mode::ReadOnly {
            self.reader.reload()?;
        }
        let searcher = self.reader.searcher();
        let term = Term::from_field_text(self.fields.doc_key, doc_key);
        let query = TermQuery::new(term, IndexRecordOption::Basic);
        let top = searcher.search(&query, &TopDocs::with_limit(1).order_by_score())?;
        let Some((_, address)) = top.into_iter().next() else {
            return Ok(None);
        };
        let doc: TantivyDocument = searcher.doc(address)?;
        let f = &self.fields;
        let text = |field| {
            doc.get_first(field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let campaign = text(f.campaign);
        Ok(Some(Passage {
            doc_key: text(f.doc_key),
            title: text(f.name),
            path: text(f.headings),
            ty: text(f.ty),
            ruleset: text(f.ruleset),
            pack: text(f.pack),
            source: text(f.source),
            campaign: (!campaign.is_empty()).then_some(campaign),
            body: text(f.body),
        }))
    }

    fn hit_from(&self, doc: &TantivyDocument, snippets: &SnippetGenerator, score: f32) -> Hit {
        let f = &self.fields;
        let text = |field| {
            doc.get_first(field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let body = text(f.body);
        let snippet = snippets.snippet_from_doc(doc);
        let (snippet_text, highlights) = if snippet.fragment().is_empty() {
            (body.chars().take(SNIPPET_CHARS).collect(), Vec::new())
        } else {
            (
                snippet.fragment().to_string(),
                snippet
                    .highlighted()
                    .iter()
                    .map(|r| (r.start, r.end))
                    .collect(),
            )
        };
        Hit {
            doc_key: text(f.doc_key),
            title: text(f.name),
            path: text(f.headings),
            ty: text(f.ty),
            ruleset: text(f.ruleset),
            pack: text(f.pack),
            source: text(f.source),
            snippet: snippet_text,
            highlights,
            score,
        }
    }
}

fn preferred_tag(req: &SearchRequest, cfg: &SearchConfig) -> Option<EditionTag> {
    match (cfg.tweaks.edition, req.edition) {
        (
            true,
            EditionScope::Both {
                preferred: Some(edition),
            },
        ) => Some(Edition::tag(edition)),
        _ => None,
    }
}

fn compare_hits(a: &Hit, b: &Hit) -> Ordering {
    b.score
        .total_cmp(&a.score)
        .then_with(|| a.doc_key.cmp(&b.doc_key))
}

/// The document key without its `#n` chunk suffix.
#[must_use]
pub fn base_key(doc_key: &str) -> &str {
    match doc_key.rsplit_once('#') {
        Some((base, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => doc_key,
    }
}

fn collapse(hits: Vec<Hit>) -> Vec<Hit> {
    let mut seen = HashSet::new();
    hits.into_iter()
        .filter(|h| seen.insert(base_key(&h.doc_key).to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_key_strips_only_numeric_chunk_suffixes() {
        assert_eq!(base_key("ID_X#2"), "ID_X");
        assert_eq!(base_key("rules:p/a/b#12"), "rules:p/a/b");
        assert_eq!(base_key("ID_X#frag"), "ID_X#frag");
        assert_eq!(base_key("ID_X"), "ID_X");
    }
}
