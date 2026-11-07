//! macOS security-scoped bookmarks for the Mac App Store build (MAS1, B-190): the only `unsafe` here is the
//! two Foundation calls objc2 marks unsafe (resolving a bookmark writes through a `BOOL *`, and starting access
//! is unaudited); both are used exactly as Apple documents them. See [`crate::folder_access`] for the logic.

use crate::folder_access::{Resolved, SecurityScope};
use objc2::runtime::Bool;
use objc2_foundation::{
    NSData, NSString, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL,
};
use std::path::{Path, PathBuf};

/// `NSURL` bookmarks with `WithSecurityScope` (needs `com.apple.security.files.bookmarks.app-scope`).
#[derive(Debug, Clone, Copy, Default)]
pub struct MacScope;

fn describe(e: &objc2_foundation::NSError) -> String {
    e.localizedDescription().to_string()
}

impl SecurityScope for MacScope {
    fn create(&self, folder: &Path) -> Result<Vec<u8>, String> {
        let text = folder
            .to_str()
            .ok_or_else(|| "the folder path is not valid UTF-8".to_string())?;
        let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(text), true);
        url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::WithSecurityScope,
            None,
            None,
        )
        .map(|data| data.to_vec())
        .map_err(|e| describe(&e))
    }

    fn resolve_and_start(&self, bookmark: &[u8]) -> Result<Resolved, String> {
        let data = NSData::with_bytes(bookmark);
        let mut stale = Bool::NO;
        // SAFETY: `stale` is a valid, writable BOOL for the duration of the call.
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                NSURLBookmarkResolutionOptions::WithSecurityScope,
                None,
                &mut stale,
            )
        }
        .map_err(|e| describe(&e))?;
        let path = url
            .path()
            .map(|p| PathBuf::from(p.to_string()))
            .ok_or_else(|| "the bookmark does not name a file path".to_string())?;
        // SAFETY: a plain Objective-C message to a valid NSURL. Access is kept for the rest of the process on
        // purpose (the folder is used until the app quits), so there is no matching stop call.
        if !unsafe { url.startAccessingSecurityScopedResource() } {
            return Err("macOS refused access to the folder".into());
        }
        Ok(Resolved {
            path,
            stale: stale.as_bool(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real Foundation calls on a temporary folder (outside the sandbox the bookmark still round-trips).
    #[test]
    fn a_real_bookmark_round_trips_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().canonicalize().unwrap();
        let bookmark = MacScope.create(&folder).unwrap();
        assert!(!bookmark.is_empty());
        let resolved = MacScope.resolve_and_start(&bookmark).unwrap();
        assert_eq!(resolved.path.canonicalize().unwrap(), folder);
        assert!(MacScope.resolve_and_start(b"not a bookmark").is_err());
    }
}
