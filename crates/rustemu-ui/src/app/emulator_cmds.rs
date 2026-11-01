//! Commands of the emulator window menus (Emu8086): run until / set CS:IP at the
//! selected instruction, emulator state, reset.

use super::*;
use crate::tools::Tool;

impl App {
    /// Linear address of the instruction selected in the disassembly.
    pub fn selected_addr(&self) -> Option<u32> {
        self.view.code_selected.map(|(s, o)| rustemu_core::linear(s, o))
    }

    /// The program waits for a key and none is typed yet (a step would only wait again).
    pub fn waiting_for_key(&self) -> bool {
        let m = &self.session.machine;
        m.is_waiting_input() && m.sys.keys.is_empty()
    }

    /// Registers and flags may be changed by hand: a program is loaded and paused, and it is not
    /// in the middle of waiting for a key.
    pub fn cpu_editable(&self) -> bool {
        self.loaded() && self.run == RunMode::Paused && !self.session.machine.is_waiting_input()
    }

    /// Changes the CPU by hand when [`App::cpu_editable`]. The step-back history stays: stepping
    /// back returns to the state before the previous instruction, before this change too.
    pub fn edit_cpu(&mut self, f: impl FnOnce(&mut Cpu)) -> bool {
        if !self.cpu_editable() {
            return false;
        }
        f(&mut self.session.machine.cpu);
        let c = &self.session.machine.cpu;
        self.status = format!("Changed by hand at {:04X}:{:04X}", c.cs(), c.ip);
        true
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
        let Some((seg, off)) = self.view.code_selected else { return };
        self.stop();
        let c = &mut self.session.machine.cpu;
        c.sregs[rustemu_core::cpu::CS] = seg;
        c.ip = off;
        c.halted = false;
        self.view.code_view_pinned = false;
        self.status = format!("CS:IP = {seg:04X}:{off:04X}");
    }

    /// Debug -> show current instruction: the disassembly follows CS:IP again.
    pub fn show_current_instruction(&mut self) {
        self.view.code_view_pinned = false;
        self.view.code_view_start = self.session.machine.cpu.pc();
    }

    /// Offset of the first row of the disassembly in the code segment (None: it shows another segment).
    pub fn disasm_offset(&self) -> Option<u16> {
        let base = rustemu_core::linear(self.session.machine.cpu.cs(), 0);
        self.view.code_view_start.checked_sub(base).filter(|&o| o <= 0xFFFF).map(|o| o as u16)
    }

    /// Shows the disassembly from `off` in the code segment, from the row the byte belongs to.
    /// Decoding from a little earlier falls in step with the instructions.
    pub fn show_disasm_at(&mut self, off: u16) {
        let cs = self.session.machine.cpu.cs();
        let mut o = off.saturating_sub(256) as u32;
        let mut row = o;
        while o <= off as u32 {
            row = o;
            o += self
                .session
                .code_row(rustemu_core::linear(cs, o as u16), o as u16, self.settings.disasm_data)
                .len
                .max(1);
        }
        self.view.code_view_start = rustemu_core::linear(cs, row as u16);
        self.view.code_view_pinned = true;
    }

    /// Scrolls the disassembly by `rows` rows (negative: up) within the code segment. The list
    /// stays there until the next execution command or "show current instruction".
    pub fn scroll_disasm(&mut self, rows: i32) {
        let Some(mut off) = self.disasm_offset() else { return };
        let cs = self.session.machine.cpu.cs();
        for _ in 0..rows.unsigned_abs() {
            if rows > 0 {
                let len = self.session.code_row(rustemu_core::linear(cs, off), off, self.settings.disasm_data).len;
                if off as u32 + len > 0xFFFF {
                    break;
                }
                off += len as u16;
            } else if off > 0 {
                self.show_disasm_at(off - 1);
                off = self.disasm_offset().unwrap_or(0);
            }
        }
        self.view.code_view_start = rustemu_core::linear(cs, off);
        self.view.code_view_pinned = true;
    }

