//! RustEmu8086 for tablets (iPadOS, Android): a top bar with the run controls, Editor / Debugger /
//! Memory tabs of fixed cards and a status bar, in a dark theme sized for touch, plus an on-screen
//! PC keyboard. Dialogs and tool windows are the shared ones of `rustemu_ui`.

mod bars;
mod cards;
#[cfg(target_os = "ios")]
mod ios;
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
    #[cfg(target_os = "ios")]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        for _ in 0..ios::take_return_keys() {
            for pressed in [true, false] {
                raw_input.events.push(egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
    }

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

/// Runs the application (iPadOS and the Mac: `main.rs`; Android: [`android_main`]). The web
/// version starts [`TabletApp`] itself.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(options: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native(
        "RustEmu8086",
        options,
        Box::new(|cc| {
            #[cfg(target_os = "ios")]
            {
                ios::attach_to_scene(cc);
                ios::catch_return_key();
            }
            let t = TabletApp::new(App::new(cc));
            t.app.apply_theme(&cc.egui_ctx);
            Ok(Box::new(t))
        }),
    )
}

/// Android entry point, called by the NativeActivity glue (the library is built as a `cdylib`).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let options = eframe::NativeOptions { android_app: Some(app), ..rustemu_ui::native_options() };
    if let Err(e) = run(options) {
        eprintln!("RustEmu8086: {e}");
    }
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
    // On iPadOS the bottom edge belongs to the home indicator.
    let bottom = if cfg!(target_os = "ios") { 22 } else { 8 };
    egui::Panel::bottom("tablet_status")
        .frame(egui::Frame::new().fill(pal::BAR).inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom }))
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
