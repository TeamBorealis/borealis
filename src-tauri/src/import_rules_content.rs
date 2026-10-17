//! Content the rules-based importers read never goes to the AI importer (user rule: Aurora and
//! 5etools imports are deterministic and never involve AI). Aurora XML (an `<elements>` file, an
//! element fragment or an `.index`), 5etools data and Borealis content packs are recognised by shape
//! and refused with a pointer to Additional Content, before any model is called. Mirrors
//! `ui/src/pages/import/rulesContent.ts`.

use serde_json::Value;

/// Content a rules-based importer owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesContent {
    /// Aurora XML: Additional Content > Custom content.
    Aurora,
    /// 5etools data: Additional Content > 5etools import.
    FiveTools,
    /// A Borealis content file or pack manifest: Additional Content > Custom content.
    Borealis,
}

/// Top-level arrays of a 5etools data file (`data/*.json`, homebrew).
const FIVETOOLS_KEYS: &[&str] = &[
    "action",
    "adventure",
    "background",
    "baseitem",
    "book",
    "boon",
    "card",
    "charoption",
    "class",
    "classFeature",
    "condition",
    "cult",
    "deck",
    "deity",
    "disease",
    "facility",
    "feat",
    "hazard",
    "item",
    "itemGroup",
    "itemMastery",
    "itemProperty",
    "itemType",
    "language",
    "legendaryGroup",
    "magicvariant",
    "monster",
    "object",
    "optionalfeature",
    "psionic",
    "race",
    "recipe",
    "reward",
    "sense",
    "skill",
    "spell",
    "status",
    "subclass",
    "subclassFeature",
    "subrace",
    "table",
    "trap",
    "variantrule",
    "vehicle",
];

/// What kind of rules content `text` is, if any.
pub fn detect(text: &str) -> Option<RulesContent> {
    let t = text.trim_start_matches('\u{feff}').trim();
    if t.starts_with('{') || t.starts_with('[') {
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            return detect_json(&v);
        }
    }
    is_aurora_xml(t).then_some(RulesContent::Aurora)
}

/// The friendly refusal; `subject` is a quoted file name or "The pasted text".
pub fn refusal(kind: RulesContent, subject: &str) -> String {
    match kind {
        RulesContent::Aurora => format!(
            "{subject} is Aurora content. It is added as it is, without AI: save it as an .xml file and add it under Additional Content > Custom content."
        ),
        RulesContent::FiveTools => format!(
            "{subject} is 5etools data. It is converted as it is, without AI: use Additional Content > 5etools import with your 5etools data folder."
        ),
        RulesContent::Borealis => format!(
            "{subject} is already a Borealis content pack. Add it as it is under Additional Content > Custom content."
        ),
    }
}

fn detect_json(v: &Value) -> Option<RulesContent> {
    match v {
        Value::Object(o) => {
            let format = o.get("format").and_then(Value::as_str).unwrap_or_default();
            if format.starts_with("borealis-") {
                return Some(RulesContent::Borealis);
            }
            let fivetools = o.get("_meta").is_some_and(Value::is_object)
                || o.iter().any(|(k, val)| {
                    (FIVETOOLS_KEYS.contains(&k.as_str()) || k.ends_with("Fluff"))
                        && has_sourced_entry(val)
                })
                || is_fivetools_entity(v);
            fivetools.then_some(RulesContent::FiveTools)
        }
        Value::Array(a) => a
            .iter()
            .any(is_fivetools_entity)
            .then_some(RulesContent::FiveTools),
        _ => None,
    }
}

fn has_sourced_entry(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        a.iter()
            .any(|e| e.get("source").is_some_and(Value::is_string))
    })
}

/// One 5etools record: a name, a source abbreviation and its `entries` or page number.
fn is_fivetools_entity(v: &Value) -> bool {
    v.get("name").is_some_and(Value::is_string)
        && v.get("source").is_some_and(Value::is_string)
        && (v.get("entries").is_some_and(Value::is_array)
            || v.get("page").is_some_and(Value::is_number))
}

