//! Conversion of the non-rule child nodes of `<element>`.

use super::util::{
    bool_attr, opt_attr, required_attr, u32_attr, unknown_node, CResult, ConvertError, Ctx,
};
use crate::dom::{write_node, XmlElement, XmlNode};
use borealis_content::{
    Compendium, Equipment, Extra, Extract, ExtractItem, IssueCode, Setter, Sheet, SheetEntry,
    SourceRef, Spellcasting, SpellcastingList,
};
use std::collections::BTreeMap;

/// `<setters>`: every element child must carry a `name`; other attributes are kept.
pub(crate) fn setters(node: &XmlElement) -> CResult<Vec<Setter>> {
    let mut out = Vec::new();
    for child in node.elements() {
        let name = child.attr("name").ok_or_else(|| {
            ConvertError::new(
                IssueCode::MissingAttribute,
                format!("<{}> in <setters> has no `name` attribute", child.name),
                child,
            )
        })?;
        let attributes: BTreeMap<String, String> = child
            .attrs
            .iter()
            .filter(|(k, _)| k != "name")
            .cloned()
            .collect();
        out.push(Setter {
            name: name.trim().to_string(),
            value: child.text().trim().to_string(),
            attributes,
        });
    }
    Ok(out)
}

/// The description HTML, preserving mixed content.
///
/// * a leading CDATA section contributes its raw text (trimmed)
/// * a leading `<html>` element contributes its inner XML (or its text when it starts
///   with text)
/// * otherwise children are serialized back to XML text
///
/// Anything that follows a leading CDATA or `<html>` is appended instead of dropped.
pub(crate) fn description(node: &XmlElement) -> String {
    let mut out = String::new();
    let mut rest: &[XmlNode] = &node.children;
    match node.children.first() {
        Some(XmlNode::CData(text)) => {
            out.push_str(text.trim());
            rest = &node.children[1..];
        }
        Some(XmlNode::Element(html)) if html.name.eq_ignore_ascii_case("html") => {
            match html.children.first() {
                Some(XmlNode::Text(_) | XmlNode::CData(_)) => out.push_str(html.text().trim()),
                _ => out.push_str(&html.inner_xml()),
            }
            rest = &node.children[1..];
        }
        _ => {}
    }
    for child in rest {
        write_node(child, &mut out);
    }
    out.trim().to_string()
}

/// `<sheet>`.
pub(crate) fn sheet(ctx: &mut Ctx, node: &XmlElement) -> CResult<Sheet> {
    let mut out = Sheet {
        display: bool_attr(node, "display")?,
        alt: opt_attr(node, "alt"),
        usage: opt_attr(node, "usage"),
        action: opt_attr(node, "action"),
        ..Sheet::default()
    };
    ctx.keep_attributes(
        &mut out.extra,
        "sheet",
        node,
        &["display", "alt", "usage", "action"],
    );
    for child in node.elements() {
        if child.name != "description" {
            ctx.report(
                borealis_content::Severity::Info,
                IssueCode::Normalized,
                format!(
                    "sheet entry tag <{}> read as <description> (the tag name is not significant)",
                    child.name
                ),
                None,
                Some(child.line),
            );
            ctx.note_normalized("sheet entry tag");
        }
        let mut entry = SheetEntry {
            text: child.text().trim().to_string(),
            level: u32_attr(child, "level")?,
            usage: opt_attr(child, "usage"),
            action: opt_attr(child, "action"),
            ..SheetEntry::default()
        };
        ctx.keep_attributes(
            &mut entry.extra,
            "sheet/description",
            child,
            &["level", "usage", "action"],
        );
        out.descriptions.push(entry);
    }
    Ok(out)
}

/// `<compendium>`.
pub(crate) fn compendium(ctx: &mut Ctx, node: &XmlElement) -> CResult<Compendium> {
    let mut out = Compendium {
        display: bool_attr(node, "display")?,
        ..Compendium::default()
    };
    ctx.keep_attributes(&mut out.extra, "compendium", node, &["display"]);
    for child in node.elements() {
        ctx.keep_child(&mut out.extra, "compendium", child);
    }
    Ok(out)
}

fn list_entry(ctx: &mut Ctx, node: &XmlElement, owner: &str) -> CResult<SpellcastingList> {
    let mut out = SpellcastingList {
        supports: node.text().trim().to_string(),
        known: bool_attr(node, "known")?,
        ..SpellcastingList::default()
    };
    ctx.keep_attributes(&mut out.extra, owner, node, &["known"]);
    Ok(out)
}

