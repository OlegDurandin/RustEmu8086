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

#[test]
fn reference_window_and_editor_lookup() {
    use crate::reference::Topic;
    let mut h = harness(app_with_example(0));
    h.run();
    // F1 with the text cursor on `int 21h` opens the DOS function chosen by the preceding MOV AH.
    let st = h.state_mut();
    st.text = "org 100h\nmov ah, 9\nlea dx, msg\nint 21h\nret\nmsg db 'hi$'".into();
    let ci = st.text.find("21h").unwrap();
    st.selection = (ci, ci);
    st.help_at_cursor();
    h.run_steps(5);
    let int21 = crate::reference::find_interrupt(0x21).unwrap();
    assert_eq!(h.state().reference.topic, Some(Topic::Int(int21, Some(9))));
    save(&mut h, "13_reference_interrupt");
    let mov = crate::reference::find_instruction("adc").unwrap();
    h.state_mut().reference.show(Topic::Instr(mov));
    h.run_steps(5);
    save(&mut h, "14_reference_instruction");
}

/// Runs the emulation loop until execution pauses.
fn run_until_paused(app: &mut App) {
    let ctx = egui::Context::default();
    for _ in 0..500 {
        if app.run == crate::app::RunMode::Paused {
            return;
        }
        app.tick(&ctx);
    }
    panic!("still running: {}", app.status);
}

const LOOP_SRC: &str = "org 100h
    mov cx, 10
next:
    inc ax
    loop next
    mov bx, 1
    ret
";

#[test]
fn conditional_breakpoint_run_to_cursor_and_stop_condition() {
    use rustemu_core::cpu::{AX, BX, CX};
    let mut app = App::with_settings(crate::app::Settings { last_file: None, ..Default::default() });
    app.text = LOOP_SRC.into();
    app.emulate();
    assert!(app.loaded());
    // Breakpoint on `inc ax` that only stops when AX = 5.
    app.toggle_breakpoint_at_line(0, 4);
    app.bp_lines.get_mut(&(0, 4)).unwrap().condition = "ax == 5".into();
    app.start_run();
    run_until_paused(&mut app);
    assert_eq!(app.session.machine.cpu.regs[AX], 5, "{}", app.status);
    assert_eq!(app.bp_lines[&(0, 4)].hits, 1);
    // Run to line 6: the condition is not true again on the way.
    app.run_to_line(0, 6);
    run_until_paused(&mut app);
    assert_eq!(app.session.current_line(), Some((0, 6)));
    assert_eq!(app.session.machine.cpu.regs[CX], 0);
    // ALU window data: the last operation was the final INC AX.
    assert_eq!(app.session.machine.cpu.alu_trace.op, rustemu_core::AluOp::Inc);

    // Stop on condition: stops right after CX becomes 7.
    app.reload();
    app.bp_lines.clear();
    app.sync_breakpoints();
    app.stop_condition.text = "cx == 7".into();
    app.stop_condition.enabled = true;
    app.start_run();
    run_until_paused(&mut app);
    assert_eq!(app.session.machine.cpu.regs[CX], 7, "{}", app.status);
    assert!(app.status.contains("is true"));
    // "changes" mode on BX.
    app.stop_condition.text = "bx".into();
    app.stop_condition.compiled = None;
    app.stop_condition.on_change = true;
    app.start_run();
    run_until_paused(&mut app);
    assert_eq!(app.session.machine.cpu.regs[BX], 1, "{}", app.status);

    // Editing the source makes the loaded program outdated.
    assert!(!app.is_stale());
    app.text.push_str("; changed\n");
    assert!(app.is_stale());
    app.run_to_line(0, 6);
    assert_eq!(app.run, crate::app::RunMode::Paused);
    assert!(app.status.contains("reassemble"));
}

#[test]
fn breakpoints_follow_edited_lines() {
    let map = crate::editor::line_mapper("a\nb\nc\nd", "a\nnew\nb\nc\nd");
    assert_eq!((map(1), map(2), map(4)), (Some(1), Some(3), Some(5)));
    // Typing inside a line keeps it; deleting a line drops its marks.
    let map = crate::editor::line_mapper("a\nb\nc", "a\nbx\nc");
    assert_eq!((map(2), map(3)), (Some(2), Some(3)));
    let map = crate::editor::line_mapper("a\nb\nc", "a\nc");
    assert_eq!((map(1), map(2), map(3)), (Some(1), None, Some(2)));
}

#[test]
fn debugger_windows_snapshot() {
    let src = "org 100h
    mov ax, 7
    call square
    ret
square proc
    push bx
    mov bx, ax
    mul bx
    pop bx
    ret
square endp
value dw 1234h
";
    let mut h = harness(App::with_settings(crate::app::Settings { last_file: None, ..Default::default() }));
    h.run();
    let app = h.state_mut();
    app.text = src.into();
    app.emulate();
    app.toggle_breakpoint_at_line(0, 9);
    app.toggle_breakpoint_at_line(0, 2);
    app.bp_lines.get_mut(&(0, 2)).unwrap().condition = "ax != 0".into();
    app.start_run();
    run_until_paused(app);
    assert_eq!(app.session.current_line(), Some((0, 9)));
    crate::debugger::add_watch(app, "ax");
    crate::debugger::add_watch(app, "value");
    crate::debugger::add_watch(app, "word ss:[sp]");
    let w = &mut app.windows;
    (w.breakpoints, w.call_stack, w.alu, w.source, w.emulator, w.screen) = (true, true, true, false, false, false);
    h.run_steps(8);
    save(&mut h, "15_debugger");
}

