//! Text/PDF import into a user-import pack.

mod common;

use borealis_ai::task::RunOptions;
use borealis_assist::import::extract_pdf_text;
use borealis_assist::{import_text_to_pack, AssistError, ChunkStatus, ImportRequest, ImportSource};
use borealis_content::ContentStoreBuilder;
use common::{pack_ctx, FakeRunner};
use serde_json::json;

const DOC: &str = "# The Gloamwood Supplement\n\nA swamp set.\n\n\
Ember Ward\n2nd-level abjuration\nCasting Time: 1 action\nRange: Self\nDuration: 1 hour\n\
A shimmering ward of embers surrounds you. Creatures that hit you with a melee attack take 1d6 fire damage.\n\n\
Marsh Sense\nPrerequisite: Wisdom 13 or higher\n\
You sense creatures moving in mud and shallow water. Increase your Wisdom score by 1, to a maximum of 20.\n\n\
Lantern of the Drowned\nWondrous item, rare (requires attunement)\n\
This rusted lantern burns with a pale green flame and sheds dim light in a 30-foot radius.\n";

fn reply(elements: serde_json::Value) -> String {
    json!({"content":{"format":"borealis-content/1","elements":elements},"notes":"","warnings":[]})
        .to_string()
}

fn spell() -> serde_json::Value {
    json!([{"id":"ID_X_EMBER","name":"Ember Ward","type":"Spell","source":"Whatever","supports":["Wizard"],
        "description":"<p>Fire ward.</p>","setters":[{"name":"level","value":"2"},{"name":"school","value":"Abjuration"},{"name":"time","value":"1 action"},{"name":"duration","value":"1 hour"},{"name":"range","value":"Self"}]}])
}

fn feat() -> serde_json::Value {
    json!([{"id":"ID_X_MARSH","name":"Marsh Sense","type":"Feat","source":"Whatever","prerequisite":"Wisdom 13 or higher",
        "description":"<p>Sense mud.</p>","rules":[{"kind":"stat","name":"wisdom","value":"1"}]}])
}

fn req(source: ImportSource) -> ImportRequest {
    ImportRequest {
        source,
        id_prefix: "gloom".into(),
        source_name: "Gloamwood".into(),
        pack_id: "gloamwood-import".into(),
        pack_name: "Gloamwood (import)".into(),
    }
}

fn one_repair() -> RunOptions<'static> {
    RunOptions {
        max_repairs: 1,
        sink: None,
    }
}

#[tokio::test]
async fn text_import_converts_repairs_and_reports_failures() {
    let missing_id = reply(json!([{"name":"Marsh Sense","type":"Feat","source":"Whatever"}]));
    let dangling = reply(
        json!([{"id":"ID_X_LANTERN","name":"Lantern of the Drowned","type":"Magic Item","source":"W",
        "rules":[{"kind":"grant","id":"ID_NO_SUCH_THING","type":"Proficiency"}]}]),
    );
    let runner = FakeRunner::new(&[
        &reply(spell()),
        &missing_id,
        &reply(feat()),
        &dangling,
        &dangling,
    ]);
    let out = import_text_to_pack(
        &runner,
        &pack_ctx(),
        &req(ImportSource::Text(DOC.into())),
        &one_repair(),
    )
    .await
    .unwrap();

    let status: Vec<ChunkStatus> = out.chunks.iter().map(|c| c.status).collect();
    assert_eq!(
        status,
        vec![
            ChunkStatus::Skipped,
            ChunkStatus::Converted,
            ChunkStatus::Converted,
            ChunkStatus::Failed
        ],
        "{:#?}",
        out.chunks
    );
    assert_eq!(
        out.chunks[2].attempts, 2,
        "malformed chunk was repaired once"
    );
    assert!(
        out.chunks[3].issues[0].contains("ID_NO_SUCH_THING")
            || out.chunks[3].issues[0].contains("failed")
    );
    assert_eq!(runner.calls(), 5);

    let pack = out.pack.expect("pack");
    let ids: Vec<&str> = pack
        .content
        .elements
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["ID_GLOOM_SPELL_EMBER_WARD", "ID_GLOOM_FEAT_MARSH_SENSE"]
    );
    assert!(pack
        .content
        .elements
        .iter()
        .all(|e| e.source == "Gloamwood"));
    assert_eq!(pack.provenance.origin, "user-import");
    assert!(!pack.provenance.redistributable);

    let dir = std::env::temp_dir().join(format!("borealis-import-test-{}", std::process::id()));
    pack.write_to_dir(&dir).unwrap();
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&dir)
        .unwrap()
        .build();
    assert!(store.contains_id("ID_GLOOM_SPELL_EMBER_WARD"));
    let prov = std::fs::read_to_string(dir.join("import.json")).unwrap();
    assert!(prov.contains("\"redistributable\": false"));
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn wrong_element_type_is_marked_for_review() {
    let runner = FakeRunner::new(&[&reply(feat())]);
    let doc = "Ember Ward\n2nd-level abjuration\nCasting Time: 1 action\nA ward of embers surrounds you for one hour.\n";
    let out = import_text_to_pack(
        &runner,
        &pack_ctx(),
        &req(ImportSource::Text(doc.into())),
        &one_repair(),
    )
    .await
    .unwrap();
    assert_eq!(out.chunks[0].status, ChunkStatus::NeedsReview);
    assert!(out.chunks[0].issues[0].contains("Spell"));
}

/// A minimal one-page PDF with Helvetica text, one line per entry.
fn tiny_pdf(lines: &[&str]) -> Vec<u8> {
    let mut stream = String::from("BT /F1 12 Tf 72 720 Td 16 TL\n");
    for l in lines {
        let esc = l
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        stream.push_str(&format!("({esc}) Tj T*\n"));
    }
    stream.push_str("ET");
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .bytes(),
    );
    out
}

#[test]
fn tiny_generated_pdf_extracts_its_text() {
    let pdf = tiny_pdf(&[
        "Ember Ward",
        "2nd-level abjuration",
        "Casting Time: 1 action",
        "Embers surround you.",
    ]);
    let t = extract_pdf_text(&pdf).unwrap();
    assert_eq!(t.pages, 1);
    assert!(t.text.contains("Ember Ward"), "{:?}", t.text);
    assert!(t.text.contains("Casting Time: 1 action"), "{:?}", t.text);
}

#[test]
fn pdf_without_a_text_layer_is_reported_clearly() {
    let err = extract_pdf_text(&tiny_pdf(&["x"])).unwrap_err();
    assert!(matches!(err, AssistError::Extraction(_)));
    assert!(err.to_string().contains("OCR"), "{err}");
    assert!(extract_pdf_text(b"not a pdf").is_err());
}

#[tokio::test]
async fn pdf_import_runs_the_same_pipeline() {
    let pdf = tiny_pdf(&[
        "Ember Ward",
        "2nd-level abjuration",
        "Casting Time: 1 action",
        "A ward of embers surrounds you for one hour.",
    ]);
    let runner = FakeRunner::new(&[&reply(spell())]);
    let out = import_text_to_pack(
        &runner,
        &pack_ctx(),
        &req(ImportSource::Pdf(pdf)),
        &one_repair(),
    )
    .await
    .unwrap();
    assert_eq!(out.extraction.source_kind, "pdf");
    assert_eq!(out.extraction.pages, Some(1));
    assert_eq!(out.count(ChunkStatus::Converted), 1);
    assert_eq!(out.pack.unwrap().content.elements.len(), 1);
}
