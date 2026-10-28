//! The tablet layout of RustEmu8086 (used by the web version on iPads and touch screens): a top
//! bar with the run controls, Editor / Debugger / Memory tabs of fixed cards and a status bar, in a
//! dark theme sized for touch, plus an on-screen PC keyboard. Dialogs and tool windows are the
//! shared ones of `rustemu_ui`.

mod bars;
mod cards;
mod keypad;
mod memory;
mod tabs;
#[cfg(test)]
mod tests;
mod theme;
mod widgets;

use egui::Ui;
use rustemu_ui::app::{App, RunMode};

use crate::bars::{status_bar, tabs_row, top_bar};
use crate::keypad::{KeypadState, keyboard_sheet};
use crate::memory::MemoryView;
use crate::tabs::{debugger_tab, editor_tab, memory_tab};
use crate::theme::pal;
use crate::widgets::GAP;

/// The application: the shared state and the tablet layout's own.
pub struct TabletApp {
    pub app: App,
    pub state: TabletState,
}

impl TabletApp {
    /// The layout has its own theme (installed by `App::apply_theme`).
    pub fn new(mut app: App) -> Self {
        app.fixed_theme = Some(theme::apply);
        app.current_line_color = Some(pal::AMBER);
        app.tablet_layout = true;
        TabletApp { app, state: TabletState::default() }
    }
}

impl eframe::App for TabletApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app.persist(storage);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.app.shortcuts(&ctx);
        self.app.tick(&ctx);
        tablet_ui(&mut self.app, &mut self.state, ui);
        self.app.confirm_close(&ctx);
    }
}

/// Shows the layout in a desktop window (`main.rs`, for development). The web version starts
/// [`TabletApp`] itself.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(options: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native(
        "RustEmu8086",
        options,
        Box::new(|cc| {
            let t = TabletApp::new(App::new(cc));
            t.app.apply_theme(&cc.egui_ctx);
            Ok(Box::new(t))
        }),
    )
}

/// State of the tablet layout.
#[derive(Default)]
pub struct TabletState {
    pub(crate) tab: Tab,
    pub(crate) output: Output,
    pub(crate) mem: MemoryView,
    pub(crate) keypad: KeypadState,
    /// Errors were shown in the last frame (a new build error switches to the messages).
    had_errors: bool,
    /// Instructions per second while running: (time, instruction count) of the last sample.
    speed_sample: (f64, u64),
    pub(crate) speed: f64,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tab {
    #[default]
    Editor,
    Debugger,
    Memory,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Output {
    #[default]
    Screen,
    Messages,
}

fn tablet_ui(app: &mut App, t: &mut TabletState, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    app.screen.focused = false;
    follow_errors(app, t);
    keypad::follow_input(app, &mut t.keypad);
    measure_speed(app, t, ui);
    egui::Panel::top("tablet_top")
        .frame(egui::Frame::new().fill(pal::BAR).inner_margin(egui::Margin::symmetric(16, 10)))
        .show(ui, |ui| top_bar(app, ui));
    egui::Panel::bottom("tablet_status")
        .frame(egui::Frame::new().fill(pal::BAR).inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 8 }))
        .show(ui, |ui| status_bar(app, t, ui));
    if t.keypad.open {
        egui::Panel::bottom("tablet_keys")
            .frame(egui::Frame::new().fill(pal::BAR).inner_margin(egui::Margin::same(10)))
            .show(ui, |ui| keyboard_sheet(app, &mut t.keypad, ui));
    }
    egui::CentralPanel::default().frame(egui::Frame::new().fill(pal::BG).inner_margin(egui::Margin::same(10))).show(
        ui,
        |ui| {
            tabs_row(app, t, ui);
            ui.add_space(GAP);
            let rect = ui.available_rect_before_wrap();
            match t.tab {
                Tab::Editor => editor_tab(app, t, ui, rect),
                Tab::Debugger => debugger_tab(app, t, ui, rect),
                Tab::Memory => memory_tab(app, t, ui, rect),
            }
        },
    );
    rustemu_ui::windows::shared_windows(app, &ctx);
}

/// A new build error shows the messages.
fn follow_errors(app: &mut App, t: &mut TabletState) {
    let errors = app.messages.iter().any(|m| m.error);
    if errors && !t.had_errors {
        t.output = Output::Messages;
        t.tab = Tab::Editor;
    }
    t.had_errors = errors;
}

fn measure_speed(app: &mut App, t: &mut TabletState, ui: &Ui) {
    let now = ui.input(|i| i.time);
    let count = app.session.machine.cpu.instr_count;
    let (t0, c0) = t.speed_sample;
    if app.run == RunMode::Paused {
        t.speed_sample = (now, count);
    } else if now - t0 >= 0.5 {
        t.speed = count.saturating_sub(c0) as f64 / (now - t0);
        t.speed_sample = (now, count);
    }
}
