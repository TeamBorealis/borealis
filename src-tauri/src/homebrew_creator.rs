//! The homebrew creator's backend (1.5.0; `specs/homebrew-creator-spec.md`). The commands in `commands_homebrew.rs`
//! delegate here. Until each command's work item lands (spec section 21), it answers [`not_implemented`]; the UI
//! shows the creator only behind `AppSettings::creator_enabled` (HC3), so 1.4.x builds never reach it.

use crate::error::{ApiError, ErrorCode};
use crate::homebrew_dto::{
    HomebrewBlockField, HomebrewBlockSpec, HomebrewBlockVariant, HomebrewChoice, HomebrewFieldSpec,
    HomebrewPickerTarget, HomebrewStatEntry, HomebrewStatGroup,
};
use borealis_authoring::blocks::{self, BlockField, BlockSpec, Choice, Control};
use borealis_authoring::stats::{self, StatGroup};
use borealis_content::ContentStore;

/// The answer of a creator command whose work item has not landed: `requirement_failed` with
/// `details.notImplemented` = the command name (`docs/ui-api-contract.md`, Homebrew creator).
#[must_use]
pub fn not_implemented(command: &str) -> ApiError {
    let mut e = ApiError::new(
        ErrorCode::RequirementFailed,
        format!("The homebrew creator can't do this yet ({command} is not implemented)."),
    );
    e.details = Some(serde_json::json!({ "notImplemented": command }));
    e
}

/// Block dialogs sit on the RULES sub-tab of every form.
const RULES_TAB: &str = "rules";

fn choices(c: &[Choice]) -> Option<Vec<HomebrewChoice>> {
    (!c.is_empty()).then(|| {
        c.iter()
            .map(|x| HomebrewChoice {
                value: x.value.to_string(),
                label: x.label.to_string(),
            })
            .collect()
    })
}

fn picker(types: &[&str], multiple: bool) -> HomebrewPickerTarget {
    HomebrewPickerTarget {
        types: types.iter().map(|t| (*t).to_string()).collect(),
        tags: None,
        multiple: multiple.then_some(true),
    }
}

