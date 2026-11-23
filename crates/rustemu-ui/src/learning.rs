//! Offline exercise checking in private machines, in bounded slices between UI frames.
use crate::{app::App, build_task::Task};
use rustemu_build::{
    Outcome,
    exercise::{Exercise, Termination},
    project::Project,
};
use rustemu_session::{Arch, Event, Session};
use std::collections::VecDeque;

/// Public starter projects; solutions are test fixtures, not bundled into these projects.
pub const EXAMPLES: &[(&str, &str)] = &[
    ("8086 Carry across AL and AH", include_str!("../../../examples/learn/8086-carry.remu")),
    ("8086 Swap using the stack", include_str!("../../../examples/learn/8086-stack.remu")),
    ("MIPS Sum two integers", include_str!("../../../examples/learn/mips-sum.remu")),
    ("MIPS Maximum in an array", include_str!("../../../examples/learn/mips-max.remu")),
    ("8086 MOV byte halves", include_str!("../../../examples/learn/8086-byte-move.remu")),
    ("8086 Store a word in memory", include_str!("../../../examples/learn/8086-array-store.remu")),
    ("8086 LOOP triangular sum", include_str!("../../../examples/learn/8086-loop-sum.remu")),
    ("8086 Keyboard and DOS output", include_str!("../../../examples/learn/8086-key-output.remu")),
    ("MIPS ADDIU wrapping arithmetic", include_str!("../../../examples/learn/mips-addiu.remu")),
    ("MIPS LW SW aligned memory", include_str!("../../../examples/learn/mips-memory.remu")),
    ("MIPS BEQ zero classification", include_str!("../../../examples/learn/mips-zero-branch.remu")),
    ("MIPS JAL JR procedure", include_str!("../../../examples/learn/mips-call.remu")),
];

/// Bounded public feedback for a completed case.
#[derive(Clone, Debug)]
pub struct CaseResult {
    /// Name from the exercise.
    pub name: String,
    /// All conditions matched.
    pub passed: bool,
    /// Violated conditions, expected and observed values (at most 4096 bytes).
    pub details: String,
}

/// One grading run. It owns every machine and never borrows the editor's Session or drive C:.
pub struct CheckRun {
    project: Project,
    outcome: Outcome,
    session: Session,
    input: VecDeque<u8>,
    steps: u64,
    /// Execution time charged to the current case: only time spent inside [`CheckRun::advance`],
    /// so gaps between UI frames (a slow device, a hidden browser tab) never count.
    spent: std::time::Duration,
    /// Completed cases in their original order.
    pub results: Vec<CaseResult>,
}

impl CheckRun {
    /// Starts with a validated package and an assembly response from its restricted request.
    pub fn new(project: Project, outcome: Outcome) -> Result<Self, String> {
        project.validate()?;
        if project.exercise.is_none() {
            return Err("This project has no exercise".into());
        }
        match (&outcome, project.architecture.as_str()) {
            (Outcome::X86(Ok(_)), "8086") | (Outcome::Mips(Ok(_)), "MIPS32") => {}
            (Outcome::X86(Err(e)), _) => return Err(format!("Assembly failed: {e}")),
            (Outcome::Mips(Err(e)), _) => return Err(format!("Assembly failed: {e}")),
            (Outcome::Failed(e), _) => return Err(format!("Assembly failed: {e}")),
            _ => return Err("Architecture of the build does not match the exercise".into()),
        }
        let mut r = Self {
            project,
            outcome,
            session: Session::new(),
            input: VecDeque::new(),
            steps: 0,
            spent: std::time::Duration::ZERO,
            results: Vec::new(),
        };
        r.load_case()?;
        Ok(r)
    }

    /// Every case has a result; no pending work remains.
    pub fn done(&self) -> bool {
        self.results.len() == self.project.exercise.as_ref().unwrap().cases.len()
    }

