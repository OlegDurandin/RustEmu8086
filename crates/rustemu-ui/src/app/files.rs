//! Files: new from a template, open, save, uploads, examples.

use super::*;

impl App {
    pub fn new_from_template(&mut self, i: usize) {
        self.new_from(crate::examples::TEMPLATES.get(i).map(|t| t.1).unwrap_or(""));
    }

    /// File -> New with "NASM syntax": COM (0), BIN (2) or boot sector (3) template in NASM.
    pub fn new_from_nasm_template(&mut self, i: usize) {
        let t = match i {
            0 => crate::examples::NASM_TEMPLATES[0].1,
            2 => crate::examples::NASM_TEMPLATES[1].1,
            3 => crate::examples::NASM_TEMPLATES[2].1,
            _ => "bits 16\n",
        };
        self.new_from(t);
    }

    fn new_from(&mut self, template: &str) {
        self.store_session();
        self.path = None;
        self.doc_name = None;
        self.opened_from_link = false;
        self.program = None;
        self.example_key = None;
        self.clear_session();
        self.bookmarks.clear();
        self.text = template.to_string();
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.clear_errors();
        self.status = "New file".into();
    }

    pub fn open_dialog(&mut self) {
        if self.browser {
            self.windows.programs = true;
            return;
        }
        let dir = self.path.as_ref().and_then(|p| p.parent());
        let filters: [crate::platform::Filter; 2] =
            [("Assembly", &["asm", "nasm", "inc", "txt"]), ("All files", &["*"])];
        if let Some(p) = crate::platform::pick_file(&filters, dir) {
            let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "com" | "exe" | "bin" | "boot") {
                self.load_binary(&p);
            } else {
                self.open_path(&p);
            }
        }
    }

    pub fn open_path(&mut self, p: &Path) {
        match std::fs::read(p) {
            Ok(bytes) => {
                self.store_session();
                self.text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                self.saved_text = self.text.clone();
                self.path = Some(p.to_path_buf());
                self.doc_name = None;
                self.opened_from_link = false;
                self.program = None;
                self.messages.clear();
                self.clear_errors();
                self.add_recent(p);
                self.settings.last_file = Some(p.to_path_buf());
                self.example_key = None;
                self.restore_session();
                self.status = format!("Opened {}", p.display());
            }
            Err(e) => self.notice("Open", &format!("Cannot open {}:\n{e}", p.display())),
        }
    }

    /// Opens a file the user uploaded (the browser): an executable is loaded into the emulator,
    /// anything else goes into the editor.
    fn open_upload(&mut self, u: crate::platform::Upload) {
        let ext = Path::new(&u.name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "com" | "exe" | "bin" | "boot" | "img") {
            match self.session.load_named(&u.name, u.bytes) {
                Ok(()) => self.after_binary_load(),
                Err(e) => self.notice("Load", &e),
            }
            return;
        }
        let text = String::from_utf8_lossy(&u.bytes).replace("\r\n", "\n");
        self.import_program(&u.name, text);
    }

    /// Asks the browser for the requested file and opens the files that arrived.
    pub(super) fn handle_uploads(&mut self, ctx: &egui::Context) {
        use crate::platform::UploadFor;
        if let Some(purpose) = self.pending_upload.take() {
            let filters: &[crate::platform::Filter] = match purpose {
                UploadFor::Open => &[
                    ("Assembly or executable", &["asm", "nasm", "inc", "txt", "com", "exe", "bin"]),
                    ("All files", &["*"]),
                ],
                UploadFor::DriveFiles => &[],
            };
            crate::platform::request_upload(ctx, purpose, filters);
        }
        for u in crate::platform::take_uploads() {
            match u.purpose {
                UploadFor::Open => self.open_upload(u),
                UploadFor::DriveFiles => self.add_drive_file(&u.name, u.bytes),
            }
        }
    }

    /// Puts a file on the memory drive C: (an upload for DOS programs).
    pub fn add_drive_file(&mut self, name: &str, bytes: Vec<u8>) {
        let Some(drive) = self.session.machine.dos_memory_drive_mut() else { return };
        match drive.write(name, bytes) {
            Ok(key) => self.status = format!("C:\\{} added to drive C:", key.replace('/', "\\")),
            Err(e) => self.notice("Drive C:", &e),
        }
    }

    fn add_recent(&mut self, p: &Path) {
        self.settings.recent.retain(|r| r != p);
        self.settings.recent.insert(0, p.to_path_buf());
        self.settings.recent.truncate(10);
    }

    /// Saves the file (in the browser: as a program of "my programs"; a new one asks for its name
    /// first and returns `false`).
    pub fn save(&mut self) -> bool {
        if self.browser {
            match self.program.clone() {
                Some(name) => {
                    self.save_program(&name);
                    return true;
                }
                None => return self.save_as(),
            }
        }
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        if self.browser {
            self.save_name = Some(crate::programs::SaveName { text: self.file_name(), then: None });
            return false;
        }
        let dir = self.path.as_ref().and_then(|p| p.parent());
        match crate::platform::save_file(&[("Assembly", &["asm"])], dir, &self.file_name()) {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    /// Saves the editor text as a download (the browser).
    pub fn download_source(&mut self) {
        let name = self.file_name();
        crate::platform::download(&name, self.text.as_bytes());
        self.status = format!("Downloaded {name}");
    }

    fn save_to(&mut self, p: &Path) -> bool {
        match std::fs::write(p, &self.text) {
            Ok(()) => {
                self.example_key = None;
                self.saved_text = self.text.clone();
                self.path = Some(p.to_path_buf());
                self.add_recent(p);
                self.settings.last_file = Some(p.to_path_buf());
                self.status = format!("Saved {}", p.display());
                true
            }
            Err(e) => {
                self.notice("Save", &format!("Cannot save {}:\n{e}", p.display()));
                false
            }
        }
    }

    pub fn load_example(&mut self, i: usize) {
        let (name, src) = EXAMPLES[i];
        self.load_example_text(name, src);
    }

    /// Opens `src` as the built-in example `name` (it has no file; Save asks for one).
    pub fn load_example_text(&mut self, name: &str, src: &str) {
        self.store_session();
        self.path = None;
        self.doc_name = None;
        self.opened_from_link = false;
        self.program = None;
        self.example_key = Some(format!("example:{name}"));
        // Emu8086's files have DOS line ends.
        self.text = src.replace("\r\n", "\n");
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.clear_errors();
        self.restore_session();
        self.status = format!("Example: {name}");
    }
}
