//! Tool registry: schemas, argument parsing, dispatch and output caps.

mod element;
mod glossary;
mod ids;
mod overlay;
mod passage;
mod schemas;
mod search;
mod sessions;
mod sweep;
mod validate;

use std::collections::HashSet;
use std::sync::OnceLock;

use borealis_ai::retrieval::RetrievalIndex;
use borealis_ai::task::JsonSchemaValidator;
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use borealis_search::SearchEngine;

use crate::config::{load_base, open_search, McpConfig, SessionScope};

pub use sessions::{NoSessions, SessionChunk, SessionSource};

/// Hard cap on the text of one tool result.
pub const MAX_OUTPUT_CHARS: usize = 24_000;

/// The loaded content plus lazily built helpers.
pub struct Tools {
    pub(crate) builder: ContentStoreBuilder,
    pub(crate) store: ContentStore,
    index: OnceLock<RetrievalIndex>,
    schema: OnceLock<JsonSchemaValidator>,
    baseline: OnceLock<HashSet<String>>,
    search_dir: Option<std::path::PathBuf>,
    /// Index pack ids this run may return (`None` = every pack).
    allowed_packs: Option<Vec<String>>,
    engine: OnceLock<Option<SearchEngine>>,
    pub(crate) scope: SessionScope,
    pub(crate) sessions: Box<dyn SessionSource>,
}

impl Tools {
    /// Load the configured packs.
    ///
    /// # Errors
    /// A message when the core pack cannot be loaded.
    pub fn load(config: &McpConfig) -> Result<Self, String> {
        let (builder, store) = load_base(config)?;
        Ok(Self {
            builder,
            store,
            index: OnceLock::new(),
            schema: OnceLock::new(),
            baseline: OnceLock::new(),
            search_dir: config.search_dir.clone(),
            allowed_packs: config.enabled_packs.as_ref().map(index_packs),
            engine: OnceLock::new(),
            scope: config.session_scope.clone(),
            sessions: Box::new(NoSessions),
        })
    }

    /// Use `sessions` as the source of campaign notes (reachable only inside the session scope).
    #[must_use]
    pub fn with_sessions(mut self, sessions: Box<dyn SessionSource>) -> Self {
        self.sessions = sessions;
        self
    }

    /// The app's search index, opened read-only on first use. `None` when it does not exist yet
    /// or cannot be used; the search tools then answer from the in-memory content index.
    /// The packs the index path may return, if restricted.
    pub(crate) fn allowed_packs(&self) -> Option<&[String]> {
        self.allowed_packs.as_deref()
    }

    /// Whether a pack id of the index is enabled for this run.
    pub(crate) fn pack_allowed(&self, pack: &str) -> bool {
        self.allowed_packs
            .as_ref()
            .is_none_or(|list| list.iter().any(|p| p == pack))
    }

    pub(crate) fn search_engine(&self) -> Option<&SearchEngine> {
        self.engine
            .get_or_init(|| {
                let dir = self.search_dir.as_deref()?;
                match open_search(dir) {
                    Ok(engine) => Some(engine),
                    Err(borealis_search::SearchError::NotBuilt) => None,
                    Err(e) => {
                        eprintln!("borealis-mcp: search index unavailable: {e}");
                        None
                    }
                }
            })
            .as_ref()
    }

