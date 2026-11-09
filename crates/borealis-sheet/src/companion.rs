//! The companion stat block of the sheet (S6): the creature the character's Companion select
//! chose, as the app's companion view shows it. The app fills it through
//! [`crate::SheetContent::companion`]; the sheet crate only formats it.

const ABILITY_LABELS: [&str; 6] = ["STR", "DEX", "CON", "INT", "WIS", "CHA"];

/// One named trait, action or reaction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompanionEntry {
    /// Entry name.
    pub name: String,
    /// Entry text.
    pub text: String,
}

/// One skill or saving throw line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompanionSkillLine {
    /// Skill or ability name ("Perception", "Dexterity").
    pub name: String,
    /// Key ability abbreviation ("DEX").
    pub ability: String,
    /// Signed total bonus.
    pub bonus: i32,
}

/// The companion creature's stat block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompanionSheet {
    /// The creature's own name (the template).
    pub creature: String,
    /// The name the player gave it (may be empty).
    pub name: String,
    /// "Medium beast, unaligned".
    pub build: String,
    /// Armor class as printed.
    pub armor_class: String,
    /// Hit points as printed.
    pub hit_points: String,
    /// Initiative modifier.
    pub initiative: i32,
    /// Proficiency bonus.
    pub proficiency: i32,
    /// Challenge rating as printed.
    pub challenge: String,
    /// Walking speed in feet.
    pub speed: i32,
    /// Fly, climb, swim and burrow speed in feet (0 = none).
    pub other_speeds: [i32; 4],
    /// STR, DEX, CON, INT, WIS, CHA scores.
    pub abilities: [i32; 6],
    /// The six saving throws, STR to CHA (empty: no section).
    pub saves: Vec<CompanionSkillLine>,
    /// Skills the stat block lists.
    pub skills: Vec<CompanionSkillLine>,
    /// Passive Perception.
    pub passive_perception: i32,
    /// Damage and condition lines (`Damage Resistances`: `Poison`), in stat block order.
    pub defenses: Vec<CompanionEntry>,
    /// Special senses as printed (`Darkvision 60 ft.`); empty when the creature has none.
    pub senses: String,
    /// Languages as printed (empty: no line).
    pub languages: String,
    /// Traits.
    pub traits: Vec<CompanionEntry>,
    /// Actions.
    pub actions: Vec<CompanionEntry>,
    /// Reactions.
    pub reactions: Vec<CompanionEntry>,
}

/// Regions printed in the left column (statistics, saving throws, skills); the creature's
/// traits, actions and reactions go in the right one.
const LEFT_REGIONS: [&str; 3] = ["companion.stats", "companion.saves", "companion.skills"];

/// `+2` / `−1` / `+0`.
#[must_use]
pub fn signed(n: i32) -> String {
    if n < 0 {
        format!("\u{2212}{}", -n)
    } else {
        format!("+{n}")
    }
}

