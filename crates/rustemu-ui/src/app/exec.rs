//! Execution: steps, runs, breakpoints, stop conditions and the emulation loop.

use super::*;

impl App {
    /// Execution commands run the current build: after a failed build they try to build again
    /// instead of running the old image (it stays loaded for inspection).
    pub(super) fn ensure_current_build(&mut self) -> bool {
        if self.loaded() && !self.build_failed {
            return true;
        }
        self.emulate();
        // `emulate` leaves the assembly error in `status`.
        self.loaded() && !self.build_failed
    }

    pub fn single_step(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        self.run = RunMode::Paused;
        self.begin_exec();
        let ev = self.session.machine.step();
        self.handle_event(ev);
    }

    pub fn step_back(&mut self) {
        self.run = RunMode::Paused;
        self.begin_exec();
        if self.session.machine.step_back() {
            self.status = "Stepped back".into();
        } else {
            self.status = "No more history".into();
        }
    }

    /// Step Over: runs the current source line (including macros and called procedures).
    /// Without debug information: runs CALL / INT / LOOP / REP to the next instruction.
    pub fn step_over(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        if self.session.program.is_some() {
            self.begin_exec();
            self.run = RunMode::Source(self.session.step_goal(StepKind::Over, self.show_macro_code));
            self.status = "Stepping over...".into();
            return;
        }
        let m = &self.session.machine;
        let pc = m.cpu.pc();
        let bytes = m.sys.mem.slice(pc, 8).to_vec();
        let ins = disasm::disassemble(&bytes, m.cpu.ip);
        let t = ins.text.as_str();
        let over = t.starts_with("CALL") || t.starts_with("INT") || t.starts_with("LOOP") || t.starts_with("REP");
        if over {
            let next = rustemu_core::linear(m.cpu.cs(), m.cpu.ip.wrapping_add(ins.len as u16));
            self.begin_exec();
            self.run = RunMode::RunTo(vec![next]);
        } else {
            self.single_step();
        }
    }

    /// Step Out: runs until the current macro or procedure returns to its caller.
    pub fn step_out(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        self.begin_exec();
        self.run = RunMode::Source(self.session.step_goal(StepKind::Out, self.show_macro_code));
        self.status = "Stepping out...".into();
    }

    pub fn start_run(&mut self) {
        if !self.ensure_current_build() {
            return;
        }
        if self.session.machine.terminated.is_some() {
            self.status = "Program has terminated - press Reload to run again".into();
            return;
        }
        self.begin_exec();
        self.run = RunMode::Running;
        self.status = "Running...".into();
    }

    /// F4: runs until execution reaches the line with the text cursor (or a breakpoint).
    pub fn run_to_cursor(&mut self) {
        self.run_to_line(0, self.cursor_line);
    }