/// `<spellcasting>`. At most one `<list>` is used (the last wins); earlier ones are kept
/// in `extra.children`.
pub(crate) fn spellcasting(ctx: &mut Ctx, node: &XmlElement) -> CResult<Spellcasting> {
    let mut out = Spellcasting {
        name: opt_attr(node, "name"),
        ability: opt_attr(node, "ability"),
        prepare: bool_attr(node, "prepare")?,
        allow_replace: bool_attr(node, "allowReplace")?,
        extend: bool_attr(node, "extend")?,
        all: bool_attr(node, "all")?,
        ..Spellcasting::default()
    };
    ctx.keep_attributes(
        &mut out.extra,
        "spellcasting",
        node,
        &[
            "name",
            "ability",
            "prepare",
            "allowReplace",
            "extend",
            "all",
        ],
    );
    let lists: Vec<&XmlElement> = node.elements().filter(|c| c.name == "list").collect();
    for child in node.elements() {
        match child.name.as_str() {
            "list" => {
                let is_last = lists.last().is_some_and(|l| std::ptr::eq(*l, child));
                if is_last {
                    out.list = Some(list_entry(ctx, child, "spellcasting/list")?);
                } else {
                    ctx.warn(
                        IssueCode::DuplicateChild,
                        "multiple <list> nodes; the last one is used",
                        child,
                    );
                    ctx.keep_child_silent(&mut out.extra, "spellcasting", child);
                }
            }
            "extend" => out
                .extends
                .push(list_entry(ctx, child, "spellcasting/extend")?),
            _ => ctx.keep_child(&mut out.extra, "spellcasting", child),
        }
    }
    Ok(out)
}

/// `<equipment name="..">` with an optional description.
pub(crate) fn equipment(ctx: &mut Ctx, node: &XmlElement) -> Equipment {
    let mut out = Equipment {
        name: opt_attr(node, "name"),
        ..Equipment::default()
    };
    ctx.keep_attributes(&mut out.extra, "equipment", node, &["name"]);
    for child in node.elements() {
        if child.name == "description" && out.description.is_none() {
            out.description = Some(description(child));
        } else {
            ctx.keep_child(&mut out.extra, "equipment", child);
        }
    }
    out
}

/// Element-level `<source>`: attributes `name`/`id`, or plain text (`ID_` prefix = id).
pub(crate) fn source_ref(ctx: &mut Ctx, node: &XmlElement) -> SourceRef {
    let mut out = SourceRef {
        name: opt_attr(node, "name"),
        id: opt_attr(node, "id"),
        ..SourceRef::default()
    };
    ctx.keep_attributes(&mut out.extra, "source", node, &["name", "id"]);
    for child in node.elements() {
        match child.name.as_str() {
            "url" if out.url.is_none() => out.url = Some(child.text().trim().to_string()),
            "page" if out.page.is_none() => out.page = Some(child.text().trim().to_string()),
            _ => ctx.keep_child(&mut out.extra, "source", child),
        }
    }
    let text = if node.has_element_children() {
        String::new()
    } else {
        node.text().trim().to_string()
    };
    if !text.is_empty() && out.name.is_none() && out.id.is_none() {
        if text.starts_with("ID_") {
            out.id = Some(text);
        } else {
            out.name = Some(text);
        }
    }
    out
}

/// Item `<extract>`.
pub(crate) fn extract(ctx: &mut Ctx, node: &XmlElement) -> Extract {
    let mut out = Extract::default();
    ctx.keep_attributes(&mut out.extra, "extract", node, &[]);
    for child in node.elements() {
        match child.name.as_str() {
            "item" => {
                let mut item = ExtractItem {
                    id: child.text().trim().to_string(),
                    ..ExtractItem::default()
                };
                if let Some(raw) = child.attr("amount") {
                    match raw.trim().parse::<u32>() {
                        Ok(n) if n >= 1 => item.amount = Some(n),
                        _ => {
                            ctx.warn(
                                IssueCode::InvalidInteger,
                                format!("extract item amount {raw:?} is not a positive integer; read as 1"),
                                child,
                            );
                            item.extra
                                .attributes
                                .insert("amount".into(), raw.to_string());
                        }
                    }
                }
                ctx.keep_attributes(&mut item.extra, "extract/item", child, &["amount"]);
                out.items.push(item);
            }
            "description" if out.description.is_none() => {
                out.description = Some(description(child))
            }
            _ => ctx.keep_child(&mut out.extra, "extract", child),
        }
    }
    out
}

/// `<supports>` or `<prerequisite>` style tag lists share this extra-children helper.
pub(crate) fn push_dup(ctx: &mut Ctx, extra: &mut Extra, owner: &str, el: &XmlElement) {
    ctx.warn(
        IssueCode::DuplicateChild,
        format!(
            "<{}> appears more than once in <{owner}>; only the first is used",
            el.name
        ),
        el,
    );
    extra.children.push(unknown_node(el));
    *ctx.stats
        .unknown_children
        .entry(format!("{owner}/{}", el.name))
        .or_default() += 1;
}

