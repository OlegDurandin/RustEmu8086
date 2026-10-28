//! The application in the desktop or the tablet layout, switchable at run time (options window,
//! tablet menu) without losing anything: both wrap the same `App`.

use rustemu_desktop::DesktopApp;
use rustemu_tablet::TabletApp;
use rustemu_ui::app::App;

enum Layout {
    Desktop(DesktopApp),
    Tablet(TabletApp),
}

pub struct WebApp {
    layout: Option<Layout>,
    /// Called with the new layout (`true`: tablet) after a switch, to remember it.
    on_switch: fn(bool),
}

impl WebApp {
    pub fn new(mut app: App, tablet: bool, ctx: &egui::Context, on_switch: fn(bool)) -> Self {
        app.can_switch_layout = true;
        WebApp { layout: Some(wrap(app, tablet, ctx)), on_switch }
    }

    #[cfg(test)]
    fn app(&self) -> &App {
        match self.layout.as_ref().expect("layout") {
            Layout::Desktop(d) => &d.0,
            Layout::Tablet(t) => &t.app,
        }
    }

    fn app_mut(&mut self) -> &mut App {
        match self.layout.as_mut().expect("layout") {
            Layout::Desktop(d) => &mut d.0,
            Layout::Tablet(t) => &mut t.app,
        }
    }
}

fn wrap(mut app: App, tablet: bool, ctx: &egui::Context) -> Layout {
    if tablet {
        let t = TabletApp::new(app);
        t.app.apply_theme(ctx);
        Layout::Tablet(t)
    } else {
        // Undo the tablet theme: touch-sized spacing and its dark colours.
        app.fixed_theme = None;
        app.current_line_color = None;
        app.tablet_layout = false;
        ctx.set_style_of(egui::Theme::Dark, egui::Style::default());
        app.apply_theme(ctx);
        Layout::Desktop(DesktopApp(app))
    }
}

impl eframe::App for WebApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app_mut().persist(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        match self.layout.as_mut().expect("layout") {
            Layout::Desktop(d) => d.ui(ui, frame),
            Layout::Tablet(t) => t.ui(ui, frame),
        }
        if !std::mem::take(&mut self.app_mut().switch_layout) {
            return;
        }
        let (app, tablet) = match self.layout.take().expect("layout") {
            Layout::Desktop(d) => (d.0, true),
            Layout::Tablet(t) => (t.app, false),
        };
        self.layout = Some(wrap(app, tablet, ui.ctx()));
        (self.on_switch)(tablet);
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use rustemu_ui::testing::{app_with_example, save};

    use super::*;

    #[test]
    fn switching_layouts_keeps_the_program_and_the_theme() {
        let mut h = Harness::builder().with_size([1376.0, 1032.0]).with_max_steps(64).build_eframe(|cc| {
            rustemu_ui::fonts::install(&cc.egui_ctx);
            let mut app = app_with_example(0);
            app.emulate();
            app.single_step();
            WebApp::new(app, false, &cc.egui_ctx, |_| {})
        });
        h.run_steps(2);
        let spacing = h.ctx.style_of(egui::Theme::Dark).spacing.interact_size.y;
        // Desktop -> tablet from the options window.
        h.state_mut().app_mut().options_open = true;
        h.run_steps(2);
        h.get_by_label("tablet (touch)").click();
        h.run_steps(4);
        assert!(h.state().app().tablet_layout);
        assert!(h.query_by_label("Run").is_some(), "the tablet top bar is shown");
        assert_eq!(h.state().app().session.machine.cpu.instr_count, 1, "the loaded program stays");
        save(&mut h, "32_web_switch_to_tablet");
        // Tablet -> desktop from the menu: the desktop gets its own style back.
        h.state_mut().app_mut().options_open = false;
        h.state_mut().app_mut().switch_layout = true;
        h.run_steps(4);
        assert!(!h.state().app().tablet_layout);
        assert!(h.state().app().fixed_theme.is_none());
        assert_eq!(h.ctx.style_of(egui::Theme::Dark).spacing.interact_size.y, spacing);
        assert!(h.query_by_label_contains("emulate").is_some(), "the desktop toolbar is shown");
    }
}
