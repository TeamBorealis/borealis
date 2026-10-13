//! Profile tables read from the content (`ID_2024_PROFILE_TABLES`, an element of type `Core`
//! tagged `Profile Tables`): XP, proficiency bonus, point buy, standard arrays and fixed hit
//! points. Values present in the content override the profile's built-in design values.

use crate::profile::AbilityGeneration;
use borealis_content::Element;
use serde_json::Value;
use std::collections::BTreeMap;

/// Support tag that marks a profile tables element.
pub const PROFILE_TABLES_TAG: &str = "Profile Tables";

/// Tables a content pack supplies for its ruleset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileTables {
    /// Experience thresholds for levels 1..=20.
    pub xp: Option<[u32; 20]>,
    /// Proficiency bonus for levels 1..=20.
    pub proficiency: Option<[i32; 20]>,
    /// Point buy cost per base score.
    pub point_buy_costs: BTreeMap<i32, i32>,
    /// Point buy budget.
    pub point_buy_budget: Option<i32>,
    /// Lowest and highest base score under point buy.
    pub point_buy_range: Option<(i32, i32)>,
    /// The standard array.
    pub standard_array: Option<[i32; 6]>,
    /// Standard array assignment per class name (lower-cased), STR..CHA.
    pub standard_array_by_class: BTreeMap<String, [i32; 6]>,
    /// Fixed hit points per level (after level 1) per class name (lower-cased).
    pub fixed_hit_points: BTreeMap<String, u32>,
    /// Names of the Standard languages.
    pub standard_languages: Vec<String>,
    /// Default ability maximum.
    pub ability_cap: Option<i32>,
    /// Ability maximum granted by Epic Boons.
    pub epic_boon_cap: Option<i32>,
}

fn int_array<const N: usize, T: TryFrom<i64> + Copy + Default>(v: &Value) -> Option<[T; N]> {
    let a = v.as_array()?;
    if a.len() != N {
        return None;
    }
    let mut out = [T::default(); N];
    for (slot, x) in out.iter_mut().zip(a) {
        *slot = T::try_from(x.as_i64()?).ok()?;
    }
    Some(out)
}

fn to_i32(v: &Value) -> Option<i32> {
    i32::try_from(v.as_i64()?).ok()
}

const ABILITY_ORDER: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

impl ProfileTables {
    /// Is this element a profile tables element?
    #[must_use]
    pub fn is_tables_element(el: &Element) -> bool {
        el.ty == "Core" && el.supports.iter().any(|s| s == PROFILE_TABLES_TAG)
    }

    /// Read the tables from the element's `data`; unreadable parts stay unset.
    #[must_use]
    pub fn from_element(el: &Element) -> Self {
        let d = &el.data;
        let mut t = Self {
            xp: d.get("experience_points").and_then(int_array::<20, u32>),
            proficiency: d.get("proficiency_bonus").and_then(int_array::<20, i32>),
            standard_array: d.get("standard_array").and_then(int_array::<6, i32>),
            ability_cap: d.get("ability_score_cap").and_then(to_i32),
            epic_boon_cap: d.get("epic_boon_ability_cap").and_then(to_i32),
            ..Self::default()
        };
        if let Some(pb) = d.get("point_buy").and_then(Value::as_object) {
            t.point_buy_budget = pb.get("budget").and_then(to_i32);
            t.point_buy_range = pb
                .get("min_score")
                .and_then(to_i32)
                .zip(pb.get("max_score").and_then(to_i32));
            if let Some(costs) = pb.get("costs").and_then(Value::as_object) {
                t.point_buy_costs = costs
                    .iter()
                    .filter_map(|(k, v)| Some((k.parse().ok()?, to_i32(v)?)))
                    .collect();
            }
        }
        if let Some(by) = d.get("standard_array_by_class").and_then(Value::as_object) {
            for (class, scores) in by {
                let mut arr = [0; 6];
                for (slot, name) in arr.iter_mut().zip(ABILITY_ORDER) {
                    *slot = scores.get(name).and_then(to_i32).unwrap_or(10);
                }
                t.standard_array_by_class.insert(class.to_lowercase(), arr);
            }
        }
        if let Some(hp) = d
            .get("fixed_hit_points_per_level")
            .and_then(Value::as_object)
        {
            t.fixed_hit_points = hp
                .iter()
                .filter_map(|(k, v)| Some((k.to_lowercase(), u32::try_from(v.as_u64()?).ok()?)))
                .collect();
        }
        if let Some(l) = d
            .get("languages")
            .and_then(|l| l.get("standard"))
            .and_then(Value::as_array)
        {
            t.standard_languages = l
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        t
    }

    /// Proficiency bonus for a level, if the tables define it.
    #[must_use]
    pub fn proficiency_bonus(&self, level: u32) -> Option<i32> {
        self.proficiency
            .map(|t| t[(level.clamp(1, 20) - 1) as usize])
    }

    /// Point buy cost of a base score: the table when it lists the score, else `fallback`.
    #[must_use]
    pub fn point_cost(&self, score: i32, fallback: impl Fn(i32) -> i32) -> i32 {
        self.point_buy_costs
            .get(&score)
            .copied()
            .unwrap_or_else(|| fallback(score))
    }

    /// Fixed hit points per level (after level 1) for a class name.
    #[must_use]
    pub fn fixed_hp(&self, class_name: &str) -> Option<u32> {
        self.fixed_hit_points
            .get(&class_name.trim().to_lowercase())
            .copied()
    }

    /// The recommended standard array assignment of a class.
    #[must_use]
    pub fn standard_array_for(&self, class_name: &str) -> Option<[i32; 6]> {
        self.standard_array_by_class
            .get(&class_name.trim().to_lowercase())
            .copied()
    }

    /// Ability score generation rules: the profile's, overridden by the tables.
    #[must_use]
    pub fn generation(&self, base: AbilityGeneration) -> AbilityGeneration {
        AbilityGeneration {
            point_buy_budget: self.point_buy_budget.unwrap_or(base.point_buy_budget),
            point_buy_range: self.point_buy_range.unwrap_or(base.point_buy_range),
            standard_array: self.standard_array.unwrap_or(base.standard_array),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn element(data: Value) -> Element {
        let mut el = Element {
            ty: "Core".into(),
            supports: vec![PROFILE_TABLES_TAG.into()],
            ..Element::default()
        };
        if let Value::Object(m) = data {
            el.data = m.into_iter().collect();
        }
        el
    }

    #[test]
    fn reads_arrays_costs_and_class_tables() {
        let el = element(json!({
            "proficiency_bonus": [2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,6,6,6,6],
            "standard_array": [15,14,13,12,10,8],
            "point_buy": {"budget": 27, "min_score": 8, "max_score": 15, "costs": {"8": 0, "15": 9}},
            "fixed_hit_points_per_level": {"Fighter": 6},
            "standard_array_by_class": {"Fighter": {"strength": 15, "dexterity": 14, "constitution": 13, "intelligence": 8, "wisdom": 10, "charisma": 12}}
        }));
        assert!(ProfileTables::is_tables_element(&el));
        let t = ProfileTables::from_element(&el);
        assert_eq!(t.proficiency_bonus(17), Some(6));
        assert_eq!(t.point_cost(15, |_| -1), 9);
        assert_eq!(t.point_cost(16, |_| -1), -1);
        assert_eq!(t.fixed_hp("fighter"), Some(6));
        assert_eq!(
            t.standard_array_for("Fighter"),
            Some([15, 14, 13, 8, 10, 12])
        );
        assert_eq!(t.point_buy_range, Some((8, 15)));
    }
}
