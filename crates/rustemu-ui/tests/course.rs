//! The course in `course/`: every program assembles and runs as its `;!` lines say, and every
//! link and listing in the lessons shows the current text of its program.
//!
//! `;! input: 12\n30\n` — keys to type (`\n` is Enter); `;! output: text` — text the program
//! prints; `;! screen: text` — text on the screen at the end; `;! endless` — the program does not
//! end (it runs for a while, then its output is checked).
//!
//! A lesson links to a program with `(…/RustEmu8086/#name=file.asm…)` and shows its text in the
//! ```asm block after `<!-- program: file.asm -->`. `COURSE_UPDATE_LINKS=1 cargo test -p rustemu-ui
//! --test course` rewrites both from `course/programs/` (the template `_*.md` is skipped).
//!
//! The course is kept outside the repository; without `course/` the tests are skipped.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rustemu_core::Event;
use rustemu_session::Session;
use rustemu_ui::app::WEB_URL;

/// The course directory, or `None` (with a note) when it is not there.
fn course() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../course");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("no course/ directory, skipping");
        None
    }
}

fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    v.sort();
    v
}

/// Values of the `;! key: value` lines (`;! key` alone gives "").
fn tags<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix(";!"))
        .filter_map(|l| {
            let (k, v) = l.split_once(':').unwrap_or((l, ""));
            (k.trim() == key).then(|| v.trim())
        })
        .collect()
}

/// Text of the 80x25 screen.
fn screen(s: &Session) -> String {
    (0..80 * 25u32).map(|i| s.machine.sys.mem.read8(0xB8000 + i * 2) as char).collect()
}

