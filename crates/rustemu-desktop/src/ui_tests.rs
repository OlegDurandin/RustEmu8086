//! Headless UI tests: drive the app with egui_kittest and render snapshots to
//! `target/ui-snapshots/*.png` for visual inspection.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use rustemu_ui::app::{App, PendingAction, Settings};
use rustemu_ui::testing::{app_with_example, save};

fn harness(app: App) -> Harness<'static, App> {
    let h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_max_steps(64)
        .build_ui_state(|ui, app: &mut App| crate::ui::main_ui(app, ui), app);
    rustemu_ui::fonts::install(&h.ctx);
    h.state().apply_theme(&h.ctx);
    h
}

/// eframe storage in memory; counts the values written.
#[derive(Default)]
struct MemStorage {
    values: std::collections::HashMap<String, String>,
    writes: usize,
}

impl eframe::Storage for MemStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.writes += 1;
        self.values.insert(key.to_string(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.values.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn run_does_not_start_after_assembly_failure() {
    let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
    app.text = "org 100h\nnot_an_instruction\n".into();

    app.start_run();

    assert_eq!(app.run, rustemu_ui::app::RunMode::Paused);
    assert!(!app.loaded());
    assert_eq!(app.status, "Assembly failed");

    // A failed rebuild keeps the previous machine available for inspection,
    // but F5 must not start that stale image.
    app.load_example(0);
    app.emulate();
    assert!(app.loaded());
    let cpu = app.session.machine.cpu.clone();
    app.text = "org 100h\nnot_an_instruction\n".into();
    app.emulate();
    assert_eq!(app.status, "Assembly failed");

    app.start_run();

    assert_eq!(app.run, rustemu_ui::app::RunMode::Paused);
    assert!(app.loaded());
    assert_eq!(app.status, "Assembly failed");
    assert_eq!(app.session.machine.cpu, cpu);

    // A successful Compile writes files but does not load the resulting image.
    // F5 must therefore still rebuild/load it instead of running the old image.
    let dir = std::env::temp_dir().join(format!("rustemu_f5_after_error_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    app.path = Some(dir.join("fixed.asm"));
    app.text = "org 100h\nmov ax, 1234h\nhlt\n".into();
    let expected = rustemu_asm::assemble_str(&app.text).unwrap().image;
    app.compile();
    assert!(app.status.starts_with("Compiled to"));

    app.start_run();

    assert_eq!(app.run, rustemu_ui::app::RunMode::Running);
    assert_eq!(app.session.program.as_ref().unwrap().image, expected);
    std::fs::remove_dir_all(dir).unwrap();
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
    // The parts of the lines: the unknown mnemonic and the value that does not fit.
    assert_eq!(h.state().error_spans, vec![(3, 0, 3), (4, 8, 3)]);
    assert!(h.state().messages[1].text.contains("(4:9): value 300"), "{}", h.state().messages[1].text);
    save(&mut h, "03_errors");
    // A click on the message selects that part of the line.
    h.get_by_label_contains("(4:9)").click();
    h.run();
    let start = "org 100h\nmov ax, bx\nfoo 1\n".len();
    assert_eq!(h.state().selection, (start + 8, start + 11));
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

    let robot = rustemu_ui::examples::EXAMPLES.iter().position(|e| e.0 == "robot.asm").unwrap();
    let mut h = harness(app_with_example(robot));
    h.run();
    h.state_mut().emulate();
    h.state_mut().session.machine.run(3_000_000);
    h.state_mut().windows.devices = true;
    h.run();
    save(&mut h, "06_devices");
}

#[test]
fn editing_a_device_clears_step_back_history() {
    let mut h = harness(app_with_example(0));
    h.state_mut().emulate();
    h.state_mut().single_step();
    assert!(h.state().session.machine.history_len() > 0);
    h.state_mut().session.machine.sys.dev.printer.extend_from_slice(b"AB");
    h.state_mut().device_tab = rustemu_ui::devices_ui::Device::Printer;
    h.state_mut().windows.devices = true;
    h.run();
    h.get_by_label("clear paper").click();
    h.run();

    assert!(h.state().session.machine.sys.dev.printer.is_empty());
    assert_eq!(h.state().session.machine.history_len(), 0);
}

#[test]
fn stepping_into_library_macros() {
    let calc = rustemu_ui::examples::EXAMPLES.iter().position(|e| e.0 == "calc.asm").unwrap();
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
    assert!(h.state().run == rustemu_ui::app::RunMode::Paused);
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
    rustemu_ui::editcmd::toggle_bookmark(st);
    h.run_steps(5);
    save(&mut h, "12_calculator_options");
}

#[test]
fn reference_window_and_editor_lookup() {
    use rustemu_ui::reference::Topic;
    let mut h = harness(app_with_example(0));
    h.run();
    // F1 with the text cursor on `int 21h` opens the DOS function chosen by the preceding MOV AH.
    let st = h.state_mut();
    st.text = "org 100h\nmov ah, 9\nlea dx, msg\nint 21h\nret\nmsg db 'hi$'".into();
    let ci = st.text.find("21h").unwrap();
    st.selection = (ci, ci);
    st.help_at_cursor();
    h.run_steps(5);
    let int21 = rustemu_ui::reference::find_interrupt(0x21).unwrap();
    assert_eq!(h.state().reference.topic, Some(Topic::Int(int21, Some(9))));
    save(&mut h, "13_reference_interrupt");
    let mov = rustemu_ui::reference::find_instruction("adc").unwrap();
    h.state_mut().reference.show(Topic::Instr(mov));
    h.run_steps(5);
    save(&mut h, "14_reference_instruction");
    // The NASM tab opens at the overview; its sections and the NASM directives are listed.
    h.state_mut().reference.show_tab(rustemu_ui::reference_ui::Tab::Nasm);
    h.run_steps(5);
    assert!(h.query_by_label("Hello from NASM!").is_none(), "the example is shown highlighted, not as a label");
    h.get_by_label("Names, memory and sizes").click();
    h.run_steps(5);
    assert!(matches!(h.state().reference.topic, Some(Topic::Nasm(_))));
    assert!(h.query_by_label("loads the address of value").is_some());
    assert!(h.query_by_label("%MACRO").is_some(), "NASM directives are listed");
    save(&mut h, "15_reference_nasm");
}

#[test]
fn about_window() {
    let mut app = app_with_example(0);
    app.windows.about = true;
    let mut h = harness(app);
    h.run_steps(4);
    assert!(h.query_by_label("source code (GitHub)").is_some());
    assert!(h.query_by_label_contains("NASM").is_some());
    save(&mut h, "16_about");
}

/// Runs the emulation loop until execution pauses.
fn run_until_paused(app: &mut App) {
    let ctx = egui::Context::default();
    for _ in 0..500 {
        if app.run == rustemu_ui::app::RunMode::Paused {
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
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
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
    assert_eq!(app.run, rustemu_ui::app::RunMode::Paused);
    assert!(app.status.contains("reassemble"));
}

#[test]
fn breakpoints_follow_edited_lines() {
    let map = rustemu_ui::editor::line_mapper("a\nb\nc\nd", "a\nnew\nb\nc\nd");
    assert_eq!((map(1), map(2), map(4)), (Some(1), Some(3), Some(5)));
    // Typing inside a line keeps it; deleting a line drops its marks.
    let map = rustemu_ui::editor::line_mapper("a\nb\nc", "a\nbx\nc");
    assert_eq!((map(2), map(3)), (Some(2), Some(3)));
    let map = rustemu_ui::editor::line_mapper("a\nb\nc", "a\nc");
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
    let mut h = harness(App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() }));
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
    rustemu_ui::debugger::add_watch(app, "ax");
    rustemu_ui::debugger::add_watch(app, "value");
    rustemu_ui::debugger::add_watch(app, "word ss:[sp]");
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
    use rustemu_ui::app::Settings;
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
    rustemu_ui::debugger::add_watch(&mut app, "cx");
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
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.load_example(0);
    app.emulate();
    let m = &mut app.session.machine;
    let before = m.sys.keys.len();
    rustemu_ui::screen::paste_keys(m, "12\r\n3\tx");
    let keys: Vec<u16> = m.sys.keys.iter().skip(before).copied().collect();
    assert_eq!(keys.len(), 6);
    assert_eq!(keys[2], 0x1C0D, "new line is Enter");
    assert_eq!(keys[4], 0x0F09, "tab is Tab");
    assert_eq!(keys[0] & 0xFF, b'1' as u16);
    // Copy: the text the program printed.
    app.start_run();
    run_until_paused(&mut app);
    let text = rustemu_ui::screen::screen_text(&app.session.machine.sys.mem);
    assert!(text.contains("Hello"), "{text}");
}

#[test]
fn no_execution_command_runs_the_old_image_after_a_failed_build() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.text = LOOP_SRC.into();
    app.emulate();
    let cpu = app.session.machine.cpu.clone();
    app.text = "org 100h\nnot_an_instruction\n".into();
    app.emulate();
    assert_eq!(app.status, "Assembly failed");
    app.single_step();
    app.step_over();
    app.step_out();
    app.run_to_line(0, 2);
    assert_eq!(app.run, rustemu_ui::app::RunMode::Paused);
    assert_eq!(app.session.machine.cpu, cpu, "the old image must not execute");
    // Fixing the code makes stepping work again (on the new build).
    app.text = LOOP_SRC.into();
    app.single_step();
    assert_eq!(app.session.machine.cpu.instr_count, 1);
}

#[test]
fn address_breakpoints_belong_to_one_program() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.text = LOOP_SRC.into();
    app.emulate();
    app.toggle_breakpoint_at_line(0, 4);
    let line_bp = *app.bp_addr.keys().next().unwrap();
    let addr_bp = line_bp + 1; // as if set by double-clicking in the disassembly
    app.session.machine.breakpoints.insert(addr_bp);
    app.reload();
    assert!(app.session.machine.breakpoints.contains(&addr_bp), "kept by Reload");
    app.emulate();
    assert!(app.session.machine.breakpoints.contains(&addr_bp), "kept when the same code is reassembled");
    app.text = format!("{LOOP_SRC}\n    nop\n");
    app.text = app.text.replacen("mov cx, 10", "mov cx, 11", 1);
    app.emulate();
    assert!(!app.session.machine.breakpoints.contains(&addr_bp), "dropped for a different program");
    assert!(app.session.machine.breakpoints.contains(&line_bp), "line breakpoints stay");
}

#[test]
fn emulator_window_debug_and_file_menus() {
    use rustemu_core::cpu::{BX, CX};
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.text = LOOP_SRC.into();
    app.emulate();
    // Select `mov bx, 1` (line 6) as if clicked in the disassembly.
    let a6 = app.session.addr_of_line(0, 6).unwrap();
    let sel = (0x0700, (a6 - rustemu_core::linear(0x0700, 0)) as u16);
    app.code_selected = Some(sel);
    app.run_until_selected();
    run_until_paused(&mut app);
    assert_eq!(app.session.machine.cpu.pc(), a6);
    assert_eq!(app.session.machine.cpu.regs[CX], 0);

    // set break point (selected) / show current break point / clear break point.
    let a4 = app.session.addr_of_line(0, 4).unwrap();
    app.code_selected = Some((0x0700, (a4 - 0x7000) as u16));
    app.toggle_breakpoint_at_addr(a4);
    assert!(app.bp_lines.contains_key(&(0, 4)), "maps to a line breakpoint");
    app.show_next_breakpoint();
    assert!(app.code_view_pinned && app.code_view_start == a4);
    app.show_current_instruction();
    assert!(!app.code_view_pinned);
    app.toggle_breakpoint_at_addr(a4);
    assert!(app.bp_lines.is_empty());

    // set CS:IP to selected position: back to `inc ax`.
    app.set_cs_ip_to_selected();
    assert_eq!(app.session.machine.cpu.pc(), a4);

    // save the emulator's state / load from previous state.
    app.save_emulator_state();
    app.single_step();
    assert_ne!(app.session.machine.cpu.pc(), a4);
    app.load_emulator_state();
    assert_eq!(app.session.machine.cpu.pc(), a4);
    assert_eq!(app.session.current_line(), Some((0, 4)));

    // reset emulator and ram.
    app.reset_emulator_and_ram();
    assert!(!app.loaded());
    assert_eq!(app.session.machine.cpu.regs[BX], 0);
    app.load_emulator_state();
    assert!(app.loaded(), "a saved state brings the program back");
    app.close_emulator();
    assert!(!app.windows.emulator);
}

#[test]
fn emulator_menu_snapshots() {
    let mut h = harness(app_with_example(0));
    h.run();
    let app = h.state_mut();
    app.emulate();
    (app.windows.screen, app.windows.source) = (false, false);
    h.run_steps(4);
    // "debug" is also a button at the bottom of the window; the menu comes first.
    h.get_all_by_label("debug").next().unwrap().click();
    h.run_steps(4);
    save(&mut h, "18_emulator_debug_menu");
    // The first click only closes the open menu.
    h.get_by_label("file").click();
    h.run_steps(4);
    h.get_by_label("file").click();
    h.run_steps(4);
    save(&mut h, "19_emulator_file_menu");
}

#[test]
fn value_viewer_addresses_and_view_menu_snapshots() {
    let mut cpu = rustemu_core::Cpu::new();
    cpu.sregs[rustemu_core::cpu::DS] = 0x0700;
    cpu.regs[rustemu_core::cpu::SI] = 0x0010;
    use rustemu_ui::debugger::parse_view_addr;
    assert_eq!(parse_view_addr(&cpu, "DS:0100"), Some(0x7100));
    assert_eq!(parse_view_addr(&cpu, "ds:si"), Some(0x7010));
    assert_eq!(parse_view_addr(&cpu, "07105h"), Some(0x7105));
    assert_eq!(parse_view_addr(&cpu, "zz:1"), None);

    let mut h = harness(app_with_example(0));
    h.run();
    let app = h.state_mut();
    app.emulate();
    (app.windows.screen, app.windows.source) = (false, false);
    h.run_steps(4);
    h.get_by_label("view").click();
    h.run_steps(4);
    save(&mut h, "20_emulator_view_menu");
    let app = h.state_mut();
    rustemu_ui::debugger::view_register(app, "SP");
    assert!(app.windows.value_viewer && app.value_viewer.reg == Some(7));
    app.device_tab = rustemu_ui::devices_ui::Device::VgaState;
    app.windows.devices = true;
    h.key_press(egui::Key::Escape);
    h.run_steps(6);
    save(&mut h, "21_value_viewer_vga_state");
}

#[test]
fn value_viewer_sets_registers_and_memory() {
    use rustemu_ui::debugger::{set_viewed_value, view_register};
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.load_example(0);
    app.emulate();
    view_register(&mut app, "BX");
    set_viewed_value(&mut app, "1010b").unwrap();
    assert_eq!(app.session.machine.cpu.regs[rustemu_core::cpu::BX], 10);
    set_viewed_value(&mut app, "-2").unwrap();
    assert_eq!(app.session.machine.cpu.regs[rustemu_core::cpu::BX], 0xFFFE);
    view_register(&mut app, "FLAGS");
    set_viewed_value(&mut app, "1").unwrap();
    assert!(app.session.machine.cpu.flags & rustemu_core::cpu::flags::CF != 0);
    app.value_viewer.reg = None;
    app.value_viewer.addr = "DS:0200".into();
    set_viewed_value(&mut app, "'AB'").unwrap();
    assert_eq!(app.session.machine.sys.mem.read16(0x7200), 0x4241);
    assert!(set_viewed_value(&mut app, "70000").unwrap_err().contains("does not fit"));
}

#[test]
fn nasm_templates_and_syntax_setting() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    for i in [0, 2, 3] {
        app.new_from_nasm_template(i);
        app.emulate();
        assert!(app.loaded(), "template {i}: {:?}", app.messages.iter().map(|m| &m.text).collect::<Vec<_>>());
        assert!(app.messages[0].text.contains("NASM syntax"), "{}", app.messages[0].text);
    }
    // Forcing the Emu8086 syntax on NASM code fails; forcing NASM works.
    app.text = "mov ax, [bx]\nret\n".into();
    app.settings.syntax = rustemu_ui::app::SyntaxChoice::Nasm;
    app.emulate();
    assert!(app.messages[0].text.contains("NASM syntax"));
    app.text = "org 100h\nmov word [count], 1\nret\ncount dw 0\n".into();
    app.settings.syntax = rustemu_ui::app::SyntaxChoice::Masm;
    app.emulate();
    assert!(app.messages.iter().any(|m| m.error));
}

#[test]
fn nasm_editor_snapshot() {
    let i = rustemu_ui::examples::EXAMPLES.iter().position(|e| e.0 == "hello_nasm.asm").unwrap();
    let mut h = harness(app_with_example(i));
    h.run();
    let app = h.state_mut();
    app.emulate();
    (app.windows.emulator, app.windows.screen, app.windows.source) = (false, false, false);
    h.run_steps(4);
    save(&mut h, "22_nasm_editor");
    assert!(h.state().messages[0].text.contains("NASM syntax, COM format"));
}

#[test]
fn memory_drive_window_and_storage() {
    // The browser's drive C:: a program writes a file next to an uploaded one, the "drive C:"
    // window lists both and previews one, and the drive survives a restart through storage.
    let mut app = app_with_example(0);
    app.session.machine.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    app.add_drive_file("data/in.txt", b"first line\r\nsecond line\r\n".to_vec());
    app.text = "org 100h
    mov ah, 3Ch
    xor cx, cx
    mov dx, offset fname
    int 21h
    mov bx, ax
    mov ah, 40h
    mov cx, 4
    mov dx, offset text
    int 21h
    mov ah, 3Eh
    int 21h
    ret
fname db 'out.txt', 0
text  db 'done'
"
    .into();
    app.emulate();
    app.start_run();
    run_until_paused(&mut app);
    let drive = app.session.machine.dos_memory_drive().unwrap().clone();
    assert_eq!(drive.read("out.txt"), Some(&b"done"[..]), "{}", app.status);

    let mut storage = MemStorage::default();
    app.persist(&mut storage);
    let mut restarted = app_with_example(0);
    restarted.session.machine.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    restarted.restore_drive(&storage);
    assert_eq!(restarted.session.machine.dos_memory_drive(), Some(&drive));

    app.windows = Default::default();
    app.windows.drive = true;
    app.drive_preview = Some("DATA/IN.TXT".into());
    let mut h = harness(app);
    h.run_steps(4);
    assert!(h.query_by_label("C:\\OUT.TXT").is_some());
    save(&mut h, "23_drive_window");
    h.get_by_label("C:\\OUT.TXT").click();
    h.run_steps(2);
    assert_eq!(h.state().drive_preview.as_deref(), Some("OUT.TXT"));
}

#[test]
fn programs_from_links_get_a_temporary_drive() {
    // A program from a link runs with an empty drive C: of its own: it doesn't see the user's
    // files and what it writes is not kept; saved, it runs with the user's drive.
    let mut app = app_with_example(0);
    app.session.machine.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    app.add_drive_file("secret.txt", b"mine".to_vec());
    let text = "org 100h
    mov ax, 3D00h
    mov dx, offset secret
    int 21h
    jc write
    mov dx, offset seen
    jmp create
write:
    mov dx, offset out
create:
    mov ah, 3Ch
    xor cx, cx
    int 21h
    ret
secret db 'secret.txt', 0
seen   db 'seen.txt', 0
out    db 'out.txt', 0
";
    app.open_link(rustemu_ui::share::Shared { name: "link.asm".into(), text: text.into(), run: true });
    run_until_paused(&mut app);
    let temporary = app.session.machine.dos_memory_drive().unwrap();
    assert_eq!(temporary.files().map(|f| f.0).collect::<Vec<_>>(), ["OUT.TXT"], "{}", app.status);
    assert_eq!(app.notice.as_ref().map(|n| n.0.as_str()), Some("Program from a link"));
    // The user's drive is the one kept, untouched.
    let mut storage = MemStorage::default();
    app.persist(&mut storage);
    let mut restarted = app_with_example(0);
    restarted.session.machine.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    restarted.restore_drive(&storage);
    let own = restarted.session.machine.dos_memory_drive().unwrap();
    assert_eq!(own.files().map(|f| f.0).collect::<Vec<_>>(), ["SECRET.TXT"]);

    app.notice = None;
    app.windows = Default::default();
    app.windows.drive = true;
    let mut h = harness(app);
    h.run_steps(4);
    save(&mut h, "24_drive_from_link");
    let mut app = h.into_state();

    // Saved, it is the user's program.
    app.save_program("link.asm");
    app.emulate();
    app.start_run();
    run_until_paused(&mut app);
    let own = app.session.machine.dos_memory_drive().unwrap();
    assert_eq!(own.files().map(|f| f.0).collect::<Vec<_>>(), ["SECRET.TXT", "SEEN.TXT"]);
    assert!(app.notice.is_none());
}

/// Every CP437 character, grey and then in the colours of its own code.
const CP437_SRC: &str = "org 100h
    mov ax, 0B800h
    mov es, ax
    xor di, di
    xor al, al
    mov ah, 07h
grey:
    mov cx, 32
grey_cells:
    stosw
    inc al
    loop grey_cells
    add di, 160 - 64
    or al, al
    jnz grey
    add di, 160
coloured:
    mov cx, 32
coloured_cells:
    mov ah, al
    stosw
    inc al
    loop coloured_cells
    add di, 160 - 64
    or al, al
    jnz coloured
    ret
";

#[test]
fn screen_draws_every_cp437_glyph() {
    let mut app = App::with_settings(Settings { last_file: None, ..Default::default() });
    app.text = CP437_SRC.into();
    app.windows = Default::default();
    let mut h = harness(app);
    let app = h.state_mut();
    app.emulate();
    app.windows.emulator = false;
    app.windows.screen = true;
    app.start_run();
    run_until_paused(app);
    let mem = &app.session.machine.sys.mem;
    assert_eq!(rustemu_core::video::Video::read_cell(mem, 0, 2, 0), (2, 7), "☻ in row 0");
    h.run_steps(3);
    save(&mut h, "24_screen_cp437");
}

/// An app with files in browser storage (the web version).
fn browser_app() -> App {
    let mut app = app_with_example(0);
    app.browser = true;
    app
}

#[test]
fn programs_are_kept_in_the_browser() {
    // A new program: Save asks for its name, then it is in "my programs".
    let mut app = browser_app();
    app.new_from_template(0);
    app.text.push_str("; mine\n");
    assert!(!app.save());
    app.save_name.as_mut().unwrap().text = "first".into();
    let mut h = harness(app);
    h.run_steps(2);
    save(&mut h, "25_save_in_browser");
    h.get_by_label("Save").click();
    h.run_steps(2);
    let app = h.state_mut();
    assert_eq!(app.program.as_deref(), Some("first.asm"));
    assert!(!app.dirty() && app.save_name.is_none());

    // The open program and its unsaved changes come back on the next visit.
    app.text.push_str("nop\n");
    let mut storage = MemStorage::default();
    app.persist(&mut storage);
    let writes = storage.writes;
    app.persist(&mut storage);
    assert_eq!(storage.writes, writes, "unchanged values are not written again");
    let mut next = browser_app();
    next.restore_browser(&storage);
    assert_eq!(next.program.as_deref(), Some("first.asm"));
    assert_eq!(next.text, app.text);
    assert!(next.dirty());

    // Another program with unsaved changes: the unsaved changes dialog names it first, then opens.
    next.import_program("other.asm", "org 100h\nret\n".into());
    next.text.push_str("; edited\n");
    next.program = None;
    next.guarded(PendingAction::Program("first.asm".into()));
    let mut h = harness(next);
    h.run_steps(2);
    h.get_by_label("Save").click();
    h.run_steps(2);
    h.state_mut().save_name.as_mut().unwrap().text = "copy".into();
    h.run_steps(1);
    h.get_by_label("Save").click();
    h.run_steps(2);
    let app = h.state_mut();
    assert!(app.programs["copy.asm"].text.ends_with("; edited\n"));
    assert_eq!(app.program.as_deref(), Some("first.asm"), "then the chosen program opened");

    // An upload named like a different program is kept under a new name; the same text is not
    // duplicated.
    app.import_program("first.asm", "org 100h\nret\n".into());
    assert_eq!(app.program.as_deref(), Some("first (2).asm"));
    app.import_program("first.asm", app.programs["first.asm"].text.clone());
    assert_eq!(app.program.as_deref(), Some("first.asm"));
    assert_eq!(app.recent_programs()[0], "first.asm");

    // The "my programs" window.
    app.windows = Default::default();
    app.windows.programs = true;
    h.run_steps(3);
    assert!(h.query_by_label("copy.asm").is_some());
    save(&mut h, "26_my_programs");
    h.get_all_by_label("delete…").next().unwrap().click();
    h.run_steps(2);
    h.get_by_label("yes").click();
    h.run_steps(2);
    assert_eq!(h.state().programs.len(), 3);
}

#[test]
fn share_links_open_the_program() {
    let mut app = app_with_example(0);
    app.share_base = Some("https://example.org/RustEmu8086/".into());
    app.share = Some(rustemu_ui::share::ShareDialog { run: true });
    let link = app.share_link(true);
    assert!(link.starts_with("https://example.org/RustEmu8086/#name=hello.asm&code="), "{link}");
    let shared = rustemu_ui::share::parse(link.split_once('#').unwrap().1).unwrap();
    assert_eq!((shared.text.as_str(), shared.run), (app.text.as_str(), true));
    let mut h = harness(app);
    h.run_steps(2);
    save(&mut h, "27_share_link");
    h.get_by_label("copy").click();
    h.run_steps(2);
    assert!(h.state().share.is_none());

    // Opening the link: the program is loaded and runs; unsaved changes are asked about first.
    let mut other = app_with_example(5);
    other.text.push_str("; edited\n");
    other.open_link(shared.clone());
    assert!(other.confirm_discard.is_some() && other.text.ends_with("; edited\n"));
    let mut clean = app_with_example(5);
    clean.open_link(shared);
    assert_eq!(clean.file_name(), "hello.asm");
    assert!(!clean.dirty() && clean.loaded());
    run_until_paused(&mut clean);
    assert_eq!(String::from_utf8_lossy(&clean.session.machine.output), "Hello, World!\r\n");
}
