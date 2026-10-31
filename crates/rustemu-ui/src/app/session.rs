//! Per-file debugging state (breakpoints, watches, bookmarks...) kept in the settings.

use super::*;

impl App {
    /// Key under which the current file's session is stored (None for a new unsaved file).
    pub fn session_key(&self) -> Option<String> {
        match &self.path {
            Some(p) => Some(p.to_string_lossy().into_owned()),
            None => self.example_key.clone().or_else(|| self.program.as_ref().map(|n| format!("program:{n}"))),
        }
    }

    /// The current file's breakpoints, watches, bookmarks and stop condition.
    pub fn capture_session(&self) -> FileSession {
        let name = |f: usize| {
            if f == 0 { String::new() } else { self.source_files.get(f).map(|s| s.0.clone()).unwrap_or_default() }
        };
        let mut breakpoints: Vec<SavedBreakpoint> = self
            .bp_lines
            .iter()
            .filter(|&(&(f, _), _)| f == 0 || f < self.source_files.len())
            .map(|(&(f, line), bp)| SavedBreakpoint {
                file: name(f),
                line,
                condition: bp.condition.clone(),
                enabled: bp.enabled,
            })
            .collect();
        breakpoints.extend(self.pending_breakpoints.iter().cloned());
        let sc = &self.stop_condition;
        FileSession {
            breakpoints,
            watches: self.watches.iter().map(|w| w.text.clone()).collect(),
            bookmarks: self.bookmarks.iter().copied().collect(),
            stop_condition: sc.text.clone(),
            stop_enabled: sc.enabled,
            stop_on_change: sc.on_change,
            cursor_line: self.cursor_line,
        }
    }

    /// Remembers the current file's session in the settings.
    pub fn store_session(&mut self) {
        if !self.session_started {
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
        self.bp_lines.clear();
        self.pending_breakpoints.clear();
        self.watches.clear();
        self.bookmarks.clear();
        self.stop_condition = StopCondition::default();
        self.sync_breakpoints();
        self.session_started = true;
    }

    /// Restores the session saved for the current file (or starts an empty one).
    pub fn restore_session(&mut self) {
        self.clear_session();
        let Some(key) = self.session_key() else { return };
        let Some(s) = self.settings.sessions.iter().find(|(k, _)| *k == key).map(|(_, s)| s.clone()) else { return };
        for b in s.breakpoints {
            if b.file.is_empty() {
                let bp = Breakpoint { condition: b.condition, enabled: b.enabled, hits: 0, compiled: None };
                self.bp_lines.insert((0, b.line), bp);
            } else {
                self.pending_breakpoints.push(b);
            }
        }
        self.watches = s.watches.into_iter().map(|text| Watch { text, compiled: None, before: None }).collect();
        self.bookmarks = s.bookmarks.into_iter().collect();
        self.stop_condition = StopCondition {
            text: s.stop_condition,
            enabled: s.stop_enabled,
            on_change: s.stop_on_change,
            ..Default::default()
        };
        if s.cursor_line > 1 {
            self.goto_line = Some(s.cursor_line);
        }
        self.resolve_pending_breakpoints();
        self.sync_breakpoints();
    }

    /// Turns saved breakpoints of included files into line breakpoints once the program is loaded.
    pub(super) fn resolve_pending_breakpoints(&mut self) {
        let pending = std::mem::take(&mut self.pending_breakpoints);
        for b in pending {
            match self.source_files.iter().position(|(n, _)| n.eq_ignore_ascii_case(&b.file)) {
                Some(f) if f > 0 => {
                    let bp = Breakpoint { condition: b.condition, enabled: b.enabled, hits: 0, compiled: None };
                    self.bp_lines.insert((f, b.line), bp);
                }
                _ => self.pending_breakpoints.push(b),
            }
        }
    }

    /// Copies the window and debugging state into the settings (before they are saved).
    pub fn store_settings(&mut self) {
        self.store_session();
        self.settings.windows = self.windows.clone();
        if let Some(tools) = &self.deferred_tools {
            self.settings.windows.open_also(tools.clone());
        }
        self.settings.panels = self.panels.as_ref().and_then(|t| ron::to_string(t).ok());
        self.settings.show_macro_code = self.show_macro_code;
        self.settings.follow_source = self.follow_source;
        self.settings.reference_lang = self.reference.lang;
        self.settings.vars_format = self.vars_format;
    }
}
