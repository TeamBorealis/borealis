//! Offline replay of a stored draft (no model): `BOREALIS_FINALIZE_REPLAY=<draft.json>
//! BOREALIS_FINALIZE_OUT=<dir> cargo test -p borealis-assist --lib -- --ignored
//! refinalize_a_stored_draft --nocapture` re-finalizes the draft with the current code into
//! `<dir>` (judge it like an imported pack), and prints the plan elements a rebuilt plan would
//! add and the mechanics the post-generation check would send to repair.

use std::path::Path;

use borealis_content::ContentStoreBuilder;

#[test]
#[ignore = "needs BOREALIS_FINALIZE_REPLAY (a stored draft.json)"]
fn refinalize_a_stored_draft() {
    let Ok(path) = std::env::var("BOREALIS_FINALIZE_REPLAY") else {
        return;
    };
    let text = std::fs::read_to_string(&path).expect("draft reads");
    let draft: super::types::ImportDraft = serde_json::from_str(&text).expect("draft parses");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    let store = b.clone().build();
    let mut plan = draft.plan.clone();
    super::plan::rebuild(&mut plan, &store, &draft.id_prefix, &draft.source_text);
    for e in plan
        .elements
        .iter()
        .filter(|e| !draft.plan.elements.iter().any(|o| o.name == e.name))
    {
        println!("plan adds: {} ({}, {:?})", e.name, e.ty, e.role);
    }
    for o in &draft.detection.other {
        let g = super::plan::text_group(&o.name, &o.what, &o.note);
        if g.kind == super::types::ImportKind::Condition {
            println!("condition element: {}", o.name);
        }
    }
    let flags = super::mechanics::sweep(&draft);
    println!("mechanics findings: {}", flags.len());
    for f in &flags {
        println!(
            "  {}: {}",
            f.element_id.as_deref().unwrap_or("-"),
            f.message.chars().take(140).collect::<String>()
        );
    }
    let done = super::finalize::finalize_draft(&b, &draft).expect("finalizes");
    if let Some(r) = done.pack.report.as_ref() {
        for n in r.normalised.iter().filter(|n| {
            n.message.contains("spell choice per tier")
                || n.message.contains("already proficient")
                || n.message.contains("attunement")
        }) {
            println!("fixed: {} {}", n.element, n.message);
        }
    }
    if let Ok(out) = std::env::var("BOREALIS_FINALIZE_OUT") {
        done.pack
            .write_to_dir(Path::new(&out))
            .expect("pack writes");
        println!("written to {out}");
    }
}
