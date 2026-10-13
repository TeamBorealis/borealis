//! Backstory, personality and NPC generator (task d).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::prompt::{base_system, fence, Ruleset};
use super::AiTask;

/// What to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeKind {
    /// Character backstory.
    Backstory,
    /// Personality traits, ideals, bonds and flaws.
    Personality,
    /// A non-player character.
    Npc,
}

/// Desired length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Length {
    /// About 100 words.
    Short,
    /// About 250 words.
    #[default]
    Medium,
    /// About 500 words.
    Long,
}

impl Length {
    fn words(self) -> u32 {
        match self {
            Length::Short => 100,
            Length::Medium => 250,
            Length::Long => 500,
        }
    }
}

/// Inputs for [`Narrative`] (also the wire format).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NarrativeInput {
    /// What to produce.
    pub kind: NarrativeKind,
    /// Active ruleset (affects terminology: race vs species).
    pub ruleset: Ruleset,
    /// Character or setting summary.
    #[serde(default)]
    pub character: Option<Value>,
    /// Tone ("grim", "comic"...).
    #[serde(default)]
    pub tone: Option<String>,
    /// Free-form requirements from the user.
    #[serde(default)]
    pub constraints: Option<String>,
    /// Target length.
    #[serde(default)]
    pub length: Length,
}

/// NPC stat-free profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcProfile {
    /// Name.
    pub name: String,
    /// Race or species.
    #[serde(default)]
    pub race: String,
    /// Occupation/role.
    #[serde(default)]
    pub role: String,
    /// Looks.
    #[serde(default)]
    pub appearance: String,
    /// What they want.
    #[serde(default)]
    pub motivation: String,
    /// A secret.
    #[serde(default)]
    pub secret: String,
    /// Mannerisms.
    #[serde(default)]
    pub quirks: Vec<String>,
}

/// Typed result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NarrativeResult {
    /// Title.
    pub title: String,
    /// Main text in Markdown.
    pub text: String,
    /// Personality traits.
    #[serde(default)]
    pub traits: Vec<String>,
    /// Ideals.
    #[serde(default)]
    pub ideals: Vec<String>,
    /// Bonds.
    #[serde(default)]
    pub bonds: Vec<String>,
    /// Flaws.
    #[serde(default)]
    pub flaws: Vec<String>,
    /// Adventure hooks for the DM.
    #[serde(default)]
    pub hooks: Vec<String>,
    /// NPC profile (NPC kind only).
    #[serde(default)]
    pub npc: Option<NpcProfile>,
}

/// Task (d).
#[derive(Debug, Clone)]
pub struct Narrative {
    /// Inputs.
    pub input: NarrativeInput,
}

impl AiTask for Narrative {
    type Output = NarrativeResult;
    const NAME: &'static str = "narrative";

    fn system_prompt(&self) -> String {
        let (goal, extra) = match self.input.kind {
            NarrativeKind::Backstory => ("write a character backstory", "Fill `text` with the backstory; add 2-3 `hooks` a DM could use."),
            NarrativeKind::Personality => ("write personality details", "Provide 2 `traits`, 1-2 `ideals`, 1-2 `bonds` and 1-2 `flaws`; `text` is a short summary."),
            NarrativeKind::Npc => ("create an NPC", "Fill the `npc` object completely; `text` is a table-ready description; add 2-3 `hooks`."),
        };
        format!(
            "{}\n\nRuleset: {}\n{extra} Aim for about {} words in `text`. \
Use only the supplied character facts as canon and invent the rest; keep content suitable for a mixed-age gaming table.",
            base_system(goal),
            self.input.ruleset.describe(),
            self.input.length.words()
        )
    }

    fn user_prompt(&self) -> String {
        let i = &self.input;
        let mut p = String::from("Write it for this context.");
        if let Some(c) = &i.character {
            p.push_str("\n\n");
            p.push_str(&fence("character", &c.to_string()));
        }
        if let Some(t) = &i.tone {
            p.push_str(&format!("\nTone: {t:?}"));
        }
        if let Some(c) = &i.constraints {
            p.push_str("\n\nRequirements from the user:\n");
            p.push_str(&fence("user_input", c));
        }
        p
    }

