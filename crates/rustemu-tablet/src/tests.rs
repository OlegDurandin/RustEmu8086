//! UI tests of the tablet layout (egui_kittest); snapshots go to `target/ui-snapshots/*.png`.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rustemu_ui::testing::{app_with_example, example, save};

use crate::{Tab, TabletApp};
use rustemu_ui::tools::Tool;

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
    let line = app.doc.text.lines().position(|l| l.trim_start().starts_with("loop")).map_or(12, |l| l as u32 + 1);
    app.toggle_breakpoint_at_line(0, line);
    for w in ["ax", "cx * 2"] {
        app.debugger.watches.push(rustemu_ui::app::Watch { text: w.into(), compiled: None, before: None });
    }
    let ds = app.session.x86().unwrap().cpu.sregs[rustemu_core::cpu::DS];
    let mut t = TabletApp::new(app);
    crate::memory::show_address(&mut t.app, &mut t.state.mem, rustemu_core::linear(ds, 0x0102));
    t
}

#[test]
fn tablet_layout_snapshots() {
    for (size, suffix) in [
        ([1376.0, 1032.0], "landscape"),
        ([1024.0, 768.0], "landscape_1024"),
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
    assert!(h.state().app.session.target.terminated().is_some(), "{}", h.state().app.status);
    assert!(rustemu_ui::screen::screen_text(&h.state().app.session.x86().unwrap().sys.mem).contains("Hello, World!"));
    // Reset reloads it; Step executes one instruction.
    h.get_by_label("Reset").click();
    h.run_steps(2);
    assert_eq!(h.state().app.session.target.instr_count(), 0);
    h.get_by_label("Step").click();
    h.run_steps(2);
    assert_eq!(h.state().app.session.target.instr_count(), 1);
    h.get_by_label("Debugger").click();
    h.run_steps(2);
    assert_eq!(h.state().state.tab, Tab::Debugger);
    // A flag cell shows the flag in the inspector, which sets it.
    let cf = h.state().app.session.x86().unwrap().cpu.flags & rustemu_core::cpu::flags::CF;
    h.get_by_label("CF = 0").click();
    h.run_steps(2);
    h.get_by_label("Set the flag").click();
    h.run_steps(2);
    assert_ne!(h.state().app.session.x86().unwrap().cpu.flags & rustemu_core::cpu::flags::CF, cf);
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
    assert!(h.state().app.session.target.is_waiting_input());
    assert!(h.state().state.keypad.open);
    save(&mut h, "31_tablet_keyboard");
    for key in ["key H", "key Shift", "key I", "key Enter"] {
        // There are two Shift keys.
        h.get_all_by_label(key).next().unwrap().click();
        h.run_steps(2);
    }
    h.run_steps(6);
    assert!(h.state().app.session.target.terminated().is_some(), "{}", h.state().app.status);
    // Shift applied to "i" only: "hI", printed back in upper case.
    let text = rustemu_ui::screen::screen_text(&h.state().app.session.x86().unwrap().sys.mem);
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
fn debug_log_is_a_debugger_tool() {
    // The debug log is not a window over the layout (it once grew every frame): the menu opens it
    // below the code of the Debugger tab, where it keeps its size.
    let mut t = TabletApp::new(app_with_example(0));
    t.app.windows[Tool::Console] = true;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(4);
    let windows = |h: &Harness<'_, TabletApp>| {
        h.ctx.memory(|m| m.areas().visible_layer_ids().into_iter().filter(|l| l.order == egui::Order::Middle).count())
    };
    assert_eq!(windows(&h), 0);
    h.get_by_label("file, examples, tools").click();
    h.run_steps(2);
    h.get_by_label("Debug log").click();
    h.run_steps(4);
    assert_eq!(h.state().state.tab, Tab::Debugger);
    assert!(h.query_by_label("DEBUG LOG").is_some() || h.query_by_label_contains("debug console").is_some());
    h.run_steps(30);
    save(&mut h, "32_tablet_debug_log");
}

#[test]
fn share_button_and_byte_registers() {
    let mut h = tablet_harness(tablet_app(), [1376.0, 1032.0]);
    h.run_steps(3);
    // The Editor's CPU preview: AX..DX with their bytes, no SP (it is on the Debugger tab).
    for cell in ["AX = 001F", "AH = 00", "AL = 1F", "CX = 0007", "CL = 07"] {
        assert!(h.query_by_label(cell).is_some(), "{cell}");
    }
    assert!(h.query_by_label("SP = FFFC").is_none());
    h.get_by_label("share link").click();
    h.run_steps(3);
    assert!(h.state().app.dialogs.share.is_some());
    assert!(h.query_by_label("copy").is_some());
    save(&mut h, "30_tablet_share");
}

/// A program typed into the editor (not saved anywhere).
fn app_with_text(text: &str) -> TabletApp {
    let mut app = app_with_example(0);
    app.doc.text = text.into();
    app.doc.saved_text = text.into();
    TabletApp::new(app)
}

fn step(h: &mut Harness<'_, TabletApp>) {
    h.get_by_label("Step").click();
    h.run_steps(3);
}

/// Step once more: the first Step only assembles and loads the program.
fn load(h: &mut Harness<'_, TabletApp>) {
    step(h);
    assert_eq!(h.state().app.session.target.instr_count(), 0);
}

#[test]
fn visual_step_shows_what_changed() {
    let mut t = app_with_text("org 100h\nmov bx, 000Fh\nsub bl, 1\nmov ax, 0BEEFh\nret\n");
    t.state.tab = Tab::Debugger;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(2);
    assert!(h.query_by_label_contains("Press Step").is_some());
    load(&mut h);
    assert!(h.query_by_label("Nothing executed yet.").is_some());
    step(&mut h);
    step(&mut h);
    // R3: SUB BL, 1 from 000F changes BL (and so BX), not BH.
    assert!(h.query_by_label("SUB BL, 01h").is_some());
    assert!(h.query_by_label("BL 0Fh → 0Eh").is_some());
    assert!(h.query_by_label("BX 000Fh → 000Eh").is_some());
    assert!(h.query_by_label_contains("BH 00h →").is_none(), "BH did not change");
    for cell in ["BX = 000E", "BH = 00", "BL = 0E"] {
        assert!(h.query_by_label(cell).is_some(), "{cell}");
    }
    h.get_by_label("Why did this happen?").click();
    h.run_steps(2);
    assert!(h.query_by_label("SUB 0Fh − 01h = 0Eh").is_some());
    save(&mut h, "32_tablet_visual_step");
    // R11: the inspector shows a byte as 8 bits and a word as 16.
    h.get_by_label("BL = 0E").click();
    h.run_steps(2);
    assert!(h.query_by_label("BL = 0Eh · 14 · 0000 1110b").is_some());
    h.get_by_label("BX = 000E").click();
    h.run_steps(2);
    assert!(h.query_by_label("BX = 000Eh · 14 · 0000 0000 0000 1110b").is_some());
    // R4: MOV AX, BEEFh updates both bytes.
    step(&mut h);
    for cell in ["AX = BEEF", "AH = BE", "AL = EF"] {
        assert!(h.query_by_label(cell).is_some(), "{cell}");
    }
    // The other code views mark the same lines.
    for mode in ["Mixed", "Assembly"] {
        h.get_by_label(mode).click();
        h.run_steps(2);
        assert!(h.query_by_label_contains("MOV").is_some() || h.query_by_label_contains("RET").is_some());
    }
}

#[test]
fn registers_are_edited_in_the_inspector() {
    let mut t = app_with_text("org 100h\nmov ax, 1234h\nnop\njmp $\n");
    t.state.tab = Tab::Debugger;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(2);
    load(&mut h);
    step(&mut h);
    let ax = |h: &Harness<'_, TabletApp>| h.state().app.session.x86().unwrap().cpu.regs[rustemu_core::cpu::AX];
    // R1: AL = AA keeps AH.
    h.get_by_label("AL = 34").click();
    h.run_steps(2);
    h.state_mut().state.cpu.edit = "AA".into();
    h.get_by_label("Set").click();
    h.run_steps(2);
    assert_eq!(ax(&h), 0x12AA);
    // R2: AH = FF keeps AL.
    h.get_by_label("AH = 12").click();
    h.run_steps(2);
    h.state_mut().state.cpu.edit = "FF".into();
    h.get_by_label("Set").click();
    h.run_steps(2);
    assert_eq!(ax(&h), 0xFFAA);
    // R12: read-only while running.
    h.get_by_label("Run").click();
    h.run_steps(3);
    assert!(h.query_by_label("Set").is_none());
    assert!(h.query_by_label_contains("Read-only while the program runs").is_some());
}

#[test]
fn waiting_for_a_key_is_explained() {
    let i = example("echo.asm");
    let mut h = tablet_harness(TabletApp::new(app_with_example(i)), [1376.0, 1032.0]);
    h.run_steps(2);
    h.get_by_label("Run").click();
    h.run_steps(6);
    // R5: the reason, the line of the program and the keyboard.
    assert!(h.state().state.keypad.open);
    assert!(h.query_by_label("Waiting for a key — INT 21h at echo.asm:10").is_some());
    // Hide does not end the wait; the output offers the keyboard again.
    h.get_by_label("Hide").click();
    h.run_steps(2);
    assert!(h.state().app.session.target.is_waiting_input());
    h.get_by_label("Open keyboard").click();
    h.run_steps(2);
    assert!(h.state().state.keypad.open);
    // The Debugger shows the calling INT, not the BIOS, unless asked.
    h.state_mut().state.tab = Tab::Debugger;
    h.run_steps(3);
    assert!(h.query_by_label_contains("The BIOS is serving INT 21h").is_some());
    save(&mut h, "32_tablet_waiting");
    h.get_by_label("BIOS internals").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("BIOS code serving INT 21h").is_some());
}

