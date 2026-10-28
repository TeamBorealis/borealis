//! Picker rows of one name (B-55, `specs/version-rows-and-filters.md` 1.1-1.2 and 4): the
//! engine-offered options of a slot that share a group key ([`borealis_engine::versions`]) are
//! the versions of one row. Grouping only presents what the engine offers; it never makes an
//! option choosable.

use borealis_content::{ContentStore, Element, PackOrigin, Ruleset};
use borealis_engine::versions::{group_key, name_key};
use std::cmp::Ordering;

/// One offered option, with what grouping and the default version order read.
#[derive(Debug, Clone)]
pub struct Candidate<'a, T> {
    /// What the caller turns into the DTO (the engine's option row).
    pub option: &'a T,
    /// The element.
    pub element: &'a Element,
    /// From bundled content (a `core` pack: the SRD packs).
    pub bundled: bool,
    /// Written for the character's edition, or for neither (edition-neutral).
    pub native: bool,
    group: String,
}

impl<'a, T> Candidate<'a, T> {
    /// A candidate of a character of `ruleset`.
    #[must_use]
    pub fn new(
        option: &'a T,
        element: &'a Element,
        store: &ContentStore,
        ruleset: Ruleset,
    ) -> Self {
        Self {
            option,
            element,
            bundled: store
                .get(&element.id)
                .is_some_and(|s| s.origin == PackOrigin::Core),
            native: element.ruleset.is_none_or(|r| r.compatible_with(ruleset)),
            group: group_key(&element.ty, &element.name),
        }
    }
}

/// The default version order (spec 1.2 rule 3), the slot's current pick first: the character's
/// edition (edition-neutral counts as it) before the other, imported before bundled, source
/// name A-Z, element id.
fn version_order<T>(a: &Candidate<'_, T>, b: &Candidate<'_, T>, chosen: Option<&str>) -> Ordering {
    let key = |c: &Candidate<'_, T>| {
        (
            chosen != Some(c.element.id.as_str()),
            !c.native,
            c.bundled,
            c.element.source.to_lowercase(),
        )
    };
    key(a)
        .cmp(&key(b))
        .then_with(|| a.element.source.cmp(&b.element.source))
        .then_with(|| a.element.id.cmp(&b.element.id))
}

/// The rows of a slot: candidates of one group key form a row, rows in the order their first
/// candidate is offered, versions in [`version_order`]. Rows whose normalized name does not
/// contain the normalized `query` are left out.
#[must_use]
pub fn group_rows<'a, T>(
    candidates: Vec<Candidate<'a, T>>,
    chosen: Option<&str>,
    query: Option<&str>,
) -> Vec<Vec<Candidate<'a, T>>> {
    let needle = query.map(name_key).filter(|q| !q.is_empty());
    let mut rows: Vec<Vec<Candidate<'a, T>>> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for c in candidates {
        if let Some(&at) = index.get(&c.group) {
            rows[at].push(c);
        } else {
            index.insert(c.group.clone(), rows.len());
            rows.push(vec![c]);
        }
    }
    rows.into_iter()
        .filter(|row| {
            needle
                .as_ref()
                .is_none_or(|n| name_key(&row[0].element.name).contains(n.as_str()))
        })
        .map(|mut row| {
            row.sort_by(|a, b| version_order(a, b, chosen));
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentFile, ContentStoreBuilder};
    use serde_json::json;

    fn store() -> ContentStore {
        let el = |id: &str, name: &str, source: &str, ruleset: Option<&str>| {
            let mut v = json!({"id": id, "name": name, "type": "Feat", "source": source});
            if let Some(r) = ruleset {
                v["ruleset"] = json!(r);
            }
            v
        };
        let core: ContentFile = serde_json::from_value(json!({"format": "borealis-content/1",
            "elements": [el("ID_B_24", "Alert", "Bundle 24", Some("dnd2024")),
                         el("ID_B_14", "Alert", "Bundle 14", Some("dnd2014"))]}))
        .unwrap();
        let custom: ContentFile = serde_json::from_value(json!({"format": "borealis-content/1",
            "elements": [el("ID_I_B", "alert ", "beta Book", Some("dnd2024")),
                         el("ID_I_A", "Alert", "Alpha Book", Some("dnd2024")),
                         el("ID_I_N", "ALERT", "Any Book", None),
                         el("ID_I_O", "Other", "Alpha Book", None)]}))
        .unwrap();
        ContentStoreBuilder::new()
            .add_file("core.json", PackOrigin::Core, core)
            .add_file("custom.json", PackOrigin::Custom, custom)
            .build()
    }

    fn ids(row: &[Candidate<'_, ()>]) -> Vec<String> {
        row.iter().map(|c| c.element.id.clone()).collect()
    }

    fn rows_of(
        store: &ContentStore,
        chosen: Option<&str>,
        query: Option<&str>,
    ) -> Vec<Vec<String>> {
        let order = ["ID_B_14", "ID_I_O", "ID_B_24", "ID_I_B", "ID_I_A", "ID_I_N"];
        let cands = order
            .iter()
            .map(|id| Candidate::new(&(), store.element(id).unwrap(), store, Ruleset::Dnd2024))
            .collect();
        group_rows(cands, chosen, query)
            .iter()
            .map(|r| ids(r))
            .collect()
    }

    #[test]
    fn default_order_is_edition_then_imported_then_source_then_id() {
        let s = store();
        let rows = rows_of(&s, None, None);
        assert_eq!(
            rows,
            vec![
                vec!["ID_I_A", "ID_I_N", "ID_I_B", "ID_B_24", "ID_B_14"],
                vec!["ID_I_O"],
            ]
        );
    }

    #[test]
    fn the_chosen_version_comes_first() {
        let s = store();
        assert_eq!(rows_of(&s, Some("ID_B_14"), None)[0][0], "ID_B_14");
    }

    #[test]
    fn the_query_matches_the_normalized_row_name() {
        let s = store();
        assert_eq!(rows_of(&s, None, Some(" ALE")).len(), 1);
        assert_eq!(rows_of(&s, None, Some("oth")), vec![vec!["ID_I_O"]]);
        assert!(rows_of(&s, None, Some("zzz")).is_empty());
    }
}
