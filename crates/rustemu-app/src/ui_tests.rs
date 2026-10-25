//! Headless UI tests: drive the app with egui_kittest and render snapshots to
//! `target/ui-snapshots/*.png` for visual inspection.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::{App, Settings};

fn harness(app: App) -> Harness<'static, App> {
    let h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_max_steps(64)
        .build_ui_state(|ui, app: &mut App| crate::ui::main_ui(app, ui), app);
    crate::fonts::install(&h.ctx);
    h.state().apply_theme(&h.ctx);
    h
}

fn save(h: &mut Harness<'_, App>, name: &str) {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    std::fs::create_dir_all(&dir).unwrap();
    match h.render() {
        Ok(img) => img.save(dir.join(format!("{name}.png"))).unwrap(),
        Err(e) => eprintln!("render failed (no GPU?): {e}"),
    }
}

fn app_with_example(i: usize) -> App {
    let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
    app.load_example(i);
    app
}

#[test]
fn editor_and_emulate_hello() {
    let mut h = harness(app_with_example(0));
    h.run();
    save(&mut h, "01_editor");
    h.get_by_label("▶ emulate").click();
    h.run();
    assert!(h.state().loaded());
    // Run the program to completion.
    h.state_mut().start_run();
    for _ in 0..20 {
        let ctx = h.ctx.clone();
        h.state_mut().tick(&ctx);
        h.step();
    }
    h.run_steps(2);
    let out = String::from_utf8_lossy(&h.state().session.machine.output).into_owned();
    assert_eq!(out, "Hello, World!\r\n");
    assert!(h.state().session.machine.screen_text().starts_with("Hello, World!"));
    save(&mut h, "02_emulator_hello");
}

#[test]
fn compile_error_is_reported() {
    let mut app = app_with_example(0);
    app.text = "org 100h\nmov ax, bx\nfoo 1\nmov al, 300\n".into();
    let mut h = harness(app);
    h.run();
    h.get_by_label("▶ emulate").click();
    h.run();
    assert_eq!(h.state().error_lines, vec![3, 4]);
    save(&mut h, "03_errors");
}

#[test]
fn stepping_and_debug_windows() {
    let mut h = harness(app_with_example(5)); // bubble sort
    h.run();
    h.state_mut().emulate();
    let ws = &mut h.state_mut().windows;
    (ws.memory, ws.stack, ws.flags, ws.vars) = (true, true, true, true);
    for _ in 0..12 {
        h.state_mut().single_step();
    }
    h.run();
    assert!(h.state().session.machine.cpu.instr_count >= 12);
    save(&mut h, "04_debug_windows");
}

#[test]
fn graphics_and_devices() {
    let mut h = harness(app_with_example(8)); // graphics
    h.run();
    h.state_mut().emulate();
    let ev = h.state_mut().session.machine.run(5_000_000);
    assert_eq!(ev, rustemu_core::Event::WaitingInput);
    h.run();
    save(&mut h, "05_graphics");

    let robot = crate::examples::EXAMPLES.iter().position(|e| e.0 == "robot.asm").unwrap();
    let mut h = harness(app_with_example(robot));
    h.run();
    h.state_mut().emulate();
    h.state_mut().session.machine.run(3_000_000);
    h.state_mut().windows.devices = true;
    h.run();
    save(&mut h, "06_devices");
}

#[test]
fn stepping_into_library_macros() {
    let calc = crate::examples::EXAMPLES.iter().position(|e| e.0 == "calc.asm").unwrap();
    let mut h = harness(app_with_example(calc));
    h.run();
    h.state_mut().emulate();
    // First instruction belongs to the PRINT macro: the source window shows emu8086.inc.
    h.run();
    let loc = h.state().session.current_location().unwrap();
    assert!(loc.origin.is_some());
    save(&mut h, "07_inside_print_macro");
    // Run until SCAN_NUM waits for a key: we are inside the library procedure.
    let ev = h.state_mut().session.machine.run(10_000);
    assert_eq!(ev, rustemu_core::Event::WaitingInput);
    h.state_mut().session.machine.push_char(b'7');
    for _ in 0..6 {
        h.state_mut().single_step();
    }
    h.run_steps(20);
    let loc = h.state().session.current_location().unwrap();
    let (of, _) = loc.origin.unwrap();
    assert_eq!(h.state().source_files[of].0, "emu8086.inc");
    save(&mut h, "08_inside_scan_num");

    // Step out of SCAN_NUM back to the program (it still needs Enter).
    h.state_mut().step_out();
    h.state_mut().session.machine.push_char(b'\r');
    for _ in 0..10 {
        let ctx = h.ctx.clone();
        h.state_mut().tick(&ctx);
        h.step();
    }
    let v = h.state().session.current_view(true).unwrap();
    assert_eq!((v.file, v.level), (0, 0));
    assert!(h.state().run == crate::app::RunMode::Paused);
    h.run_steps(20);
    save(&mut h, "10_after_step_out");

    h.state_mut().library.show_entry("scan_num");
    h.run_steps(20);
    save(&mut h, "09_library");
}

#[test]
fn new_dialog_menus_and_tools() {
    let mut h = harness(app_with_example(0));
    h.run();
    h.get_by_label("🗋 new").click();
    h.run_steps(5);
    assert_eq!(h.state().new_dialog, Some(0));
    save(&mut h, "11_new_dialog");
    // Choose EXE template and confirm.
    h.get_by_label("EXE template - advanced executable file. header: relocation, checksum").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(5);
    assert!(h.state().new_dialog.is_none());
    assert!(h.state().text.contains("data segment"));

    let st = h.state_mut();
    st.calculator.open = true;
    st.calculator.expr = "0FFh + 'A' * 2".into();
    st.options_open = true;
    st.cursor_line = 3;
    crate::editcmd::toggle_bookmark(st);
    h.run_steps(5);
    save(&mut h, "12_calculator_options");
}
