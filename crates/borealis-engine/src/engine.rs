//! The public entry point.

use crate::adapter::{EditionAdapter, StandardAdapter};
use crate::derive::{thresholds, Evaluation};
use crate::profile::{all_profiles, profile_for};
use crate::provenance::{EvaluateOptions, StatTrace};
use crate::settings::EngineSettings;
use crate::snapshot::CharacterSnapshot;
use crate::state::CharacterState;
use crate::universe::Universe;
use borealis_content::{ContentStore, Ruleset};

/// The rules engine for one content store.
///
/// Building an engine prepares the element universe (derived multiclass and improvement elements,
/// parsed expressions); evaluating characters against it is then cheap and side-effect free.
#[derive(Debug)]
pub struct Engine {
    settings: EngineSettings,
    universes: [Universe; 2],
    /// Process-unique id of this engine (its content and settings never change after build).
    serial: u64,
}

/// Source of [`Engine::serial`].
static NEXT_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Engine {
    /// Prepare an engine with default settings.
    #[must_use]
    pub fn new(store: &ContentStore) -> Self {
        Self::with_settings(store, EngineSettings::default())
    }

    /// Prepare an engine with explicit settings.
    #[must_use]
    pub fn with_settings(store: &ContentStore, settings: EngineSettings) -> Self {
        Self::with_adapter(store, settings, &StandardAdapter)
    }

    /// Prepare an engine whose foreign-edition content goes through `adapter`.
    #[must_use]
    pub fn with_adapter(
        store: &ContentStore,
        settings: EngineSettings,
        adapter: &dyn EditionAdapter,
    ) -> Self {
        Self {
            settings,
            universes: all_profiles().map(|p| Universe::with_adapter(store, p.ruleset(), adapter)),
            serial: NEXT_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Evaluate a character against a store (convenience; prefer holding an [`Engine`]).
    #[must_use]
    pub fn evaluate(store: &ContentStore, state: &CharacterState) -> CharacterSnapshot {
        Self::new(store).derive(state)
    }

    /// Process-unique id of this engine: two engines with the same serial are one engine.
    #[must_use]
    pub(crate) fn serial(&self) -> u64 {
        self.serial
    }

    /// The settings in effect.
    #[must_use]
    pub fn settings(&self) -> &EngineSettings {
        &self.settings
    }

    /// The prepared element universe for a ruleset.
    #[must_use]
    pub fn universe(&self, ruleset: Ruleset) -> &Universe {
        &self.universes[profile_for(ruleset).universe_index()]
    }

    /// Derive everything from a character state.
    #[must_use]
    pub fn derive(&self, state: &CharacterState) -> CharacterSnapshot {
        self.derive_with(state, self.settings.include_options)
    }

    /// Derive with explicit control over option lists (they dominate the snapshot size).
    #[must_use]
    pub fn derive_with(&self, state: &CharacterState, include_options: bool) -> CharacterSnapshot {
        self.derive_opts(state, include_options, EvaluateOptions::default())
    }

    /// Derive with explicit [`EvaluateOptions`] (provenance traces on request).
    #[must_use]
    pub fn derive_opts(
        &self,
        state: &CharacterState,
        include_options: bool,
        opts: EvaluateOptions,
    ) -> CharacterSnapshot {
        let uni = self.universe(state.ruleset);
        let snap = Evaluation::run(uni, state, &self.settings, opts.provenance).snapshot(
            state,
            &self.settings,
            include_options,
            opts.provenance,
        );
        if include_options && !opts.provenance {
            crate::invariants::after_derive(self, state, &snap);
        }
        snap
    }

    /// The deterministic provenance trace of one statistic (`ac`, `strength:score`, ...):
    /// base value, every contributing or rejected rule with the reason, the final value and the
    /// diagnostics touching it. An unknown statistic yields an empty trace with value 0.
    #[must_use]
    pub fn explain(&self, state: &CharacterState, stat: &str) -> StatTrace {
        let key = stat.trim().to_lowercase();
        let uni = self.universe(state.ruleset);
        let eval = Evaluation::run(uni, state, &self.settings, true);
        let diags = eval.diags.items().to_vec();
        eval.trace_of(&key, &diags)
    }

    /// Run `f` on an internal evaluation (used by commands that need the graph).
    pub(crate) fn with_evaluation<R>(
        &self,
        state: &CharacterState,
        f: impl FnOnce(&Evaluation<'_>) -> R,
    ) -> R {
        let uni = self.universe(state.ruleset);
        f(&Evaluation::run(uni, state, &self.settings, false))
    }

    /// XP needed to reach `level` (1..=20) in the state's ruleset.
    #[must_use]
    pub fn xp_threshold(&self, ruleset: Ruleset, level: u32) -> u32 {
        thresholds(self.universe(ruleset))[(level.clamp(1, 20) - 1) as usize]
    }

    /// Does the content define this element id?
    #[must_use]
    pub fn has_element(&self, ruleset: Ruleset, id: &str) -> bool {
        self.universe(ruleset).get(id).is_some()
    }

    /// Option default (`default` setter) for Option elements: ids that new characters register.
    #[must_use]
    pub fn default_options(&self, ruleset: Ruleset) -> Vec<String> {
        self.universe(ruleset)
            .by_type("Option")
            .filter(|o| {
                o.el.setter_value("default")
                    .and_then(borealis_content::setter::parse_bool)
                    .unwrap_or(false)
            })
            .map(|o| o.el.id.clone())
            .collect()
    }
}
