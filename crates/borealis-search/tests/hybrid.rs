//! Hybrid fusion onto `Hit` rows (`--features embed`).
#![cfg(feature = "embed")]

use borealis_search::hybrid::{fuse_hits, is_exact_name};
use borealis_search::{Hit, Passage};

fn hit(key: &str, title: &str) -> Hit {
    Hit {
        doc_key: key.into(),
        title: title.into(),
        path: "Spell".into(),
        ty: "Spell".into(),
        ruleset: "2014".into(),
        pack: "srd-5.1".into(),
        source: "SRD".into(),
        snippet: format!("{title} keyword snippet"),
        highlights: vec![(0, 3)],
        score: 9.0,
    }
}

fn passage(key: &str) -> Option<Passage> {
    let (title, ruleset) = match key {
        "ID_FLAME_STRIKE" => ("Flame Strike", "2014"),
        "ID_FLAME_STRIKE_2024" => ("Flame Strike", "2024"),
        _ => return None,
    };
    Some(Passage {
        doc_key: key.into(),
        title: title.into(),
        path: "Spell".into(),
        ty: "Spell".into(),
        ruleset: ruleset.into(),
        pack: "srd".into(),
        source: "SRD".into(),
        campaign: None,
        body: "A vertical column of divine fire roars down. Each creature takes fire damage."
            .into(),
    })
}

#[test]
fn exact_name_pinned_vector_only_hits_get_snippets_and_filters() {
    let keyword = vec![
        hit("ID_FIRE_BOLT", "Fire Bolt"),
        hit("ID_FIREBALL", "Fireball"),
    ];
    let vector = [
        ("ID_FLAME_STRIKE", 0.9),
        ("ID_FLAME_STRIKE_2024", 0.88),
        ("ID_FIRE_BOLT", 0.8),
        ("ID_GONE", 0.7),
    ];
    let out = fuse_hits(
        "fireball",
        keyword,
        &vector,
        10,
        |k| Ok(passage(k)),
        |p| p.ruleset == "2014",
    )
    .expect("fuse");
    let keys: Vec<&str> = out.iter().map(|h| h.doc_key.as_str()).collect();
    // Fireball is the exact name: pinned first though Fire Bolt is ahead on both lists.
    assert_eq!(keys, ["ID_FIREBALL", "ID_FIRE_BOLT", "ID_FLAME_STRIKE"]);
    assert!(out[0].score > 0.0 && out[0].score < 1.0, "RRF score");
    assert_eq!(out[1].snippet, "Fire Bolt keyword snippet");
    // No sentence shares a query word ("fireball"), so the snippet is the start of the text.
    assert_eq!(
        out[2].snippet,
        "A vertical column of divine fire roars down. Each creature takes fire damage."
    );
    assert!(out[2].highlights.is_empty());
}

#[test]
fn exact_name_uses_name_normalisation_and_limit_applies() {
    assert!(is_exact_name(
        &hit("k", "Tasha's Hideous Laughter"),
        "tashas hideous laughter"
    ));
    assert!(!is_exact_name(&hit("k", "Fireball"), "fire"));
    let out = fuse_hits(
        "x",
        vec![hit("a", "A"), hit("b", "B")],
        &[],
        1,
        |_| Ok(None),
        |_| true,
    )
    .expect("fuse");
    assert_eq!(out.len(), 1);
}
