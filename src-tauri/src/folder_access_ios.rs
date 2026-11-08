//! iOS and iPadOS folder access for the Obsidian vault (B-221, decision U17b). See [`crate::folder_access`] for the
//! bookmark logic this shares with the Mac App Store build.
//!
//! - [`pick_folder`] shows the system document picker for folders (Files: iCloud Drive, On My iPad / iPhone, the
//!   Obsidian folder, other file providers). While the picked folder's security scope is open, a bookmark is made;
//!   it waits in [`pending`] until the vault setting is saved and [`IosScope::create`] stores it.
//! - [`start`] resolves a stored bookmark and starts access; the returned [`ScopedUrl`] stops it when dropped, so access
//!   is held only around each vault operation.
//! - [`coordinated_read`] and [`coordinated_write`] go through `NSFileCoordinator`, so iCloud Drive and other providers
//!   download a file before it is read and see each save as one change. [`start_download`] asks iCloud Drive for a placeholder's file.
//! - [`trash`] moves a deleted note to the provider's Recently Deleted when it has one.
//!
//! The `unsafe` here is the Objective-C runtime: the picker delegate class, the view controller pointer Tauri hands over
//! on the main thread, and the Foundation calls objc2 marks unsafe, all used as Apple documents them.

use crate::folder_access::{Resolved, SecurityScope};
use block2::StackBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly, Message};
use objc2_foundation::{
    NSArray, NSData, NSError, NSFileCoordinator, NSFileCoordinatorReadingOptions,
    NSFileCoordinatorWritingOptions, NSFileManager, NSObject, NSObjectProtocol, NSString,
    NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL,
};
// The mode belongs to the type-identifier initialiser, deprecated in favour of UniformTypeIdentifiers (see `present`).
#[allow(deprecated)]
use objc2_ui_kit::UIDocumentPickerMode;
use objc2_ui_kit::{
    UIAdaptivePresentationControllerDelegate, UIDocumentPickerDelegate,
    UIDocumentPickerViewController, UIPresentationController, UIViewController,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::{mpsc, Mutex, OnceLock};
use tauri::{AppHandle, Manager, Runtime};

/// The uniform type the picker offers: folders only.
const FOLDER_TYPE: &str = "public.folder";

fn describe(e: &NSError) -> String {
    e.localizedDescription().to_string()
}

/// A `file://` URL for `path`.
fn file_url(path: &Path) -> Result<Retained<NSURL>, String> {
    let text = path
        .to_str()
        .ok_or_else(|| "the path is not valid UTF-8".to_string())?;
    Ok(NSURL::fileURLWithPath(&NSString::from_str(text)))
}

fn path_of(url: &NSURL) -> Option<PathBuf> {
    url.path().map(|p| PathBuf::from(p.to_string()))
}

/// A bookmark for `url` (its scope must be open). iOS has no explicit security-scope option: a bookmark made from a
/// security-scoped URL carries the scope.
fn bookmark_of(url: &NSURL) -> Result<Vec<u8>, String> {
    url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
        NSURLBookmarkCreationOptions::empty(),
        None,
        None,
    )
    .map(|data| data.to_vec())
    .map_err(|e| describe(&e))
}

/// Bookmarks of folders the picker returned, waiting for the vault setting to be saved: path -> bookmark.
fn pending_map() -> &'static Mutex<HashMap<PathBuf, Vec<u8>>> {
    static PENDING: OnceLock<Mutex<HashMap<PathBuf, Vec<u8>>>> = OnceLock::new();
    PENDING.get_or_init(Mutex::default)
}

/// The keys a folder may be named by: as picked, and canonical (`/var/...` is `/private/var/...`).
fn keys(path: &Path) -> Vec<PathBuf> {
    let mut out = vec![path.to_path_buf()];
    out.extend(std::fs::canonicalize(path).ok());
    out
}

/// The bookmark of a folder the picker just returned, if `path` is one.
#[must_use]
pub fn pending(path: &Path) -> Option<Vec<u8>> {
    let map = pending_map().lock().ok()?;
    keys(path).iter().find_map(|k| map.get(k).cloned())
}

fn take_pending(path: &Path) -> Option<Vec<u8>> {
    let mut map = pending_map().lock().ok()?;
    let found = keys(path).iter().find_map(|k| map.get(k).cloned());
    if found.is_some() {
        map.retain(|_, b| Some(&*b) != found.as_ref());
    }
    found
}

/// A started security scope, stopped when dropped.
pub struct ScopedUrl {
    url: Retained<NSURL>,
    started: bool,
}

impl Drop for ScopedUrl {
    fn drop(&mut self) {
        if self.started {
            // SAFETY: a plain message to a valid NSURL, balancing the start in `start`.
            unsafe { self.url.stopAccessingSecurityScopedResource() };
        }
    }
}

