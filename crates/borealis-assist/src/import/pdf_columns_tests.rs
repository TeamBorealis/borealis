//! Reading-order tests on fragment geometry measured from real pages (invented text only).

use super::super::Fragment;
use super::reading_order;

/// A landscape page of three columns, each holding a numbered table: a row label, an item name
/// centred beside its text and text running on at line leading. Rows of neighbouring columns
/// share baselines. The geometry (x, end, baseline, size) is measured from a real catalogue
/// page; the text is invented: "ash", "birch" and "cedar" tag the page column, numbers are the
/// row labels, the 11.6 / 10.5 / 7.7 lines are running page lines (one a lone page number).
const THREE_TABLES: &[(f32, f32, f32, f32, &str)] = &[
    (
        296.6,
        496.4,
        564.2,
        11.6,
        "Running lines kept at hearthwell",
    ),
    (41.1, 50.2, 539.1, 9.6, "1"),
    (124.8, 235.4, 539.1, 9.6, "ash ab"),
    (300.4, 309.5, 539.1, 9.6, "4"),
    (383.8, 491.6, 539.1, 9.6, "birch ab"),
    (559.4, 568.5, 539.1, 9.6, "7"),
    (643.1, 764.8, 539.1, 9.6, "cedar ab"),
    (124.8, 224.5, 528.1, 9.6, "ash ac"),
    (383.8, 501.0, 528.1, 9.6, "birch ac"),
    (643.1, 756.0, 528.1, 9.6, "cedar ac"),
    (124.8, 230.0, 517.2, 9.6, "ash ad"),
    (383.8, 498.5, 517.2, 9.6, "birch ad"),
    (643.1, 761.2, 517.2, 9.6, "cedar ad"),
    (124.8, 231.8, 506.2, 9.6, "ash ae"),
    (383.8, 490.6, 506.2, 9.6, "birch ae"),
    (643.1, 721.2, 506.2, 9.6, "cedar ae"),
    (63.5, 101.6, 495.3, 9.6, "ash af"),
    (124.8, 231.2, 495.3, 9.6, "ash ag"),
    (383.8, 502.5, 495.3, 9.6, "birch af"),
    (643.1, 754.2, 495.3, 9.6, "cedar af"),
    (63.5, 116.5, 484.3, 9.6, "ash ah"),
    (124.8, 232.3, 484.3, 9.6, "ash ai"),
    (383.8, 504.7, 484.3, 9.6, "birch ag"),
    (643.1, 762.9, 484.3, 9.6, "cedar ag"),
    (124.8, 240.6, 473.4, 9.6, "ash aj"),
    (322.8, 380.8, 473.4, 9.6, "birch ah"),
    (383.8, 498.5, 473.4, 9.6, "birch ai"),
    (581.8, 638.9, 473.4, 9.6, "cedar ah"),
    (643.1, 756.0, 473.4, 9.6, "cedar ai"),
    (124.8, 227.0, 462.7, 9.6, "ash ak"),
    (322.8, 344.7, 462.7, 9.6, "birch aj"),
    (383.8, 497.2, 462.7, 9.6, "birch ak"),
    (643.1, 749.0, 462.7, 9.6, "cedar aj"),
    (124.8, 244.5, 451.7, 9.6, "ash al"),
    (383.8, 486.9, 451.7, 9.6, "birch al"),
    (643.1, 758.2, 451.7, 9.6, "cedar ak"),
    (124.8, 140.6, 440.8, 9.6, "ash am"),
    (383.8, 489.4, 440.8, 9.6, "birch am"),
    (643.1, 759.8, 440.8, 9.6, "cedar al"),
    (383.8, 498.1, 429.8, 9.6, "birch an"),
    (643.1, 762.0, 429.8, 9.6, "cedar am"),
    (41.1, 50.2, 429.1, 9.6, "2"),
    (124.8, 241.2, 429.1, 9.6, "ash an"),
    (383.8, 493.9, 418.9, 9.6, "birch ao"),
    (643.1, 748.2, 418.9, 9.6, "cedar an"),
    (124.8, 223.7, 418.2, 9.6, "ash ao"),
    (383.8, 480.3, 407.9, 9.6, "birch ap"),
    (643.1, 727.7, 407.9, 9.6, "cedar ao"),
    (124.8, 238.2, 407.2, 9.6, "ash ap"),
    (383.8, 410.9, 397.0, 9.6, "birch aq"),
    (124.8, 238.8, 396.3, 9.6, "ash aq"),
    (559.4, 568.5, 396.3, 9.6, "8"),
    (643.1, 763.8, 396.3, 9.6, "cedar ap"),
    (63.5, 111.8, 385.3, 9.6, "ash ar"),
    (124.8, 229.9, 385.3, 9.6, "ash as"),
    (300.4, 309.5, 385.3, 9.6, "5"),
    (383.8, 491.6, 385.3, 9.6, "birch ar"),
    (643.1, 754.3, 385.3, 9.6, "cedar aq"),
    (124.8, 240.6, 374.4, 9.6, "ash at"),
    (383.8, 490.5, 374.4, 9.6, "birch as"),
    (643.1, 730.3, 374.4, 9.6, "cedar ar"),
    (124.8, 227.1, 363.7, 9.6, "ash au"),
    (383.8, 501.0, 363.7, 9.6, "birch at"),
    (643.1, 747.7, 363.7, 9.6, "cedar as"),
    (124.8, 244.5, 352.7, 9.6, "ash av"),
    (383.8, 499.5, 352.7, 9.6, "birch au"),
    (643.1, 749.1, 352.7, 9.6, "cedar at"),
    (581.8, 621.2, 347.1, 9.6, "cedar au"),
    (124.8, 140.6, 341.8, 9.6, "ash aw"),
    (383.8, 490.6, 341.8, 9.6, "birch av"),
    (643.1, 763.3, 341.8, 9.6, "cedar av"),
    (581.8, 635.2, 336.2, 9.6, "cedar aw"),
    (383.8, 502.5, 330.8, 9.6, "birch aw"),
    (643.1, 758.3, 330.8, 9.6, "cedar ax"),
    (41.1, 50.2, 330.1, 9.6, "3"),
    (124.8, 232.6, 330.1, 9.6, "ash ax"),
    (383.8, 504.7, 319.9, 9.6, "birch ax"),
    (643.1, 752.8, 319.9, 9.6, "cedar ay"),
    (124.8, 242.4, 319.2, 9.6, "ash ay"),
    (322.8, 362.4, 314.5, 9.6, "birch ay"),
    (383.8, 498.5, 308.9, 9.6, "birch az"),
    (643.1, 747.1, 308.9, 9.6, "cedar az"),
    (124.8, 235.2, 308.2, 9.6, "ash az"),
    (322.8, 370.2, 303.6, 9.6, "birch ba"),
    (383.8, 497.2, 298.0, 9.6, "birch bb"),
    (643.1, 762.4, 298.0, 9.6, "cedar ba"),
    (124.8, 237.4, 297.3, 9.6, "ash ba"),
    (383.8, 487.0, 287.0, 9.6, "birch bc"),
    (643.1, 684.2, 287.0, 9.6, "cedar bb"),
    (124.8, 245.5, 286.3, 9.6, "ash bb"),
    (383.8, 489.4, 276.3, 9.6, "birch bd"),
    (124.8, 244.1, 275.4, 9.6, "ash bc"),
    (559.4, 568.5, 275.4, 9.6, "9"),
    (643.1, 757.0, 275.4, 9.6, "cedar bc"),
    (383.8, 498.1, 265.4, 9.6, "birch be"),
    (63.5, 103.1, 264.7, 9.6, "ash bd"),
    (124.8, 246.8, 264.7, 9.6, "ash be"),
    (643.1, 762.6, 264.7, 9.6, "cedar bd"),
    (383.8, 493.9, 254.4, 9.6, "birch bf"),
    (63.5, 120.7, 253.7, 9.6, "ash bf"),
    (124.8, 247.0, 253.7, 9.6, "ash bg"),
    (581.8, 749.4, 253.7, 9.6, "cedar be"),
    (383.8, 488.4, 243.5, 9.6, "birch bg"),
    (124.8, 239.7, 242.8, 9.6, "ash bh"),
    (581.8, 598.6, 242.8, 9.6, "cedar bf"),
    (643.1, 756.1, 242.8, 9.6, "cedar bg"),
    (383.8, 410.9, 232.5, 9.6, "birch bh"),
    (124.8, 248.2, 231.8, 9.6, "ash bi"),
    (643.1, 756.1, 231.8, 9.6, "cedar bh"),
    (124.8, 237.7, 220.9, 9.6, "ash bj"),
    (300.4, 309.5, 220.9, 9.6, "6"),
    (383.8, 490.9, 220.9, 9.6, "birch bi"),
    (643.1, 753.7, 220.9, 9.6, "cedar bi"),
    (124.8, 240.0, 209.9, 9.6, "ash bk"),
    (383.8, 507.2, 209.9, 9.6, "birch bj"),
    (559.4, 568.5, 209.2, 9.6, "10"),
    (643.1, 764.2, 209.2, 9.6, "cedar bj"),
    (124.8, 230.9, 199.0, 9.6, "ash bl"),
    (383.8, 505.5, 199.0, 9.6, "birch bk"),
    (643.1, 751.1, 198.3, 9.6, "cedar bk"),
    (124.8, 171.7, 188.0, 9.6, "ash bm"),
    (383.8, 504.5, 188.0, 9.6, "birch bl"),
    (643.1, 751.7, 187.3, 9.6, "cedar bl"),
    (383.8, 481.8, 177.3, 9.6, "birch bm"),
    (581.8, 639.5, 176.4, 9.6, "cedar bm"),
    (643.1, 765.2, 176.4, 9.6, "cedar bn"),
    (383.8, 489.7, 166.4, 9.6, "birch bn"),
    (643.1, 756.7, 165.7, 9.6, "cedar bo"),
    (322.8, 381.5, 160.8, 9.6, "birch bo"),
    (383.8, 491.5, 155.4, 9.6, "birch bp"),
    (643.1, 751.1, 154.7, 9.6, "cedar bp"),
    (383.8, 506.9, 144.5, 9.6, "birch bq"),
    (643.1, 705.5, 143.8, 9.6, "cedar bq"),
    (383.8, 495.2, 133.5, 9.6, "birch br"),
    (559.4, 568.5, 132.1, 9.6, "11"),
    (643.1, 765.3, 132.1, 9.6, "cedar br"),
    (383.8, 473.4, 122.6, 9.6, "birch bs"),
    (643.1, 760.1, 121.2, 9.6, "cedar bs"),
    (581.8, 641.8, 115.6, 9.6, "cedar bt"),
    (383.8, 486.5, 111.6, 9.6, "birch bt"),
    (643.1, 741.4, 110.2, 9.6, "cedar bu"),
    (383.8, 407.6, 100.7, 9.6, "birch bu"),
    (643.1, 721.2, 99.3, 9.6, "cedar bv"),
    (
        28.4,
        332.2,
        39.9,
        7.7,
        "Running notice: copy this page for your own table and no other use.",
    ),
    (372.6, 450.1, 39.9, 7.7, "Running ODD LAMP NOTES"),
    (508.8, 523.0, 39.9, 10.5, "31!"),
];
const LANDSCAPE_WIDTH: f32 = 792.0;
/// Where the fixture's page columns meet (test geometry only).
const PAGE_COLUMNS: [f32; 2] = [290.0, 550.0];

