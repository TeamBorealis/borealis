//! Feature details (descriptions, sheet text) and stat provenance for a finished evaluation.

use crate::derive::Evaluation;
use crate::diag::Diagnostic;
use crate::graph::{NodeId, Origin};
use crate::provenance::StatTrace;
use crate::snapshot::FeatureDetail;
use crate::text::sanitize_html;
use borealis_content::expr::{PlaceholderSyntax, TemplatePart};
use borealis_content::{Element, Rule};

/// Element types listed as character features.
const FEATURE_TYPES: &[&str] = &[
    "Class Feature",
    "Archetype Feature",
    "Racial Trait",
    "Background Feature",
    "Feat",
];

/// A feat's own part (the ability score or spell list it lets the player choose). Listed, as
/// Aurora's sheet lists it, only when the sheet shows it: a hidden one is a recorded choice
/// (GitHub #62).
const FEAT_FEATURE: &str = "Feat Feature";

impl Evaluation<'_> {
    /// The provenance trace of one statistic from this evaluation (needs `trace`).
    pub(crate) fn trace_of(&self, stat: &str, diags: &[Diagnostic]) -> StatTrace {
        StatTrace::build(stat, self.stats.trace_steps(stat).to_vec(), diags)
    }

    /// Feature details in registration order, and the joined sheet summary.
    pub(crate) fn features(&self) -> (Vec<FeatureDetail>, String) {
        let mut out = Vec::new();
        let mut lines = Vec::new();
        for id in self.graph.preorder() {
            let node = &self.graph.nodes[id];
            let el = &node.e.el;
            if !is_feature(el) {
                continue;
            }
            let summary = self.sheet_text(id);
            let displayed = shown_on_sheet(el) || (el.ty == "Feat" && !self.stood_in_for(id));
            let sheet_name = sheet_name(el);
            if displayed {
                if let Some(t) = &summary {
                    lines.push(format!(
                        "{}: {t}",
                        sheet_name.as_deref().unwrap_or(&el.name)
                    ));
                }
            }
            let level = self.granted_level(id);
            let detail = FeatureDetail {
                id: el.id.clone(),
                name: el.name.clone(),
                sheet_name,
                ty: el.ty.clone(),
                source: el.source.clone(),
                level,
                granted_by: node.parent.map(|p| self.graph.nodes[p].e.el.name.clone()),
                description: sanitize_html(&self.injected_description(&el.description)),
                sheet_summary: summary,
                displayed,
                prerequisites_unmet: self.graph.node_needs_prereq_override(id)
                    || self.graph.added_target_needs_override(id),
                added: self.graph.is_added_target(id),
            };
            let again: Vec<FeatureDetail> = self
                .feat_select_levels(id, level)
                .into_iter()
                .map(|l| FeatureDetail {
                    level: l,
                    ..detail.clone()
                })
                .collect();
            out.push(detail);
            out.extend(again);
        }
        (out, lines.join("\n"))
    }

    /// GitHub #62: a part of a feat the sheet shows (its chosen option, printed under the
    /// feat's name) stands in for a feat that hides itself. Without one the feat is listed
    /// itself, so every feat the character has is on the sheet.
    fn stood_in_for(&self, id: NodeId) -> bool {
        let mut stack = self.graph.nodes[id].children.clone();
        while let Some(n) = stack.pop() {
            let node = &self.graph.nodes[n];
            if !node.alive {
                continue;
            }
            if is_feature(&node.e.el) && shown_on_sheet(&node.e.el) {
                return true;
            }
            stack.extend(node.children.iter().copied());
        }
        false
    }

    /// Later levels at which a feature opens another feat select (a 2024 class's one Ability
    /// Score Improvement feature with a `select_feat` per level): the feature is listed again
    /// at each, as Aurora lists one granted Ability Score Improvement feature per level.
    fn feat_select_levels(&self, id: NodeId, base: u32) -> Vec<u32> {
        let node = &self.graph.nodes[id];
        let mut levels: Vec<u32> = node
            .slots
            .iter()
            .filter_map(|sl| match node.e.el.rules.get(sl.rule) {
                Some(Rule::Select(s)) if s.ty == "Feat" => s.level,
                _ => None,
            })
            .filter(|l| *l > base && *l <= self.graph.mgr_level(node.mgr))
            .collect();
        levels.sort_unstable();
        levels.dedup();
        levels
    }

    /// Level of the rule that granted or offered a node; a rule without a level inherits the
    /// level its owner was gained at (a Fighting Style feat picked by the Paladin's level-2
    /// Fighting Style feature is gained at 2); a root, and an unlevelled grant of a class,
    /// count as level 1.
    fn granted_level(&self, id: NodeId) -> u32 {
        let node = &self.graph.nodes[id];
        let Some(parent) = node.parent else { return 1 };
        let p = &self.graph.nodes[parent];
        let rule_index = match node.origin {
            Origin::Granted(ri) => Some(ri),
            Origin::Selected(si) => p.slots.get(si).map(|s| s.rule),
            _ => None,
        };
        let own = match rule_index.and_then(|ri| p.e.el.rules.get(ri)) {
            Some(Rule::Grant(g)) => g.level,
            Some(Rule::Select(s)) => s.level,
            _ => None,
        };
        // a class grants its unlevelled features at class level 1, whatever character level
        // the class (a multiclass) was taken at
        own.unwrap_or_else(|| {
            if matches!(p.e.el.ty.as_str(), "Class" | "Multiclass") {
                1
            } else {
                self.granted_level(parent)
            }
        })
    }

    /// The sheet entries that apply at the node's current level, placeholders resolved.
    fn sheet_text(&self, id: NodeId) -> Option<String> {
        let node = &self.graph.nodes[id];
        let sheet = node.e.el.sheet.as_ref()?;
        let level = self.graph.mgr_level(node.mgr);
        let best = sheet
            .descriptions
            .iter()
            .map(|d| d.effective_level())
            .filter(|l| *l <= level)
            .max()?;
        let text: Vec<String> = sheet
            .descriptions
            .iter()
            .filter(|d| d.effective_level() == best)
            .map(|d| self.resolve_template(d.template().parts()))
            .collect();
        let joined = text.join(" ");
        (!joined.trim().is_empty()).then_some(joined)
    }

    /// A description with its `<div element="ID"/>` references replaced by the referenced
    /// elements' text, as Aurora's description panel shows it (GitHub #53).
    fn injected_description(&self, html: &str) -> String {
        let source = |id: &str| {
            self.graph
                .uni
                .get(id)
                .map(|u| (u.el.name.clone(), u.el.description.clone()))
        };
        crate::inject::inject_references(html, &source)
    }

    fn resolve_template(&self, parts: &[TemplatePart]) -> String {
        let mut out = String::new();
        for part in parts {
            match part {
                TemplatePart::Text(t) => out.push_str(t),
                TemplatePart::Placeholder(ph) => {
                    // Aurora's ReplaceDoubleBrackets: a statistic group first, then an inline
                    // value (`<stat inline="true" value="Fire"/>`, GitHub #52).
                    let key = ph.stat_key();
                    if self.stats.exists(&key) {
                        out.push_str(&self.stats.sum(&key).to_string());
                    } else if let Some(text) = crate::breath::inline_text(&self.graph, &key) {
                        out.push_str(&text);
                    } else {
                        match ph.syntax {
                            PlaceholderSyntax::Braces => {
                                out.push_str(&format!("{{{{{}}}}}", ph.key))
                            }
                            PlaceholderSyntax::Dollar => out.push_str(&format!("$({})", ph.key)),
                        }
                    }
                }
            }
        }
        out
    }
}

/// The content shows the element in the sheet's feature list (`<sheet display>`, default true).
fn shown_on_sheet(el: &Element) -> bool {
    el.sheet.as_ref().is_none_or(|s| s.is_displayed())
}

/// The element is listed as a character feature.
fn is_feature(el: &Element) -> bool {
    FEATURE_TYPES.contains(&el.ty.as_str()) || (el.ty == FEAT_FEATURE && shown_on_sheet(el))
}

/// The sheet's alternate name (`<sheet alt>`), which Aurora's sheet prints instead of the name.
fn sheet_name(el: &Element) -> Option<String> {
    el.sheet
        .as_ref()
        .and_then(|s| s.alt.as_deref())
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
}