fn wire(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

fn block_field(f: &BlockField, path: &str) -> HomebrewBlockField {
    let path = if path.is_empty() {
        f.key.to_string()
    } else {
        format!("{path}.{}", f.key)
    };
    let mut base = HomebrewFieldSpec {
        key: f.key.to_string(),
        label: f.label.to_string(),
        tab: RULES_TAB.to_string(),
        section: None,
        control: f.control.wire().to_string(),
        required: f.required,
        editions: None,
        choices: None,
        allow_custom: None,
        min: None,
        max: None,
        picker: None,
        bindings: vec![path.clone()],
        help: if f.note {
            Some(blocks::NOTE_HELP.to_string())
        } else {
            f.help.map(str::to_string)
        },
        expert: f.expert.then_some(true),
    };
    let mut out = HomebrewBlockField {
        base: base.clone(),
        dialect: None,
        shown_as_note: f.note.then_some(true),
        scope: None,
        multiple: None,
        fields: None,
        variants: None,
        kinds: None,
    };
    match f.control {
        Control::Number { min, max } | Control::Numbers { min, max } => {
            base.min = Some(f64::from(min));
            base.max = Some(f64::from(max));
        }
        Control::NumberOr {
            min,
            max,
            words,
            custom,
        } => {
            base.min = Some(f64::from(min));
            base.max = Some(f64::from(max));
            base.choices = choices(words);
            base.allow_custom = custom.then_some(true);
        }
        Control::Combo { choices: c, custom } | Control::Chips { choices: c, custom } => {
            base.choices = choices(c);
            base.allow_custom = custom.then_some(true);
        }
        // the eighteen, then the names of the skills loaded content adds (HC16)
        Control::Skills { choices: c, .. } => {
            base.choices = choices(c);
            base.picker = Some(picker(&[blocks::SKILL_NAME_PICKER], false));
        }
        Control::Reference { types } => base.picker = Some(picker(types, false)),
        Control::References { types } => base.picker = Some(picker(types, true)),
        Control::Name { scope, multiple } => {
            out.scope = wire(&scope);
            out.multiple = multiple.then_some(true);
        }
        Control::Formula(d) | Control::Formulas(d) => out.dialect = wire(&d),
        Control::Group(parts) | Control::Rows(parts) => {
            out.fields = Some(parts.iter().map(|p| block_field(p, &path)).collect());
        }
        Control::Map(kv) => {
            out.fields = Some(kv.iter().map(|p| block_field(p, &path)).collect());
        }
        Control::Variant(variants) => {
            out.variants = Some(
                variants
                    .iter()
                    .map(|v| HomebrewBlockVariant {
                        value: v.value.to_string(),
                        label: v.label.to_string(),
                        fields: v.fields.iter().map(|p| block_field(p, &path)).collect(),
                    })
                    .collect(),
            );
        }
        Control::Blocks(kinds) => {
            out.kinds = Some(kinds.iter().map(|k| (*k).to_string()).collect())
        }
        _ => {}
    }
    out.base = base;
    out
}

fn block_spec(b: &BlockSpec) -> HomebrewBlockSpec {
    HomebrewBlockSpec {
        kind: b.kind.to_string(),
        label: b.label.to_string(),
        group: b.group.id().to_string(),
        group_label: b.group.label().to_string(),
        common: b.common,
        core_only: b.core_only(),
        effect_only: b.effect_only(),
        level_gated: b.level_gated,
        only_if: b.only_if,
        fields: b.fields.iter().map(|f| block_field(f, "")).collect(),
    }
}

/// `homebrew_block_catalog` (spec 6.1, 6.2): every block, in Add menu order.
#[must_use]
pub fn block_catalog() -> Vec<HomebrewBlockSpec> {
    blocks::catalog().iter().map(block_spec).collect()
}

fn stat_group(g: &StatGroup) -> HomebrewStatGroup {
    HomebrewStatGroup {
        id: g.id.clone(),
        label: g.label.clone(),
        stats: g
            .stats
            .iter()
            .map(|e| HomebrewStatEntry {
                name: e.name.clone(),
                label: e.label.clone(),
                class_scoped: e.class_scoped.then_some(true),
            })
            .collect(),
    }
}

/// `homebrew_stat_catalog` (spec 5.2): the catalog, plus "Other statistics your content uses" for names the loaded
/// packs use that the catalog doesn't know.
#[must_use]
pub fn stat_catalog(store: Option<&ContentStore>) -> Vec<HomebrewStatGroup> {
    let found = store
        .map(|s| stats::used_names(s.iter().flat_map(|e| e.element.rules.iter())))
        .unwrap_or_default();
    stats::with_found(found.iter().map(String::as_str))
        .iter()
        .map(stat_group)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_implemented_names_the_command_in_details() {
        let e = not_implemented("homebrew_kinds");
        assert_eq!(e.code, ErrorCode::RequirementFailed);
        assert!(e.message.contains("homebrew_kinds"), "{}", e.message);
        assert_eq!(
            e.details,
            Some(serde_json::json!({ "notImplemented": "homebrew_kinds" }))
        );
    }

    #[test]
    fn block_fields_carry_their_paths_and_control_details() {
        let catalog = block_catalog();
        assert_eq!(catalog.len(), blocks::catalog().len());
        let resource = catalog
            .iter()
            .find(|b| b.kind == "resource")
            .expect("resource");
        assert_eq!((resource.group.as_str(), resource.common), ("uses", true));
        let max = resource
            .fields
            .iter()
            .find(|f| f.base.key == "max")
            .expect("max");
        assert_eq!(max.base.control, "variant");
        let formula = max
            .variants
            .as_ref()
            .and_then(|v| v.iter().find(|x| x.value == "formula"))
            .expect("formula variant");
        assert_eq!(
            formula.fields[0].base.bindings,
            vec!["max.expr".to_string()]
        );
        assert_eq!(formula.fields[0].dialect.as_deref(), Some("formula"));
        let save_dc = catalog
            .iter()
            .find(|b| b.kind == "save_dc")
            .expect("save_dc");
        assert_eq!(save_dc.fields[1].dialect.as_deref(), Some("saveDc"));
        let alt = resource
            .fields
            .iter()
            .find(|f| f.base.key == "alt_regain")
            .expect("alt_regain");
        assert_eq!(alt.shown_as_note, Some(true));
        assert_eq!(alt.base.help.as_deref(), Some(blocks::NOTE_HELP));
    }

    #[test]
    fn stat_catalog_adds_names_only_loaded_content_uses() {
        let plain = stat_catalog(None);
        assert_eq!(plain.len(), stats::catalog().len());
        assert!(plain.iter().all(|g| g.id != stats::OTHER_GROUP));
    }
}
