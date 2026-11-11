//! Plain sentences for the creature validator's problems (`monster_validate.rs`, code `InvalidMonsterData`): one per
//! part of the stat block, read from the problem's location (`data.ac`, `data.actions[2].damage[0]`) and its text.

/// The sentence for a stat block problem at `path` (`None`: the stat block can't be read at all).
#[must_use]
pub fn monster_message(path: Option<&str>, message: &str) -> String {
    let Some(at) = path.and_then(|p| p.strip_prefix("data.")) else {
        return "Part of this creature's stat block can't be read. Undo the last change, or remove the part kept from the original."
            .into();
    };
    let part = at.split(['.', '[']).next().unwrap_or(at);
    let s = match part {
        "size" => "Pick at least one size.",
        "alignment" => "Write the creature's alignment, like unaligned or neutral evil.",
        "ac" => "Give the creature an Armor Class above 0, or words that say how it is worked out.",
        "ac_note" => "Words for the Armor Class stand in for its number: remove one of them.",
        "hp" if at.ends_with("formula") => "Write the hit dice as dice, like 2d8+2.",
        "hp" => "Give the creature hit points above 0, or words that say how they are worked out.",
        "hp_note" => "Words for the hit points stand in for their number: remove one of them.",
        "abilities" => "Each ability score is from 1 to 30.",
        "speed" => "Give the creature at least one speed.",
        "damage" => "Each damage line needs a damage type or the words for it.",
        "cr_note" => "Words for the challenge rating stand in for it: remove them or the challenge rating.",
        "proficiency_bonus" => "The proficiency bonus doesn't match the challenge rating. Clear it to use the usual one.",
        "xp" if message.contains("without") => "Experience points come with a challenge rating: pick one, or clear the XP.",
        "xp" => "Pick a challenge rating: the experience points follow it.",
        "xp_lair" => "The experience points in its lair can't be fewer than the usual ones.",
        "gear" => "Remove the empty gear line.",
        "legendary" if at.ends_with("per_round") => "Say how many legendary actions it can take each round.",
        "legendary" if at.ends_with("lair_uses") => {
            "Legendary actions in its lair can't be fewer than the usual ones."
        }
        "lair_actions" | "regional_effects" if at.contains("options") => {
            "Each option needs its text, and a name when it has one."
        }
        "lair_actions" => "Write the lair actions, or remove them.",
        "regional_effects" => "Write the regional effects, or remove them.",
        _ => entry_message(message),
    };
    s.into()
}

/// A trait's, action's or legendary action's problem, by what the validator found.
fn entry_message(message: &str) -> &'static str {
    if message.contains("name and text") {
        "Each entry needs a name and its text."
    } else if message.contains("recharge") {
        "A recharge is a range on a d6, like 5 to 6."
    } else if message.contains("in lair") {
        "Uses in its lair can't be fewer than the usual uses per day."
    } else if message.contains("spell list") {
        "Each spell list needs at least one spell."
    } else if message.contains("dice") {
        "Write each damage roll as dice, like 2d6+3."
    } else if message.contains("cost") {
        "A legendary action costs at least 1 action."
    } else {
        "Part of this creature's stat block doesn't add up. Check the marked part."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_part_of_a_stat_block_has_its_own_sentence() {
        assert_eq!(
            monster_message(Some("data.ac"), "needs an armor class or a note for it"),
            "Give the creature an Armor Class above 0, or words that say how it is worked out."
        );
        assert_eq!(
            monster_message(Some("data.hp.formula"), "\"2d\" is not NdS[+-M]"),
            "Write the hit dice as dice, like 2d8+2."
        );
        assert_eq!(
            monster_message(Some("data.actions[1]"), "name and text must not be empty"),
            "Each entry needs a name and its text."
        );
        assert_eq!(
            monster_message(Some("data.traits[0].damage[1]"), "bad dice \"x\""),
            "Write each damage roll as dice, like 2d6+3."
        );
        assert_eq!(
            monster_message(Some("data.xp"), "XP without a challenge rating"),
            "Experience points come with a challenge rating: pick one, or clear the XP."
        );
        assert!(monster_message(None, "missing field `size`").starts_with("Part of this creature"));
    }
}
