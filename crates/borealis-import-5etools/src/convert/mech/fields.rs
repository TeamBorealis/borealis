//! The proficiency-like fields 5etools states the same way on every entity that grants them
//! (`skillProficiencies`, `toolProficiencies`, ..., `expertise`, `savingThrowProficiencies`,
//! damage relations and `senses`), read by field name from a raw entry. Used where the typed
//! model does not list them (optional features: PHB Beguiling Influence's skills, TCE Blind
//! Fighting's blindsight).

use super::Mech;
use serde_json::Value;

fn list<'v>(raw: &'v Value, key: &str) -> &'v [Value] {
    raw.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

impl Mech<'_> {
    /// Every proficiency, expertise, saving throw, damage relation and sense field of `raw`.
    /// A saving throw choice cannot be stated as a grant and is reported.
    pub fn proficiency_fields(&mut self, raw: &Value) {
        self.skills(list(raw, "skillProficiencies"));
        self.tools(list(raw, "toolProficiencies"));
        self.languages(list(raw, "languageProficiencies"));
        self.weapons(list(raw, "weaponProficiencies"));
        self.armor(list(raw, "armorProficiencies"));
        self.skill_tool_language(list(raw, "skillToolLanguageProficiencies"));
        self.expertise(list(raw, "expertise"));
        for obj in list(raw, "savingThrowProficiencies") {
            for (k, v) in obj.as_object().into_iter().flatten() {
                if v == &Value::Bool(true) {
                    self.saving_throws(std::slice::from_ref(k));
                } else {
                    self.note(
                        "savingThrowProficiencies",
                        "saving throw choice was not converted".into(),
                    );
                }
            }
        }
        self.resistances(
            list(raw, "resist"),
            list(raw, "immune"),
            list(raw, "vulnerable"),
            list(raw, "conditionImmune"),
        );
        self.senses(None, None, list(raw, "senses"));
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::{ids_of, shared};
    use super::*;
    use serde_json::json;

    #[test]
    fn every_field_is_read_by_name() {
        let sh = shared();
        let mut m = Mech::new(&sh, "Beguiling Influence");
        m.proficiency_fields(&json!({
            "skillProficiencies": [{"deception": true, "persuasion": true}],
            "savingThrowProficiencies": [{"wis": true}],
            "senses": [{"blindsight": 10}],
            "toolProficiencies": null
        }));
        let ids = ids_of(&m);
        for id in [
            "grant:ID_PROFICIENCY_SKILL_DECEPTION",
            "grant:ID_PROFICIENCY_SKILL_PERSUASION",
            "grant:ID_PROFICIENCY_SAVINGTHROW_WISDOM",
        ] {
            assert!(ids.iter().any(|i| i == id), "{id} in {ids:?}");
        }
        assert!(ids.iter().any(|i| i.contains("BLINDSIGHT")), "{ids:?}");
        assert!(m.partial.is_empty(), "{:?}", m.partial);
    }

    #[test]
    fn a_saving_throw_choice_is_reported() {
        let sh = shared();
        let mut m = Mech::new(&sh, "X");
        m.proficiency_fields(&json!({"savingThrowProficiencies": [{"choose": {"from": ["str"]}}]}));
        assert!(m.rules.is_empty());
        assert_eq!(m.partial.len(), 1);
        assert!(m.unmapped.contains_key("savingThrowProficiencies"));
    }
}
