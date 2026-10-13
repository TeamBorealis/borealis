//! The 2014 -> 2024 id mapping, built from `maps_to` / `renamed_from` links in the loaded packs.

use borealis_content::{ContentStore, MapKind};
use std::collections::HashMap;

/// Where a 2014 element goes in 2024.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The 2024 element id.
    pub id: String,
    /// How close the counterpart is.
    pub kind: Option<MapKind>,
}

/// Lookup from 2014 element ids to their 2024 counterparts.
#[derive(Debug, Clone, Default)]
pub struct MappingIndex {
    forward: HashMap<String, Target>,
}

fn rank(kind: Option<MapKind>) -> u8 {
    match kind {
        Some(MapKind::Equivalent) => 2,
        Some(MapKind::Approximate) | None => 1,
        Some(MapKind::None) => 0,
    }
}

impl MappingIndex {
    /// Build the index from every `maps_to` and `renamed_from` link in the store.
    #[must_use]
    pub fn build(store: &ContentStore) -> Self {
        let mut index = Self::default();
        for stored in store.iter() {
            let e = &stored.element;
            if let Some(m) = &e.maps_to {
                if let Some(old) = &m.dnd2014 {
                    index.insert(old, &e.id, m.kind);
                }
                if let Some(new) = &m.dnd2024 {
                    index.insert(&e.id, new, m.kind);
                }
            }
            if let Some(old) = e.renamed_from.as_deref().filter(|r| store.contains_id(r)) {
                index.insert(old, &e.id, Some(MapKind::Equivalent));
            }
        }
        index
    }

    fn insert(&mut self, from: &str, to: &str, kind: Option<MapKind>) {
        if from == to || rank(kind) == 0 {
            return;
        }
        let better = match self.forward.get(from) {
            Some(cur) => rank(kind) > rank(cur.kind),
            None => true,
        };
        if better {
            self.forward.insert(
                from.to_string(),
                Target {
                    id: to.to_string(),
                    kind,
                },
            );
        }
    }

    /// The 2024 counterpart of a 2014 id, if one is mapped.
    #[must_use]
    pub fn get(&self, old: &str) -> Option<&Target> {
        self.forward.get(old)
    }

    /// Number of mapped ids.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Is the index empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }
}
