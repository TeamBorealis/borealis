use super::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("content dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            json_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

/// Every statistic name the bundled packs use, with a count: `stat` rule names, statistic values and maximums (also
/// inside toggle effects), `resource` maxima of type `stat`, `extra_damage.dice_count.stat`,
/// `damage_bonus_spell.damage_type_stat`. Also the loaded class names.
fn bundled_names() -> (BTreeMap<String, usize>, Vec<String>) {
    fn stat_rule(r: &Value, out: &mut BTreeMap<String, usize>) {
        let mut add = |n: &str| {
            let n = n.trim().trim_start_matches('-').to_lowercase();
            if !n.is_empty() && n.parse::<i64>().is_err() {
                *out.entry(n).or_default() += 1;
            }
        };
        if let Some(n) = r["name"].as_str() {
            add(n);
        }
        if r["inline"] != Value::Bool(true) {
            if let Some(v) = r["value"].as_str() {
                add(v);
            }
        }
        if let Some(m) = r["maximum"].as_str() {
            add(m);
        }
    }
    fn walk(v: &Value, out: &mut BTreeMap<String, usize>) {
        match v {
            Value::Object(m) => {
                if let Some(Value::Array(rules)) = m.get("rules") {
                    for r in rules {
                        match r["kind"].as_str() {
                            Some("stat") => stat_rule(r, out),
                            Some("toggle") => {
                                for e in r["effects"].as_array().into_iter().flatten() {
                                    if e["kind"] == "stat" {
                                        stat_rule(e, out);
                                    }
                                }
                            }
                            _ => {}
                        }
                        let named = [
                            (r["kind"] == "resource" && r["max"]["type"] == "stat")
                                .then(|| r["max"]["stat"].as_str()),
                            (r["dice_count"]["type"] == "stat")
                                .then(|| r["dice_count"]["stat"].as_str()),
                            Some(r["damage_type_stat"].as_str()),
                        ];
                        for n in named.into_iter().flatten().flatten() {
                            *out.entry(n.to_lowercase()).or_default() += 1;
                        }
                    }
                }
                m.values().for_each(|x| walk(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut files = Vec::new();
    json_files(&root, &mut files);
    let mut out = BTreeMap::new();
    let mut classes = Vec::new();
    for f in files {
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(&f).expect("read")).expect("json");
        walk(&doc, &mut out);
        for e in doc["elements"].as_array().into_iter().flatten() {
            if e["type"] == "Class" {
                if let Some(n) = e["name"].as_str() {
                    classes.push(n.to_lowercase());
                }
            }
        }
    }
    assert!(out.len() > 200, "the bundled packs are there");
    (out, classes)
}

#[test]
fn every_statistic_the_bundled_packs_use_is_in_the_catalog() {
    let (names, classes) = bundled_names();
    let mut missing = Vec::new();
    let mut scoped = 0;
    for n in names.keys() {
        match find(n) {
            None => missing.push(n.clone()),
            Some(m) => {
                if let Some(c) = &m.class {
                    scoped += 1;
                    assert!(classes.contains(c), "{n}: {c} is no bundled class");
                }
            }
        }
    }
    let entries: usize = catalog().iter().map(|g| g.stats.len()).sum();
    println!(
        "{} bundled statistic names ({scoped} class-scoped), {entries} catalog entries in {} groups",
        names.len(),
        catalog().len()
    );
    assert!(
        missing.is_empty(),
        "statistics with no catalog entry: {missing:?}"
    );
}

#[test]
fn curated_entries_are_used_by_bundled_content() {
    let (names, _) = bundled_names();
    let unused: Vec<&str> = table::CURATED
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| !names.contains_key(*n))
        .collect();
    assert!(
        unused.is_empty(),
        "curated entries no bundled pack uses: {unused:?}"
    );
    let groups: Vec<&str> = catalog().iter().map(|g| g.id.as_str()).collect();
    for (n, _, g) in table::CURATED {
        assert!(groups.contains(g), "{n}: group {g}");
    }
}

#[test]
fn names_are_unique_and_labels_plain() {
    let mut seen = BTreeSet::new();
    for g in catalog() {
        assert!(!g.stats.is_empty(), "{}", g.id);
        for e in &g.stats {
            assert!(seen.insert(e.name.clone()), "{} twice", e.name);
            assert!(
                !e.label.is_empty() && !e.label.contains([':', '_', '{']),
                "{}: {}",
                e.name,
                e.label
            );
            assert_eq!(
                e.class_scoped,
                e.name.contains(CLASS) || e.name.contains(NUMBER),
                "{}",
                e.name
            );
            assert_eq!(e.name, e.name.to_lowercase(), "rule names are lower case");
        }
    }
}

#[test]
fn class_scoped_names_match_patterns() {
    let m = find("paladin:spellcasting:slots:3").expect("slots");
    assert_eq!(m.entry.name, "{class}:spellcasting:slots:{n}");
    assert_eq!((m.class.as_deref(), m.number), (Some("paladin"), Some(3)));
    assert_eq!(
        label("paladin:spellcasting:slots:3"),
        "Spell slots of level 3 (Paladin)"
    );
    assert_eq!(label("level:cleric"), "Class level (Cleric)");
    assert_eq!(
        label("spell save dc:sorcerer"),
        "Spell save DC of the class's spells (Sorcerer)"
    );
    assert_eq!(
        find("multiclass:spellcasting:level").map(|m| m.class),
        Some(None)
    );
    assert_eq!(
        label("multiclass:spellcasting:slot:4"),
        "Multiclass spell slots of level 4"
    );
    assert!(
        find("paladin:spellcasting:slots:12").is_none(),
        "slot levels are 1-9"
    );
    assert!(
        find("Darkvision:Range").is_none(),
        "rule names are lower case"
    );
}

#[test]
fn labels_and_the_other_group() {
    assert_eq!(label("darkvision:range"), "Darkvision range");
    assert_eq!(label("-dexterity:modifier"), "minus Dexterity modifier");
    assert_eq!(label("innate speed:fly"), "Base speed, flying");
    assert_eq!(label("my homebrew:counter"), "My homebrew counter");
    let groups = with_found([
        "darkvision:range",
        "my homebrew:counter",
        "2",
        "",
        "Shouty",
        "my homebrew:counter",
    ]);
    let other = groups.last().expect("groups");
    assert_eq!(other.id, OTHER_GROUP);
    assert_eq!(other.stats.len(), 1);
    assert_eq!(other.stats[0].name, "my homebrew:counter");
    assert_eq!(with_found(["darkvision:range"]).len(), catalog().len());
}