    fn load_case(&mut self) -> Result<(), String> {
        let case = &self.project.exercise.as_ref().unwrap().cases[self.results.len()];
        self.session = match &self.outcome {
            Outcome::X86(Ok(program)) => {
                let mut s = Session::new();
                s.set_dos_memory_drive(Default::default());
                let m = s.x86_mut().unwrap();
                m.timer_irq = false;
                m.instructions_per_tick = Some(2000);
                m.instant_waits = true;
                s.load_program(program.clone())?;
                s
            }
            Outcome::Mips(Ok(program)) => {
                let mut s = Session::new_mips();
                let m = s.mips_mut().unwrap();
                m.files.storage = rustemu_mips::files::Storage::Memory(Default::default());
                m.dialogs_in_console = true;
                m.instant_waits = true;
                m.program_arguments = self.project.mars.arguments();
                s.load_mips(program.clone(), &self.project.entry)?;
                s
            }
            _ => return Err("No executable program".into()),
        };
        self.session.target.set_history(false);
        for name in case.expected.registers.keys() {
            self.session.target.register_index(name).ok_or_else(|| format!("Unsupported register: {name}"))?;
        }
        for (name, value) in &case.initial_registers {
            let i = self.session.target.register_index(name).ok_or_else(|| format!("Unsupported register: {name}"))?;
            self.session.target.set_register(i, *value);
        }
        self.input = case
            .input
            .bytes()
            .map(|b| if b == b'\n' && self.session.target.arch() == Arch::I8086 { b'\r' } else { b })
            .collect();
        self.steps = 0;
        self.spent = std::time::Duration::ZERO;
        Ok(())
    }

    // The MVP accepts console I/O and program exits only. Time, RNG, host/file services,
    // dialogs, hardware I/O and MIPS MMIO have no reproducible external-event model here.
    fn unsupported_service(&self) -> Option<String> {
        let t = &*self.session.target;
        if let Some(m) = self.session.mips() {
            let pc = t.pc();
            let word = u32::from_le_bytes([t.read8(pc), t.read8(pc + 1), t.read8(pc + 2), t.read8(pc + 3)]);
            if word & 0xfc00003f == 0x0c && !matches!(m.cpu.regs[2], 1 | 4 | 5 | 8 | 10 | 11 | 12 | 17) {
                return Some(format!(
                    "Unsupported in exercises: syscall {} (console I/O and exit only)",
                    m.cpu.regs[2]
                ));
            }
            // Refuse loads/stores into MMIO, including through computed register addresses.
            let op = word >> 26;
            if matches!(op, 0x20..=0x3f) {
                let base = m.cpu.regs[((word >> 21) & 31) as usize];
                let addr = base.wrapping_add(word as i16 as i32 as u32);
                // By the loaded layout: compact configurations put MMIO at 0x7f00, not 0xffff0000.
                if m.mem.layout.segment(addr) == Some(rustemu_mips::Segment::Mmio) {
                    return Some("Unsupported in exercises: MMIO".into());
                }
            }
        } else if t.system_caller().is_none() {
            let m = self.session.x86().unwrap();
            let instruction = t.disassemble(t.pc(), m.cpu.ip as u32);
            let text = instruction.text.to_ascii_uppercase();
            if text
                .split_whitespace()
                .any(|op| matches!(op, "IN" | "OUT" | "INSB" | "INSW" | "OUTSB" | "OUTSW" | "INT3" | "INTO"))
            {
                return Some(format!("Unsupported in exercises: {text}"));
            }
            if text.split_whitespace().any(|op| op == "INT") {
                let vector = t.read8(t.pc().wrapping_add(instruction.len.saturating_sub(1)));
                let ah = m.cpu.regs[rustemu_core::cpu::AX] >> 8;
                if !(vector == 0x20
                    || vector == 0x16 && matches!(ah, 0 | 0x10)
                    || vector == 0x21 && matches!(ah, 1 | 2 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 0x4c))
                {
                    return Some(format!("Unsupported in exercises: INT {vector:02X}h / AH={ah:02X}h"));
                }
            }
        }
        None
    }

    /// Executes at most 1024 dispatches or 4 ms; the caller schedules the next UI frame.
    pub fn advance(&mut self) {
        if self.done() {
            return;
        }
        let frame = web_time::Instant::now();
        for _ in 0..1024 {
            let e = self.project.exercise.as_ref().unwrap();
            let failure = if self.steps >= e.step_limit {
                Some("Step limit exceeded".into())
            } else if (self.spent + frame.elapsed()).as_millis() >= u128::from(e.time_limit_ms) {
                Some("Timed out".into())
            } else if self.session.target.output().len() > 16384 {
                Some("Output limit exceeded".into())
            } else {
                self.unsupported_service()
            };
            if let Some(reason) = failure {
                self.finish(None, Some(reason));
                return;
            }
            if !self.session.target.has_pending_input() {
                let limit = if self.session.target.arch() == Arch::I8086 { 64 } else { 4096 };
                for _ in 0..limit {
                    if let Some(b) = self.input.pop_front() {
                        self.session.target.push_char(b);
                    } else {
                        break;
                    }
                }
            }
            self.steps += 1;
            let ev = self.session.target.step();
            match ev {
                Event::Continue | Event::Budget => {}
                Event::Terminated(code) => {
                    self.finish(Some(Termination::Exited(code)), None);
                    return;
                }
                Event::Halted => {
                    self.finish(Some(Termination::Halted), None);
                    return;
                }
                Event::WaitingInput if !self.input.is_empty() => {}
                Event::WaitingInput => {
                    self.finish(
                        None,
                        Some("Input exhausted: the program requested more input than the case provides".into()),
                    );
                    return;
                }
                Event::Error(e) => {
                    self.finish(None, Some(format!("Execution fault: {e}")));
                    return;
                }
                ev => {
                    self.finish(None, Some(format!("Unsupported execution event: {ev:?}")));
                    return;
                }
            }
            if frame.elapsed().as_millis() >= 4 {
                break;
            }
        }
        // Reached only while the same case is still running; `finish` resets the budget for the next.
        self.spent += frame.elapsed();
    }

