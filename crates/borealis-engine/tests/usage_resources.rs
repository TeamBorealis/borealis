//! Limited-use pools from Aurora-format sheet `usage` text (`usage.rs`) and half proficiency on
//! skills, on the bundled core + SRD 5.1 packs (whose classes carry Aurora usage text only).
//! The env-gated `acore_usage_pools` counts the A-core elements whose usage reads as a pool
//! (`BOREALIS_ACORE_DATA` = an imported data directory with `library/*/core`).

mod common;

use borealis_engine::snapshot::ResourceSnapshot;
use borealis_engine::usage::parse_usage;
use borealis_engine::CharacterSnapshot;
use common::{class, Hero, Variant, ACOLYTE, RACE_HUMAN};
use std::path::{Path, PathBuf};

fn res<'a>(snap: &'a CharacterSnapshot, name: &str) -> Option<&'a ResourceSnapshot> {
    snap.resources.iter().find(|r| r.name == name)
}

fn hero(class_name: &str, level: u32) -> Option<CharacterSnapshot> {
    let e = common::engine(Variant::Bundled)?;
    let mut h = Hero::new(e, true);
    h.base([15, 14, 13, 12, 10, 16])
        .race_class(RACE_HUMAN, &class(class_name))
        .background(ACOLYTE)
        .level_to(level);
    Some(h.snap())
}

#[test]
fn rage_count_follows_the_class_table_and_recharges_on_a_long_rest() {
    let Some(snap) = hero("Barbarian", 5) else {
        return;
    };
    let r = res(&snap, "Rage").expect("Rage pool");
    assert_eq!((r.max, r.current), (3, 3));
    assert_eq!(r.recharge, vec!["long_rest".to_string()]);
    assert_eq!(r.short_rest_regain, None);
    assert!(r.long_rest_regain_all);
    assert_eq!(r.source, "Rage");
}

#[test]
fn per_level_usage_replaces_the_default_and_short_rests_refill() {
    let Some(snap) = hero("Fighter", 2) else {
        return;
    };
    for name in ["Second Wind", "Action Surge"] {
        let r = res(&snap, name).unwrap_or_else(|| panic!("{name} pool"));
        assert_eq!(r.max, 1, "{name}");
        assert_eq!(r.short_rest_regain, Some(1), "{name}");
    }
    let Some(snap) = hero("Fighter", 17) else {
        return;
    };
    assert_eq!(res(&snap, "Action Surge").map(|r| r.max), Some(2));
}

#[test]
fn placeholder_pools_take_the_unit_name_or_the_feature_name() {
    if let Some(snap) = hero("Sorcerer", 3) {
        let r = res(&snap, "Sorcery Points").expect("Sorcery Points pool");
        assert_eq!(r.max, 3);
        assert_eq!(r.source, "Font of Magic");
    }
    if let Some(snap) = hero("Paladin", 2) {
        assert_eq!(res(&snap, "Lay on Hands").map(|r| r.max), Some(10));
        // 1 + Cha modifier (+3)
        assert_eq!(res(&snap, "Divine Sense").map(|r| r.max), Some(4));
    }
    if let Some(snap) = hero("Monk", 2) {
        assert_eq!(res(&snap, "Ki").map(|r| r.max), Some(2));
        // "1 ki point" is a cost, not a pool
        assert!(res(&snap, "Flurry of Blows").is_none());
    }
}

#[test]
fn unlimited_usage_at_a_higher_level_removes_the_pool() {
    if let Some(snap) = hero("Barbarian", 20) {
        assert!(res(&snap, "Rage").is_none(), "Rage is Unlimited at 20");
    }
}

#[test]
fn a_spent_usage_pool_is_restored_by_a_long_rest() {
    let Some(e) = common::engine(Variant::Bundled) else {
        return;
    };
    let mut h = Hero::new(e, true);
    h.base([15, 14, 13, 12, 10, 16])
        .race_class(RACE_HUMAN, &class("Barbarian"))
        .level_to(3);
    let id = res(&h.snap(), "Rage").expect("Rage").id.clone();
    let spent = borealis_engine::commands::spend_resource(e, &h.s, &id, 2)
        .expect("spend")
        .state;
    assert_eq!(res(&e.derive(&spent), "Rage").map(|r| r.current), Some(1));
    let rested = borealis_engine::commands::long_rest(e, &spent)
        .expect("rest")
        .state;
    assert_eq!(res(&e.derive(&rested), "Rage").map(|r| r.current), Some(3));
}

