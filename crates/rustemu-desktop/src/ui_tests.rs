//! Headless UI tests: drive the app with egui_kittest and render snapshots to
//! `target/ui-snapshots/*.png` for visual inspection.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use rustemu_ui::app::{App, PendingAction, Settings};
use rustemu_ui::testing::{app_with_example, save};
use rustemu_ui::tools::Tool;

fn harness(app: App) -> Harness<'static, App> {
    let h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_max_steps(64)
        .build_ui_state(|ui, app: &mut App| crate::ui::main_ui(app, ui), app);
    rustemu_ui::fonts::install(&h.ctx);
    h.state().apply_theme(&h.ctx);
    h
}

/// eframe storage in memory; counts the values written. With a quota it keeps the old value of a
/// write that does not fit, as a browser's local storage does.
#[derive(Default)]
struct MemStorage {
    values: std::collections::HashMap<String, String>,
    writes: usize,
    quota: Option<usize>,
}

impl eframe::Storage for MemStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.writes += 1;
        if let Some(quota) = self.quota {
            let others: usize = self.values.iter().filter(|(k, _)| *k != key).map(|(_, v)| v.len()).sum();
            if others + value.len() > quota {
                return;
            }
        }
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
    app.doc.text = "org 100h\nnot_an_instruction\n".into();

    app.start_run();

    assert_eq!(app.run, rustemu_ui::app::RunMode::Paused);
    assert!(!app.loaded());
    assert_eq!(app.status, "Assembly failed");

    // A failed rebuild keeps the previous machine available for inspection,
    // but F5 must not start that stale image.
    app.load_example_text(rustemu_ui::testing::PROGRAMS[0].0, rustemu_ui::testing::PROGRAMS[0].1);
    app.emulate();
    assert!(app.loaded());
    let cpu = app.session.x86().unwrap().cpu.clone();
    app.doc.text = "org 100h\nnot_an_instruction\n".into();
    app.emulate();
    assert_eq!(app.status, "Assembly failed");

    app.start_run();

    assert_eq!(app.run, rustemu_ui::app::RunMode::Paused);
    assert!(app.loaded());
    assert_eq!(app.status, "Assembly failed");
    assert_eq!(app.session.x86().unwrap().cpu, cpu);

    // A successful Compile writes files but does not load the resulting image.
    // F5 must therefore still rebuild/load it instead of running the old image.
    let dir = std::env::temp_dir().join(format!("rustemu_f5_after_error_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    app.doc.path = Some(dir.join("fixed.asm"));
    app.doc.text = "org 100h\nmov ax, 1234h\nhlt\n".into();
    let expected = rustemu_asm::assemble_str(&app.doc.text).unwrap().image;
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
    // The string after RET is shown as data, with its name.
    h.get_by_label("DB 'Hello, World!', 0Dh, 0Ah, '$'");
    h.get_by_label("MSG:");
    // Run the program to completion.
    h.state_mut().start_run();
    for _ in 0..20 {
        let ctx = h.ctx.clone();
        h.state_mut().tick(&ctx);
        h.step();
    }
    h.run_steps(2);
    let out = String::from_utf8_lossy(h.state().session.target.output()).into_owned();
    assert_eq!(out, "Hello, World!\r\n");
    assert!(h.state().session.x86().unwrap().screen_text().starts_with("Hello, World!"));
    // CS:IP is in the PSP now: the disassembler says the program has ended instead.
    h.get_by_label("The program has terminated (exit code 0).");
    save(&mut h, "02_emulator_hello");
}

#[test]
fn compile_error_is_reported() {
    let mut app = app_with_example(0);
    app.doc.text = "org 100h\nmov ax, bx\nfoo 1\nmov al, 300\n".into();
    let mut h = harness(app);
    h.run();
    h.get_by_label("▶ emulate").click();
    h.run();
    assert_eq!(h.state().build.error_lines, vec![3, 4]);
    // The parts of the lines: the unknown mnemonic and the value that does not fit.
    assert_eq!(h.state().build.error_spans, vec![(3, 0, 3), (4, 8, 3)]);
    assert!(h.state().build.messages[1].text.contains("(4:9): value 300"), "{}", h.state().build.messages[1].text);
    save(&mut h, "03_errors");
    // A click on the message selects that part of the line.
    h.get_by_label_contains("(4:9)").click();
    h.run();
    let start = "org 100h\nmov ax, bx\nfoo 1\n".len();
    assert_eq!(h.state().editor.selection, (start + 8, start + 11));
}

#[test]
fn stepping_and_debug_windows() {
    let mut h = harness(app_with_example(5)); // bubble sort
    h.run();
    h.state_mut().emulate();
    let ws = &mut h.state_mut().windows;
    (ws[Tool::Memory], ws[Tool::Stack], ws[Tool::Flags], ws[Tool::Vars]) = (true, true, true, true);
    for _ in 0..12 {
        h.state_mut().single_step();
    }
    h.run();
    assert!(h.state().session.target.instr_count() >= 12);
    save(&mut h, "04_debug_windows");
}

#[test]
fn graphics_and_devices() {
    let mut h = harness(app_with_example(8)); // graphics
    h.run();
    h.state_mut().emulate();
    let ev = h.state_mut().session.target.run(5_000_000);
    assert_eq!(ev, rustemu_core::Event::WaitingInput);
    h.run();
    save(&mut h, "05_graphics");

    let robot = rustemu_ui::testing::example("robot.asm");
    let mut h = harness(app_with_example(robot));
    h.run();
    h.state_mut().emulate();
    h.state_mut().session.target.run(3_000_000);
    h.state_mut().windows[Tool::Devices] = true;
    h.run();
    save(&mut h, "06_devices");
}

#[test]
fn editing_a_device_clears_step_back_history() {
    let mut h = harness(app_with_example(0));
    h.state_mut().emulate();
    h.state_mut().single_step();
    assert!(h.state().session.target.history_len() > 0);
    h.state_mut().session.x86_mut().unwrap().sys.dev.printer.write(b"AB");
    h.state_mut().view.device_tab = rustemu_ui::devices_ui::Device::Printer;
    h.state_mut().windows[Tool::Devices] = true;
    h.run();
    h.get_by_label("clear paper").click();
    h.run();

    assert!(h.state().session.x86().unwrap().sys.dev.printer.is_empty());
    assert_eq!(h.state().session.target.history_len(), 0);
}

#[test]
fn stepping_into_library_macros() {
    let calc = rustemu_ui::testing::example("calc.asm");
    let mut h = harness(app_with_example(calc));
    h.run();
    h.state_mut().emulate();
    // First instruction belongs to the PRINT macro: the source window shows emu8086.inc.
    h.run();
    let loc = h.state().session.current_location().unwrap();
    assert!(loc.origin.is_some());
    save(&mut h, "07_inside_print_macro");
    // Run until SCAN_NUM waits for a key: we are inside the library procedure.
    let ev = h.state_mut().session.target.run(10_000);
    assert_eq!(ev, rustemu_core::Event::WaitingInput);
    h.state_mut().session.target.push_char(b'7');
    for _ in 0..6 {
        h.state_mut().single_step();
    }
    h.run_steps(20);
    let loc = h.state().session.current_location().unwrap();
    let (of, _) = loc.origin.unwrap();
    assert_eq!(h.state().program.files[of].0, "emu8086.inc");
    save(&mut h, "08_inside_scan_num");

    // Step out of SCAN_NUM back to the program (it still needs Enter).
    h.state_mut().step_out();
    h.state_mut().session.target.push_char(b'\r');
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
    assert_eq!(h.state().dialogs.new_file, Some(0));
    save(&mut h, "11_new_dialog");
    // Choose EXE template and confirm.
    h.get_by_label("EXE template - advanced executable file. header: relocation, checksum").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(5);
    assert!(h.state().dialogs.new_file.is_none());
    assert!(h.state().doc.text.contains("data segment"));

    let st = h.state_mut();
    st.windows[Tool::Calculator] = true;
    st.calculator.expr = "0FFh + 'A' * 2".into();
    st.dialogs.options = true;
    st.editor.cursor_line = 3;
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
    st.doc.text = "org 100h\nmov ah, 9\nlea dx, msg\nint 21h\nret\nmsg db 'hi$'".into();
    let ci = st.doc.text.find("21h").unwrap();
    st.editor.selection = (ci, ci);
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
fn mips_reference() {
    use rustemu_session::Arch;
    use rustemu_ui::reference::Topic;
    let mut h = harness(App::with_settings(Settings { last_file: None, ..Default::default() }));
    h.run();
    h.state_mut().switch_arch(Arch::Mips32);
    h.run();
    // F1 on `syscall` after `li $v0, 4` opens print string; on `la`, its article with the forms.
    let st = h.state_mut();
    // (The first `syscall` of hello.s is in a comment: no article there.)
    let ci = st.doc.text.find("syscall").unwrap();
    st.editor.selection = (ci, ci);
    st.help_at_cursor();
    assert_eq!(st.reference.topic, None);
    let ci = st.doc.text.find("msg\n").unwrap()
        + 4
        + st.doc.text[st.doc.text.find("msg\n").unwrap() + 4..].find("syscall").unwrap();
    st.editor.selection = (ci, ci);
    st.help_at_cursor();
    h.run_steps(5);
    let print_string = rustemu_ui::reference::find_syscall(4).unwrap();
    assert_eq!(h.state().reference.topic, Some(Topic::Syscall(print_string)));
    assert!(h.state().reference.mips);
    h.get_by_label("print string");
    save(&mut h, "49_mips_reference_syscall");
    let st = h.state_mut();
    let ci = st.doc.text.find("la ").unwrap();
    st.editor.selection = (ci, ci);
    st.help_at_cursor();
    h.run_steps(5);
    let la = rustemu_ui::reference::find_mips_instruction("la").unwrap();
    assert_eq!(h.state().reference.topic, Some(Topic::MipsInstr(la)));
    // The forms: what the pseudo-instruction becomes.
    h.get_by_label("la $t1,label");
    assert!(h.query_all_by_label_contains("→ lui $at").count() > 1);
    save(&mut h, "49_mips_reference_instruction");
    // In Russian; the registers tab.
    h.state_mut().reference.lang = rustemu_ui::reference::Lang::Ru;
    h.state_mut().reference.show_tab(rustemu_ui::reference_ui::Tab::Registers);
    h.run_steps(5);
    h.get_by_label("указатель стека: стек растёт вниз от 0x7FFFEFFC");
    save(&mut h, "49_mips_reference_registers");
    // Back to the 8086: its reference again.
    h.state_mut().reference.lang = rustemu_ui::reference::Lang::En;
    h.state_mut().switch_arch(Arch::I8086);
    h.run();
    assert!(!h.state().reference.mips);
}

#[test]
fn about_window() {
    let mut app = app_with_example(0);
    app.windows[Tool::About] = true;
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
    app.doc.text = LOOP_SRC.into();
    app.emulate();
    assert!(app.loaded());
    // Breakpoint on `inc ax` that only stops when AX = 5.
    app.toggle_breakpoint_at_line(0, 4);
    app.debugger.bp_lines.get_mut(&(0, 4)).unwrap().condition = "ax == 5".into();
    app.start_run();
    run_until_paused(&mut app);
    assert_eq!(app.session.x86().unwrap().cpu.regs[AX], 5, "{}", app.status);
    assert_eq!(app.debugger.bp_lines[&(0, 4)].hits, 1);
    // Run to line 6: the condition is not true again on the way.
    app.run_to_line(0, 6);
    run_until_paused(&mut app);
    assert_eq!(app.session.current_line(), Some((0, 6)));
    assert_eq!(app.session.x86().unwrap().cpu.regs[CX], 0);
    // ALU window data: the last operation was the final INC AX.
    assert_eq!(app.session.x86().unwrap().cpu.alu_trace.op, rustemu_core::AluOp::Inc);

    // Stop on condition: stops right after CX becomes 7.
    app.reload();
    app.debugger.bp_lines.clear();
    app.sync_breakpoints();
    app.debugger.stop_condition.text = "cx == 7".into();
    app.debugger.stop_condition.enabled = true;
    app.start_run();
    run_until_paused(&mut app);
    assert_eq!(app.session.x86().unwrap().cpu.regs[CX], 7, "{}", app.status);
    assert!(app.status.contains("is true"));
    // "changes" mode on BX.
    app.debugger.stop_condition.text = "bx".into();
    app.debugger.stop_condition.compiled = None;
    app.debugger.stop_condition.on_change = true;
    app.start_run();
    run_until_paused(&mut app);
    assert_eq!(app.session.x86().unwrap().cpu.regs[BX], 1, "{}", app.status);

    // Editing the source makes the loaded program outdated.
    assert!(!app.is_stale());
    app.doc.text.push_str("; changed\n");
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
fn typing_moves_breakpoints_and_bookmarks() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.doc.text = "nop\nnop\nret\nhlt".into();
    app.toggle_breakpoint_at_line(0, 3);
    app.editor.bookmarks.insert(4);
    app.editor.pending_select = Some((0, 0));
    let mut h = harness(app);
    h.run();
    h.event(egui::Event::Text("mov ax, 1\n".into()));
    h.run();
    let app = h.state();
    assert!(app.doc.text.starts_with("mov ax, 1\nnop"), "{:?}", app.doc.text);
    assert!(app.debugger.bp_lines.contains_key(&(0, 4)) && app.debugger.bp_lines.len() == 1);
    assert_eq!(app.editor.bookmarks.iter().copied().collect::<Vec<_>>(), [5]);
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
    app.doc.text = src.into();
    app.emulate();
    app.toggle_breakpoint_at_line(0, 9);
    app.toggle_breakpoint_at_line(0, 2);
    app.debugger.bp_lines.get_mut(&(0, 2)).unwrap().condition = "ax != 0".into();
    app.start_run();
    run_until_paused(app);
    assert_eq!(app.session.current_line(), Some((0, 9)));
    rustemu_ui::debugger::add_watch(app, "ax");
    rustemu_ui::debugger::add_watch(app, "value");
    rustemu_ui::debugger::add_watch(app, "word ss:[sp]");
    let w = &mut app.windows;
    (w[Tool::Breakpoints], w[Tool::CallStack], w[Tool::Alu], w[Tool::Source], w[Tool::Emulator], w[Tool::Screen]) =
        (true, true, true, false, false, false);
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
    (w[Tool::Stack], w[Tool::Flags], w[Tool::FlagAnalyser], w[Tool::Console], w[Tool::Source], w[Tool::Screen]) =
        (true, true, true, true, false, false);
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
    app.windows[Tool::Screen] = false;
    app.windows[Tool::Source] = false;
    h.run_steps(4);
    h.get_by_label("aux").click();
    h.run_steps(4);
    save(&mut h, "17_emulator_aux_menu");
    h.get_by_label("reset").click();
    h.run_steps(2);
    assert_eq!(h.state().session.x86().unwrap().cpu.ip, 0x100);
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
    app.debugger.bp_lines.get_mut(&(0, 3)).unwrap().condition = "cx == 3".into();
    app.toggle_breakpoint_at_line(0, 6);
    app.debugger.bp_lines.get_mut(&(0, 6)).unwrap().enabled = false;
    // A breakpoint inside emu8086.inc (file 1): the first line with code of the PUTC macro.
    let inc = app.program.files.iter().position(|f| f.0 == "emu8086.inc").unwrap();
    let putc = app.program.index[inc].iter().find(|e| e.name == "PUTC").unwrap().line;
    app.toggle_breakpoint_at_line(inc, putc);
    let inc_bp = *app.debugger.bp_lines.keys().find(|k| k.0 == inc).unwrap();
    rustemu_ui::debugger::add_watch(&mut app, "cx");
    app.editor.bookmarks.insert(5);
    app.debugger.stop_condition.text = "cx == 1".into();
    app.windows[Tool::Watch] = true;
    app.windows[Tool::Alu] = true;
    app.editor.cursor_line = 4;

    // Another file has its own (empty) session.
    app.open_path(&b);
    assert!(app.debugger.bp_lines.is_empty() && app.debugger.watches.is_empty() && app.editor.bookmarks.is_empty());
    app.toggle_breakpoint_at_line(0, 4);
    app.store_settings();
    let saved = app.settings.clone();

    // "Restart": a new app from the saved settings reopens b.asm with its breakpoint.
    let mut app = App::with_settings(saved);
    assert_eq!(app.doc.path.as_deref(), Some(b.as_path()));
    assert!(app.debugger.bp_lines.contains_key(&(0, 4)) && app.debugger.bp_lines.len() == 1);
    // As in Emu8086 the editor is alone at the start: the tools open with the emulator.
    assert!(!app.windows[Tool::Watch] && !app.windows[Tool::Alu] && !app.windows[Tool::Emulator]);
    app.store_settings();
    assert!(app.settings.windows[Tool::Watch] && app.settings.windows[Tool::Alu], "kept while not shown");
    app.emulate();
    assert!(app.windows[Tool::Watch] && app.windows[Tool::Alu] && app.windows[Tool::Emulator]);
    // Back to a.asm: everything is there again.
    app.open_path(&a);
    assert_eq!(app.debugger.bp_lines[&(0, 3)].condition, "cx == 3");
    assert!(!app.debugger.bp_lines[&(0, 6)].enabled);
    assert_eq!(app.debugger.watches.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["cx"]);
    assert!(app.editor.bookmarks.contains(&5));
    assert_eq!(app.debugger.stop_condition.text, "cx == 1");
    assert_eq!(app.editor.goto_line, Some(4));
    // The emu8086.inc breakpoint returns when the program is loaded.
    assert_eq!(app.debugger.pending_breakpoints.len(), 1);
    app.emulate();
    assert!(app.debugger.pending_breakpoints.is_empty());
    assert!(app.debugger.bp_lines.contains_key(&inc_bp));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn screen_paste_and_copy() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.load_example_text(rustemu_ui::testing::PROGRAMS[0].0, rustemu_ui::testing::PROGRAMS[0].1);
    app.emulate();
    let m = app.session.x86_mut().unwrap();
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
    let text = rustemu_ui::screen::screen_text(&app.session.x86().unwrap().sys.mem);
    assert!(text.contains("Hello"), "{text}");
}

#[test]
fn no_execution_command_runs_the_old_image_after_a_failed_build() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.doc.text = LOOP_SRC.into();
    app.emulate();
    let cpu = app.session.x86().unwrap().cpu.clone();
    app.doc.text = "org 100h\nnot_an_instruction\n".into();
    app.emulate();
    assert_eq!(app.status, "Assembly failed");
    app.single_step();
    app.step_over();
    app.step_out();
    app.run_to_line(0, 2);
    assert_eq!(app.run, rustemu_ui::app::RunMode::Paused);
    assert_eq!(app.session.x86().unwrap().cpu, cpu, "the old image must not execute");
    // Fixing the code makes stepping work again (on the new build).
    app.doc.text = LOOP_SRC.into();
    app.single_step();
    assert_eq!(app.session.target.instr_count(), 1);
}

#[test]
fn address_breakpoints_belong_to_one_program() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.doc.text = LOOP_SRC.into();
    app.emulate();
    app.toggle_breakpoint_at_line(0, 4);
    let line_bp = *app.debugger.bp_addr.keys().next().unwrap();
    let addr_bp = line_bp + 1; // as if set by double-clicking in the disassembly
    app.session.target.breakpoints_mut().insert(addr_bp);
    app.reload();
    assert!(app.session.target.breakpoints().contains(&addr_bp), "kept by Reload");
    app.emulate();
    assert!(app.session.target.breakpoints().contains(&addr_bp), "kept when the same code is reassembled");
    app.doc.text = format!("{LOOP_SRC}\n    nop\n");
    app.doc.text = app.doc.text.replacen("mov cx, 10", "mov cx, 11", 1);
    app.emulate();
    assert!(!app.session.target.breakpoints().contains(&addr_bp), "dropped for a different program");
    assert!(app.session.target.breakpoints().contains(&line_bp), "line breakpoints stay");
}