    /// Debug -> show current break point: shows the next breakpoint after the shown one.
    pub fn show_next_breakpoint(&mut self) {
        let mut bps: Vec<u32> = self.session.machine.breakpoints.iter().copied().collect();
        bps.sort_unstable();
        let from = if self.view.code_view_pinned { self.view.code_view_start + 1 } else { 0 };
        let Some(&a) = bps.iter().find(|&&a| a >= from).or(bps.first()) else { return };
        self.view.code_view_start = a;
        self.view.code_view_pinned = true;
        let cs = self.session.machine.cpu.cs();
        let off = a.wrapping_sub(rustemu_core::linear(cs, 0));
        self.view.code_selected =
            Some(if off <= 0xFFFF { (cs, off as u16) } else { ((a >> 4) as u16, (a & 15) as u16) });
    }

    /// File -> save the emulator's state (kept until the application is closed).
    pub fn save_emulator_state(&mut self) {
        self.program.saved_state = Some(SavedState {
            session: self.session.save_state(),
            source_files: self.program.files.clone(),
            source_index: self.program.index.clone(),
            loaded_text: self.program.text.clone(),
        });
        let c = &self.session.machine.cpu;
        self.status = format!("Emulator state saved at {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// File -> load from previous state.
    pub fn load_emulator_state(&mut self) {
        let Some(s) = self.program.saved_state.clone() else { return };
        self.stop();
        self.session.restore_state(s.session);
        self.program.files = s.source_files;
        self.program.index = s.source_index;
        self.program.text = s.loaded_text;
        self.build.failed = false;
        self.program.generation += 1;
        self.exec.last_event = None;
        for w in &mut self.debugger.watches {
            w.compiled = None;
        }
        self.debugger.stop_condition.compiled = None;
        self.sync_breakpoints();
        self.exec.prev_cpu = self.session.machine.cpu.clone();
        self.exec.prev_fpu = self.session.machine.sys.fpu.clone();
        self.view.code_view_pinned = false;
        let c = &self.session.machine.cpu;
        self.status = format!("Emulator state restored - CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
    }

    /// File -> reset emulator and RAM: nothing loaded, memory cleared.
    pub fn reset_emulator_and_ram(&mut self) {
        self.run = RunMode::Paused;
        self.session.unload();
        self.program.files.clear();
        self.program.index.clear();
        self.program.text = None;
        self.debugger.bp_addr.clear();
        self.exec.prev_cpu = self.session.machine.cpu.clone();
        self.exec.prev_fpu = self.session.machine.sys.fpu.clone();
        self.view.code_selected = None;
        self.view.code_view_pinned = false;
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

    /// Opens a file and loads it into the emulator: an executable as is, a source after assembling it.
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
        self.windows[Tool::Emulator] = false;
    }

    /// Toggles the breakpoint on the editor line of the cursor.
    pub fn toggle_breakpoint_at_cursor(&mut self) {
        self.toggle_breakpoint_at_line(0, self.editor.cursor_line);
    }

    /// Maps line breakpoints to addresses of the loaded program (address breakpoints set in the
    /// disassembly are kept). Outdated programs keep the old mapping until they are reassembled.
    pub fn sync_breakpoints(&mut self) {
        if self.is_stale() {
            return;
        }
        let bps = &mut self.session.machine.breakpoints;
        bps.retain(|a| !self.debugger.bp_addr.contains_key(a));
        self.debugger.bp_addr.clear();
        for (&(f, l), bp) in &mut self.debugger.bp_lines {
            bp.compiled = None;
            if !bp.enabled {
                continue;
            }
            for a in self.session.addrs_of_line(f, l) {
                self.debugger.bp_addr.insert(a, (f, l));
                self.session.machine.breakpoints.insert(a);
            }
        }
    }
}
