//! Conversion of `<rules>` children: `grant`, `select`, `stat`.

use super::parts;
use super::util::{bool_attr, opt_attr, u32_attr, CResult, ConvertError, Ctx};
use crate::dom::{XmlElement, XmlNode};
use borealis_content::legacy::{map_stat_name, map_stat_value};
use borealis_content::{
    Extra, GrantRule, IssueCode, ListItem, Rule, SelectRule, Setter, Severity, StatRule,
    UnknownRule,
};

/// The element a rule list belongs to, for messages and defaults.
pub(crate) struct Owner<'a> {
    pub ty: &'a str,
    pub name: &'a str,
}

/// Convert a `<rules>` node. Unknown rule tags are reported as errors and dropped; the
/// never-implemented `<inline>` tag is kept as an `unknown` rule with a warning.
pub(crate) fn convert_rules(
    ctx: &mut Ctx,
    node: &XmlElement,
    owner: &Owner<'_>,
) -> CResult<Vec<Rule>> {
    let mut out = Vec::new();
    for child in &node.children {
        let XmlNode::Element(child) = child else {
            continue;
        };
        let rule = match child.name.to_lowercase().as_str() {
            "grant" => {
                let r = grant(ctx, child);
                match rule_or_skip(ctx, r, child)? {
                    Some(r) => Rule::Grant(r),
                    None => continue,
                }
            }
            "select" => Rule::Select(select(ctx, child, owner)?),
            "stat" => Rule::Stat(stat(ctx, child)?),
            "statistic" => {
                ctx.note_normalized("statistic -> stat");
                Rule::Stat(stat(ctx, child)?)
            }
            "inline" => {
                ctx.warn(
                    IssueCode::UnsupportedConstruct,
                    "<inline> rules have no effect; kept as an `unknown` rule",
                    child,
                );
                Rule::Unknown(UnknownRule {
                    tag: child.name.clone(),
                    attributes: child.attrs.iter().cloned().collect(),
                    text: Some(child.text().trim().to_string()).filter(|t| !t.is_empty()),
                })
            }
            other => {
                ctx.report(
                    Severity::Error,
                    IssueCode::UnknownRuleTag,
                    format!("unknown rule tag <{other}> dropped (not a known rule kind)"),
                    None,
                    Some(child.line),
                );
                continue;
            }
        };
        out.push(rule);
    }
    Ok(out)
}

