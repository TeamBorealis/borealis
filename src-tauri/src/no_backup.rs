//! Device-local state never goes into a phone backup (device-sync threat model, "Cloned identity";
//! local-search-spec S1.5: the search index is derived data).
//!
//! * [`NO_BACKUP_DIRS`] under the app data folder: `link/` (paired-device registry), `sync/` (sync
//!   state, staging, incoming files and the trash), `search/` (indexes and extracted text) and `models/` (the
//!   semantic search model: 210-310 MB, downloaded again on request, local-search-spec sec. 8).
//! * Android: `res/xml/data_extraction_rules.xml` (cloud backup and device-to-device transfer,
//!   Android 12+) and `res/xml/backup_rules.xml` (`fullBackupContent`, Android 11 and older)
//!   exclude the same folders plus the secure-store preferences file
//!   ([`ANDROID_KEYRING_PREFS`]); the app data folder is the app's `dataDir` (backup domain `root`).
//! * iOS: [`exclude_from_backup`] sets `NSURLIsExcludedFromBackupKey` on each folder at start-up;
//!   the device identity and pair keys are keychain items that never leave the device
//!   (`ThisDeviceOnly`, borealis-link `store::platform`).

use std::path::Path;

/// Folders of the app data folder that never go into a backup.
pub const NO_BACKUP_DIRS: &[&str] = &["link", "sync", "search", "models"];

/// The SharedPreferences file of the Android secure store (`android-native-keyring-store`: store
/// `default`, file `keyring-default`): its values are encrypted with a Keystore key that a restore
/// cannot bring along, so it is useless in a backup and stays out.
pub const ANDROID_KEYRING_PREFS: &str = "keyring-default.xml";

/// Create each [`NO_BACKUP_DIRS`] folder under `data` and mark it excluded from backups. Failures
/// are logged, never fatal (the app works the same; only the backup would be larger).
pub fn exclude_from_backup(data: &Path) {
    for dir in NO_BACKUP_DIRS {
        let path = data.join(dir);
        let marked = std::fs::create_dir_all(&path).and_then(|()| mark(&path));
        if let Err(e) = marked {
            tracing::warn!("could not keep {dir}/ out of backups: {e}");
        }
    }
}

#[cfg(target_vendor = "apple")]
use apple::mark;

