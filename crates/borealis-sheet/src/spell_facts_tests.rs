use super::*;
use borealis_content::{ContentStore, ContentStoreBuilder};
use std::path::Path;

fn store(pack: &str) -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    ContentStoreBuilder::new()
        .add_pack_dir(&root.join("core"))
        .and_then(|b| b.add_pack_dir(&root.join(pack)))
        .expect("bundled packs load")
        .build()
}

fn facts(store: &ContentStore, id: &str) -> SpellFacts {
    spell_facts(store.element(id).unwrap_or_else(|| panic!("{id} missing")))
}

fn effect(f: &SpellFacts) -> &str {
    f.effect.as_deref().expect("effect")
}

#[test]
fn srd_2014_spells() {
    let s = store("srd-5.1");
    let fire_bolt = facts(&s, "ID_PHB_SPELL_FIRE_BOLT");
    assert_eq!(fire_bolt.casting_time.as_deref(), Some("1 action"));
    assert_eq!(fire_bolt.range.as_deref(), Some("120 feet"));
    assert_eq!(fire_bolt.components.as_deref(), Some("V, S"));
    assert_eq!(effect(&fire_bolt), "Ranged attack, 1d10 fire");
    assert!(!fire_bolt.concentration && !fire_bolt.ritual);

    let fireball = facts(&s, "ID_PHB_SPELL_FIREBALL");
    assert_eq!(fireball.components.as_deref(), Some("V, S, M"));
    assert_eq!(effect(&fireball), "8d6 fire, DEX save, half on save");

    let cure = facts(&s, "ID_PHB_SPELL_CURE_WOUNDS");
    assert_eq!(effect(&cure), "Heals 1d8 + mod");
    assert_eq!(cure.range.as_deref(), Some("Touch"));

    let shield = facts(&s, "ID_PHB_SPELL_SHIELD");
    assert_eq!(shield.casting_time.as_deref(), Some("1 reaction"));
    assert!(effect(&shield).starts_with("An invisible barrier"));

    let detect = facts(&s, "ID_PHB_SPELL_DETECT_MAGIC");
    assert!(detect.ritual && detect.concentration);
    assert_eq!(detect.duration.as_deref(), Some("Up to 10 minutes"));

    let bless = facts(&s, "ID_PHB_SPELL_BLESS");
    assert!(bless.concentration && !bless.ritual);
    assert_eq!(bless.components.as_deref(), Some("V, S, M"));
}

#[test]
fn srd_2024_spells() {
    let s = store("srd-5.2.1");
    let fire_bolt = facts(&s, "ID_2024_SPELL_FIRE_BOLT");
    assert_eq!(fire_bolt.casting_time.as_deref(), Some("Action"));
    assert_eq!(effect(&fire_bolt), "Ranged attack, 1d10 fire");

    let fireball = facts(&s, "ID_2024_SPELL_FIREBALL");
    assert_eq!(effect(&fireball), "8d6 fire, DEX save, half on save");
    assert_eq!(fireball.range.as_deref(), Some("150 feet"));

    let cure = facts(&s, "ID_2024_SPELL_CURE_WOUNDS");
    assert_eq!(effect(&cure), "Heals 2d8 plus mod");

    let detect = facts(&s, "ID_2024_SPELL_DETECT_MAGIC");
    assert!(detect.ritual && detect.concentration);
    assert_eq!(detect.casting_time.as_deref(), Some("Action"));

    let bless = facts(&s, "ID_2024_SPELL_BLESS");
    assert!(bless.concentration);
    assert!(effect(&bless).starts_with("You bless up to three creatures"));
}

#[test]
fn every_bundled_spell_yields_bounded_facts() {
    for (pack, prefix) in [
        ("srd-5.1", "ID_PHB_SPELL_"),
        ("srd-5.2.1", "ID_2024_SPELL_"),
    ] {
        let s = store(pack);
        let mut n = 0;
        for el in s
            .iter()
            .map(|e| &e.element)
            .filter(|e| e.ty == "Spell" && e.id.starts_with(prefix))
        {
            let f = spell_facts(el);
            n += 1;
            assert!(f.casting_time.is_some(), "{} time", el.id);
            assert!(f.range.is_some(), "{} range", el.id);
            assert!(f.duration.is_some(), "{} duration", el.id);
            let e = f.effect.unwrap_or_else(|| panic!("{} effect", el.id));
            assert!(e.chars().count() <= EFFECT_MAX_CHARS, "{}: {e}", el.id);
        }
        assert!(n > 250, "{pack}: only {n} spells");
    }
}

#[test]
fn effect_is_deterministic_and_clipped() {
    let long = format!("<p>{}</p>", "word ".repeat(60));
    let e = spell_effect(&long).unwrap();
    assert!(e.chars().count() <= EFFECT_MAX_CHARS && e.ends_with('\u{2026}'));
    assert_eq!(spell_effect(&long), Some(e));
    assert_eq!(spell_effect("<p> </p>"), None);
}

#[test]
fn attack_roll_and_save_ability_are_extracted() {
    let s = store("srd-5.1");
    let fire_bolt = facts(&s, "ID_PHB_SPELL_FIRE_BOLT");
    assert!(fire_bolt.attack_roll && fire_bolt.save.is_none());
    let acid = facts(&s, "ID_PHB_SPELL_ACID_SPLASH");
    assert!(!acid.attack_roll);
    assert_eq!(acid.save.as_deref(), Some("DEX"));
}