#[test]
fn jack_of_all_trades_is_half_proficiency() {
    let Some(snap) = hero("Bard", 2) else {
        return;
    };
    let acro = snap.skills.iter().find(|s| s.name == "acrobatics").unwrap();
    assert!(!acro.proficient && acro.half_proficiency);
    let Some(snap) = hero("Bard", 1) else {
        return;
    };
    assert!(snap.skills.iter().all(|s| !s.half_proficiency));
}

/// Placeholders replaced by 1 (a static reading: is the text a pool once resolved?).
fn static_usage(u: &str) -> (String, Vec<bool>) {
    let mut out = String::new();
    let mut dynamic = vec![false];
    let mut at_start = true;
    let mut rest = u;
    while let Some(ch) = rest.chars().next() {
        if rest.starts_with("{{") || rest.starts_with("$(") {
            let close = if rest.starts_with("{{") { "}}" } else { ")" };
            if let Some(end) = rest.find(close) {
                if at_start {
                    *dynamic.last_mut().unwrap() = true;
                }
                at_start = false;
                out.push('1');
                rest = &rest[end + close.len()..];
                continue;
            }
        }
        if ch == '—' || ch == '–' {
            dynamic.push(false);
            at_start = true;
        } else if !ch.is_whitespace() {
            at_start = false;
        }
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    (out, dynamic)
}

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            json_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

#[test]
fn acore_usage_pools() {
    let Some(data) = std::env::var_os("BOREALIS_ACORE_DATA").map(PathBuf::from) else {
        return;
    };
    let mut files = Vec::new();
    json_files(&data.join("library"), &mut files);
    let (mut with_usage, mut explicit, mut pools) = (0, 0, 0);
    let mut missed: std::collections::BTreeMap<String, usize> = Default::default();
    for f in files {
        let Ok(v) =
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&f).unwrap())
        else {
            continue;
        };
        for el in v["elements"].as_array().into_iter().flatten() {
            let sheet = &el["sheet"];
            let mut usages: Vec<&str> = sheet["usage"].as_str().into_iter().collect();
            for d in sheet["descriptions"].as_array().into_iter().flatten() {
                usages.extend(d["usage"].as_str());
            }
            usages.retain(|u| !u.trim().is_empty());
            if usages.is_empty() {
                continue;
            }
            with_usage += 1;
            let has_rule = el["rules"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["kind"] == "resource");
            explicit += usize::from(has_rule);
            let hit = usages.iter().any(|u| {
                let (t, d) = static_usage(u);
                parse_usage(&t, &d).is_some()
            });
            if hit {
                pools += 1;
            } else {
                for u in usages {
                    *missed.entry(u.to_string()).or_default() += 1;
                }
            }
        }
    }
    eprintln!("METRIC\tusage\tacore\telements_with_usage\t{with_usage}");
    eprintln!("METRIC\tusage\tacore\tbefore_explicit_resource\t{explicit}");
    eprintln!("METRIC\tusage\tacore\tafter_usage_pool\t{pools}");
    for (u, n) in &missed {
        eprintln!("NOPOOL\t{n}\t{u}");
    }
    assert!(
        with_usage > 0,
        "no A-core elements under {}",
        data.display()
    );
}

#[test]
fn static_usage_reading() {
    assert_eq!(
        static_usage("{{a:b}} Sorcery Points"),
        ("1 Sorcery Points".to_string(), vec![true])
    );
    assert_eq!(
        static_usage("1/Turn — {{x}}/Long Rest"),
        ("1/Turn — 1/Long Rest".to_string(), vec![false, true])
    );
}

#[test]
fn a_2014_monk_has_an_unarmed_strike_row() {
    let Some(snap) = hero("Monk", 1) else {
        return;
    };
    assert!(snap.attacks.iter().any(|a| a.name == "Unarmed Strike"));
    let Some(snap) = hero("Fighter", 1) else {
        return;
    };
    assert!(!snap.attacks.iter().any(|a| a.name == "Unarmed Strike"));
}

#[test]
fn improved_divine_smite_adds_its_dice_to_melee_lines() {
    let Some(e) = common::engine(Variant::Bundled) else {
        return;
    };
    let mut h = Hero::new(e, true);
    h.base([16, 10, 14, 8, 10, 16])
        .race_class(RACE_HUMAN, &class("Paladin"))
        .level_to(11);
    h.equip("ID_WOTC_PHB_WEAPON_LONGSWORD");
    let snap = h.snap();
    let sword = snap
        .attacks
        .iter()
        .find(|a| a.name == "Longsword")
        .expect("Longsword row");
    assert!(sword.damage.ends_with("+ 1d8 radiant"), "{}", sword.damage);
}
