//! `entries` (the recursive rich text structure) to the HTML fragment subset of borealis-content.

use super::tags::{esc, render_inline, strip_tags, RefKind};
use serde_json::Value;
use std::collections::BTreeSet;

/// Link resolver: `(kind, name, source) -> element id`.
pub type LinkFn<'a> = &'a dyn Fn(RefKind, &str, &str) -> Option<String>;

/// Renders 5etools entries. Collects what it could not render for the import report.
pub struct Renderer<'a> {
    link_for: LinkFn<'a>,
    /// Entry `type` values that had no renderer.
    pub unknown_types: BTreeSet<String>,
    /// Number of images/galleries/stat blocks that were dropped.
    pub dropped_media: usize,
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

impl<'a> Renderer<'a> {
    /// A renderer that resolves entity links with `link_for`.
    #[must_use]
    pub fn new(link_for: LinkFn<'a>) -> Self {
        Self {
            link_for,
            unknown_types: BTreeSet::new(),
            dropped_media: 0,
        }
    }

    /// Inline text with tags to HTML.
    #[must_use]
    pub fn inline(&self, s: &str) -> String {
        render_inline(s, self.link_for)
    }

    /// Render a list of entries as an HTML fragment.
    pub fn entries(&mut self, entries: &[Value]) -> String {
        let mut out = String::new();
        for e in entries {
            out.push_str(&self.entry(e, 0));
        }
        out
    }

    /// Render one entry.
    pub fn entry(&mut self, value: &Value, depth: usize) -> String {
        match value {
            Value::String(s) => format!("<p>{}</p>", self.inline(s)),
            Value::Number(n) => format!("<p>{n}</p>"),
            Value::Array(a) => a.iter().map(|e| self.entry(e, depth)).collect(),
            Value::Object(_) => self.object(value, depth),
            _ => String::new(),
        }
    }

    fn children(&mut self, value: &Value, depth: usize) -> String {
        match value.get("entries") {
            Some(Value::Array(a)) => a.iter().map(|e| self.entry(e, depth + 1)).collect(),
            _ => match value.get("entry") {
                Some(e) => self.entry(e, depth + 1),
                None => String::new(),
            },
        }
    }

    fn named_block(&mut self, value: &Value, depth: usize) -> String {
        let name = value.get("name").and_then(Value::as_str);
        match name {
            None => self.children(value, depth),
            Some(name) if depth == 0 => {
                format!(
                    "<h4>{}</h4>{}",
                    self.inline(name),
                    self.children(value, depth)
                )
            }
            Some(name) => {
                // Inline-named paragraph: "<strong>Name.</strong> first paragraph".
                let name_html = self.inline(name);
                let entries = value
                    .get("entries")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut iter = entries.iter();
                let mut out = String::new();
                match iter.next() {
                    Some(Value::String(first)) => {
                        out.push_str(&format!(
                            "<p><strong>{name_html}.</strong> {}</p>",
                            self.inline(first)
                        ));
                    }
                    Some(other) => {
                        out.push_str(&format!("<p><strong>{name_html}.</strong></p>"));
                        out.push_str(&self.entry(other, depth + 1));
                    }
                    None => out.push_str(&format!("<p><strong>{name_html}.</strong></p>")),
                }
                for e in iter {
                    out.push_str(&self.entry(e, depth + 1));
                }
                out
            }
        }
    }

