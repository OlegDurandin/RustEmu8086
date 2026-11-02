//! Files: new from a template, open, save, uploads, examples.

use super::*;
use crate::tools::Tool;

/// Session key prefix of the built-in examples. The examples were ours before they became
/// Emu8086's (some with the same names): sessions saved under the former "example:" belong to
/// other texts and are not restored.
pub(crate) const EXAMPLE_KEY: &str = "emu8086-example:";
/// Session key prefix of the built-in MIPS examples.
pub(crate) const MIPS_EXAMPLE_KEY: &str = "mips-example:";

impl App {
    /// A new file from the `i`-th code template.
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

    /// File -> New in the MIPS workspace: the MIPS template (it has no choices).
    pub fn new_mips_file(&mut self) {
        self.new_from(crate::examples::MIPS_TEMPLATE);
    }

    fn new_from(&mut self, template: &str) {
        self.store_session();
        self.doc.path = None;
        self.doc.name = None;
        self.doc.from_link = false;
        self.doc.program = None;
        self.doc.example_key = None;
        self.clear_session();
        self.editor.bookmarks.clear();
        self.doc.text = template.to_string();
        self.doc.saved_text = self.doc.text.clone();
        self.build.messages.clear();
        self.clear_errors();
        self.status = "New file".into();
    }

    /// Asks for a file to open (the browser: shows "my programs").
    pub fn open_dialog(&mut self) {
        if self.front.browser {
            self.windows[Tool::Programs] = true;
            return;
        }
        let dir = self.doc.path.as_ref().and_then(|p| p.parent());
        let filters: [crate::platform::Filter; 2] =
            [("Assembly", &["asm", "s", "nasm", "inc", "txt"]), ("All files", &["*"])];
        if let Some(p) = crate::platform::pick_file(&filters, dir) {
            let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "com" | "exe" | "bin" | "boot") {
                self.load_binary(&p);
            } else {
                self.open_path(&p);
            }
        }
    }

    /// Opens a source file in the editor.
    pub fn open_path(&mut self, p: &Path) {
        match std::fs::read(p) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                let saved = self.saved_arch(&p.to_string_lossy());
                if !self.enter_workspace_for(&PendingAction::Open(Some(p.to_path_buf())), saved, &text) {
                    return;
                }
                self.store_session();
                self.doc.text = text;
                self.doc.saved_text = self.doc.text.clone();
                self.doc.path = Some(p.to_path_buf());
                self.doc.name = None;
                self.doc.from_link = false;
                self.doc.program = None;
                self.build.messages.clear();
                self.clear_errors();
                self.add_recent(p);
                *self.settings.last_file_mut(self.arch()) = Some(p.to_path_buf());
                self.doc.example_key = None;
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
        if let Some(purpose) = self.web.pending_upload.take() {
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
        let Some(drive) = self.session.dos_memory_drive_mut() else { return };
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
        if self.front.browser {
            match self.doc.program.clone() {
                Some(name) => {
                    self.save_program(&name);
                    return true;
                }
                None => return self.save_as(),
            }
        }
        match self.doc.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    /// Saves under a new name (the browser: asks for a name in "my programs").
    pub fn save_as(&mut self) -> bool {
        if self.front.browser {
            self.dialogs.save_name = Some(crate::programs::SaveName { text: self.file_name(), then: None });
            return false;
        }
        let dir = self.doc.path.as_ref().and_then(|p| p.parent());
        match crate::platform::save_file(&[("Assembly", &["asm", "s"])], dir, &self.file_name()) {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    /// Shares the source as a `.asm` file through the system share sheet; a browser that cannot
    /// share files downloads it instead, the desktop offers a link to the web version.
    pub fn share_source(&mut self) {
        let name = self.file_name();
        if crate::platform::share_file(&name, self.doc.text.as_bytes()) {
            self.status = format!("Sharing {name}");
        } else if self.front.browser {
            self.download_source();
            self.status = format!("This browser cannot share files: {name} was downloaded instead");
        } else {
            self.dialogs.share = Some(crate::share::ShareDialog { run: false });
        }
    }

    /// Saves the editor text as a download (the browser).
    pub fn download_source(&mut self) {
        let name = self.file_name();
        crate::platform::download(&name, self.doc.text.as_bytes());
        self.status = format!("Downloaded {name}");
    }

    fn save_to(&mut self, p: &Path) -> bool {
        match std::fs::write(p, &self.doc.text) {
            Ok(()) => {
                self.doc.example_key = None;
                self.doc.saved_text = self.doc.text.clone();
                self.doc.path = Some(p.to_path_buf());
                self.add_recent(p);
                *self.settings.last_file_mut(self.arch()) = Some(p.to_path_buf());
                self.status = format!("Saved {}", p.display());
                true
            }
            Err(e) => {
                self.notice("Save", &format!("Cannot save {}:\n{e}", p.display()));
                false
            }
        }
    }

    /// Opens the built-in example `i` in the editor.
    pub fn load_example(&mut self, i: usize) {
        self.dialogs.open_in = Some(Arch::I8086);
        if !self.enter_workspace_for(&PendingAction::Example(i), None, "") {
            return;
        }
        let (name, src) = EXAMPLES[i];
        self.load_example_text(name, src);
    }

    /// Opens the built-in MIPS example `i` in the MIPS workspace.
    pub fn load_mips_example(&mut self, i: usize) {
        self.dialogs.open_in = Some(Arch::Mips32);
        if !self.enter_workspace_for(&PendingAction::MipsExample(i), None, "") {
            return;
        }
        let (_, name, src) = crate::examples::MIPS_EXAMPLES[i];
        self.load_example_text(name, src);
    }

    /// Opens `src` as the built-in example `name` (it has no file; Save asks for one).
    pub fn load_example_text(&mut self, name: &str, src: &str) {
        self.store_session();
        self.doc.path = None;
        self.doc.name = None;
        self.doc.from_link = false;
        self.doc.program = None;
        let prefix = if self.arch() == Arch::Mips32 { MIPS_EXAMPLE_KEY } else { EXAMPLE_KEY };
        self.doc.example_key = Some(format!("{prefix}{name}"));
        // Emu8086's files have DOS line ends.
        self.doc.text = src.replace("\r\n", "\n");
        self.doc.saved_text = self.doc.text.clone();
        self.build.messages.clear();
        self.clear_errors();
        self.restore_session();
        self.status = format!("Example: {name}");
    }
}
