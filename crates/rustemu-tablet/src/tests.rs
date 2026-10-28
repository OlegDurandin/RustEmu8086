//! UI tests of the tablet layout (egui_kittest); snapshots go to `target/ui-snapshots/*.png`.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rustemu_ui::testing::{app_with_example, example, save};

use crate::{Tab, TabletApp};

fn tablet_harness(app: TabletApp, size: [f32; 2]) -> Harness<'static, TabletApp> {
    Harness::builder()
        .with_size(size)
        .with_max_steps(64)
        .with_context_setup(|ctx| {
            rustemu_ui::fonts::install(ctx);
            crate::theme::apply(ctx);
        })
        .build_ui_state(
            |ui, t: &mut TabletApp| {
                t.app.tick(&ui.ctx().clone());
                crate::tablet_ui(&mut t.app, &mut t.state, ui)
            },
            app,
        )
}

/// Bubble sort a few steps in, with a breakpoint, watches and a selected byte.
fn tablet_app() -> TabletApp {
    let mut app = app_with_example(example("bubble_sort.asm"));
    app.emulate();
    for _ in 0..6 {
        app.single_step();
    }
    let line = app.text.lines().position(|l| l.trim_start().starts_with("loop")).map_or(12, |l| l as u32 + 1);
    app.toggle_breakpoint_at_line(0, line);
    for w in ["ax", "cx * 2"] {
        app.watches.push(rustemu_ui::app::Watch { text: w.into(), compiled: None, before: None });
    }
    let ds = app.session.machine.cpu.sregs[rustemu_core::cpu::DS];
    let mut t = TabletApp::new(app);
    crate::memory::show_address(&mut t.app, &mut t.state.mem, rustemu_core::linear(ds, 0x0102));
    t
}

#[test]
fn tablet_layout_snapshots() {
    for (size, suffix) in [
        ([1376.0, 1032.0], "landscape"),
        ([1180.0, 820.0], "landscape_11in"),
        ([1032.0, 1376.0], "portrait"),
        ([700.0, 980.0], "narrow"),
    ] {
        let mut h = tablet_harness(tablet_app(), size);
        for (tab, name) in [(Tab::Editor, "editor"), (Tab::Debugger, "debugger"), (Tab::Memory, "memory")] {
            h.state_mut().state.tab = tab;
            h.run_steps(4);
            save(&mut h, &format!("30_tablet_{name}_{suffix}"));
        }
    }
}

#[test]
fn tablet_run_step_and_tabs() {
    let mut h = tablet_harness(TabletApp::new(app_with_example(0)), [1376.0, 1032.0]);
    h.run_steps(2);
    // Run assembles the editor text, loads it and runs it to the end.
    h.get_by_label("Run").click();
    h.run_steps(10);
    assert!(h.state().app.session.machine.terminated.is_some(), "{}", h.state().app.status);
    assert!(rustemu_ui::screen::screen_text(&h.state().app.session.machine.sys.mem).contains("Hello, World!"));
    // Reset reloads it; Step executes one instruction.
    h.get_by_label("Reset").click();
    h.run_steps(2);
    assert_eq!(h.state().app.session.machine.cpu.instr_count, 0);
    h.get_by_label("Step").click();
    h.run_steps(2);
    assert_eq!(h.state().app.session.machine.cpu.instr_count, 1);
    h.get_by_label("Debugger").click();
    h.run_steps(2);
    assert_eq!(h.state().state.tab, Tab::Debugger);
    // A flag cell toggles the flag.
    let cf = h.state().app.session.machine.cpu.flags & rustemu_core::cpu::flags::CF;
    h.get_by_label("CF = 0").click();
    h.run_steps(2);
    assert_ne!(h.state().app.session.machine.cpu.flags & rustemu_core::cpu::flags::CF, cf);
}

#[test]
fn tablet_keyboard_types_into_the_program() {
    let i = example("echo.asm");
    let mut h = tablet_harness(TabletApp::new(app_with_example(i)), [1376.0, 1032.0]);
    h.run_steps(2);
    assert!(!h.state().state.keypad.open);
    h.get_by_label("Run").click();
    h.run_steps(6);
    // The keyboard opens when the program waits for input.
    assert!(h.state().app.session.machine.is_waiting_input());
    assert!(h.state().state.keypad.open);
    save(&mut h, "31_tablet_keyboard");
    for key in ["key H", "key Shift", "key I", "key Enter"] {
        // There are two Shift keys.
        h.get_all_by_label(key).next().unwrap().click();
        h.run_steps(2);
    }
    h.run_steps(6);
    assert!(h.state().app.session.machine.terminated.is_some(), "{}", h.state().app.status);
    // Shift applied to "i" only: "hI", printed back in upper case.
    let text = rustemu_ui::screen::screen_text(&h.state().app.session.machine.sys.mem);
    assert!(text.contains("hI") && text.contains("HI"), "{text}");
    // Fn shows the function keys; Hide closes the keyboard.
    h.get_by_label("key Fn").click();
    h.run_steps(2);
    assert!(h.query_by_label("key F10").is_some());
    h.get_by_label("key Hide").click();
    h.run_steps(2);
    assert!(!h.state().state.keypad.open);

    // Portrait: the screen is above the keys.
    let mut h = tablet_harness(TabletApp::new(app_with_example(i)), [1032.0, 1376.0]);
    h.run_steps(2);
    h.get_by_label("Run").click();
    h.run_steps(6);
    for key in ["key H", "key I"] {
        h.get_by_label(key).click();
        h.run_steps(2);
    }
    save(&mut h, "31_tablet_keyboard_portrait");
}

#[test]
fn tool_windows_keep_their_size() {
    // The debug log once reserved too little room for its touch-sized command line and grew
    // every frame.
    let mut t = TabletApp::new(app_with_example(0));
    t.app.windows.console = true;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    // The only window shown.
    let size = |h: &Harness<'_, TabletApp>| {
        h.ctx.memory(|m| {
            let layer = m.areas().visible_layer_ids().into_iter().find(|l| l.order == egui::Order::Middle).unwrap();
            m.area_rect(layer.id).unwrap().size()
        })
    };
    h.run_steps(4);
    let before = size(&h);
    h.run_steps(30);
    assert_eq!(size(&h), before);
}
