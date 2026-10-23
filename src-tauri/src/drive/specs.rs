//! Tool definitions shared by the MCP write mode (`tools/list`) and the CLI (argument types,
//! `borealis help`). JSON Schema 2020-12, one entry per tool, in a fixed order.

use serde_json::{json, Map, Value};

/// How a tool touches the world (drives the MCP annotations).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Reads the library only.
    Read,
    /// Changes the library.
    Write,
    /// May remove data for good.
    Destructive,
    /// Downloads from the network (explicit content import only).
    Network,
}

fn tool(name: &str, description: &str, props: &Value, required: &[&str], kind: Kind) -> Value {
    let read_only = kind == Kind::Read;
    json!({
        "name": name,
        "title": name.replace('_', " "),
        "description": description,
        "inputSchema": {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": false
        },
        "outputSchema": {"type": "object"},
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": kind == Kind::Destructive,
            "idempotentHint": read_only,
            "openWorldHint": kind == Kind::Network
        }
    })
}

fn s(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn int(description: &str) -> Value {
    json!({"type": "integer", "minimum": 0, "description": description})
}

fn flag(description: &str) -> Value {
    json!({"type": "boolean", "description": description})
}

fn props(entries: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (k, v) in entries {
        map.insert((*k).to_string(), v.clone());
    }
    Value::Object(map)
}

fn id() -> (&'static str, Value) {
    (
        "id",
        s("Character id (from list_characters or create_character)."),
    )
}

fn uid() -> (&'static str, Value) {
    (
        "uid",
        s("Inventory row id (inventory[].uid in get_character section=inventory)."),
    )
}

/// Every drive tool, library first, then builder, gear, content.
#[must_use]
pub fn all() -> Vec<Value> {
    let mut out = library_tools();
    out.extend(builder_tools());
    out.extend(gear_tools());
    out.extend(content_tools());
    out
}

/// The names of all drive tools, in order.
#[must_use]
pub fn names() -> Vec<String> {
    all()
        .iter()
        .filter_map(|t| t["name"].as_str().map(String::from))
        .collect()
}

/// One tool's definition.
#[must_use]
pub fn find(name: &str) -> Option<Value> {
    all().into_iter().find(|t| t["name"] == name)
}

fn library_tools() -> Vec<Value> {
    let ruleset =
        json!({"type": "string", "enum": ["2014", "2024"], "description": "Rules edition."});
    let method = json!({"type": "string", "enum": ["manual", "point_buy", "standard_array", "roll_3d6", "roll_4d6"], "description": "Ability score method."});
    let section = json!({"type": "string", "enum": ["summary", "abilities", "stats", "slots", "spells", "inventory", "full"], "description": "Part of the character to return (default summary)."});
    vec![
        tool("list_characters", "List the characters saved in the library (id, name, build, level, ruleset).", &props(&[]), &[], Kind::Read),
        tool("create_character", "Create and save a new level-1 (or higher) character. Returns its summary with the id.", &props(&[
            ("name", s("Character name.")),
            ("ruleset", ruleset),
            ("level", int("Starting level (default 1).")),
            ("ability_method", method),
            ("feats", flag("Allow feats (default: the app default).")),
            ("multiclassing", flag("Allow multiclassing (default: the app default).")),
        ]), &["name", "ruleset"], Kind::Write),
        tool("open_character", "Open a saved character and return its summary.", &props(&[id()]), &["id"], Kind::Read),
        tool("get_character", "A character's computed values: summary, abilities, stats (AC, HP, skills, features), slots, spells, inventory or the full snapshot.", &props(&[id(), ("section", section)]), &["id"], Kind::Read),
        tool("save_character", "Save the character (changes are already saved after each edit); with `as_name`, save a copy as a new character.", &props(&[id(), ("as_name", s("Name of a new copy."))]), &["id"], Kind::Write),
        tool("close_character", "Release an open character (saved first).", &props(&[id()]), &["id"], Kind::Write),
        tool("delete_character", "Move a character to the library's trash folder; `permanent: true` deletes it for good.", &props(&[id(), ("permanent", flag("Delete for good instead of moving to trash."))]), &["id"], Kind::Destructive),
    ]
}

fn builder_tools() -> Vec<Value> {
    let slot = ("slot_id", s("Selection slot id (from list_slots)."));
    vec![
        tool("list_slots", "The character's selection slots (race/species, class, background, feats, spells, ...), with what is chosen. `open_only` lists only empty slots.", &props(&[id(), ("open_only", flag("Only slots with nothing chosen.")), ("tab", s("Only slots of this builder tab."))]), &["id"], Kind::Read),
        tool("list_options", "The options the engine offers for one slot, with the reason an option is not available.", &props(&[id(), slot.clone(), ("query", s("Filter by name.")), ("limit", int("Page size (default 50).")), ("offset", int("Page start."))]), &["id", "slot_id"], Kind::Read),
        tool("pick", "Choose an element for a slot. The engine validates it exactly as the app does.", &props(&[id(), slot.clone(), ("element_id", s("Element id from list_options."))]), &["id", "slot_id", "element_id"], Kind::Write),
        tool("unpick", "Clear a slot's choice.", &props(&[id(), slot]), &["id", "slot_id"], Kind::Write),
        tool("level_up", "Gain a level in a class (`class_slot_id`, default the main class `main`).", &props(&[id(), ("class_slot_id", s("Class slot id (`main` or a multiclass slot)."))]), &["id"], Kind::Write),
        tool("add_multiclass", "Add a multiclass slot (pick its class with `pick`, then `level_up` with its class slot id).", &props(&[id()]), &["id"], Kind::Write),
        tool("set_abilities", "Set the ability method and/or base scores. Point buy and manual set each score; standard array assigns array values; a roll method with no scores rolls all six.", &props(&[
            id(),
            ("method", json!({"type": "string", "enum": ["manual", "point_buy", "standard_array", "roll_3d6", "roll_4d6"]})),
            ("scores", json!({"type": "object", "description": "Base scores by ability, e.g. {\"str\": 15, \"dex\": 14}.", "properties": {"str": {"type": "integer"}, "dex": {"type": "integer"}, "con": {"type": "integer"}, "int": {"type": "integer"}, "wis": {"type": "integer"}, "cha": {"type": "integer"}}, "additionalProperties": false})),
        ]), &["id"], Kind::Write),
    ]
}

fn gear_tools() -> Vec<Value> {
    let equip = json!({"type": "string", "enum": ["auto", "worn", "primary", "secondary", "two-handed", "versatile"], "description": "Where to equip (default auto)."});
    let spell = ("spell_id", s("Spell id (get_character section=spells)."));
    let class_slot = (
        "class_slot_id",
        s("Spellcasting class slot id (default `main`)."),
    );
    vec![
        tool("search_items", "Search equipment; with `id`, restricted to the character's ruleset and sources.", &props(&[("id", s("Character id (optional).")), ("query", s("Name filter.")), ("category", s("Item category.")), ("limit", int("Page size (default 25).")), ("offset", int("Page start."))]), &[], Kind::Read),
        tool("list_base_items", "The base items a generic magic weapon or armor (+1 armor, +2 weapon, ...) can be added on; empty `items` and `needsBase: false` when the item needs none.", &props(&[id(), ("item_id", s("Item id from search_items."))]), &["id", "item_id"], Kind::Read),
        tool("add_item", "Add an item to the inventory. A generic magic weapon or armor needs `base_item_id` (see list_base_items); without a suitable base it is refused.", &props(&[id(), ("item_id", s("Item id from search_items.")), ("quantity", int("How many (default 1).")), ("base_item_id", s("Base item id from list_base_items (required for a generic magic weapon or armor).")), ("buy", flag("Pay its price from the coins."))]), &["id", "item_id"], Kind::Write),
        tool("remove_item", "Remove one item (or the whole stack with `all`).", &props(&[id(), uid(), ("all", flag("Remove the whole stack."))]), &["id", "uid"], Kind::Write),
        tool("equip_item", "Equip an inventory item.", &props(&[id(), uid(), ("slot", equip)]), &["id", "uid"], Kind::Write),
        tool("unequip_item", "Unequip an inventory item.", &props(&[id(), uid()]), &["id", "uid"], Kind::Write),
        tool("attune_item", "Attune (or with `attuned: false` end attunement to) an item.", &props(&[id(), uid(), ("attuned", flag("Default true."))]), &["id", "uid"], Kind::Write),
        tool("prepare_spell", "Prepare a spell for a spellcasting class (the engine enforces the limit).", &props(&[id(), spell.clone(), class_slot.clone()]), &["id", "spell_id"], Kind::Write),
        tool("unprepare_spell", "Unprepare a spell.", &props(&[id(), spell, class_slot]), &["id", "spell_id"], Kind::Write),
        tool("export_sheet", "Render the character sheet to a file inside the library (`.pdf`, or `.html` with format html).", &props(&[id(), ("path", s("Output file, relative to the library or absolute inside it.")), ("format", json!({"type": "string", "enum": ["pdf", "html"]}))]), &["id", "path"], Kind::Write),
    ]
}

fn content_tools() -> Vec<Value> {
    let path = ("path", s("Path inside the library (relative or absolute)."));
    vec![
        tool("list_packs", "Installed content packs (bundled and imported).", &props(&[]), &[], Kind::Read),
        tool("import_index", "Add a content index URL (HTTPS) and download its files into deterministic packs. The only tool that uses the network.", &props(&[("url", s("HTTPS URL of a .index file."))]), &["url"], Kind::Network),
        tool("scan_5etools", "List the sources found in a 5etools `data` folder or `.zip` inside the library.", &props(std::slice::from_ref(&path)), &["path"], Kind::Read),
        tool("import_5etools", "Convert sources from a 5etools `data` folder or `.zip` inside the library into packs (deterministic).", &props(&[path.clone(), ("sources", json!({"type": "array", "items": {"type": "string"}, "description": "Source abbreviations (default: every source found)."})), ("include_fluff", flag("Include descriptive fluff text."))]), &["path"], Kind::Write),
        tool("import_content_file", "Convert a content file inside the library (XML, JSON, .index or .zip) into a pack (deterministic).", &props(&[path]), &["path"], Kind::Write),
    ]
}
