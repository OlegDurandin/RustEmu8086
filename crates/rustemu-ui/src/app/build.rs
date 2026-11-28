//! Compile and emulate: assembling the editor text and loading it into the emulator.

use super::*;
use crate::tools::Tool;

impl App {
    /// Removes the error marks of the editor.
    pub fn clear_errors(&mut self) {
        self.build.error_lines.clear();
        self.build.error_spans.clear();
    }

    /// Shows the place message `i` refers to in the editor: the part of the line an error is
    /// about is selected, otherwise the cursor goes to the line.
    pub fn jump_to_message(&mut self, i: usize) {
        let Some(m) = self.build.messages.get(i) else { return };
        let Some((_, line)) = m.loc else { return };
        match m.span {
            Some((col, len)) => {
                let start = crate::find::line_start_char(&self.doc.text, line);
                let line_len = self.doc.text.split('\n').nth(line as usize - 1).map_or(0, |l| l.chars().count());
                let (a, b) = (col.min(line_len), (col + len).min(line_len));
                self.editor.pending_select = Some((start + a, start + b));
            }
            None => self.editor.goto_line = Some(line),
        }
    }

    /// The 8086 result of the build that [`App::submit_build`] has just completed.
    fn assemble(&mut self) -> Option<rustemu_asm::Program> {
        self.build.messages.clear();
        self.clear_errors();
        let Some(rustemu_build::Outcome::X86(result)) = self.build.completed.take() else { return None };
        match result {
            Ok(p) => {
                self.build.messages.push(Message {
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
                self.build.fail();
                for err in e.errors.iter().take(rustemu_mips::asm::MAX_DIAGNOSTICS) {
                    let file = e.files.get(err.file).cloned().unwrap_or_default();
                    let short =
                        Path::new(&file).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or(file.clone());
                    let span = err.col.map(|c| (c, err.len.max(1)));
                    if err.file == 0 && err.line > 0 {
                        self.build.error_lines.push(err.line);
                        if let Some((col, len)) = span {
                            self.build.error_spans.push((err.line, col, len));
                        }
                    }
                    self.build.messages.push(Message {
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
                self.build.messages.push(Message {
                    text: format!("{} error(s)", e.errors.len()),
                    loc: None,
                    span: None,
                    error: true,
                });
                if let Some(i) = self.build.messages.iter().position(|m| m.loc.is_some()) {
                    self.jump_to_message(i);
                }
                self.status = "Assembly failed".into();
                None
            }
        }
    }

    /// Messages of a MIPS assembly: errors (with their places in the editor) or warnings; `files`
    /// are the names the errors' `file` indices refer to (0: the editor's program).
    fn mips_messages(&mut self, list: &[rustemu_mips::asm::AsmError], files: &[String], error: bool) {
        let main = self.file_name();
        let limit = rustemu_mips::asm::MAX_DIAGNOSTICS;
        let shown = limit - usize::from(list.len() > limit);
        if list.len() > limit {
            self.build.messages.push(Message {
                text: format!("Further messages omitted (showing the first {shown})"),
                loc: None,
                span: None,
                error,
            });
        }
        for e in list.iter().take(shown) {
            // A line of an included file: shown, but not a place in the editor.
            let file = match e.file {
                0 => main.clone(),
                i => files.get(i).map_or(String::new(), |f| {
                    Path::new(f).file_name().map_or(f.clone(), |n| n.to_string_lossy().into_owned())
                }),
            };
            let span = (e.col > 0 && e.file == 0).then(|| {
                let line = self.doc.text.split('\n').nth(e.line.saturating_sub(1)).unwrap_or("");
                let col = e.col - 1;
                // The token at the column.
                let len = line.chars().skip(col).take_while(|c| !c.is_whitespace() && *c != ',').count();
                (col, len.max(1))
            });
            let line = e.line as u32;
            if error && line > 0 && e.file == 0 {
                self.build.error_lines.push(line);
                if let Some((col, len)) = span {
                    self.build.error_spans.push((line, col, len));
                }
            }
            let kind = if error { "" } else { "warning: " };
            // In a macro expansion: the lines of the calls first, as MARS writes them (12->5).
            let calls: String = e.expansion.iter().map(|l| format!("{l}->")).collect();
            let col = if e.col > 0 { format!(":{}", e.col) } else { String::new() };
            self.build.messages.push(Message {
                text: match line {
                    0 => format!("{kind}{}", e.message),
                    _ => format!("{file} ({calls}{line}{col}): {kind}{}", e.message),
                },
                loc: (line > 0 && e.file == 0).then(|| (main.clone(), line)),
                span,
                error,
            });
        }
    }

    /// The MIPS result (MARS's assembler, with the files it includes) of the build that
    /// [`App::submit_build`] has just completed.
    fn assemble_mips(&mut self) -> Option<rustemu_mips::asm::Program> {
        self.build.messages.clear();
        self.clear_errors();
        let Some(rustemu_build::Outcome::Mips(result)) = self.build.completed.take() else { return None };
        match result {
            Ok(p) => {
                let words: usize = p.statements.iter().map(|s| s.words.len()).sum();
                let data: u32 = p.data.iter().map(|d| d.len).sum();
                self.build.messages.push(Message {
                    text: format!(
                        "{}: assembled successfully - MIPS32, {words} instructions, {data} bytes of data",
                        self.file_name()
                    ),
                    loc: None,
                    span: None,
                    error: false,
                });
                let files: Vec<String> = p.files.iter().map(|f| f.name.clone()).collect();
                self.mips_messages(&p.warnings, &files, false);
                Some(p)
            }
            Err(e) => {
                self.build.fail();
                self.mips_messages(&e.errors, &e.files, true);
                self.build.messages.push(Message {
                    text: format!("{} error(s)", e.errors.len()),
                    loc: None,
                    span: None,
                    error: true,
                });
                if let Some(i) = self.build.messages.iter().position(|m| m.loc.is_some()) {
                    self.jump_to_message(i);
                }
                self.status = "Assembly failed".into();
                None
            }
        }
    }

    /// Assembles the MIPS program and loads it; its files are in drive C:'s folder (the desktop)
    /// or in memory (the browser).
    fn emulate_mips(&mut self) {
        let Some(p) = self.assemble_mips() else { return };
        let same_image = self.session.mips_program.as_ref().is_some_and(|old| old.image == p.image);
        // The browser: the files of drive C: (DOS's memory drive), copied back as the program
        // writes them (`sync_mips_files`).
        // A project or a program from a link gets an empty drive and writes nothing back.
        let isolated = self.doc.project.is_some() || self.from_link();
        let drive = self.document_drive();
        let memory_dirs: Vec<String> = drive.map(|d| d.dirs().map(str::to_string).collect()).unwrap_or_default();
        let storage = if isolated {
            rustemu_mips::files::Storage::Memory(Default::default())
        } else {
            match self.vdrive(self.doc.path.as_deref()) {
                Some(root) => rustemu_mips::files::Storage::Host(root),
                None => rustemu_mips::files::Storage::Memory(
                    drive.map(|d| d.files().map(|(k, v)| (k.to_string(), v.to_vec())).collect()).unwrap_or_default(),
                ),
            }
        };
        if let Some(m) = self.session.mips_mut() {
            m.files.storage = storage;
            m.files.set_memory_directories(memory_dirs);
            self.web.mips_files_synced = m.files.generation;
        }
        let name = self.file_name();
        // The source window shows the lines of the program and of the files it includes.
        let files: Vec<(String, Vec<String>)> = p
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let short = if i == 0 {
                    name.clone()
                } else {
                    Path::new(&f.name).file_name().map_or(f.name.clone(), |n| n.to_string_lossy().into_owned())
                };
                (short, f.text.lines().map(str::to_string).collect())
            })
            .collect();
        if let Some(m) = self.session.mips_mut() {
            m.program_arguments = self.settings.mars.arguments();
        }
        if let Err(e) = self.session.load_mips(p, &name) {
            self.build.fail();
            self.notice("Load", &e);
            return;
        }
        self.program.index = vec![Vec::new(); files.len()];
        self.program.files = files;
        self.program.text = Some(self.doc.text.clone());
        self.program.portable = self.doc.project.is_some();
        self.program.on_user_drive = !isolated;
        if !same_image {
            self.clear_address_breakpoints();
        }
        self.after_load();
    }

    /// Compile and write the output file next to the source.
    pub fn compile(&mut self) {
        if self.submit_build(super::background::BuildAction::Compile) {
            return;
        }
        if self.arch() == Arch::Mips32 {
            // MARS has no executable format either: the program is assembled and checked.
            if self.assemble_mips().is_some() {
                self.build.messages.push(Message {
                    text: "MIPS programs run in the emulator: there is no executable file to write (emulate - F9)"
                        .into(),
                    loc: None,
                    span: None,
                    error: false,
                });
                self.status = "Assembled".into();
            }
            return;
        }
        let Some(p) = self.assemble() else { return };
        if self.front.browser {
            let stem = self.file_name();
            let stem = stem.rsplit_once('.').map_or(stem.as_str(), |(s, _)| s);
            let name = format!("{stem}.{}", p.format.extension());
            crate::platform::download(&name, &p.file_bytes());
            self.build.messages.push(Message {
                text: format!("downloaded: {name}"),
                loc: None,
                span: None,
                error: false,
            });
            self.status = format!("Compiled to {name}");
            return;
        }
        let out = match &self.doc.path {
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
                    self.build.messages.push(Message {
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
        if self.submit_build(super::background::BuildAction::Emulate) {
            return;
        }
        if self.arch() == Arch::Mips32 {
            return self.emulate_mips();
        }
        let Some(p) = self.assemble() else { return };
        let source_files: Vec<(String, Vec<String>)> = p
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let text = if i == 0 {
                    self.doc.text.clone()
                } else if let Some(text) = self.build.sources.get(f) {
                    text.clone()
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
        let had_own_drive = self.web.own_drive.is_some();
        self.use_temporary_drive(from_link || self.doc.project.is_some());
        let root = if self.doc.project.is_some() {
            if let Some(m) = self.session.x86_mut() {
                m.set_dos_memory_drive(Default::default());
            }
            None
        } else {
            if self.program.portable && !self.front.browser && !had_own_drive {
                let root = self.drive_dir.clone();
                if let Some(m) = self.session.x86_mut() {
                    m.set_dos_root(root);
                }
            }
            self.vdrive(self.doc.path.as_deref())
        };
        let same_image = self.session.program.as_ref().is_some_and(|old| old.file_bytes() == p.file_bytes());
        if let Err(e) = self.session.load_program(p) {
            self.build.fail();
            self.notice("Load", &e);
            return;
        }
        self.program.files = source_files;
        self.program.index = source_index;
        self.program.text = Some(self.doc.text.clone());
        self.program.portable = self.doc.project.is_some();
        if let Some(root) = root
            && let Some(m) = self.session.x86_mut()
        {
            m.set_dos_root(root);
        }
        if !same_image {
            self.clear_address_breakpoints();
        }
        self.after_load();
        if let Some(d) = devices.iter().find_map(|n| crate::devices_ui::Device::from_start_name(n)) {
            self.view.device_tab = d;
            self.windows[Tool::Devices] = true;
        }
        if from_link && self.web.own_drive.is_some() {
            self.status.push_str(" - on a temporary drive C:");
            if std::mem::take(&mut self.web.link_notice_pending) {
                let text = if self.doc.project.is_some() {
                    crate::share::project_drive_text().to_string()
                } else {
                    crate::share::temporary_drive_text(&self.file_name())
                };
                self.notice("Program from a link", &text);
            }
        }
    }

    /// Loads a binary executable without source.
    pub fn load_binary(&mut self, p: &Path) {
        if self.program.portable && !self.front.browser && self.web.own_drive.is_none() {
            let root = self.drive_dir.clone();
            if let Some(m) = self.session.x86_mut() {
                m.set_dos_root(root);
            }
        }
        match self.session.load_path(p) {
            Ok(()) => {
                if let Some(root) = self.vdrive(Some(p))
                    && let Some(m) = self.session.x86_mut()
                {
                    m.set_dos_root(root);
                }
                self.after_binary_load();
            }
            Err(e) => self.notice("Load", &e),
        }
    }

    /// Virtual drive -> boot from virtual floppy. The boot program assembled last keeps its source
    /// when it is what the floppy holds (see `Session::boot_from_floppy`).
    pub fn boot_from_floppy(&mut self, drive: usize) {
        let signed = match self.session.boot_from_floppy(drive) {
            Ok(signed) if self.session.program.is_some() => {
                self.after_load();
                signed
            }
            Ok(signed) => {
                self.after_binary_load();
                signed
            }
            Err(e) => return self.notice("Virtual drive", &e),
        };
        let warning =
            if signed { String::new() } else { " (no 55AAh boot signature: a real BIOS would not boot it)".into() };
        self.status = format!("Booted from FLOPPY_{drive}{warning} - {}", self.status);
    }

    /// After loading code without debug information (binary file, boot from floppy).
    pub fn after_binary_load(&mut self) {
        self.program.portable = false;
        self.use_temporary_drive(false);
        self.clear_address_breakpoints();
        self.program.text = None;
        self.program.files.clear();
        self.program.index.clear();
        self.after_load();
    }

    /// Removes breakpoints set by address in the disassembly: they belong to the previous image.
    /// Line breakpoints are mapped again by [`App::sync_breakpoints`].
    fn clear_address_breakpoints(&mut self) {
        let lines = &self.debugger.bp_addr;
        self.session.target.breakpoints_mut().retain(|a| lines.contains_key(a));
    }

    fn after_load(&mut self) {
        self.program.prepared_input.clear();
        if self.program.portable
            && let Some(project) = &self.doc.project
        {
            self.program.prepared_input = if self.arch() == Arch::Mips32 {
                project.input.replace("\r\n", "\n").replace('\r', "\n").bytes().collect()
            } else {
                project
                    .input
                    .replace("\r\n", "\n")
                    .chars()
                    .map(|c| match c {
                        '\n' | '\r' => 13,
                        c => rustemu_core::font::char_to_cp437(c),
                    })
                    .collect()
            };
        }
        self.feed_prepared_input();
        self.build.succeed();
        if let Some(m) = self.session.x86_mut() {
            m.timer_irq = self.settings.timer_irq;
        }
        self.program.generation += 1;
        self.exec.last_event = None;
        for w in &mut self.debugger.watches {
            w.compiled = None;
        }
        self.debugger.stop_condition.compiled = None;
        self.resolve_pending_breakpoints();
        self.sync_breakpoints();
        self.run = RunMode::Paused;
        self.begin_exec();
        self.view.source_file_idx = 0;
        if let Some(tools) = self.deferred_tools.take() {
            self.windows.open_also(&tools);
        }
        self.windows[Tool::Emulator] = true;
        if self.arch() == Arch::Mips32 {
            self.windows[Tool::RunIo] = true;
            self.mips.focus_request = true;
            let (keyboard, lab) = self.session.mips_program.as_ref().map_or((false, false), |p| {
                let mmio = p.image.settings.layout().mmio_base;
                crate::mips_devices::devices_used(p.statements.iter().flat_map(|s| s.words.iter().copied()), mmio)
            });
            if keyboard {
                self.windows[Tool::MmioDisplay] = true;
                self.devices.focus_request = true;
                self.mips.focus_request = false;
            }
            if lab {
                self.windows[Tool::DigitalLab] = true;
            }
            self.devices.typed.clear();
            self.devices.queue.clear();
        } else {
            self.windows[Tool::Screen] = true;
            self.screen.focus_request = true;
        }
        self.windows[Tool::Source] = !self.program.files.is_empty();
        // The memory window starts at the data (8086: DS:0000, MIPS: .data).
        self.view.mem_view_addr = match self.arch() {
            Arch::Mips32 => crate::mips_ui::layout(self).data_base,
            _ => self.session.target.data_address(None, 0),
        };
        self.status = format!("Loaded - {}", self.pc_status());
    }

    /// The directory DOS file functions see as drive C: when `program` runs: a folder of its own,
    /// as Emu8086's `vdrive\C` ([`App::drive_dir`]), or the program's folder with
    /// [`Settings::drive_c_source_folder`]. None for the memory drive (the browser).
    pub(super) fn vdrive(&self, program: Option<&Path>) -> Option<PathBuf> {
        if self.session.x86().is_some_and(|m| m.dos_memory_drive().is_some()) || self.front.browser {
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
        self.session.x86().and_then(|m| m.dos_host_root()).unwrap_or(&self.drive_dir)
    }

    /// Loads the same program again (Reload): registers, memory and devices as at the start.
    pub fn reload(&mut self) {
        match self.session.reload() {
            Ok(()) => self.after_load(),
            Err(e) => self.notice("Reload", &e),
        }
    }

    /// A program is loaded in the emulator.
    pub fn loaded(&self) -> bool {
        self.session.target.is_loaded()
    }

    /// The loaded program was assembled from a different version of the editor text.
    pub fn is_stale(&self) -> bool {
        self.loaded() && self.program.text.as_ref().is_some_and(|t| *t != self.doc.text)
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn mips_diagnostic_display_is_bounded_independently() {
        let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
        let list = vec![
            rustemu_mips::asm::AsmError {
                file: 0,
                line: 1,
                col: 0,
                message: "bad instruction".into(),
                expansion: vec![],
            };
            1000
        ];
        let before = app.build.messages.len();
        app.mips_messages(&list, &["main.s".into()], true);
        assert_eq!(app.build.messages.len() - before, rustemu_mips::asm::MAX_DIAGNOSTICS);
        assert_eq!(app.build.error_lines.len(), rustemu_mips::asm::MAX_DIAGNOSTICS - 1);
        assert!(app.build.messages.iter().any(|m| m.text.contains("Further messages omitted")));
    }
}
