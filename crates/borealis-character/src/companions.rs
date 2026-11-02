//! Companions beyond the one content grants (GitHub #13, decision U16(c)).
//!
//! The granted companion is the creature a `Companion` select holds; its name stays in
//! `details.companion_name` and its picture in the UI extras, as before. What this module adds is
//! saved under the top-level `companions` key:
//! - `granted`: the user's notes, traits and bonuses for the granted companion;
//! - `added`: companions the user added by hand, each with its own creature, name, picture,
//!   notes, traits and bonuses.
//!
//! The key is left out when there is nothing in it, so a character with zero or one companion
//! and no edits is written byte for byte as before. Loading is lenient: a broken entry is dropped
//! or repaired and never stops the character loading. Keys this version does not know are kept.
//! Older apps (1.2.x) keep the whole block too, because they keep unknown top-level keys.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

mod load;

/// Most companions a character can add by hand.
pub const MAX_ADDED_COMPANIONS: usize = 12;
/// Most traits on one companion.
pub const MAX_COMPANION_TRAITS: usize = 30;
/// Longest companion or trait name, in characters.
pub const MAX_COMPANION_NAME: usize = 60;
/// Longest trait text, in characters.
pub const MAX_TRAIT_TEXT: usize = 2000;
/// Longest notes text, in characters.
pub const MAX_COMPANION_NOTES: usize = 4000;
/// Largest bonus (or penalty) to armor class, hit points or speed.
pub const MAX_COMPANION_BONUS: i32 = 99;
/// Prefix of the ids given to added companions.
pub const ADDED_ID_PREFIX: &str = "companion-";
/// Longest id kept as written; a longer one gets a new id on load.
pub(crate) const MAX_ID: usize = 64;

/// A trait the user wrote for a companion: a name and its text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionTrait {
    /// Name ("Keen Nose").
    pub name: String,
    /// Text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// The user's edits on one companion: notes, traits and simple bonuses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CompanionEdits {
    /// Free notes.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// User-written traits, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<CompanionTrait>,
    /// Added to the creature's armor class.
    #[serde(skip_serializing_if = "is_zero")]
    pub armor_class_bonus: i32,
    /// Added to the creature's hit points.
    #[serde(skip_serializing_if = "is_zero")]
    pub hit_points_bonus: i32,
    /// Added to the creature's walking speed (feet).
    #[serde(skip_serializing_if = "is_zero")]
    pub speed_bonus: i32,
}

/// A companion the user added by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AddedCompanion {
    /// Stable id (`companion-<n>`).
    pub id: String,
    /// Content element id of the creature (a `Companion` or `Monster` element); empty for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub creature: String,
    /// The user's name for it; empty shows the creature's name.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Picture as an image data URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portrait: Option<String>,
    /// Notes, traits and bonuses.
    #[serde(flatten)]
    pub edits: CompanionEdits,
    /// Keys this version does not know, kept and written back.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Everything saved under the top-level `companions` key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Companions {
    /// Edits on the granted companion.
    #[serde(skip_serializing_if = "CompanionEdits::is_empty")]
    pub granted: CompanionEdits,
    /// Companions added by hand, in the order they were added.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<AddedCompanion>,
    /// Keys this version does not know, kept and written back.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Why a companion change was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionError {
    /// A value is out of range or malformed; the text says which.
    Invalid(String),
    /// No added companion has that id.
    NotFound(String),
    /// The character already has [`MAX_ADDED_COMPANIONS`] added companions.
    TooMany,
}

impl std::fmt::Display for CompanionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(why) => f.write_str(why),
            Self::NotFound(id) => write!(f, "No companion has the id \"{id}\"."),
            Self::TooMany => write!(
                f,
                "A character can have at most {MAX_ADDED_COMPANIONS} added companions."
            ),
        }
    }
}

impl std::error::Error for CompanionError {}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &i32) -> bool {
    *n == 0
}

impl CompanionEdits {
    /// Nothing is edited.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The edits after checking every limit.
    ///
    /// # Errors
    /// [`CompanionError::Invalid`] naming the first value out of range.
    pub fn validated(self) -> Result<Self, CompanionError> {
        check_text("Notes", &self.notes, MAX_COMPANION_NOTES)?;
        if self.traits.len() > MAX_COMPANION_TRAITS {
            return Err(invalid(format!(
                "A companion can have at most {MAX_COMPANION_TRAITS} traits."
            )));
        }
        for t in &self.traits {
            check_name("A trait name", &t.name)?;
            check_text("A trait's text", &t.text, MAX_TRAIT_TEXT)?;
        }
        for (label, n) in [
            ("The armor class bonus", self.armor_class_bonus),
            ("The hit point bonus", self.hit_points_bonus),
            ("The speed bonus", self.speed_bonus),
        ] {
            if n.abs() > MAX_COMPANION_BONUS {
                return Err(invalid(format!(
                    "{label} must be between -{MAX_COMPANION_BONUS} and {MAX_COMPANION_BONUS}."
                )));
            }
        }
        Ok(Self {
            notes: self.notes.trim_end().to_string(),
            traits: self
                .traits
                .into_iter()
                .map(|t| CompanionTrait {
                    name: t.name.trim().to_string(),
                    text: t.text.trim_end().to_string(),
                })
                .collect(),
            ..self
        })
    }
}

