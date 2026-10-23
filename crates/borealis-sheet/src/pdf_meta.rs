//! Document metadata for PDF/UA-1 and PDF/A-2b: the XMP packet (`dc:title`, `pdfuaid:part`,
//! `pdfaid:part`/`conformance`, the producer the Info dictionary names) and the sRGB output
//! intent with its embedded ICC profile. Both are byte-for-byte the same for the same title.

use crate::pdf::Writer;
use std::fmt::Write as _;

/// The producer named in the Info dictionary and the XMP packet (they must match).
pub(crate) const PRODUCER: &str = "Borealis";

/// Compact sRGB IEC 61966-2.1 profile, ICC v2 (CC0, `icc/README.txt`).
const SRGB_ICC: &[u8] = include_bytes!("../icc/sRGB-v2-micro.icc");

/// The output condition the profile describes.
const SRGB_NAME: &str = "sRGB IEC61966-2.1";

/// XMP extension schema for `pdfuaid` (PDF/A-2 6.6.2.3: properties outside the predefined
/// schemas are described in the packet).
const PDFUA_SCHEMA: &str = "<pdfaExtension:schemas><rdf:Bag><rdf:li rdf:parseType=\"Resource\">\
<pdfaSchema:schema>PDF/UA Universal Accessibility Schema</pdfaSchema:schema>\
<pdfaSchema:namespaceURI>http://www.aiim.org/pdfua/ns/id/</pdfaSchema:namespaceURI>\
<pdfaSchema:prefix>pdfuaid</pdfaSchema:prefix>\
<pdfaSchema:property><rdf:Seq><rdf:li rdf:parseType=\"Resource\">\
<pdfaProperty:name>part</pdfaProperty:name>\
<pdfaProperty:valueType>Integer</pdfaProperty:valueType>\
<pdfaProperty:category>internal</pdfaProperty:category>\
<pdfaProperty:description>Indicates, which part of ISO 14289 standard is followed</pdfaProperty:description>\
</rdf:li></rdf:Seq></pdfaSchema:property>\
</rdf:li></rdf:Bag></pdfaExtension:schemas>";

/// `text` with the XML special characters escaped.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 has no other control characters.
            c if c.is_control() && !matches!(c, '\t' | '\n' | '\r') => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The XMP packet of a document titled `title`.
pub(crate) fn xmp_packet(title: &str) -> String {
    let mut x = String::from(
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" \
         xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" \
         xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\" \
         xmlns:pdfaExtension=\"http://www.aiim.org/pdfa/ns/extension/\" \
         xmlns:pdfaSchema=\"http://www.aiim.org/pdfa/ns/schema#\" \
         xmlns:pdfaProperty=\"http://www.aiim.org/pdfa/ns/property#\">\n",
    );
    let _ = writeln!(
        x,
        "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>",
        xml_escape(title)
    );
    let _ = writeln!(x, "<pdf:Producer>{PRODUCER}</pdf:Producer>");
    x.push_str("<pdfaid:part>2</pdfaid:part>\n<pdfaid:conformance>B</pdfaid:conformance>\n");
    x.push_str("<pdfuaid:part>1</pdfuaid:part>\n");
    x.push_str(PDFUA_SCHEMA);
    x.push_str("\n</rdf:Description>\n</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>");
    x
}

/// Write the metadata stream and the output intent; returns the catalog entries naming them.
pub(crate) fn write_metadata(w: &mut Writer, title: &str) -> String {
    let meta = w.alloc(1);
    // Uncompressed, so tools that scan for the packet find it.
    w.stream(
        meta,
        "/Type /Metadata /Subtype /XML",
        xmp_packet(title).as_bytes(),
    );
    let icc = w.alloc(1);
    w.flate_stream(icc, "/N 3", SRGB_ICC);
    let name = crate::pdf::pdf_string(SRGB_NAME);
    format!(
        "/Metadata {meta} 0 R /OutputIntents [ << /Type /OutputIntent /S /GTS_PDFA1 \
         /OutputConditionIdentifier {name} /Info {name} /DestOutputProfile {icc} 0 R >> ]"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packet_names_the_title_and_both_conformance_levels() {
        let x = xmp_packet("Ann & <Bo>");
        assert!(x.contains("<rdf:li xml:lang=\"x-default\">Ann &amp; &lt;Bo&gt;</rdf:li>"));
        assert!(x.contains("<pdfuaid:part>1</pdfuaid:part>"));
        assert!(x.contains("<pdfaid:part>2</pdfaid:part>"));
        assert!(x.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
        assert!(x.contains("<pdf:Producer>Borealis</pdf:Producer>"));
        assert_eq!(x, xmp_packet("Ann & <Bo>"), "deterministic");
    }

    #[test]
    fn the_profile_is_an_rgb_display_profile() {
        assert_eq!(&SRGB_ICC[12..16], b"mntr");
        assert_eq!(&SRGB_ICC[16..20], b"RGB ");
        assert_eq!(&SRGB_ICC[36..40], b"acsp");
    }
}
