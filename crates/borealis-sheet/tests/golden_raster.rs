//! Raster goldens: page 1 and the spell page of the colour Letter 2014 and 2024 sheets at 40 dpi,
//! compared with a tolerance. Needs `pdftoppm` (skips otherwise). Update with
//! `BOREALIS_UPDATE_GOLDEN=1 cargo test -p borealis-sheet --test golden_raster`.

mod common;
mod support;

use borealis_sheet::{render_pdf, SheetEdition, SheetOptions};
use lopdf::Document;
use std::path::Path;
use std::process::Command;

const DPI: &str = "40";
/// A pixel differs when any channel moves by more than this.
const CHANNEL_TOLERANCE: i32 = 32;
/// Share of differing pixels allowed (0.5 %).
const MAX_DIFF_SHARE: f64 = 0.005;

struct Raster {
    w: usize,
    h: usize,
    rgb: Vec<u8>,
}

fn decode(bytes: &[u8]) -> Raster {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().expect("png header");
    let mut buf = vec![0; r.output_buffer_size().expect("png size")];
    let info = r.next_frame(&mut buf).expect("png frame");
    let n = info.color_type.samples();
    let rgb = buf[..info.buffer_size()]
        .chunks(n)
        .flat_map(|p| {
            if n >= 3 {
                [p[0], p[1], p[2]]
            } else {
                [p[0]; 3]
            }
        })
        .collect();
    Raster {
        w: info.width as usize,
        h: info.height as usize,
        rgb,
    }
}

fn pdftoppm_available() -> bool {
    Command::new("pdftoppm").arg("-v").output().is_ok()
}

fn rasterize(pdf: &Path, page: u32, out_prefix: &Path) -> Vec<u8> {
    let st = Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            DPI,
            "-f",
            &page.to_string(),
            "-l",
            &page.to_string(),
            "-singlefile",
        ])
        .arg(pdf)
        .arg(out_prefix)
        .status()
        .expect("pdftoppm runs");
    assert!(st.success(), "pdftoppm failed");
    std::fs::read(out_prefix.with_extension("png")).expect("png written")
}

/// Share of pixels whose channels differ by more than the tolerance (1.0 for a size mismatch).
fn diff_share(a: &Raster, b: &Raster) -> f64 {
    if (a.w, a.h) != (b.w, b.h) {
        return 1.0;
    }
    let bad = a
        .rgb
        .chunks(3)
        .zip(b.rgb.chunks(3))
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .any(|(x, y)| (i32::from(*x) - i32::from(*y)).abs() > CHANNEL_TOLERANCE)
        })
        .count();
    bad as f64 / (a.w * a.h) as f64
}

fn spell_page(pdf: &[u8]) -> u32 {
    let doc = Document::load_mem(pdf).expect("parses");
    doc.get_pages()
        .iter()
        .find(|(_, id)| {
            support::page_widgets(&doc, **id)
                .iter()
                .any(|n| n.starts_with("spell."))
        })
        .map(|(n, _)| *n)
        .expect("a page with spell fields")
}

#[test]
fn raster_goldens_stay_within_tolerance() {
    if !pdftoppm_available() {
        eprintln!("skipped: pdftoppm missing");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("borealis-golden-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut failures = Vec::new();
    for y2024 in [false, true] {
        let (c, s) = if y2024 {
            common::wizard_2024()
        } else {
            common::wizard_2014()
        };
        let o = SheetOptions {
            edition: if y2024 {
                SheetEdition::Y2024
            } else {
                SheetEdition::Y2014
            },
            ..SheetOptions::default()
        };
        let pdf = render_pdf(&c, &s, &o).unwrap();
        let ed = if y2024 { "2024" } else { "2014" };
        let pdf_path = tmp.join(format!("{ed}.pdf"));
        std::fs::write(&pdf_path, &pdf).unwrap();
        for (label, page) in [("p1", 1), ("spells", spell_page(&pdf))] {
            let name = format!("raster_{ed}_letter_colour_{label}.png");
            let png = rasterize(&pdf_path, page, &tmp.join(format!("{ed}_{label}")));
            let golden = support::golden_dir().join(&name);
            if support::updating() {
                std::fs::write(&golden, &png).unwrap();
                continue;
            }
            let Ok(expected) = std::fs::read(&golden) else {
                failures.push(format!(
                    "{name}: golden missing; run with {}=1",
                    support::UPDATE_VAR
                ));
                continue;
            };
            let share = diff_share(&decode(&expected), &decode(&png));
            if share > MAX_DIFF_SHARE {
                let actual = tmp.join(format!("actual_{name}"));
                std::fs::write(&actual, &png).unwrap();
                failures.push(format!(
                    "{name}: {:.2}% of pixels differ (limit {:.1}%); actual kept at {}; if intended run {}=1",
                    share * 100.0,
                    MAX_DIFF_SHARE * 100.0,
                    actual.display(),
                    support::UPDATE_VAR
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn diff_share_counts_only_large_channel_moves() {
    let base = Raster {
        w: 2,
        h: 1,
        rgb: vec![10, 10, 10, 200, 200, 200],
    };
    let near = Raster {
        w: 2,
        h: 1,
        rgb: vec![40, 10, 10, 200, 200, 200],
    };
    let far = Raster {
        w: 2,
        h: 1,
        rgb: vec![60, 10, 10, 200, 200, 200],
    };
    assert!(diff_share(&base, &near) < f64::EPSILON);
    assert!((diff_share(&base, &far) - 0.5).abs() < 1e-9);
}
