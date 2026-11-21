//! GitHub #70: the Play tab's Situational list (`snapshot.notes`) holds only plain sentences of
//! rules to remember at the table. Spell grants, re-pick timing and proficiency bookkeeping are
//! shown by their own readers (free casts, the builder, weapon proficiency) and by the feature's
//! own text, never as `key: value` dumps of the rule's fields (SRD 5.2.1 content).
use borealis_content::{ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{AbilityMethod, CharacterState};
use borealis_engine::{Choice, Engine};
use std::path::Path;
use std::sync::OnceLock;

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("packs load")
            .build();
        Engine::new(&store)
    })
}

const LEVEL1: &str = "ID_LEVEL_1";
const ACOLYTE: &str = "ID_2024_BACKGROUND_ACOLYTE";

fn pick(s: &CharacterState, slot: &str, id: &str) -> CharacterState {
    let key =
        slot_key(&engine().derive(s), LEVEL1, slot, 1).unwrap_or_else(|| panic!("no {slot} slot"));
    commands::select(engine(), s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|e| panic!("select {id}: {e}"))
        .state
}

/// A level `level` character of `class` (standard array), Acolyte (Magic Initiate (Cleric)).
fn hero(species: &str, class: &str, level: u32) -> CharacterSnapshot {
    borealis_engine::invariants::enable_for_thread();
    let mut s = commands::new_character(engine(), Ruleset::Dnd2024);
    s = pick(&s, "Race", species);
    s = pick(&s, "Class", class);
    s = pick(&s, "Background", ACOLYTE);
    s = commands::set_ability_method(&s, AbilityMethod::StandardArray);
    s = commands::set_base_scores(engine(), &s, [15, 14, 13, 12, 10, 8])
        .expect("legal scores")
        .state;
    while s.level() < level {
        s = commands::level_up_main(engine(), &s, None)
            .expect("level up")
            .state;
    }
    engine().derive(&s)
}

/// Every Situational row reads as plain text: no field keys, JSON or snake_case tokens.
fn assert_plain(snap: &CharacterSnapshot) {
    for n in &snap.notes {
        let t = &n.text;
        assert!(
            !t.contains(['_', '{', '}', '[', ']', '"'])
                && !t.contains(": true")
                && !t.contains(": false"),
            "{} note from {} is not plain text: {t:?}",
            n.kind,
            n.source
        );
    }
}

fn texts(snap: &CharacterSnapshot) -> Vec<String> {
    snap.notes
        .iter()
        .map(|n| format!("{} | {}", n.source, n.text))
        .collect()
}

/// The reported character: a level 1 Halfling Cleric with Magic Initiate (Cleric) from the Acolyte
/// background. Its free cast is a tracked use, its level-up spell swap is the feat's own text: no
/// Situational row comes from the feat. Brave stays as a structured advantage.
#[test]
fn magic_initiate_adds_no_situational_rows() {
    let snap = hero("ID_2024_SPECIES_HALFLING", "ID_2024_CLASS_CLERIC", 1);
    assert!(snap.has_element("ID_2024_FEAT_MAGIC_INITIATE"));
    assert!(
        snap.notes.iter().all(|n| n.source != "Magic Initiate"),
        "{:?}",
        texts(&snap)
    );
    assert_plain(&snap);
    let brave = snap
        .advantages
        .iter()
        .find(|a| a.source == "Brave")
        .expect("Brave advantage");
    assert_eq!(
        brave.text,
        "Advantage on saving throws against being Frightened (to avoid or end the condition)"
    );
}

/// The Rogue's martial-weapon proficiency filter is applied to weapons; it is not a Situational row.
#[test]
fn rogue_weapon_proficiency_is_not_a_situational_row() {
    let snap = hero("ID_2024_SPECIES_HUMAN", "ID_2024_CLASS_ROGUE", 1);
    assert!(
        snap.notes.iter().all(|n| n.kind != "proficiency_filter"),
        "{:?}",
        texts(&snap)
    );
    assert_plain(&snap);
}

/// Wild Companion (SRD 5.2.1 Druid level 2): one plain sentence naming the spell.
#[test]
fn wild_companion_reads_as_a_sentence() {
    let snap = hero("ID_2024_SPECIES_HUMAN", "ID_2024_CLASS_DRUID", 2);
    let note = snap
        .notes
        .iter()
        .find(|n| n.source == "Wild Companion")
        .unwrap_or_else(|| panic!("Wild Companion note: {:?}", texts(&snap)));
    assert_eq!(
        note.text,
        "Cast Find Familiar by expending a spell slot or a wild shape use, without Material components"
    );
    assert_plain(&snap);
}

/// Spell Mastery (level 18) and Signature Spells (level 20), SRD 5.2.1 Wizard: plain sentences.
#[test]
fn wizard_capstones_read_as_sentences() {
    let snap = hero("ID_2024_SPECIES_HUMAN", "ID_2024_CLASS_WIZARD", 20);
    let find = |source: &str| {
        snap.notes
            .iter()
            .find(|n| n.source == source)
            .unwrap_or_else(|| panic!("{source} note: {:?}", texts(&snap)))
            .text
            .clone()
    };
    assert_eq!(
        find("Spell Mastery"),
        "Cast your chosen level 1 and level 2 spells (casting time: an action) at their lowest level without a spell slot"
    );
    assert_eq!(
        find("Signature Spells"),
        "Cast each of 2 chosen level 3 spells once without a spell slot; regained on a short rest or long rest"
    );
    assert_plain(&snap);
}
