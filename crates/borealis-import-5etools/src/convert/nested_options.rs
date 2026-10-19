//! A feature whose own entries offer a choice among other features: an `options` block with
//! an explicit `count` whose entries are `refSubclassFeature` / `refClassFeature` (PHB Totem
//! Spirit, Aspect of the Beast and Totemic Attunement offer their animals this way). The
//! feature gets a select of `count` over the option features, by id. An `options` block
//! without a count lists features the character gains all of (TCE Psionic Power), not a
//! choice, and is left alone.

use super::build::select;
use super::feature::{
    class_feature_key, feature_id, parse_class_feature_uid, parse_subclass_feature_uid,
    subclass_feature_key, FeatureRef,
};
use super::{Shared, State};
use borealis_content::Rule;
use serde_json::Value;

/// The option feature an `options` entry references, when that feature was loaded.
fn option_ref(sh: &Shared, entry: &Value, sub: bool) -> Option<FeatureRef> {
    let (ty, field) = if sub {
        ("refSubclassFeature", "subclassFeature")
    } else {
        ("refClassFeature", "classFeature")
    };
    if entry.get("type").and_then(Value::as_str) != Some(ty) {
        return None;
    }
    let uid = entry.get(field).and_then(Value::as_str)?;
    let r = if sub {
        parse_subclass_feature_uid(uid)?
    } else {
        parse_class_feature_uid(uid)?
    };
    let loaded = if sub {
        sh.subclass_feature_keys
            .contains_key(&subclass_feature_key(&r))
    } else {
        sh.class_feature_keys.contains_key(&class_feature_key(
            &r.name,
            &r.class,
            &r.class_source,
            r.level,
            &r.source,
        ))
    };
    loaded.then_some(r)
}

/// The select of the first counted `options` block among `entries` (top level only) that
/// offers at least two loaded features, typed `ty` (the option features' own type) and named
/// after the feature.
pub fn select_rule(
    sh: &Shared,
    st: &mut State,
    feature: &str,
    entries: &[Value],
    ty: &str,
    sub: bool,
) -> Option<Rule> {
    for e in entries {
        if e.get("type").and_then(Value::as_str) != Some("options") {
            continue;
        }
        let Some(count) = e
            .get("count")
            .and_then(Value::as_u64)
            .and_then(|c| u32::try_from(c).ok())
            .filter(|c| *c > 0)
        else {
            continue;
        };
        let refs: Vec<FeatureRef> = e
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|x| option_ref(sh, x, sub))
            .collect();
        if refs.len() < 2 || refs.len() <= count as usize {
            continue;
        }
        let ids: Vec<String> = refs.iter().map(|r| feature_id(st, r, sub)).collect();
        return Some(Rule::Select(select(
            ty,
            feature,
            Some(&ids.join("|")),
            count,
        )));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn r(name: &str) -> Value {
        json!({"type": "refSubclassFeature",
               "subclassFeature": format!("{name}|Barbarian|ZZZ|Totem|ZZZ|3")})
    }

    #[test]
    fn a_counted_options_block_becomes_a_select_over_the_loaded_features() {
        let mut sh = Shared::default();
        for name in ["Bear", "Eagle", "Wolf"] {
            let uid = format!("{name}|Barbarian|ZZZ|Totem|ZZZ|3");
            let key = subclass_feature_key(&parse_subclass_feature_uid(&uid).unwrap());
            sh.subclass_feature_keys.insert(key, 0);
        }
        let mut st = State::default();
        let entries = vec![
            json!("Choose a totem spirit."),
            json!({"type": "options", "count": 1,
                   "entries": [r("Bear"), r("Eagle"), r("Elk"), r("Wolf")]}),
        ];
        let rule = select_rule(
            &sh,
            &mut st,
            "Totem Spirit",
            &entries,
            "Archetype Feature",
            true,
        );
        let Some(Rule::Select(s)) = rule else {
            panic!("{rule:?}")
        };
        assert_eq!(
            (s.ty.as_str(), s.name.as_str(), s.number),
            ("Archetype Feature", "Totem Spirit", None)
        );
        let ids: Vec<&str> = s.supports.as_deref().unwrap().split('|').collect();
        assert_eq!(ids.len(), 3, "the unloaded Elk is left out: {ids:?}");
        assert!(ids[0].ends_with("_BARBARIAN_TOTEM_L3_BEAR"), "{ids:?}");
        // without a count every listed feature is gained: no choice
        let all = vec![json!({"type": "options", "entries": [r("Bear"), r("Eagle")]})];
        assert!(select_rule(&sh, &mut st, "Aura", &all, "Archetype Feature", true).is_none());
        // class features are not read from subclass references
        assert!(select_rule(
            &sh,
            &mut st,
            "Totem Spirit",
            &entries,
            "Class Feature",
            false
        )
        .is_none());
    }
}
