//! XML to JSON conversion of a single Aurora file.

mod info;
mod parts;
mod rules;
mod typos;
mod util;

pub use util::{ConvertError, Ctx};

use crate::dom::{parse_document, XmlElement};
use crate::report::Stats;
use borealis_content::{
    Append, ContentFile, Element, Extra, IndexFile, Issue, IssueCode, Multiclass, Severity, Version,
};
use rules::{convert_rules, Owner};
use std::collections::HashSet;
use util::{bool_attr, opt_attr, required_attr, split_tags, CResult};

/// The converted document.
#[derive(Debug, Clone, PartialEq)]
pub enum Document {
    /// An `elements` file.
    Content(ContentFile),
    /// An `index` file.
    Index(IndexFile),
}

/// Result of converting one XML file. `document` is `None` when the file as a whole
/// could not be converted (the reason is in `issues`).
#[derive(Debug, Clone, PartialEq)]
pub struct FileConversion {
    /// The converted document, if any.
    pub document: Option<Document>,
    /// Diagnostics with file and line context.
    pub issues: Vec<Issue>,
    /// Counters for the report.
    pub stats: Stats,
}

/// Convert the text of one Aurora XML file. `file_name` is used for diagnostics, and a
/// `.index` suffix (or an `<index>` root) selects index conversion.
#[must_use]
pub fn convert_xml(text: &str, file_name: &str) -> FileConversion {
    let mut ctx = Ctx::new(file_name);
    if text.trim().is_empty() {
        ctx.report(
            Severity::Warning,
            IssueCode::FileSkipped,
            "file is empty; skipped",
            None,
            None,
        );
        return FileConversion {
            document: None,
            issues: ctx.issues,
            stats: ctx.stats,
        };
    }
    let document = match parse_document(text) {
        Err(e) => {
            ctx.report(
                Severity::Error,
                IssueCode::XmlParse,
                e.message,
                None,
                Some(e.line),
            );
            None
        }
        Ok(root) if !is_aurora_root(&root, file_name) => {
            ctx.report(
                Severity::Info,
                IssueCode::FileSkipped,
                "not an Aurora file (root is neither <elements> nor <index>); skipped",
                None,
                None,
            );
            None
        }
        Ok(mut root) => {
            typos::repair(&mut ctx, &mut root);
            let is_index = root.name == "index" || file_name.ends_with(".index");
            let converted = if is_index {
                info::index_file(&mut ctx, &root).map(Document::Index)
            } else {
                content_file(&mut ctx, &root).map(Document::Content)
            };
            match converted {
                Ok(doc) => Some(doc),
                Err(e) => {
                    ctx.report(Severity::Error, e.code, e.message, None, e.line);
                    None
                }
            }
        }
    };
    if let Some(Document::Content(file)) = &document {
        ctx.stats.count_file(file);
    }
    FileConversion {
        document,
        issues: ctx.issues,
        stats: ctx.stats,
    }
}

fn is_aurora_root(root: &XmlElement, file_name: &str) -> bool {
    root.name == "elements" || root.name == "index" || file_name.ends_with(".index")
}

