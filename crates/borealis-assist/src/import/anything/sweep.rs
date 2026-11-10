//! The engine sweep of an import draft: a test character of the right edition takes each class (or the parent class
//! plus the subclass) and levels 1 to 20, deriving at every level. The sweep itself is
//! `borealis_authoring::check::sweep` (shared with the homebrew creator); this module turns the draft's plan into its
//! targets.

use std::collections::HashSet;

use borealis_authoring::check::sweep::SweepTarget;
use borealis_content::file::ContentFile;
use serde_json::Value;

pub use borealis_authoring::check::sweep::SweepIssue;

use crate::homebrew::PackContext;

use super::types::{DraftElement, ElementStatus, ImportPlan, Role};

fn issue(level: u32, element_id: Option<&str>, message: impl Into<String>) -> SweepIssue {
    SweepIssue {
        level,
        element_id: element_id.map(str::to_string),
        message: message.into(),
    }
}

fn targets(plan: &ImportPlan) -> Vec<SweepTarget> {
    let mut out = Vec::new();
    for g in &plan.groups {
        let Some(root_id) = g.root_id.as_deref() else {
            continue;
        };
        let Some(root) = plan.elements.iter().find(|e| e.id == root_id) else {
            continue;
        };
        if !matches!(root.role, Role::Class | Role::Subclass) {
            continue;
        }
        let features: Vec<(String, u32)> = plan
            .elements
            .iter()
            .filter(|e| e.owner.as_deref() == Some(root_id))
            .filter_map(|e| Some((e.id.clone(), e.level?)))
            .collect();
        let (class_id, subclass_id) = match (&g.parent, root.role) {
            (Some(p), Role::Subclass) => (p.class_id.clone(), Some(root.id.clone())),
            _ => (root.id.clone(), None),
        };
        // Slot casters only: a points caster gets no slots by design.
        let casts = root.role == Role::Class
            && g.outline
                .as_ref()
                .and_then(|o| o.caster())
                .is_some_and(|c| {
                    super::caster::caster_kind(c) != super::caster::CasterKind::Points
                });
        out.push(SweepTarget {
            class_id,
            subclass_id,
            features,
            root_id: root_id.to_string(),
            casts,
        });
    }
    out
}

/// Run the sweep for every class/subclass group of the plan. Standalone elements get a load check
/// only.
pub fn engine_sweep(
    ctx: &PackContext,
    plan: &ImportPlan,
    elements: &[DraftElement],
) -> Vec<SweepIssue> {
    let docs: Vec<Value> = elements
        .iter()
        .filter(|e| {
            e.status != ElementStatus::Pending
                && e.status != ElementStatus::Failed
                && !e.json.is_null()
        })
        .map(|e| e.json.clone())
        .collect();
    let library = ctx.base.clone().build();
    let doc = super::derived::content_doc(&super::derived::derive(plan, &docs, &library));
    let file = match ContentFile::from_json_str(&doc.to_string()) {
        Ok(f) => f,
        Err(e) => return vec![issue(0, None, format!("the draft does not load: {e}"))],
    };
    let ids: HashSet<String> = elements.iter().map(|e| e.id.clone()).collect();
    borealis_authoring::check::sweep::engine_sweep(
        &ctx.base,
        ctx.ruleset,
        "import-draft",
        file,
        &ids,
        &targets(plan),
    )
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
