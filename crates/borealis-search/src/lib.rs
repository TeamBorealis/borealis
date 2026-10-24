//! Local keyword search over rules text and lore content (Tantivy). Work package S1.2 of
//! `specs/local-search-spec.md`.
//!
//! * [`corpus`]: Rules corpus (rules-text packs plus Condition / glossary elements) and Lore corpus (every element
//!   with text) as [`PackInput`]s of [`Chunk`]s.
//! * [`engine::SearchEngine`]: manifest lifecycle, incremental per-pack sync, rebuild on corruption, read-only
//!   open for the MCP process, [`SearchEngine::search`].
//! * [`query`]: the ranking tweaks 1-7, each behind a flag of [`Tweaks`].
//!
//! Nothing here is wired into the app yet (S1.5, S1.6). Data stays local (C4): the crate never touches the network.

pub mod analyzer;
pub mod chunk;
pub mod corpus;
pub mod engine;
pub mod error;
pub mod index;
pub mod query;
pub mod schema;
pub mod synonyms;
pub mod text;

pub use chunk::{Chunk, EditionTag, PackInput};
pub use engine::{Corpus, Manifest, OpenReport, SearchEngine, SyncReport};
pub use error::{Result, SearchError};
pub use index::{CorpusIndex, Hit, WriterBudget};
pub use query::{Edition, EditionScope, SearchConfig, SearchRequest, Tweaks, Weights};
pub use synonyms::Synonyms;