    fn object(&mut self, value: &Value, depth: usize) -> String {
        let ty = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("entries");
        match ty {
            "entries" | "section" | "wrapper" | "variant" | "variantInner" | "variantSub"
            | "optfeature" | "flowBlock" | "inlineBlock" => {
                if ty == "wrapper" {
                    return value
                        .get("wrapped")
                        .map(|w| self.entry(w, depth))
                        .unwrap_or_default();
                }
                self.named_block(value, depth)
            }
            "inline" => {
                let inner: String = value
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|e| match e {
                                Value::String(s) => self.inline(s),
                                other => self.entry(other, depth),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                format!("<p>{inner}</p>")
            }
            "inset" | "insetReadaloud" => {
                let title = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|n| format!("<h5>{}</h5>", self.inline(n)))
                    .unwrap_or_default();
                let class = if ty == "inset" {
                    "inset"
                } else {
                    "inset readaloud"
                };
                format!(
                    "<div class=\"{class}\">{title}{}</div>",
                    self.children(value, depth)
                )
            }
            "quote" => {
                let body = value
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|e| format!("<p><em>{}</em></p>", self.inline(&text_of(e))))
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                let by = value
                    .get("by")
                    .and_then(Value::as_str)
                    .map(|b| format!("<p>\u{2014} {}</p>", self.inline(b)))
                    .unwrap_or_default();
                format!("<div class=\"quote\">{body}{by}</div>")
            }
            "list" => self.list(value, depth),
            "table" | "tableGroup" => self.table(value),
            "item" | "itemSub" => self.item_paragraph(value, depth),
            "options" => {
                let items: String = value
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|e| format!("<li>{}</li>", self.list_item_body(e, depth)))
                            .collect()
                    })
                    .unwrap_or_default();
                let count = value
                    .get("count")
                    .and_then(Value::as_u64)
                    .map(|c| format!("<p><em>Choose {c}:</em></p>"))
                    .unwrap_or_default();
                format!("{count}<ul>{items}</ul>")
            }
            "refClassFeature" | "refSubclassFeature" | "refOptionalfeature" | "refFeat"
            | "refItem" | "refSpell" => self.reference(value),
            "abilityDc" | "abilityAttackMod" => self.ability_line(value, ty),
            "abilityGeneric" => format!(
                "<p>{}</p>",
                self.inline(value.get("text").and_then(Value::as_str).unwrap_or(""))
            ),
            "bonus" => format!(
                "<p>{}</p>",
                signed_number(value.get("value").map(text_of).unwrap_or_default())
            ),
            "bonusSpeed" => format!(
                "<p>{} ft.</p>",
                signed_number(value.get("value").map(text_of).unwrap_or_default())
            ),
            "dice" => format!("<p>{}</p>", dice_text(value)),
            "link" => format!(
                "<p>{}</p>",
                self.inline(value.get("text").and_then(Value::as_str).unwrap_or(""))
            ),
            "hr" => "<hr />".to_string(),
            "image" | "gallery" | "statblock" | "statblockInline" | "flowchart" => {
                self.dropped_media += 1;
                String::new()
            }
            "cell" | "row" => self.children(value, depth),
            other => {
                let has_children = value.get("entries").is_some() || value.get("entry").is_some();
                if has_children {
                    self.unknown_types.insert(other.to_string());
                    self.named_block(value, depth)
                } else {
                    self.unknown_types.insert(other.to_string());
                    String::new()
                }
            }
        }
    }

    fn reference(&mut self, value: &Value) -> String {
        let key = [
            "classFeature",
            "subclassFeature",
            "optionalfeature",
            "feat",
            "item",
            "spell",
        ]
        .iter()
        .find_map(|k| value.get(*k).and_then(Value::as_str))
        .unwrap_or("");
        let name = key.split('|').next().unwrap_or("");
        format!("<p class=\"ref\">{}</p>", esc(name))
    }

    fn ability_line(&mut self, value: &Value, ty: &str) -> String {
        let name = value.get("name").and_then(Value::as_str).unwrap_or("Spell");
        let abilities: Vec<&str> = value
            .get("attributes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(ability_name)
                    .collect()
            })
            .unwrap_or_default();
        let which = abilities.join(" or ");
        if ty == "abilityDc" {
            format!(
                "<p><strong>{} save DC</strong> = 8 + your proficiency bonus + your {which} modifier</p>",
                esc(name)
            )
        } else {
            format!(
                "<p><strong>{} attack modifier</strong> = your proficiency bonus + your {which} modifier</p>",
                esc(name)
            )
        }
    }

    fn item_paragraph(&mut self, value: &Value, depth: usize) -> String {
        let name = value.get("name").and_then(Value::as_str).unwrap_or("");
        let body = match (value.get("entry"), value.get("entries")) {
            (Some(Value::String(s)), _) => self.inline(s),
            (Some(other), _) => self.entry(other, depth + 1),
            (None, Some(Value::Array(a))) => a
                .iter()
                .map(|e| match e {
                    Value::String(s) => self.inline(s),
                    other => self.entry(other, depth + 1),
                })
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        };
        if name.is_empty() {
            format!("<p>{body}</p>")
        } else {
            format!("<p><strong>{}</strong> {body}</p>", self.inline(name))
        }
    }

    fn list_item_body(&mut self, item: &Value, depth: usize) -> String {
        match item {
            Value::String(s) => self.inline(s),
            Value::Object(_) => {
                let ty = item.get("type").and_then(Value::as_str).unwrap_or("");
                if ty == "item" || ty == "itemSub" {
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                    let body = match (item.get("entry"), item.get("entries")) {
                        (Some(Value::String(s)), _) => self.inline(s),
                        (Some(other), _) => self.entry(other, depth + 1),
                        (None, Some(Value::Array(a))) => {
                            a.iter().map(|e| self.entry(e, depth + 1)).collect()
                        }
                        _ => String::new(),
                    };
                    if name.is_empty() {
                        body
                    } else {
                        format!("<strong>{}</strong> {body}", self.inline(name))
                    }
                } else if ty.starts_with("ref") {
                    let key = ["classFeature", "subclassFeature", "optionalfeature"]
                        .iter()
                        .find_map(|k| item.get(*k).and_then(Value::as_str))
                        .unwrap_or("");
                    esc(key.split('|').next().unwrap_or(""))
                } else {
                    self.entry(item, depth + 1)
                }
            }
            other => esc(&text_of(other)),
        }
    }

    fn list(&mut self, value: &Value, depth: usize) -> String {
        let style = value.get("style").and_then(Value::as_str).unwrap_or("");
        let tag = if style.contains("decimal") {
            "ol"
        } else {
            "ul"
        };
        let items: String = value
            .get("items")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|i| format!("<li>{}</li>", self.list_item_body(i, depth)))
                    .collect()
            })
            .unwrap_or_default();
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .map(|n| format!("<p><strong>{}</strong></p>", self.inline(n)))
            .unwrap_or_default();
        format!("{name}<{tag}>{items}</{tag}>")
    }

    fn cell(&mut self, cell: &Value) -> String {
        match cell {
            Value::String(s) => self.inline(s),
            Value::Number(n) => n.to_string(),
            Value::Object(_) => {
                let ty = cell.get("type").and_then(Value::as_str).unwrap_or("");
                match ty {
                    "cell" => {
                        let roll = cell.get("roll").map(|r| {
                            if let Some(e) = r.get("exact") {
                                text_of(e)
                            } else {
                                format!(
                                    "{}\u{2013}{}",
                                    r.get("min").map(text_of).unwrap_or_default(),
                                    r.get("max").map(text_of).unwrap_or_default()
                                )
                            }
                        });
                        match (roll, cell.get("entry")) {
                            (Some(r), _) => esc(&r),
                            (None, Some(e)) => self.cell(e),
                            _ => String::new(),
                        }
                    }
                    "bonus" => esc(&signed_number(
                        cell.get("value").map(text_of).unwrap_or_default(),
                    )),
                    "bonusSpeed" => esc(&format!(
                        "{} ft.",
                        signed_number(cell.get("value").map(text_of).unwrap_or_default())
                    )),
                    "dice" => dice_text(cell),
                    _ => {
                        let html = self.entry(cell, 1);
                        html.replace("<p>", "")
                            .replace("</p>", " ")
                            .trim()
                            .to_string()
                    }
                }
            }
            _ => String::new(),
        }
    }

    fn table(&mut self, value: &Value) -> String {
        if value.get("type").and_then(Value::as_str) == Some("tableGroup") {
            return value
                .get("tables")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|t| self.table(t)).collect())
                .unwrap_or_default();
        }
        let mut out = String::from("<table>");
        if let Some(caption) = value.get("caption").and_then(Value::as_str) {
            out.push_str(&format!("<caption>{}</caption>", self.inline(caption)));
        }
        if let Some(labels) = value.get("colLabels").and_then(Value::as_array) {
            out.push_str("<thead><tr>");
            for l in labels {
                out.push_str(&format!("<th>{}</th>", self.cell(l)));
            }
            out.push_str("</tr></thead>");
        }
        out.push_str("<tbody>");
        if let Some(rows) = value.get("rows").and_then(Value::as_array) {
            for row in rows {
                let cells = match row {
                    Value::Array(a) => Some(a.as_slice()),
                    Value::Object(_) => row.get("row").and_then(Value::as_array).map(Vec::as_slice),
                    _ => None,
                };
                if let Some(cells) = cells {
                    out.push_str("<tr>");
                    for c in cells {
                        out.push_str(&format!("<td>{}</td>", self.cell(c)));
                    }
                    out.push_str("</tr>");
                }
            }
        }
        out.push_str("</tbody></table>");
        out
    }

    /// Plain text version of entries (tags resolved, no markup), paragraphs separated by a
    /// blank line.
    #[must_use]
    pub fn plain(entries: &[Value]) -> String {
        let mut parts = Vec::new();
        for e in entries {
            collect_plain(e, &mut parts);
        }
        parts.join("\n\n")
    }
}