fn fragments(page: &[(f32, f32, f32, f32, &str)]) -> Vec<Fragment> {
    page.iter()
        .map(|&(x, end, y, size, text)| Fragment {
            x,
            end,
            y,
            size,
            bold: false,
            text: text.to_string(),
        })
        .collect()
}

fn page_column(x: f32) -> usize {
    PAGE_COLUMNS.iter().filter(|c| x >= **c).count()
}

/// Each tagged line's row: the nearest row label at or above it in its page column.
fn rows_of(
    page: &'static [(f32, f32, f32, f32, &'static str)],
) -> Vec<(&'static str, (usize, String))> {
    let labels: Vec<(usize, f32, &str)> = page
        .iter()
        .filter(|f| f.4.chars().all(|c| c.is_ascii_digit()))
        .map(|f| (page_column(f.0), f.2, f.4))
        .collect();
    page.iter()
        .filter(|f| f.4.contains(' ') && !f.4.starts_with("Running"))
        .map(|&(x, _, y, _, text)| {
            let col = page_column(x);
            let label = labels
                .iter()
                .filter(|(c, ly, _)| *c == col && *ly >= y - 1.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map_or("", |l| l.2);
            (text, (col, label.to_string()))
        })
        .collect()
}

fn holds(line: &str, tag: &str) -> bool {
    line.match_indices(tag)
        .any(|(i, _)| !line[i + tag.len()..].starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// Tables in neighbouring page columns whose rows share baselines are read column by column,
/// one row per output line: no line mixes two columns' rows, and every row comes out whole.
#[test]
fn tables_side_by_side_on_shared_baselines_keep_their_own_rows() {
    let out = reading_order(fragments(THREE_TABLES), LANDSCAPE_WIDTH);
    let rows = rows_of(THREE_TABLES);
    let text: Vec<&str> = out.iter().map(|f| f.text.as_str()).collect();
    let text = text.join("\n");
    for line in &out {
        let mut owners: Vec<&(usize, String)> = rows
            .iter()
            .filter(|(tag, _)| holds(&line.text, tag))
            .map(|(_, row)| row)
            .collect();
        owners.sort();
        owners.dedup();
        assert!(
            owners.len() <= 1,
            "mixed rows {owners:?}: {}\n{text}",
            line.text
        );
    }
    let mut whole: Vec<&(usize, String)> = rows.iter().map(|(_, row)| row).collect();
    whole.sort();
    whole.dedup();
    for row in whole {
        let lines = out
            .iter()
            .filter(|l| rows.iter().any(|(tag, r)| r == row && holds(&l.text, tag)))
            .count();
        assert_eq!(lines, 1, "row {row:?} is split over {lines} lines\n{text}");
    }
}

fn frag(x: f32, end: f32, y: f32, text: &str) -> Fragment {
    Fragment {
        x,
        end,
        y,
        size: 10.0,
        bold: false,
        text: text.to_string(),
    }
}

/// A wide line starting at a numbered table's name column (names centred below the labels) is
/// that row's text run on, but a paragraph after the table whose first line is indented (off
/// every cell edge) stays prose.
#[test]
fn an_indented_paragraph_after_a_numbered_table_is_not_a_cell() {
    let mut page = Vec::new();
    for (k, name) in ["Lamp of dusk", "Jar of plenty", "Ring of warmth"]
        .iter()
        .enumerate()
    {
        let y = 700.0 - 24.0 * k as f32;
        page.push(frag(72.0, 78.0, y, &(k + 1).to_string()));
        page.push(frag(90.0, 140.0, y - 6.0, name));
        page.push(frag(150.0, 290.0, y, "glows when it is shaken"));
        page.push(frag(150.0, 280.0, y - 12.0, "twice at night"));
    }
    page.push(frag(
        90.0,
        260.0,
        616.0,
        "Hook of holding run into its text",
    ));
    page.push(frag(
        86.0,
        300.0,
        590.0,
        "Old ways keep the lamps named and lit",
    ));
    page.push(frag(
        72.0,
        300.0,
        578.0,
        "through the long winter on every road",
    ));
    page.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let out = crate::import::pdf_table::tables(page);
    let text: Vec<&str> = out.iter().map(|f| f.text.as_str()).collect();
    assert!(
        text.iter()
            .any(|l| l.starts_with("3 | ") && l.contains("Hook of holding")),
        "{text:?}"
    );
    assert!(
        text.contains(&"Old ways keep the lamps named and lit"),
        "{text:?}"
    );
}
