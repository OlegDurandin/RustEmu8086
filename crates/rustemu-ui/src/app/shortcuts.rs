//! Keyboard shortcuts of the editor and the emulator.

use super::*;
use crate::tools::Tool;

impl App {
    /// F1: opens the reference at the instruction / directive / interrupt under the text cursor.
    pub fn help_at_cursor(&mut self) {
        let mips = self.session.mips().is_some();
        let at = if mips { crate::reference::mips_topic_at_cursor } else { crate::reference::topic_at_cursor };
        match at(&self.doc.text, self.editor.selection.0) {
            Some(t) => self.reference.show(t),
            None => {
                self.reference.mips = mips;
                self.windows[Tool::Reference] = true;
            }
        }
    }

    /// Handles the keyboard shortcuts of the frame (F5, F8, ⌘B, ...).
    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let fire = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if self.is_building() && fire(sc(Modifiers::NONE, Key::Escape)) {
            self.cancel_build();
        }
        if fire(sc(cmd, Key::N)) {
            self.guarded(PendingAction::New);
        }
        if fire(sc(cmd, Key::O)) {
            self.guarded(PendingAction::Open(None));
        }
        if fire(sc(cmd | Modifiers::SHIFT, Key::S)) {
            self.save_as();
        } else if fire(sc(cmd, Key::S)) {
            self.save();
        }
        if fire(sc(Modifiers::NONE, Key::F5)) || fire(sc(cmd, Key::R)) {
            if self.is_building() || self.run != RunMode::Paused { self.stop() } else { self.start_run() }
        }
        if fire(sc(Modifiers::NONE, Key::F9)) || fire(sc(cmd, Key::E)) {
            self.emulate();
        }
        if fire(sc(cmd, Key::B)) {
            self.compile();
        }
        if fire(sc(Modifiers::NONE, Key::F8)) && self.loaded() {
            self.single_step();
        }
        if fire(sc(Modifiers::SHIFT, Key::F8)) && self.loaded() {
            self.step_back();
        }
        if fire(sc(Modifiers::NONE, Key::F10)) && self.loaded() {
            self.step_over();
        }
        if fire(sc(Modifiers::SHIFT, Key::F11)) && self.loaded() {
            self.step_out();
        }
        // Editor commands are not taken while the program is typed into the emulator screen.
        if self.screen.focused {
            return;
        }
        let ctrl = Modifiers::CTRL;
        if fire(sc(cmd, Key::F)) || fire(sc(cmd, Key::H)) || fire(sc(ctrl, Key::H)) {
            self.editor.find.open = true;
            self.editor.find.focus = true;
        }
        if fire(sc(cmd | Modifiers::SHIFT, Key::G)) || fire(sc(Modifiers::SHIFT, Key::F3)) {
            crate::find::find_next(self, true);
        } else if fire(sc(cmd, Key::G)) || fire(sc(Modifiers::NONE, Key::F3)) {
            crate::find::find_next(self, false);
        }
        if fire(sc(cmd, Key::L)) || fire(sc(ctrl, Key::G)) {
            self.editor.find.goto_open = true;
        }
        // F1: reference article for the word at the text cursor.
        if fire(sc(Modifiers::NONE, Key::F1)) {
            self.help_at_cursor();
        }
        // Emu8086 emulator keys that do not clash with ours.
        if fire(sc(Modifiers::NONE, Key::F11)) {
            self.windows[Tool::Console] = !self.windows[Tool::Console];
        }
        if fire(sc(Modifiers::NONE, Key::F6)) && self.loaded() {
            self.step_back();
        }
        if fire(sc(ctrl | Modifiers::SHIFT, Key::F7)) {
            self.set_cs_ip_to_selected();
        } else if fire(sc(Modifiers::NONE, Key::F7)) {
            self.show_current_instruction();
        }
        if fire(sc(ctrl, Key::F8)) {
            self.run_until_selected();
        }
        if fire(sc(Modifiers::NONE, Key::F4)) {
            self.run_to_cursor();
        }
        if fire(sc(cmd, Key::F8)) {
            self.toggle_breakpoint_at_cursor();
        }
        if fire(sc(cmd, Key::Slash)) {
            crate::find::toggle_comment(self);
        }
        // Emu8086 editor keys (with the Control key).
        if fire(sc(ctrl, Key::Q)) {
            crate::editcmd::comment(self);
        }
        if fire(sc(ctrl, Key::W)) {
            crate::editcmd::uncomment(self);
        }
        if fire(sc(ctrl, Key::L)) {
            crate::editcmd::lowercase(self);
        }
        if fire(sc(ctrl, Key::U)) {
            crate::editcmd::uppercase(self);
        }
        if fire(sc(ctrl, Key::F2)) || fire(sc(cmd, Key::F2)) {
            crate::editcmd::toggle_bookmark(self);
        } else if fire(sc(Modifiers::SHIFT, Key::F2)) {
            crate::editcmd::prev_bookmark(self);
        } else if fire(sc(Modifiers::NONE, Key::F2)) {
            crate::editcmd::next_bookmark(self);
        }
    }
}
