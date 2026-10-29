//! Commands of the emulator window menus (Emu8086): run until / set CS:IP at the
//! selected instruction, emulator state, reset.

use super::*;

impl App {
    /// Linear address of the instruction selected in the disassembly.
    pub fn selected_addr(&self) -> Option<u32> {
        self.code_selected.map(|(s, o)| rustemu_core::linear(s, o))
    }

    /// Debug -> run until: runs to the selected instruction (or a breakpoint).
    pub fn run_until_selected(&mut self) {
        let Some(addr) = self.selected_addr() else { return };
        if !self.ensure_current_build() {
            return;
        }
        if self.session.machine.terminated.is_some() {
            self.status = "Program has terminated - press Reload to run again".into();
            return;
        }
        self.begin_exec();
        self.run = RunMode::RunTo(vec![addr]);
        self.status = format!("Running until {addr:05X}h...");
    }

    /// Debug -> set CS:IP to selected position.
    pub fn set_cs_ip_to_selected(&mut self) {
        let Some((seg, off)) = self.code_selected else { return };
        self.stop();
        let c = &mut self.session.machine.cpu;
        c.sregs[rustemu_core::cpu::CS] = seg;
        c.ip = off;
        c.halted = false;
        self.code_view_pinned = false;
        self.status = format!("CS:IP = {seg:04X}:{off:04X}");
    }

    /// Debug -> show current instruction: the disassembly follows CS:IP again.
    pub fn show_current_instruction(&mut self) {
        self.code_view_pinned = false;
        self.code_view_start = self.session.machine.cpu.pc();
    }

    /// Debug -> show current break point: shows the next breakpoint after the shown one.
    pub fn show_next_breakpoint(&mut self) {
        let mut bps: Vec<u32> = self.session.machine.breakpoints.iter().copied().collect();
        bps.sort_unstable();
        let from = if self.code_view_pinned { self.code_view_start + 1 } else { 0 };
        let Some(&a) = bps.iter().find(|&&a| a >= from).or(bps.first()) else { return };
        self.code_view_start = a;
        self.code_view_pinned = true;
        let cs = self.session.machine.cpu.cs();
        let off = a.wrapping_sub(rustemu_core::linear(cs, 0));
        self.code_selected = Some(if off <= 0xFFFF { (cs, off as u16) } else { ((a >> 4) as u16, (a & 15) as u16) });
    }

    /// File -> save the emulator's state (kept until the application is closed).
    pub fn save_emulator_state(&mut self) {
        self.saved_state = Some(SavedState {
            session: self.session.save_state(),
            source_files: self.source_files.clone(),
            source_index: self.source_index.clone(),
            loaded_text: self.loaded_text.clone(),
        });
        let c = &self.session.machine.cpu;
        self.status = format!("Emulator state saved at {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// File -> load from previous state.
    pub fn load_emulator_state(&mut self) {
        let Some(s) = self.saved_state.clone() else { return };
        self.stop();
        self.session.restore_state(s.session);
        self.source_files = s.source_files;
        self.source_index = s.source_index;
        self.loaded_text = s.loaded_text;
        self.build_failed = false;
        self.load_gen += 1;
        for w in &mut self.watches {
            w.compiled = None;
        }
        self.stop_condition.compiled = None;
        self.sync_breakpoints();
        self.prev_cpu = self.session.machine.cpu.clone();
        self.code_view_pinned = false;
        let c = &self.session.machine.cpu;
        self.status = format!("Emulator state restored - CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// File -> reset emulator and RAM: nothing loaded, memory cleared.
    pub fn reset_emulator_and_ram(&mut self) {
        self.run = RunMode::Paused;
        self.session.unload();
        self.source_files.clear();
        self.source_index.clear();
        self.loaded_text = None;
        self.bp_addr.clear();
        self.prev_cpu = self.session.machine.cpu.clone();
        self.code_selected = None;
        self.code_view_pinned = false;
        self.status = "Emulator and RAM reset".into();
    }

    /// File -> examples / recent files: opens the source and loads it into the emulator.
    pub fn emulate_example(&mut self, i: usize) {
        if self.dirty() {
            self.guarded(PendingAction::Example(i));
        } else {
            self.load_example(i);
            self.emulate();
        }
    }

    pub fn emulate_file(&mut self, p: &Path) {
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "com" | "exe" | "bin" | "boot") {
            self.load_binary(p);
        } else if self.dirty() {
            self.guarded(PendingAction::Open(Some(p.to_path_buf())));
        } else {
            self.open_path(p);
            self.emulate();
        }
    }

    /// File -> close the emulator.
    pub fn close_emulator(&mut self) {
        self.stop();
        self.windows.emulator = false;
    }

    pub fn toggle_breakpoint_at_cursor(&mut self) {
        self.toggle_breakpoint_at_line(0, self.cursor_line);
    }

    /// Maps line breakpoints to addresses of the loaded program (address breakpoints set in the
    /// disassembly are kept). Outdated programs keep the old mapping until they are reassembled.
    pub fn sync_breakpoints(&mut self) {
        if self.is_stale() {
            return;
        }
        let bps = &mut self.session.machine.breakpoints;
        bps.retain(|a| !self.bp_addr.contains_key(a));
        self.bp_addr.clear();
        for (&(f, l), bp) in &mut self.bp_lines {
            bp.compiled = None;
            if !bp.enabled {
                continue;
            }
            for a in self.session.addrs_of_line(f, l) {
                self.bp_addr.insert(a, (f, l));
                self.session.machine.breakpoints.insert(a);
            }
        }
    }
}
