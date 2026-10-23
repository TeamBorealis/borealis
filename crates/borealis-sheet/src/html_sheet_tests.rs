//! Tests of the HTML sheet preview against real engine snapshots.

use super::{render_sheet, HtmlOptions, PortraitImage};
use crate::model_fixtures::{cleric_2014, cleric_wizard_2014, wizard_2014, wizard_2024};
use crate::model_types::{SheetData, SpellFactsMap};
use crate::{PageSelection, Paper};

fn data(
    pair: &(
        borealis_character::Character,
        borealis_engine::CharacterSnapshot,
    ),
) -> SheetData {
    crate::sheet_data(&pair.0, &pair.1, &SpellFactsMap::new())
}

fn with_story(mut d: SheetData) -> SheetData {
    d.story.backstory = "Raised by the lighthouse keeper.".into();
    d
}

fn count(html: &str, id: &str) -> usize {
    html.matches(&format!("data-field=\"{id}\"")).count()
}

#[test]
fn both_editions_render_complete_documents_with_every_field_once() {
    for pair in [
        cleric_2014(),
        wizard_2014(),
        wizard_2024(),
        cleric_wizard_2014(),
    ] {
        let d = with_story(data(&pair));
        let html = render_sheet(&d, &HtmlOptions::default());
        assert!(html.starts_with("<!doctype html>") && html.ends_with("</html>"));
        assert!(!html.contains("<script"), "no scripts");
        let pooled: Vec<usize> = d
            .spellcasting
            .casters
            .iter()
            .enumerate()
            .filter(|(_, c)| c.slots_pooled)
            .map(|(i, _)| i + 1)
            .collect();
        for f in d.fields() {
            if pooled
                .iter()
                .any(|n| f.id.starts_with(&format!("spell.{n}.slot.")))
            {
                continue;
            }
            assert_eq!(
                count(&html, &f.id),
                1,
                "{:?}: field {} (play: {})",
                d.edition,
                f.id,
                f.play
            );
        }
    }
}

#[test]
fn page_plan_follows_the_edition() {
    let p24 = render_sheet(&with_story(data(&wizard_2024())), &HtmlOptions::default());
    for kind in ["character", "features", "spellcasting", "story"] {
        assert!(
            p24.contains(&format!("data-page=\"{kind}\"")),
            "2024 {kind}"
        );
    }
    let p14 = render_sheet(&data(&cleric_2014()), &HtmlOptions::default());
    for kind in ["character", "details", "spellcasting", "inventory"] {
        assert!(
            p14.contains(&format!("data-page=\"{kind}\"")),
            "2014 {kind}"
        );
    }
    assert!(p14.contains("Racial traits") && !p14.contains("data-page=\"story\""));
    assert!(p24.contains("Species traits"));
}

#[test]
fn story_page_is_omitted_when_empty() {
    let d = data(&wizard_2024());
    assert!(!d.story.has_content());
    let html = render_sheet(&d, &HtmlOptions::default());
    assert!(!html.contains("data-page=\"story\""));
    assert_eq!(count(&html, "story.backstory"), 0);
    let html14 = render_sheet(&data(&wizard_2014()), &HtmlOptions::default());
    assert_eq!(count(&html14, "story.backstory"), 0);
    assert!(render_sheet(&with_story(d), &HtmlOptions::default()).contains("data-page=\"story\""));
}

#[test]
fn story_page_needs_text_of_its_own_fields() {
    // 2024 treasure, quest items and additional features are on page 2: a Story page with only
    // them would be blank write-ins. A portrait, a trinket or a companion name keep the page.
    let mut d = data(&wizard_2024());
    d.story.treasure = "A sapphire.".into();
    d.story.quest = "The sealed letter.".into();
    let html = render_sheet(&d, &HtmlOptions::default());
    assert!(!html.contains("data-page=\"story\""));
    let mut with_trinket = d.clone();
    with_trinket.story.trinket = "A cracked holy symbol.".into();
    let mut with_portrait = d.clone();
    with_portrait.story.has_portrait = true;
    let mut with_companion = d;
    with_companion.extras.companion_name = "Owl".into();
    for d in [with_trinket, with_portrait, with_companion] {
        let html = render_sheet(&d, &HtmlOptions::default());
        assert!(html.contains("data-page=\"story\""));
    }
}

