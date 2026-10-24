//! Model files per tier and the consent-dialog text (spec sec. 7, 8; spike S0.2 handoff sec. 3).
//!
//! T2/T3 download the unmodified ggml-org `Q8_0` (310 MB). T1 downloads our re-quantised `Q6_K` body with a
//! `Q4_K` token embedding (211 MB): our plain `Q4_K_M` failed the cosine gate (0.966 < 0.99), the mix passes
//! (0.9926 at 768, 0.9931 at 256). The T1 file exists only on the Borealis mirror (GitHub Releases); its URL is
//! filled in when the mirror release is published (S3.2, B-46).

/// Model family id (vector headers, logs).
pub const MODEL_ID: &str = "embeddinggemma-2";
/// Native output dimensions.
pub const NATIVE_DIMS: usize = 768;
/// ggml-org/embeddinggemma-2-GGUF commit the Hugging Face URLs pin.
pub const HF_GGUF_COMMIT: &str = "58faf90940886df2d62be555c8765d00dac362c5";

/// One downloadable weights file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    /// Quantisation label (vector header, file name).
    pub quant: &'static str,
    /// File name under `<app-data>/models/`.
    pub file_name: &'static str,
    /// Exact size in bytes.
    pub bytes: u64,
    /// Lower-case hex SHA-256.
    pub sha256: &'static str,
    /// Pinned Hugging Face `resolve/<commit>/<file>` URL, when the file is published there.
    pub huggingface_url: Option<&'static str>,
    /// Pinned Borealis mirror URL (GitHub Releases), once published.
    pub mirror_url: Option<&'static str>,
    /// Size text for the consent dialog.
    pub size_text: &'static str,
}

/// ggml-org `Q8_0`, redistributed unmodified (T2, T3).
pub const Q8_0: ModelFile = ModelFile {
    quant: "Q8_0",
    file_name: "embeddinggemma-2-Q8_0.gguf",
    bytes: 309_855_456,
    sha256: "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135",
    huggingface_url: Some(
        "https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/58faf90940886df2d62be555c8765d00dac362c5/embeddinggemma-2-Q8_0.gguf",
    ),
    mirror_url: None,
    size_text: CONSENT_SIZE_Q8_0,
};

/// Borealis re-quantisation: `Q6_K` body, `Q4_K` token embedding (T1). Modified file (Apache-2.0 sec. 4b NOTICE).
pub const Q6_K_TE_Q4_K: ModelFile = ModelFile {
    quant: "Q6_K-te-Q4_K",
    file_name: "embeddinggemma-2-Q6_K-te-Q4_K.gguf",
    bytes: 211_161_312,
    sha256: "14b38ee0dc8556da0d420f8c920ecea9c0588a13ce440448697b7e0467413913",
    huggingface_url: None,
    mirror_url: None,
    size_text: CONSENT_SIZE_T1,
};

/// Consent size text of the full (Q8_0) model.
pub const CONSENT_SIZE_Q8_0: &str = "about 310 MB";
/// Consent size text of the T1 file (was "180 MB" for the failed Q4_K_M; the passing mix is 211 MB).
pub const CONSENT_SIZE_T1: &str = "210 MB";

/// Consent dialog title (spec sec. 8, exact).
pub const CONSENT_TITLE: &str = "Turn on semantic search?";
/// Consent dialog buttons (spec sec. 8, exact).
pub const CONSENT_ACCEPT: &str = "Download and turn on";
pub const CONSENT_DECLINE: &str = "Not now";

/// Consent dialog body for the file this device downloads (spec sec. 8 wording). The "; N MB on this device"
/// clause appears only when the device's file is not the full Q8_0; the charging clause only on mobile.
#[must_use]
pub fn consent_body(file: &ModelFile, mobile: bool) -> String {
    let size = if file.quant == Q8_0.quant {
        format!("Apache 2.0, {CONSENT_SIZE_Q8_0}")
    } else {
        format!(
            "Apache 2.0, {CONSENT_SIZE_Q8_0}; {} on this device",
            file.size_text
        )
    };
    let charging = if mobile {
        " and runs while your device is charging"
    } else {
        ""
    };
    format!(
        "Borealis will download the EmbeddingGemma 2 model ({size}) once, from Hugging Face or the Borealis \
         mirror. Search then also finds rules, lore and session notes by meaning, not only by the words you type. \
         Everything runs on this device; your searches and notes never leave it. Indexing takes a few \
         minutes{charging}. You can delete the model at any time in Settings."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_lower_hex_sha256() {
        for f in [Q8_0, Q6_K_TE_Q4_K] {
            assert_eq!(f.sha256.len(), 64, "{}", f.quant);
            assert!(f
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(f.quant.len() <= 16, "fits the vector header");
            assert!(f.file_name.starts_with("embeddinggemma-2-"));
        }
    }

    #[test]
    fn huggingface_url_pins_the_commit() {
        let url = Q8_0.huggingface_url.expect("url");
        assert!(url.contains(HF_GGUF_COMMIT));
        assert!(url.ends_with(Q8_0.file_name));
    }

    #[test]
    fn consent_sizes_match_the_files() {
        // Size text rounds the real byte counts (310 MB, 211 MB).
        assert_eq!(Q8_0.bytes / 1_000_000, 309);
        assert_eq!(Q6_K_TE_Q4_K.bytes / 1_000_000, 211);
        let t1 = consent_body(&Q6_K_TE_Q4_K, true);
        assert!(
            t1.contains("(Apache 2.0, about 310 MB; 210 MB on this device)"),
            "{t1}"
        );
        assert!(t1.contains("Indexing takes a few minutes and runs while your device is charging."));
        assert!(!t1.contains("180 MB"));
        let desktop = consent_body(&Q8_0, false);
        assert!(
            desktop.contains("(Apache 2.0, about 310 MB) once"),
            "{desktop}"
        );
        assert!(desktop.contains("Indexing takes a few minutes. You can delete"));
    }
}
