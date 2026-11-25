//! Per-file debugging state (breakpoints, watches, bookmarks...) kept in the settings.

use super::*;

impl App {
    /// Key under which the current file's session is stored (None for a new unsaved file).
    pub fn session_key(&self) -> Option<String> {
        match &self.doc.path {
            Some(p) => Some(p.to_string_lossy().into_owned()),
            None => self.doc.example_key.clone().or_else(|| self.doc.program.as_ref().map(|n| format!("program:{n}"))),
        }
    }

    /// The current file's breakpoints, watches, bookmarks and stop condition.
    pub fn capture_session(&self) -> FileSession {
        let name = |f: usize| {
            if f == 0 { String::new() } else { self.program.files.get(f).map(|s| s.0.clone()).unwrap_or_default() }
        };
        let mut breakpoints: Vec<SavedBreakpoint> = self
            .debugger
            .bp_lines
            .iter()
            .filter(|&(&(f, _), _)| f == 0 || f < self.program.files.len())
            .map(|(&(f, line), bp)| SavedBreakpoint {
                file: name(f),
                line,
                condition: bp.condition.clone(),
                enabled: bp.enabled,
            })
            .collect();
        breakpoints.extend(self.debugger.pending_breakpoints.iter().cloned());
        let sc = &self.debugger.stop_condition;
        FileSession {
            breakpoints,
            watches: self.debugger.watches.iter().map(|w| w.text.clone()).collect(),
            bookmarks: self.editor.bookmarks.iter().copied().collect(),
            stop_condition: sc.text.clone(),
            stop_enabled: sc.enabled,
            stop_on_change: sc.on_change,
            cursor_line: self.editor.cursor_line,
            arch: match self.arch() {
                rustemu_session::Arch::I8086 => String::new(),
                a => a.name().to_string(),
            },
        }
    }

    /// Remembers the current file's session in the settings.
    pub fn store_session(&mut self) {
        if !self.doc.session_started {
            return;
        }
        let Some(key) = self.session_key() else { return };
        let s = self.capture_session();
        let list = &mut self.settings.sessions;
        list.retain(|(k, _)| *k != key);
        let empty = s == FileSession { cursor_line: s.cursor_line, ..Default::default() };
        if !empty || s.cursor_line > 1 {
            list.insert(0, (key, s));
            list.truncate(MAX_SESSIONS);
        }
    }

    pub(super) fn clear_session(&mut self) {
        self.debugger.bp_lines.clear();
        self.debugger.pending_breakpoints.clear();
        self.debugger.watches.clear();
        self.editor.bookmarks.clear();
        self.debugger.stop_condition = StopCondition::default();
        self.sync_breakpoints();
        self.doc.session_started = true;
    }

    /// Restores the session saved for the current file (or starts an empty one).
    pub fn restore_session(&mut self) {
        self.clear_session();
        let Some(key) = self.session_key() else { return };
        let Some(s) = self.settings.sessions.iter().find(|(k, _)| *k == key).map(|(_, s)| s.clone()) else { return };
        for b in s.breakpoints {
            if b.file.is_empty() {
                let bp = Breakpoint { condition: b.condition, enabled: b.enabled, hits: 0, compiled: None };
                self.debugger.bp_lines.insert((0, b.line), bp);
            } else {
                self.debugger.pending_breakpoints.push(b);
            }
        }
        self.debugger.watches =
            s.watches.into_iter().map(|text| Watch { text, compiled: None, before: None }).collect();
        self.editor.bookmarks = s.bookmarks.into_iter().collect();
        self.debugger.stop_condition = StopCondition {
            text: s.stop_condition,
            enabled: s.stop_enabled,
            on_change: s.stop_on_change,
            ..Default::default()
        };
        if s.cursor_line > 1 {
            self.editor.goto_line = Some(s.cursor_line);
        }
        // Breakpoints of included files wait for this document's program: the loaded one is
        // still the previous document's, whose included files are not these
        // ([`App::after_load`] resolves them).
        self.sync_breakpoints();
    }

    /// Turns saved breakpoints of included files into line breakpoints once the program is loaded.
    pub(super) fn resolve_pending_breakpoints(&mut self) {
        let pending = std::mem::take(&mut self.debugger.pending_breakpoints);
        for b in pending {
            match self.program.files.iter().position(|(n, _)| n.eq_ignore_ascii_case(&b.file)) {
                Some(f) if f > 0 => {
                    let bp = Breakpoint { condition: b.condition, enabled: b.enabled, hits: 0, compiled: None };
                    self.debugger.bp_lines.insert((f, b.line), bp);
                }
                _ => self.debugger.pending_breakpoints.push(b),
            }
        }
    }

    /// Copies the window and debugging state into the settings (before they are saved). The
    /// parked workspaces stored theirs when they were left.
    pub fn store_settings(&mut self) {
        self.store_session();
        let (windows, panels) = self.settings.layout_mut(self.session.target.arch());
        *windows = self.windows.clone();
        if let Some(tools) = &self.deferred_tools {
            windows.open_also(tools);
        }
        *panels = self.panels.as_ref().and_then(|t| ron::to_string(t).ok());
        self.settings.reference_lang = self.reference.lang;
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{App, Breakpoint, Settings};

    /// TODO 55: a breakpoint in an included file waits for its own program. Reopening a file
    /// while another program is loaded must not match the saved file name against that
    /// program's includes (it moved the breakpoint into another file).
    #[test]
    fn included_file_breakpoints_resolve_against_their_own_program() {
        let dir = std::env::temp_dir().join(format!("rustemu-bp-include-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("my.inc"), "nop\nnop\nnop\nnop\nnop\nnop\n").unwrap();
        let b = dir.join("b.asm");
        let a = dir.join("a.asm");
        std::fs::write(&b, "include 'my.inc'\ninclude 'emu8086.inc'\norg 100h\nret\n").unwrap();
        std::fs::write(&a, "include 'emu8086.inc'\norg 100h\nret\n").unwrap();
        let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
        let index = |app: &App, name: &str| app.program.files.iter().position(|(n, _)| n.eq_ignore_ascii_case(name));

        app.open_path(&b);
        app.emulate();
        let lib = index(&app, "emu8086.inc").expect("b includes emu8086.inc");
        assert_eq!(index(&app, "my.inc"), Some(1));
        let bp = Breakpoint { condition: String::new(), enabled: true, hits: 0, compiled: None };
        app.debugger.bp_lines.insert((lib, 5), bp);

        app.open_path(&a);
        app.emulate();
        app.open_path(&b);
        assert!(!app.debugger.bp_lines.contains_key(&(1, 5)), "not moved into a's includes");
        app.emulate();
        assert_eq!(app.debugger.bp_lines.keys().copied().collect::<Vec<_>>(), vec![(lib, 5)]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