#[test]
fn user_text_is_escaped_and_marked_dir_auto() {
    let mut d = with_story(data(&wizard_2024()));
    d.identity.name = "<script>alert(1)</script> \u{5e9}\u{5dc}\u{5d5}\u{5dd}".into();
    d.story.backstory = "<img src=x onerror=alert(1)>".into();
    d.identity.player = "\"><b>".into();
    let html = render_sheet(&d, &HtmlOptions::default());
    assert!(!html.contains("<script>alert") && !html.contains("<img src=x"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(html.contains("data-field=\"char.name\" dir=\"auto\""));
}

#[test]
fn only_allow_listed_portraits_are_embedded() {
    let mut d = with_story(data(&wizard_2024()));
    d.story.has_portrait = true;
    let mut o = HtmlOptions {
        portrait: Some(PortraitImage {
            mime: "image/png".into(),
            base64: "iVBORw0KGgo=".into(),
        }),
        ..HtmlOptions::default()
    };
    assert!(render_sheet(&d, &o).contains("src=\"data:image/png;base64,iVBORw0KGgo=\""));
    for bad in [
        ("image/svg+xml", "AAAA"),
        ("image/png", "AA\"onerror=\"x"),
        ("text/html", "AAAA"),
    ] {
        o.portrait = Some(PortraitImage {
            mime: bad.0.into(),
            base64: bad.1.into(),
        });
        let html = render_sheet(&d, &o);
        assert!(
            !html.contains("data:image") && !html.contains("onerror"),
            "{bad:?}"
        );
    }
}

fn colour_hexes(html: &str) -> Vec<String> {
    let b = html.as_bytes();
    let mut out = Vec::new();
    for (i, _) in html.match_indices('#') {
        let run: String = b[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .map(|c| *c as char)
            .collect();
        if run.len() == 3 || run.len() == 6 {
            out.push(run);
        }
    }
    out
}

fn is_grey(h: &str) -> bool {
    let c: Vec<char> = h.chars().collect();
    if c.len() == 3 {
        c[0] == c[1] && c[1] == c[2]
    } else {
        c[0..2] == c[2..4] && c[2..4] == c[4..6]
    }
}

#[test]
fn ink_mode_uses_greys_only_and_colour_mode_does_not() {
    let d = with_story(data(&wizard_2024()));
    let ink = render_sheet(
        &d,
        &HtmlOptions {
            ink: true,
            ..HtmlOptions::default()
        },
    );
    let hexes = colour_hexes(&ink);
    assert!(!hexes.is_empty());
    for h in &hexes {
        assert!(is_grey(h), "colour {h} in ink output");
    }
    let colour = render_sheet(&d, &HtmlOptions::default());
    assert!(colour_hexes(&colour).iter().any(|h| !is_grey(h)));
}

#[test]
fn options_are_honoured() {
    let d = data(&wizard_2024());
    let a4 = render_sheet(
        &d,
        &HtmlOptions {
            paper: Paper::A4,
            compact: true,
            ..HtmlOptions::default()
        },
    );
    assert!(
        a4.contains("size:A4") && a4.contains("class=\"sheet compact\"") && a4.contains("page a4")
    );
    let tr = render_sheet(
        &d,
        &HtmlOptions {
            lang: "he".into(),
            ..HtmlOptions::default()
        },
    );
    assert!(tr.contains("<html lang=\"he\">"));
    let bad = render_sheet(
        &d,
        &HtmlOptions {
            lang: "x\"><script>".into(),
            ..HtmlOptions::default()
        },
    );
    assert!(bad.contains("<html lang=\"en\">"));
    let pages = PageSelection {
        core: true,
        spellcasting: false,
        inventory: false,
        story: false,
    };
    let only = render_sheet(
        &d,
        &HtmlOptions {
            pages,
            ..HtmlOptions::default()
        },
    );
    assert!(!only.contains("data-page=\"spellcasting\""));
    let plain = render_sheet(
        &d,
        &HtmlOptions {
            style_formatting: false,
            ..HtmlOptions::default()
        },
    );
    assert!(plain.contains("feat plain"));
}

#[test]
fn unprepared_spells_can_be_hidden() {
    let d = data(&wizard_2014());
    let all = render_sheet(&d, &HtmlOptions::default());
    let some = render_sheet(
        &d,
        &HtmlOptions {
            include_non_prepared_spells: false,
            ..HtmlOptions::default()
        },
    );
    let rows = |h: &str| h.matches("spell.1.list.").count();
    assert!(rows(&some) < rows(&all));
}

#[test]
fn swap_puts_the_modifier_in_the_circle() {
    let d = data(&wizard_2024());
    let html = render_sheet(
        &d,
        &HtmlOptions {
            swap_ability_positions: true,
            ..HtmlOptions::default()
        },
    );
    assert!(html.contains("<div class=\"score\"><span class=\"fv \" data-field=\"abil.str.mod\""));
}

/// `BOREALIS_HTML_SAMPLE=/dir cargo test -p borealis-sheet html_sheet_sample` writes sample files.
#[test]
fn html_sheet_sample() {
    let Ok(dir) = std::env::var("BOREALIS_HTML_SAMPLE") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [("2014", cleric_2014()), ("2024", wizard_2024())];
    for (name, pair) in cases {
        let mut d = crate::model_fixtures::with_sample_extras(data(&pair));
        d.story.backstory = "Raised by the keeper of a lighthouse on the Sword Coast.".into();
        d.story.traits = "I always have a plan for what to do when things go wrong.".into();
        for ink in [false, true] {
            let html = render_sheet(
                &d,
                &HtmlOptions {
                    ink,
                    ..HtmlOptions::default()
                },
            );
            let file = format!("{dir}/{name}-{}.html", if ink { "ink" } else { "colour" });
            std::fs::write(file, html).unwrap();
        }
    }
}

#[test]
fn hit_dice_total_is_printed_once_per_die_size() {
    let d = data(&wizard_2024());
    let html = render_sheet(&d, &HtmlOptions::default());
    let h = &d.combat.hit_dice[0];
    let total = format!(">{}d{}<", h.count, h.die);
    assert_eq!(html.matches(&total).count(), 1, "{total}");
    assert!(html.contains(&format!("d{} \u{d7}{}", h.die, h.count)));
}

#[test]
fn legacy_spellcasting_page_gives_full_levels_the_blank_rows_of_others() {
    let mut d = data(&wizard_2014());
    let extra = d.spellcasting.casters[0].cantrips[0].clone();
    for _ in 0..10 {
        d.spellcasting.casters[0].cantrips.push(extra.clone());
    }
    let o = HtmlOptions {
        legacy_spellcasting: true,
        ..HtmlOptions::default()
    };
    let html = render_sheet(&d, &o);
    assert!(html.contains("data-page=\"spellcasting-basic\""));
    assert!(!html.contains("data-page=\"spellcasting\""));
    let cantrips = d.spellcasting.casters[0].cantrips.len();
    assert!(cantrips > 8, "more cantrips than the spec capacity");
    assert_eq!(count(&html, &format!("legacy.1.0.{cantrips}.name")), 1);
    assert_eq!(
        count(&html, &format!("legacy.1.0.{}.name", cantrips + 1)),
        0,
        "the cantrip rows grow to the list"
    );
    assert!(!html.contains("more spell"), "nothing is left out");
    let plain = render_sheet(&data(&wizard_2014()), &o);
    assert_eq!(count(&plain, "legacy.1.0.8.name"), 1, "spec capacity 8");
    assert_eq!(count(&plain, "legacy.1.0.9.name"), 0);
    let normal = render_sheet(&d, &HtmlOptions::default());
    assert!(normal.contains("data-page=\"spellcasting\"") && !normal.contains("legacy.1."));
}

#[test]
fn page_1_of_2014_has_personality_and_equipment_with_coins() {
    let mut d = with_story(data(&cleric_2014()));
    d.story.flaws = "Stubborn.".into();
    d.gear.coins = [0, 0, 0, 1234, 0];
    let html = render_sheet(&d, &HtmlOptions::default());
    let page = |kind: &str| {
        let at = html
            .find(&format!("data-page=\"{kind}\""))
            .unwrap_or_else(|| panic!("{kind} page"));
        let end = html[at..].find("</section>").map_or(html.len(), |e| at + e);
        html[at..end].to_string()
    };
    let p1 = page("character");
    assert!(p1.contains(">Personality<") && p1.contains(">Equipment<"));
    assert!(p1.contains("data-field=\"story.flaws\"") && p1.contains("Stubborn."));
    assert!(p1.contains("1,234"));
    assert!(p1.contains(&d.gear.items[0].name));
    assert!(!page("details").contains("data-field=\"story.flaws\""));
    // 2024 keeps them on the story page.
    let h24 = render_sheet(&with_story(data(&wizard_2024())), &HtmlOptions::default());
    let at = h24.find("data-page=\"story\"").expect("story page");
    assert!(h24[at..].contains("data-field=\"story.flaws\""));
}
