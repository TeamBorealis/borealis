//! Apple PDFKit (Preview, Safari) parity: every page of fillable sheets, rendered by PDFKit with
//! its own widget drawing, must match poppler's render. PDFKit ignores `/AP` for text widgets and
//! redraws their values in a fallback font, so this guards the "prefilled values are page content"
//! rule of the form writer. macOS only; skips without `xcrun swiftc` or `pdftoppm`.
#![cfg(target_os = "macos")]

mod common;

use borealis_sheet::{render_pdf, Paper, SheetOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

const DPI: &str = "72";
/// A blurred pixel differs when its grey level moves by more than this.
const GREY_TOLERANCE: u8 = 48;
/// Share of differing pixels allowed per page (0.5 %; the PDFKit fallback fonts score ~1.4 %).
const MAX_DIFF_SHARE: f64 = 0.005;

struct Grey {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

fn decode(path: &Path) -> Grey {
    let bytes = std::fs::read(path).expect("png written");
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().expect("png header");
    let mut buf = vec![0; r.output_buffer_size().expect("png size")];
    let info = r.next_frame(&mut buf).expect("png frame");
    let n = info.color_type.samples();
    let px = buf[..info.buffer_size()]
        .chunks(n)
        .map(|p| {
            if n >= 3 {
                ((u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2])) / 3) as u8
            } else {
                p[0]
            }
        })
        .collect();
    Grey {
        w: info.width as usize,
        h: info.height as usize,
        px,
    }
}

/// 3x3 box blur, so anti-aliasing differences between the two rasterisers fade out.
fn blur(g: &Grey) -> Vec<u8> {
    let at = |x: usize, y: usize| u32::from(g.px[y * g.w + x]);
    let mut out = g.px.clone();
    for y in 1..g.h - 1 {
        for x in 1..g.w - 1 {
            let sum: u32 = (y - 1..=y + 1)
                .flat_map(|yy| (x - 1..=x + 1).map(move |xx| (xx, yy)))
                .map(|(xx, yy)| at(xx, yy))
                .sum();
            out[y * g.w + x] = (sum / 9) as u8;
        }
    }
    out
}

fn diff_share(a: &Grey, b: &Grey) -> f64 {
    if (a.w, a.h) != (b.w, b.h) {
        return 1.0;
    }
    let (a, b) = (blur(a), blur(b));
    let bad = a
        .iter()
        .zip(&b)
        .filter(|(p, q)| p.abs_diff(**q) > GREY_TOLERANCE)
        .count();
    bad as f64 / a.len() as f64
}

/// Compile the PDFKit renderer once per target directory.
fn pdfkit_renderer(dir: &Path) -> Option<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/pdfkit_render.swift");
    let bin = dir.join("pdfkit_render");
    let fresh = |b: &Path| {
        let m = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        matches!((m(b), m(&src)), (Some(b), Some(s)) if b >= s)
    };
    if fresh(&bin) {
        return Some(bin);
    }
    let out = Command::new("xcrun")
        .args(["swiftc", "-O", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .ok()?;
    if !out.status.success() {
        eprintln!(
            "skipping: swiftc failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    Some(bin)
}

fn page_count(pdf: &[u8]) -> usize {
    lopdf::Document::load_mem(pdf)
        .expect("pdf parses")
        .get_pages()
        .len()
}

fn check(name: &str, pdf: &[u8], renderer: &Path, dir: &Path) -> Vec<String> {
    let file = dir.join(format!("{name}.pdf"));
    std::fs::write(&file, pdf).expect("pdf written");
    let mut failures = Vec::new();
    for page in 1..=page_count(pdf) {
        let kit = dir.join(format!("{name}-{page}-pdfkit.png"));
        let st = Command::new(renderer)
            .arg(&file)
            .arg((page - 1).to_string())
            .arg(DPI)
            .arg(&kit)
            .status()
            .expect("pdfkit renderer runs");
        assert!(st.success(), "PDFKit render of {name} page {page} failed");
        let prefix = dir.join(format!("{name}-{page}-poppler"));
        let st = Command::new("pdftoppm")
            .args(["-png", "-gray", "-r", DPI, "-singlefile", "-f"])
            .arg(page.to_string())
            .arg("-l")
            .arg(page.to_string())
            .arg(&file)
            .arg(&prefix)
            .status()
            .expect("pdftoppm runs");
        assert!(st.success(), "pdftoppm failed");
        let share = diff_share(&decode(&kit), &decode(&prefix.with_extension("png")));
        if share > MAX_DIFF_SHARE {
            failures.push(format!(
                "{name} page {page}: {:.2} % of pixels differ (see {})",
                share * 100.0,
                kit.display()
            ));
        }
    }
    failures
}

#[test]
fn pdfkit_draws_fillable_sheets_like_poppler() {
    if Command::new("pdftoppm").arg("-v").output().is_err() {
        eprintln!("skipping: pdftoppm not found");
        return;
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("pdfkit_parity");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let Some(renderer) = pdfkit_renderer(&dir) else {
        return;
    };
    let mut failures = Vec::new();
    for (name, (c, snap), paper) in [
        ("wizard_2024_letter", common::wizard_2024(), Paper::Letter),
        (
            "fighter_wizard_2014_a4",
            common::fighter_wizard_2014(),
            Paper::A4,
        ),
    ] {
        let options = SheetOptions {
            paper,
            form_fillable: true,
            ..SheetOptions::default()
        };
        let pdf = render_pdf(&c, &snap, &options).expect("renders");
        failures.extend(check(name, &pdf, &renderer, &dir));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