#[test]
fn a_key_completes_a_step_that_waits() {
    // Paused in a step into INT 16h: Step is off, a key of the PC keyboard finishes the step.
    let mut t = app_with_text("org 100h\nmov ah, 0\nint 16h\nmov bl, al\nret\n");
    t.state.tab = Tab::Debugger;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(2);
    load(&mut h);
    step(&mut h);
    step(&mut h);
    assert!(h.state().app.session.target.is_waiting_input());
    assert!(h.state().state.keypad.open);
    h.get_by_label("key A").click();
    h.run_steps(3);
    let c = &h.state().app.session.x86().unwrap().cpu;
    assert!(!h.state().app.session.target.is_waiting_input());
    assert_eq!(c.regs[rustemu_core::cpu::AX], 0x1E61, "scan code and ASCII of 'a'");
    assert!(h.query_by_label("INT 16h").is_some(), "the visual step shows the INT");
}

#[test]
fn project_menu_opens_examples_and_exports() {
    let mut t = TabletApp::new(app_with_example(0));
    // As in the browser: drafts are kept there, export is a download.
    t.app.front.browser = true;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(2);
    let open_menu = |h: &mut Harness<'_, TabletApp>| {
        h.get_by_label("project menu").click();
        h.run_steps(3);
    };
    open_menu(&mut h);
    for item in [
        "New ASM file",
        "Open from Files…",
        "Example library",
        "Save draft",
        "My drafts",
        "Export .asm",
        "Share source",
    ] {
        assert!(h.query_by_label(item).is_some(), "{item}");
    }
    save(&mut h, "33_tablet_project_menu");
    h.get_by_label("Example library").click();
    h.run_steps(3);
    assert_eq!(h.state().app.dialogs.more_examples, Some(false));
    h.state_mut().app.dialogs.more_examples = None;
    open_menu(&mut h);
    h.get_by_label("Export .asm").click();
    h.run_steps(2);
    assert!(h.state().app.status.starts_with("Downloaded"), "{}", h.state().app.status);
    // Without the Web Share API the source is downloaded, and the status says so.
    open_menu(&mut h);
    h.get_by_label("Share source").click();
    h.run_steps(2);
    assert!(h.state().app.status.contains("cannot share files"), "{}", h.state().app.status);
    open_menu(&mut h);
    h.get_by_label("New ASM file").click();
    h.run_steps(2);
    assert!(h.state().app.dialogs.new_file.is_some());
}

