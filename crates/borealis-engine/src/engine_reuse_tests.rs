//! IzDfk (unit tests: they compare the universes' private parts): a creator save swaps in a new engine; with a large library preparing every element of both universes again
//! was most of a save on a phone. An engine built over the previous one takes each prepared element whose store element
//! is the previous store's own allocation (unchanged, shared by `build_reusing`) and that no later pass of the previous
//! build changed; everything else is prepared again, and every universe-wide pass runs as before. The result must be
//! the engine a build from scratch makes.

use crate::{Engine, EngineSettings};
use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::{ContentStore, ContentStoreBuilder, Element, Ruleset, Setter};
use std::fs;
use std::path::Path;

fn bundled() -> ContentStoreBuilder {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut dirs: Vec<_> = fs::read_dir(&root)
        .expect("content dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.join("pack.json").exists())
        .collect();
    dirs.sort();
    let mut b = ContentStoreBuilder::new();
    for d in dirs {
        b = b.add_pack_dir(&d).expect("pack loads");
    }
    b
}

fn spell(id: &str, name: &str, level: &str) -> Element {
    let mut el = Element::new(id, name, "Spell", "Ember Isles");
    el.supports = vec!["Wizard".into()];
    el.setters.push(Setter::new("level", level));
    el
}

fn store(over: Option<&ContentStore>, homebrew: Vec<Element>) -> ContentStore {
    bundled()
        .add_file(
            "ember-isles/content.json",
            PackOrigin::Custom,
            ContentFile::new(homebrew),
        )
        .build_reusing(over)
}

/// Both universes as text (every prepared element in order, the indexes sorted, the diagnostics).
fn universes(e: &Engine) -> [String; 2] {
    [Ruleset::Dnd2014, Ruleset::Dnd2024].map(|r| e.universe(r).canonical())
}

/// The two engines' universes are the same text; else the first place they differ (the whole text is megabytes).
fn assert_same(reused: &Engine, fresh: &Engine) {
    for (a, b) in universes(reused).iter().zip(universes(fresh).iter()) {
        if a != b {
            let at = a
                .bytes()
                .zip(b.bytes())
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            let from = at.saturating_sub(300);
            panic!(
                "the universes differ at byte {at}:\n{}\n---\n{}",
                a.get(from..(at + 300).min(a.len())).unwrap_or(""),
                b.get(from..(at + 300).min(b.len())).unwrap_or("")
            );
        }
    }
}

#[test]
fn an_engine_built_over_the_previous_one_is_the_engine_built_from_scratch() {
    let settings = EngineSettings::default();
    let first = store(None, vec![spell("ID_EI_SPELL_LANCE", "Ember Lance", "1")]);
    let previous = Engine::with_settings(&first, settings.clone());
    // a save: the spell changed, one added; every other element is the previous store's own
    let next = store(
        Some(&first),
        vec![
            spell("ID_EI_SPELL_LANCE", "Ember Lance", "2"),
            spell("ID_EI_SPELL_SPARK", "Ember Spark", "0"),
        ],
    );
    let fresh = Engine::with_settings(&next, settings.clone());
    let reused = Engine::with_settings_reusing(&next, settings, &previous);
    assert_same(&reused, &fresh);
    for r in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        let u = reused.universe(r);
        // most of it is taken over; the changed and added spells, the classes, the elements the engine generates and
        // those a universe-wide pass changed are not
        assert!(
            u.reused_elements() * 4 > u.len() * 3,
            "{r:?}: {} of {} prepared elements taken over",
            u.reused_elements(),
            u.len()
        );
        assert!(u.reused_elements() < u.len());
        assert_eq!(fresh.universe(r).reused_elements(), 0);
    }
}

#[test]
fn a_changed_support_name_prepares_everything_again() {
    let settings = EngineSettings::default();
    let support = |name: &str| Element::new("ID_EI_SUPPORT_EMBER", name, "Support", "Ember Isles");
    let mut lance = spell("ID_EI_SPELL_LANCE", "Ember Lance", "1");
    lance.supports.push("ID_EI_SUPPORT_EMBER".into());
    let first = store(None, vec![support("Ember"), lance.clone()]);
    let previous = Engine::with_settings(&first, settings.clone());
    // every element's tags may name the support: none is taken over
    let next = store(Some(&first), vec![support("Cinder"), lance]);
    let fresh = Engine::with_settings(&next, settings.clone());
    let reused = Engine::with_settings_reusing(&next, settings, &previous);
    assert_same(&reused, &fresh);
    for r in [Ruleset::Dnd2014, Ruleset::Dnd2024] {
        assert_eq!(reused.universe(r).reused_elements(), 0, "{r:?}");
    }
}