fn collect_plain(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(strip_tags(s)),
        Value::Array(a) => a.iter().for_each(|e| collect_plain(e, out)),
        Value::Object(_) => {
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                if value.get("entries").is_some() || value.get("entry").is_some() {
                    out.push(strip_tags(name));
                }
            }
            for key in ["entries", "items", "entry"] {
                if let Some(v) = value.get(key) {
                    collect_plain(v, out);
                }
            }
            if let Some(rows) = value.get("rows").and_then(Value::as_array) {
                for r in rows {
                    if let Some(cells) = r.as_array() {
                        let line: Vec<String> = cells
                            .iter()
                            .map(|c| match c {
                                Value::String(s) => strip_tags(s),
                                other => text_of(other),
                            })
                            .collect();
                        out.push(line.join(" | "));
                    }
                }
            }
        }
        _ => {}
    }
}

fn signed_number(n: String) -> String {
    if n.starts_with('+') || n.starts_with('-') {
        n
    } else {
        format!("+{n}")
    }
}

fn dice_text(value: &Value) -> String {
    if let Some(Value::Array(rolls)) = value.get("toRoll") {
        let parts: Vec<String> = rolls
            .iter()
            .map(|r| {
                format!(
                    "{}d{}",
                    r.get("number").map(text_of).unwrap_or_default(),
                    r.get("faces").map(text_of).unwrap_or_default()
                )
            })
            .collect();
        return esc(&parts.join(" + "));
    }
    esc(&value.get("rollable").map(text_of).unwrap_or_default())
}