#[cfg(not(target_vendor = "apple"))]
fn mark(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// `CFURLSetResourcePropertyForKey(kCFURLIsExcludedFromBackupKey, true)`: the documented way to
/// keep an item out of iCloud and computer backups on iOS (also understood by macOS).
#[cfg(target_vendor = "apple")]
#[allow(unsafe_code)]
mod apple {
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    type CFTypeRef = *const c_void;
    type CFIndex = isize;
    type Boolean = u8;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFURLIsExcludedFromBackupKey: CFTypeRef;
        static kCFBooleanTrue: CFTypeRef;
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: CFTypeRef,
            buffer: *const u8,
            len: CFIndex,
            is_directory: Boolean,
        ) -> CFTypeRef;
        fn CFURLSetResourcePropertyForKey(
            url: CFTypeRef,
            key: CFTypeRef,
            value: CFTypeRef,
            error: *mut CFTypeRef,
        ) -> Boolean;
        #[cfg(test)]
        fn CFURLCopyResourcePropertyForKey(
            url: CFTypeRef,
            key: CFTypeRef,
            value: *mut CFTypeRef,
            error: *mut CFTypeRef,
        ) -> Boolean;
        fn CFRelease(cf: CFTypeRef);
    }

    /// An owned CF object, released on drop.
    struct Owned(CFTypeRef);

    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: `self.0` is a CF object this module created or copied (+1 retain).
                unsafe { CFRelease(self.0) };
            }
        }
    }

    fn url(path: &Path) -> std::io::Result<Owned> {
        let bytes = path.as_os_str().as_bytes();
        let len =
            CFIndex::try_from(bytes.len()).map_err(|_| std::io::Error::other("path too long"))?;
        // SAFETY: `bytes` is valid for `len` bytes; a null allocator is the default allocator.
        let url = unsafe {
            CFURLCreateFromFileSystemRepresentation(std::ptr::null(), bytes.as_ptr(), len, 1)
        };
        if url.is_null() {
            return Err(std::io::Error::other("not a file URL"));
        }
        Ok(Owned(url))
    }

    pub(super) fn mark(path: &Path) -> std::io::Result<()> {
        let url = url(path)?;
        let mut error: CFTypeRef = std::ptr::null();
        // SAFETY: `url` is a live CFURL; the key and value are CoreFoundation constants; `error`
        // receives a +1 CFError on failure, released by `Owned`.
        let ok = unsafe {
            CFURLSetResourcePropertyForKey(
                url.0,
                kCFURLIsExcludedFromBackupKey,
                kCFBooleanTrue,
                &mut error,
            )
        };
        let _error = Owned(error);
        if ok == 0 {
            return Err(std::io::Error::other("the system refused the backup flag"));
        }
        Ok(())
    }

    /// Whether `path` is excluded from backups (tests).
    #[cfg(test)]
    pub(super) fn is_marked(path: &Path) -> bool {
        let Ok(url) = url(path) else { return false };
        let mut value: CFTypeRef = std::ptr::null();
        let mut error: CFTypeRef = std::ptr::null();
        // SAFETY: as in `mark`; `value` receives a +1 CFBoolean on success.
        let ok = unsafe {
            CFURLCopyResourcePropertyForKey(
                url.0,
                kCFURLIsExcludedFromBackupKey,
                &mut value,
                &mut error,
            )
        };
        let (_error, value) = (Owned(error), Owned(value));
        // CFBoolean values are singletons, so pointer equality is the comparison.
        // SAFETY: reading an immutable CoreFoundation constant.
        ok != 0 && value.0 == unsafe { kCFBooleanTrue }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_every_no_backup_folder() {
        let data = tempfile::tempdir().unwrap();
        exclude_from_backup(data.path());
        for dir in NO_BACKUP_DIRS {
            assert!(data.path().join(dir).is_dir(), "{dir}");
        }
    }

    #[cfg(target_vendor = "apple")]
    #[test]
    fn marks_the_folders_excluded_from_backup() {
        let data = tempfile::tempdir().unwrap();
        let other = data.path().join("characters");
        std::fs::create_dir_all(&other).unwrap();
        exclude_from_backup(data.path());
        for dir in NO_BACKUP_DIRS {
            assert!(apple::is_marked(&data.path().join(dir)), "{dir}");
        }
        assert!(!apple::is_marked(&other), "user data stays in backups");
    }

    const BACKUP_RULES: &str = include_str!("../gen/android/app/src/main/res/xml/backup_rules.xml");
    const EXTRACTION_RULES: &str =
        include_str!("../gen/android/app/src/main/res/xml/data_extraction_rules.xml");
    const MANIFEST: &str = include_str!("../gen/android/app/src/main/AndroidManifest.xml");

    fn excludes(xml: &str) -> Vec<(String, String)> {
        xml.split("<exclude ")
            .skip(1)
            .map(|e| {
                let attr = |name: &str| {
                    let tag = format!("{name}=\"");
                    e.split(&tag)
                        .nth(1)
                        .and_then(|v| v.split('"').next())
                        .unwrap_or_default()
                        .to_string()
                };
                (attr("domain"), attr("path"))
            })
            .collect()
    }

    fn assert_device_state_excluded(rules: &[(String, String)], what: &str) {
        for dir in NO_BACKUP_DIRS {
            assert!(
                rules.contains(&("root".into(), (*dir).into())),
                "{what}: {dir}/ is not excluded: {rules:?}"
            );
        }
        assert!(
            rules.contains(&("sharedpref".into(), ANDROID_KEYRING_PREFS.into())),
            "{what}: the secure-store preferences are not excluded"
        );
        assert!(
            rules.iter().all(|(_, p)| !p.is_empty() && p != "."),
            "{what}: excludes only named paths, never everything: {rules:?}"
        );
    }

    #[test]
    fn android_backups_leave_out_device_state() {
        assert!(MANIFEST.contains(r#"android:dataExtractionRules="@xml/data_extraction_rules""#));
        assert!(MANIFEST.contains(r#"android:fullBackupContent="@xml/backup_rules""#));
        assert!(
            !MANIFEST.contains(r#"android:allowBackup="false""#),
            "characters stay backed up"
        );
        assert_device_state_excluded(&excludes(BACKUP_RULES), "fullBackupContent");
        for section in ["<cloud-backup>", "<device-transfer>"] {
            let start = EXTRACTION_RULES
                .find(section)
                .unwrap_or_else(|| panic!("{section} missing"));
            let rest = &EXTRACTION_RULES[start..];
            let end = rest
                .find(&section.replace('<', "</"))
                .unwrap_or_else(|| panic!("{section} not closed"));
            assert_device_state_excluded(&excludes(&rest[..end]), section);
        }
    }
}
