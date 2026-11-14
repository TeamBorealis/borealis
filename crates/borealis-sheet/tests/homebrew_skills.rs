//! HC1 (item HtCY8): a skill a homebrew `Skill` element adds prints like the eighteen: its own name, a field of its
//! own (key from its name), the proficiency mark, and room for its row in the 2024 ability cards and the 2014 list.

mod common;

use borealis_character::Character;
use borealis_engine::snapshot::SkillSnapshot;
use borealis_engine::CharacterSnapshot;
use borealis_sheet::model_ids::{skill, skill_exp, skill_prof};
use borealis_sheet::{render_html, render_pdf, sheet_data, SheetOptions};
use lopdf::{Document, Object};
use std::collections::BTreeSet;

/// Fully qualified names of every widget (partial names joined along `/Parent`), as `sheet_v2.rs` reads them.
fn field_names(doc: &Document) -> BTreeSet<String> {
    let partial = |d: &lopdf::Dictionary| match d.get(b"T") {
        Ok(Object::String(t, _)) => Some(String::from_utf8_lossy(t).into_owned()),
        _ => None,
    };
    let mut out = BTreeSet::new();
    for obj in doc.objects.values() {
        let Object::Dictionary(d) = obj else { continue };
        if d.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Widget") {
            continue;
        }
        let mut parts = vec![partial(d).unwrap_or_default()];
        let mut cur = d.clone();
        while let Ok(Object::Reference(r)) = cur.get(b"Parent") {
            let Ok(Object::Dictionary(pd)) = doc.get_object(*r) else {
                break;
            };
            parts.push(partial(pd).unwrap_or_default());
            cur = pd.clone();
        }
        parts.reverse();
        out.insert(parts.join("."));
    }
    out
}

fn homebrew(name: &str, key: &str, ability: &str, proficient: bool) -> SkillSnapshot {
    SkillSnapshot {
        name: key.into(),
        ability: ability.into(),
        proficiency: if proficient { 3 } else { 0 },
        misc: 0,
        total: if proficient { 6 } else { 3 },
        proficient,
        expertise: false,
        half_proficiency: false,
        passive: if proficient { 16 } else { 13 },
        label: Some(name.into()),
    }
}

fn with_skills((c, mut snap): (Character, CharacterSnapshot)) -> (Character, CharacterSnapshot) {
    snap.skills
        .push(homebrew("Linguistics", "linguistics", "intelligence", true));
    snap.skills
        .push(homebrew("Lore (Tides)", "lore (tides)", "wisdom", false));
    (c, snap)
}

#[test]
fn a_homebrew_skill_prints_with_its_own_name_and_field() {
    for (label, character) in [
        ("2014", with_skills(common::wizard_2014())),
        ("2024", with_skills(common::wizard_2024())),
    ] {
        let (c, snap) = character;
        let d = sheet_data(&c, &snap, &Default::default());
        assert_eq!(d.skills.len(), 20, "{label}");
        let lore = d
            .skills
            .iter()
            .find(|s| s.key == "lore__tides_")
            .expect("lore");
        assert_eq!(
            (lore.name.as_str(), lore.ability),
            ("Lore (Tides)", "wis"),
            "{label}"
        );
        let ids: Vec<String> = d.fields().into_iter().map(|f| f.id).collect();
        for id in [
            skill("linguistics"),
            skill_prof("linguistics"),
            skill("lore__tides_"),
        ] {
            assert!(ids.contains(&id), "{label}: {id}");
        }
        let html = render_html(&c, &snap, &SheetOptions::default()).expect("html");
        assert!(
            html.contains("Linguistics") && html.contains("Lore (Tides)"),
            "{label}"
        );
        let pdf = render_pdf(&c, &snap, &SheetOptions::default()).expect("pdf");
        // the check boxes are widgets of their own (the value is the row's text field, as for the eighteen)
        let names = field_names(&Document::load_mem(&pdf).expect("pdf parses"));
        for id in [
            skill_prof("linguistics"),
            skill_exp("linguistics"),
            skill_prof("lore__tides_"),
        ] {
            assert!(names.contains(&id), "{label}: the PDF has {id}");
        }
    }
}