    /// Number of loaded elements.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.store.len()
    }

    pub(crate) fn index(&self) -> &RetrievalIndex {
        self.index
            .get_or_init(|| RetrievalIndex::build(&self.store))
    }

    pub(crate) fn schema(&self) -> &JsonSchemaValidator {
        self.schema.get_or_init(JsonSchemaValidator::content_format)
    }

    /// Fingerprints of the issues the unmodified base content already has.
    pub(crate) fn baseline(&self) -> &HashSet<String> {
        self.baseline.get_or_init(|| {
            self.store
                .validate()
                .iter()
                .chain(self.store.diagnostics())
                .map(overlay::fingerprint)
                .collect()
        })
    }

    /// `tools/list` entries in deterministic order; `with_output_schema` adds `outputSchema`.
    #[must_use]
    pub fn list(&self, with_output_schema: bool) -> Value {
        schemas::tool_list(with_output_schema, self.scope.campaign().is_some())
    }

    /// Run a tool. `None` when the tool does not exist; otherwise the MCP tool result
    /// (`content`, `isError`, and `structuredContent` when `structured` and successful).
    #[must_use]
    pub fn call(&self, name: &str, args: &Value, structured: bool) -> Option<Value> {
        let outcome = match name {
            "search_content" => search::content(self, args),
            "search_rules" => search::rules(self, args),
            "search_lore" => search::lore(self, args),
            // Without a session scope the tool does not exist: same answer as for any unknown name.
            schemas::SEARCH_SESSIONS if self.scope.campaign().is_none() => return None,
            schemas::SEARCH_SESSIONS => sessions::run(self, args),
            "get_passage" => passage::run(self, args),
            "get_element" => element::run(self, args),
            "list_core_ids" => ids::run(self, args),
            "validate_element" => validate::run(self, args),
            "class_sweep" => sweep::run(self, args),
            "explain_rules_term" => glossary::run(self, args),
            _ => return None,
        };
        Some(tool_result(outcome, structured))
    }
}

/// Index pack ids for the enabled content packs: `core`, each enabled pack, and the rules-text
/// pack that belongs to it (`<pack>-rules-text`).
fn index_packs(enabled: &std::collections::BTreeSet<String>) -> Vec<String> {
    let mut out = vec![crate::config::CORE_PACK.to_string()];
    for id in enabled {
        out.push(id.clone());
        out.push(format!("{id}-rules-text"));
    }
    out
}

/// The MCP tool result for a tool outcome: `content` and `isError`, plus `structuredContent`
/// when `structured` and successful (and under [`MAX_OUTPUT_CHARS`]).
#[must_use]
pub fn tool_result(outcome: Result<Value, String>, structured: bool) -> Value {
    match outcome {
        Ok(value) => success_result(value, structured),
        Err(message) => json!({
            "content": [{"type": "text", "text": message}],
            "isError": true,
        }),
    }
}

fn success_result(value: Value, structured: bool) -> Value {
    let text = value.to_string();
    if text.chars().count() > MAX_OUTPUT_CHARS {
        // Over the cap: keep the text view only (a truncated value would break the schema).
        let mut cut: String = text.chars().take(MAX_OUTPUT_CHARS).collect();
        cut.push_str("... [truncated]");
        return json!({"content": [{"type": "text", "text": cut}], "isError": false});
    }
    let mut result = json!({"content": [{"type": "text", "text": text}], "isError": false});
    if structured {
        result["structuredContent"] = value;
    }
    result
}

/// Deserialize tool arguments, mapping errors to a message the model can act on.
pub(crate) fn parse_args<T: DeserializeOwned>(args: &Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("invalid arguments: {e}"))
}

/// Parse a ruleset / edition word: `2014`, `2024`, `dnd2014`, `dnd2024`, `any`.
pub(crate) fn parse_ruleset(text: &str) -> Result<Ruleset, String> {
    match text.trim().to_ascii_lowercase().trim_start_matches("dnd") {
        "2014" | "5.1" | "5e" => Ok(Ruleset::Dnd2014),
        "2024" | "5.2" | "5.5" | "5.2.1" => Ok(Ruleset::Dnd2024),
        "any" => Ok(Ruleset::Any),
        other => Err(format!(
            "unknown ruleset {other:?}; use \"2014\" or \"2024\""
        )),
    }
}

/// Plain text of an element description, capped to `max` characters.
pub(crate) fn plain_text(html: &str, max: usize) -> String {
    let text = borealis_ai::retrieval::html_to_text(html);
    if text.chars().count() <= max {
        return text;
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push_str("...");
    cut
}
