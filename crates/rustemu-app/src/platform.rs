//! Platform services that come from the operating system: file dialogs, reading the clipboard
//! on demand, opening web pages and the folder for user data.
//!
//! Desktop (macOS, also Windows / Linux) uses `rfd` for dialogs. iPadOS and Android have no `rfd`;
//! there the functions are stubs for now:
//! - iPadOS: `UIDocumentPickerViewController` for open / save, `UIPasteboard` for the clipboard,
//!   `UIApplication.open` for URLs, the app's Documents folder for data;
//! - Android: the Storage Access Framework (`ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT`)
//!   through JNI, `ClipboardManager`, an `ACTION_VIEW` intent, `Context.getFilesDir()`.

use std::path::{Path, PathBuf};

/// A file type for open / save dialogs: description and extensions without the dot.
pub type Filter<'a> = (&'a str, &'a [&'a str]);

/// Lets the user choose an existing file.
pub fn pick_file(filters: &[Filter], dir: Option<&Path>) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = (filters, dir);
        None
    }
}

/// Lets the user choose where to save a file (`name` is the suggested file name).
pub fn save_file(filters: &[Filter], dir: Option<&Path>, name: &str) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = (filters, dir, name);
        None
    }
}

/// Whether file dialogs are available (menus can hide what cannot work).
pub fn has_file_dialogs() -> bool {
    cfg!(not(any(target_os = "ios", target_os = "android")))
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

/// Opens a web page in the system browser.
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let _ = url;
}

/// Folder for files of unsaved programs (the DOS drive C:): `~/Documents/RustEmu8086/vdrive` on
/// the desktop; on iPadOS `HOME` is the app's sandbox, so this is inside its Documents folder.
pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Documents").join("RustEmu8086")
}
