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
            let displayed = el.sheet.as_ref().map_or(true, |s| s.is_displayed());
            if displayed {
                if let Some(t) = &summary {
                    lines.push(format!("{}: {t}", el.name));
                }
            }
            out.push(FeatureDetail {
                id: el.id.clone(),
                name: el.name.clone(),
                ty: el.ty.clone(),
                source: el.source.clone(),
                level: self.granted_level(id),
                granted_by: node.parent.map(|p| self.graph.nodes[p].e.el.name.clone()),
                description: sanitize_html(&el.description),
                sheet_summary: summary,
                displayed,
            });
        }
        (out, lines.join("\n"))
    }

    /// Level of the rule that granted or offered a node (1 when unrestricted).
    fn granted_level(&self, id: NodeId) -> u32 {
        let node = &self.graph.nodes[id];
        let Some(parent) = node.parent else { return 1 };
        let p = &self.graph.nodes[parent];
        let rule_index = match node.origin {
            Origin::Granted(ri) => Some(ri),
            Origin::Selected(si) => p.slots.get(si).map(|s| s.rule),
            _ => None,
        };
        match rule_index.and_then(|ri| p.e.el.rules.get(ri)) {
            Some(Rule::Grant(g)) => g.level.unwrap_or(1),
            Some(Rule::Select(s)) => s.level.unwrap_or(1),
            _ => 1,
        }
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
