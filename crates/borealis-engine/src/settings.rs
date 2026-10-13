//! Engine settings and per-character house rules.

use serde::{Deserialize, Serialize};

/// Application-level settings that influence engine results (`storage-and-settings.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSettings {
    /// Apply the per-ability maximum (`<ability>:max`, normally 20). Off lifts the cap to 100.
    pub use_default_ability_maximum: bool,
    /// Hard attunement limit override; `None` uses the `attunement:max` statistic (3).
    pub attunement_limit: Option<u32>,
    /// Point buy budget override (spending beyond it is rejected); `None` uses the profile's.
    pub point_buy_budget: Option<i32>,
    /// Lowest base ability score the commands accept.
    pub min_base_score: i32,
    /// Highest base ability score the commands accept.
    pub max_base_score: i32,
    /// New characters get the "average hit points" option.
    pub default_average_hp: bool,
    /// Include option lists in snapshots (large for spell slots).
    pub include_options: bool,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            use_default_ability_maximum: true,
            attunement_limit: None,
            point_buy_budget: None,
            min_base_score: 3,
            max_base_score: 30,
            default_average_hp: true,
            include_options: true,
        }
    }
}

/// Per-character overrides of [`EngineSettings`], stored in the character file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HouseRules {
    /// Override of the ability maximum switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_default_ability_maximum: Option<bool>,
    /// Override of the attunement limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attunement_limit: Option<u32>,
    /// Override of the point buy budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_buy_budget: Option<i32>,
}
