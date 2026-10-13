//! Read files from the APK's `assets/` folder through `Context.getAssets()` over JNI.
//!
//! Android bundles resources inside the APK, so they are not filesystem paths. This crate holds the
//! only `unsafe` in the workspace (raw JavaVM/Context pointers from `ndk-context`). It is empty on
//! other platforms.

#[cfg(target_os = "android")]
mod apk;

#[cfg(target_os = "android")]
pub use apk::{list, read};
