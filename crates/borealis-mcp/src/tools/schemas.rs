//! JSON-schema'd tool definitions for `tools/list`.

use serde_json::{json, Value};

/// Read-only, closed-world, idempotent: the only behaviour this server has.
fn tool(name: &str, description: &str, schema: &Value, output: &Value) -> Value {
    json!({
        "name": name,
        "title": name.replace('_', " "),
        "description": description,
        "inputSchema": schema,
        "outputSchema": output,
        "annotations": {
            "readOnlyHint": true, "destructiveHint": false,
            "idempotentHint": true, "openWorldHint": false
        }
    })
}

/// The tool that exists only inside a session scope.
pub const SEARCH_SESSIONS: &str = "search_sessions";

/// Output schema of every search tool: hits that carry their citation key.
fn hits_output() -> Value {
    json!({"type":"object","properties":{"count":{"type":"integer"},"hits":{"type":"array","items":{"type":"object","properties":{
        "cite":{"type":"string","description":"Citation key: pass to get_passage and cite as [[cite]]."},
        "id":{"type":"string"},"name":{"type":"string"},"type":{"type":"string"},"path":{"type":"string"},
        "ruleset":{"type":["string","null"]},"source":{"type":"string"},"snippet":{"type":"string"}},
        "required":["cite","name","type"]}}},"required":["count","hits"]})
}

/// All tools in a fixed order. Every tool is read-only. Schemas are JSON Schema 2020-12.
/// `with_output_schema` is false only for the oldest legacy protocol (2024-11-05).
/// `search_sessions` is listed only when `with_sessions` (a session scope was granted for this run).
pub fn tool_list(with_output_schema: bool, with_sessions: bool) -> Value {
    let mut tools = all_tools();
    if !with_sessions {
        if let Some(items) = tools.as_array_mut() {
            items.retain(|t| t["name"] != SEARCH_SESSIONS);
        }
    }
    if !with_output_schema {
        for t in tools.as_array_mut().into_iter().flatten() {
            if let Some(obj) = t.as_object_mut() {
                obj.remove("outputSchema");
                obj.remove("annotations");
            }
        }
    }
    tools
}