fn modifier(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

impl CompanionSheet {
    /// Heading of the page: the creature's name.
    #[must_use]
    pub fn title(&self) -> String {
        if self.creature.is_empty() {
            "Companion".to_string()
        } else {
            self.creature.clone()
        }
    }

    /// `40 ft., fly 60 ft.`
    #[must_use]
    pub fn speed_line(&self) -> String {
        let mut parts = vec![format!("{} ft.", self.speed)];
        for (label, v) in ["fly", "climb", "swim", "burrow"]
            .iter()
            .zip(self.other_speeds)
        {
            if v > 0 {
                parts.push(format!("{label} {v} ft."));
            }
        }
        parts.join(", ")
    }

    /// The statistics text: build, the headline numbers and the six scores, one per line.
    #[must_use]
    pub fn stats_text(&self) -> String {
        self.stats_lines(true)
    }

    /// The statistics text; `with_scores` false leaves the six scores out (the PDF draws them as
    /// score boxes).
    fn stats_lines(&self, with_scores: bool) -> String {
        let mut lines = Vec::new();
        if !self.name.is_empty() && self.name != self.creature {
            lines.push(format!("Name: {}", self.name));
        }
        if !self.build.is_empty() {
            lines.push(self.build.clone());
        }
        lines.push(format!(
            "AC {}   Initiative {}   HP {}",
            self.armor_class,
            signed(self.initiative),
            self.hit_points
        ));
        lines.push(format!("Speed {}", self.speed_line()));
        lines.push(format!(
            "Proficiency {}   Challenge {}",
            signed(self.proficiency),
            self.challenge
        ));
        let score = |i: usize| {
            format!(
                "{} {} ({})",
                ABILITY_LABELS[i],
                self.abilities[i],
                signed(modifier(self.abilities[i]))
            )
        };
        if with_scores {
            lines.push((0..3).map(score).collect::<Vec<_>>().join("   "));
            lines.push((3..6).map(score).collect::<Vec<_>>().join("   "));
        }
        // The lines Aurora's companion page prints after the saves and skills: damage and
        // condition lines, Senses (only when the creature has some) and Languages.
        for d in &self.defenses {
            lines.push(format!("{} {}", d.name, d.text.trim()));
        }
        for (label, text) in [("Senses", &self.senses), ("Languages", &self.languages)] {
            if !text.trim().is_empty() {
                lines.push(format!("{label} {}", text.trim()));
            }
        }
        lines.join("\n")
    }

    /// The saving throws text, one per line (`+4 Dexterity`); empty without saves.
    #[must_use]
    pub fn saves_text(&self) -> String {
        self.saves
            .iter()
            .map(|s| format!("{} {}", signed(s.bonus), s.name))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Whether the region `id` goes in the page's left column.
    #[must_use]
    pub fn in_left_column(id: &str) -> bool {
        LEFT_REGIONS.contains(&id)
    }

    /// The skills text, ending with Passive Perception.
    #[must_use]
    pub fn skills_text(&self) -> String {
        let mut lines: Vec<String> = self
            .skills
            .iter()
            .map(|s| format!("{} {} ({})", signed(s.bonus), s.name, s.ability))
            .collect();
        lines.push(format!("{} Passive Perception", self.passive_perception));
        lines.join("\n")
    }

    /// `Name. Text` paragraphs of a list of entries (empty when none).
    #[must_use]
    pub fn entries_text(entries: &[CompanionEntry]) -> String {
        entries
            .iter()
            .map(|e| format!("{}. {}", e.name, e.text.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The text regions of the page as `(id, title, text)`, in reading order; sections with no
    /// text are left out. The PDF page and the HTML preview both use this list.
    #[must_use]
    pub fn regions(&self) -> Vec<(&'static str, &'static str, String)> {
        self.regions_for(true)
    }

    /// [`CompanionSheet::regions`]; `scores_in_text` false leaves the six scores out of the
    /// statistics (drawn as score boxes instead).
    #[must_use]
    pub fn regions_for(&self, scores_in_text: bool) -> Vec<(&'static str, &'static str, String)> {
        [
            (
                "companion.stats",
                "Statistics",
                self.stats_lines(scores_in_text),
            ),
            ("companion.saves", "Saving Throws", self.saves_text()),
            ("companion.skills", "Skills", self.skills_text()),
            (
                "companion.traits",
                "Traits",
                Self::entries_text(&self.traits),
            ),
            (
                "companion.actions",
                "Actions",
                Self::entries_text(&self.actions),
            ),
            (
                "companion.reactions",
                "Reactions",
                Self::entries_text(&self.reactions),
            ),
        ]
        .into_iter()
        .filter(|(_, _, t)| !t.is_empty())
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hound() -> CompanionSheet {
        CompanionSheet {
            creature: "Test Hound".into(),
            name: "Rex".into(),
            build: "Medium beast, unaligned".into(),
            armor_class: "13".into(),
            hit_points: "11".into(),
            initiative: 2,
            proficiency: 2,
            challenge: "1/4".into(),
            speed: 40,
            other_speeds: [0, 30, 0, 0],
            abilities: [12, 15, 12, 3, 12, 6],
            skills: vec![CompanionSkillLine {
                name: "Perception".into(),
                ability: "WIS".into(),
                bonus: 3,
            }],
            passive_perception: 13,
            traits: vec![CompanionEntry {
                name: "Keen Hearing".into(),
                text: "Advantage on hearing checks.".into(),
            }],
            ..CompanionSheet::default()
        }
    }

    #[test]
    fn stats_text_lists_headline_numbers_and_scores() {
        let t = hound().stats_text();
        assert!(t.contains("AC 13   Initiative +2   HP 11"), "{t}");
        assert!(t.contains("Speed 40 ft., climb 30 ft."), "{t}");
        assert!(t.contains("DEX 15 (+2)"), "{t}");
        assert!(t.contains("INT 3 (\u{2212}4)"), "{t}");
        assert!(t.starts_with("Name: Rex"), "{t}");
    }

    #[test]
    fn skills_text_ends_with_passive_perception() {
        assert_eq!(
            hound().skills_text(),
            "+3 Perception (WIS)\n13 Passive Perception"
        );
    }

    #[test]
    fn regions_skip_empty_sections() {
        let ids: Vec<_> = hound().regions().iter().map(|r| r.0).collect();
        assert_eq!(
            ids,
            ["companion.stats", "companion.skills", "companion.traits"]
        );
    }

    fn cat() -> CompanionSheet {
        let save = |name: &str, ability: &str, bonus: i32| CompanionSkillLine {
            name: name.into(),
            ability: ability.into(),
            bonus,
        };
        CompanionSheet {
            saves: vec![
                save("Strength", "STR", -4),
                save("Dexterity", "DEX", 4),
                save("Constitution", "CON", 0),
                save("Intelligence", "INT", -4),
                save("Wisdom", "WIS", 1),
                save("Charisma", "CHA", -2),
            ],
            senses: "Darkvision 60 ft.".into(),
            languages: "\u{2014}".into(),
            defenses: vec![CompanionEntry {
                name: "Damage Resistances".into(),
                text: "Poison".into(),
            }],
            actions: vec![CompanionEntry {
                name: "Scratch".into(),
                text: "Melee Attack Roll: +4, reach 5 ft. Hit: 1 Slashing damage.".into(),
            }],
            ..hound()
        }
    }

    #[test]
    fn saving_throws_get_their_own_section_after_the_statistics() {
        let c = cat();
        assert_eq!(
            c.saves_text(),
            "\u{2212}4 Strength\n+4 Dexterity\n+0 Constitution\n\u{2212}4 Intelligence\n+1 Wisdom\n\u{2212}2 Charisma"
        );
        let ids: Vec<_> = c.regions().iter().map(|r| r.0).collect();
        assert_eq!(
            ids,
            [
                "companion.stats",
                "companion.saves",
                "companion.skills",
                "companion.traits",
                "companion.actions"
            ]
        );
        assert!(CompanionSheet::in_left_column("companion.saves"));
        assert!(!CompanionSheet::in_left_column("companion.actions"));
    }

    #[test]
    fn statistics_end_with_defenses_senses_and_languages() {
        let t = cat().stats_text();
        assert!(
            t.ends_with("Damage Resistances Poison\nSenses Darkvision 60 ft.\nLanguages \u{2014}"),
            "{t}"
        );
        // No senses: no Senses line (the stat block prints none either).
        let t = hound().stats_text();
        assert!(!t.contains("Senses"), "{t}");
    }

    #[test]
    fn a_familiars_actions_are_still_listed() {
        // Find Familiar says the familiar can't attack, but Aurora's companion page lists the
        // creature's actions all the same (U7), so the sheet keeps them.
        let regions = cat().regions();
        let actions = regions.iter().find(|r| r.0 == "companion.actions");
        assert_eq!(
            actions.map(|r| r.2.as_str()),
            Some("Scratch. Melee Attack Roll: +4, reach 5 ft. Hit: 1 Slashing damage.")
        );
    }
}