#[test]
fn emulator_window_debug_and_file_menus() {
    use rustemu_core::cpu::{BX, CX};
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.doc.text = LOOP_SRC.into();
    app.emulate();
    // Select `mov bx, 1` (line 6) as if clicked in the disassembly.
    let a6 = app.session.addr_of_line(0, 6).unwrap();
    let sel = (0x0700, (a6 - rustemu_core::linear(0x0700, 0)) as u16);
    app.view.code_selected = Some(sel);
    app.run_until_selected();
    run_until_paused(&mut app);
    assert_eq!(app.session.target.pc(), a6);
    assert_eq!(app.session.x86().unwrap().cpu.regs[CX], 0);

    // set break point (selected) / show current break point / clear break point.
    let a4 = app.session.addr_of_line(0, 4).unwrap();
    app.view.code_selected = Some((0x0700, (a4 - 0x7000) as u16));
    app.toggle_breakpoint_at_addr(a4);
    assert!(app.debugger.bp_lines.contains_key(&(0, 4)), "maps to a line breakpoint");
    app.show_next_breakpoint();
    assert!(app.view.code_view_pinned && app.view.code_view_start == a4);
    app.show_current_instruction();
    assert!(!app.view.code_view_pinned);
    app.toggle_breakpoint_at_addr(a4);
    assert!(app.debugger.bp_lines.is_empty());

    // set CS:IP to selected position: back to `inc ax`.
    app.set_cs_ip_to_selected();
    assert_eq!(app.session.target.pc(), a4);

    // save the emulator's state / load from previous state.
    app.save_emulator_state();
    app.single_step();
    assert_ne!(app.session.target.pc(), a4);
    app.load_emulator_state();
    assert_eq!(app.session.target.pc(), a4);
    assert_eq!(app.session.current_line(), Some((0, 4)));

    // reset emulator and ram.
    app.reset_emulator_and_ram();
    assert!(!app.loaded());
    assert_eq!(app.session.x86().unwrap().cpu.regs[BX], 0);
    app.load_emulator_state();
    assert!(app.loaded(), "a saved state brings the program back");
    app.close_emulator();
    assert!(!app.windows[Tool::Emulator]);
}