fn all_tools() -> Value {
    let ruleset =
        json!({"type": "string", "enum": ["2014", "2024"], "description": "Rules edition."});
    let doc = json!({
        "description": "Content as a JSON object: a content file {format, elements[]}, a single element, or an array of elements.",
        "type": ["object", "array", "string"]
    });
    json!([
        tool("search_content", "Search rules text and content elements together (alias of search_rules + search_lore). Returns cite keys, ids, names, types and snippets.", &json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search words."},
                "type": {"type": "string", "description": "Element type filter, e.g. Condition, Spell, Class Feature."},
                "ruleset": ruleset,
                "k": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8}
            },
            "required": ["query"], "additionalProperties": false
        }), &hits_output()),
        tool("search_rules", "Keyword search over the rules text (SRD sections, conditions, glossary). Each hit has a `cite` key: quote it as [[cite]] and read the whole passage with get_passage.", &json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search words."},
                "ruleset": ruleset,
                "k": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8}
            },
            "required": ["query"], "additionalProperties": false
        }), &hits_output()),
        tool("search_lore", "Keyword search over spells, monsters, classes, items and other content elements. Each hit has a `cite` key: quote it as [[cite]] and read the whole passage with get_passage.", &json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search words."},
                "type": {"type": "string", "description": "Element type filter, e.g. Spell, Monster, Class Feature."},
                "ruleset": ruleset,
                "k": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8}
            },
            "required": ["query"], "additionalProperties": false
        }), &hits_output()),
        tool(SEARCH_SESSIONS, "Keyword search over the notes of the one campaign the user chose to share for this question. Cite hits as [[session:...]] keys.", &json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search words."},
                "k": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8}
            },
            "required": ["query"], "additionalProperties": false
        }), &hits_output()),
        tool("get_passage", "Read the full text (at most 4,000 characters) of one search hit by its `cite` key.", &json!({
            "type": "object",
            "properties": {"cite": {"type": "string", "description": "The `cite` of a search hit, e.g. ID_2024_CONDITION_GRAPPLED or rules:srd-5.2.1-rules-text/combat."}},
            "required": ["cite"], "additionalProperties": false
        }), &json!({"type":"object","properties":{"cite":{"type":"string"},"title":{"type":"string"},"path":{"type":"string"},"type":{"type":"string"},"ruleset":{"type":["string","null"]},"source":{"type":"string"},"text":{"type":"string"},"truncated":{"type":"boolean"}},"required":["cite","title","text","truncated"]})),
        tool("get_element", "Fetch one element by id: trimmed JSON (setters, rules, spellcasting) with the description as plain text.", &json!({
            "type": "object",
            "properties": {"id": {"type": "string", "description": "Element id, e.g. ID_2024_CONDITION_GRAPPLED."}},
            "required": ["id"], "additionalProperties": false
        }), &json!({"type":"object","properties":{"id":{"type":"string"},"name":{"type":"string"},"type":{"type":"string"},"description":{"type":"string"}},"required":["id","name","type"]})),
        tool("list_core_ids", "List ids and names of core vocabulary elements, to avoid inventing ids.", &json!({
            "type": "object",
            "properties": {
                "category": {"type": "string", "enum": ["skills", "abilities", "proficiencies", "languages", "damage_types", "conditions", "sizes"]},
                "ruleset": ruleset
            },
            "required": ["category"], "additionalProperties": false
        }), &json!({"type":"object","properties":{"category":{"type":"string"},"count":{"type":"integer"},"ids":{"type":"array","items":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":2}}},"required":["category","count","ids"]})),
        tool("validate_element", "Validate content: JSON schema, then load it in an overlay on top of the installed content (reports dangling references, bad expressions, duplicate ids). Nothing is saved.", &json!({
            "type": "object",
            "properties": {
                "json": doc,
                "elements": {"type": "array", "items": {"type": "object"}, "description": "Alternative to json: a list of elements."},
                "ruleset": {"type": "string", "enum": ["2014", "2024", "dnd2014", "dnd2024", "any"], "description": "Edition of the content; recorded as the file-level `ruleset` unless the content already sets one. Elements may also carry their own `ruleset`."}
            },
            "additionalProperties": false
        }), &json!({"type":"object","properties":{"ok":{"type":"boolean"},"stage":{"type":"string","enum":["schema","store"]},"schema_issues":{"type":"array","items":{"type":"object"}},"report":{"type":"object"}},"required":["ok","stage"]})),
        tool("class_sweep", "Load a class bundle in an overlay, build a test character and level it 1 to 20 with the engine. Reports per-level errors and key stats.", &json!({
            "type": "object",
            "properties": {
                "class_json_bundle": doc,
                "edition": ruleset,
                "subclass": {"type": "string", "description": "Optional subclass (Archetype) id or name to pick when offered."}
            },
            "required": ["class_json_bundle", "edition"], "additionalProperties": false
        }), &json!({"type":"object","properties":{"passed":{"type":"boolean"},"class":{"type":"string"},"subclass":{"type":["string","null"]},"edition":{"type":"string"},"levels_reached":{"type":"integer"},"engine_errors":{"type":"integer"},"load":{"type":"object"},"levels":{"type":"array","items":{"type":"object"}}},"required":["passed","class","levels_reached","levels"]})),
        tool("explain_rules_term", "Look up a rules term (condition, glossary entry) and return its rules text.", &json!({
            "type": "object",
            "properties": {
                "term": {"type": "string", "description": "e.g. Grappled, Advantage, Concentration."},
                "ruleset": ruleset
            },
            "required": ["term"], "additionalProperties": false
        }), &json!({"type":"object","properties":{"exact":{"type":"boolean"},"matches":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"name":{"type":"string"},"text":{"type":"string"}},"required":["id","name","text"]}}},"required":["exact","matches"]})),
    ])
}
