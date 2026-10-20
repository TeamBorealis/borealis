//! The last full derives of a content hub's engine.
//!
//! The builder derives an unchanged character again and again: an option page of every slot
//! the user opens, and the selection that follows, each derive the whole character with every
//! option list. A derive is a pure function of the engine (its content and settings) and the
//! character's state, so the hub remembers its last few full derives by state; a new hub (new
//! content or settings) starts empty.

use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::CharacterState;
use borealis_engine::Engine;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Full derives a hub keeps (a few open characters, each with its latest state).
const KEEP: usize = 4;

/// The last [`KEEP`] full derives of one engine, most recent first.
#[derive(Debug, Default)]
pub struct DeriveCache {
    recent: Mutex<VecDeque<(CharacterState, Arc<CharacterSnapshot>)>>,
}

impl DeriveCache {
    /// [`Engine::derive`] of `state`, from the cache when this state was derived lately. The
    /// engine must be the one this cache belongs to (its hub's).
    pub fn derive(&self, engine: &Engine, state: &CharacterState) -> Arc<CharacterSnapshot> {
        if let Some(hit) = self.take(state) {
            self.keep(state, &hit);
            return hit;
        }
        // derived outside the lock: other characters' derives do not wait for this one
        let snap = Arc::new(engine.derive(state));
        self.keep(state, &snap);
        snap
    }

    /// Keeps `snap`, which must be [`Engine::derive`] of `state` by this cache's engine (a
    /// command that evaluated the state already made it).
    pub fn insert(&self, state: &CharacterState, snap: &Arc<CharacterSnapshot>) {
        self.keep(state, snap);
    }

    /// Removes and returns the derive of `state`, if kept (a poisoned lock is a miss).
    fn take(&self, state: &CharacterState) -> Option<Arc<CharacterSnapshot>> {
        let mut recent = self.recent.lock().ok()?;
        let at = recent.iter().position(|(s, _)| s == state)?;
        recent.remove(at).map(|(_, snap)| snap)
    }

    /// Keeps the derive of `state` as the most recent one.
    fn keep(&self, state: &CharacterState, snap: &Arc<CharacterSnapshot>) {
        let Ok(mut recent) = self.recent.lock() else {
            return;
        };
        recent.retain(|(s, _)| s != state);
        recent.push_front((state.clone(), Arc::clone(snap)));
        recent.truncate(KEEP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentStoreBuilder, Ruleset};
    use borealis_engine::commands;
    use std::path::Path;

    fn engine() -> Engine {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .expect("bundled packs")
            .build();
        Engine::new(&store)
    }

    #[test]
    fn a_kept_state_is_not_derived_again_and_matches_a_fresh_derive() {
        let e = engine();
        let cache = DeriveCache::default();
        let one = commands::new_character(&e, Ruleset::Dnd2014);
        let mut two = one.clone();
        two.experience += 10;
        let first = cache.derive(&e, &one);
        assert!(Arc::ptr_eq(&first, &cache.derive(&e, &one)), "kept");
        assert_eq!(*first, e.derive(&one), "a kept derive is the engine's");
        let other = cache.derive(&e, &two);
        assert!(!Arc::ptr_eq(&first, &other), "another state is derived");
        assert!(Arc::ptr_eq(&first, &cache.derive(&e, &one)), "both kept");
    }

    #[test]
    fn a_level_up_keeps_the_derive_its_command_made() {
        let e = engine();
        let cache = DeriveCache::default();
        let one = commands::new_character(&e, Ruleset::Dnd2014);
        let out = commands::level_up_main_derived(&e, &one, None).expect("level up");
        let carried = out.derived.clone().expect("a level up drops nothing here");
        cache.insert(&out.state, &carried);
        let got = cache.derive(&e, &out.state);
        assert!(Arc::ptr_eq(&carried, &got), "not derived again");
        assert_eq!(
            *got,
            e.derive(&out.state),
            "the carried derive is the engine's"
        );
    }

    #[test]
    fn the_oldest_derive_is_dropped_past_the_limit() {
        let e = engine();
        let cache = DeriveCache::default();
        let states: Vec<CharacterState> = (0..=KEEP)
            .map(|i| {
                let mut s = commands::new_character(&e, Ruleset::Dnd2014);
                s.experience += 10 * u32::try_from(i).expect("small");
                s
            })
            .collect();
        let snaps: Vec<_> = states.iter().map(|s| cache.derive(&e, s)).collect();
        // the newest KEEP are kept; the first was dropped and is derived again
        assert!(Arc::ptr_eq(&snaps[KEEP], &cache.derive(&e, &states[KEEP])));
        assert!(!Arc::ptr_eq(&snaps[0], &cache.derive(&e, &states[0])));
    }
}
