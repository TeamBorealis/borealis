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

/// All tools in a fixed order. Every tool is read-only. Schemas are JSON Schema 2020-12.
/// `with_output_schema` is false only for the oldest legacy protocol (2024-11-05).
pub fn tool_list(with_output_schema: bool) -> Value {
    let mut tools = all_tools();
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
        tool("search_content", "BM25 search over loaded content (rules text, conditions, spells, classes, items...). Returns ids, names, types and snippets.", &json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search words."},
                "type": {"type": "string", "description": "Element type filter, e.g. Condition, Spell, Class Feature."},
                "ruleset": ruleset,
                "k": {"type": "integer", "minimum": 1, "maximum": 20, "default": 8}
            },
            "required": ["query"], "additionalProperties": false
        }), &json!({"type":"object","properties":{"count":{"type":"integer"},"hits":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"name":{"type":"string"},"type":{"type":"string"},"ruleset":{"type":["string","null"]},"source":{"type":"string"},"snippet":{"type":"string"}},"required":["id","name","type"]}}},"required":["count","hits"]})),
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
        tool("encounter_evaluate", "Evaluate an encounter's difficulty (XP budget) for a party. Both editions use the SRD 5.2.1 XP-budget method (output has `method`); the classic 2014 DMG method needs user-supplied tables.", &json!({
            "type": "object",
            "properties": {
                "party_levels": {"type": "array", "items": {"type": "integer", "minimum": 1, "maximum": 20}, "minItems": 1},
                "monsters": {"type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "description": "Monster element id (uses its CR)."},
                        "cr": {"type": ["string", "number"], "description": "Challenge rating such as \"1/4\" or 3."},
                        "count": {"type": "integer", "minimum": 1, "default": 1}
                    },
                    "additionalProperties": false
                }},
                "ruleset": ruleset
            },
            "required": ["party_levels", "monsters", "ruleset"], "additionalProperties": false
        }), &json!({"type":"object","properties":{"party_size":{"type":"integer"},"monster_count":{"type":"integer"},"total_xp":{"type":"integer"},"adjusted_xp":{"type":"integer"},"thresholds":{"type":"object"},"band":{"type":"string"},"warnings":{"type":"array"},"monsters":{"type":"array","items":{"type":"object"}}},"required":["total_xp","band"]})),
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
