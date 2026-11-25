//! One presentation of execution state for both front ends; the engines remain authoritative.
use super::*;
use crate::tools::Tool;

/// User-visible execution state, derived without parsing status messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionState {
    /// No instructions executed in the current program.
    Ready,
    /// Execution is advancing.
    Running,
    /// Execution was paused by the user or a step command.
    Paused,
    /// Input is needed before the program can advance.
    WaitingInput,
    /// A breakpoint was reached.
    Breakpoint,
    /// DOS exit, MIPS exit or HLT.
    Finished,
    /// Assembly or execution failed.
    Fault,
    /// The current source is being assembled in the background.
    Assembling,
    /// The source changed (or its build was cancelled): the loaded image is not run until the
    /// source is assembled again.
    Outdated,
}

impl ExecutionState {
    /// Stable label shared by desktop and tablet.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Paused => "Paused",
            Self::WaitingInput => "Waiting for input",
            Self::Breakpoint => "Breakpoint",
            Self::Finished => "Finished",
            Self::Fault => "Fault",
            Self::Assembling => "Assembling",
            Self::Outdated => "Outdated",
        }
    }
}

impl App {
    /// Derives state from the current machine and the most recent execution event.
    pub fn execution_state(&self) -> ExecutionState {
        use ExecutionState::*;
        if self.is_building() {
            Assembling
        } else if self.build.failed && self.build.outdated {
            Outdated
        } else if self.build.failed || matches!(self.exec.last_event, Some(Event::Error(_))) {
            Fault
        } else if !self.loaded() {
            Ready
        } else if self.session.target.terminated().is_some() || self.exec.last_event == Some(Event::Halted) {
            Finished
        } else if self.session.target.is_waiting_input() {
            WaitingInput
        } else if self.run != RunMode::Paused {
            Running
        } else if self.exec.last_event == Some(Event::Breakpoint) {
            Breakpoint
        } else if self.session.target.instr_count() == 0 {
            Ready
        } else {
            Paused
        }
    }

    /// Text and an explicit route back to the appropriate input surface. Returns true when the
    /// tablet should show its PC keyboard. Native desktop instead focuses the emulator screen.
    pub fn execution_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let state = self.execution_state();
        let mut keyboard = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong(state.label());
            match state {
                ExecutionState::WaitingInput => {
                    if self.session.mips().is_some_and(|m| m.dialog().is_some()) {
                        ui.label("Answer the syscall dialog to continue without Reset.");
                    } else if self.arch() == Arch::Mips32 {
                        let kind = match self.session.mips().map(|m| m.cpu.regs[2]) {
                            Some(5) => "an integer",
                            Some(6) => "a floating-point number",
                            Some(7) => "a double-precision number",
                            Some(8) => "a line of text",
                            Some(12) => "a character",
                            _ => "the requested value",
                        };
                        ui.label(format!("Enter {kind} in Run I/O; Enter sends the line. No Reset needed."));
                        if ui.button("Open Run I/O").clicked() {
                            self.windows[Tool::RunIo] = true;
                            self.mips.focus_request = true;
                        }
                    } else {
                        ui.label("Ожидается клавиша / Waiting for a key. Continue without Reset.");
                        if ui.button("Enter a key").clicked() {
                            self.windows[Tool::Screen] = true;
                            self.screen.focus_request = true;
                            keyboard = true;
                        }
                    }
                }
                ExecutionState::Finished => {
                    ui.label("Step back to inspect, or Reload to run again.");
                }
                ExecutionState::Fault => {
                    ui.label("Read the error message; fix the source and assemble again.");
                }
                ExecutionState::Assembling => {
                    ui.label("Assembling the current source…");
                }
                ExecutionState::Outdated => {
                    ui.label("The source changed since it was loaded; Step or Run assembles it again.");
                }
                ExecutionState::Breakpoint | ExecutionState::Paused => {
                    ui.label("Step to inspect or Run to continue.");
                }
                ExecutionState::Ready => {
                    ui.label("Step or Run assembles the current source.");
                }
                ExecutionState::Running => {
                    ui.label("Pause / Stop keeps the current machine state.");
                }
            }
        });
        keyboard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_follow_machine_events_and_undo_clears_the_stop_reason() {
        let mut a = App::with_settings(Settings { last_file: None, ..Default::default() });
        a.doc.text = "org 100h\nnop\nhlt\n".into();
        a.emulate();
        assert_eq!(a.execution_state(), ExecutionState::Ready);
        a.start_run();
        assert_eq!(a.execution_state(), ExecutionState::Running);
        a.handle_event(Event::Breakpoint);
        assert_eq!(a.execution_state(), ExecutionState::Breakpoint);
        a.single_step();
        assert_eq!(a.execution_state(), ExecutionState::Paused);
        a.single_step();
        assert_eq!(a.execution_state(), ExecutionState::Finished);
        a.step_back();
        assert_eq!(a.execution_state(), ExecutionState::Paused);
        a.doc.text = "not_an_instruction".into();
        a.emulate();
        assert_eq!(a.execution_state(), ExecutionState::Fault);
    }

    #[test]
    fn browser_viewport_allows_user_zoom() {
        let html = include_str!("../../../rustemu-web/index.html");
        assert!(!html.contains("user-scalable=no"));
        assert!(!html.contains("maximum-scale="));
    }
}
