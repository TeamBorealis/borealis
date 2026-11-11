//! SPELL LISTS (spec 12.4, HC5): put spells on a list and take them off. A spell of the pack changes its own tags
//! (through its form, [`with_tag`]); a loaded spell of another pack gets an additive append in the pack ("add to this
//! list", `appends[{id, supports: [tag]}]`, content-format 9.2), never an in-place edit. Only what the pack added can
//! be taken off: its own spells' tags and its own appends. Deterministic, no AI (G7).

use super::refs::same_tag;
use crate::form::{with_tag, FormContext, FormProblem};
use borealis_content::{Append, ContentFile, Element};

/// The spell carries the list's tag.
#[must_use]
pub fn on_list(spell: &Element, tag: &str) -> bool {
    spell.supports.iter().any(|s| same_tag(s, tag))
}

/// The pack's own append puts the spell `id` on the list `tag`.
#[must_use]
pub fn appended(content: &ContentFile, id: &str, tag: &str) -> bool {
    content
        .appends
        .iter()
        .any(|a| a.id == id && a.supports.iter().any(|s| same_tag(s, tag)))
}

/// The pack can take the spell off the list: it is the pack's own spell on that list, or the pack's append put it
/// there.
#[must_use]
pub fn removable(content: &ContentFile, spell: &Element, tag: &str) -> bool {
    let own = content.elements.iter().find(|e| e.id == spell.id);
    match own {
        Some(e) => on_list(e, tag),
        None => appended(content, &spell.id, tag),
    }
}

/// The result of a list change.
#[derive(Debug, Clone, PartialEq)]
pub struct ListChange {
    /// The pack's content after the change.
    pub content: ContentFile,
    /// The pack's own spells that changed (their ids), for the content checks.
    pub edited: Vec<String>,
}

/// An append with nothing left in it.
fn is_empty(a: &Append) -> bool {
    a.supports.is_empty()
        && a.setters.is_empty()
        && a.rules.is_empty()
        && a.spellcasting.is_none()
        && a.extra.is_empty()
}

fn add_append(appends: &mut Vec<Append>, id: &str, tag: &str) {
    match appends.iter_mut().find(|a| a.id == id) {
        Some(a) => {
            if !a.supports.iter().any(|s| same_tag(s, tag)) {
                a.supports.push(tag.to_string());
            }
        }
        None => appends.push(Append {
            id: id.to_string(),
            supports: vec![tag.to_string()],
            ..Append::default()
        }),
    }
}

fn remove_append(appends: &mut Vec<Append>, id: &str, tag: &str) {
    for a in appends.iter_mut().filter(|a| a.id == id) {
        a.supports.retain(|s| !same_tag(s, tag));
    }
    appends.retain(|a| a.id != id || !is_empty(a));
}

