//! The user's edits to an automatic attack row (GitHub #27) reach the sheet: typed texts replace
//! the worked-out ones, and a row left out of the combat section prints neither line nor card. A
//! hand-made row worked out from an ability (GitHub #29) prints the worked-out values.

use crate::model_fixtures::wizard_2014;
use crate::model_types::SpellFactsMap;
use borealis_character::AttackEdit;
use borealis_engine::snapshot::AttackSnapshot;

const UID: &str = "item-9";

fn with_sword() -> (
    borealis_character::Character,
    borealis_engine::CharacterSnapshot,
) {
    let (mut c, mut snap) = wizard_2014();
    snap.attacks.push(AttackSnapshot {
        uid: UID.into(),
        name: "Longsword".into(),
        range: String::new(),
        attack: 1,
        damage: "1d8-1 slashing".into(),
        ability: "strength".into(),
        proficient: false,
        mastery: None,
        crit_range: 20,
        die_floor: None,
        offhand_damage: None,
        ammunition: None,
        save: None,
        ability_lines: Vec::new(),
    });
    c.details.auto_attack_cards.insert(UID.into(), true);
    (c, snap)
}

fn sheet_row(
    c: &borealis_character::Character,
    snap: &borealis_engine::CharacterSnapshot,
) -> Option<(String, String, String, String)> {
    crate::sheet_data(c, snap, &SpellFactsMap::new())
        .combat
        .attacks
        .iter()
        .find(|a| a.kind == crate::model_types::AttackKind::Weapon)
        .map(|a| {
            (
                a.name.clone(),
                a.bonus.clone(),
                a.damage.clone(),
                a.range.clone(),
            )
        })
}

#[test]
fn typed_texts_replace_the_worked_out_ones_on_line_and_card() {
    let (mut c, snap) = with_sword();
    c.details.auto_attack_edits.insert(
        UID.into(),
        AttackEdit {
            name: "Oathblade".into(),
            range: "Reach".into(),
            attack: "+7".into(),
            description: "Glows near fiends".into(),
            ..AttackEdit::default()
        },
    );
    let (name, bonus, damage, range) = sheet_row(&c, &snap).expect("weapon line");
    assert_eq!(name, "Oathblade");
    assert_eq!(bonus, "+7");
    assert_eq!(
        damage, "1d8-1 slashing",
        "an untouched text stays worked out"
    );
    assert_eq!(range, "Reach");
    let cards = crate::card_model::attack_cards(&c, &snap, &crate::SheetContent::default());
    let card = cards.iter().find(|k| k.title == "Oathblade").expect("card");
    assert!(card.facts.contains(&("Attack".into(), "+7".into())));
    assert!(card.facts.contains(&("Range".into(), "Reach".into())));
    assert_eq!(card.body, "Glows near fiends");
}

#[test]
fn a_row_left_out_of_the_combat_section_prints_no_line_and_no_card() {
    let (mut c, snap) = with_sword();
    assert!(sheet_row(&c, &snap).is_some());
    c.details.auto_attack_edits.insert(
        UID.into(),
        AttackEdit {
            hidden: true,
            ..AttackEdit::default()
        },
    );
    assert!(sheet_row(&c, &snap).is_none());
    let cards = crate::card_model::attack_cards(&c, &snap, &crate::SheetContent::default());
    assert!(cards.iter().all(|k| k.title != "Longsword"));
}

#[test]
fn a_calculated_hand_made_row_prints_its_worked_out_bonus_and_damage() {
    // GitHub #29: the Strength modifier plus proficiency, and the dice plus the modifier
    let (mut c, snap) = wizard_2014();
    c.details.attacks.push(borealis_character::ManualAttack {
        name: "Bite".into(),
        attack: "+0".into(),
        damage: "1d6 piercing".into(),
        displayed: true,
        ability: "Strength".into(),
        calculated: true,
        as_card: Some(true),
        ..borealis_character::ManualAttack::default()
    });
    let row = crate::sheet_data(&c, &snap, &SpellFactsMap::new())
        .combat
        .attacks
        .into_iter()
        .find(|a| a.name == "Bite")
        .expect("bite line");
    let m = snap.abilities[0].modifier;
    let bonus = format!("{:+}", m + snap.proficiency_bonus);
    let damage = borealis_character::with_modifier("1d6 piercing", m);
    assert_ne!(
        damage, "1d6 piercing",
        "the fixture has a Strength modifier"
    );
    assert_eq!(row.bonus, bonus);
    assert_eq!(row.damage, damage);
    let cards = crate::card_model::attack_cards(&c, &snap, &crate::SheetContent::default());
    let card = cards.iter().find(|k| k.title == "Bite").expect("card");
    assert!(card.facts.contains(&("Attack".into(), bonus)));
    assert!(card.facts.contains(&("Damage".into(), damage)));
}
