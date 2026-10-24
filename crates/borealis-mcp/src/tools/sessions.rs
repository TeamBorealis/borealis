//! `search_sessions` and the `session:` side of `get_passage`: the only door to campaign notes.
//!
//! Scope enforcement lives here and nowhere else:
//! * the campaign comes from the launch arguments ([`SessionScope`]), never from a tool argument;
//! * without a scope the tool is not listed and a call is rejected as an unknown tool;
//! * every result is re-checked against the scope, so a [`SessionSource`] that over-returns still
//!   cannot leak another campaign;
//! * a passage of another campaign is reported exactly like a missing one (no existence oracle).
//!
//! There is no sessions corpus yet (S2.1): the default source is empty. A real source plugs in
//! through [`Tools::with_sessions`](super::Tools::with_sessions).

use serde::Deserialize;
use serde_json::{json, Value};

use super::{parse_args, Tools};
use crate::config::SessionScope;

const DEFAULT_K: usize = 8;
const MAX_K: usize = 20;
/// Key prefix of session chunks (`session:<entry>#n`).
pub const SESSION_PREFIX: &str = "session:";

/// One chunk of campaign notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChunk {
    /// `session:<entry>#n`.
    pub cite: String,
    /// Entry title.
    pub title: String,
    /// Where in the campaign it sits.
    pub path: String,
    /// The full text of the chunk.
    pub text: String,
    /// The campaign it belongs to.
    pub campaign: String,
}

/// Where session notes come from. Implementations must be read-only.
pub trait SessionSource: Send + Sync {
    /// Chunks of `campaign` matching `query`, best first.
    fn search(&self, campaign: &str, query: &str, k: usize) -> Vec<SessionChunk>;
    /// The chunk stored under `cite`, whichever campaign it belongs to.
    fn passage(&self, cite: &str) -> Option<SessionChunk>;
}

/// The source while no sessions corpus exists: nothing to find.
pub struct NoSessions;

impl SessionSource for NoSessions {
    fn search(&self, _campaign: &str, _query: &str, _k: usize) -> Vec<SessionChunk> {
        Vec::new()
    }
    fn passage(&self, _cite: &str) -> Option<SessionChunk> {
        None
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    query: String,
    k: Option<usize>,
}

/// The granted campaign, or the rejection a call without scope gets.
fn granted(scope: &SessionScope) -> Result<&str, String> {
    scope
        .campaign()
        .ok_or_else(|| "session notes were not shared for this question".to_string())
}

pub fn run(tools: &Tools, args: &Value) -> Result<Value, String> {
    let campaign = granted(&tools.scope)?;
    let a: Args = parse_args(args)?;
    if a.query.trim().is_empty() {
        return Err("query must not be empty".into());
    }
    let k = a.k.unwrap_or(DEFAULT_K).clamp(1, MAX_K);
    let hits: Vec<Value> = tools
        .sessions
        .search(campaign, &a.query, k)
        .into_iter()
        .filter(|c| c.campaign == campaign && c.cite.starts_with(SESSION_PREFIX))
        .take(k)
        .map(|c| {
            json!({
                "cite": c.cite, "name": c.title, "type": "Session note", "path": c.path,
                "ruleset": null, "source": "Campaign notes",
                "snippet": c.text.chars().take(super::search::SNIPPET_CHARS).collect::<String>()
            })
        })
        .collect();
    Ok(json!({"count": hits.len(), "hits": hits}))
}

/// `get_passage` for a `session:` cite.
pub fn passage(tools: &Tools, cite: &str) -> Result<Value, String> {
    let campaign = granted(&tools.scope)?;
    match tools
        .sessions
        .passage(cite)
        .filter(|c| c.campaign == campaign && c.cite == cite)
    {
        Some(c) => Ok(super::passage::result(
            &c.cite,
            &c.title,
            &c.path,
            "Session note",
            None,
            "Campaign notes",
            &c.text,
        )),
        None => Err(super::passage::not_found(cite)),
    }
}
