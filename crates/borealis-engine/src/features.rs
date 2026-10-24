//! Feature details (descriptions, sheet text) and stat provenance for a finished evaluation.

use crate::derive::Evaluation;
use crate::diag::Diagnostic;
use crate::graph::{NodeId, Origin};
use crate::provenance::StatTrace;
use crate::snapshot::FeatureDetail;
use crate::text::sanitize_html;
use borealis_content::expr::{PlaceholderSyntax, TemplatePart};
use borealis_content::Rule;

/// Element types listed as character features.
const FEATURE_TYPES: &[&str] = &[
    "Class Feature",
    "Archetype Feature",
    "Racial Trait",
    "Background Feature",
    "Feat",
];

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
            if !FEATURE_TYPES.contains(&el.ty.as_str()) {
                continue;
            }
            let summary = self.sheet_text(id);
            let displayed = el.sheet.as_ref().is_none_or(|s| s.is_displayed());
            if displayed {
                if let Some(t) = &summary {
                    lines.push(format!("{}: {t}", el.name));
                }
            }
            let level = self.granted_level(id);
            let detail = FeatureDetail {
                id: el.id.clone(),
                name: el.name.clone(),
                ty: el.ty.clone(),
                source: el.source.clone(),
                level,
                granted_by: node.parent.map(|p| self.graph.nodes[p].e.el.name.clone()),
                description: sanitize_html(&el.description),
                sheet_summary: summary,
                displayed,
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

    fn resolve_template(&self, parts: &[TemplatePart]) -> String {
        let mut out = String::new();
        for part in parts {
            match part {
                TemplatePart::Text(t) => out.push_str(t),
                TemplatePart::Placeholder(ph) => {
                    let key = ph.stat_key();
                    if self.stats.exists(&key) {
                        out.push_str(&self.stats.sum(&key).to_string());
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
