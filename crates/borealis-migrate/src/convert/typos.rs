//! Lenient repair of well-known misspellings of tag and attribute names.
//!
//! A misspelled name has no effect in a legacy file (the data is simply lost), so repairing it is a
//! deliberate deviation in favour of the author's intent. Every repair is reported as a
//! `Normalized` warning with its line. Only an explicit table is applied (no fuzzy
//! matching), and an attribute is only renamed when the correct one is absent.

use super::util::Ctx;
use crate::dom::{XmlElement, XmlNode};

/// `(parent tag, misspelled child tag, correct tag)`.
const TAG_ALIASES: &[(&str, &str, &str)] = &[
    ("element", "descripton", "description"),
    ("element", "desription", "description"),
    ("element", "descripion", "description"),
    ("element", "discription", "description"),
    ("element", "descrption", "description"),
    ("element", "shet", "sheet"),
    ("element", "sheat", "sheet"),
    ("element", "suports", "supports"),
    ("element", "supprots", "supports"),
    ("element", "support", "supports"),
    ("element", "spellcastin", "spellcasting"),
    ("element", "spelcasting", "spellcasting"),
    ("element", "requirment", "requirements"),
    ("element", "requirments", "requirements"),
    ("element", "prerquisite", "prerequisite"),
    ("multiclass", "prerquisite", "prerequisite"),
    ("multiclass", "prerequisites", "prerequisite"),
];

/// `(tag, misspelled attribute, correct attribute)`.
const ATTR_ALIASES: &[(&str, &str, &str)] = &[
    ("sheet", "useage", "usage"),
    ("sheet", "uage", "usage"),
    ("sheet", "Usage", "usage"),
    ("sheet", "diplay", "display"),
    ("description", "useage", "usage"),
    ("description", "Usage", "usage"),
    ("spellcasting", "nme", "name"),
    ("select", "suports", "supports"),
    ("select", "suppports", "supports"),
    ("select", "support", "supports"),
    ("select", "spellcastin", "spellcasting"),
    ("select", "requirments", "requirements"),
    ("grant", "requirments", "requirements"),
];

/// Repair misspellings in place and return the number of repairs.
pub(crate) fn repair(ctx: &mut Ctx, el: &mut XmlElement) -> usize {
    let id = matches!(el.name.as_str(), "element" | "append")
        .then(|| el.attr("id").map(|i| i.trim().to_string()))
        .flatten();
    let outer = id.map(|i| ctx.current.replace(i));
    let mut repaired = 0;
    for (tag, bad, good) in ATTR_ALIASES {
        if el.name != *tag {
            continue;
        }
        let has_good = el.attrs.iter().any(|(k, _)| k == good);
        if has_good {
            continue;
        }
        let line = el.line;
        if let Some(slot) = el.attrs.iter_mut().find(|(k, _)| k == bad) {
            slot.0 = (*good).to_string();
            note(ctx, &format!("{tag}@{bad} -> {good}"), line);
            repaired += 1;
        }
    }
    let parent = el.name.clone();
    for child in &mut el.children {
        let XmlNode::Element(child) = child else {
            continue;
        };
        if let Some((_, _, good)) = TAG_ALIASES
            .iter()
            .find(|(p, bad, _)| *p == parent && *bad == child.name)
        {
            note(ctx, &format!("<{}> -> <{good}>", child.name), child.line);
            child.name = (*good).to_string();
            repaired += 1;
        }
        repaired += repair(ctx, child);
    }
    if let Some(outer) = outer {
        ctx.current = outer;
    }
    repaired
}

fn note(ctx: &mut Ctx, what: &str, line: u32) {
    ctx.warn_line(
        borealis_content::IssueCode::Normalized,
        format!("misspelled name repaired: {what}"),
        line,
    );
    ctx.note_normalized(&format!("typo {what}"));
}