fn content_file(ctx: &mut Ctx, root: &XmlElement) -> CResult<ContentFile> {
    let mut file = ContentFile::new(Vec::new());
    if let Some(app) = root.attr("app") {
        Version::parse_app(app.trim())
            .map_err(|e| ConvertError::new(IssueCode::InvalidVersion, e.to_string(), root))?;
        file.min_app_version = Some(app.trim().to_string());
    }
    file.ignore = bool_attr(root, "ignore")?;
    ctx.keep_attributes(&mut file.extra, "elements", root, &["app", "ignore"]);

    let mut info_seen = false;
    for child in root.elements() {
        match child.name.as_str() {
            "info" if !info_seen => {
                info_seen = true;
                file.info = Some(info::info(ctx, child)?);
            }
            "element" => {
                let label = child.attr("id").map(|s| s.trim().to_string());
                match element(ctx, child) {
                    Ok(e) => file.elements.push(e),
                    Err(err) => {
                        ctx.stats.dropped_elements += 1;
                        let mut issue =
                            Issue::error(err.code, format!("element dropped: {}", err.message))
                                .in_file(ctx.file.clone());
                        if let Some(id) = label {
                            issue = issue.at_element(id);
                        }
                        if let Some(line) = err.line {
                            issue = issue.at_line(line);
                        }
                        if let Some(path) = err.path {
                            issue = issue.at_path(path);
                        }
                        ctx.issues.push(issue);
                    }
                }
            }
            "append" => match append(ctx, child) {
                Ok(Some(a)) => file.appends.push(a),
                Ok(None) => {}
                Err(err) => ctx.report(
                    Severity::Error,
                    err.code,
                    format!("append dropped: {}", err.message),
                    child.attr("id"),
                    err.line,
                ),
            },
            _ => ctx.keep_child(&mut file.extra, "elements", child),
        }
    }
    Ok(file)
}

fn header(el: &XmlElement, name: &str) -> CResult<String> {
    let value = required_attr(el, name)?;
    if value.is_empty() && name != "source" {
        return Err(ConvertError::new(
            IssueCode::MissingAttribute,
            format!("<element> attribute `{name}` is empty"),
            el,
        ));
    }
    Ok(value)
}

/// Singleton children use the first occurrence; later ones are kept in `extra`.
struct Singletons {
    seen: HashSet<&'static str>,
}

impl Singletons {
    fn first(&mut self, key: &'static str) -> bool {
        self.seen.insert(key)
    }
}

fn element(ctx: &mut Ctx, el: &XmlElement) -> CResult<Element> {
    let mut e = Element::new(
        header(el, "id")?,
        header(el, "name")?,
        header(el, "type")?,
        header(el, "source")?,
    );
    ctx.keep_attributes(
        &mut e.extra,
        "element",
        el,
        &["id", "name", "type", "source"],
    );
    let owner_ty = e.ty.clone();
    let owner_name = e.name.clone();
    let owner = Owner {
        ty: &owner_ty,
        name: &owner_name,
    };
    let prerequisite_tag = if el.child("prerequisite").is_some() {
        "prerequisite"
    } else {
        "prerequisites"
    };
    let mut once = Singletons {
        seen: HashSet::new(),
    };
    let mut extra = std::mem::take(&mut e.extra);

    for child in el.elements() {
        let key: &'static str = match child.name.as_str() {
            "supports" => "supports",
            "prerequisite" | "prerequisites" => "prerequisite",
            "requirements" => "requirements",
            "description" => "description",
            "sheet" => "sheet",
            "setters" => "setters",
            "rules" => "rules",
            "spellcasting" => "spellcasting",
            "equipment" => "equipment",
            "compendium" => "compendium",
            "source" => "source",
            "multiclass" => "multiclass",
            "extract" => "extract",
            "set" => {
                ctx.warn(
                    IssueCode::UnsupportedConstruct,
                    "<set> directly under <element> has no effect; kept in extra",
                    child,
                );
                ctx.keep_child_silent(&mut extra, "element", child);
                continue;
            }
            _ => {
                ctx.keep_child(&mut extra, "element", child);
                continue;
            }
        };
        if key == "prerequisite" && child.name != prerequisite_tag {
            parts::push_dup(ctx, &mut extra, "element", child);
            continue;
        }
        if !once.first(key) {
            parts::push_dup(ctx, &mut extra, "element", child);
            continue;
        }
        match key {
            "supports" => {
                let (tags, dups) = split_tags(&child.text());
                if dups > 0 {
                    // a repeated tag adds no meaning, so nothing is lost.
                    ctx.report(
                        Severity::Info,
                        IssueCode::DuplicateChild,
                        format!("{dups} repeated support tag(s) dropped"),
                        None,
                        Some(child.line),
                    );
                }
                e.supports = tags;
            }
            "prerequisite" => {
                if child.name == "prerequisites" {
                    ctx.note_normalized("prerequisites -> prerequisite");
                }
                e.prerequisite = child.text().trim().to_string();
            }
            "requirements" => e.requirements = child.text().trim().to_string(),
            "description" => e.description = parts::description(child),
            "sheet" => e.sheet = Some(parts::sheet(ctx, child).map_err(|x| x.at("sheet"))?),
            "setters" => e.setters = parts::setters(child).map_err(|x| x.at("setters"))?,
            "rules" => {
                e.rules = convert_rules(ctx, child, &owner).map_err(|x| x.at("rules"))?;
            }
            "spellcasting" => {
                e.spellcasting =
                    Some(parts::spellcasting(ctx, child).map_err(|x| x.at("spellcasting"))?);
            }
            "equipment" => e.equipment = Some(parts::equipment(ctx, child)),
            "compendium" => {
                e.compendium = Some(parts::compendium(ctx, child).map_err(|x| x.at("compendium"))?)
            }
            "source" => e.source_ref = Some(parts::source_ref(ctx, child)),
            "multiclass" => {
                e.multiclass =
                    Some(multiclass(ctx, child, &owner).map_err(|x| x.at("multiclass"))?);
            }
            "extract" => e.extract = Some(parts::extract(ctx, child)),
            _ => unreachable!("key list is closed"),
        }
    }
    e.extra = extra;
    Ok(e)
}