    fn output_schema(&self) -> Value {
        let strings = json!({"type": "array", "items": {"type": "string"}});
        json!({
            "type": "object",
            "required": ["title", "text"],
            "properties": {
                "title": {"type": "string", "minLength": 1},
                "text": {"type": "string", "minLength": 1},
                "traits": strings, "ideals": strings, "bonds": strings, "flaws": strings, "hooks": strings,
                "npc": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {"type": "string"}, "race": {"type": "string"}, "role": {"type": "string"},
                        "appearance": {"type": "string"}, "motivation": {"type": "string"}, "secret": {"type": "string"},
                        "quirks": strings
                    }
                }
            }
        })
    }

    fn check(&self, out: &NarrativeResult) -> Vec<String> {
        match self.input.kind {
            NarrativeKind::Backstory => Vec::new(),
            NarrativeKind::Personality => {
                let mut e = Vec::new();
                for (name, empty) in [
                    ("traits", out.traits.is_empty()),
                    ("ideals", out.ideals.is_empty()),
                    ("bonds", out.bonds.is_empty()),
                    ("flaws", out.flaws.is_empty()),
                ] {
                    if empty {
                        e.push(format!(
                            "`{name}` must not be empty for a personality result"
                        ));
                    }
                }
                e
            }
            NarrativeKind::Npc => {
                if out.npc.is_none() {
                    vec!["`npc` is required for an NPC result".to_string()]
                } else {
                    Vec::new()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::ScriptedRunner;
    use super::super::{run_task, RunOptions};
    use super::*;

    fn input(kind: NarrativeKind) -> NarrativeInput {
        NarrativeInput {
            kind,
            ruleset: Ruleset::Dnd2014,
            character: Some(json!({"name":"Mira"})),
            tone: Some("grim".into()),
            constraints: Some("no dragons".into()),
            length: Length::Short,
        }
    }

    #[tokio::test]
    async fn backstory_needs_only_title_and_text() {
        let runner = ScriptedRunner::texts(&[r##"{"title":"T","text":"Once..."}"##]);
        let r = run_task(
            &runner,
            &Narrative {
                input: input(NarrativeKind::Backstory),
            },
            &RunOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(r.attempts, 1);
        assert!(r.output.hooks.is_empty());
    }

    #[tokio::test]
    async fn personality_requires_all_lists() {
        let thin = r#"{"title":"T","text":"x","traits":["a"]}"#;
        let full =
            r#"{"title":"T","text":"x","traits":["a"],"ideals":["b"],"bonds":["c"],"flaws":["d"]}"#;
        let runner = ScriptedRunner::texts(&[thin, full]);
        let r = run_task(
            &runner,
            &Narrative {
                input: input(NarrativeKind::Personality),
            },
            &RunOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(r.attempts, 2);
        let repair = &runner.requests.lock().unwrap()[1].prompt;
        assert!(
            repair.contains("`ideals`")
                && repair.contains("`flaws`")
                && !repair.contains("`traits` must")
        );
    }

    #[tokio::test]
    async fn npc_requires_profile() {
        let no = r#"{"title":"T","text":"x"}"#;
        let yes = r#"{"title":"T","text":"x","npc":{"name":"Bram","quirks":["hums"]}}"#;
        let runner = ScriptedRunner::texts(&[no, yes]);
        let r = run_task(
            &runner,
            &Narrative {
                input: input(NarrativeKind::Npc),
            },
            &RunOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(r.output.npc.unwrap().name, "Bram");
    }

    #[test]
    fn prompt_includes_length_tone_and_fenced_constraints() {
        let n = Narrative {
            input: input(NarrativeKind::Backstory),
        };
        assert!(n.system_prompt().contains("about 100 words"));
        let u = n.user_prompt();
        assert!(u.contains("grim") && u.contains("<user_input>\nno dragons"));
    }
}
