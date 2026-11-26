//! Accidental duplicate ids (HC20, spec 22.5): an element of the draft whose id belongs to an element of another loaded
//! pack replaces that element wherever it loads later (custom content is "last wins", D-06). On purpose that is an
//! override (saved apart, spec 22.2), so the ids an override draft overrides are exempt; anything else is warned
//! about. Deterministic, no AI (G7).

use super::problem::{Problem, ProblemSeverity};
use crate::form::FormState;
use borealis_content::{ContentStore, IssueCode};
use std::collections::HashSet;

/// The problem code.
pub const ID_CLASH: &str = "id.clash";
/// What the warning says.
pub const ID_CLASH_MESSAGE: &str =
    "Another pack has an element with this id; this one replaces it wherever it loads \
later. To change that element on purpose, override it instead.";

/// The id-clash warnings of the draft's forms. `pack_id` is the draft's pack (its files are `<pack_id>/...`); `exempt`
/// are the ids the draft overrides.
#[must_use]
pub fn id_clash_problems(
    form: &FormState,
    store: &ContentStore,
    pack_id: &str,
    exempt: &HashSet<&str>,
) -> Vec<Problem> {
    let own = format!("{pack_id}/");
    let mut out = Vec::new();
    walk(form, "", &mut |prefix, f| {
        let Some(id) = f.id() else { return };
        if exempt.contains(id) || !clashes(store, id, &own) {
            return;
        }
        out.push(Problem::new(
            ProblemSeverity::Warning,
            ID_CLASH,
            ID_CLASH_MESSAGE,
            format!("{prefix}fields.name"),
            Some(id),
        ));
    });
    out
}

fn walk(form: &FormState, prefix: &str, f: &mut dyn FnMut(&str, &FormState)) {
    f(prefix, form);
    for (i, c) in form.children.iter().enumerate() {
        walk(c, &format!("{prefix}children.{i}."), f);
    }
}

/// `id` is defined by another pack: the loaded element (or the original it overrides) comes from a file outside the
/// pack, or the pack's element replaced another pack's when the content loaded.
fn clashes(store: &ContentStore, id: &str, own: &str) -> bool {
    let other_file = store.get_exact(id).is_some_and(|s| {
        let file = s.overridden.as_ref().map_or(&s.file, |o| &o.file);
        !file.starts_with(own)
    });
    other_file
        || store.diagnostics().iter().any(|d| {
            d.code == IssueCode::ElementReplaced
                && d.element.as_deref() == Some(id)
                && d.file.as_deref().is_some_and(|f| f.starts_with(own))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentFile, ContentStoreBuilder, Element, PackOrigin};

    fn form(id: &str, child: Option<&str>) -> FormState {
        let mut f = FormState::new("e", "other");
        f.memo.id = Some(id.into());
        if let Some(c) = child {
            let mut cf = FormState::new("e/0", "other");
            cf.memo.id = Some(c.into());
            f.children.push(cf);
        }
        f
    }

    fn store() -> ContentStore {
        let srd = ContentFile::new(vec![Element::new("ID_SRD_X", "X", "Spell", "SRD")]);
        let mut own = ContentFile::new(vec![
            Element::new("ID_HB_OWN", "Own", "Spell", "Hb"),
            Element::new("ID_SHARED", "Mine", "Spell", "Hb"),
        ]);
        own.overrides = vec![Element::new("ID_SRD_X", "X changed", "Spell", "SRD")];
        let other = ContentFile::new(vec![Element::new("ID_SHARED", "Theirs", "Spell", "Other")]);
        ContentStoreBuilder::new()
            .add_file("srd-5.1/spells.json", PackOrigin::Core, srd)
            .add_file("aa-other/content.json", PackOrigin::Custom, other)
            .add_file("homebrew-hb/content.json", PackOrigin::Custom, own)
            .build()
    }

    #[test]
    fn an_id_of_another_pack_is_warned_about_unless_overridden() {
        let s = store();
        let none = HashSet::new();
        let p = id_clash_problems(
            &form("ID_HB_OWN", Some("ID_SRD_X")),
            &s,
            "homebrew-hb",
            &none,
        );
        assert_eq!(p.len(), 1, "{p:?}");
        assert_eq!(p[0].code, ID_CLASH);
        assert_eq!(p[0].path.as_deref(), Some("children.0.fields.name"));
        assert_eq!(p[0].severity, ProblemSeverity::Warning);
        let exempt: HashSet<&str> = ["ID_SRD_X"].into_iter().collect();
        assert!(id_clash_problems(&form("ID_SRD_X", None), &s, "homebrew-hb", &exempt).is_empty());
    }

    #[test]
    fn the_pack_s_element_that_replaced_another_s_is_warned_about() {
        let p = id_clash_problems(
            &form("ID_SHARED", None),
            &store(),
            "homebrew-hb",
            &HashSet::new(),
        );
        assert_eq!(p.len(), 1, "{p:?}");
    }

    #[test]
    fn the_pack_s_own_ids_are_fine() {
        assert!(id_clash_problems(
            &form("ID_HB_OWN", None),
            &store(),
            "homebrew-hb",
            &HashSet::new()
        )
        .is_empty());
        let mut new = FormState::new("e", "other");
        new.memo.id = None;
        assert!(id_clash_problems(&new, &store(), "homebrew-hb", &HashSet::new()).is_empty());
    }
}
