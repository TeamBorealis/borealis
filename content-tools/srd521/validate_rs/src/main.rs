//! Loads core + srd-5.2.1 packs with borealis-content and prints the semantic validation report.
use borealis_content::{ContentStoreBuilder, Severity};
use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let content = args.get(1).map(String::as_str).unwrap_or("../../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.2.1"] {
        let dir = Path::new(content).join(pack);
        if !dir.join("pack.json").exists() {
            eprintln!("skipping missing pack {pack}");
            continue;
        }
        b = b.add_pack_dir(&dir).unwrap_or_else(|e| panic!("load {pack}: {e}"));
    }
    let store = b.build();
    println!("elements: {}", store.len());
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in store.iter() {
        *counts.entry(format!("{:?}", e.element.ty)).or_default() += 1;
    }
    println!("{counts:?}");
    let mut issues: Vec<_> = store.diagnostics().to_vec();
    issues.extend(store.validate());
    let mut by: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for i in &issues {
        let sev = match i.severity { Severity::Error => "error", Severity::Warning => "warning", Severity::Info => "info" };
        by.entry((sev.to_string(), format!("{:?}", i.code))).or_default().push(i.message.clone());
    }
    for ((sev, code), msgs) in &by {
        println!("{sev} {code}: {}", msgs.len());
        for m in msgs.iter().take(6) { println!("    {m}"); }
    }
}
