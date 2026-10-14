//! Synthetic documents for tests and fixtures (invented text only). Not part of the API.
#![allow(clippy::cast_possible_truncation, clippy::missing_panics_doc)]

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

/// One piece of text: (x, y, size, bold, text).
pub type Piece<'a> = (f32, f32, f32, bool, &'a str);

/// A PDF whose pages hold `pages[i]` pieces; `image_bytes` > 0 adds an uncompressed image
/// of that many bytes to every page.
pub fn synthetic_pdf(pages: &[Vec<Piece<'_>>], image_bytes: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let regular = doc.add_object(
        dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"},
    );
    let bold = doc.add_object(
        dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold"},
    );
    let mut kids = Vec::new();
    for (n, pieces) in pages.iter().enumerate() {
        let mut ops = Vec::new();
        let mut xobjects = lopdf::Dictionary::new();
        if image_bytes > 0 {
            let side = ((image_bytes / 3) as f64).sqrt().ceil() as i64;
            let mut seed = 0x9E37_79B9u32.wrapping_add(n as u32);
            let data: Vec<u8> = (0..(side * side * 3))
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    (seed & 0xFF) as u8
                })
                .collect();
            let img = Stream::new(
                dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => side, "Height" => side,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8},
                data,
            )
            .with_compression(false);
            let img_id = doc.add_object(img);
            xobjects.set("Im1", img_id);
            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new(
                "cm",
                vec![
                    500.into(),
                    0.into(),
                    0.into(),
                    700.into(),
                    50.into(),
                    50.into(),
                ],
            ));
            ops.push(Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]));
            ops.push(Operation::new("Q", vec![]));
        }
        for (x, y, size, is_bold, text) in pieces {
            ops.push(Operation::new("BT", vec![]));
            let f = if *is_bold { "F2" } else { "F1" };
            ops.push(Operation::new(
                "Tf",
                vec![Object::Name(f.as_bytes().to_vec()), (*size).into()],
            ));
            ops.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
            ops.push(Operation::new("Tj", vec![Object::string_literal(*text)]));
            ops.push(Operation::new("ET", vec![]));
        }
        let content = Content { operations: ops };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {"Font" => dictionary! {"F1" => regular, "F2" => bold}, "XObject" => xobjects},
        });
        kids.push(page.into());
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => count}),
    );
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// A tiny ZIP writer (stored and deflated entries) for tests and fixtures.
pub fn synthetic_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(data).unwrap();
        let packed = enc.finish().unwrap();
        let local = out.len() as u32;
        let header = |sig: u32, central: bool| {
            let mut h = sig.to_le_bytes().to_vec();
            if central {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes());
            h.extend(0u16.to_le_bytes());
            h.extend(8u16.to_le_bytes());
            h.extend([0u8; 4]);
            h.extend(crc.sum().to_le_bytes());
            h.extend((packed.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            if central {
                h.extend([0u8; 10]);
                h.extend(local.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(header(0x0403_4b50u32, false));
        out.extend(&packed);
        central.extend(header(0x0201_4b50u32, true));
    }
    let at = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}
