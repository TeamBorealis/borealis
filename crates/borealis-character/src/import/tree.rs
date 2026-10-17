//! The `build/elements` selection tree: levels, class progression, slot mapping.

use super::equipment::ItemMap;
use super::{Ctx, MissingContent, UnmappedSlot};
use crate::crc::slot_checksum;
use crate::model::UnresolvedSelection;
use borealis_engine::state::{Choice, ClassSlot, HpRolls, Selection, SlotKey};
use borealis_migrate::dom::XmlElement;
use std::collections::HashMap;

const ITEM_TYPES: [&str; 4] = ["Weapon", "Armor", "Item", "Magic Item"];

pub(super) fn read_elements(elements: &XmlElement, items: &ItemMap, ctx: &mut Ctx<'_>) {
    let nodes: Vec<&XmlElement> = elements
        .elements()
        .filter(|e| e.name == "element")
        .collect();
    let mut starts: HashMap<String, u32> = HashMap::new();
    let mut levels: Vec<ClassSlot> = Vec::new();
    for node in nodes.iter().filter(|n| n.attr("type") == Some("Option")) {
        let id = node.attr("id").unwrap_or("");
        if ctx.uni.get(id).is_some() {
            ctx.state.options.push(id.to_string());
        } else {
            ctx.report.missing_content.push(MissingContent {
                id: id.to_string(),
                ty: "Option".into(),
                name: node.attr("name").unwrap_or("").to_string(),
                context: "option".into(),
            });
        }
    }
    for node in nodes.iter().filter(|n| n.attr("type") == Some("Level")) {
        let n = u32::try_from(levels.len()).unwrap_or(0) + 1;
        let class_id = node.attr("class").unwrap_or("").to_string();
        let slot = if node
            .attr("multiclass")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
        {
            if node
                .attr("starting")
                .is_some_and(|v| v.eq_ignore_ascii_case("true"))
            {
                starts.insert(class_id, n);
                ClassSlot::Multi { starting_level: n }
            } else if let Some(s) = starts.get(&class_id) {
                ClassSlot::Multi { starting_level: *s }
            } else {
                ctx.report.warnings.push(format!("level {n} belongs to a multiclass that was never started; treated as a main class level"));
                ClassSlot::Main
            }
        } else {
            ClassSlot::Main
        };
        if let Some(rolls) = node
            .attr("rndhp")
            .map(|r| {
                r.split(',')
                    .filter_map(|v| v.trim().parse::<u32>().ok())
                    .collect::<Vec<_>>()
            })
            .filter(|r| !r.is_empty())
        {
            ctx.state.hp_rolls.push(HpRolls {
                class: slot.clone(),
                rolls,
            });
        }
        levels.push(slot);
    }
    if levels.is_empty() {
        ctx.report
            .warnings
            .push("the file contains no Level nodes; starting at level 1".into());
        levels.push(ClassSlot::Main);
    }
    ctx.state.levels = levels;
    for node in &nodes {
        let ty = node.attr("type").unwrap_or("");
        let id = node.attr("id").unwrap_or("");
        match ty {
            "Option" | "Level" => read_children(node, id, None, ctx),
            t if ITEM_TYPES.contains(&t) => {
                let uid = items.next_uid(id);
                read_children(node, id, uid, ctx);
            }
            _ => {}
        }
    }
}

fn read_children(node: &XmlElement, owner: &str, instance: Option<String>, ctx: &mut Ctx<'_>) {
    for child in node.elements().filter(|e| e.name == "element") {
        match child.attr("registered") {
            Some(reg) => {
                if reg.is_empty() {
                    continue;
                }
                read_slot(child, owner, reg, instance.clone(), ctx);
                if child.attr("isList") != Some("true") {
                    read_children(child, reg, instance.clone(), ctx);
                }
            }
            None => {
                let id = child.attr("id").unwrap_or("");
                if id.is_empty() {
                    continue;
                }
                read_children(child, id, instance.clone(), ctx);
            }
        }
    }
}

fn read_slot(
    node: &XmlElement,
    owner: &str,
    registered: &str,
    instance: Option<String>,
    ctx: &mut Ctx<'_>,
) {
    let slot = SlotNode {
        owner: owner.to_string(),
        registered: registered.to_string(),
        ty: node.attr("type").unwrap_or("").to_string(),
        name: node.attr("name").unwrap_or("").to_string(),
        ordinal: node
            .attr("number")
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(1)
            .max(1),
        checksum: node.attr("checksum").map(str::to_lowercase),
        required_level: node
            .attr("requiredLevel")
            .and_then(|v| v.trim().parse().ok()),
        is_list: node.attr("isList") == Some("true"),
        replace_level: node
            .attr("replaceLevel")
            .and_then(|v| v.trim().parse().ok()),
        instance,
    };
    match find_rule(ctx, &slot) {
        Some(rule) => place_slot(ctx, slot, rule),
        None => unmap_slot(ctx, slot),
    }
}

/// One stored selection slot, from a file node or from a previously unresolved record.
pub(super) struct SlotNode {
    pub owner: String,
    pub registered: String,
    pub ty: String,
    pub name: String,
    pub ordinal: u32,
    pub checksum: Option<String>,
    pub required_level: Option<u32>,
    pub is_list: bool,
    pub replace_level: Option<u32>,
    pub instance: Option<String>,
}

