//! Host characters of the source-truth audit (`source_truth.rs`): a level-1 fighter for
//! species, subraces and backgrounds; for feats the corpus sweep's hosts (level 1 human with an
//! origin / variant feat, the level 4 and 19 feat slots), else a host built to the feat's
//! prerequisites (`src-tauri/tests/support/prereq.rs`). Every host comes as the pair
//! (state before the pick, state after it), so the audit can read what the pick changed.
#![allow(dead_code)]

use super::prereq::{self, Needs, NeedsReader};
use borealis_content::{ContentStore, Element, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterSnapshot, CharacterState, Choice, Engine, SlotKey};
use std::collections::HashMap;

/// Base scores of every host (feat prerequisites raise them).
pub const SCORES: [i32; 6] = [15, 14, 14, 12, 12, 10];
const FILL_PASSES: usize = 60;
const MAX_LEVEL: u32 = 20;
/// Slot types the host fill leaves open (the audit picks them itself).
const NOT_FILLED: &[&str] = &[
    "Race",
    "Sub Race",
    "Species",
    "Background",
    "Class",
    "Archetype",
    "Feat",
    "Multiclass",
];

pub fn fighter(rs: Ruleset) -> &'static str {
    if rs == Ruleset::Dnd2024 {
        "ID_5ET_XPHB_CLASS_FIGHTER"
    } else {
        "ID_5ET_PHB_CLASS_FIGHTER"
    }
}

fn human(rs: Ruleset) -> &'static str {
    if rs == Ruleset::Dnd2024 {
        "ID_5ET_XPHB_SPECIES_HUMAN"
    } else {
        "ID_5ET_PHB_RACE_HUMAN"
    }
}

const VARIANT_HUMAN: &str = "ID_5ET_PHB_SUBRACE_HUMAN_VARIANT";

/// A host before and after the audited pick.
pub struct Picked {
    pub before: CharacterState,
    pub after: CharacterState,
}

pub struct Kit<'a> {
    pub engine: &'a Engine,
    pub store: &'a ContentStore,
    pub ours: &'a HashMap<String, Ruleset>,
}

