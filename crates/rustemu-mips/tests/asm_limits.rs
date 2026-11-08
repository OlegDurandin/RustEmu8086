//! Resource budgets apply to the entire assembly, not just macro invocations.

use rustemu_mips::Settings;
use rustemu_mips::asm::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn file(name: &str, text: String) -> SourceFile {
    SourceFile { name: name.into(), text }
}
fn comments(bytes: usize) -> String {
    let block = format!("#{}\n", " ".repeat(MAX_LINE_BYTES - 2));
    assert_eq!(bytes % block.len(), 0);
    block.repeat(bytes / block.len())
}

#[test]
fn ordinary_source_has_byte_line_and_token_limits() {
    assert!(assemble(&comments(MAX_SOURCE_BYTES)).is_ok());
    let e = assemble(&(comments(MAX_SOURCE_BYTES) + "\n")).unwrap_err();
    assert!(e[0].message.contains("bytes in total"));
    assert!(assemble(&"#\n".repeat(MAX_EXPANDED_LINES)).is_ok());
    let e = assemble(&"#\n".repeat(MAX_EXPANDED_LINES + 1)).unwrap_err();
    assert_eq!(e[0].line, MAX_EXPANDED_LINES + 1);
    assert!(e[0].message.contains("250000 lines"));
    let e = assemble(&format!("#{}", "x".repeat(MAX_LINE_BYTES))).unwrap_err();
    assert!(e[0].message.contains("line exceeds"));
    let e = assemble(&format!(".data\n.byte {}", "1,".repeat(MAX_LINE_TOKENS))).unwrap_err();
    assert!(e[0].message.contains("tokens"));
}

#[test]
fn project_files_share_the_budget_with_the_exception_handler() {
    let e = assemble_files(
        file("main.s", comments(MAX_SOURCE_BYTES / 2)),
        Some(file("handler.s", comments(MAX_SOURCE_BYTES / 4))),
        vec![file("library.s", comments(MAX_SOURCE_BYTES / 2))],
        &mut |_| panic!("no include should be read"),
        &Settings::default(),
    )
    .unwrap_err();
    assert_eq!(e.files[e.errors[0].file], "library.s");
    assert!(e.errors[0].message.contains("bytes in total"));
}

#[test]
fn includes_share_line_and_byte_budgets_and_stop_loading() {
    let main = "#\n".repeat(MAX_EXPANDED_LINES / 2) + ".include \"child.s\"\n";
    let mut reads = 0;
    let e = assemble_with("main.s", &main, &mut |_| {
        reads += 1;
        Some("#\n".repeat(MAX_EXPANDED_LINES / 2))
    })
    .unwrap_err();
    assert_eq!(reads, 1);
    assert_eq!((e.errors[0].file, e.errors[0].line), (0, MAX_EXPANDED_LINES / 2 + 1));
    assert!(e.errors[0].message.contains("250000 lines"));
    assert_eq!(e.files, ["main.s"], "rejected include text is not retained");
    let main = comments(MAX_SOURCE_BYTES / 2) + ".include \"child.s\"\n";
    let e = assemble_with("main.s", &main, &mut |_| Some(comments(MAX_SOURCE_BYTES / 2))).unwrap_err();
    assert!(e.errors[0].message.contains("bytes in total"));
    let main: String = (0..MAX_SOURCE_FILES).map(|i| format!(".include \"f{i}.s\"\n")).collect();
    let mut reads = 0;
    let e = assemble_with("main.s", &main, &mut |_| {
        reads += 1;
        Some(String::new())
    })
    .unwrap_err();
    assert_eq!(reads, MAX_SOURCE_FILES - 1);
    assert_eq!(e.files.len(), MAX_SOURCE_FILES);
    assert!(e.errors[0].message.contains("source files"));
}

#[test]
fn macro_lines_share_the_budget_with_ordinary_source() {
    let mut source = "#\n".repeat(MAX_EXPANDED_LINES - 1000);
    source += ".macro m\n";
    source += &"#\n".repeat(10);
    source += ".end_macro\n";
    source += &"m\n".repeat(200);
    let e = assemble(&source).unwrap_err();
    assert!(e.iter().any(|e| e.message.contains("macro expansions would make more than")), "{e:?}");
    assert!(e.len() <= MAX_DIAGNOSTICS);
}

