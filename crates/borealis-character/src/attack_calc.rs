//! The bonus and damage of a hand-made attack worked out from an ability (GitHub #29).

use crate::model::ManualAttack;

/// The ability names a hand-made attack links to, in score order.
const ABILITY_NAMES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

/// Length of the leading dice (`2d6`) of `text`, and of the dice with a modifier after them.
fn leading_dice(text: &str) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let count = digits(0);
    if count == 0 || b.get(count) != Some(&b'd') {
        return None;
    }
    let sides = digits(count + 1);
    if sides == 0 {
        return None;
    }
    let dice = count + 1 + sides;
    // an existing ` + 1` / `-2` after the dice
    let rest = &text[dice..];
    let trimmed = rest.trim_start();
    let with_mod = match trimmed.as_bytes().first() {
        Some(b'+' | b'-') => {
            let after_sign = trimmed[1..].trim_start();
            let n = after_sign.bytes().take_while(u8::is_ascii_digit).count();
            if n == 0 {
                dice
            } else {
                text.len() - after_sign.len() + n
            }
        }
        _ => dice,
    };
    Some((dice, with_mod))
}

/// `damage` with `modifier` after its leading dice, replacing a modifier already there
/// (`1d6 piercing`, +3: `1d6+3 piercing`). A text that does not start with dice stays as typed.
#[must_use]
pub fn with_modifier(damage: &str, modifier: i32) -> String {
    let text = damage.trim_start();
    let Some((dice, with_mod)) = leading_dice(text) else {
        return damage.to_string();
    };
    let sign = match modifier {
        0 => String::new(),
        m => format!("{m:+}"),
    };
    format!("{}{sign}{}", &text[..dice], &text[with_mod..])
}

impl ManualAttack {
    /// The attack bonus and damage this row shows: worked out from its ability when it is
    /// [`ManualAttack::calculated`] (the ability modifier plus `proficiency`, and the dice plus
    /// the modifier), else the typed texts. `modifiers` are in score order.
    #[must_use]
    pub fn worked_out(&self, modifiers: &[i32], proficiency: i32) -> (String, String) {
        let ability = self.ability.trim().to_lowercase();
        let index = ABILITY_NAMES
            .iter()
            .position(|n| !ability.is_empty() && n.starts_with(&ability));
        match index
            .and_then(|i| modifiers.get(i))
            .filter(|_| self.calculated)
        {
            Some(m) => (
                format!("{:+}", m + proficiency),
                with_modifier(&self.damage, *m),
            ),
            None => (self.attack.clone(), self.damage.clone()),
        }
    }
}