    fn finish(&mut self, termination: Option<Termination>, failure: Option<String>) {
        let case = &self.project.exercise.as_ref().unwrap().cases[self.results.len()];
        let mut errors = Vec::new();
        if let Some(failure) = failure {
            errors.push(failure);
        } else {
            if termination.as_ref() != Some(&case.expected.termination) {
                errors.push(format!("Termination: expected {:?}, got {termination:?}", case.expected.termination));
            }
            if let Some(want) = &case.expected.output {
                let got = String::from_utf8_lossy(self.session.target.output()).replace("\r\n", "\n");
                if got != want.replace("\r\n", "\n") {
                    errors.push(format!(
                        "Output: expected {want:?}, got {:?}",
                        got.chars().take(4096).collect::<String>()
                    ));
                }
            }
            for (name, want) in &case.expected.registers {
                let i = self.session.target.register_index(name).unwrap();
                let got = self.session.target.register(i);
                if got != *want {
                    errors.push(format!("Register {name}: expected {want:#X}, got {got:#X}"));
                }
            }
            for mem in &case.expected.memory {
                let got = self.session.target.read_bytes(mem.address, mem.bytes.len());
                if got != mem.bytes {
                    errors.push(format!("Memory {:#X}: expected {:02X?}, got {got:02X?}", mem.address, mem.bytes));
                }
            }
        }
        self.results.push(CaseResult {
            name: case.name.clone(),
            passed: errors.is_empty(),
            details: if errors.is_empty() {
                "All specified conditions matched".into()
            } else {
                errors.join("\n").chars().take(4096).collect()
            },
        });
        if !self.done()
            && let Err(e) = self.load_case()
        {
            while !self.done() {
                let case = &self.project.exercise.as_ref().unwrap().cases[self.results.len()];
                self.results.push(CaseResult { name: case.name.clone(), passed: false, details: e.clone() });
            }
        }
    }
}

/// UI-owned background build and private checker; never persisted as live machine state.
#[derive(Default)]
pub struct LearningState {
    exercise: Option<Box<Exercise>>,
    source: String,
    context: Option<Project>,
    mars: Option<rustemu_build::mars::MarsSettings>,
    syntax: Option<crate::app::SyntaxChoice>,
    open: bool,
    task: Option<Task>,
    project: Option<Project>,
    started: Option<web_time::Instant>,
    run: Option<CheckRun>,
    /// Last completed public results.
    pub results: Vec<CaseResult>,
    message: String,
}

/// Examples / Learn menu for both layouts.
pub fn menu(app: &mut App, ui: &mut egui::Ui) {
    for arch in ["8086", "MIPS"] {
        ui.menu_button(arch, |ui| {
            for (i, (title, _)) in EXAMPLES.iter().enumerate().filter(|(_, (title, _))| title.starts_with(arch)) {
                if ui.button(*title).clicked() {
                    open_example(app, i);
                    ui.close();
                }
            }
        });
    }
}

/// Opens a bundled exercise using the normal unsaved-work and architecture guards.
pub fn open_example(app: &mut App, index: usize) {
    let Some((_, text)) = EXAMPLES.get(index) else {
        return;
    };
    match Project::decode(text.as_bytes()) {
        Ok(p) => app.open_link(crate::share::Shared {
            name: p.entry.clone(),
            text: p.files[&p.entry].clone(),
            arch: Arch::from_name(&p.architecture).unwrap(),
            project: Some(p),
            run: false,
        }),
        Err(e) => app.notice("Learn", &e),
    }
}

/// Reopens the exercise card without resetting or replacing the source.
pub fn entry_ui(app: &mut App, ui: &mut egui::Ui) {
    if app.doc.project.as_ref().is_some_and(|p| p.exercise.is_some()) && ui.button("Check solution").clicked() {
        app.learning.open = true;
    }
}

