use serde_json::json;

use super::*;
use crate::import::anything::types::{ImportPlan, PlannedElement, SourceExcerpt};

/// A planned element located at `line` (1-based; `None` = unlocated).
pub(crate) fn planned(
    id: &str,
    name: &str,
    ty: &str,
    role: Role,
    line: Option<usize>,
) -> PlannedElement {
    PlannedElement {
        id: id.into(),
        ty: ty.into(),
        name: name.into(),
        role,
        level: None,
        owner: None,
        brief: String::new(),
        excerpt: line.map(|l| SourceExcerpt {
            start_line: l,
            end_line: l,
            snippet: String::new(),
        }),
        parent: None,
        picks: None,
        prerequisite_level: None,
        spells: Vec::new(),
    }
}

/// A draft of `source` with `planned` elements, each generated with `descriptions[i]`.
pub(crate) fn draft(
    source: &str,
    planned: Vec<PlannedElement>,
    descriptions: &[&str],
) -> ImportDraft {
    let mut d: ImportDraft = serde_json::from_value(json!({
        "edition": "2024", "id_prefix": "t", "source_name": "T", "pack_id": "t",
        "pack_name": "T", "source_kind": "attachments", "source_text": source,
        "detection": {"kind": "feat", "edition": "2024", "confidence": 1.0, "items": []},
        "plan": {"edition": "2024", "groups": [], "elements": [], "notes": []},
        "elements": [], "sweep_issues": []}))
    .expect("draft parses");
    d.plan = ImportPlan {
        elements: planned,
        ..d.plan
    };
    d.elements = crate::import::anything::generate::pending(&d.plan);
    for (e, desc) in d.elements.iter_mut().zip(descriptions) {
        e.json = json!({"id": e.id, "name": e.name, "type": e.ty, "description": desc});
        e.status = ElementStatus::Ok;
    }
    d
}

const SOURCE: &str = "## Lantern Kin\n\n**Ember Sight.** You can see through smoke and \
fog out to 30 feet as if it were clear air, and you can't be blinded by bright light. You \
can use Ember Sight again after a short rest.\n\
**Warm Hands.** Once per long rest you can warm a creature you touch, ending the frozen \
condition on it.\n";

#[test]
fn words_join_line_end_hyphens_and_split_digits() {
    assert_eq!(
        words("de-\n termining 25gp can't"),
        ["determining", "25", "gp", "cant"]
    );
    assert_eq!(words("well-known"), ["well", "known"]);
}

#[test]
fn a_compound_hyphen_at_a_line_end_matches_the_rejoined_compound() {
    let source = "The shield bears a large sky-\nblue stone set in a ring of iron that hums when \
        struck.\nIts maker was a re-\nnowned smith of the coast.";
    let index = Index::new(source);
    let el = json!({"description": "<p>The shield bears a large sky-blue stone set in a ring of \
        iron that hums when struck.</p><p>Its maker was a renowned smith of the coast.</p>"});
    assert!(untraced(&el, &index, &HashSet::new()).is_empty());
}

#[test]
fn a_model_written_text_element_is_checked_and_a_located_one_is_not() {
    let invented = "<p>Every caravan of the north hires these wanderers as its trusted guides.</p>";
    let mut d = draft(
        SOURCE,
        vec![planned(
            "t1",
            "Lantern Lore",
            "Rules Glossary Entry",
            Role::Text,
            None,
        )],
        &[invented],
    );
    // Unlocated: the model wrote it from the whole source, so it gets the review note.
    assert_eq!(findings(&d).len(), 1);
    assert!(sweep(&d).is_empty(), "no passage to repair from: no call");
    note_leftovers(&mut d);
    assert_eq!(d.elements[0].status, ElementStatus::NeedsReview);
    assert!(d.elements[0].issues[0].starts_with(NOTE));
    // Located: the pipeline copied the passage itself.
    d.plan.elements[0].excerpt = Some(SourceExcerpt {
        start_line: 3,
        end_line: 3,
        snippet: String::new(),
    });
    assert!(findings(&d).is_empty());
}

#[test]
fn a_printed_sentence_passes_and_an_invented_one_is_found() {
    let index = Index::new(SOURCE);
    let el = json!({"description": "<p>You can see through smoke and fog out to 30 feet as if \
        it were clear air.</p><p>Lantern kin are prized as guides by every caravan in the north.</p>"});
    let found = untraced(&el, &index, &HashSet::new());
    assert_eq!(
        found,
        vec!["Lantern kin are prized as guides by every caravan in the north.".to_string()]
    );
}