#[test]
fn emulator_menu_snapshots() {
    let mut h = harness(app_with_example(0));
    h.run();
    let app = h.state_mut();
    app.emulate();
    (app.windows[Tool::Screen], app.windows[Tool::Source]) = (false, false);
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
    (app.windows[Tool::Screen], app.windows[Tool::Source]) = (false, false);
    h.run_steps(4);
    h.get_by_label("view").click();
    h.run_steps(4);
    save(&mut h, "20_emulator_view_menu");
    let app = h.state_mut();
    rustemu_ui::debugger::view_register(app, "SP");
    assert!(app.windows[Tool::ValueViewer] && app.debugger.value_viewer.reg == Some(7));
    app.view.device_tab = rustemu_ui::devices_ui::Device::VgaState;
    app.windows[Tool::Devices] = true;
    h.key_press(egui::Key::Escape);
    h.run_steps(6);
    save(&mut h, "21_value_viewer_vga_state");
}

#[test]
fn value_viewer_sets_registers_and_memory() {
    use rustemu_ui::debugger::{set_viewed_value, view_register};
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.load_example_text(rustemu_ui::testing::PROGRAMS[0].0, rustemu_ui::testing::PROGRAMS[0].1);
    app.emulate();
    view_register(&mut app, "BX");
    set_viewed_value(&mut app, "1010b").unwrap();
    assert_eq!(app.session.x86().unwrap().cpu.regs[rustemu_core::cpu::BX], 10);
    set_viewed_value(&mut app, "-2").unwrap();
    assert_eq!(app.session.x86().unwrap().cpu.regs[rustemu_core::cpu::BX], 0xFFFE);
    view_register(&mut app, "FLAGS");
    set_viewed_value(&mut app, "1").unwrap();
    assert!(app.session.x86().unwrap().cpu.flags & rustemu_core::cpu::flags::CF != 0);
    app.debugger.value_viewer.reg = None;
    app.debugger.value_viewer.addr = "DS:0200".into();
    set_viewed_value(&mut app, "'AB'").unwrap();
    assert_eq!(app.session.x86().unwrap().sys.mem.read16(0x7200), 0x4241);
    assert!(set_viewed_value(&mut app, "70000").unwrap_err().contains("does not fit"));
}