/// Required `id` of a `<multiclass>` node.
pub(crate) fn multiclass_id(node: &XmlElement) -> CResult<String> {
    required_attr(node, "id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_document;

    fn ctx() -> Ctx {
        Ctx::new("t.xml")
    }

    #[test]
    fn setters_keep_attributes_and_flatten_markup() {
        let el = parse_document(
            r#"<setters><set name="cost" currency="gp"> 15 </set><set name="x"><b>a</b>b</set><!-- c --></setters>"#,
        )
        .unwrap();
        let s = setters(&el).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            (s[0].value.as_str(), s[0].attributes["currency"].as_str()),
            ("15", "gp")
        );
        assert_eq!(s[1].value, "ab");
        let bad = parse_document("<setters><set>x</set></setters>").unwrap();
        assert_eq!(setters(&bad).unwrap_err().code, IssueCode::MissingAttribute);
    }

    #[test]
    fn description_variants() {
        let d = |xml: &str| description(&parse_document(xml).unwrap());
        assert_eq!(
            d("<description>\n <p>a &amp; b</p>\n <p/>\n</description>"),
            "<p>a &amp; b</p><p />"
        );
        assert_eq!(
            d("<description><![CDATA[<p>raw & ok</p>]]></description>"),
            "<p>raw & ok</p>"
        );
        assert_eq!(
            d("<description><html><p>x</p></html></description>"),
            "<p>x</p>"
        );
        assert_eq!(d("<description><html>plain</html></description>"), "plain");
        assert_eq!(
            d("<description>Intro <b>x</b></description>"),
            "Intro <b>x</b>"
        );
        assert_eq!(d("<description/>"), "");
        assert_eq!(
            d("<description><![CDATA[a]]><p>b</p></description>"),
            "a<p>b</p>"
        );
    }

    #[test]
    fn sheet_entries_and_defaults() {
        let el = parse_document(
            r#"<sheet display="false" name="x" usage="1/Day"><description level="3" action="">t {{a}}</description><descr>u</descr></sheet>"#,
        )
        .unwrap();
        let mut c = ctx();
        let s = sheet(&mut c, &el).unwrap();
        assert_eq!(s.display, Some(false));
        assert_eq!(s.extra.attributes["name"], "x");
        assert_eq!(s.descriptions[0].level, Some(3));
        assert_eq!(s.descriptions[0].action.as_deref(), Some(""));
        assert_eq!(s.descriptions[1].text, "u");
        assert_eq!(c.stats.normalized["sheet entry tag"], 1);
        let bad = parse_document(r#"<sheet display="maybe"/>"#).unwrap();
        assert_eq!(
            sheet(&mut ctx(), &bad).unwrap_err().code,
            IssueCode::InvalidBoolean
        );
    }

    #[test]
    fn spellcasting_list_last_wins_and_extends() {
        let el = parse_document(
            r#"<spellcasting name="W" ability="Int" prepare="true" extend="false" odd="1">
                 <list>First</list><list known="true">Second</list><extend>ID_X</extend><weird/></spellcasting>"#,
        )
        .unwrap();
        let mut c = ctx();
        let sc = spellcasting(&mut c, &el).unwrap();
        assert_eq!(sc.list.as_ref().unwrap().supports, "Second");
        assert_eq!(sc.list.as_ref().unwrap().known, Some(true));
        assert_eq!(sc.extends.len(), 1);
        assert_eq!(sc.extra.attributes["odd"], "1");
        assert_eq!(sc.extra.children.len(), 2);
        // the unknown `odd` attribute, the duplicate <list> and <weird/> are reported
        assert_eq!(c.issues.len(), 3);
    }

    #[test]
    fn extract_amounts() {
        let el = parse_document(
            r#"<extract><item>ID_A</item><item amount="5">ID_B</item><item amount="zero">ID_C</item><description><p>d</p></description></extract>"#,
        )
        .unwrap();
        let mut c = ctx();
        let x = extract(&mut c, &el);
        assert_eq!(x.items.len(), 3);
        assert_eq!(x.items[1].amount, Some(5));
        assert_eq!(x.items[2].amount, None);
        assert_eq!(x.items[2].extra.attributes["amount"], "zero");
        assert_eq!(x.description.as_deref(), Some("<p>d</p>"));
        assert_eq!(c.issues.len(), 1);
    }

    #[test]
    fn source_ref_forms() {
        let t = |xml: &str| source_ref(&mut ctx(), &parse_document(xml).unwrap());
        assert_eq!(t("<source>ID_SRC</source>").id.as_deref(), Some("ID_SRC"));
        assert_eq!(
            t("<source>Some Book</source>").name.as_deref(),
            Some("Some Book")
        );
        let full = t(r#"<source name="N"><url>u</url><page>3</page></source>"#);
        assert_eq!(
            (
                full.name.as_deref(),
                full.url.as_deref(),
                full.page.as_deref()
            ),
            (Some("N"), Some("u"), Some("3"))
        );
    }

    #[test]
    fn equipment_and_compendium() {
        let mut c = ctx();
        let eq = equipment(
            &mut c,
            &parse_document(
                r#"<equipment name="E"><description><p>x</p></description></equipment>"#,
            )
            .unwrap(),
        );
        assert_eq!(eq.name.as_deref(), Some("E"));
        assert_eq!(eq.description.as_deref(), Some("<p>x</p>"));
        let comp = compendium(
            &mut c,
            &parse_document(r#"<compendium display="false" x="y"/>"#).unwrap(),
        )
        .unwrap();
        assert_eq!(comp.display, Some(false));
        assert_eq!(comp.extra.attributes["x"], "y");
    }
}
