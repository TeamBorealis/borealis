//! Spells and inventory: mapped by id, kept as cross-edition picks when allowed, else dropped.

use super::mapping::MappingIndex;
use super::preview::{Category, ConversionItem, Outcome};
use borealis_content::{ContentStore, ElementId, Ruleset};
use borealis_engine::{CharacterState, Engine};

/// Shared lookups for gear conversion.
pub(crate) struct GearCx<'a> {
    pub store: &'a ContentStore,
    pub engine: &'a Engine,
    pub map: &'a MappingIndex,
}

/// The decision for one id.
enum Fate {
    Mapped(String, Option<borealis_content::MapKind>),
    Kept,
    Dropped(String),
}

impl GearCx<'_> {
    pub fn name_of(&self, id: &str) -> String {
        self.store
            .element(id)
            .map_or_else(|| id.to_string(), |e| e.name.clone())
    }

    fn fate(&self, id: &str, allow_cross: bool) -> Fate {
        if let Some(t) = self.map.get(id) {
            if self.engine.has_element(Ruleset::Dnd2024, &t.id) {
                return Fate::Mapped(t.id.clone(), t.kind);
            }
        }
        if !self.engine.has_element(Ruleset::Dnd2024, id) {
            return Fate::Dropped("not defined by the loaded content".to_string());
        }
        if allow_cross {
            Fate::Kept
        } else {
            Fate::Dropped("no 2024 equivalent and cross-edition picks are disabled".to_string())
        }
    }

    /// Convert one id; returns the id to store (if any) and the report line.
    fn convert_id(
        &self,
        id: &str,
        category: Category,
        allow_cross: bool,
    ) -> (Option<String>, ConversionItem) {
        let label = self.name_of(id);
        let (new, outcome) = match self.fate(id, allow_cross) {
            Fate::Mapped(to, kind) => (
                Some(to.clone()),
                Outcome::Mapped {
                    from: id.to_string(),
                    to,
                    kind,
                },
            ),
            Fate::Kept => (
                Some(id.to_string()),
                Outcome::Kept {
                    id: id.to_string(),
                    cross_edition: true,
                },
            ),
            Fate::Dropped(reason) => (
                None,
                Outcome::Dropped {
                    id: id.to_string(),
                    reason,
                },
            ),
        };
        (
            new,
            ConversionItem {
                category,
                label,
                outcome,
            },
        )
    }

    fn convert_list(
        &self,
        ids: &[String],
        category: Category,
        allow_cross: bool,
        items: &mut Vec<ConversionItem>,
    ) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in ids {
            let (new, item) = self.convert_id(id, category, allow_cross);
            items.push(item);
            if let Some(n) = new.filter(|n| !out.contains(n)) {
                out.push(n);
            }
        }
        out
    }

    /// Carry prepared spells, spellbooks and the inventory from `old` into `state`.
    pub fn carry(
        &self,
        old: &CharacterState,
        mut state: CharacterState,
        items: &mut Vec<ConversionItem>,
    ) -> CharacterState {
        let allow = state.allow_cross_edition.unwrap_or(true);
        for p in &old.prepared {
            let spells = self.convert_list(&p.spells, Category::Spell, allow, items);
            state.prepared.push(borealis_engine::state::PreparedSpells {
                class: p.class.clone(),
                spells,
            });
        }
        for (class, book) in &old.spellbooks {
            let ids: Vec<String> = book.iter().map(|e| e.0.clone()).collect();
            let spells = self.convert_list(&ids, Category::Spell, allow, items);
            state
                .spellbooks
                .insert(class.clone(), spells.into_iter().map(ElementId).collect());
        }
        state.inventory.storages = old.inventory.storages.clone();
        state.inventory.coins = old.inventory.coins;
        for it in &old.inventory.items {
            let (new, item) = self.convert_id(&it.item, Category::Item, allow);
            items.push(item);
            let Some(new_id) = new else { continue };
            let mut moved = it.clone();
            moved.item = new_id;
            moved.adorner = it
                .adorner
                .as_deref()
                .and_then(|a| self.convert_id(a, Category::Item, allow).0);
            state.inventory.items.push(moved);
        }
        state
    }
}