#[test]
fn nasm_templates_and_syntax_setting() {
    let mut app = App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    for i in [0, 2, 3] {
        app.new_from_nasm_template(i);
        app.emulate();
        assert!(app.loaded(), "template {i}: {:?}", app.build.messages.iter().map(|m| &m.text).collect::<Vec<_>>());
        assert!(app.build.messages[0].text.contains("NASM syntax"), "{}", app.build.messages[0].text);
    }
    // Forcing the Emu8086 syntax on NASM code fails; forcing NASM works.
    app.doc.text = "mov ax, [bx]\nret\n".into();
    app.settings.syntax = rustemu_ui::app::SyntaxChoice::Nasm;
    app.emulate();
    assert!(app.build.messages[0].text.contains("NASM syntax"));
    app.doc.text = "org 100h\nmov word [count], 1\nret\ncount dw 0\n".into();
    app.settings.syntax = rustemu_ui::app::SyntaxChoice::Masm;
    app.emulate();
    assert!(app.build.messages.iter().any(|m| m.error));
}

#[test]
fn nasm_editor_snapshot() {
    let i = rustemu_ui::testing::example("hello_nasm.asm");
    let mut h = harness(app_with_example(i));
    h.run();
    let app = h.state_mut();
    app.emulate();
    (app.windows[Tool::Emulator], app.windows[Tool::Screen], app.windows[Tool::Source]) = (false, false, false);
    h.run_steps(4);
    save(&mut h, "22_nasm_editor");
    assert!(h.state().build.messages[0].text.contains("NASM syntax, COM format"));
}

#[test]
fn memory_drive_window_and_storage() {
    // The browser's drive C:: a program writes a file next to an uploaded one, the "drive C:"
    // window lists both and previews one, and the drive survives a restart through storage.
    let mut app = app_with_example(0);
    app.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    app.add_drive_file("data/in.txt", b"first line\r\nsecond line\r\n".to_vec());
    app.doc.text = "org 100h
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
    let drive = app.session.dos_memory_drive().unwrap().clone();
    assert_eq!(drive.read("out.txt"), Some(&b"done"[..]), "{}", app.status);

    let mut storage = MemStorage::default();
    app.persist(&mut storage);
    let mut restarted = app_with_example(0);
    restarted.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    restarted.restore_drive(&storage);
    assert_eq!(restarted.session.dos_memory_drive(), Some(&drive));

    app.windows = Default::default();
    app.windows[Tool::Drive] = true;
    app.web.drive_preview = Some("DATA/IN.TXT".into());
    let mut h = harness(app);
    h.run_steps(4);
    assert!(h.query_by_label("C:\\OUT.TXT").is_some());
    save(&mut h, "23_drive_window");
    h.get_by_label("C:\\OUT.TXT").click();
    h.run_steps(2);
    assert_eq!(h.state().web.drive_preview.as_deref(), Some("OUT.TXT"));
}