#[test]
fn substitution_is_checked_before_it_grows() {
    let mut source = String::from(".eqv n0 1\n");
    for i in 1..20 {
        source += &format!(".eqv n{i} n{},n{}\n", i - 1, i - 1);
    }
    let e = assemble(&source).unwrap_err();
    assert!(e.iter().any(|e| e.message.contains(".eqv expansion")), "{e:?}");
    let source =
        format!(".macro m (%x)\n.ascii %x %x\n.end_macro\n.data\nm \"{}\"\n", "a".repeat(MAX_LINE_BYTES * 3 / 4));
    let e = assemble(&source).unwrap_err();
    assert!(e[0].message.contains("macro substitution exceeds"));
    assert!(!e[0].expansion.is_empty(), "the call chain is retained");
}

#[test]
fn aggregate_token_and_expanded_text_budgets_are_enforced() {
    // Definition bodies are tokenized but need no instruction/data output allocations.
    let body = format!("{}\n", "x ".repeat(MAX_LINE_TOKENS));
    let source = format!(".macro m\n{}.end_macro\n", body.repeat(MAX_PROCESSED_TOKENS / MAX_LINE_TOKENS + 1));
    let e = assemble(&source).unwrap_err();
    assert!(e[0].message.contains("processed token budget"), "{e:?}");
    // A large .eqv argument used by a macro can generate lots of text from a small source.
    let source = format!(
        ".eqv big {}\n.macro m (%x)\n.globl %x\n.end_macro\n{}",
        "x".repeat(MAX_LINE_BYTES / 2),
        "m big\n".repeat(600)
    );
    let e = assemble(&source).unwrap_err();
    assert!(e.iter().any(|e| e.message.contains("text budget") || e.message.contains("token/text budget")), "{e:?}");
}

#[test]
fn oversized_source_paths_and_project_inventories_are_rejected() {
    let e = assemble_with("main.s", &format!(".include \"{}\"", "x".repeat(MAX_SOURCE_PATH_BYTES + 1)), &mut |_| {
        panic!("oversized paths are rejected before loading")
    })
    .unwrap_err();
    assert!(e.errors[0].message.contains("source path"));
    let e = assemble_files(
        file("main.s", String::new()),
        None,
        (0..MAX_SOURCE_FILES).map(|i| file(&format!("f{i}.s"), String::new())).collect(),
        &mut |_| None,
        &Settings::default(),
    )
    .unwrap_err();
    assert!(e.errors[0].message.contains("source files"));
    assert_eq!(e.files.len(), 1);
}

#[test]
fn diagnostics_are_bounded_and_report_truncation() {
    let e = assemble(&"unknown_instruction\n".repeat(1000)).unwrap_err();
    assert_eq!(e.len(), MAX_DIAGNOSTICS);
    assert!(e.iter().any(|e| e.message.contains("further messages omitted")));
    let e = assemble(&(".data\n".to_string() + &".byte 256\n".repeat(1000))).unwrap_err();
    assert!(e.iter().any(|e| e.message.contains("diagnostic limit")));
}

#[test]
fn cancellation_stops_before_reading_or_retaining_an_include() {
    let flag = Arc::new(AtomicBool::new(true));
    let e = assemble_files_cancellable(
        file("main.s", ".include \"child.s\"".into()),
        None,
        vec![],
        &mut |_| panic!("cancelled before loading"),
        &Settings::default(),
        flag.clone(),
    )
    .unwrap_err();
    assert!(e.errors[0].message.contains("cancelled"));
    flag.store(false, Ordering::Relaxed);
    let e = assemble_files_cancellable(
        file("main.s", ".include \"child.s\"".into()),
        None,
        vec![],
        &mut |_| {
            flag.store(true, Ordering::Relaxed);
            Some("nop".into())
        },
        &Settings::default(),
        flag.clone(),
    )
    .unwrap_err();
    assert!(e.errors[0].message.contains("cancelled"));
    assert_eq!(e.files, ["main.s"]);
}

#[test]
fn host_source_reads_are_bounded_before_assembly() {
    let path = std::env::temp_dir().join(format!("remu_asm_limits_{}.s", std::process::id()));
    let f = std::fs::File::create(&path).unwrap();
    f.set_len((MAX_SOURCE_BYTES * 2) as u64).unwrap();
    drop(f);
    let text = read_source_file(&path).unwrap();
    assert_eq!(text.len(), MAX_SOURCE_BYTES + 1);
    assert!(assemble(&text).unwrap_err()[0].message.contains("bytes in total"));
    std::fs::remove_file(path).unwrap();
}