/// Resolve `bookmark` and start access to its folder until the returned scope is dropped.
///
/// # Errors
/// The OS message when the bookmark no longer resolves.
pub fn start(bookmark: &[u8]) -> Result<(Resolved, ScopedUrl), String> {
    let data = NSData::with_bytes(bookmark);
    let mut stale = Bool::NO;
    // SAFETY: `stale` is a valid, writable BOOL for the duration of the call.
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data,
            NSURLBookmarkResolutionOptions::empty(),
            None,
            &mut stale,
        )
    }
    .map_err(|e| describe(&e))?;
    let path = path_of(&url).ok_or_else(|| "the bookmark does not name a file path".to_string())?;
    // SAFETY: a plain message to a valid NSURL. A folder that needs no scope (inside this app) answers NO and is
    // still readable, so NO is not an error here.
    let started = unsafe { url.startAccessingSecurityScopedResource() };
    Ok((
        Resolved {
            path,
            stale: stale.as_bool(),
        },
        ScopedUrl { url, started },
    ))
}

/// The document picker's bookmarks.
#[derive(Debug, Clone, Copy, Default)]
pub struct IosScope;

impl SecurityScope for IosScope {
    fn create(&self, folder: &Path) -> Result<Vec<u8>, String> {
        if let Some(bookmark) = take_pending(folder) {
            return Ok(bookmark);
        }
        bookmark_of(&*file_url(folder)?)
    }

    fn resolve_and_start(&self, bookmark: &[u8]) -> Result<Resolved, String> {
        let (resolved, scope) = start(bookmark)?;
        // This trait's contract: access stays for the rest of the process.
        std::mem::forget(scope);
        Ok(resolved)
    }
}

/// What the picker returned.
struct Picked {
    path: PathBuf,
    bookmark: Vec<u8>,
}

/// While its scope is open: the picked folder's path and bookmark.
fn picked(url: &NSURL) -> Option<Picked> {
    // SAFETY: plain messages to a valid NSURL; every start is balanced by the stop below.
    let started = unsafe { url.startAccessingSecurityScopedResource() };
    let result = path_of(url).and_then(|path| match bookmark_of(url) {
        Ok(bookmark) => Some(Picked { path, bookmark }),
        Err(e) => {
            tracing::warn!("vault: no bookmark for the picked folder: {e}");
            None
        }
    });
    if let Some(p) = &result {
        // Remember the canonical name too while the folder can be read.
        if let Ok(mut map) = pending_map().lock() {
            for key in keys(&p.path) {
                map.insert(key, p.bookmark.clone());
            }
        }
    }
    if started {
        // SAFETY: balances the start above.
        unsafe { url.stopAccessingSecurityScopedResource() };
    }
    result
}

/// The picker delegate's state: where to send the result (once).
struct Waiter {
    tx: RefCell<Option<mpsc::Sender<Option<PathBuf>>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the class is used on the main thread only.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BorealisVaultPickerDelegate"]
    #[ivars = Waiter]
    struct PickerDelegate;

    // SAFETY: NSObjectProtocol has no extra requirements.
    unsafe impl NSObjectProtocol for PickerDelegate {}

    // SAFETY: the methods match the protocol's signatures.
    unsafe impl UIDocumentPickerDelegate for PickerDelegate {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn did_pick(&self, _picker: &UIDocumentPickerViewController, urls: &NSArray<NSURL>) {
            let chosen = urls.firstObject().and_then(|u| picked(&u)).map(|p| p.path);
            self.finish(chosen);
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn was_cancelled(&self, _picker: &UIDocumentPickerViewController) {
            self.finish(None);
        }
    }

    // SAFETY: the method matches the protocol's signature.
    unsafe impl UIAdaptivePresentationControllerDelegate for PickerDelegate {
        /// The sheet was swiped down or tapped away: that is a cancel too, or the pick would wait forever.
        #[unsafe(method(presentationControllerDidDismiss:))]
        fn did_dismiss(&self, _controller: &UIPresentationController) {
            self.finish(None);
        }
    }
);

impl PickerDelegate {
    fn new(mtm: MainThreadMarker, tx: mpsc::Sender<Option<PathBuf>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Waiter {
            tx: RefCell::new(Some(tx)),
        });
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }

    fn finish(&self, chosen: Option<PathBuf>) {
        if let Some(tx) = self.ivars().tx.borrow_mut().take() {
            let _ = tx.send(chosen);
        }
    }
}

thread_local! {
    /// The delegate of the picker on screen (the picker holds it weakly). Replaced by the next pick.
    static ACTIVE: RefCell<Option<Retained<PickerDelegate>>> = const { RefCell::new(None) };
}