/// Record a slot whose rule could not be found.
pub(super) fn unmap_slot(ctx: &mut Ctx<'_>, slot: SlotNode) {
    let reason = if ctx.uni.get(&slot.owner).is_some() {
        "no matching select rule on the owner"
    } else {
        "the owner element is not installed"
    };
    if ctx.uni.get(&slot.owner).is_none()
        && !ctx
            .report
            .missing_content
            .iter()
            .any(|m| m.id == slot.owner)
    {
        ctx.report.missing_content.push(MissingContent {
            id: slot.owner.clone(),
            ty: String::new(),
            name: String::new(),
            context: "selection owner".into(),
        });
    }
    ctx.report.unmapped_slots.push(UnmappedSlot {
        owner: slot.owner.clone(),
        ty: slot.ty.clone(),
        name: slot.name.clone(),
        chosen: slot.registered.clone(),
        reason: reason.into(),
    });
    ctx.unresolved.push(UnresolvedSelection {
        owner: slot.owner,
        ty: slot.ty,
        name: slot.name,
        number: slot.ordinal,
        chosen: slot.registered,
        checksum: slot.checksum,
        required_level: slot.required_level,
        is_list: slot.is_list,
        replace_level: slot.replace_level,
        instance: slot.instance,
    });
}

/// Store a selection for a slot whose declaring rule index is known.
pub(super) fn place_slot(ctx: &mut Ctx<'_>, slot: SlotNode, rule: usize) {
    let choice = if slot.is_list {
        match slot.registered.trim().parse::<i64>() {
            Ok(i) => Choice::ListItem(i),
            Err(_) => return,
        }
    } else {
        if ctx.uni.get(&slot.registered).is_none()
            && !ctx
                .report
                .missing_content
                .iter()
                .any(|m| m.id == slot.registered)
        {
            ctx.report.missing_content.push(MissingContent {
                id: slot.registered.clone(),
                ty: slot.ty.clone(),
                name: slot.name.clone(),
                context: "selection".into(),
            });
        }
        Choice::Element(slot.registered.clone())
    };
    let key = SlotKey {
        owner: slot.owner,
        rule: u32::try_from(rule).unwrap_or(u32::MAX),
        ordinal: slot.ordinal,
        instance: slot.instance,
    };
    ctx.state.selections.retain(|s| s.slot != key);
    ctx.state.selections.push(Selection {
        slot: key,
        choice,
        replace_level: slot.replace_level,
    });
    ctx.report.selections_imported += 1;
}

/// Map a legacy slot to the index of the declaring select rule.
pub(super) fn find_rule(ctx: &Ctx<'_>, w: &SlotNode) -> Option<usize> {
    let owner = w.owner.as_str();
    let el = ctx.uni.get(owner)?;
    let selects: Vec<(usize, &borealis_content::SelectRule)> = el
        .el
        .rules
        .iter()
        .enumerate()
        .filter_map(|(i, r)| match r {
            borealis_content::Rule::Select(s) => Some((i, s)),
            _ => None,
        })
        .collect();
    let by =
        |pred: &dyn Fn(&borealis_content::SelectRule) -> bool| -> Vec<(usize, &borealis_content::SelectRule)> {
            selects.iter().filter(|(_, s)| pred(s)).copied().collect()
        };
    let mut cands = by(&|s| s.ty == w.ty && s.name == w.name);
    if cands.is_empty() {
        cands = by(&|s| s.ty == w.ty && s.name.eq_ignore_ascii_case(&w.name));
    }
    if cands.is_empty() {
        let same_type = by(&|s| s.ty == w.ty);
        if same_type.len() == 1 {
            cands = same_type;
        }
    }
    if cands.len() > 1 {
        if let Some(lvl) = w.required_level {
            let at_level: Vec<_> = cands
                .iter()
                .copied()
                .filter(|(_, s)| s.level.unwrap_or(1) == lvl)
                .collect();
            if !at_level.is_empty() {
                cands = at_level;
            }
        }
    }
    if cands.len() > 1 {
        cands.retain(|(_, s)| s.number.unwrap_or(1) >= w.ordinal);
    }
    match cands.as_slice() {
        [] => None,
        [one] => Some(one.0),
        many => {
            let owners = [owner.to_string(), original_class_id(ctx, owner)];
            let matches = |s: &borealis_content::SelectRule| {
                w.checksum.as_deref().is_some_and(|wanted| {
                    owners.iter().any(|o| {
                        !o.is_empty()
                            && slot_checksum(
                                o,
                                &s.name,
                                &s.ty,
                                s.level.unwrap_or(1),
                                w.ordinal,
                                s.supports.as_deref(),
                            ) == wanted
                    })
                })
            };
            let hits: Vec<usize> = many
                .iter()
                .filter(|(_, s)| matches(s))
                .map(|(i, _)| *i)
                .collect();
            let pool: Vec<usize> = if hits.is_empty() {
                many.iter().map(|(i, _)| *i).collect()
            } else {
                hits
            };
            // selects nothing tells apart (same name, type, level, supports) share a checksum:
            // fill them in order instead of letting a later choice overwrite an earlier one
            pool.iter()
                .copied()
                .find(|&i| !slot_taken(ctx, w, i))
                .or_else(|| pool.first().copied())
        }
    }
}

/// True when the import already placed a choice in this slot of rule `rule`.
fn slot_taken(ctx: &Ctx<'_>, w: &SlotNode, rule: usize) -> bool {
    let rule = u32::try_from(rule).unwrap_or(u32::MAX);
    ctx.state.selections.iter().any(|s| {
        s.slot.owner == w.owner
            && s.slot.rule == rule
            && s.slot.ordinal == w.ordinal
            && s.slot.instance == w.instance
    })
}

/// For a generated multiclass element, the id of the class it was derived from.
fn original_class_id(ctx: &Ctx<'_>, owner: &str) -> String {
    ctx.uni
        .by_type("Class")
        .find(|c| c.el.multiclass.as_ref().is_some_and(|m| m.id == owner))
        .map(|c| c.el.id.clone())
        .unwrap_or_default()
}