/// `<name` opening a tag (followed by whitespace, `>` or `/`), case-insensitive on lowercase text.
fn tag_starts(lower: &str, name: &str) -> Vec<usize> {
    let open = format!("<{name}");
    lower
        .match_indices(&open)
        .map(|(i, _)| i)
        .filter(|&i| {
            lower[i + open.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/')
        })
        .collect()
}

fn has_tag(lower: &str, name: &str) -> bool {
    !tag_starts(lower, name).is_empty()
}

/// An Aurora element tag: `<element ... id="ID_...">`.
fn has_aurora_element(lower: &str) -> bool {
    tag_starts(lower, "element").into_iter().any(|i| {
        let tag = &lower[i..];
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        tag.match_indices("id=").any(|(j, _)| {
            tag[..j].ends_with(char::is_whitespace)
                && (tag[j + 3..].starts_with("\"id_") || tag[j + 3..].starts_with("'id_"))
        })
    })
}

fn is_aurora_xml(text: &str) -> bool {
    if !text.contains('<') {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    has_tag(&lower, "elements")
        || has_aurora_element(&lower)
        || (has_tag(&lower, "index") && has_tag(&lower, "files"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AURORA_FILE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>Synthetic</name></info>
  <element name="Test Spark" type="Spell" source="Synthetic" id="ID_SYN_SPELL_TEST_SPARK">
    <description><p>A tiny spark.</p></description>
  </element>
</elements>"#;

    const AURORA_FRAGMENT: &str = r#"<element name="Test Blade" type="Item" source="Synthetic" id="ID_SYN_ITEM_TEST_BLADE">
  <description><p>A synthetic blade.</p></description>
</element>"#;

    const AURORA_INDEX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<index>
  <info><name>Synthetic</name></info>
  <files><file name="synthetic.xml" url="https://example.invalid/synthetic.xml" /></files>
</index>"#;

    const FIVETOOLS_SPELLS: &str = r#"{"spell":[{"name":"Test Spark","source":"SYN","page":1,"level":0,"entries":["A tiny spark."]}]}"#;
    const FIVETOOLS_HOMEBREW: &str =
        r#"{"_meta":{"sources":[{"json":"SYN","abbreviation":"SYN"}]},"feat":[]}"#;
    const FIVETOOLS_FLUFF: &str =
        r#"{"monsterFluff":[{"name":"Test Beast","source":"SYN","entries":["Lore."]}]}"#;
    const FIVETOOLS_ENTITY: &str =
        r#"{"name":"Test Feat","source":"SYN","page":3,"entries":["You gain a thing."]}"#;
    const BOREALIS_CONTENT: &str = r#"{"format":"borealis-content/1","elements":[]}"#;
    const BOREALIS_PACK: &str = r#"{"format":"borealis-pack/1","id":"syn","name":"Syn","version":"1.0.0","rulesets":[],"files":[]}"#;

    #[test]
    fn aurora_xml_is_recognised_as_file_fragment_and_index() {
        assert_eq!(detect(AURORA_FILE), Some(RulesContent::Aurora));
        assert_eq!(
            detect(&format!("\u{feff}  {AURORA_FILE}")),
            Some(RulesContent::Aurora)
        );
        assert_eq!(detect(AURORA_FRAGMENT), Some(RulesContent::Aurora));
        assert_eq!(detect(AURORA_INDEX), Some(RulesContent::Aurora));
        assert_eq!(
            detect(&format!("Here is my file:\n{AURORA_FILE}")),
            Some(RulesContent::Aurora)
        );
    }

    #[test]
    fn fivetools_shapes_are_recognised() {
        for text in [
            FIVETOOLS_SPELLS,
            FIVETOOLS_HOMEBREW,
            FIVETOOLS_FLUFF,
            FIVETOOLS_ENTITY,
        ] {
            assert_eq!(detect(text), Some(RulesContent::FiveTools), "{text}");
        }
        assert_eq!(
            detect(&format!("[{FIVETOOLS_ENTITY}]")),
            Some(RulesContent::FiveTools)
        );
    }

    #[test]
    fn borealis_packs_are_recognised() {
        assert_eq!(detect(BOREALIS_CONTENT), Some(RulesContent::Borealis));
        assert_eq!(detect(BOREALIS_PACK), Some(RulesContent::Borealis));
    }

    #[test]
    fn ordinary_sources_pass() {
        for text in [
            "Test Spark\nCantrip, evocation. A tiny spark of light.",
            "The <element> of surprise is key. An <index> of feats.",
            r#"{"name":"Test Spark","level":0,"text":"A tiny spark."}"#,
            r#"{"spell":"not a list"}"#,
            r#"[1, 2, 3]"#,
            "",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
    }

    #[test]
    fn refusals_point_to_additional_content() {
        let a = refusal(RulesContent::Aurora, "\"a.txt\"");
        assert!(a.starts_with("\"a.txt\" is Aurora content."), "{a}");
        assert!(a.contains("Additional Content > Custom content"), "{a}");
        let f = refusal(RulesContent::FiveTools, "The pasted text");
        assert!(f.contains("Additional Content > 5etools import"), "{f}");
        let b = refusal(RulesContent::Borealis, "\"p.json\"");
        assert!(b.contains("Additional Content > Custom content"), "{b}");
    }
}
