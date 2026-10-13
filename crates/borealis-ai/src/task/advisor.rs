//! Build advisor and level-up advisor (task c).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::prompt::{base_system, fence, Ruleset};
use super::qa::{ContextSnippet, MAX_SNIPPETS, MAX_SNIPPET_CHARS};
use super::AiTask;

/// What kind of advice is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AdvisorMode {
    /// General build review/optimisation.
    Build,
    /// Choices for gaining a level.
    LevelUp {
        /// Current character level.
        from_level: u8,
        /// Level being gained.
        to_level: u8,
    },
}

/// Inputs for [`Advisor`] (also the wire format).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvisorInput {
    /// Build review or level-up.
    #[serde(flatten)]
    pub mode: AdvisorMode,
    /// Active ruleset.
    pub ruleset: Ruleset,
    /// Character summary JSON (the app decides what to include).
    pub character: Value,
    /// What the player wants ("tanky frontliner", "stealth + control"...).
    #[serde(default)]
    pub goal: Option<String>,
    /// Legal options available to choose from; when given, suggestions must reference these ids.
    #[serde(default)]
    pub options: Vec<ContextSnippet>,
}

/// Suggestion topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionCategory {
    /// Ability scores / point buy.
    AbilityScores,
    /// Feat choice.
    Feat,
    /// Spell choices.
    Spells,
    /// Gear.
    Equipment,
    /// Multiclassing.
    Multiclass,
    /// Subclass / archetype.
    Subclass,
    /// Combat or exploration tactics.
    Tactics,
    /// Roleplay hooks.
    Roleplay,
    /// Anything else.
    Other,
}

/// One recommendation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Short headline.
    pub title: String,
    /// Why, with the mechanics.
    pub rationale: String,
    /// 1 (nice to have) to 5 (strongly recommended).
    pub priority: u8,
    /// Topic.
    pub category: SuggestionCategory,
    /// Content element ids this refers to.
    #[serde(default)]
    pub element_ids: Vec<String>,
}

/// Typed advice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Advice {
    /// Two-sentence overview.
    pub summary: String,
    /// Ranked suggestions.
    pub suggestions: Vec<Suggestion>,
    /// Problems spotted in the current build.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Task (c).
#[derive(Debug, Clone)]
pub struct Advisor {
    /// Inputs.
    pub input: AdvisorInput,
}

impl AiTask for Advisor {
    type Output = Advice;
    const NAME: &'static str = "advisor";

    fn system_prompt(&self) -> String {
        let focus = match self.input.mode {
            AdvisorMode::Build => "review the build and suggest improvements".to_string(),
            AdvisorMode::LevelUp {
                from_level,
                to_level,
            } => {
                format!(
                    "advise on the choices for going from level {from_level} to level {to_level}"
                )
            }
        };
        format!(
            "{}\n\nRuleset: {}\nOnly suggest options the character is legal to take. When <context> options are supplied, \
every `element_ids` entry must come from them. Order suggestions by priority (5 first) and keep rationales to a few sentences. \
Respect the player's stated goal and play style; this is advice, not a mandate.",
            base_system(&format!("{focus} for a D&D character")),
            self.input.ruleset.describe()
        )
    }

