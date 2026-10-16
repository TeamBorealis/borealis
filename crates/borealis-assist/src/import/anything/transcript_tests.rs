use super::*;

#[test]
fn paraphrase_markers_are_dropped_and_each_marked_passage_is_recorded() {
    let t = "# Mossfolk\n[Paraphrased] Mossfolk live in bogs and sing to the frogs.\n\n**Age.** Adult at 20 and old at 90.\n\n**Bog Step.** (Paraphrased.)\nYou cross mud without slowing down at all.";
    let r = read(t);
    assert!(r.paraphrased);
    assert_eq!(
        r.text,
        "# Mossfolk\nMossfolk live in bogs and sing to the frogs.\n\n**Age.** Adult at 20 and old at 90.\n\n**Bog Step.** \nYou cross mud without slowing down at all."
    );
    // A marked line is its own passage; a marker on a short line stands for its paragraph.
    assert_eq!(r.spans.len(), 2, "{:?}", r.spans);
    assert_eq!(r.spans[0], "Mossfolk live in bogs and sing to the frogs.");
    assert!(r.spans[1].contains("You cross mud"), "{:?}", r.spans);
    assert!(!r.spans.iter().any(|s| s.contains("Adult at 20")));
}

#[test]
fn a_disclaimed_copy_records_every_paragraph_and_illegible_print_is_kept_and_recorded() {
    let r = read("> **Note:** This is not a word-for-word copy of the pages.\n\n# Mossfolk\n\nMossfolk live in bogs and sing.\n\nThey trade reeds for salt.");
    assert!(r.paraphrased);
    assert!(r.text.starts_with("# Mossfolk"), "{}", r.text);
    assert_eq!(r.spans.len(), 2, "{:?}", r.spans);
    let r = read("# Mossfolk\n\nThe bog is [illegible] deep and very cold.\n\nThey trade reeds.");
    assert!(!r.paraphrased);
    assert!(r.text.contains("is [illegible] deep"));
    assert_eq!(r.spans, vec!["The bog is [illegible] deep and very cold."]);
    // A marked passage among the first paragraphs is printed text, not a disclaimer.
    let r =
        read("# Mossfolk\n\n[Paraphrased] Mossfolk live in bogs and sing.\n\nThey trade reeds.");
    assert!(
        r.text.contains("Mossfolk live in bogs and sing."),
        "{}",
        r.text
    );
    assert_eq!(r.spans, vec!["Mossfolk live in bogs and sing."]);
    // Plain text is untouched.
    let plain = "# Mossfolk\n\nMossfolk live in bogs.\n\nThey repeat oaths word for word.";
    let r = read(plain);
    assert_eq!(
        (r.text.as_str(), r.spans.len(), r.paraphrased),
        (plain, 0, false)
    );
}

#[test]
fn transcriber_notes_never_reach_the_source_text() {
    let t = "[Note: Page 1 is the cover. It shows a lantern over a bog.]\n\n# Mossfolk\n\n*[Image: a mossfolk child holding a frog]*\nMossfolk live in bogs.\n(Note: the next page is blank.)\n\n[illegible]\n\nThe Bog Step costs [1] action.";
    let r = read(t);
    assert!(!r.text.contains("cover") && !r.text.contains("Image") && !r.text.contains("blank"));
    assert!(
        r.text.starts_with("# Mossfolk\n\nMossfolk live in bogs."),
        "{}",
        r.text
    );
    // A lone marker and a bracket inside a sentence are printed text.
    assert!(r.text.contains("[illegible]") && r.text.contains("costs [1] action"));
    // Parenthesised printed text is not a note.
    assert!(!is_transcriber_note("(see the Bog Step table below)"));
    assert!(is_transcriber_note(
        "**[The page shows a map of the bog.]**"
    ));
}

#[test]
fn recorded_passages_round_trip_through_the_notes_and_are_found_in_descriptions() {
    let spans = vec![
        "Mossfolk live in bogs and sing to the frogs at night.".to_string(),
        "The bog is [illegible] deep.".to_string(),
    ];
    let mut notes = span_notes("page 3 of `moss.pdf`", &spans);
    // The review's paraphrase check reads the notes after the reading note prefix.
    assert!(NOT_VERBATIM_NOTE.starts_with(super::super::READING_NOTE));
    assert_eq!(
        notes[0],
        "Reading the source: paraphrased passage: \"Mossfolk live in bogs and sing to the frogs at night.\" (page 3 of `moss.pdf`)"
    );
    notes.push("Reading the source: something else".into());
    let read_back = not_verbatim_spans(&notes);
    assert_eq!(read_back.len(), 2);
    assert_eq!(read_back[0].0, "page 3 of `moss.pdf`");
    assert_eq!(read_back[1].1, spans[1]);
    let html = "<p>Mossfolk live in bogs and sing to the frogs at night.</p><p>Other text.</p>";
    assert_eq!(
        holds_not_verbatim(html, &read_back),
        Some(spans[0].as_str())
    );
    assert_eq!(
        holds_not_verbatim("<p>The bog is deep.</p>", &read_back),
        Some(spans[1].as_str())
    );
    assert_eq!(
        holds_not_verbatim("<p>Mossfolk trade reeds.</p>", &read_back),
        None
    );
}