#[test]
fn drive_c_is_a_folder_of_its_own() {
    // As Emu8086's vdrive: a program's files go to drive C:, not next to its source; the option
    // makes drive C: the program's folder.
    let base = std::env::temp_dir().join(format!("rustemu_vdrive_{}", std::process::id()));
    let (source_dir, drive) = (base.join("src"), base.join("vdrive").join("C"));
    std::fs::create_dir_all(&source_dir).unwrap();
    let mut app = App::with_settings(Settings { last_file: None, ..Settings::default() });
    app.drive_dir = drive.clone();
    app.doc.path = Some(source_dir.join("make.asm"));
    app.doc.text = "org 100h
    mov ah, 3Ch
    xor cx, cx
    mov dx, offset fname
    int 21h
    ret
fname db 'made.txt', 0
"
    .into();
    app.emulate();
    app.start_run();
    run_until_paused(&mut app);
    assert!(drive.join("MADE.TXT").exists(), "{}", app.status);
    assert!(!source_dir.join("MADE.TXT").exists());
    assert_eq!(app.host_drive(), drive);

    app.settings.drive_c_source_folder = true;
    app.emulate();
    app.start_run();
    run_until_paused(&mut app);
    assert!(source_dir.join("MADE.TXT").exists(), "{}", app.status);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn full_storage_is_reported_and_retried() {
    // The browser keeps the old value when its quota is full: the user is told once, the write is
    // tried again on the next save and goes through once there is room.
    let mut app = browser_app();
    app.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    let mut storage = MemStorage { quota: Some(64 * 1024), ..MemStorage::default() };
    app.persist(&mut storage);
    assert!(!app.storage_failed() && app.dialogs.notice.is_none());

    app.add_drive_file("big.bin", vec![0x55; 100 * 1024]);
    app.persist(&mut storage);
    assert!(app.storage_failed());
    let notice = app.dialogs.notice.take().expect("a notice");
    assert_eq!(notice.0, "Browser storage is full");
    assert!(notice.1.contains("drive C:"), "{}", notice.1);
    let writes = storage.writes;
    app.persist(&mut storage);
    assert!(storage.writes > writes, "the failed value is written again");
    assert!(app.dialogs.notice.is_none(), "the notice is shown once");

    storage.quota = None;
    app.persist(&mut storage);
    assert!(!app.storage_failed());
    let mut restarted = browser_app();
    restarted.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    restarted.restore_drive(&storage);
    assert!(restarted.session.dos_memory_drive().unwrap().read("big.bin").is_some());
}

#[test]
fn programs_from_links_get_a_temporary_drive() {
    // A program from a link runs with an empty drive C: of its own: it doesn't see the user's
    // files and what it writes is not kept; saved, it runs with the user's drive.
    let mut app = app_with_example(0);
    app.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
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
    app.open_link(rustemu_ui::share::Shared {
        name: "link.asm".into(),
        text: text.into(),
        run: true,
        arch: rustemu_session::Arch::I8086,
    });
    run_until_paused(&mut app);
    let temporary = app.session.dos_memory_drive().unwrap();
    assert_eq!(temporary.files().map(|f| f.0).collect::<Vec<_>>(), ["OUT.TXT"], "{}", app.status);
    assert_eq!(app.dialogs.notice.as_ref().map(|n| n.0.as_str()), Some("Program from a link"));
    // The user's drive is the one kept, untouched.
    let mut storage = MemStorage::default();
    app.persist(&mut storage);
    let mut restarted = app_with_example(0);
    restarted.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    restarted.restore_drive(&storage);
    let own = restarted.session.dos_memory_drive().unwrap();
    assert_eq!(own.files().map(|f| f.0).collect::<Vec<_>>(), ["SECRET.TXT"]);

    app.dialogs.notice = None;
    app.windows = Default::default();
    app.windows[Tool::Drive] = true;
    let mut h = harness(app);
    h.run_steps(4);
    save(&mut h, "24_drive_from_link");
    let mut app = h.into_state();

    // Saved, it is the user's program.
    app.save_program("link.asm");
    app.emulate();
    app.start_run();
    run_until_paused(&mut app);
    let own = app.session.dos_memory_drive().unwrap();
    assert_eq!(own.files().map(|f| f.0).collect::<Vec<_>>(), ["SECRET.TXT", "SEEN.TXT"]);
    assert!(app.dialogs.notice.is_none());
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
    app.doc.text = CP437_SRC.into();
    app.windows = Default::default();
    let mut h = harness(app);
    let app = h.state_mut();
    app.emulate();
    app.windows[Tool::Emulator] = false;
    app.windows[Tool::Screen] = true;
    app.start_run();
    run_until_paused(app);
    let mem = &app.session.x86().unwrap().sys.mem;
    assert_eq!(rustemu_core::video::Video::read_cell(mem, 0, 2, 0), (2, 7), "☻ in row 0");
    h.run_steps(3);
    save(&mut h, "24_screen_cp437");
}

/// An app with files in browser storage (the web version).
fn browser_app() -> App {
    let mut app = app_with_example(0);
    app.front.browser = true;
    app
}

#[test]
fn programs_are_kept_in_the_browser() {
    // A new program: Save asks for its name, then it is in "my programs".
    let mut app = browser_app();
    app.new_from_template(0);
    app.doc.text.push_str("; mine\n");
    assert!(!app.save());
    app.dialogs.save_name.as_mut().unwrap().text = "first".into();
    let mut h = harness(app);
    h.run_steps(2);
    save(&mut h, "25_save_in_browser");
    h.get_by_label("Save").click();
    h.run_steps(2);
    let app = h.state_mut();
    assert_eq!(app.doc.program.as_deref(), Some("first.asm"));
    assert!(!app.dirty() && app.dialogs.save_name.is_none());

    // The open program and its unsaved changes come back on the next visit.
    app.doc.text.push_str("nop\n");
    let mut storage = MemStorage::default();
    app.persist(&mut storage);
    let writes = storage.writes;
    app.persist(&mut storage);
    assert_eq!(storage.writes, writes, "unchanged values are not written again");
    let mut next = browser_app();
    next.restore_browser(&storage);
    assert_eq!(next.doc.program.as_deref(), Some("first.asm"));
    assert_eq!(next.doc.text, app.doc.text);
    assert!(next.dirty());

    // Another program with unsaved changes: the unsaved changes dialog names it first, then opens.
    next.import_program("other.asm", "org 100h\nret\n".into());
    next.doc.text.push_str("; edited\n");
    next.doc.program = None;
    next.guarded(PendingAction::Program("first.asm".into()));
    let mut h = harness(next);
    h.run_steps(2);
    h.get_by_label("Save").click();
    h.run_steps(2);
    h.state_mut().dialogs.save_name.as_mut().unwrap().text = "copy".into();
    h.run_steps(1);
    h.get_by_label("Save").click();
    h.run_steps(2);
    let app = h.state_mut();
    assert!(app.web.programs["copy.asm"].text.ends_with("; edited\n"));
    assert_eq!(app.doc.program.as_deref(), Some("first.asm"), "then the chosen program opened");

    // An upload named like a different program is kept under a new name; the same text is not
    // duplicated.
    app.import_program("first.asm", "org 100h\nret\n".into());
    assert_eq!(app.doc.program.as_deref(), Some("first (2).asm"));
    app.import_program("first.asm", app.web.programs["first.asm"].text.clone());
    assert_eq!(app.doc.program.as_deref(), Some("first.asm"));
    assert_eq!(app.recent_programs()[0], "first.asm");

    // The "my programs" window.
    app.windows = Default::default();
    app.windows[Tool::Programs] = true;
    h.run_steps(3);
    assert!(h.query_by_label("copy.asm").is_some());
    save(&mut h, "26_my_programs");
    h.get_all_by_label("delete…").next().unwrap().click();
    h.run_steps(2);
    h.get_by_label("yes").click();
    h.run_steps(2);
    assert_eq!(h.state().web.programs.len(), 3);
}

#[test]
fn share_links_open_the_program() {
    let mut app = app_with_example(0);
    app.front.share_base = Some("https://example.org/RustEmu8086/".into());
    app.dialogs.share = Some(rustemu_ui::share::ShareDialog { run: true });
    let link = app.share_link(true);
    assert!(link.starts_with("https://example.org/RustEmu8086/#name=hello.asm&code="), "{link}");
    let shared = rustemu_ui::share::parse(link.split_once('#').unwrap().1).unwrap();
    assert_eq!((shared.text.as_str(), shared.run), (app.doc.text.as_str(), true));
    let mut h = harness(app);
    h.run_steps(2);
    save(&mut h, "27_share_link");
    h.get_by_label("copy").click();
    h.run_steps(2);
    assert!(h.state().dialogs.share.is_none());

    // Opening the link: the program is loaded and runs; unsaved changes are asked about first.
    let mut other = app_with_example(5);
    other.doc.text.push_str("; edited\n");
    other.open_link(shared.clone());
    assert!(other.dialogs.confirm_discard.is_some() && other.doc.text.ends_with("; edited\n"));
    let mut clean = app_with_example(5);
    clean.open_link(shared);
    assert_eq!(clean.file_name(), "hello.asm");
    assert!(!clean.dirty() && clean.loaded());
    run_until_paused(&mut clean);
    assert_eq!(String::from_utf8_lossy(clean.session.target.output()), "Hello, World!\r\n");
}

/// Test program `name` in the panels layout (the default is classic windows).
fn panels_app(name: &str) -> App {
    let mut app = app_with_example(rustemu_ui::testing::example(name));
    app.settings.layout = rustemu_ui::workspace::Layout::Panels;
    app
}

#[test]
fn panels_and_classic_windows() {
    use rustemu_ui::workspace::Layout;
    let mut h = harness(panels_app("calc.asm"));
    h.run();
    h.state_mut().emulate();
    let w = &mut h.state_mut().windows;
    (w[Tool::Memory], w[Tool::Vars], w[Tool::Watch], w[Tool::CallStack], w[Tool::Devices]) =
        (true, true, true, true, true);
    h.run_steps(8);
    save(&mut h, "40_panels_debugging");
    // Every open tool has a tab, nothing floats over the editor.
    let tree = h.state().panels.clone().unwrap();
    for t in [Tool::Editor, Tool::Emulator, Tool::Screen, Tool::Source, Tool::Watch, Tool::Devices] {
        assert!(tree.tiles.find_pane(&t).is_some(), "{t:?} has no tab");
    }
    // The memory window floats: as a tab it took the emulator's column.
    assert!(tree.tiles.find_pane(&Tool::Memory).is_none());
    h.get_by_label_contains("memory");
    // The tabs of the inspection tools are below the emulator, the screen is on the right.
    let rect = |t| tree.tiles.rect(tree.tiles.parent_of(tree.tiles.find_pane(&t).unwrap()).unwrap()).unwrap();
    assert!(rect(Tool::Watch).top() > rect(Tool::Emulator).bottom() - 1.0);
    assert!(rect(Tool::Screen).left() > rect(Tool::Emulator).right() - 1.0);
    assert!(rect(Tool::Emulator).left() > rect(Tool::Editor).right() - 1.0);

    // The layout is saved with the settings (as eframe stores them) and comes back.
    use eframe::Storage;
    h.state_mut().store_settings();
    let mut storage = MemStorage::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &h.state().settings);
    let back: Settings = eframe::get_value(&storage, eframe::APP_KEY).unwrap();
    let app = App::with_settings(back);
    assert!(app.panels.unwrap().tiles.find_pane(&Tool::Watch).is_some());
    // A layout saved by another version is dropped, not the other settings.
    let text = storage.get_string(eframe::APP_KEY).unwrap().replace("Watch", "Unknown");
    storage.set_string(eframe::APP_KEY, text);
    let back: Settings = eframe::get_value(&storage, eframe::APP_KEY).unwrap();
    assert!(back.windows[Tool::Watch]);
    assert!(App::with_settings(back).panels.is_none());

    // Classic windows: the same tools float, as in Emu8086.
    h.state_mut().settings.layout = Layout::Classic;
    h.run_steps(8);
    save(&mut h, "41_classic_windows");
    h.get_by_label("original source code");
    h.get_by_label("emulator screen (80x25 chars)");
    // Back to panels: the arrangement is kept; reset puts the tools where they start.
    h.state_mut().settings.layout = Layout::Panels;
    h.run_steps(4);
    assert!(h.state().panels.is_some());
    let ctx = h.ctx.clone();
    rustemu_ui::workspace::reset_layout(h.state_mut(), &ctx);
    h.run_steps(4);
    assert!(h.state().panels.as_ref().unwrap().tiles.find_pane(&Tool::Watch).is_some());

    // The ASCII table floats over the panels, as in Emu8086, instead of taking a tab.
    h.state_mut().windows[Tool::Ascii] = true;
    h.run_steps(4);
    save(&mut h, "42_ascii_codes");
    h.get_by_label("ASCII codes");
    // It isn't modal: the emulator keeps working while the table is open.
    h.get_by_label("▶ Run").click();
    h.run_steps(4);
    assert!(h.state().windows[Tool::Ascii] && h.state().run != rustemu_ui::app::RunMode::Paused);
    h.state_mut().stop();
}