impl Companions {
    /// Nothing to save.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.granted.is_empty() && self.added.is_empty() && self.extra.is_empty()
    }

    /// The added companion with this id.
    #[must_use]
    pub fn added(&self, id: &str) -> Option<&AddedCompanion> {
        self.added.iter().find(|c| c.id == id)
    }

    /// A copy with one more added companion; returns it with the new companion's id.
    ///
    /// # Errors
    /// [`CompanionError::TooMany`] at the limit, [`CompanionError::Invalid`] for a bad name.
    pub fn with_added(&self, creature: &str, name: &str) -> Result<(Self, String), CompanionError> {
        if self.added.len() >= MAX_ADDED_COMPANIONS {
            return Err(CompanionError::TooMany);
        }
        let name = validated_name(name)?;
        let id = next_id(&self.added);
        let companion = AddedCompanion {
            id: id.clone(),
            creature: creature.trim().to_string(),
            name,
            ..AddedCompanion::default()
        };
        let mut added = self.added.clone();
        added.push(companion);
        Ok((
            Self {
                added,
                ..self.clone()
            },
            id,
        ))
    }

    /// A copy without the added companion `id`.
    ///
    /// # Errors
    /// [`CompanionError::NotFound`] for an unknown id.
    pub fn without(&self, id: &str) -> Result<Self, CompanionError> {
        self.added(id)
            .ok_or_else(|| CompanionError::NotFound(id.to_string()))?;
        Ok(Self {
            added: self.added.iter().filter(|c| c.id != id).cloned().collect(),
            ..self.clone()
        })
    }

    /// A copy where the added companion `id` is replaced by `change(old)`.
    ///
    /// # Errors
    /// [`CompanionError::NotFound`] for an unknown id, or the error `change` returns.
    pub fn with_changed(
        &self,
        id: &str,
        change: impl FnOnce(&AddedCompanion) -> Result<AddedCompanion, CompanionError>,
    ) -> Result<Self, CompanionError> {
        let old = self
            .added(id)
            .ok_or_else(|| CompanionError::NotFound(id.to_string()))?;
        let new = AddedCompanion {
            id: old.id.clone(),
            ..change(old)?
        };
        Ok(Self {
            added: self
                .added
                .iter()
                .map(|c| if c.id == id { new.clone() } else { c.clone() })
                .collect(),
            ..self.clone()
        })
    }
}

/// A companion name after trimming, checked against the limits (it may be empty).
///
/// # Errors
/// [`CompanionError::Invalid`] when it is too long or holds control characters.
pub fn validated_name(name: &str) -> Result<String, CompanionError> {
    let name = name.trim();
    if name.chars().count() > MAX_COMPANION_NAME || name.chars().any(char::is_control) {
        return Err(invalid(format!(
            "A companion name is at most {MAX_COMPANION_NAME} characters, without control characters."
        )));
    }
    Ok(name.to_string())
}

fn invalid(why: String) -> CompanionError {
    CompanionError::Invalid(why)
}

fn check_name(label: &str, name: &str) -> Result<(), CompanionError> {
    let n = name.trim().chars().count();
    if n == 0 || n > MAX_COMPANION_NAME || name.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{label} must be 1 to {MAX_COMPANION_NAME} characters, without control characters."
        )));
    }
    Ok(())
}

fn check_text(label: &str, text: &str, max: usize) -> Result<(), CompanionError> {
    if text.chars().count() > max {
        return Err(invalid(format!("{label} is longer than {max} characters.")));
    }
    Ok(())
}

/// `companion-<n>`, one above the highest number in use.
pub(crate) fn next_id(added: &[AddedCompanion]) -> String {
    let top = added
        .iter()
        .filter_map(|c| c.id.strip_prefix(ADDED_ID_PREFIX)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{ADDED_ID_PREFIX}{}", top.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trait_(name: &str) -> CompanionTrait {
        CompanionTrait {
            name: name.into(),
            text: "Text.".into(),
        }
    }

    #[test]
    fn ids_count_up_and_are_not_reused_while_held() {
        let c = Companions::default();
        let (c, a) = c.with_added("ID_WOLF", " Ash ").unwrap();
        let (c, b) = c.with_added("", "").unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("companion-1", "companion-2"));
        assert_eq!(c.added[0].name, "Ash");
        let c = c.without("companion-1").unwrap();
        let (c, again) = c.with_added("", "").unwrap();
        assert_eq!(again, "companion-3");
        assert_eq!(c.added.len(), 2);
    }

    #[test]
    fn the_limits_are_enforced() {
        let mut c = Companions::default();
        for _ in 0..MAX_ADDED_COMPANIONS {
            c = c.with_added("", "").unwrap().0;
        }
        assert_eq!(c.with_added("", ""), Err(CompanionError::TooMany));
        assert!(validated_name(&"x".repeat(61)).is_err());
        assert!(validated_name("a\u{7}b").is_err());
        let too_many = CompanionEdits {
            traits: (0..31).map(|i| trait_(&format!("T{i}"))).collect(),
            ..CompanionEdits::default()
        };
        assert!(too_many.validated().is_err());
        let blank = CompanionEdits {
            traits: vec![trait_("  ")],
            ..CompanionEdits::default()
        };
        assert!(blank.validated().is_err());
        let big = CompanionEdits {
            speed_bonus: 100,
            ..CompanionEdits::default()
        };
        assert!(big.validated().is_err());
        assert_eq!(
            Companions::default().without("companion-9"),
            Err(CompanionError::NotFound("companion-9".into()))
        );
    }

    #[test]
    fn changes_keep_the_id_and_leave_the_original_untouched() {
        let (c, id) = Companions::default().with_added("", "Ash").unwrap();
        let changed = c
            .with_changed(&id, |old| {
                Ok(AddedCompanion {
                    id: "hijack".into(),
                    name: "Cinder".into(),
                    ..old.clone()
                })
            })
            .unwrap();
        assert_eq!(changed.added[0].id, id);
        assert_eq!(changed.added[0].name, "Cinder");
        assert_eq!(c.added[0].name, "Ash");
    }
}