    fn user_prompt(&self) -> String {
        let i = &self.input;
        let mut p = String::new();
        p.push_str(&fence("character", &i.character.to_string()));
        if !i.options.is_empty() {
            let body: String = i
                .options
                .iter()
                .take(MAX_SNIPPETS)
                .map(|c| {
                    let text: String = c.text.chars().take(MAX_SNIPPET_CHARS).collect();
                    format!("[{}] {}\n{}", c.id, c.title, text)
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            p.push_str("\n\nAvailable options:\n");
            p.push_str(&fence("context", &body));
        }
        if let Some(g) = &i.goal {
            p.push_str("\n\nPlayer goal:\n");
            p.push_str(&fence("user_input", g));
        }
        p
    }

    fn output_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["summary", "suggestions"],
            "properties": {
                "summary": {"type": "string"},
                "suggestions": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "required": ["title", "rationale", "priority", "category"],
                        "properties": {
                            "title": {"type": "string"},
                            "rationale": {"type": "string"},
                            "priority": {"type": "integer", "minimum": 1, "maximum": 5},
                            "category": {"enum": ["ability_scores", "feat", "spells", "equipment", "multiclass", "subclass", "tactics", "roleplay", "other"]},
                            "element_ids": {"type": "array", "items": {"type": "string"}}
                        }
                    }
                },
                "warnings": {"type": "array", "items": {"type": "string"}}
            }
        })
    }

    fn check(&self, out: &Advice) -> Vec<String> {
        if self.input.options.is_empty() {
            return Vec::new();
        }
        let known: HashSet<&str> = self.input.options.iter().map(|c| c.id.as_str()).collect();
        out.suggestions
            .iter()
            .flat_map(|s| s.element_ids.iter())
            .filter(|id| !known.contains(id.as_str()))
            .map(|id| format!("element id {id:?} is not among the available options"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::ScriptedRunner;
    use super::super::{run_task, RunOptions};
    use super::*;

    fn input(mode: AdvisorMode) -> AdvisorInput {
        AdvisorInput {
            mode,
            ruleset: Ruleset::Dnd2024,
            character: json!({"class":"Fighter","level":3}),
            goal: Some("tank".into()),
            options: vec![ContextSnippet {
                id: "ID_FEAT_TOUGH".into(),
                title: "Tough".into(),
                text: "hp".into(),
            }],
        }
    }

    #[test]
    fn wire_format_flattens_the_mode() {
        let v = serde_json::to_value(input(AdvisorMode::LevelUp {
            from_level: 3,
            to_level: 4,
        }))
        .unwrap();
        assert_eq!(v["mode"], "level_up");
        assert_eq!(v["to_level"], 4);
        let back: AdvisorInput = serde_json::from_value(v).unwrap();
        assert_eq!(
            back.mode,
            AdvisorMode::LevelUp {
                from_level: 3,
                to_level: 4
            }
        );
        let b: AdvisorInput =
            serde_json::from_value(json!({"mode":"build","ruleset":"dnd2014","character":{}}))
                .unwrap();
        assert_eq!(b.mode, AdvisorMode::Build);
    }

    #[test]
    fn system_prompt_reflects_level_up() {
        let s = Advisor {
            input: input(AdvisorMode::LevelUp {
                from_level: 3,
                to_level: 4,
            }),
        }
        .system_prompt();
        assert!(s.contains("level 3 to level 4"));
    }

    #[tokio::test]
    async fn rejects_ids_outside_options_then_accepts_repair() {
        let mk = |id: &str, p: u8| {
            format!(
                r#"{{"summary":"s","suggestions":[{{"title":"t","rationale":"r","priority":{p},"category":"feat","element_ids":["{id}"]}}]}}"#
            )
        };
        let runner = ScriptedRunner::texts(&[&mk("ID_FEAT_MADE_UP", 4), &mk("ID_FEAT_TOUGH", 4)]);
        let r = run_task(
            &runner,
            &Advisor {
                input: input(AdvisorMode::Build),
            },
            &RunOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(r.attempts, 2);
        assert_eq!(r.output.suggestions[0].category, SuggestionCategory::Feat);
    }

    #[tokio::test]
    async fn priority_out_of_range_fails_schema() {
        let bad = r#"{"summary":"s","suggestions":[{"title":"t","rationale":"r","priority":9,"category":"feat"}]}"#;
        let good = r#"{"summary":"s","suggestions":[{"title":"t","rationale":"r","priority":5,"category":"feat"}]}"#;
        let runner = ScriptedRunner::texts(&[bad, good]);
        let mut i = input(AdvisorMode::Build);
        i.options.clear();
        let r = run_task(&runner, &Advisor { input: i }, &RunOptions::default())
            .await
            .unwrap();
        assert_eq!(r.attempts, 2);
    }
}
