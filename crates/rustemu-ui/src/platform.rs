//! Platform services that come from the operating system: file dialogs, reading the clipboard
//! on demand, opening web pages and the folder for user data.
//!
//! Desktop (macOS, also Windows / Linux) uses `rfd` for dialogs. The browser has no paths: files
//! are uploaded ([`request_upload`], picked up with [`take_uploads`]) and saved as downloads
//! ([`download`]). iPadOS and Android have no `rfd`; there the functions are stubs for now:
//! - iPadOS: `UIDocumentPickerViewController` for open / save, `UIPasteboard` for the clipboard,
//!   `UIApplication.open` for URLs, the app's Documents folder for data;
//! - Android: the Storage Access Framework (`ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT`)
//!   through JNI, `ClipboardManager`, an `ACTION_VIEW` intent, `Context.getFilesDir()`.

use std::path::{Path, PathBuf};

/// A file type for open / save dialogs: description and extensions without the dot.
pub type Filter<'a> = (&'a str, &'a [&'a str]);

/// Lets the user choose an existing file.
pub fn pick_file(filters: &[Filter], dir: Option<&Path>) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    {
        let mut dlg = rfd::FileDialog::new();
        for (name, exts) in filters {
            dlg = dlg.add_filter(*name, exts);
        }
        if let Some(d) = dir {
            dlg = dlg.set_directory(d);
        }
        dlg.pick_file()
    }
    #[cfg(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))]
    {
        let _ = (filters, dir);
        None
    }
}

/// Lets the user choose where to save a file (`name` is the suggested file name).
pub fn save_file(filters: &[Filter], dir: Option<&Path>, name: &str) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
    {
        let mut dlg = rfd::FileDialog::new().set_file_name(name);
        for (desc, exts) in filters {
            dlg = dlg.add_filter(*desc, exts);
        }
        if let Some(d) = dir {
            dlg = dlg.set_directory(d);
        }
        dlg.save_file()
    }
    #[cfg(any(target_os = "ios", target_os = "android", target_arch = "wasm32"))]
    {
        let _ = (filters, dir, name);
        None
    }
}

/// Whether file dialogs with paths are available (menus can hide what cannot work).
pub fn has_file_dialogs() -> bool {
    cfg!(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))
}

/// Whether files are opened by upload and saved by download (the browser).
pub fn is_web() -> bool {
    cfg!(target_arch = "wasm32")
}

/// What an uploaded file is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UploadFor {
    /// Assembly source for the editor, or an executable to load (by its extension).
    Open,
    /// Files for DOS programs on the memory drive C: (several at once).
    DriveFiles,
}

/// A file the user uploaded (the browser).
pub struct Upload {
    pub purpose: UploadFor,
    pub name: String,
    pub bytes: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static UPLOADS: std::cell::RefCell<Vec<Upload>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Asks the browser for a file; it arrives later through [`take_uploads`] (a repaint of `ctx` is
/// requested then). Elsewhere it does nothing.
pub fn request_upload(ctx: &egui::Context, purpose: UploadFor, filters: &[Filter]) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut dlg = rfd::AsyncFileDialog::new();
        for (name, exts) in filters {
            if !exts.contains(&"*") {
                dlg = dlg.add_filter(*name, exts);
            }
        }
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let files = match purpose {
                UploadFor::DriveFiles => dlg.pick_files().await.unwrap_or_default(),
                UploadFor::Open => dlg.pick_file().await.into_iter().collect(),
            };
            for file in files {
                let upload = Upload { purpose, name: file.file_name(), bytes: file.read().await };
                UPLOADS.with(|u| u.borrow_mut().push(upload));
            }
            ctx.request_repaint();
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (ctx, purpose, filters);
}

/// Files uploaded since the last call.
pub fn take_uploads() -> Vec<Upload> {
    #[cfg(target_arch = "wasm32")]
    {
        UPLOADS.with(|u| std::mem::take(&mut *u.borrow_mut()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

/// Saves `bytes` as a download named `name` (the browser); elsewhere it does nothing.
pub fn download(name: &str, bytes: &[u8]) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("application/octet-stream");
        let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options) else { return };
        let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
        if let Ok(a) = document.create_element("a") {
            let a: web_sys::HtmlAnchorElement = a.unchecked_into();
            a.set_href(&url);
            a.set_download(name);
            a.click();
        }
        // The download has started by now; free the blob a little later (Safari reads it lazily).
        let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
            let _ = web_sys::Url::revoke_object_url(&url);
        });
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (name, bytes);
}

/// Text on the clipboard, read on demand (Edit -> Paste, the emulator screen's paste button).
/// ⌘V / Ctrl+V do not need this: egui delivers the clipboard with the paste event.
pub fn read_clipboard() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("pbpaste").output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Opens a web page in the system browser (a new tab on the web).
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let _ = url;
}

/// Folder for files of unsaved programs (the DOS drive C:): `~/Documents/RustEmu8086/vdrive` on
/// the desktop; on iPadOS `HOME` is the app's sandbox, so this is inside its Documents folder. The
/// browser has no file system (DOS file functions report errors there).
pub fn data_dir() -> PathBuf {
    if cfg!(target_arch = "wasm32") {
        // std::env::temp_dir panics there.
        return PathBuf::from("/RustEmu8086");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Documents").join("RustEmu8086")
}