#[test]
fn short_emulator_panel_scrolls() {
    let app = panels_app("calc.asm");
    let mut h = Harness::builder()
        .with_size([1100.0, 560.0])
        .with_max_steps(64)
        .build_ui_state(|ui, app: &mut App| crate::ui::main_ui(app, ui), app);
    rustemu_ui::fonts::install(&h.ctx);
    h.run();
    h.state_mut().emulate();
    h.state_mut().windows[Tool::Memory] = true;
    h.run_steps(8);
    let tree = h.state().panels.clone().unwrap();
    let panel = tree.tiles.rect(tree.tiles.parent_of(tree.tiles.find_pane(&Tool::Emulator).unwrap()).unwrap()).unwrap();
    // The registers and the buttons under the list don't fit: the panel scrolls to them.
    assert!(h.get_by_label("flags").rect().top() > panel.bottom());
    // Over the registers: the wheel over the disassembly scrolls the code instead.
    h.hover_at(egui::pos2(panel.left() + 40.0, panel.center().y));
    for _ in 0..4 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -100.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Default::default(),
        });
        h.run_steps(2);
    }
    h.run_steps(8);
    save(&mut h, "43_panels_short_emulator");
    assert!(h.get_by_label("flags").rect().bottom() < panel.bottom());
}

#[test]
fn disassembly_scrolls_and_tools_go_under_the_screen() {
    let mut h = harness(panels_app("calc.asm"));
    h.run();
    h.state_mut().emulate();
    h.run_steps(8);
    let pc = h.state().session.target.pc();
    let wheel = |h: &mut Harness<'static, App>, dy: f32| {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Default::default(),
        });
        h.run_steps(2);
    };
    // The wheel over the disassembly scrolls the code: down three rows and back.
    h.hover_at(h.get_by_label("0700:0101").rect().center());
    for _ in 0..3 {
        wheel(&mut h, -24.0);
    }
    assert!(h.state().view.code_view_pinned);
    assert!(h.state().view.code_view_start > pc);
    save(&mut h, "44_disasm_scrolled");
    for _ in 0..6 {
        wheel(&mut h, 24.0);
    }
    assert!(h.state().view.code_view_start <= pc);
    // Back up to the data rows of the PRINT macro: they are shown whole, not from a byte inside.
    h.state_mut().scroll_disasm(3);
    h.state_mut().scroll_disasm(-1);
    h.run_steps(2);
    h.get_by_label("DB 'First number: ', 0");
    // The next step shows CS:IP again.
    h.get_by_label("⌖ CS:IP").click();
    h.run_steps(2);
    assert!(!h.state().view.code_view_pinned);

    // vars, stack and the debug log open under the screen; flags float.
    let w = &mut h.state_mut().windows;
    (w[Tool::Vars], w[Tool::Stack], w[Tool::Console], w[Tool::Flags]) = (true, true, true, true);
    h.run_steps(8);
    save(&mut h, "45_panels_under_screen");
    let tree = h.state().panels.clone().unwrap();
    let rect = |t| tree.tiles.rect(tree.tiles.parent_of(tree.tiles.find_pane(&t).unwrap()).unwrap()).unwrap();
    for t in [Tool::Vars, Tool::Stack, Tool::Console] {
        assert!(rect(t).top() > rect(Tool::Screen).bottom() - 1.0, "{t:?}");
        assert!(rect(t).left() > rect(Tool::Emulator).right() - 1.0, "{t:?}");
    }
    h.get_by_label("analyse").click();
    h.run_steps(4);
    h.get_by_label("lexical flag analyser");
}

#[test]
fn examples_menu_and_more_examples() {
    let mut h = harness(app_with_example(0));
    h.run();
    // The Examples menu of Emu8086.
    h.get_by_label("📚 examples").click();
    h.run_steps(4);
    save(&mut h, "46_examples_menu");
    h.get_by_label("compare numbers").click();
    h.run_steps(4);
    assert_eq!(h.state().doc.example_key.as_deref(), Some("emu8086-example:4_sample.asm"));
    assert!(h.state().doc.text.starts_with("name \"flags\"\n"), "DOS line ends are gone");
    // "more examples..." lists every file of Emu8086's examples folder.
    h.get_by_label("📚 examples").click();
    h.run_steps(4);
    h.get_by_label("more examples...").click();
    h.run_steps(4);
    assert_eq!(h.state().dialogs.more_examples, Some(false));
    h.state_mut().dialogs.examples_filter = "palin".into();
    h.run_steps(4);
    save(&mut h, "47_more_examples");
    h.get_by_label("palindrome.asm").click();
    h.run_steps(4);
    assert_eq!(h.state().doc.example_key.as_deref(), Some("emu8086-example:palindrome.asm"));
    assert_eq!(h.state().dialogs.more_examples, None);
}

#[test]
fn classic_windows_by_default_and_flag_tips() {
    assert_eq!(rustemu_ui::app::Settings::default().layout, rustemu_ui::workspace::Layout::Classic);
    let mut h = harness(app_with_example(rustemu_ui::testing::example("calc.asm")));
    h.run();
    h.state_mut().emulate();
    h.run_steps(8);
    assert!(h.state().panels.is_none());
    // The flags under the registers say what they are.
    h.hover_at(h.get_by_label("C0").rect().center());
    h.run_steps(30);
    save(&mut h, "48_classic_emulator");
    h.get_by_label("CF = 0: Carry Flag");
}

#[test]
fn options_menu() {
    use rustemu_ui::app::{SyntaxChoice, ThemeChoice};
    use rustemu_ui::workspace::Layout;
    let mut h = harness(app_with_example(0));
    h.run();
    let menu = |h: &mut Harness<'static, App>, item: &str| {
        h.get_by_label("Options").click();
        h.run_steps(4);
        h.get_by_label(item).click();
        h.run_steps(4);
    };
    h.get_by_label("Options").click();
    h.run_steps(4);
    save(&mut h, "50_options_menu");
    h.get_by_label("Options").click();
    h.run_steps(4);
    menu(&mut h, "NASM");
    assert!(h.state().settings.syntax == SyntaxChoice::Nasm);
    menu(&mut h, "panels");
    assert_eq!(h.state().settings.layout, Layout::Panels);
    menu(&mut h, "dark");
    assert!(h.state().settings.theme == ThemeChoice::Dark);
    assert!(h.ctx.global_style().visuals.dark_mode);
    menu(&mut h, "all options…");
    assert!(h.state().dialogs.options);
}