fn multiclass(ctx: &mut Ctx, node: &XmlElement, owner: &Owner<'_>) -> CResult<Multiclass> {
    let mut m = Multiclass {
        id: parts::multiclass_id(node)?,
        ..Multiclass::default()
    };
    ctx.keep_attributes(&mut m.extra, "multiclass", node, &["id"]);
    let mut once = Singletons {
        seen: HashSet::new(),
    };
    let mut extra = std::mem::take(&mut m.extra);
    for child in node.elements() {
        let key: &'static str = match child.name.as_str() {
            "prerequisite" | "prerequisites" => "prerequisite",
            "requirements" => "requirements",
            "description" => "description",
            "setters" => "setters",
            "rules" => "rules",
            _ => {
                ctx.keep_child(&mut extra, "multiclass", child);
                continue;
            }
        };
        if !once.first(key) {
            parts::push_dup(ctx, &mut extra, "multiclass", child);
            continue;
        }
        match key {
            "prerequisite" => m.prerequisite = child.text().trim().to_string(),
            "requirements" => m.requirements = child.text().trim().to_string(),
            "description" => m.description = parts::description(child),
            "setters" => m.setters = parts::setters(child)?,
            _ => m.rules = convert_rules(ctx, child, owner)?,
        }
    }
    m.extra = extra;
    Ok(m)
}

