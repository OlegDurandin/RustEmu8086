use rustemu_build::{Outcome, project::Project};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
};

fn project(mips: bool) -> Project {
    Project {
        version: 1,
        architecture: if mips { "MIPS32" } else { "8086" }.into(),
        entry: "src/main.asm".into(),
        dialect: None,
        mars: Default::default(),
        input: String::new(),
        files: BTreeMap::from([
            (
                "src/main.asm".into(),
                if mips {
                    ".include \"lib/value.inc\"\n.text\nmain: li $a0, VALUE\nli $v0, 10\nsyscall\n"
                } else {
                    "org 100h\ninclude 'lib/value.inc'\nmov ax, VALUE\nret\n"
                }
                .into(),
            ),
            ("src/lib/value.inc".into(), if mips { ".eqv VALUE 42\n" } else { "VALUE equ 42\n" }.into()),
        ]),
    }
}

#[test]
fn package_roundtrip_and_nested_includes_build_without_host() {
    for mips in [false, true] {
        let p = project(mips);
        let p = Project::decode(&p.encode().unwrap()).unwrap();
        let r = rustemu_build::execute(p.request(7), Arc::new(AtomicBool::new(false)));
        assert_eq!(r.sources.len(), 2);
        assert!(matches!(r.outcome, Outcome::X86(Ok(_)) | Outcome::Mips(Ok(_))), "{:?}", r.outcome);
    }
}

#[test]
fn reject_versions_paths_ambiguous_names_missing_entry_and_budgets() {
    let p = project(true);
    let mut bad = p.clone();
    bad.version = 2;
    assert!(bad.encode().is_err());
    for path in ["../secret", "/etc/passwd", "C:\\secret", "a/../b", "a//b", "a\\b", "a\0b"] {
        let mut bad = p.clone();
        bad.files.insert(path.into(), "".into());
        assert!(bad.validate().is_err(), "{path}");
    }
    let mut bad = p.clone();
    bad.files.insert("SRC/MAIN.ASM".into(), "".into());
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.entry = "missing.asm".into();
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.mars.memory = "unknown".into();
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.input = "a".repeat(4097);
    assert!(bad.validate().is_err());
    let mut bad = p;
    bad.files.insert("large".into(), "a".repeat(rustemu_mips::asm::MAX_SOURCE_BYTES));
    assert!(bad.validate().is_err());
    assert!(Project::decode(&vec![b' '; rustemu_build::project::MAX_PACKAGE_BYTES + 1]).is_err());
}

#[test]
fn source_lookup_does_not_substitute_same_basename_in_another_folder() {
    let mut p = project(true);
    let text = p.files.remove("src/lib/value.inc").unwrap();
    p.files.insert("other/value.inc".into(), text);
    let r = rustemu_build::execute(p.request(0), Arc::new(AtomicBool::new(false)));
    assert!(matches!(r.outcome, Outcome::Mips(Err(_))));
}

#[test]
fn project_mode_and_exception_handler_work_from_memory() {
    let mut p = project(true);
    p.mars.assemble_all = true;
    p.mars.exception_handler_enabled = true;
    p.mars.exception_handler = "handler.s".into();
    p.files.insert("handler.s".into(), ".ktext 0x80000180\neret\n".into());
    p.files.insert("src/helper.s".into(), ".text\n.globl helper\nhelper: jr $ra\nnop\n".into());
    let r = rustemu_build::execute(p.request(0), Arc::new(AtomicBool::new(false)));
    assert!(matches!(r.outcome, Outcome::Mips(Ok(_))), "{:?}", r.outcome);
    assert_eq!(r.sources.len(), 4);
}

#[test]
fn native_export_collects_used_dependencies_and_rebuilds_after_the_folder_is_gone() {
    let dir = std::env::temp_dir().join(format!("remu-package-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let text = "org 100h\ninclude 'lib/value.inc'\nmov ax, VALUE\nret\n";
    std::fs::write(dir.join("lib/value.inc"), "VALUE equ 42\n").unwrap();
    std::fs::write(dir.join("private.txt"), "must not be included").unwrap();
    let mut r = project(false).request(0);
    r.name = dir.join("main.asm").to_str().unwrap().into();
    r.text = text.into();
    r.files.clear();
    r.host = true;
    r.portable = false;
    let p = Project::capture(r, Default::default(), "yes\n".into()).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(p.entry, "main.asm");
    assert_eq!(p.files.len(), 2);
    assert!(p.files.contains_key("lib/value.inc"));
    assert!(!p.encode().unwrap().windows(7).any(|w| w == b"private"));
    let r = rustemu_build::execute(p.request(0), Arc::new(AtomicBool::new(false)));
    assert!(matches!(r.outcome, Outcome::X86(Ok(_))));
}