fn check(path: &Path) -> Vec<String> {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let text = std::fs::read_to_string(path).unwrap();
    let sandbox = std::env::temp_dir().join(format!("rustemu_course_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&sandbox).unwrap();
    let mut s = Session::new();
    if let Err(e) = s.load_path(path) {
        return vec![format!("{name}: {e}")];
    }
    s.machine.set_dos_root(sandbox.clone());
    s.machine.instant_waits = true;
    s.machine.history_enabled = false;
    s.machine.instructions_per_tick = Some(2000);
    let input: String = tags(&text, "input").concat().replace("\\n", "\r");
    let mut keys: Vec<u8> = input.into_bytes();
    let endless = !tags(&text, "endless").is_empty();
    let mut ev = Event::Budget;
    for _ in 0..100 {
        ev = s.machine.run(1_000_000);
        match ev {
            Event::WaitingInput if !keys.is_empty() => s.machine.push_char(keys.remove(0)),
            Event::Sleeping | Event::Continue => {}
            Event::Budget if !endless => {}
            _ => break,
        }
    }
    std::fs::remove_dir_all(&sandbox).ok();

    let mut problems = Vec::new();
    let expected = if endless { Event::Budget } else { Event::Terminated(0) };
    if ev != expected {
        problems.push(format!("{name}: ended with {ev:?}, expected {expected:?}"));
    }
    if !keys.is_empty() {
        problems.push(format!("{name}: input not read: {:?}", String::from_utf8_lossy(&keys)));
    }
    problems.extend(s.machine.log.iter().map(|l| format!("{name}: {l}")));
    let output = String::from_utf8_lossy(&s.machine.output).into_owned();
    for want in tags(&text, "output") {
        if !output.contains(want) {
            problems.push(format!("{name}: no {want:?} in the output {output:?}"));
        }
    }
    let screen = screen(&s);
    for want in tags(&text, "screen") {
        if !screen.contains(want) {
            problems.push(format!("{name}: no {want:?} on the screen"));
        }
    }
    problems
}

#[test]
fn course_programs_run_as_described() {
    let Some(course) = course() else { return };
    let problems: Vec<String> = files(&course.join("programs"), "asm").iter().flat_map(|p| check(p)).collect();
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// The name of the program in a link (`…#name=file.asm&code=…`).
fn link_name(link: &str) -> Option<&str> {
    let start = link.find("name=")? + 5;
    let rest = &link[start..];
    Some(&rest[..rest.find('&').unwrap_or(rest.len())])
}

/// Puts the current text of the programs into the ```asm blocks after `<!-- program: name -->`.
fn with_listings(text: &str, programs: &Path, linked: &mut BTreeSet<String>, problems: &mut Vec<String>) -> String {
    const MARK: &str = "<!-- program: ";
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(MARK) {
        let after = i + MARK.len();
        let Some(close) = rest[after..].find("-->") else { break };
        let name = rest[after..after + close].trim();
        let block_start = rest[after..].find("```asm\n").map(|b| after + b + "```asm\n".len());
        let block_end = block_start.and_then(|s| rest[s..].find("```").map(|e| s + e));
        let (Some(s), Some(e)) = (block_start, block_end) else {
            problems.push(format!("no ```asm block after the mark of {name}"));
            break;
        };
        out.push_str(&rest[..s]);
        match std::fs::read_to_string(programs.join(name)) {
            Ok(code) => {
                linked.insert(name.to_string());
                out.push_str(&code);
                if !code.ends_with('\n') {
                    out.push('\n');
                }
            }
            Err(_) => {
                problems.push(format!("a listing of a missing program: {name}"));
                out.push_str(&rest[s..e]);
            }
        }
        rest = &rest[e..];
    }
    out.push_str(rest);
    out
}

#[test]
fn lesson_links_open_the_current_programs() {
    let Some(course) = course() else { return };
    let update = std::env::var_os("COURSE_UPDATE_LINKS").is_some();
    let programs = course.join("programs");
    let prefix = format!("({WEB_URL}#");
    let mut linked = BTreeSet::new();
    let mut problems = Vec::new();
    for lesson in files(&course, "md") {
        let lesson_name = lesson.file_name().unwrap().to_string_lossy().into_owned();
        if lesson_name.starts_with('_') {
            continue;
        }
        let text = std::fs::read_to_string(&lesson).unwrap();
        let mut listing_problems = Vec::new();
        let listed = with_listings(&text, &programs, &mut linked, &mut listing_problems);
        problems.extend(listing_problems.into_iter().map(|p| format!("{lesson_name}: {p}")));
        if listed != text && !update {
            problems.push(format!("{lesson_name}: the text of a program in the lesson is out of date"));
        }
        let mut out = String::new();
        let mut rest = listed.as_str();
        while let Some(i) = rest.find(&prefix) {
            let start = i + prefix.len();
            let end = start + rest[start..].find(')').unwrap_or(rest.len() - start);
            let link = &rest[start..end];
            out.push_str(&rest[..start]);
            let fresh = link_name(link).and_then(|name| {
                let code = std::fs::read_to_string(programs.join(name)).ok()?;
                linked.insert(name.to_string());
                Some(rustemu_ui::share::fragment(name, &code, false))
            });
            match fresh {
                Some(f) if f == link => out.push_str(link),
                Some(f) if update => out.push_str(&f),
                Some(_) => {
                    problems.push(format!("{lesson_name}: the link to {} is out of date", link_name(link).unwrap()));
                    out.push_str(link);
                }
                None => {
                    problems.push(format!("{lesson_name}: a link to a missing program: {link:.40}"));
                    out.push_str(link);
                }
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        if update && out != text {
            std::fs::write(&lesson, out).unwrap();
        }
    }
    for p in files(&programs, "asm") {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !linked.contains(&name) {
            problems.push(format!("{name}: no lesson links to it"));
        }
    }
    assert!(
        problems.is_empty(),
        "\n{}\n\nCOURSE_UPDATE_LINKS=1 cargo test -p rustemu-ui --test course updates the links.",
        problems.join("\n")
    );
}

/// Lesson 19: the kernel goes to the floppy from sector 2, the loader to the boot sector; booting
/// from the floppy runs the loader (with its source) and then the kernel.
#[test]
fn lesson_19_boots_from_the_floppy() {
    let Some(course) = course() else { return };
    let programs = course.join("programs");
    let mut s = Session::new();
    s.load_path(&programs.join("19_kernel.asm")).unwrap();
    let kernel = s.loaded_bytes().unwrap().to_vec();
    assert_eq!(s.write_floppy(0, 0, 0, 2, &kernel), Ok(1));
    s.load_path(&programs.join("19_loader.asm")).unwrap();
    s.write_boot_sector(0).unwrap();
    s.boot_from_floppy(0).unwrap();
    assert!(s.program.is_some(), "the loader keeps its source after booting from the floppy");
    s.machine.run(1_000_000);
    let screen = screen(&s);
    assert!(screen.contains("Loading kernel...") && screen.contains("Kernel started from the floppy!"), "{screen}");
    assert_eq!(s.machine.cpu.cs(), 0x0800);
}