/// Advances the private checker and draws the exercise card; used on desktop and WASM.
pub fn window(app: &mut App, ctx: &egui::Context) {
    let Some(exercise) = app.doc.project.as_ref().and_then(|p| p.exercise.clone()) else {
        app.learning = LearningState::default();
        return;
    };
    let mut state = std::mem::take(&mut app.learning);
    if state.exercise.as_ref() != Some(&exercise) {
        state = LearningState {
            exercise: Some(exercise.clone()),
            open: true,
            source: app.doc.text.clone(),
            context: app.doc.project.clone(),
            mars: Some(app.settings.mars.clone()),
            syntax: Some(app.settings.syntax),
            ..Default::default()
        };
    }
    if state.source != app.doc.text
        || state.context.as_ref() != app.doc.project.as_ref()
        || state.mars.as_ref() != Some(&app.settings.mars)
        || state.syntax != Some(app.settings.syntax)
    {
        state.task = None;
        state.run = None;
        state.project = None;
        state.results.clear();
        state.message = "Source or settings changed. Check the current solution again.".into();
        state.source = app.doc.text.clone();
        state.context = app.doc.project.clone();
        state.mars = Some(app.settings.mars.clone());
        state.syntax = Some(app.settings.syntax);
    }
    if let Some(task) = &state.task {
        if let Some(result) = task.poll() {
            state.task = None;
            match result.and_then(|response| CheckRun::new(state.project.take().unwrap(), response.outcome)) {
                Ok(run) => state.run = Some(run),
                Err(e) => state.message = e,
            }
        } else if state.started.is_some_and(|t| t.elapsed().as_millis() >= 10000) {
            state.task = None;
            state.project = None;
            state.message = "Timed out while assembling. Check again or simplify the source.".into();
        }
    }
    if let Some(run) = &mut state.run {
        run.advance();
        state.results = run.results.clone();
        if run.done() {
            state.run = None;
            state.message = "Check finished. Every displayed case is public.".into();
        }
    }
    if state.task.is_some() || state.run.is_some() {
        ctx.request_repaint();
    }
    if state.open {
        egui::Window::new("Learn / Учебное задание").id(egui::Id::unique("exercise_card")).default_width(500.0)
            .default_height(600.0).default_pos(egui::pos2(60.0,100.0))
            .max_height((ctx.content_rect().height()-64.0).max(160.0))
            .max_width((ctx.content_rect().width()-32.0).max(200.0))
            .vscroll(true).show(ctx,|ui| {
                ui.heading(&exercise.title);ui.label(&exercise.description);
                if ui.add(egui::Button::new("Edit code").min_size(egui::vec2(0.0,44.0))).clicked() {state.open=false;}
                ui.weak("Edit code closes this card before typing or opening the system keyboard. Your solution is kept.");
                ui.collapsing("Hint",|ui|{ui.label(&exercise.hint);});
                ui.collapsing("Public test cases", |ui| {
                    for case in &exercise.cases {
                        ui.strong(&case.name);
                        ui.label(format!("Input: {:?}; initial registers: {:?}",case.input,case.initial_registers));
                        ui.label(format!("Expected: {:?}",case.expected));
                    }
                });
                ui.weak("Export .remu carries this exercise, sources, settings and public cases. It does not carry live CPU state.");
                if ui.add(egui::Button::new("Проверить решение / Check solution").min_size(egui::vec2(0.0,44.0))).clicked() {
                    state.task=None;state.run=None;state.results.clear();state.project=None;
                    match app.portable_project().and_then(|p|Task::start(p.request(0)).map(|task|(p,task))) {
                        Ok((p,task))=>{state.project=Some(p);state.task=Some(task);state.started=Some(web_time::Instant::now());state.message="Assembling in the background…".into();},
                        Err(e)=>state.message=e,
                    }
                }
                if (state.task.is_some()||state.run.is_some()) && ui.add(egui::Button::new("Cancel check").min_size(egui::vec2(0.0,44.0))).clicked() {
                    state.task=None;state.run=None;state.project=None;state.message="Check cancelled".into();
                }
                ui.label(&state.message);
                for r in &state.results {
                    ui.strong(format!("{}: {}",r.name,if r.passed {"passed"} else {"failed"}));ui.label(&r.details);
                }
                if ui.add(egui::Button::new("Return to code").min_size(egui::vec2(0.0,44.0))).clicked() {state.open=false;}
            });
    }
    app.learning = state;
}