#[test]
fn source_follows_into_and_out_of_the_library() {
    // Into SCAN_NUM of emu8086.inc (waiting for keys there) and back to the program: the source
    // shows the file of the next line, with that line in view.
    let mut t = app_with_text(
        "include 'emu8086.inc'\norg 100h\nmov ax, 1\ncall scan_num\nmov bx, cx\nmov ax, bx\nret\nDEFINE_SCAN_NUM\nend\n",
    );
    t.state.tab = Tab::Debugger;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(2);
    load(&mut h);
    let mut keys = ["key 5", "key Enter"].into_iter();
    let mut in_library = false;
    for _ in 0..200 {
        if h.state().app.session.target.is_waiting_input() {
            h.get_by_label(keys.next().unwrap_or("key Enter")).click();
            h.run_steps(3);
        } else {
            step(&mut h);
        }
        let app = &h.state().app;
        let Some(v) = app.session.current_view(true) else { break };
        let (file, first, visible) = h.state().state.debug.shown.unwrap();
        assert_eq!(file, v.file, "the source shows the file of the next line");
        let last = first + visible as u32 - 1;
        assert!((first..=last).contains(&v.line), "line {} out of view ({first}..={last})", v.line);
        in_library |= v.file != 0;
        if in_library && v.file == 0 && v.line == 6 {
            return;
        }
    }
    panic!("did not come back from scan_num");
}