#[test]
fn emu8086_style_emulator_windows() {
    let mut h = harness(app_with_example(0));
    h.run();
    let app = h.state_mut();
    app.emulate();
    app.single_step();
    let w = &mut app.windows;
    (w.stack, w.flags, w.flag_analyser, w.console, w.source, w.screen) = (true, true, true, true, false, false);
    h.run_steps(8);
    save(&mut h, "16_emulator_emu8086_style");
    // The debug log shows the state after the step, DEBUG.EXE style.
    let lines = &h.state().console.lines;
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("AX=") && l.contains("BX=") && l.find("BX").unwrap() < l.find("CX").unwrap())
    );
}

#[test]
fn emulator_bottom_buttons_and_aux_menu() {
    let mut h = harness(app_with_example(0));
    h.run();
    let app = h.state_mut();
    app.emulate();
    app.windows.screen = false;
    app.windows.source = false;
    h.run_steps(4);
    h.get_by_label("aux").click();
    h.run_steps(4);
    save(&mut h, "17_emulator_aux_menu");
    h.get_by_label("reset").click();
    h.run_steps(2);
    assert_eq!(h.state().session.machine.cpu.ip, 0x100);
}

#[test]
fn session_is_restored_after_restart() {
    use crate::app::Settings;
    let dir = std::env::temp_dir().join(format!("rustemu_session_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.asm"), dir.join("b.asm"));
    std::fs::write(&a, "include 'emu8086.inc'\norg 100h\n    mov cx, 3\nnext:\n    putc 'x'\n    loop next\n    ret\n")
        .unwrap();
    std::fs::write(&b, LOOP_SRC).unwrap();

    let mut app = App::with_settings(Settings { last_file: None, ..Default::default() });
    app.open_path(&a);
    app.emulate();
    app.toggle_breakpoint_at_line(0, 3);
    app.bp_lines.get_mut(&(0, 3)).unwrap().condition = "cx == 3".into();
    app.toggle_breakpoint_at_line(0, 6);
    app.bp_lines.get_mut(&(0, 6)).unwrap().enabled = false;
    // A breakpoint inside emu8086.inc (file 1): the first line with code of the PUTC macro.
    let inc = app.source_files.iter().position(|f| f.0 == "emu8086.inc").unwrap();
    let putc = app.source_index[inc].iter().find(|e| e.name == "PUTC").unwrap().line;
    app.toggle_breakpoint_at_line(inc, putc);
    let inc_bp = *app.bp_lines.keys().find(|k| k.0 == inc).unwrap();
    crate::debugger::add_watch(&mut app, "cx");
    app.bookmarks.insert(5);
    app.stop_condition.text = "cx == 1".into();
    app.windows.watch = true;
    app.windows.alu = true;
    app.screen.font_size = 17.0;
    app.cursor_line = 4;

    // Another file has its own (empty) session.
    app.open_path(&b);
    assert!(app.bp_lines.is_empty() && app.watches.is_empty() && app.bookmarks.is_empty());
    app.toggle_breakpoint_at_line(0, 4);
    app.store_settings();
    let saved = app.settings.clone();

    // "Restart": a new app from the saved settings reopens b.asm with its breakpoint.
    let mut app = App::with_settings(saved);
    assert_eq!(app.path.as_deref(), Some(b.as_path()));
    assert!(app.bp_lines.contains_key(&(0, 4)) && app.bp_lines.len() == 1);
    assert!(app.windows.watch && app.windows.alu);
    assert_eq!(app.screen.font_size, 17.0);
    // Back to a.asm: everything is there again.
    app.open_path(&a);
    assert_eq!(app.bp_lines[&(0, 3)].condition, "cx == 3");
    assert!(!app.bp_lines[&(0, 6)].enabled);
    assert_eq!(app.watches.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["cx"]);
    assert!(app.bookmarks.contains(&5));
    assert_eq!(app.stop_condition.text, "cx == 1");
    assert_eq!(app.goto_line, Some(4));
    // The emu8086.inc breakpoint returns when the program is loaded.
    assert_eq!(app.pending_breakpoints.len(), 1);
    app.emulate();
    assert!(app.pending_breakpoints.is_empty());
    assert!(app.bp_lines.contains_key(&inc_bp));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn screen_paste_and_copy() {
    let mut app = App::with_settings(crate::app::Settings { last_file: None, ..Default::default() });
    app.load_example(0);
    app.emulate();
    let m = &mut app.session.machine;
    let before = m.sys.keys.len();
    crate::screen::paste_keys(m, "12\r\n3\tx");
    let keys: Vec<u16> = m.sys.keys.iter().skip(before).copied().collect();
    assert_eq!(keys.len(), 6);
    assert_eq!(keys[2], 0x1C0D, "new line is Enter");
    assert_eq!(keys[4], 0x0F09, "tab is Tab");
    assert_eq!(keys[0] & 0xFF, b'1' as u16);
    // Copy: the text the program printed.
    app.start_run();
    run_until_paused(&mut app);
    let text = crate::screen::screen_text(&app.session.machine.sys.mem);
    assert!(text.contains("Hello"), "{text}");
}
