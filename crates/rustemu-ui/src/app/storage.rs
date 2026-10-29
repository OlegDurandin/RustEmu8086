//! Persistence: settings, and in the browser drive C:, "my programs" and the editor, written
//! to eframe storage; closing with unsaved changes.

use super::*;

/// Storage key of the memory drive C: (see [`App::persist`]).
const DRIVE_KEY: &str = "dos_drive";

fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// What a storage key holds, for the user.
fn storage_key_name(key: &str) -> &'static str {
    match key {
        DRIVE_KEY => "drive C:",
        crate::programs::PROGRAMS_KEY => "my programs",
        crate::programs::DRAFT_KEY => "the editor text",
        _ => "settings",
    }
}

fn storage_full_text(what: &[&str]) -> String {
    format!(
        "The browser refused to keep {}: its storage for this site (about 5 MB) is full. \
         The last changes will be lost when the page is closed.\n\n\
         Free some space: delete the files you do not need in the \"drive C: files\" window or old \
         programs in Open, or download what you want to keep. Saving is tried again automatically.",
        what.join(", ")
    )
}

impl App {
    /// Restores the memory drive C: saved by [`App::persist`] (the browser keeps it in local
    /// storage).
    pub fn restore_drive(&mut self, storage: &dyn eframe::Storage) {
        use base64::Engine;
        if self.session.machine.dos_memory_drive().is_none() {
            return;
        }
        let saved = storage
            .get_string(DRIVE_KEY)
            .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
            .and_then(|b| rustemu_core::memdrive::MemDrive::from_bytes(&b));
        if let Some(drive) = saved {
            self.session.machine.set_dos_memory_drive(drive);
        }
    }

    /// Saves the settings (`eframe::App::save` of the front ends); in the browser also drive C:,
    /// "my programs" and the editor. Values that did not change since the last call are not
    /// written again.
    pub fn persist(&mut self, storage: &mut dyn eframe::Storage) {
        use base64::Engine;
        self.store_settings();
        let settings = ron::ser::to_string(&self.settings).unwrap_or_default();
        self.put(storage, eframe::APP_KEY, settings);
        if let Some(drive) = self.own_drive.as_ref().or(self.session.machine.dos_memory_drive()) {
            let drive = base64::engine::general_purpose::STANDARD.encode(drive.to_bytes());
            self.put(storage, DRIVE_KEY, drive);
        }
        if self.browser {
            self.persist_browser(storage);
        }
        if self.unsaved_keys.is_empty() {
            self.storage_warned = false;
        } else if !std::mem::replace(&mut self.storage_warned, true) {
            let what: Vec<&str> = self.unsaved_keys.iter().map(|k| storage_key_name(k)).collect();
            self.notice("Browser storage is full", &storage_full_text(&what));
        }
    }

    /// Some data could not be written by the last [`App::persist`].
    pub fn storage_failed(&self) -> bool {
        !self.unsaved_keys.is_empty()
    }

    /// Writes `value` under `key` unless it is what was written last time. The value is read back:
    /// a browser whose quota is full keeps the old one (eframe only logs the error). A failed
    /// write is tried again next time.
    pub(crate) fn put(&mut self, storage: &mut dyn eframe::Storage, key: &'static str, value: String) {
        let hash = text_hash(&value);
        if self.persisted.get(key) == Some(&hash) {
            return;
        }
        storage.set_string(key, value);
        if storage.get_string(key).is_some_and(|v| text_hash(&v) == hash) {
            self.persisted.insert(key, hash);
            self.unsaved_keys.remove(key);
        } else {
            self.persisted.remove(key);
            self.unsaved_keys.insert(key);
        }
    }

    /// Asks to save unsaved changes when the window is being closed.
    pub fn confirm_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && self.confirm_discard.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_discard = Some(PendingAction::Quit);
        }
    }
}
