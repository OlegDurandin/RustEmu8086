//! Build submission, cancellation and revision-checked completion for both UI layouts.
use super::*;

/// What to do with an accepted build.
#[derive(Clone, Copy)]
pub(crate) enum BuildAction {
    Compile,
    Emulate,
}

/// Execution command to continue after a successful load.
#[derive(Clone, Copy)]
pub(super) enum BuildStep {
    Into,
    Over,
    Out,
}

/// The snapshot against which a worker result is validated.
pub(crate) struct PendingBuild {
    task: crate::build_task::Task,
    request: rustemu_build::Request,
    action: BuildAction,
    run_after: bool,
    step_after: Option<BuildStep>,
    started: web_time::Instant,
    syntax: SyntaxChoice,
    mars: crate::mars_settings::MarsSettings,
}

impl App {
    /// Whether assembly is running independently of the UI.
    pub fn is_building(&self) -> bool {
        self.build.pending.is_some()
    }

    /// Cancels without waiting for a task; a late result can no longer be applied.
    pub fn cancel_build(&mut self) {
        if self.build.pending.take().is_some() {
            self.build.failed = true;
            self.status = "Assembly cancelled".into();
        }
    }

    /// Advances the source revision and cancels any job for an older snapshot.
    pub fn source_changed(&mut self) {
        self.build.document_version = self.build.document_version.wrapping_add(1);
        self.cancel_build();
    }

    /// Visible progress and cancellation while an assembler task owns the request.
    pub fn build_status_ui(&mut self, ui: &mut egui::Ui) {
        if self.is_building() {
            ui.spinner();
            ui.label("Assembling…");
            if ui.button("Cancel assembly").on_hover_text("Escape / F5").clicked() {
                self.cancel_build();
            }
        }
    }

    /// Live machine/file/device counters and the most recent assembler input counters.
    pub fn resource_usage(&self) -> rustemu_target::resources::ResourceBudget {
        let mut b = self.session.target.resources();
        b.usage.extend(self.build.resource_usage.usage.clone());
        b
    }