#[test]
fn a_table_re_laid_in_another_row_order_is_the_printed_text() {
    // Arrange: a d6 table printed in two column pairs; the model lays it out one row per roll.
    let source = "## Odd Hobbies\n\nEvery tinker has a hobby they cannot stop talking about.\n\n\
        | d6 | Hobby | d6 | Hobby |\n| 1 | Kites | 4 | Moths |\n| 2 | Clocks | 5 | Riddles |\n\
        | 3 | Bells | 6 | Knots |\n";
    let index = Index::new(source);
    let rows: String = ["Kites", "Clocks", "Bells", "Moths", "Riddles", "Knots"]
        .iter()
        .enumerate()
        .map(|(i, h)| format!("<tr><td>{}</td><td>{h}</td></tr>", i + 1))
        .collect();
    let el = json!({"description": format!("<p>Every tinker has a hobby they cannot stop \
        talking about.</p><table><thead><tr><th>d6</th><th>Hobby</th></tr></thead><tbody>{rows}\
        </tbody></table><h5>Feature: Endless Chatter</h5><p>Every tinker has a hobby they \
        cannot stop talking about.</p>")});
    // Act / Assert: no cell or heading runs into a sentence.
    assert!(untraced(&el, &index, &HashSet::new()).is_empty());
    let sentences = sentences(el["description"].as_str().unwrap());
    assert!(sentences.contains(&"Feature: Endless Chatter".to_string()));
    // An invented sentence after a table is still found.
    let invented = json!({"description": format!("<table><tbody>{rows}</tbody></table><p>Tinkers \
        are welcome guests at every harvest fair in the valley.</p>")});
    assert_eq!(untraced(&invented, &index, &HashSet::new()).len(), 1);
}

#[test]
fn pipeline_paragraphs_and_short_sentences_are_not_checked() {
    let index = Index::new(SOURCE);
    let el = json!({"description": "<p>See below.</p><p><strong>Crafting Cost:</strong> 40 gp \
        and two days of work at a forge</p><p><em>Requires attunement by a bard or a cleric of \
        the old faith.</em></p>"});
    assert!(untraced(&el, &index, &HashSet::new()).is_empty());
}

#[test]
fn the_pipelines_own_names_are_no_evidence_of_rewording() {
    // The plan names the element "Glow Gaze" where the source prints "Ember Sight".
    let d = draft(
        SOURCE,
        vec![planned(
            "t1",
            "Glow Gaze",
            "Racial Trait",
            Role::Part,
            Some(3),
        )],
        &["<p>You can use Glow Gaze again after a short rest.</p>"],
    );
    let index = Index::new(&d.source_text);
    let plain_mask = HashSet::new();
    assert_eq!(untraced(&d.elements[0].json, &index, &plain_mask).len(), 1);
    assert!(untraced(&d.elements[0].json, &index, &name_mask(&d)).is_empty());
}

#[test]
fn untraced_sentences_get_one_repair_then_a_review_note() {
    let mut d = draft(
        SOURCE,
        vec![
            planned("t1", "Ember Sight", "Racial Trait", Role::Part, Some(3)),
            planned("t2", "Warm Hands", "Racial Trait", Role::Part, Some(4)),
        ],
        &[
            "<p>You can see through smoke and fog out to 30 feet as if it were clear air, and you can't be blinded by bright light.</p>",
            "<p>Your touch carries the warmth of a hearth that never goes out.</p>",
        ],
    );
    let issues = sweep(&d);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].element_id.as_deref(), Some("t2"));
    assert!(issues[0].message.starts_with(ISSUE));
    note_leftovers(&mut d);
    assert_eq!(d.elements[1].status, ElementStatus::NeedsReview);
    assert!(d.elements[1].issues[0].starts_with(NOTE));
    assert_eq!(d.elements[0].status, ElementStatus::Ok);
    // Unlocated: nothing printed to copy, so no repair call; the review note still comes.
    d.elements[1].status = ElementStatus::Ok;
    d.elements[1].issues.clear();
    d.plan.elements[1].excerpt = None;
    assert!(sweep(&d).is_empty());
    note_leftovers(&mut d);
    assert_eq!(d.elements[1].status, ElementStatus::NeedsReview);
    // A description the user revised is theirs.
    d.elements[1].revisions = 1;
    assert!(sweep(&d).is_empty());
}

#[test]
fn briefs_lose_instructions_to_leave_source_lines_out() {
    let brief = "A fey folk of the marshes. Ability score, age, size, speed and language lines \
        are left out. The Extra Tongue line is omitted. Its traits follow.";
    assert_eq!(
        sanitize_brief(brief),
        "A fey folk of the marshes. Its traits follow."
    );
    // Game text about leaving something out is not an instruction.
    let keep = "You can omit the material component. Excluded creatures are immune.";
    assert_eq!(sanitize_brief(keep), keep);
}
