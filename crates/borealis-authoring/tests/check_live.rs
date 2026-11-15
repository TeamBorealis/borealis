//! The live pipeline (spec 8.1 steps 1-5) over every element of every bundled pack, opened in its creator form with
//! all packs loaded: bundled content must read clean. No element has an error (it would refuse Save on a copy), and
//! the warnings are exactly the reviewed list below (T6: an asserted outcome, not a known-failure list). Every
//! problem is a plain sentence at a form path.

use borealis_authoring::check::{live_problems, LiveInput, ProblemSeverity};
use borealis_authoring::form::{element_to_form, CatalogBlockCodec, FormContext};
use borealis_content::{ContentStore, ContentStoreBuilder};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

/// Warnings bundled content has, as (code, count), with the reason. Reviewed.
const WARNINGS: &[(&str, usize, &str)] = &[];

fn bundled() -> ContentStore {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut dirs: Vec<_> = fs::read_dir(&root)
        .expect("content dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.join("pack.json").exists())
        .collect();
    dirs.sort();
    let mut b = ContentStoreBuilder::new();
    for d in dirs {
        b = b.add_pack_dir(&d).expect("pack loads");
    }
    b.build()
}

#[test]
fn bundled_content_reads_clean_in_the_live_pipeline() {
    let store = bundled();
    let codec = CatalogBlockCodec;
    let cx = FormContext::from_store(&store, &codec);
    let hidden = HashSet::new();
    let mut errors = Vec::new();
    let mut warnings: BTreeMap<String, usize> = BTreeMap::new();
    let mut samples: BTreeMap<String, String> = BTreeMap::new();
    let mut n = 0;
    for stored in store.iter() {
        n += 1;
        let e = &stored.element;
        let form = element_to_form(e, &cx);
        let problems = live_problems(&LiveInput {
            form: &form,
            cx: &cx,
            store: &store,
            file: &stored.file,
            hidden: &hidden,
            pack: &[],
        });
        for p in problems {
            assert!(!p.message.contains("ID_"), "{}: {p:?}", e.id);
            match p.severity {
                ProblemSeverity::Error => errors.push(format!("{} {}: {p:?}", e.id, e.ty)),
                ProblemSeverity::Warning => {
                    *warnings.entry(p.code.clone()).or_default() += 1;
                    samples.entry(p.code.clone()).or_insert_with(|| {
                        format!("{} ({}): {} at {:?}", e.id, e.ty, p.message, p.path)
                    });
                }
                ProblemSeverity::Info => {}
            }
        }
    }
    assert!(n >= 4000, "{n} elements");
    println!("live pipeline: {n} bundled elements, warnings {warnings:?}");
    assert!(
        errors.is_empty(),
        "{} errors, first: {:#?}",
        errors.len(),
        &errors[..errors.len().min(20)]
    );
    let want: BTreeMap<String, usize> = WARNINGS
        .iter()
        .map(|(c, k, _)| ((*c).to_string(), *k))
        .collect();
    assert_eq!(warnings, want, "samples: {samples:#?}");
}