    /// Runs until execution reaches `line` of source file `file` (the next line with code).
    pub fn run_to_line(&mut self, file: usize, line: u32) {
        // A binary without source is (re)built from the editor text first.
        let ok = if self.session.program.is_none() {
            self.emulate();
            self.loaded() && !self.build_failed
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
        if self.session.machine.terminated.is_some() {
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
        self.prev_cpu = self.session.machine.cpu.clone();
        self.code_view_pinned = false;
        for i in 0..self.watches.len() {
            self.watches[i].before = self.watch_value(i).ok();
        }
        let v = self.stop_value();
        self.stop_condition.last = v;
    }

    /// Current value of watch `i`.
    pub fn watch_value(&mut self, i: usize) -> Result<i64, String> {
        let w = &mut self.watches[i];
        if w.compiled.is_none() {
            w.compiled = Some(self.session.compile(&w.text));
        }
        match w.compiled.as_ref().unwrap() {
            Ok(e) => Ok(e.eval(&self.session.machine)),
            Err(e) => Err(e.clone()),
        }
    }

    /// Value of the stop condition, when it is enabled and valid.
    fn stop_value(&mut self) -> Option<i64> {
        let sc = &mut self.stop_condition;
        if !sc.enabled || sc.text.trim().is_empty() || self.session.machine.loaded.is_none() {
            return None;
        }
        if sc.compiled.is_none() {
            sc.compiled = Some(self.session.compile(&sc.text));
        }
        sc.compiled.as_ref().unwrap().as_ref().ok().map(|e| e.eval(&self.session.machine))
    }

    fn stop_condition_active(&self) -> bool {
        let sc = &self.stop_condition;
        sc.enabled && !sc.text.trim().is_empty() && !matches!(sc.compiled, Some(Err(_)))
    }

    /// Checks the stop condition after an instruction; stops execution when it is met.
    fn check_stop_condition(&mut self) -> bool {
        let Some(v) = self.stop_value() else { return false };
        let last = self.stop_condition.last.replace(v);
        let met = if self.stop_condition.on_change { last.is_some_and(|l| l != v) } else { v != 0 && last == Some(0) };
        if met {
            self.run = RunMode::Paused;
            let c = &self.session.machine.cpu;
            self.status = if self.stop_condition.on_change {
                format!(
                    "Stopped: {} changed from {} to {v} at {:04X}:{:04X}",
                    self.stop_condition.text,
                    last.unwrap_or(0),
                    c.cs(),
                    c.ip
                )
            } else {
                format!("Stopped: {} is true at {:04X}:{:04X}", self.stop_condition.text, c.cs(), c.ip)
            };
        }
        met
    }

    /// Decides whether execution that reached a breakpoint address should stop there: the line
    /// breakpoint's condition must hold. Counts the hit.
    fn breakpoint_hit(&mut self, pc: u32) -> bool {
        let Some(key) = self.bp_addr.get(&pc).copied() else { return true };
        let Some(bp) = self.bp_lines.get_mut(&key) else { return true };
        if bp.compiled.is_none() && !bp.condition.trim().is_empty() {
            bp.compiled = Some(self.session.compile(&bp.condition));
        }
        let stop = match &bp.compiled {
            Some(Ok(e)) if !bp.condition.trim().is_empty() => e.eval(&self.session.machine) != 0,
            _ => true,
        };
        if stop {
            bp.hits += 1;
        }
        stop
    }

    /// Is there a breakpoint at the current position that should stop execution?
    fn at_breakpoint(&mut self) -> bool {
        let pc = self.session.machine.cpu.pc();
        self.session.machine.breakpoints.contains(&pc) && self.breakpoint_hit(pc)
    }

    pub fn stop(&mut self) {
        if self.run != RunMode::Paused {
            self.run = RunMode::Paused;
            self.status =
                format!("Stopped at {:04X}:{:04X}", self.session.machine.cpu.cs(), self.session.machine.cpu.ip);
        }
    }

    pub fn handle_event(&mut self, ev: Event) {
        let m = &self.session.machine;
        let at = format!("{:04X}:{:04X}", m.cpu.cs(), m.cpu.ip);
        match ev {
            Event::Continue | Event::Budget => {
                if self.run == RunMode::Paused {
                    self.status = format!("CS:IP = {at}");
                }
            }
            Event::Sleeping => {
                self.status = "Waiting (INT 15h/86h)...".into();
            }
            Event::WaitingInput => {
                self.status = "Waiting for keyboard input - type in the emulator screen".into();
                self.screen.focus_request = true;
                self.windows.screen = true;
            }
            Event::Halted => {
                self.run = RunMode::Paused;
                self.status = format!("HLT executed at {at}");
            }
            Event::Terminated(code) => {
                self.run = RunMode::Paused;
                self.status = format!("Program terminated (exit code {code})");
                self.messages.push(Message { text: self.status.clone(), loc: None, span: None, error: false });
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

    /// Advances the emulation for this frame.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.handle_uploads(ctx);
        if self.run == RunMode::Paused || !self.loaded() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let waiting = self.session.machine.is_waiting_input() && self.session.machine.sys.keys.is_empty();
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
                if (now - self.last_step) * 1000.0 >= self.settings.step_delay_ms as f64 {
                    self.last_step = now;
                    self.prev_cpu = self.session.machine.cpu.clone();
                    let ev = self.session.machine.step();
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
                        let ev = self.session.machine.step();
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
                    let ev = self.session.machine.run(self.instr_per_frame);
                    let el = t.elapsed().as_secs_f64();
                    if ev == Event::Budget {
                        // Aim at ~12 ms of emulation per frame.
                        if el < 0.008 {
                            self.instr_per_frame = (self.instr_per_frame * 3 / 2).min(20_000_000);
                        } else if el > 0.016 {
                            self.instr_per_frame = (self.instr_per_frame * 2 / 3).max(1000);
                        }
                    }
                    // A conditional breakpoint whose condition is false: keep going.
                    if ev == Event::Breakpoint && !self.breakpoint_hit(self.session.machine.cpu.pc()) {
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
                        let ev = self.session.machine.step();
                        if ev != Event::Continue {
                            self.handle_event(ev);
                            break 'run;
                        }
                        let pc = self.session.machine.cpu.pc();
                        if targets.contains(&pc) {
                            self.run = RunMode::Paused;
                            let c = &self.session.machine.cpu;
                            self.status = format!("CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
                            break 'run;
                        }
                        if self.at_breakpoint() {
                            self.handle_event(Event::Breakpoint);
                            break 'run;
                        }
                        if self.stop_condition.enabled && self.check_stop_condition() {
                            break 'run;
                        }
                    }
                }
                ctx.request_repaint();
            }
            RunMode::Source(mut goal) => {
                loop {
                    match self.session.run_step_goal(&mut goal, self.instr_per_frame.max(10_000)) {
                        None => self.run = RunMode::Source(goal.clone()),
                        Some(Event::Continue) => {
                            self.run = RunMode::Paused;
                            let c = &self.session.machine.cpu;
                            self.status = format!("CS:IP = {:04X}:{:04X}", c.cs(), c.ip);
                        }
                        Some(Event::Breakpoint) if !self.breakpoint_hit(self.session.machine.cpu.pc()) => {
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
        if self.bp_lines.remove(&(file, line)).is_some() {
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
        self.bp_lines.entry((file, line)).or_default();
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
            let bps = &mut self.session.machine.breakpoints;
            if !bps.remove(&addr) {
                bps.insert(addr);
            }
        }
    }
}