fn append(ctx: &mut Ctx, node: &XmlElement) -> CResult<Option<Append>> {
    let Some(id) = opt_attr(node, "id")
        .map(|i| i.trim().to_string())
        .filter(|i| !i.is_empty())
    else {
        ctx.warn(
            IssueCode::MissingAttribute,
            "<append> without `id` has no target; dropped",
            node,
        );
        return Ok(None);
    };
    let mut a = Append {
        id,
        ty: opt_attr(node, "type").map(|t| t.trim().to_string()),
        ..Append::default()
    };
    ctx.keep_attributes(&mut a.extra, "append", node, &["id", "type"]);
    let owner_ty = a.ty.clone().unwrap_or_default();
    let owner_name = a.id.clone();
    let owner = Owner {
        ty: &owner_ty,
        name: &owner_name,
    };
    let mut once = Singletons {
        seen: HashSet::new(),
    };
    let mut extra: Extra = std::mem::take(&mut a.extra);
    for child in node.elements() {
        let key: &'static str = match child.name.as_str() {
            "supports" => "supports",
            "setters" => "setters",
            "rules" => "rules",
            "spellcasting" => "spellcasting",
            _ => {
                ctx.keep_child(&mut extra, "append", child);
                continue;
            }
        };
        if !once.first(key) {
            parts::push_dup(ctx, &mut extra, "append", child);
            continue;
        }
        match key {
            "supports" => a.supports = split_tags(&child.text()).0,
            "setters" => a.setters = parts::setters(child)?,
            "rules" => a.rules = convert_rules(ctx, child, &owner)?,
            _ => a.spellcasting = Some(parts::spellcasting(ctx, child)?),
        }
    }
    a.extra = extra;
    Ok(Some(a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Rule;

    fn content(xml: &str) -> (ContentFile, FileConversion) {
        let conv = convert_xml(xml, "t.xml");
        let Some(Document::Content(file)) = conv.document.clone() else {
            panic!("no document: {:#?}", conv.issues)
        };
        (file, conv)
    }

    #[test]
    fn full_element_round() {
        let (file, conv) = content(
            r#"<?xml version="1.0"?>
<elements app="1" ignore="false" rootextra="x">
  <info><name>N</name><update version="1.0.0"><file name="a.xml" url="https://e.invalid/a.xml"/></update></info>
  <element name=" Fireball " type="Spell" source="PHB" id="ID_PHB_SPELL_FIREBALL" odd="1">
    <supports>Sorcerer, Wizard, Wizard</supports>
    <prerequisites>text</prerequisites>
    <requirements> [level:5] </requirements>
    <description><p>Boom &amp; bang</p></description>
    <sheet display="false"><description>t</description></sheet>
    <compendium display="false"/>
    <setters><set name="level">3</set></setters>
    <rules><grant type="T" id="ID_A"/></rules>
    <spellcasting name="W"><list>W</list></spellcasting>
    <equipment name="e"/><source>Book</source>
    <extract><item>ID_I</item></extract>
    <mystery a="b">c</mystery>
  </element>
</elements>"#,
        );
        assert_eq!(file.min_app_version.as_deref(), Some("1"));
        assert_eq!(file.ignore, Some(false));
        assert_eq!(file.extra.attributes["rootextra"], "x");
        let e = &file.elements[0];
        assert_eq!(e.name, "Fireball");
        assert_eq!(e.supports, ["Sorcerer", "Wizard"]);
        assert_eq!(e.prerequisite, "text");
        assert_eq!(e.requirements, "[level:5]");
        assert_eq!(e.description, "<p>Boom &amp; bang</p>");
        assert_eq!(e.extra.attributes["odd"], "1");
        assert_eq!(e.extra.children[0].tag, "mystery");
        assert!(e.sheet.is_some() && e.compendium.is_some() && e.spellcasting.is_some());
        assert!(e.equipment.is_some() && e.source_ref.is_some() && e.extract.is_some());
        assert_eq!(conv.stats.elements, 1);
        assert_eq!(conv.stats.rules_by_kind["grant"], 1);
        assert_eq!(conv.stats.normalized["prerequisites -> prerequisite"], 1);
        assert_eq!(conv.stats.unknown_children["element/mystery"], 1);
    }

    #[test]
    fn prerequisite_wins_and_duplicate_singletons_are_kept() {
        let (file, conv) = content(
            r#"<elements><element name="A" type="T" source="S" id="ID_A">
                 <prerequisites>old</prerequisites><prerequisite>new</prerequisite>
                 <requirements>ID_X</requirements><requirements>ID_Y</requirements>
                 <set name="x">1</set></element></elements>"#,
        );
        let e = &file.elements[0];
        assert_eq!(e.prerequisite, "new");
        assert_eq!(e.requirements, "ID_X");
        assert_eq!(e.extra.children.len(), 3);
        assert!(
            conv.issues
                .iter()
                .filter(|i| i.code == IssueCode::DuplicateChild)
                .count()
                == 2
        );
        assert!(conv
            .issues
            .iter()
            .any(|i| i.code == IssueCode::UnsupportedConstruct));
    }

    #[test]
    fn multiclass_and_append() {
        let (file, _) = content(
            r#"<elements>
                 <element name="Fighter" type="Class" source="S" id="ID_C">
                   <rules><grant id="ID_G"/></rules>
                   <multiclass id="ID_MC" x="1"><prerequisite>Str 13</prerequisite><requirements>[str:13]</requirements>
                     <description><p>d</p></description><setters><set name="s">v</set></setters><rules><grant id="ID_H"/></rules><weird/></multiclass>
                 </element>
                 <append id="ID_C" type="Class"><supports>X</supports><setters><set name="k">v</set></setters>
                   <rules><stat name="a" value="1"/></rules><spellcasting name="W" extend="true"/><description><p>ignored</p></description></append>
                 <append><supports>nope</supports></append>
               </elements>"#,
        );
        let m = file.elements[0].multiclass.as_ref().unwrap();
        assert_eq!(
            (
                m.id.as_str(),
                m.prerequisite.as_str(),
                m.requirements.as_str()
            ),
            ("ID_MC", "Str 13", "[str:13]")
        );
        assert_eq!(m.rules.len(), 1);
        assert_eq!(m.extra.attributes["x"], "1");
        assert_eq!(m.extra.children[0].tag, "weird");
        assert_eq!(file.appends.len(), 1);
        let a = &file.appends[0];
        assert_eq!(
            (a.supports.clone(), a.rules.len(), a.setters.len()),
            (vec!["X".to_string()], 1, 1)
        );
        assert!(a.spellcasting.is_some());
        assert_eq!(a.extra.children[0].tag, "description");
        assert!(matches!(a.rules[0], Rule::Stat(_)));
    }

    #[test]
    fn element_errors_drop_only_that_element() {
        let conv = convert_xml(
            r#"<elements>
                 <element name="Bad" type="T" source="S" id="ID_BAD"><rules><select name="x"/></rules></element>
                 <element name="Good" type="T" source="S" id="ID_GOOD"/>
                 <element name="NoId" type="T" source="S"/>
                 <element name="Lvl" type="T" source="S" id="ID_L"><rules><grant id="ID_Q" level="abc"/></rules></element>
               </elements>"#,
            "t.xml",
        );
        let Some(Document::Content(file)) = &conv.document else {
            panic!()
        };
        assert_eq!(file.elements.len(), 1);
        assert_eq!(conv.stats.dropped_elements, 3);
        let errors: Vec<_> = conv
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 3);
        assert!(errors
            .iter()
            .any(|i| i.element.as_deref() == Some("ID_BAD") && i.path.as_deref() == Some("rules")));
        assert!(errors
            .iter()
            .all(|i| i.file.as_deref() == Some("t.xml") && i.line.is_some()));
    }

    #[test]
    fn fatal_file_errors() {
        for xml in [
            "<elements><element",
            r#"<elements app="x"/>"#,
            r#"<elements ignore="maybe"/>"#,
            r#"<elements><info><update/></info></elements>"#,
        ] {
            let conv = convert_xml(xml, "t.xml");
            assert!(conv.document.is_none(), "{xml}");
            assert_eq!(conv.issues.len(), 1);
        }
    }

    #[test]
    fn index_dispatch() {
        let conv = convert_xml(
            r#"<index><info><update version="1.0"><file name="a.index" url="https://e.invalid/a.index"/></update></info>
               <files><file name="a.xml" url="https://e.invalid/a.xml"/></files></index>"#,
            "a.index",
        );
        assert!(matches!(conv.document, Some(Document::Index(_))));
    }

    #[test]
    fn empty_header_attributes_are_rejected_except_source() {
        let conv = convert_xml(
            r#"<elements><element name="" type="T" source="" id="ID_A"/></elements>"#,
            "t.xml",
        );
        assert_eq!(conv.stats.dropped_elements, 1);
        let conv = convert_xml(
            r#"<elements><element name="A" type="T" source="" id="ID_A"/></elements>"#,
            "t.xml",
        );
        assert_eq!(conv.stats.dropped_elements, 0);
    }
}
