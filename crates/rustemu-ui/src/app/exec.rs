//! Execution: steps, runs, breakpoints, stop conditions and the emulation loop.

use super::*;
use crate::tools::Tool;

/// The message of a caught panic.
pub(crate) fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".into())
}

impl App {
    /// Runs `f`; a panic inside it (a bug of the emulator) stops the program with a notice instead
    /// of closing the application. None after a panic. (In the browser a panic cannot be caught:
    /// the web page shows it and keeps the text — `rustemu-web`.)
    pub(crate) fn catching<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> Option<T> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self))) {
            Ok(v) => Some(v),
            Err(payload) => {
                let text = panic_text(&*payload);
                self.run = RunMode::Paused;
                self.status = format!("Internal error of the emulator: {text}");
                self.notice(
                    "Internal error",
                    &format!(
                        "The emulator stopped the program because of an error of its own:\n\n{text}\n\n\
                         The machine may be in a broken state: reload the program (Reload) to go on. \
                         Please report it with the program that caused it."
                    ),
                );
                None
            }
        }
    }

    /// Execution commands run the current build: after a failed build they try to build again
    /// instead of running the old image (it stays loaded for inspection).
    pub(super) fn ensure_current_build(&mut self) -> bool {
        if self.is_building() {
            return false;
        }
        if self.front.background_builds
            && (self.is_stale()
                || (self.program.text.is_some() && self.build.loaded_version != Some(self.build.document_version)))
        {
            self.emulate();
            return false;
        }
        if self.loaded() && !self.build.failed {
            return true;
        }
        self.emulate();
        // `emulate` leaves the assembly error in `status`.
        self.loaded() && !self.build.failed && !self.is_building()
    }

    /// Executes one instruction (assembling the editor text first when it changed).
    pub fn single_step(&mut self) {
        if !self.ensure_current_build() {
            self.step_after_build(super::background::BuildStep::Into);
            return;
        }
        self.run = RunMode::Paused;
        self.begin_exec();
        let Some(ev) = self.catching(|app| app.session.target.step()) else { return };
        self.handle_event(ev);
        self.sync_mips_files();
    }

    /// Undoes the last instruction (up to the depth of the step-back history).
    pub fn step_back(&mut self) {
        self.run = RunMode::Paused;
        self.begin_exec();
        if self.catching(|app| app.session.target.step_back()) == Some(true) {
            self.status = "Stepped back".into();
        } else {
            self.status = "No more history".into();
        }
    }

    /// Step Over: runs the current source line (including macros and called procedures).
    /// Without debug information: runs CALL / INT / LOOP / REP to the next instruction.
    pub fn step_over(&mut self) {
        if !self.ensure_current_build() {
            self.step_after_build(super::background::BuildStep::Over);
            return;
        }
        if self.session.program.is_some() {
            self.begin_exec();
            self.run = RunMode::Source(self.session.step_goal(StepKind::Over, self.settings.show_macro_code));
            self.status = "Stepping over...".into();
            return;
        }
        if let Some(next) = self.session.target.step_over_stop() {
            self.begin_exec();
            self.run = RunMode::RunTo(vec![next]);
        } else {
            self.single_step();
        }
    }

    /// Step Out: runs until the current macro or procedure returns to its caller.
    pub fn step_out(&mut self) {
        if !self.ensure_current_build() {
            self.step_after_build(super::background::BuildStep::Out);
            return;
        }
        self.begin_exec();
        self.run = RunMode::Source(self.session.step_goal(StepKind::Out, self.settings.show_macro_code));
        self.status = "Stepping out...".into();
    }

    /// Runs freely until a breakpoint, an error, the end of the program or Stop.
    pub fn start_run(&mut self) {
        if self.is_building() {
            self.run_after_build();
            return;
        }
        if !self.ensure_current_build() {
            if self.is_building() {
                self.run_after_build();
            }
            return;
        }
        if self.session.target.terminated().is_some() {
            self.status = "Program has terminated - press Reload to run again".into();
            return;
        }
        self.begin_exec();
        self.run = RunMode::Running;
        self.status = "Running...".into();
    }

    /// F4: runs until execution reaches the line with the text cursor (or a breakpoint).
    pub fn run_to_cursor(&mut self) {
        self.run_to_line(0, self.editor.cursor_line);
    }

    /// Runs until execution reaches `line` of source file `file` (the next line with code).
    pub fn run_to_line(&mut self, file: usize, line: u32) {
        // A binary without source is (re)built from the editor text first.
        let ok = if self.session.program.is_none() {
            self.emulate();
            self.loaded() && !self.build.failed
        } else {
            self.ensure_current_build()
        };
        if !ok || self.session.program.is_none() {
            return;
        }
        if file == 0 && self.is_stale() {
            self.status = "The source was changed - reassemble (F9) before running to a line".into();
            return;
        }
        if self.session.target.terminated().is_some() {
            self.status = "Program has terminated - press Reload to run again".into();
            return;
        }
        let Some(l) = (line..line + 20).find(|&l| self.session.addr_of_line(file, l).is_some()) else {
            self.status = format!("No code at line {line}");
            return;
        };
        self.begin_exec();
        self.run = RunMode::RunTo(self.session.addrs_of_line(file, l));
        self.status = format!("Running to line {l}...");
    }

    /// Remembers the state before an execution command (changed registers and watches are highlighted).
    pub fn begin_exec(&mut self) {
        self.exec.remember(&*self.session.target);
        self.view.code_view_pinned = false;
        for i in 0..self.debugger.watches.len() {
            self.debugger.watches[i].before = self.watch_value(i).ok();
        }
        let v = self.stop_value();
        self.debugger.stop_condition.last = v;
    }

    /// Current value of watch `i`.
    pub fn watch_value(&mut self, i: usize) -> Result<i64, String> {
        let w = &mut self.debugger.watches[i];
        if w.compiled.is_none() {
            w.compiled = Some(self.session.compile(&w.text));
        }
        match w.compiled.as_ref().unwrap() {
            Ok(e) => Ok(e.eval(&*self.session.target)),
            Err(e) => Err(e.clone()),
        }
    }

    /// Value of the stop condition, when it is enabled and valid.
    fn stop_value(&mut self) -> Option<i64> {
        let sc = &mut self.debugger.stop_condition;
        if !sc.enabled || sc.text.trim().is_empty() || !self.session.target.is_loaded() {
            return None;
        }
        if sc.compiled.is_none() {
            sc.compiled = Some(self.session.compile(&sc.text));
        }
        sc.compiled.as_ref().unwrap().as_ref().ok().map(|e| e.eval(&*self.session.target))
    }

    fn stop_condition_active(&self) -> bool {
        let sc = &self.debugger.stop_condition;
        sc.enabled && !sc.text.trim().is_empty() && !matches!(sc.compiled, Some(Err(_)))
    }

    /// Checks the stop condition after an instruction; stops execution when it is met.
    fn check_stop_condition(&mut self) -> bool {
        let Some(v) = self.stop_value() else { return false };
        let last = self.debugger.stop_condition.last.replace(v);
        let met = if self.debugger.stop_condition.on_change {
            last.is_some_and(|l| l != v)
        } else {
            v != 0 && last == Some(0)
        };
        if met {
            self.run = RunMode::Paused;
            let at = self.session.target.pc_text();
            self.status = if self.debugger.stop_condition.on_change {
                format!(
                    "Stopped: {} changed from {} to {v} at {at}",
                    self.debugger.stop_condition.text,
                    last.unwrap_or(0),
                )
            } else {
                format!("Stopped: {} is true at {at}", self.debugger.stop_condition.text)
            };
        }
        met
    }

    /// Decides whether execution that reached a breakpoint address should stop there: the line
    /// breakpoint's condition must hold. Counts the hit.
    fn breakpoint_hit(&mut self, pc: u32) -> bool {
        let Some(key) = self.debugger.bp_addr.get(&pc).copied() else { return true };
        let Some(bp) = self.debugger.bp_lines.get_mut(&key) else { return true };
        if bp.compiled.is_none() && !bp.condition.trim().is_empty() {
            bp.compiled = Some(self.session.compile(&bp.condition));
        }
        let stop = match &bp.compiled {
            Some(Ok(e)) if !bp.condition.trim().is_empty() => e.eval(&*self.session.target) != 0,
            _ => true,
        };
        if stop {
            bp.hits += 1;
        }
        stop
    }

    /// Is there a breakpoint at the current position that should stop execution?
    fn at_breakpoint(&mut self) -> bool {
        let pc = self.session.target.pc();
        self.session.target.breakpoints().contains(&pc) && self.breakpoint_hit(pc)
    }

    /// "CS:IP = 0700:0100": where execution is, for the status line.
    pub fn pc_status(&self) -> String {
        let t = &*self.session.target;
        format!("{} = {}", t.pc_name(), t.pc_text())
    }

    /// Stops a run.
    pub fn stop(&mut self) {
        self.cancel_build();
        if self.run != RunMode::Paused {
            self.run = RunMode::Paused;
            self.status = format!("Stopped at {}", self.session.target.pc_text());
        }
    }

    /// Updates the run state and the status line after the emulator reports `ev`.
    pub fn handle_event(&mut self, ev: Event) {
        self.exec.last_event = Some(ev.clone());
        let t = &*self.session.target;
        let at = t.pc_text();
        let pc_name = t.pc_name();
        match ev {
            Event::Continue | Event::Budget => {
                if self.run == RunMode::Paused {
                    self.status = format!("{pc_name} = {at}");
                }
            }
            Event::Sleeping => {
                self.status = "Waiting (INT 15h/86h)...".into();
            }
            Event::WaitingInput if self.session.mips().is_some_and(|m| m.dialog().is_some()) => {
                self.status = "Waiting for the dialog window of the syscall".into();
            }
            Event::WaitingInput if t.arch() == rustemu_session::Arch::Mips32 => {
                self.status = "Waiting for input - type it in the Run I/O console".into();
                self.mips.focus_request = true;
                self.windows[Tool::RunIo] = true;
            }
            Event::WaitingInput => {
                self.status = "Waiting for keyboard input - type in the emulator screen".into();
                self.screen.focus_request = true;
                self.windows[Tool::Screen] = true;
            }
            Event::Halted => {
                self.run = RunMode::Paused;
                self.status = format!("HLT executed at {at}");
            }
            Event::Terminated(code) => {
                self.run = RunMode::Paused;
                self.status = format!("Program terminated (exit code {code})");
                self.build.messages.push(Message { text: self.status.clone(), loc: None, span: None, error: false });
            }
            Event::Breakpoint => {
                self.run = RunMode::Paused;
                self.status = format!("Breakpoint at {at}");
            }
            Event::Error(e) => {
                self.run = RunMode::Paused;
                self.status = format!("Error at {at}: {e}");
                self.notice("Emulator", &format!("{e}\n\nat {at}"));
            }
        }
    }

    /// Advances the emulation for this frame (a panic of the emulator stops the program).
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.poll_build(ctx);
        self.apply_mips_devices();
        let begin = web_time::Instant::now();
        self.catching(|app| app.tick_inner(ctx));
        self.exec.tick_ms = begin.elapsed().as_secs_f64() * 1000.0;
        self.exec.max_tick_ms = self.exec.max_tick_ms.max(self.exec.tick_ms);
        self.sync_mips_files();
    }

    /// The browser: copies the files a MIPS program wrote to drive C: (DOS's memory drive, kept
    /// in the browser and shown in the "drive C:" window), when they changed.
    pub(crate) fn sync_mips_files(&mut self) {
        let Some(m) = self.session.mips() else { return };
        let rustemu_mips::files::Storage::Memory(files) = &m.files.storage else { return };
        if m.files.generation == self.web.mips_files_synced {
            return;
        }
        self.web.mips_files_synced = m.files.generation;
        let files: Vec<(String, Vec<u8>)> = files.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let Some(drive) = self.x86_session_mut().and_then(|s| s.dos_memory_drive_mut()) else { return };
        // Stage the complete update so a shrink can fund another file's growth regardless
        // of name order, and a failed sync cannot leave only part of the batch on drive C:.
        let mut updated = drive.clone();
        for (name, _) in &files {
            if updated.read(name).is_some() {
                updated.remove(name);
            }
        }
        let errors: Vec<String> =
            files.into_iter().filter_map(|(name, data)| updated.write(&name, data).err()).collect();
        if errors.is_empty() {
            *drive = updated;
        }
        if let Some(e) = errors.first() {
            self.notice("Drive C:", &format!("A file of the MIPS program could not be kept on drive C:\n\n{e}"));
        }
    }

    fn tick_inner(&mut self, ctx: &egui::Context) {
        self.handle_uploads(ctx);
        if self.run == RunMode::Paused || !self.loaded() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let waiting = self.session.target.is_waiting_input() && !self.session.target.has_pending_input();
        if waiting {
            // Nothing to do until a key arrives; keep repainting slowly.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        // Emulation time per frame.
        let frame = std::time::Duration::from_millis(12);
        let start = web_time::Instant::now();
        match self.run.clone() {
            RunMode::Running if self.settings.step_delay_ms > 0 => {
                if (now - self.exec.last_step) * 1000.0 >= self.settings.step_delay_ms as f64 {
                    self.exec.last_step = now;
                    self.exec.remember(&*self.session.target);
                    let ev = self.session.target.step();
                    self.handle_event(ev);
                    if self.run != RunMode::Paused && self.at_breakpoint() {
                        self.handle_event(Event::Breakpoint);
                    } else if self.run != RunMode::Paused {
                        self.check_stop_condition();
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(self.settings.step_delay_ms.min(30) as u64));
            }
            RunMode::Running if self.stop_condition_active() => {
                // The condition is checked after every instruction.
                'run: while start.elapsed() < frame {
                    for _ in 0..4096 {
                        let ev = self.session.target.step();
                        if ev != Event::Continue {
                            self.handle_event(ev);
                            break 'run;
                        }
                        if self.at_breakpoint() {
                            self.handle_event(Event::Breakpoint);
                            break 'run;
                        }
                        if self.check_stop_condition() {
                            break 'run;
                        }
                    }
                }
                ctx.request_repaint();
            }
            RunMode::Running => {
                loop {
                    let t = web_time::Instant::now();
                    let ev = self.session.target.run(self.exec.instr_per_frame);
                    let el = t.elapsed().as_secs_f64();
                    if ev == Event::Budget {
                        // Aim at ~12 ms of emulation per frame.
                        if el < 0.008 {
                            self.exec.instr_per_frame = (self.exec.instr_per_frame * 3 / 2).min(20_000_000);
                        } else if el > 0.016 {
                            self.exec.instr_per_frame = (self.exec.instr_per_frame * 2 / 3).max(1000);
                        }
                    }
                    // A conditional breakpoint whose condition is false: keep going.
                    if ev == Event::Breakpoint && !self.breakpoint_hit(self.session.target.pc()) {
                        if start.elapsed() < frame {
                            continue;
                        }
                        break;
                    }
                    self.handle_event(ev);
                    break;
                }
                ctx.request_repaint();
            }
            RunMode::RunTo(targets) => {
                'run: while start.elapsed() < frame {
                    for _ in 0..4096 {
                        let ev = self.session.target.step();
                        if ev != Event::Continue {
                            self.handle_event(ev);
                            break 'run;
                        }
                        let pc = self.session.target.pc();
                        if targets.contains(&pc) {
                            self.run = RunMode::Paused;
                            self.status = self.pc_status();
                            break 'run;
                        }
                        if self.at_breakpoint() {
                            self.handle_event(Event::Breakpoint);
                            break 'run;
                        }
                        if self.debugger.stop_condition.enabled && self.check_stop_condition() {
                            break 'run;
                        }
                    }
                }
                ctx.request_repaint();
            }
            RunMode::Source(mut goal) => {
                loop {
                    match self.session.run_step_goal(&mut goal, self.exec.instr_per_frame.max(10_000)) {
                        None => self.run = RunMode::Source(goal.clone()),
                        Some(Event::Continue) => {
                            self.run = RunMode::Paused;
                            self.status = self.pc_status();
                        }
                        Some(Event::Breakpoint) if !self.breakpoint_hit(self.session.target.pc()) => {
                            self.run = RunMode::Source(goal.clone());
                            if start.elapsed() < frame {
                                continue;
                            }
                        }
                        Some(ev @ (Event::WaitingInput | Event::Sleeping)) => {
                            self.run = RunMode::Source(goal.clone());
                            self.handle_event(ev);
                        }
                        Some(ev) => self.handle_event(ev),
                    }
                    break;
                }
                ctx.request_repaint();
            }
            RunMode::Paused => {}
        }
    }

    /// Adds or removes a breakpoint. With a program loaded, a line without code puts the
    /// breakpoint on the next line that has code.
    pub fn toggle_breakpoint_at_line(&mut self, file: usize, line: u32) {
        if self.debugger.bp_lines.remove(&(file, line)).is_some() {
            self.sync_breakpoints();
            return;
        }
        let mapped = self.loaded() && self.session.program.is_some() && !(file == 0 && self.is_stale());
        let line = if mapped {
            match (line..line + 20).find(|&l| self.session.addr_of_line(file, l).is_some()) {
                Some(l) => l,
                None => {
                    self.status = format!("No code on line {line}");
                    return;
                }
            }
        } else {
            line
        };
        self.debugger.bp_lines.entry((file, line)).or_default();
        self.sync_breakpoints();
    }

    /// Breakpoint on the editor line with the text cursor.
    /// Breakpoint at an instruction of the disassembly: a line breakpoint when the address maps to a
    /// program line (it survives reloads); macro code keeps an address breakpoint (its line may be
    /// expanded in many places).
    pub fn toggle_breakpoint_at_addr(&mut self, addr: u32) {
        if let Some(loc) = self.session.location_at_addr(addr).filter(|l| l.origin.is_none())
            && self.session.addr_of_line(loc.file, loc.line) == Some(addr)
        {
            self.toggle_breakpoint_at_line(loc.file, loc.line);
        } else {
            let bps = self.session.target.breakpoints_mut();
            if !bps.remove(&addr) {
                bps.insert(addr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mips_browser_file_limits_match_drive_c_and_sync_as_a_batch() {
        use rustemu_mips::files::{MEMORY_CAPACITY, MEMORY_MAX_ENTRIES, O_WRONLY};
        assert_eq!(rustemu_mips::files::MAX_CREATED_FILES, rustemu_core::dos::HOST_MAX_CREATED as usize);
        assert_eq!(MEMORY_CAPACITY, rustemu_core::memdrive::CAPACITY);
        assert_eq!(MEMORY_MAX_ENTRIES, rustemu_core::memdrive::MAX_ENTRIES);
        let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
        app.front.browser = true;
        let mut drive = rustemu_core::memdrive::MemDrive::new();
        drive.write("z", vec![1; MEMORY_CAPACITY]).unwrap();
        // Existing empty folders must occupy entries in the MIPS view too.
        for i in 0..MEMORY_MAX_ENTRIES - 2 {
            let path = format!("d{i}/temp");
            drive.write(&path, vec![]).unwrap();
            drive.remove(&path);
        }
        app.session.set_dos_memory_drive(drive);
        app.switch_arch(rustemu_session::Arch::Mips32);
        app.doc.text = "li $v0, 10\nsyscall".into();
        app.emulate();
        assert!(app.loaded());
        let m = app.session.mips_mut().unwrap();
        let fd = m.files.open("z", O_WRONLY).unwrap();
        m.files.close(fd);
        let fd = m.files.open("a", O_WRONLY).unwrap();
        m.files.write(fd, &vec![2; MEMORY_CAPACITY]).unwrap();
        assert!(m.files.write(fd, &[3]).is_err());
        m.files.close(fd);
        assert!(m.files.open("overflow", O_WRONLY).is_err());
        assert!(m.files.open("newdir/child", O_WRONLY).is_err());
        app.sync_mips_files();
        let drive = app.x86_session().unwrap().dos_memory_drive().unwrap();
        assert_eq!(drive.used(), MEMORY_CAPACITY);
        assert_eq!(drive.read("z"), Some(&[][..]));
        assert_eq!(drive.read("a").unwrap(), vec![2; MEMORY_CAPACITY]);
        assert!(drive.read("overflow").is_none());
        assert!(app.dialogs.notice.is_none(), "{:?}", app.dialogs.notice);
    }

    /// In the browser MIPS programs see drive C: (DOS's memory drive): they read its files, in
    /// any case of their names, and what they write is kept there.
    #[test]
    fn mips_files_are_on_drive_c_in_the_browser() {
        let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
        app.front.browser = true;
        let mut drive = rustemu_core::memdrive::MemDrive::new();
        drive.write("in.txt", b"abc".to_vec()).unwrap();
        app.session.set_dos_memory_drive(drive);
        app.switch_arch(rustemu_session::Arch::Mips32);
        app.doc.text = ".data
inn: .asciiz \"in.txt\"
out: .asciiz \"out.txt\"
buf: .space 8
.text
la $a0, inn
li $a1, 0
li $v0, 13
syscall
move $a0, $v0
la $a1, buf
li $a2, 8
li $v0, 14
syscall
move $a2, $v0
la $a0, out
li $a1, 1
li $v0, 13
syscall
move $a0, $v0
la $a1, buf
li $v0, 15
syscall
li $v0, 16
syscall
"
        .into();
        app.emulate();
        assert!(app.loaded(), "{:?}", app.build.messages.iter().map(|m| &m.text).collect::<Vec<_>>());
        while app.session.target.terminated().is_none() {
            app.single_step();
        }
        let drive = app.x86_session().and_then(|s| s.dos_memory_drive()).unwrap();
        assert_eq!(drive.read("OUT.TXT"), Some(&b"abc"[..]));
    }

    /// A panic of the emulator stops the program with a notice; the application goes on.
    #[test]
    fn a_panic_becomes_a_notice() {
        let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
        app.emulate();
        app.start_run();
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r: Option<()> = app.catching(|_| panic!("index out of bounds"));
        std::panic::set_hook(hook);
        assert_eq!(r, None);
        assert_eq!(app.run, RunMode::Paused);
        let (title, text) = app.dialogs.notice.clone().unwrap();
        assert_eq!(title, "Internal error");
        assert!(text.contains("index out of bounds"), "{text}");
        // And the application still works.
        app.single_step();
        assert!(app.session.target.instr_count() > 0);
    }
}
