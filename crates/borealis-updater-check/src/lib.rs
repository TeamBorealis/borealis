//! Does the updater signing key match the committed public key? (B-203)
//!
//! The release workflow signs a probe file with `TAURI_SIGNING_PRIVATE_KEY` (`tauri signer sign --app-version`) and
//! checks it here against `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`, the way the app's updater
//! (tauri-plugin-updater, `requireSignedVersion: true`) checks a download: the public key and the signature are the
//! base64 of minisign's text files, the signature must verify, and its trusted comment must carry the version.
//! A key that does not match would ship a feed every client rejects, so the workflow stops before it builds.

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};

/// Why a probe does not verify. The messages name no key material.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CheckError {
    #[error("plugins.updater.pubkey is not a minisign public key: {0}")]
    PublicKey(String),
    #[error("the probe's .sig is not a minisign signature: {0}")]
    Signature(String),
    #[error(
        "the signature does not verify with plugins.updater.pubkey: the signing key is not the one the app trusts ({0})"
    )]
    Mismatch(String),
    #[error("the signature is for version {signed:?}, not {expected:?} (the app requires the signed version)")]
    Version { signed: String, expected: String },
}

/// Verify `data` against `signature_b64` with `pubkey_b64` and require the signed version `version`.
pub fn verify(
    pubkey_b64: &str,
    data: &[u8],
    signature_b64: &str,
    version: &str,
) -> Result<(), CheckError> {
    let pubkey_text = base64_text(pubkey_b64).map_err(CheckError::PublicKey)?;
    let pubkey =
        PublicKey::decode(&pubkey_text).map_err(|e| CheckError::PublicKey(e.to_string()))?;
    let signature_text = base64_text(signature_b64).map_err(CheckError::Signature)?;
    let signature =
        Signature::decode(&signature_text).map_err(|e| CheckError::Signature(e.to_string()))?;
    pubkey
        .verify(data, &signature, true)
        .map_err(|e| CheckError::Mismatch(e.to_string()))?;
    match signed_version(signature.trusted_comment()) {
        Some(signed) if signed.trim_start_matches('v') == version.trim_start_matches('v') => Ok(()),
        signed => Err(CheckError::Version {
            signed: signed.unwrap_or("none").to_string(),
            expected: version.to_string(),
        }),
    }
}

fn base64_text(encoded: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("not base64 ({e})"))?;
    String::from_utf8(bytes).map_err(|_| "not UTF-8 text".to_string())
}

/// The `version:` field of a trusted comment (`timestamp:...\tfile:...\tversion:1.4.0`), as the updater reads it.
fn signed_version(trusted_comment: &str) -> Option<&str> {
    trusted_comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    struct Key(minisign::KeyPair);

    impl Key {
        fn new() -> Self {
            Self(minisign::KeyPair::generate_unencrypted_keypair().expect("key pair"))
        }

        /// What `tauri signer generate` writes to the .key.pub file, base64: plugins.updater.pubkey.
        fn pubkey(&self) -> String {
            b64(self
                .0
                .pk
                .to_box()
                .expect("public key")
                .to_string()
                .as_bytes())
        }

        /// What `tauri signer sign --app-version` writes to the .sig file.
        fn sign(&self, data: &[u8], trusted: &str) -> String {
            let sig = minisign::sign(
                Some(&self.0.pk),
                &self.0.sk,
                std::io::Cursor::new(data),
                Some(trusted),
                Some("signature from tauri secret key"),
            )
            .expect("signature");
            b64(sig.to_string().as_bytes())
        }
    }

    const TRUSTED: &str = "timestamp:1\tfile:probe\tversion:1.4.0";

    #[test]
    fn the_matching_key_verifies() {
        let key = Key::new();
        let sig = key.sign(b"probe", TRUSTED);
        assert_eq!(verify(&key.pubkey(), b"probe", &sig, "1.4.0"), Ok(()));
        assert_eq!(
            verify(&key.pubkey(), b"probe", &format!("{sig}\n"), "v1.4.0"),
            Ok(())
        );
    }

    #[test]
    fn another_key_is_a_mismatch() {
        let (signer, committed) = (Key::new(), Key::new());
        let sig = signer.sign(b"probe", TRUSTED);
        let err = verify(&committed.pubkey(), b"probe", &sig, "1.4.0").unwrap_err();
        assert!(matches!(err, CheckError::Mismatch(_)), "{err}");
        assert!(err.to_string().contains("not the one the app trusts"));
    }

    #[test]
    fn changed_data_does_not_verify() {
        let key = Key::new();
        let sig = key.sign(b"probe", TRUSTED);
        let err = verify(&key.pubkey(), b"other", &sig, "1.4.0").unwrap_err();
        assert!(matches!(err, CheckError::Mismatch(_)), "{err}");
    }

    #[test]
    fn the_signed_version_must_match_and_be_present() {
        let key = Key::new();
        let other = key.sign(b"probe", "timestamp:1\tfile:probe\tversion:1.3.0");
        assert_eq!(
            verify(&key.pubkey(), b"probe", &other, "1.4.0"),
            Err(CheckError::Version {
                signed: "1.3.0".into(),
                expected: "1.4.0".into()
            })
        );
        let none = key.sign(b"probe", "timestamp:1\tfile:probe");
        assert!(matches!(
            verify(&key.pubkey(), b"probe", &none, "1.4.0"),
            Err(CheckError::Version { .. })
        ));
    }

    #[test]
    fn the_placeholder_or_garbage_is_not_a_key_or_signature() {
        let key = Key::new();
        let sig = key.sign(b"probe", TRUSTED);
        let placeholder = "PLACEHOLDER-NOT-A-KEY: replace with the updater public key";
        assert!(matches!(
            verify(placeholder, b"probe", &sig, "1.4.0"),
            Err(CheckError::PublicKey(_))
        ));
        assert!(matches!(
            verify(
                &b64(b"untrusted comment: x\nnot a key\n"),
                b"probe",
                &sig,
                "1.4.0"
            ),
            Err(CheckError::PublicKey(_))
        ));
        assert!(matches!(
            verify(&key.pubkey(), b"probe", "%%%", "1.4.0"),
            Err(CheckError::Signature(_))
        ));
        assert!(matches!(
            verify(&key.pubkey(), b"probe", &b64(&[0xff, 0xfe]), "1.4.0"),
            Err(CheckError::Signature(_))
        ));
    }
}