#[test]
fn long_instructions_keep_their_bytes_apart() {
    // 2E C6 06 64 02 00 (6 bytes) once ran into the mnemonic.
    let mut t = app_with_text("org 100h\nmov byte ptr cs:[264h], 0\nmov cx, 0\nret\n");
    t.state.tab = Tab::Debugger;
    t.state.debug.mode = crate::debugger::CodeMode::Mixed;
    let mut h = tablet_harness(t, [1024.0, 768.0]);
    h.run_steps(2);
    load(&mut h);
    save(&mut h, "32_tablet_mixed_1024");
}

#[test]
fn source_comes_back_from_a_macro_to_the_next_line() {
    // include.asm: PUTC of emu8086.inc and back, far down the program. The rows drawn are
    // checked (show_rows once placed them by 32 points while they are 26 high).
    let mut t = app_with_text(include_str!("../../../examples/include.asm"));
    t.state.tab = Tab::Debugger;
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(2);
    load(&mut h);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..25 {
        step(&mut h);
        let v = h.state().app.session.current_view(true).unwrap();
        let (file, first, visible) = h.state().state.debug.shown.unwrap();
        assert_eq!(file, v.file);
        assert!((first..first + visible as u32).contains(&v.line), "line {} not drawn (from {first})", v.line);
        seen.insert(v.file);
    }
    assert_eq!(seen.len(), 2, "into the library and back");
}

#[test]
fn virtual_drive_is_in_the_menu() {
    // The browser: drive C: in memory, its files one tap away; the floppies in "Virtual drive".
    let mut t = TabletApp::new(app_with_example(0));
    t.app.front.browser = true;
    t.app.session.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
    let mut h = tablet_harness(t, [1376.0, 1032.0]);
    h.run_steps(2);
    h.get_by_label("file, examples, tools").click();
    h.run_steps(2);
    h.get_by_label("Drive C: files").click();
    h.run_steps(3);
    assert!(h.state().app.windows[Tool::Drive]);
    h.get_by_label("file, examples, tools").click();
    h.run_steps(2);
    h.get_by_label("Virtual drive").hover();
    h.run_steps(3);
    for item in
        ["drive C: files…", "boot from virtual floppy", "write '.bin' file to floppy…", "create new floppy drive"]
    {
        assert!(h.query_by_label(item).is_some(), "{item}");
    }
    save(&mut h, "33_tablet_virtual_drive");
}