/// On the main thread: show the folder picker over `root` (the web view's view controller).
fn present(mtm: MainThreadMarker, root: &UIViewController, tx: mpsc::Sender<Option<PathBuf>>) {
    // Present over whatever is already presented, or UIKit refuses (and the pick would never answer).
    let mut top = root.retain();
    while let Some(next) = top.presentedViewController() {
        top = next;
    }
    let types = NSArray::from_retained_slice(&[NSString::from_str(FOLDER_TYPE)]);
    // `initForOpeningContentTypes:` needs UniformTypeIdentifiers; the type-identifier form is the same picker.
    #[allow(deprecated)]
    let picker = UIDocumentPickerViewController::initWithDocumentTypes_inMode(
        mtm.alloc(),
        &types,
        UIDocumentPickerMode::Open,
    );
    picker.setAllowsMultipleSelection(false);
    let delegate = PickerDelegate::new(mtm, tx);
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    if let Some(sheet) = picker.presentationController() {
        // SAFETY: the delegate outlives the sheet (it is kept in `ACTIVE` until the next pick).
        unsafe { sheet.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
    }
    ACTIVE.with(|a| *a.borrow_mut() = Some(delegate));
    top.presentViewController_animated_completion(&picker, true, None);
}

/// Show the folder picker and wait for the user. `None` when they cancel. Call off the main thread.
///
/// # Errors
/// When there is no window to show it in.
pub fn pick_folder<R: Runtime>(app: &AppHandle<R>) -> Result<Option<PathBuf>, String> {
    let window = app
        .webview_windows()
        .into_values()
        .next()
        .ok_or_else(|| "no window".to_string())?;
    let (tx, rx) = mpsc::channel();
    window
        .with_webview(move |webview| {
            let Some(mtm) = MainThreadMarker::new() else {
                let _ = tx.send(None);
                return;
            };
            let controller = webview.view_controller().cast::<UIViewController>();
            // SAFETY: Tauri hands over the web view's live UIViewController on the main thread.
            match unsafe { controller.as_ref() } {
                Some(root) => present(mtm, root, tx),
                None => {
                    let _ = tx.send(None);
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(rx.recv().ok().flatten())
}

/// Read `path` inside a coordinated read (see the module docs). A file that cannot be coordinated is read directly.
///
/// # Errors
/// What `read` returns, or the coordinator's reason (the provider could not bring the file down).
pub fn coordinated_read<T>(
    path: &Path,
    read: impl FnOnce(&Path) -> std::io::Result<T>,
) -> std::io::Result<T> {
    coordinate(path, false, read)
}

/// Write `path` inside a coordinated write (for replacing), so iCloud Drive and other providers, and any app presenting the
/// file, see one finished change.
///
/// # Errors
/// What `write` returns, or the coordinator's reason.
pub fn coordinated_write<T>(
    path: &Path,
    write: impl FnOnce(&Path) -> std::io::Result<T>,
) -> std::io::Result<T> {
    coordinate(path, true, write)
}

fn coordinate<T>(
    path: &Path,
    writing: bool,
    access: impl FnOnce(&Path) -> std::io::Result<T>,
) -> std::io::Result<T> {
    let Ok(url) = file_url(path) else {
        return access(path);
    };
    let accessor = RefCell::new(Some(access));
    let out: RefCell<Option<std::io::Result<T>>> = RefCell::new(None);
    let block = StackBlock::new(|at: NonNull<NSURL>| {
        // SAFETY: the coordinator passes a valid NSURL for the duration of the call.
        let at = path_of(unsafe { at.as_ref() }).unwrap_or_else(|| path.to_path_buf());
        if let Some(f) = accessor.borrow_mut().take() {
            *out.borrow_mut() = Some(f(&at));
        }
    });
    let coordinator = NSFileCoordinator::new();
    let mut error: Option<Retained<NSError>> = None;
    if writing {
        coordinator.coordinateWritingItemAtURL_options_error_byAccessor(
            &url,
            NSFileCoordinatorWritingOptions::ForReplacing,
            Some(&mut error),
            &block,
        );
    } else {
        coordinator.coordinateReadingItemAtURL_options_error_byAccessor(
            &url,
            NSFileCoordinatorReadingOptions::empty(),
            Some(&mut error),
            &block,
        );
    }
    if let Some(result) = out.into_inner() {
        return result;
    }
    Err(std::io::Error::other(error.map_or_else(
        || "the file could not be reached".to_string(),
        |e| describe(&e),
    )))
}

/// Ask iCloud Drive to download the file `path` (a placeholder stands for it now). Logged, never fatal.
pub fn start_download(path: &Path) {
    let Ok(url) = file_url(path) else {
        return;
    };
    if let Err(e) = NSFileManager::defaultManager().startDownloadingUbiquitousItemAtURL_error(&url)
    {
        tracing::warn!(
            "vault: iCloud Drive did not start a download: {}",
            describe(&e)
        );
    }
}

/// Move `path` to the file provider's trash (Recently Deleted), where it has one.
///
/// # Errors
/// The OS message when there is no trash there.
pub fn trash(path: &Path) -> Result<(), String> {
    let url = file_url(path)?;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| describe(&e))
}
