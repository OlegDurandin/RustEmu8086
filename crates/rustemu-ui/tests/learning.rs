use rustemu_build::{exercise::MemoryCheck, project::Project};
use rustemu_ui::learning::{CheckRun, EXAMPLES};
use std::sync::{Arc, atomic::AtomicBool};

fn check(p: Project) -> Vec<rustemu_ui::learning::CaseResult> {
    let response = rustemu_build::execute(p.request(0), Arc::new(AtomicBool::new(false)));
    let mut run = CheckRun::new(p, response.outcome).unwrap();
    for _ in 0..10000 {
        run.advance();
        if run.done() {
            return run.results;
        }
    }
    panic!("checker never finished");
}

fn solved(i: usize) -> Project {
    let mut p = Project::decode(EXAMPLES[i].1.as_bytes()).unwrap();
    let id = &p.exercise.as_ref().unwrap().id;
    let ext = if p.architecture == "8086" { "asm" } else { "s" };
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../tests/learning-solutions/{id}.{ext}"));
    let solution = std::fs::read_to_string(path).unwrap();
    p.files.insert(p.entry.clone(), solution);
    p
}

#[test]
fn catalog_reference_solutions_pass_and_wrong_solutions_fail_the_right_conditions() {
    for i in 0..EXAMPLES.len() {
        let p = solved(i);
        let results = check(p.clone());
        assert!(results.iter().all(|r| r.passed), "{i}: {results:?}");
        let mut wrong = p;
        let code = if wrong.architecture == "8086" { "org 100h\nhlt\n" } else { ".text\nmain: li $v0,10\nsyscall\n" };
        wrong.files.insert(wrong.entry.clone(), code.into());
        let results = check(wrong);
        assert!(results.iter().any(|r| !r.passed), "wrong solution must not pass the whole suite: {i}: {results:?}");
        assert!(
            results.iter().filter(|r| !r.passed).all(|r| r.details.contains("Register")
                || r.details.contains("Memory")
                || r.details.contains("Output")),
            "{results:?}"
        );
    }
}

#[test]
fn legacy_v1_and_exercise_projects_roundtrip_with_settings() {
    let old = Project::decode(include_bytes!("../../../examples/projects/mips-input.remu")).unwrap();
    assert!(old.exercise.is_none());
    assert!(!String::from_utf8(old.encode().unwrap()).unwrap().contains("exercise"));
    for i in 0..EXAMPLES.len() {
        let p = solved(i);
        assert_eq!(Project::decode(&p.encode().unwrap()).unwrap(), p);
    }
}

#[test]
fn limits_and_unsupported_services_are_explicit_failures() {
    let mut looped = solved(0);
    looped.exercise.as_mut().unwrap().step_limit = 20;
    looped.files.insert(looped.entry.clone(), "org 100h\nagain: jmp again\n".into());
    assert!(check(looped).iter().all(|r| !r.passed && r.details.contains("Step limit")));
    let mut p = solved(2);
    p.files.insert(p.entry.clone(), ".text\nmain: li $v0,30\nsyscall\n".into());
    assert!(check(p).iter().all(|r| !r.passed && r.details.contains("Unsupported")));
    let mut p = solved(0);
    p.files.insert(p.entry.clone(), "org 100h\nint 1ah\nhlt\n".into());
    assert!(check(p).iter().all(|r| !r.passed && r.details.contains("Unsupported")));
}

#[test]
fn imported_budgets_and_memory_addresses_are_validated_before_execution() {
    let p = solved(0);
    let mut bad = p.clone();
    bad.exercise.as_mut().unwrap().step_limit = 200001;
    assert!(bad.encode().is_err());
    let mut bad = p.clone();
    bad.exercise.as_mut().unwrap().cases[0].expected.memory.push(MemoryCheck { address: u32::MAX, bytes: vec![1, 2] });
    assert!(bad.encode().is_err());
    let mut bad = p.clone();
    bad.exercise.as_mut().unwrap().cases[0].expected.memory.push(MemoryCheck { address: 0x100000, bytes: vec![1] });
    assert!(bad.encode().is_err());
    let mut bad = p.clone();
    bad.exercise.as_mut().unwrap().cases[0].input = "a".repeat(4097);
    assert!(bad.encode().is_err());
    let mut bad = p;
    bad.exercise.as_mut().unwrap().version = 99;
    assert!(bad.encode().is_err());
}

