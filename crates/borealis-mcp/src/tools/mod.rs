//! Tool registry: schemas, argument parsing, dispatch and output caps.

mod element;
mod encounter;
mod glossary;
mod ids;
mod overlay;
mod schemas;
mod search;
mod sweep;
mod validate;

use std::collections::HashSet;
use std::sync::OnceLock;

use borealis_ai::retrieval::RetrievalIndex;
use borealis_ai::task::JsonSchemaValidator;
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::config::{load_base, McpConfig};

/// Hard cap on the text of one tool result.
pub const MAX_OUTPUT_CHARS: usize = 24_000;

/// The loaded content plus lazily built helpers.
pub struct Tools {
    pub(crate) builder: ContentStoreBuilder,
    pub(crate) store: ContentStore,
    index: OnceLock<RetrievalIndex>,
    schema: OnceLock<JsonSchemaValidator>,
    baseline: OnceLock<HashSet<String>>,
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
        })
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
        schemas::tool_list(with_output_schema)
    }

    /// Run a tool. `None` when the tool does not exist; otherwise the MCP tool result
    /// (`content`, `isError`, and `structuredContent` when `structured` and successful).
    #[must_use]
    pub fn call(&self, name: &str, args: &Value, structured: bool) -> Option<Value> {
        let outcome = match name {
            "search_content" => search::run(self, args),
            "get_element" => element::run(self, args),
            "list_core_ids" => ids::run(self, args),
            "validate_element" => validate::run(self, args),
            "class_sweep" => sweep::run(self, args),
            "encounter_evaluate" => encounter::run(self, args),
            "explain_rules_term" => glossary::run(self, args),
            _ => return None,
        };
        Some(match outcome {
            Ok(value) => success_result(value, structured),
            Err(message) => json!({
                "content": [{"type": "text", "text": message}],
                "isError": true,
            }),
        })
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