/// A `grant` without target breaks only itself (required attributes are not enforced at
/// parse time in legacy files), so it is dropped with a warning instead of discarding the
/// whole element. Other errors still fail the element.
fn rule_or_skip<T>(ctx: &mut Ctx, result: CResult<T>, node: &XmlElement) -> CResult<Option<T>> {
    match result {
        Ok(rule) => Ok(Some(rule)),
        Err(e) if e.code == IssueCode::MissingAttribute => {
            ctx.warn(
                IssueCode::MissingAttribute,
                format!("rule dropped, the element is kept: {}", e.message),
                node,
            );
            ctx.stats.dropped_rules += 1;
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Child `<set>` nodes of a rule; any other child goes to `extra.children`.
fn rule_children(
    ctx: &mut Ctx,
    node: &XmlElement,
    owner: &str,
    extra: &mut Extra,
    skip: &[&str],
) -> CResult<Vec<Setter>> {
    let mut sets = Vec::new();
    for child in node.elements() {
        if child.name == "set" {
            let one = XmlElement {
                name: "setters".into(),
                attrs: Vec::new(),
                children: vec![XmlNode::Element(child.clone())],
                line: child.line,
            };
            sets.extend(parts::setters(&one)?);
        } else if !skip.contains(&child.name.as_str()) {
            ctx.keep_child(extra, owner, child);
        }
    }
    Ok(sets)
}

fn grant(ctx: &mut Ctx, node: &XmlElement) -> CResult<GrantRule> {
    // `name` is a legacy alias of `id`; whichever occurs later in the XML wins.
    let mut id: Option<String> = None;
    let mut used_alias = false;
    for (k, v) in &node.attrs {
        match k.as_str() {
            "id" => {
                id = Some(v.trim().to_string());
                used_alias = false;
            }
            "name" => {
                id = Some(v.trim().to_string());
                used_alias = true;
            }
            _ => {}
        }
    }
    if used_alias {
        ctx.note_normalized("grant@name -> id");
    }
    let id = id.filter(|i| !i.is_empty()).ok_or_else(|| {
        ConvertError::new(
            IssueCode::MissingAttribute,
            "<grant> has neither `id` nor `name`",
            node,
        )
    })?;
    let mut rule = GrantRule {
        id,
        ty: opt_attr(node, "type"),
        level: u32_attr(node, "level")?,
        requirements: opt_attr(node, "requirements"),
        spellcasting: opt_attr(node, "spellcasting"),
        prepared: bool_attr(node, "prepared")?,
        ..GrantRule::default()
    };
    ctx.keep_attributes(
        &mut rule.extra,
        "grant",
        node,
        &[
            "id",
            "name",
            "type",
            "level",
            "requirements",
            "spellcasting",
            "prepared",
        ],
    );
    let mut extra = std::mem::take(&mut rule.extra);
    rule.setters = rule_children(ctx, node, "grant", &mut extra, &[])?;
    rule.extra = extra;
    Ok(rule)
}

fn list_items(ctx: &mut Ctx, node: &XmlElement) -> CResult<Vec<ListItem>> {
    let mut items = Vec::new();
    for child in node.elements() {
        if child.name != "item" {
            continue;
        }
        let raw = child.attr("id").ok_or_else(|| {
            ConvertError::new(
                IssueCode::MissingAttribute,
                "list <item> has no `id`",
                child,
            )
        })?;
        let id = raw.trim().parse::<i64>().map_err(|_| {
            ConvertError::new(
                IssueCode::InvalidInteger,
                format!("list <item> id {raw:?} is not an integer"),
                child,
            )
        })?;
        let mut item = ListItem {
            id,
            text: child.text().trim().to_string(),
            ..ListItem::default()
        };
        ctx.keep_attributes(&mut item.extra, "select/item", child, &["id"]);
        items.push(item);
    }
    Ok(items)
}

fn select(ctx: &mut Ctx, node: &XmlElement, owner: &Owner<'_>) -> CResult<SelectRule> {
    let ty = opt_attr(node, "type")
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            ConvertError::new(IssueCode::MissingAttribute, "<select> has no `type`", node)
        })?;
    let name = match opt_attr(node, "name")
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
    {
        Some(n) => n,
        None => {
            let substitute = format!("{} ({})", owner.ty, owner.name);
            ctx.warn(
                IssueCode::Normalized,
                format!("<select> has no name; using {substitute:?}"),
                node,
            );
            ctx.note_normalized("select blank name");
            substitute
        }
    };
    let is_list = ty == "List";
    let mut rule = SelectRule {
        ty,
        name,
        number: u32_attr(node, "number")?,
        level: u32_attr(node, "level")?,
        requirements: opt_attr(node, "requirements"),
        supports: opt_attr(node, "supports"),
        optional: bool_attr(node, "optional")?,
        default_id: opt_attr(node, "default"),
        spellcasting: opt_attr(node, "spellcasting"),
        prepared: bool_attr(node, "prepared")?,
        allow_replace: bool_attr(node, "allowReplace")?,
        default_behaviour: opt_attr(node, "default-behaviour"),
        existing: bool_attr(node, "existing")?,
        stat: opt_attr(node, "stat"),
        ..SelectRule::default()
    };
    ctx.keep_attributes(
        &mut rule.extra,
        "select",
        node,
        &[
            "type",
            "name",
            "number",
            "level",
            "requirements",
            "supports",
            "optional",
            "default",
            "spellcasting",
            "prepared",
            "allowReplace",
            "default-behaviour",
            "existing",
            "stat",
        ],
    );
    let mut extra = std::mem::take(&mut rule.extra);
    if is_list {
        rule.items = list_items(ctx, node)?;
    }
    let skip: &[&str] = if is_list { &["item"] } else { &[] };
    rule.setters = rule_children(ctx, node, "select", &mut extra, skip)?;
    rule.extra = extra;
    Ok(rule)
}

fn stat(ctx: &mut Ctx, node: &XmlElement) -> CResult<StatRule> {
    let name = node.attr("name").ok_or_else(|| {
        ConvertError::new(IssueCode::MissingAttribute, "<stat> has no `name`", node)
    })?;
    let value = node.attr("value").ok_or_else(|| {
        ConvertError::new(IssueCode::MissingAttribute, "<stat> has no `value`", node)
    })?;
    let inline = bool_attr(node, "inline")?;
    // `cap` is a legacy alias of `maximum`; the later attribute wins.
    let mut maximum = None;
    let mut used_cap = false;
    for (k, v) in &node.attrs {
        match k.as_str() {
            "maximum" => {
                maximum = Some(v.clone());
                used_cap = false;
            }
            "cap" => {
                maximum = Some(v.clone());
                used_cap = true;
            }
            _ => {}
        }
    }
    if used_cap {
        ctx.note_normalized("cap -> maximum");
    }
    let mapped_name = map_stat_name(name.trim());
    if mapped_name != name.trim() {
        ctx.note_normalized("stat name lower-cased/legacy-mapped");
    }
    let mut rule = StatRule {
        name: mapped_name,
        value: if inline == Some(true) {
            value.to_string()
        } else {
            map_stat_value(value)
        },
        bonus: opt_attr(node, "bonus"),
        level: u32_attr(node, "level")?,
        requirements: opt_attr(node, "requirements"),
        equipped: opt_attr(node, "equipped"),
        not_equipped: opt_attr(node, "not-equipped"),
        inline,
        maximum,
        minimum: opt_attr(node, "minimum"),
        alt: opt_attr(node, "alt"),
        merge: bool_attr(node, "merge")?,
        ..StatRule::default()
    };
    ctx.keep_attributes(
        &mut rule.extra,
        "stat",
        node,
        &[
            "name",
            "value",
            "bonus",
            "level",
            "requirements",
            "equipped",
            "not-equipped",
            "inline",
            "maximum",
            "cap",
            "minimum",
            "alt",
            "merge",
        ],
    );
    let mut extra = std::mem::take(&mut rule.extra);
    rule.setters = rule_children(ctx, node, "stat", &mut extra, &[])?;
    rule.extra = extra;
    Ok(rule)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_document;

    fn convert(xml: &str) -> (Vec<Rule>, Ctx) {
        let node = parse_document(xml).unwrap();
        let mut ctx = Ctx::new("t.xml");
        let rules = convert_rules(
            &mut ctx,
            &node,
            &Owner {
                ty: "Class",
                name: "Fighter",
            },
        )
        .unwrap();
        (rules, ctx)
    }

    fn convert_err(xml: &str) -> ConvertError {
        let node = parse_document(xml).unwrap();
        let mut ctx = Ctx::new("t.xml");
        convert_rules(
            &mut ctx,
            &node,
            &Owner {
                ty: "Class",
                name: "Fighter",
            },
        )
        .unwrap_err()
    }

    #[test]
    fn grant_aliases_and_attributes() {
        let (rules, ctx) = convert(
            r#"<rules><grant type="Spell" name="ID_OLD" level="3" spellcasting="Druid" prepared="true" requirements="!ID_X"/>
                      <grant id="ID_A" name="ID_B" extra="1"><set name="k">v</set><note/></grant></rules>"#,
        );
        let Rule::Grant(a) = &rules[0] else { panic!() };
        assert_eq!(
            (a.id.as_str(), a.level, a.prepared),
            ("ID_OLD", Some(3), Some(true))
        );
        let Rule::Grant(b) = &rules[1] else { panic!() };
        assert_eq!(b.id, "ID_B", "the later of id/name wins");
        assert_eq!(b.setters[0].name, "k");
        assert_eq!(b.extra.attributes["extra"], "1");
        assert_eq!(b.extra.children[0].tag, "note");
        assert_eq!(ctx.stats.normalized["grant@name -> id"], 2);
        // a grant without a target is dropped on its own; the element survives
        let (rules, ctx) = convert("<rules><grant type=\"T\"/><grant id=\"ID_K\"/></rules>");
        assert_eq!(rules.len(), 1);
        assert_eq!(ctx.stats.dropped_rules, 1);
        assert!(ctx
            .issues
            .iter()
            .any(|i| i.code == IssueCode::MissingAttribute && i.severity == Severity::Warning));
    }

    #[test]
    fn select_lists_defaults_and_blank_names() {
        let (rules, ctx) = convert(
            r#"<rules><select type="List" name="Ideal" number="2"><!--c--><item id="1">One</item><item id="2"> Two </item></select>
                      <select type="Spell" supports="A,B" default="ID_S" optional="TRUE" allowReplace="false"/></rules>"#,
        );
        let Rule::Select(list) = &rules[0] else {
            panic!()
        };
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[1].text, "Two");
        assert_eq!(list.number, Some(2));
        let Rule::Select(spell) = &rules[1] else {
            panic!()
        };
        assert_eq!(spell.name, "Class (Fighter)");
        assert_eq!(spell.optional, Some(true));
        assert_eq!(spell.allow_replace, Some(false));
        assert_eq!(spell.default_id.as_deref(), Some("ID_S"));
        assert_eq!(ctx.issues.len(), 1);
        assert_eq!(
            convert_err(r#"<rules><select name="x"/></rules>"#).code,
            IssueCode::MissingAttribute
        );
        assert_eq!(
            convert_err(
                r#"<rules><select type="List" name="x"><item id="a">t</item></select></rules>"#
            )
            .code,
            IssueCode::InvalidInteger
        );
        assert_eq!(
            convert_err(r#"<rules><select type="List" name="x"><item>t</item></select></rules>"#)
                .code,
            IssueCode::MissingAttribute
        );
    }

    #[test]
    fn stat_normalization() {
        let (rules, ctx) = convert(
            r#"<rules>
                 <stat name="Strength Maximum" value="+2" cap="Strength Modifier"/>
                 <statistic name="Proficiency" value="Proficiency Half Rounded Up" bonus="b" maximum="20"/>
                 <stat name="x" value="Fire" inline="true"/>
                 <STAT name="y" value="-Dexterity Modifier" level="5" not-equipped="[armor:any]" weird="1"/></rules>"#,
        );
        let stats: Vec<&StatRule> = rules
            .iter()
            .map(|r| match r {
                Rule::Stat(s) => s,
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            (stats[0].name.as_str(), stats[0].value.as_str()),
            ("strength:max", "2")
        );
        assert_eq!(stats[0].maximum.as_deref(), Some("Strength Modifier"));
        assert_eq!(
            (stats[1].name.as_str(), stats[1].value.as_str()),
            ("proficiency", "proficiency:half:up")
        );
        assert_eq!(stats[2].value, "Fire", "inline values are raw text");
        assert_eq!(stats[3].value, "-dexterity:modifier");
        assert_eq!(stats[3].level, Some(5));
        assert_eq!(stats[3].extra.attributes["weird"], "1");
        assert_eq!(ctx.stats.normalized["statistic -> stat"], 1);
        assert_eq!(ctx.stats.normalized["cap -> maximum"], 1);
        assert_eq!(
            convert_err(r#"<rules><stat name="x"/></rules>"#).code,
            IssueCode::MissingAttribute
        );
        assert_eq!(
            convert_err(r#"<rules><stat value="1"/></rules>"#).code,
            IssueCode::MissingAttribute
        );
        assert_eq!(
            convert_err(r#"<rules><stat name="x" value="1" level="x"/></rules>"#).code,
            IssueCode::InvalidInteger
        );
    }

    #[test]
    fn unknown_and_inline_tags() {
        let (rules, ctx) =
            convert(r#"<rules><inline name="x">y</inline><bogus/><grant id="ID_A"/></rules>"#);
        assert_eq!(rules.len(), 2);
        assert!(
            matches!(&rules[0], Rule::Unknown(u) if u.tag == "inline" && u.text.as_deref() == Some("y"))
        );
        assert!(ctx
            .issues
            .iter()
            .any(|i| i.code == IssueCode::UnknownRuleTag && i.severity == Severity::Error));
        assert!(ctx
            .issues
            .iter()
            .any(|i| i.code == IssueCode::UnsupportedConstruct));
    }
}