/// The pack's content with the spells `add` put on the list `tag` and `remove` taken off it. `add` and `remove` are
/// the spells as loaded (a spell of the pack is changed as the pack holds it).
///
/// # Errors
/// A spell the pack can't take off (another pack put it there), or a spell form that can't write.
pub fn with_spell_list(
    content: &ContentFile,
    tag: &str,
    add: &[&Element],
    remove: &[&Element],
    cx: &FormContext<'_>,
) -> Result<ListChange, Vec<FormProblem>> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Err(vec![FormProblem::new("", "Choose a spell list first.")]);
    }
    let mut next = content.clone();
    let mut edited = Vec::new();
    let mut problems = Vec::new();
    let changes = add
        .iter()
        .map(|s| (*s, true))
        .chain(remove.iter().map(|s| (*s, false)));
    for (spell, adding) in changes {
        let own = next.elements.iter().position(|e| e.id == spell.id);
        match own {
            Some(i) => match with_tag(&next.elements[i], tag, adding, cx) {
                Ok(Some(e)) => {
                    next.elements[i] = e;
                    if !edited.contains(&spell.id) {
                        edited.push(spell.id.clone());
                    }
                }
                Ok(None) => {}
                Err(ps) => problems.extend(ps.into_iter().map(|p| FormProblem {
                    message: format!("{}: {}", spell.name, p.message),
                    ..p
                })),
            },
            None if adding => {
                if !on_list(spell, tag) {
                    add_append(&mut next.appends, &spell.id, tag);
                }
            }
            None if appended(&next, &spell.id, tag) => {
                remove_append(&mut next.appends, &spell.id, tag);
            }
            None if on_list(spell, tag) => problems.push(FormProblem::new(
                "",
                format!(
                    "{} is on the {tag} list in its own pack. A homebrew pack can only add to that list.",
                    spell.name
                ),
            )),
            None => {}
        }
    }
    if problems.is_empty() {
        Ok(ListChange {
            content: next,
            edited,
        })
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr_build::condition::ConditionContext;
    use crate::form::{CatalogBlockCodec, NoLookup};
    use serde_json::json;

    fn el(v: serde_json::Value) -> Element {
        serde_json::from_value(v).expect("element")
    }

    fn spell(id: &str, tags: &[&str]) -> Element {
        el(
            json!({"id": id, "name": id, "type": "Spell", "source": "X", "supports": tags,
            "setters": [{"name": "level", "value": "1"}, {"name": "school", "value": "Evocation"},
                {"name": "time", "value": "1 action"}, {"name": "range", "value": "Self"},
                {"name": "duration", "value": "Instantaneous"}]}),
        )
    }

    fn with<T>(f: impl FnOnce(&FormContext<'_>) -> T) -> T {
        let codec = CatalogBlockCodec;
        let cx = FormContext {
            conditions: ConditionContext::new(["Wizard"]),
            lookup: &NoLookup,
            blocks: &codec,
        };
        f(&cx)
    }

    #[test]
    fn loaded_spells_get_an_append_and_own_spells_their_tag() {
        with(|cx| {
            let own = spell("ID_HB_SPELL_EMBER", &["Sorcerer"]);
            let content = ContentFile::new(vec![own.clone()]);
            let loaded = spell("ID_SRD_SPELL_BLESS", &["Cleric"]);
            let change =
                with_spell_list(&content, "Wizard", &[&own, &loaded], &[], cx).expect("change");
            assert_eq!(change.edited, vec!["ID_HB_SPELL_EMBER".to_string()]);
            assert!(on_list(&change.content.elements[0], "Wizard"));
            assert_eq!(
                serde_json::to_value(&change.content.appends).expect("json"),
                json!([{"id": "ID_SRD_SPELL_BLESS", "supports": ["Wizard"]}])
            );
            assert!(removable(&change.content, &loaded, "Wizard"));
            assert!(removable(&change.content, &own, "Wizard"));
            assert!(
                !removable(&change.content, &loaded, "Cleric"),
                "another pack's list"
            );
            // a second list on the same spell joins its append; adding again changes nothing
            let again =
                with_spell_list(&change.content, "Bard", &[&loaded], &[], cx).expect("change");
            assert_eq!(again.content.appends.len(), 1);
            assert_eq!(again.content.appends[0].supports, vec!["Wizard", "Bard"]);
            let same =
                with_spell_list(&again.content, "Wizard", &[&loaded], &[], cx).expect("change");
            assert_eq!(same.content, again.content);
        });
    }

    #[test]
    fn only_what_the_pack_added_comes_off() {
        with(|cx| {
            let own = spell("ID_HB_SPELL_EMBER", &["Sorcerer"]);
            let loaded = spell("ID_SRD_SPELL_BLESS", &["Cleric"]);
            let content = ContentFile::new(vec![own.clone()]);
            let added = with_spell_list(&content, "Wizard", &[&own, &loaded], &[], cx)
                .expect("change")
                .content;
            let mut on_wizard = loaded.clone();
            on_wizard.supports.push("Wizard".into());
            let back =
                with_spell_list(&added, "Wizard", &[], &[&added.elements[0], &on_wizard], cx)
                    .expect("change");
            assert_eq!(
                back.content, content,
                "the append is gone, the spell as it was"
            );
            let refused =
                with_spell_list(&content, "Cleric", &[], &[&loaded], cx).expect_err("refused");
            assert!(refused[0].message.contains("can only add"), "{refused:?}");
            assert!(with_spell_list(&content, " ", &[], &[], cx).is_err());
        });
    }

    #[test]
    fn an_append_keeps_its_other_parts_when_its_tag_comes_off() {
        with(|cx| {
            let loaded = spell("ID_SRD_SPELL_BLESS", &["Cleric", "Wizard"]);
            let mut content = ContentFile::new(Vec::new());
            content.appends.push(
                serde_json::from_value(json!({"id": "ID_SRD_SPELL_BLESS", "supports": ["Wizard"],
                    "setters": [{"name": "short", "value": "Blessed."}]}))
                .expect("append"),
            );
            let next = with_spell_list(&content, "Wizard", &[], &[&loaded], cx).expect("change");
            assert_eq!(
                serde_json::to_value(&next.content.appends).expect("json"),
                json!([{"id": "ID_SRD_SPELL_BLESS", "setters": [{"name": "short", "value": "Blessed."}]}])
            );
        });
    }
}