#[test]
fn screen_window_resizes_with_the_mouse() {
    let mut h = harness(app_with_example(0));
    h.run();
    h.state_mut().emulate();
    h.run_steps(8);
    let window =
        |h: &Harness<'static, App>| egui::AreaState::load(&h.ctx, egui::Id::unique("screen_window")).unwrap().rect();
    let copy = |h: &Harness<'static, App>| h.get_by_label("copy").rect();
    let before = window(&h);
    // Drag the bottom-right corner up and to the left.
    let corner = before.right_bottom() - egui::vec2(3.0, 3.0);
    h.hover_at(corner);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: corner,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    for k in 1..=10 {
        h.hover_at(corner - egui::vec2(20.0, 12.0) * k as f32);
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton {
        pos: corner - egui::vec2(200.0, 120.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run_steps(8);
    save(&mut h, "51_screen_window_small");
    let after = window(&h);
    assert!(after.width() < before.width() - 100.0, "{before:?} -> {after:?}");
    // The screen fills the window; the buttons stay under it, inside the window.
    assert!(copy(&h).right() <= after.right() && copy(&h).bottom() <= after.bottom(), "{:?} {after:?}", copy(&h));
}

#[test]
fn kernel_to_floppy_and_boot_with_the_loader_source() {
    const KERNEL: &str = "#make_bin#\n#load_segment=0800#\n#load_offset=0000#\n#cs=0800#\n#ip=0000#\norg 0\n\
        jmp short start\ndb 'K', 'R'\nstart: mov ax, 1234h\nhang: jmp hang\n";
    const LOADER: &str = "#make_boot#\norg 7C00h\nstart: mov ax, 0800h\nmov es, ax\nxor bx, bx\n\
        mov ax, 0201h\nmov cx, 0002h\nmov dh, 0\nint 13h\njmp 0800h:0000h\n\
        db 510 - ($ - start) dup(0)\ndw 0AA55h\n";
    let mut h = harness(app_with_example(0));
    h.run();
    // The kernel: emulate it, then write the loaded program to the floppy from sector 2.
    h.state_mut().load_example_text("kernel.asm", KERNEL);
    h.state_mut().emulate();
    h.state_mut().dialogs.bin_to_floppy =
        Some(rustemu_ui::app::BinToFloppy { loaded: true, path: String::new(), drive: 0, cyl: 0, head: 0, sector: 2 });
    h.run_steps(4);
    save(&mut h, "52_write_to_floppy");
    h.get_by_label("write").click();
    h.run_steps(4);
    assert!(h.state().dialogs.bin_to_floppy.is_none(), "{}", h.state().status);
    assert!(h.state().status.contains("written to FLOPPY_0"), "{}", h.state().status);
    // The loader: emulate, write its boot sector, boot from the floppy - its source stays.
    h.state_mut().load_example_text("loader.asm", LOADER);
    h.state_mut().emulate();
    h.state_mut().session.write_boot_sector(0).unwrap();
    h.state_mut().boot_from_floppy(0);
    h.run_steps(4);
    assert!(h.state().status.starts_with("Booted from FLOPPY_0"), "{}", h.state().status);
    assert!(!h.state().program.files.is_empty() && h.state().session.program.is_some());
    // Run: the loader reads the kernel and jumps to it.
    let m = h.state_mut().session.x86_mut().unwrap();
    m.run(10_000);
    assert_eq!((m.cpu.cs(), m.cpu.regs[0]), (0x0800, 0x1234));
}

#[test]
fn reference_in_russian() {
    use rustemu_ui::reference::{Lang, Topic};
    let mut h = harness(app_with_example(0));
    h.run();
    h.state_mut().reference.show(Topic::Instr(rustemu_ui::reference::find_instruction("SHL").unwrap()));
    h.run_steps(4);
    h.get_by_label("Shift left (multiply by 2): bits move left, 0 comes in, the last bit out goes to CF.");
    // The language switch: articles, sections and flags in Russian.
    h.get_by_label("RU").click();
    h.run_steps(4);
    save(&mut h, "53_reference_russian");
    assert_eq!(h.state().reference.lang, Lang::Ru);
    h.get_by_label("Синтаксис");
    h.get_by_label("Флаги");
    // The choice is kept with the settings and used by the editor tooltips.
    h.state_mut().store_settings();
    let app = App::with_settings(h.state().settings.clone());
    assert_eq!(app.reference.lang, Lang::Ru);
    let (title, details) = Topic::Instr(0).tooltip(app.reference.lang);
    assert!(title.contains("Скопировать") && details.starts_with("флаги"), "{title} / {details}");
}

#[test]
fn fpu_window_shows_the_8087() {
    const SRC: &str = "org 100h\nfinit\nfld two\nfsqrt\nfldpi\nret\ntwo dd 2.0\n";
    let mut h = harness(app_with_example(0));
    h.run();
    h.state_mut().load_example_text("fpu.asm", SRC);
    h.state_mut().emulate();
    h.state_mut().windows[Tool::Fpu] = true;
    for _ in 0..5 {
        h.state_mut().single_step();
    }
    h.run_steps(8);
    save(&mut h, "54_fpu_window");
    let f = &h.state().session.x86().unwrap().sys.fpu;
    assert_eq!((f.top(), rustemu_core::fpu::to_f64(f.st(1))), (6, 2f64.sqrt()));
    h.get_by_label("3.141592653589793");
    h.get_by_label("1.4142135623730951");
    h.get_by_label("TOP = 6");
    // The raw bytes, as Emu8086's FPU window shows them: ST(0) = pi, lowest byte first.
    h.get_by_label("raw bytes (as in Emu8086)").click();
    h.run_steps(4);
    save(&mut h, "54_fpu_window");
    h.get_by_label("control register:");
    h.get_by_label("35 C2 68 21 A2 DA 0F C9 00 40");
}

/// Runs the program until it stops (at most `frames` frames).
fn run_frames(h: &mut Harness<'static, App>, frames: usize) {
    for _ in 0..frames {
        let ctx = h.ctx.clone();
        h.state_mut().tick(&ctx);
        h.run();
        if h.state().run == rustemu_ui::app::RunMode::Paused {
            break;
        }
    }
}

#[test]
fn mips_workspace() {
    use rustemu_session::Arch;
    let mut h = harness(App::with_settings(Settings { last_file: None, ..Default::default() }));
    h.run();
    let x86_text = h.state().doc.text.clone();
    // The MIPS workspace opens with its first example.
    h.state_mut().switch_arch(Arch::Mips32);
    h.run();
    assert_eq!(h.state().arch(), Arch::Mips32);
    assert_eq!(h.state().file_name(), "hello.s");
    save(&mut h, "40_mips_editor");
    h.get_by_label("▶ emulate").click();
    h.run();
    assert!(h.state().loaded(), "{:?}", h.state().build.messages.iter().map(|m| &m.text).collect::<Vec<_>>());
    assert!(h.state().windows[Tool::RunIo] && !h.state().windows[Tool::Screen]);
    // Registers in groups: all 32 general purpose ones, then Special, CP0 and FPU.
    h.get_by_label("$v0");
    h.get_by_label("$ra");
    h.get_by_label("General purpose registers");
    // The PC above every group.
    h.get_by_label("PC");
    h.get_by_label("00400000");
    // The code list: MARS's Basic column; `la` takes two instructions, the second marked as its.
    h.get_by_label("basic");
    h.get_by_label("7: la $a0, msg");
    h.get_by_label("  ↳");
    save(&mut h, "41_mips_loaded");
    h.get_by_label("Special").click();
    h.run();
    h.get_by_label("HI");
    h.get_by_label("LO");
    // Special lists the PC too, as MARS does.
    assert_eq!(h.query_all_by_label("PC").count(), 2);
    h.get_by_label("CP0").click();
    h.run();
    h.get_by_label("Status");
    h.get_by_label("0000FF11");
    h.get_by_label("FPU").click();
    h.run();
    h.get_by_label("$f31");
    h.get_by_label("condition flags");
    save(&mut h, "41_mips_fpu");
    h.get_by_label("DEC").click();
    h.get_by_label("GPR").click();
    h.run();
    h.get_by_label("2147479548");
    save(&mut h, "41_mips_registers_dec");
    h.get_by_label("HEX").click();
    h.run();
    h.state_mut().single_step();
    h.run();
    assert_eq!(h.state().session.mips().unwrap().cpu.regs[2], 4, "li $v0, 4");
    h.state_mut().start_run();
    run_frames(&mut h, 20);
    assert!(h.state().session.mips().unwrap().console.text().contains("Hello, MIPS!"));
    h.get_by_label_contains("program is finished running");
    save(&mut h, "42_mips_finished");

    // Input through the Run I/O line.
    h.state_mut().load_mips_example(1);
    h.state_mut().emulate();
    h.state_mut().start_run();
    run_frames(&mut h, 20);
    assert!(h.state().session.target.is_waiting_input());
    h.state_mut().mips.input = "100".into();
    h.run();
    h.get_by_label("send").click();
    h.run();
    h.state_mut().start_run();
    run_frames(&mut h, 20);
    let out = h.state().session.mips().unwrap().console.text();
    assert!(out.contains("sum = 5050"), "{out:?} {}", h.state().status);

    // Memory: unmapped addresses are marked, the data segment has the string.
    h.state_mut().windows[Tool::Memory] = true;
    h.state_mut().view.mem_view_addr = rustemu_mips::memory::DATA_BASE;
    h.run();
    h.get_by_label("10010000  ");
    h.get_by_label("203d206e ");
    save(&mut h, "43_mips_memory");
    // A click on a word while paused edits it; Enter writes it.
    h.get_by_label("203d206e ").click();
    h.run();
    assert_eq!(h.state().mips.mem_edit.as_ref().map(|e| e.0), Some(rustemu_mips::memory::DATA_BASE));
    h.state_mut().mips.mem_edit.as_mut().unwrap().1 = "0x41424344".into();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().mips.mem_edit, None);
    assert_eq!(h.state().session.mips().unwrap().mem.peek32(rustemu_mips::memory::DATA_BASE), 0x4142_4344);
    h.get_by_label("41424344 ");

    // Back to the 8086: its document and windows are as they were.
    h.state_mut().switch_arch(Arch::I8086);
    h.run();
    assert_eq!(h.state().doc.text, x86_text);
    assert!(!h.state().windows[Tool::RunIo]);
}

#[test]
fn mips_panels() {
    use rustemu_session::Arch;
    let mut app = App::with_settings(Settings { last_file: None, ..Default::default() });
    app.settings.layout = rustemu_ui::workspace::Layout::Panels;
    app.switch_arch(Arch::Mips32);
    // Fonts are installed with the harness: the emulator opens after that.
    let mut h = harness(app);
    h.state_mut().emulate();
    h.state_mut().windows[Tool::Memory] = true;
    h.run_steps(6);
    // Run I/O takes the screen's place; the 8086's tools are not tabs.
    assert!(h.query_all_by_label("Run I/O").next().is_some());
    assert!(h.query_by_label("screen").is_none());
    save(&mut h, "44_mips_panels");
}

/// A MIPS example with macros and `.include`: the included file is found among the built-in
/// ones, the code list shows the body lines as MARS does, the program runs.
#[test]
fn mips_macros() {
    use rustemu_session::Arch;
    let mut h = harness(App::with_settings(Settings { last_file: None, ..Default::default() }));
    h.run();
    h.state_mut().switch_arch(Arch::Mips32);
    let i = rustemu_ui::examples::MIPS_EXAMPLES.iter().position(|e| e.1 == "macros.s").unwrap();
    h.state_mut().load_mips_example(i);
    h.run();
    h.get_by_label("▶ emulate").click();
    h.run();
    assert!(h.state().loaded(), "{:?}", h.state().build.messages.iter().map(|m| &m.text).collect::<Vec<_>>());
    // print_str ("Squares: ") at line 21: the body lines of print_macros.s, substituted.
    h.get_by_label_contains("21: <13> la $a0, text_M0");
    save(&mut h, "42_mips_macros");
    h.state_mut().start_run();
    run_frames(&mut h, 20);
    assert_eq!(h.state().session.mips().unwrap().console.text(), "Squares: 1 4 9 16 25 \n");
}

/// MARS's memory-mapped devices: a program that uses them opens the Keyboard and Display window,
/// keys typed in its keyboard area reach the program, the Bitmap Display shows memory.
#[test]
fn mips_devices() {
    use rustemu_session::Arch;
    let mut h = harness(App::with_settings(Settings { last_file: None, ..Default::default() }));
    h.run();
    h.state_mut().switch_arch(Arch::Mips32);
    let i = rustemu_ui::examples::MIPS_EXAMPLES.iter().position(|e| e.1 == "keyboard_display.s").unwrap();
    h.state_mut().load_mips_example(i);
    h.run();
    h.get_by_label("▶ emulate").click();
    h.run();
    assert!(h.state().windows[Tool::MmioDisplay], "the program reaches 0xffff0000");
    h.state_mut().start_run();
    run_frames(&mut h, 5);
    // The keyboard area has the focus: typed text goes to the program.
    h.event(egui::Event::Text("hello, mips".into()));
    // One key a frame, when the program has read the previous one.
    run_frames(&mut h, 30);
    let shown = h.state().session.mips().unwrap().mmio.text().to_vec();
    assert_eq!(String::from_utf8_lossy(&shown), "Type in the KEYBOARD area (Esc clears):\nHELLO, MIPS");
    assert_eq!(h.state().devices.typed, "hello, mips");
    h.get_by_label_contains("DISPLAY: Store to Transmitter Data 0xffff000c");
    save(&mut h, "45_mips_keyboard_display");

    let i = rustemu_ui::examples::MIPS_EXAMPLES.iter().position(|e| e.1 == "bitmap.s").unwrap();
    h.state_mut().stop();
    h.state_mut().load_mips_example(i);
    h.run();
    h.get_by_label("▶ emulate").click();
    h.run();
    h.state_mut().windows[Tool::MmioDisplay] = false;
    h.state_mut().windows[Tool::Bitmap] = true;
    h.state_mut().start_run();
    for _ in 0..600 {
        if h.state().session.target.terminated().is_some() {
            break;
        }
        run_frames(&mut h, 1);
    }
    assert!(h.state().session.target.terminated().is_some(), "the drawing ends");
    run_frames(&mut h, 2);
    h.get_by_label("Base address for display");
    save(&mut h, "46_mips_bitmap");

    let i = rustemu_ui::examples::MIPS_EXAMPLES.iter().position(|e| e.1 == "digital_lab.s").unwrap();
    h.state_mut().load_mips_example(i);
    h.run();
    h.get_by_label("▶ emulate").click();
    h.run();
    assert!(h.state().windows[Tool::DigitalLab], "the program reaches 0xffff0010");
    h.state_mut().windows[Tool::Bitmap] = false;
    h.state_mut().start_run();
    run_frames(&mut h, 3);
    h.get_by_label("b").click();
    run_frames(&mut h, 5);
    h.state_mut().stop();
    run_frames(&mut h, 2);
    let m = h.state().session.mips().unwrap();
    assert_eq!((m.lab.pressed(), m.lab.segments()[0], m.lab.segments()[1] & 0x7f), (Some(11), 0x06, 0x7c));
    save(&mut h, "47_mips_digital_lab");
}