#[test]
fn case_timeout_and_input_exhaustion_never_pass() {
    let mut p = solved(0);
    p.exercise.as_mut().unwrap().time_limit_ms = 1;
    let response = rustemu_build::execute(p.request(0), Arc::new(AtomicBool::new(false)));
    let mut run = CheckRun::new(p, response.outcome).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    run.advance();
    assert!(!run.results[0].passed && run.results[0].details.contains("Timed out"));
    let mut p = solved(2);
    p.exercise.as_mut().unwrap().cases[0].input.clear();
    let r = check(p);
    assert!(!r[0].passed && r[0].details.contains("Input exhausted"));
}

#[test]
fn output_and_memory_failures_are_distinct_from_termination() {
    let mut p = solved(0);
    p.exercise.as_mut().unwrap().cases[0].expected.output = Some("missing".into());
    let r = check(p);
    assert!(!r[0].passed && r[0].details.contains("Output:"));
    let mut p = solved(3);
    p.exercise.as_mut().unwrap().cases[0].expected.memory[0].bytes[0] = 99;
    let r = check(p);
    assert!(!r[0].passed && r[0].details.contains("Memory"));
}

#[test]
fn grading_does_not_mutate_open_session_editor_or_personal_drive() {
    let mut app =
        rustemu_ui::app::App::with_settings(rustemu_ui::app::Settings { last_file: None, ..Default::default() });
    app.doc.text = "org 100h\nmov ax,42\nhlt\n; unsaved".into();
    app.emulate();
    app.single_step();
    app.session.set_dos_memory_drive(Default::default());
    app.session.dos_memory_drive_mut().unwrap().write("personal.txt", b"keep".to_vec()).unwrap();
    let text = app.doc.text.clone();
    let pc = app.session.target.pc();
    let cpu = app.session.x86().unwrap().cpu.clone();
    for i in 0..EXAMPLES.len() {
        assert!(check(solved(i)).iter().all(|r| r.passed));
    }
    assert_eq!(app.doc.text, text);
    assert_eq!(app.session.target.pc(), pc);
    assert_eq!(app.session.x86().unwrap().cpu, cpu);
    assert_eq!(app.session.dos_memory_drive().unwrap().read("personal.txt").unwrap(), b"keep");
}

#[test]
fn pilot_catalog_inventory_determinism_and_negative_cases() {
    let mut ids = std::collections::BTreeSet::new();
    let mut counts = [0, 0];
    assert_eq!(EXAMPLES.len(), 12);
    for i in 0..EXAMPLES.len() {
        let p = solved(i);
        let e = p.exercise.as_ref().unwrap();
        assert!(ids.insert(e.id.clone()));
        assert!(e.cases.len() >= 2);
        counts[usize::from(p.architecture == "MIPS32")] += 1;
        let a = check(p.clone());
        let b = check(p.clone());
        assert_eq!(
            a.iter().map(|r| (&r.name, r.passed, &r.details)).collect::<Vec<_>>(),
            b.iter().map(|r| (&r.name, r.passed, &r.details)).collect::<Vec<_>>()
        );
        let mut looped = p.clone();
        looped.exercise.as_mut().unwrap().step_limit = 40;
        looped.files.insert(
            looped.entry.clone(),
            if p.architecture == "8086" { "org 100h\nagain: jmp again\n" } else { ".text\nmain: b main\nnop\n" }.into(),
        );
        assert!(check(looped).iter().all(|r| !r.passed && r.details.contains("Step limit")));
        let mut invalid = p;
        invalid.files.insert(invalid.entry.clone(), "invalid_instruction".into());
        let response = rustemu_build::execute(invalid.request(0), Arc::new(AtomicBool::new(false)));
        assert!(CheckRun::new(invalid, response.outcome).err().unwrap().contains("Assembly failed"));
    }
    assert_eq!(counts, [6, 6]);
    assert!(Project::decode(b"{truncated").is_err());
}