    /// Current sources and settings, restricted to the package when one is open.
    pub(crate) fn build_request(&self, version: u64) -> rustemu_build::Request {
        if let Some(mut project) = self.doc.project.clone() {
            project.files.insert(project.entry.clone(), self.doc.text.clone());
            project.mars = self.settings.mars.clone();
            project.dialect = self.settings.syntax.dialect();
            return project.request(version);
        }
        let mut files = std::collections::BTreeMap::new();
        if let Some(drive) = self.x86_session().and_then(|s| s.dos_memory_drive()) {
            for (name, bytes) in drive.files() {
                files.insert(name.into(), String::from_utf8_lossy(bytes).into_owned());
            }
        }
        for &(name, source) in crate::examples::MIPS_INCLUDES.iter() {
            files.entry(name.into()).or_insert_with(|| source.into());
        }
        // Built-in example include names also work in the browser.
        for example in crate::examples::MIPS_EXAMPLES {
            files.entry(example.1.into()).or_insert_with(|| example.2.into());
        }
        rustemu_build::Request {
            version,
            mips: self.arch() == Arch::Mips32,
            name: self.doc.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| self.file_name()),
            text: self.doc.text.clone(),
            dialect: self.settings.syntax.dialect(),
            mars: self.settings.mars.mips(),
            files,
            roots: self.vdrive(self.doc.path.as_deref()).into_iter().collect(),
            portable: false,
            host: !self.front.browser,
            exception_handler: (self.settings.mars.exception_handler_enabled
                && !self.settings.mars.exception_handler.trim().is_empty())
            .then(|| self.settings.mars.exception_handler.clone()),
            project: self.settings.mars.assemble_all,
        }
    }

    pub(super) fn submit_build(&mut self, action: BuildAction) -> bool {
        if !self.front.background_builds {
            if self.doc.project.is_some() && self.build.completed.is_none() {
                let r = rustemu_build::execute(
                    self.build_request(self.build.document_version),
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                );
                self.build.sources = r.sources;
                if let rustemu_build::Outcome::Failed(e) = r.outcome {
                    self.build.failed = true;
                    self.notice("Project assembly", &e);
                    return true;
                }
                self.build.completed = Some(r.outcome);
            }
            return false;
        }
        self.cancel_build();
        self.run = RunMode::Paused;
        self.build.failed = true;
        self.build.messages.clear();
        self.clear_errors();
        if self.doc.text.len() > rustemu_mips::asm::MAX_SOURCE_BYTES {
            self.status = "Source byte budget exceeded".into();
            self.build.messages.push(Message { text: self.status.clone(), loc: None, span: None, error: true });
            return true;
        }
        self.build.document_version = self.build.document_version.wrapping_add(1);
        let request = self.build_request(self.build.document_version);
        match crate::build_task::Task::start(request.clone()) {
            Ok(task) => {
                self.build.pending = Some(PendingBuild {
                    task,
                    request,
                    action,
                    run_after: false,
                    step_after: None,
                    started: web_time::Instant::now(),
                    syntax: self.settings.syntax,
                    mars: self.settings.mars.clone(),
                });
                self.status = "Assembling…".into();
            }
            Err(e) => {
                self.status = "Assembly failed".into();
                self.build.messages.push(Message { text: e, loc: None, span: None, error: true });
            }
        }
        true
    }

    pub(super) fn run_after_build(&mut self) {
        if let Some(p) = &mut self.build.pending {
            p.run_after = true;
            p.step_after = None;
            p.action = BuildAction::Emulate;
        }
    }

    pub(super) fn step_after_build(&mut self, step: BuildStep) {
        if let Some(p) = &mut self.build.pending {
            p.action = BuildAction::Emulate;
            p.run_after = false;
            p.step_after = Some(step);
        }
    }

    /// Receives a worker result only if it still belongs to this document and configuration.
    pub fn poll_build(&mut self, ctx: &egui::Context) {
        let Some(p) = self.build.pending.as_ref() else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        let current_name = self.doc.project.as_ref().map(|p| p.entry.clone()).unwrap_or_else(|| {
            self.doc.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| self.file_name())
        });
        if p.request.version != self.build.document_version
            || p.request.mips != (self.arch() == Arch::Mips32)
            || p.request.name != current_name
            || p.request.text != self.doc.text
            || p.syntax != self.settings.syntax
            || p.mars != self.settings.mars
        {
            self.cancel_build();
            self.status = "Source or settings changed — build discarded".into();
            return;
        }
        let result = match p.task.poll() {
            Some(r) => r,
            None if p.started.elapsed().as_millis()
                > rustemu_target::resources::ResourceBudget::limit(rustemu_target::resources::Resource::BuildTimeMs)
                    as u128 =>
            {
                Err("Assembly worker timed out".into())
            }
            None => return,
        };
        let p = self.build.pending.take().unwrap();
        let result = match result {
            Ok(r) if r.version == p.request.version => r,
            Ok(_) => return,
            Err(e) => {
                self.status = "Assembly failed".into();
                self.build.messages.push(Message { text: e, loc: None, span: None, error: true });
                return;
            }
        };
        self.build.sources = result.sources;
        self.build.resource_usage = rustemu_target::resources::ResourceBudget::default();
        self.build
            .resource_usage
            .record(rustemu_target::resources::Resource::BuildTimeMs, p.started.elapsed().as_millis() as usize);
        self.build.resource_usage.record(rustemu_target::resources::Resource::SourceBytes, result.source_bytes);
        self.build.resource_usage.record(rustemu_target::resources::Resource::SourceLines, result.source_lines);
        self.build.resource_usage.record(rustemu_target::resources::Resource::SourceFiles, result.source_files);
        use rustemu_target::resources::Resource;
        match &result.outcome {
            rustemu_build::Outcome::X86(Ok(p)) => {
                self.build.resource_usage.record(Resource::ExpandedBytes, p.expanded_bytes)
            }
            rustemu_build::Outcome::Mips(Ok(p)) => {
                self.build.resource_usage.record(Resource::ExpandedBytes, p.expanded_bytes);
                self.build.resource_usage.record(Resource::ProcessedTokens, p.processed_tokens);
                self.build.resource_usage.record(Resource::Diagnostics, p.warnings.len());
            }
            rustemu_build::Outcome::X86(Err(e)) => {
                self.build.resource_usage.record(Resource::Diagnostics, e.errors.len())
            }
            rustemu_build::Outcome::Mips(Err(e)) => {
                self.build.resource_usage.record(Resource::Diagnostics, e.errors.len())
            }
            _ => {}
        }
        if let rustemu_build::Outcome::Failed(e) = result.outcome {
            self.status = "Assembly failed".into();
            self.build.messages.push(Message { text: e, loc: None, span: None, error: true });
            return;
        }
        self.build.completed = Some(result.outcome);
        self.front.background_builds = false;
        match p.action {
            BuildAction::Compile => self.compile(),
            BuildAction::Emulate => self.emulate(),
        }
        self.front.background_builds = true;
        if matches!(p.action, BuildAction::Emulate) && !self.build.failed {
            self.build.loaded_version = Some(self.build.document_version);
        }
        if !self.build.failed
            && let Some(step) = p.step_after
        {
            match step {
                BuildStep::Into => self.single_step(),
                BuildStep::Over => self.step_over(),
                BuildStep::Out => self.step_out(),
            }
        }
        if p.run_after && !self.build.failed {
            self.start_run();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app(arch: Arch) -> App {
        let mut a = App::with_settings(Settings { last_file: None, ..Settings::default() });
        a.switch_arch(arch);
        a.front.background_builds = true;
        a.doc.text = if arch == Arch::Mips32 {
            ".text\nmain: li $v0, 10\nsyscall\n"
        } else {
            "org 100h\nmov ax, 4c00h\nint 21h\n"
        }
        .into();
        a
    }
    fn complete(a: &mut App) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let ctx = egui::Context::default();
        while a.is_building() {
            a.poll_build(&ctx);
            assert!(std::time::Instant::now() < deadline, "build did not finish");
            std::thread::yield_now();
        }
    }
    #[test]
    fn background_builds_both_architectures_and_reports_usage() {
        for arch in [Arch::I8086, Arch::Mips32] {
            let mut a = app(arch);
            a.emulate();
            assert!(a.is_building());
            complete(&mut a);
            assert!(!a.build.failed);
            assert!(!a.build.resource_usage.usage.is_empty());
        }
    }
    #[test]
    fn revision_edit_stop_and_arch_switch_discard_results() {
        let mut a = app(Arch::I8086);
        a.emulate();
        a.doc.text.push_str("; edited");
        complete(&mut a);
        assert!(a.build.failed);
        assert!(a.status.contains("discarded"));
        a.emulate();
        a.stop();
        assert!(!a.is_building());
        a.emulate();
        a.doc.saved_text = a.doc.text.clone();
        a.switch_arch(Arch::Mips32);
        assert!(!a.is_building());
        a.switch_arch(Arch::I8086);
        assert!(!a.is_building());
        a.emulate();
        complete(&mut a);
        assert!(!a.build.failed);
    }
    #[test]
    fn step_and_run_continue_after_successful_build() {
        for arch in [Arch::I8086, Arch::Mips32] {
            let mut a = app(arch);
            a.single_step();
            complete(&mut a);
            assert!(!a.build.failed);
            assert!(a.session.target.instr_count() > 0);
            a.doc.text.push('\n');
            a.start_run();
            complete(&mut a);
            assert!(!matches!(a.run, RunMode::Paused));
            a.stop();
        }
    }

    #[test]
    fn a_new_document_with_identical_text_requires_a_new_machine_load() {
        for arch in [Arch::I8086, Arch::Mips32] {
            let mut a = app(arch);
            a.emulate();
            complete(&mut a);
            let loaded = a.build.loaded_version;
            a.source_changed();
            assert_ne!(loaded, Some(a.build.document_version));
            a.start_run();
            assert!(a.is_building());
            assert!(matches!(a.run, RunMode::Paused));
            complete(&mut a);
            assert_eq!(a.build.loaded_version, Some(a.build.document_version));
            assert!(!matches!(a.run, RunMode::Paused));
            a.stop();
        }
    }

    #[test]
    fn loading_a_binary_does_not_schedule_source_assembly() {
        let mut a = app(Arch::I8086);
        a.session.load_bytes(rustemu_session::Loadable::Com, vec![0x90, 0xf4]).unwrap();
        a.program.text = None;
        a.build.failed = false;
        a.single_step();
        assert!(!a.is_building());
        assert_eq!(a.session.target.instr_count(), 1);
    }

    #[test]
    fn portable_link_builds_in_a_worker_and_runs_with_its_internal_entry_path() {
        let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/projects/mips-input.remu"));
        let mut p = rustemu_build::project::Project::decode(bytes).unwrap();
        p.files = p.files.into_iter().map(|(name, text)| (format!("src/{name}"), text)).collect();
        p.entry = format!("src/{}", p.entry);
        let mut a = app(Arch::Mips32);
        a.doc.saved_text = a.doc.text.clone();
        let shared = crate::share::parse(&crate::share::project_fragment(&p, true).unwrap()).unwrap();
        a.open_link(shared);
        assert!(a.is_building());
        complete(&mut a);
        assert!(!a.build.failed, "{}", a.status);
        assert!(a.loaded());
        assert!(!matches!(a.run, RunMode::Paused));
        assert_eq!(a.build.sources.len(), 2);
        assert!(a.session.target.has_pending_input());
        a.stop();
    }

    #[test]
    fn failed_background_build_cannot_run_old_program() {
        let mut a = app(Arch::I8086);
        a.emulate();
        complete(&mut a);
        a.doc.text = "invalid instruction".into();
        a.emulate();
        complete(&mut a);
        assert!(a.build.failed);
        a.start_run();
        assert!(matches!(a.run, RunMode::Paused));
        a.cancel_build();
    }
}
