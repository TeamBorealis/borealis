//! The engine checks (spec 8.1 steps 6-7) on copies of the 2024 SRD classes and subclasses, opened in their creator
//! form with their features inside: the SRD itself must check clean (no test-character refusal, no sweep finding, no
//! choice that offers nothing), so a finding on a user's class means something. Exceptions would be listed with their
//! reason (T6: an asserted outcome); there are none.

use borealis_authoring::check::{deep_problems, DeepInput, ProblemSeverity};
use borealis_authoring::form::{element_group_to_form, CatalogBlockCodec, FormContext};
use borealis_authoring::pack::granted_children;
use borealis_content::{ContentStoreBuilder, Element, Ruleset};
use std::path::Path;

/// (element id, problem code) the SRD has, with the reason. Reviewed: none.
const EXCEPTIONS: &[(&str, &str, &str)] = &[];

#[test]
fn srd_2024_classes_and_subclasses_check_clean() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut base = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.2.1"] {
        base = base.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    let store = base.clone().build();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let all: Vec<Element> = store.iter().map(|s| s.element.as_ref().clone()).collect();
    let roots: Vec<&Element> = all
        .iter()
        .filter(|e| {
            matches!(e.ty.as_str(), "Class" | "Archetype") && e.ruleset == Some(Ruleset::Dnd2024)
        })
        .collect();
    assert!(roots.len() >= 24, "{} classes and subclasses", roots.len());
    let mut found = Vec::new();
    for e in roots {
        let children = granted_children(e, &all);
        let form = element_group_to_form(e, &children, &cx);
        for p in deep_problems(&DeepInput {
            base: &base,
            ruleset: Ruleset::Dnd2024,
            pack_name: "copy",
            form: &form,
            written: None,
            cx: &cx,
            loaded: None,
        }) {
            assert!(!p.message.contains("ID_"), "{p:?}");
            if p.severity != ProblemSeverity::Info
                && !EXCEPTIONS
                    .iter()
                    .any(|(id, code, _)| *id == e.id && *code == p.code)
            {
                found.push(format!(
                    "{} ({}): {} [{}] at {:?}",
                    e.name, e.id, p.message, p.code, p.path
                ));
            }
        }
    }
    assert!(found.is_empty(), "{} findings: {found:#?}", found.len());
}
