//! The same owned protocol is used by native tasks and browser workers.
use rustemu_build::{Outcome, Request, execute};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
};

fn request(mips: bool) -> Request {
    Request {
        version: 42,
        mips,
        name: "main.asm".into(),
        text: if mips { ".text\nmain: li $v0, 10\nsyscall\n" } else { "org 100h\nmov ax, 4c00h\nint 21h\n" }.into(),
        dialect: None,
        mars: Default::default(),
        files: BTreeMap::new(),
        roots: vec![],
        portable: false,
        host: false,
        exception_handler: None,
        project: false,
    }
}

#[test]
fn both_results_survive_worker_wire_roundtrip() {
    for mips in [false, true] {
        let r = request(mips);
        let r = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        let response = execute(r, Arc::new(AtomicBool::new(false)));
        let response: rustemu_build::Response =
            serde_json::from_str(&rustemu_build::serialize_response(&response).unwrap()).unwrap();
        assert_eq!(response.version, 42);
        assert_eq!(response.sources.len(), 1);
        assert!(matches!(response.outcome, Outcome::X86(Ok(_)) | Outcome::Mips(Ok(_))));
    }
}

#[test]
fn cancellation_and_source_limits_return_controlled_results() {
    for mips in [false, true] {
        let response = execute(request(mips), Arc::new(AtomicBool::new(true)));
        assert!(matches!(response.outcome, Outcome::Failed(e) if e.contains("cancelled")));
        let mut r = request(mips);
        r.text = " ".repeat(rustemu_mips::asm::MAX_SOURCE_BYTES + 1);
        let response = execute(r, Arc::new(AtomicBool::new(false)));
        assert!(matches!(response.outcome, Outcome::Failed(e) if e.contains("budget")));
    }
}

#[test]
fn both_assemblers_bound_diagnostics() {
    for mips in [false, true] {
        let mut r = request(mips);
        r.text = "invalid instruction\n".repeat(10_000);
        let response = execute(r, Arc::new(AtomicBool::new(false)));
        let count = match response.outcome {
            Outcome::X86(Err(e)) => e.errors.len(),
            Outcome::Mips(Err(e)) => e.errors.len(),
            other => panic!("expected diagnostics: {other:?}"),
        };
        assert!((1..=100).contains(&count), "{count}");
    }
}

#[test]
fn response_transfer_bounds_escaped_text_before_retaining_oversized_json() {
    let mut response = execute(request(false), Arc::new(AtomicBool::new(false)));
    // Each NUL becomes six JSON bytes: the typed payload is small enough but the wire is not.
    response.outcome = Outcome::Failed("\0".repeat(12 << 20));
    let error = rustemu_build::serialize_response(&response).unwrap_err();
    assert!(error.contains("BuildResultBytes limit exceeded"), "{error}");
}