fn ability_name(abbr: &str) -> &'static str {
    match abbr {
        "str" => "Strength",
        "dex" => "Dexterity",
        "con" => "Constitution",
        "int" => "Intelligence",
        "wis" => "Wisdom",
        "cha" => "Charisma",
        _ => "spellcasting ability",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn none(_: RefKind, _: &str, _: &str) -> Option<String> {
        None
    }

    fn render(v: Value) -> String {
        let mut r = Renderer::new(&none);
        r.entries(v.as_array().unwrap())
    }

    #[test]
    fn paragraphs_and_named_sections() {
        let html = render(json!([
            "Plain {@b text}.",
            {"type": "entries", "name": "Top", "entries": [
                "intro",
                {"type": "entries", "name": "Inner", "entries": ["body one", "body two"]}
            ]}
        ]));
        assert_eq!(
            html,
            "<p>Plain <strong>text</strong>.</p><h4>Top</h4><p>intro</p>\
             <p><strong>Inner.</strong> body one</p><p>body two</p>"
        );
    }

    #[test]
    fn lists_tables_and_insets() {
        let html = render(json!([
            {"type": "list", "items": ["a", {"type": "item", "name": "N:", "entry": "v"}]},
            {"type": "list", "style": "list-decimal", "items": ["x"]},
            {"type": "table", "caption": "Cap", "colLabels": ["L", "R"],
             "rows": [["a", 1], [{"type": "cell", "roll": {"min": 1, "max": 3}}, {"type": "bonus", "value": 2}]]},
            {"type": "inset", "name": "Note", "entries": ["in"]}
        ]));
        assert!(html.contains("<ul><li>a</li><li><strong>N:</strong> v</li></ul>"));
        assert!(html.contains("<ol><li>x</li></ol>"));
        assert!(html.contains("<caption>Cap</caption><thead><tr><th>L</th><th>R</th></tr></thead>"));
        assert!(html.contains("<tr><td>a</td><td>1</td></tr>"));
        assert!(html.contains("<td>1\u{2013}3</td><td>+2</td>"));
        assert!(html.contains("<div class=\"inset\"><h5>Note</h5><p>in</p></div>"));
    }

    #[test]
    fn references_options_ability_lines_and_media() {
        let mut r = Renderer::new(&none);
        let html = r.entries(
            json!([
                {"type": "options", "count": 1, "entries": [
                    {"type": "refOptionalfeature", "optionalfeature": "Archery"},
                    {"type": "refOptionalfeature", "optionalfeature": "Blind Fighting|TCE"}]},
                {"type": "refSubclassFeature", "subclassFeature": "Improved Critical|Fighter||Champion||3"},
                {"type": "abilityDc", "name": "Wizard", "attributes": ["int"]},
                {"type": "abilityAttackMod", "name": "Wizard", "attributes": ["int", "wis"]},
                {"type": "image", "href": {"type": "internal", "path": "x.png"}},
                {"type": "mystery", "entries": ["still shown"]},
                {"type": "mystery2"}
            ])
            .as_array()
            .unwrap(),
        );
        assert!(html
            .contains("<p><em>Choose 1:</em></p><ul><li>Archery</li><li>Blind Fighting</li></ul>"));
        assert!(html.contains("<p class=\"ref\">Improved Critical</p>"));
        assert!(html.contains(
            "Wizard save DC</strong> = 8 + your proficiency bonus + your Intelligence modifier"
        ));
        assert!(html.contains("your Intelligence or Wisdom modifier"));
        assert!(html.contains("still shown"));
        assert_eq!(r.dropped_media, 1);
        assert!(r.unknown_types.contains("mystery"));
        assert!(r.unknown_types.contains("mystery2"));
    }

    #[test]
    fn plain_text_extraction() {
        let text = Renderer::plain(
            json!([
                "Hit for {@damage 1d6} damage.",
                {"type": "entries", "name": "Sub", "entries": ["body"]},
                {"type": "table", "rows": [["a", "{@b b}"]]}
            ])
            .as_array()
            .unwrap(),
        );
        assert_eq!(text, "Hit for 1d6 damage.\n\nSub\n\nbody\n\na | b");
    }

    #[test]
    fn dice_and_quotes() {
        let html = render(json!([
            {"type": "dice", "toRoll": [{"number": 2, "faces": 6}, {"number": 1, "faces": 4}]},
            {"type": "quote", "entries": ["words"], "by": "Someone"},
            {"type": "inline", "entries": ["a ", {"type": "dice", "rollable": "1d4"}]},
            {"type": "wrapper", "wrapped": "wrapped text"},
            {"type": "hr"}
        ]));
        assert!(html.contains("<p>2d6 + 1d4</p>"));
        assert!(html.contains("<p><em>words</em></p><p>\u{2014} Someone</p>"));
        assert!(html.contains("<p>wrapped text</p><hr />"));
    }
}