impl Kit<'_> {
    pub fn derive(&self, s: &CharacterState) -> CharacterSnapshot {
        self.engine.derive(s)
    }

    /// A level-1 character of `class` with `scores`.
    pub fn with_class(&self, rs: Ruleset, class: &str, scores: [i32; 6]) -> Option<CharacterState> {
        let s = commands::new_character(self.engine, rs);
        let s = commands::set_base_scores(self.engine, &s, scores)
            .ok()?
            .state;
        let key = slot_key(&self.derive(&s), "ID_LEVEL_1", "Class", 1)?;
        commands::select(self.engine, &s, &key, Choice::Element(class.into()))
            .ok()
            .map(|o| o.state)
    }

    /// Choose `target` in the first open slot that offers it.
    pub fn pick(&self, s: &CharacterState, target: &str) -> Option<Result<CharacterState, String>> {
        let snap = self.derive(s);
        let slot = snap
            .slots
            .iter()
            .find(|x| x.chosen.is_none() && x.options.iter().any(|o| o.id == target))?;
        Some(
            commands::select(self.engine, s, &slot.key, Choice::Element(target.into()))
                .map(|o| o.state)
                .map_err(|e| format!("select refused: {e}")),
        )
    }

    /// Fill open slots (other than the [`NOT_FILLED`] types) with their first option.
    pub fn fill(&self, s: CharacterState) -> CharacterState {
        self.fill_skipping(s, &[])
    }

    /// [`Kit::fill`], also leaving slots of the `skip` types open.
    pub fn fill_skipping(&self, mut s: CharacterState, skip: &[&str]) -> CharacterState {
        let mut refused: Vec<SlotKey> = Vec::new();
        for _ in 0..FILL_PASSES {
            let snap = self.derive(&s);
            let Some(slot) = snap.slots.iter().find(|x| {
                x.chosen.is_none()
                    && x.list_choice.is_none()
                    && !NOT_FILLED.contains(&x.ty.as_str())
                    && !skip.contains(&x.ty.as_str())
                    && !refused.contains(&x.key)
                    && (!x.options.is_empty() || !x.list_items.is_empty())
            }) else {
                break;
            };
            let choice = match (slot.options.first(), slot.list_items.first()) {
                (Some(o), _) => Choice::Element(o.id.clone()),
                (None, Some((i, _))) => Choice::ListItem(*i),
                (None, None) => break,
            };
            match commands::select(self.engine, &s, &slot.key, choice) {
                Ok(o) => s = o.state,
                Err(_) => refused.push(slot.key.clone()),
            }
        }
        s
    }

    pub fn level_up(&self, s: &CharacterState) -> Option<CharacterState> {
        commands::level_up_main(self.engine, s, None)
            .ok()
            .map(|o| o.state)
    }

    /// The level-1 fighter host of an origin (class choices filled, origin slots open).
    pub fn origin_host(&self, rs: Ruleset) -> Option<CharacterState> {
        self.with_class(rs, fighter(rs), SCORES)
            .map(|s| self.fill(s))
    }

    /// Species / background: picked on the origin host. A subrace: its race, then the subrace,
    /// both against the bare host (its expectation is the race and subrace merged).
    pub fn origin(&self, rs: Ruleset, id: &str, parent: Option<&str>) -> Result<Picked, String> {
        let before = self.origin_host(rs).ok_or("no fighter host")?;
        let mut on = before.clone();
        if let Some(p) = parent {
            on = self.pick(&on, p).ok_or("parent race not offered")??;
        }
        let after = self.pick(&on, id).ok_or("not offered on the host")??;
        Ok(Picked { before, after })
    }

    fn feat_option(snap: &CharacterSnapshot) -> Option<(SlotKey, String)> {
        snap.slots.iter().find_map(|s| {
            let o = s
                .options
                .iter()
                .find(|o| prereq::is_feat_option_5et(&o.id, &o.name))?;
            (s.chosen.is_none() && s.ty == "Class Feature").then(|| (s.key.clone(), o.id.clone()))
        })
    }

    /// Take the 2014 "Feat" option of open improvement selects so the feat select opens.
    pub fn open_feat_selects(&self, mut s: CharacterState) -> CharacterState {
        while let Some((key, id)) = Self::feat_option(&self.derive(&s)) {
            match commands::select(self.engine, &s, &key, Choice::Element(id)) {
                Ok(o) => s = o.state,
                Err(_) => break,
            }
        }
        s
    }

    /// Level 1 (human with an origin / variant feat), 4 and 19 fighters with their feat
    /// selects open.
    pub fn feat_hosts(&self, rs: Ruleset) -> Vec<CharacterState> {
        let Some(mut s) = self.origin_host(rs) else {
            return Vec::new();
        };
        for id in [human(rs), VARIANT_HUMAN] {
            if let Some(Ok(next)) = self.pick(&s, id) {
                s = next;
            }
        }
        let mut s = self.fill(s);
        let mut out = vec![s.clone()];
        for target in [4, 19] {
            while self.derive(&s).level < target {
                match self.level_up(&s) {
                    Some(n) => s = n,
                    None => return out,
                }
            }
            out.push(self.open_feat_selects(s.clone()));
        }
        out
    }

    /// A feat picked on the first shared host that offers it, else on a host built to its
    /// prerequisites.
    pub fn feat(
        &self,
        rs: Ruleset,
        el: &Element,
        hosts: &[CharacterState],
    ) -> Result<Picked, String> {
        for h in hosts {
            if let Some(r) = self.pick(h, &el.id) {
                return r.map(|after| Picked {
                    before: h.clone(),
                    after,
                });
            }
        }
        self.eligible(rs, el)
    }

    fn eligible(&self, rs: Ruleset, el: &Element) -> Result<Picked, String> {
        let reader = NeedsReader {
            store: self.store,
            ours: self.ours,
            class_id: prereq::class_id_5et,
        };
        let mut needs = Needs::default();
        reader.needs(rs, el, 0, &mut needs);
        let mut s = self
            .start(rs, &needs)
            .ok_or("prerequisite host could not be built")?;
        let mut pending = needs.feats.clone();
        loop {
            s = self.open_feat_selects(s);
            while let Some(next) = pending.first() {
                match self.pick(&s, next) {
                    Some(Ok(n)) => {
                        s = self.fill(n);
                        pending.remove(0);
                    }
                    _ => break,
                }
            }
            let level = self.derive(&s).level;
            if pending.is_empty() && level >= needs.level {
                if let Some(r) = self.pick(&s, &el.id) {
                    return r.map(|after| Picked { before: s, after });
                }
            }
            if level >= MAX_LEVEL {
                return Err("not offered on any host".into());
            }
            s = self.level_up(&s).ok_or("level up refused")?;
        }
    }

    fn start(&self, rs: Ruleset, needs: &Needs) -> Option<CharacterState> {
        let class = needs.class.clone().unwrap_or_else(|| {
            if needs.caster {
                prereq::class_id_5et(rs, "wizard")
            } else {
                fighter(rs).to_string()
            }
        });
        let mut scores = SCORES;
        for (i, v) in &needs.scores {
            scores[*i] = scores[*i].max(*v);
        }
        let mut s = self.fill(self.with_class(rs, &class, scores)?);
        let race = needs.race.clone().unwrap_or_else(|| human(rs).to_string());
        let mut chain = Vec::new();
        if let Some(parent) = self
            .store
            .element(&race)
            .filter(|e| e.ty == "Sub Race")
            .and_then(|e| e.supports.first())
        {
            chain.push(parent.clone());
        }
        chain.push(race);
        if needs.race.is_none() {
            chain.push(VARIANT_HUMAN.into());
        }
        for id in chain {
            if let Some(Ok(n)) = self.pick(&s, &id) {
                s = n;
            }
        }
        Some(self.fill(s))
    }
}
