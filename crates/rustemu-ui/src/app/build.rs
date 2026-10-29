//! Compile and emulate: assembling the editor text and loading it into the emulator.

use super::*;

impl App {
    pub fn clear_errors(&mut self) {
        self.error_lines.clear();
        self.error_spans.clear();
    }

    /// Shows the place message `i` refers to in the editor: the part of the line an error is
    /// about is selected, otherwise the cursor goes to the line.
    pub fn jump_to_message(&mut self, i: usize) {
        let Some(m) = self.messages.get(i) else { return };
        let Some((_, line)) = m.loc else { return };
        match m.span {
            Some((col, len)) => {
                let start = crate::find::line_start_char(&self.text, line);
                let line_len = self.text.split('\n').nth(line as usize - 1).map_or(0, |l| l.chars().count());
                let (a, b) = (col.min(line_len), (col + len).min(line_len));
                self.pending_select = Some((start + a, start + b));
            }
            None => self.goto_line = Some(line),
        }
    }

    fn assemble(&mut self) -> Option<rustemu_asm::Program> {
        let name = self.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| self.file_name());
        let mut loader = rustemu_asm::FsLoader { include_dirs: Vec::new() };
        self.messages.clear();
        self.clear_errors();
        match rustemu_asm::assemble_dialect(&name, &self.text, &mut loader, self.settings.syntax.dialect()) {
            Ok(p) => {
                self.messages.push(Message {
                    text: format!(
                        "{}: assembled successfully - {} syntax, {} format, {} bytes",
                        self.file_name(),
                        p.dialect.name(),
                        p.format.name(),
                        p.file_bytes().len()
                    ),
                    loc: None,
                    span: None,
                    error: false,
                });
                Some(p)
            }
            Err(e) => {
                self.build_failed = true;
                for err in &e.errors {
                    let file = e.files.get(err.file).cloned().unwrap_or_default();
                    let short =
                        Path::new(&file).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or(file.clone());
                    let span = err.col.map(|c| (c, err.len.max(1)));
                    if err.file == 0 && err.line > 0 {
                        self.error_lines.push(err.line);
                        if let Some((col, len)) = span {
                            self.error_spans.push((err.line, col, len));
                        }
                    }
                    self.messages.push(Message {
                        text: match (err.line, span) {
                            (0, _) => err.msg.clone(),
                            (line, Some((col, _))) => format!("{short} ({line}:{}): {}", col + 1, err.msg),
                            (line, None) => format!("{short} ({line}): {}", err.msg),
                        },
                        loc: (err.line > 0 && err.file == 0).then_some((file, err.line)),
                        span: span.filter(|_| err.file == 0),
                        error: true,
                    });
                }
                self.messages.push(Message {
                    text: format!("{} error(s)", e.errors.len()),
                    loc: None,
                    span: None,
                    error: true,
                });
                if let Some(i) = self.messages.iter().position(|m| m.loc.is_some()) {
                    self.jump_to_message(i);
                }
                self.status = "Assembly failed".into();
                None
            }
        }
    }

    /// Compile and write the output file next to the source.
    pub fn compile(&mut self) {
        let Some(p) = self.assemble() else { return };
        if self.browser {
            let stem = self.file_name();
            let stem = stem.rsplit_once('.').map_or(stem.as_str(), |(s, _)| s);
            let name = format!("{stem}.{}", p.format.extension());
            crate::platform::download(&name, &p.file_bytes());
            self.messages.push(Message { text: format!("downloaded: {name}"), loc: None, span: None, error: false });
            self.status = format!("Compiled to {name}");
            return;
        }
        let out = match &self.path {
            Some(src) => src.with_extension(p.format.extension()),
            None => match crate::platform::save_file(&[], None, &format!("program.{}", p.format.extension())) {
                Some(f) => f,
                None => return,
            },
        };
        let mut written = vec![];
        let res = std::fs::write(&out, p.file_bytes()).and_then(|_| {
            written.push(out.clone());
            let lst = out.with_extension("lst");
            std::fs::write(&lst, p.listing_text())?;
            written.push(lst);
            let sym = out.with_extension("symbol");
            std::fs::write(&sym, p.symbols_text())?;
            written.push(sym);
            Ok(())
        });
        match res {
            Ok(()) => {
                for w in written {
                    self.messages.push(Message {
                        text: format!("written: {}", w.display()),
                        loc: None,
                        span: None,
                        error: false,
                    });
                }
                self.status = format!("Compiled to {}", out.display());
            }
            Err(e) => self.notice("Compile", &format!("Cannot write {}:\n{e}", out.display())),
        }
    }

    /// Assemble and load into the emulator.
    pub fn emulate(&mut self) {
        let Some(p) = self.assemble() else { return };
        let source_files: Vec<(String, Vec<String>)> = p
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let text = if i == 0 {
                    self.text.clone()
                } else if f == "emu8086.inc" {
                    rustemu_asm::EMU8086_INC.to_string()
                } else {
                    std::fs::read_to_string(f).unwrap_or_default()
                };
                let name = Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(f.clone());
                (name, text.lines().map(str::to_string).collect())
            })
            .collect();
        let source_index =
            source_files.iter().map(|(_, lines)| rustemu_asm::library::index(&lines.join("\n"))).collect();
        let devices = p.emu.start.clone();
        let from_link = self.from_link();
        self.use_temporary_drive(from_link);
        let root = self.vdrive(self.path.as_deref());
        let same_image = self.session.program.as_ref().is_some_and(|old| old.file_bytes() == p.file_bytes());
        if let Err(e) = self.session.load_program(p) {
            self.build_failed = true;
            self.notice("Load", &e);
            return;
        }
        self.source_files = source_files;
        self.source_index = source_index;
        self.loaded_text = Some(self.text.clone());
        if let Some(root) = root {
            self.session.machine.set_dos_root(root);
        }
        if !same_image {
            self.clear_address_breakpoints();
        }
        self.after_load();
        if let Some(d) = devices.iter().find_map(|n| crate::devices_ui::Device::from_start_name(n)) {
            self.device_tab = d;
            self.windows.devices = true;
        }
        if from_link && self.own_drive.is_some() {
            self.status.push_str(" - on a temporary drive C:");
            if std::mem::take(&mut self.link_notice_pending) {
                self.notice("Program from a link", &crate::share::temporary_drive_text(&self.file_name()));
            }
        }
    }

    /// Loads a binary executable without source.
    pub fn load_binary(&mut self, p: &Path) {
        match self.session.load_path(p) {
            Ok(()) => {
                if let Some(root) = self.vdrive(Some(p)) {
                    self.session.machine.set_dos_root(root);
                }
                self.after_binary_load();
            }
            Err(e) => self.notice("Load", &e),
        }
    }

    /// After loading code without debug information (binary file, boot from floppy).
    pub fn after_binary_load(&mut self) {
        self.use_temporary_drive(false);
        self.clear_address_breakpoints();
        self.loaded_text = None;
        self.source_files.clear();
        self.source_index.clear();
        self.after_load();
    }

    /// Removes breakpoints set by address in the disassembly: they belong to the previous image.
    /// Line breakpoints are mapped again by [`App::sync_breakpoints`].
    fn clear_address_breakpoints(&mut self) {
        let lines = &self.bp_addr;
        self.session.machine.breakpoints.retain(|a| lines.contains_key(a));
    }

    fn after_load(&mut self) {
        self.build_failed = false;
        self.session.machine.timer_irq = self.settings.timer_irq;
        self.load_gen += 1;
        for w in &mut self.watches {
            w.compiled = None;
        }
        self.stop_condition.compiled = None;
        self.resolve_pending_breakpoints();
        self.sync_breakpoints();
        self.run = RunMode::Paused;
        self.begin_exec();
        self.source_file_idx = 0;
        self.windows.emulator = true;
        self.windows.screen = true;
        self.windows.source = !self.source_files.is_empty();
        self.screen.focus_request = true;
        let c = &self.session.machine.cpu;
        self.mem_view_addr = rustemu_core::linear(c.sregs[rustemu_core::cpu::DS], 0);
        self.status = format!("Loaded - CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// The directory DOS file functions see as drive C: when `program` runs: a folder of its own,
    /// as Emu8086's `vdrive\C` ([`App::drive_dir`]), or the program's folder with
    /// [`Settings::drive_c_source_folder`]. None for the memory drive (the browser).
    fn vdrive(&self, program: Option<&Path>) -> Option<PathBuf> {
        if self.session.machine.dos_memory_drive().is_some() {
            return None;
        }
        if self.settings.drive_c_source_folder
            && let Some(dir) = program.and_then(Path::parent).filter(|d| !d.as_os_str().is_empty())
        {
            return Some(dir.to_path_buf());
        }
        std::fs::create_dir_all(&self.drive_dir).ok();
        Some(self.drive_dir.clone())
    }

    /// The folder that is drive C: on the desktop: the loaded program's, otherwise the default.
    pub fn host_drive(&self) -> &Path {
        self.session.machine.dos_host_root().unwrap_or(&self.drive_dir)
    }

    pub fn reload(&mut self) {
        match self.session.reload() {
            Ok(()) => self.after_load(),
            Err(e) => self.notice("Reload", &e),
        }
    }

    pub fn loaded(&self) -> bool {
        self.session.machine.loaded.is_some()
    }

    /// The loaded program was assembled from a different version of the editor text.
    pub fn is_stale(&self) -> bool {
        self.loaded() && self.loaded_text.as_ref().is_some_and(|t| *t != self.text)
    }
}
