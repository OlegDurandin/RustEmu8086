//! Runs the SingleStepTests/8088 v2 suite (https://github.com/SingleStepTests/8088).
//!
//! The data is not part of the repository; download `v2/*.json.gz` and `v2/metadata.json` into
//! `testdata/8088/` and run `cargo test -p rustemu-core --release --test singlestep -- --ignored`.
//! Set `SST_FILTER=F6` to run a subset of files.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use rustemu_core::bus::{Bus, Memory, linear};
use rustemu_core::cpu::*;
use serde_json::Value;

struct TestBus {
    mem: Memory,
}

impl Bus for TestBus {
    fn read8(&mut self, a: u32) -> u8 {
        self.mem.read8(a)
    }
    fn write8(&mut self, a: u32, v: u8) {
        self.mem.write8(a, v)
    }
    fn port_in8(&mut self, _p: u16) -> u8 {
        0xFF
    }
    fn port_out8(&mut self, _p: u16, _v: u8) {}
}

const REGS: [(&str, usize); 8] =
    [("ax", AX), ("bx", BX), ("cx", CX), ("dx", DX), ("sp", SP), ("bp", BP), ("si", SI), ("di", DI)];
const SREGS: [(&str, usize); 4] = [("cs", CS), ("ss", SS), ("ds", DS), ("es", ES)];

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/8088")
}

/// Returns the undefined-flags mask for a test file name like `F6.6` or `00`.
fn flags_mask(meta: &Value, name: &str) -> Option<u16> {
    let mut parts = name.split('.');
    let op = parts.next()?;
    let mut entry = &meta["opcodes"][op];
    if let Some(reg) = parts.next() {
        entry = &entry["reg"][reg];
    }
    match entry["status"].as_str() {
        Some("undefined") | Some("prefix") | Some("fpu") => None,
        _ => Some(entry["flags-mask"].as_u64().unwrap_or(0xFFFF) as u16),
    }
}

fn run_file(path: &PathBuf, mask: u16) -> (usize, usize, Vec<String>) {
    let mut s = String::new();
    GzDecoder::new(std::fs::File::open(path).unwrap()).read_to_string(&mut s).unwrap();
    let tests: Vec<Value> = serde_json::from_str(&s).unwrap();
    let mut bus = TestBus { mem: Memory::new() };
    let mut fails = Vec::new();
    for t in &tests {
        let init = &t["initial"];
        let mut cpu = Cpu::new();
        for (n, i) in REGS {
            cpu.regs[i] = init["regs"][n].as_u64().unwrap() as u16;
        }
        for (n, i) in SREGS {
            cpu.sregs[i] = init["regs"][n].as_u64().unwrap() as u16;
        }
        cpu.ip = init["regs"]["ip"].as_u64().unwrap() as u16;
        cpu.flags = init["regs"]["flags"].as_u64().unwrap() as u16;
        for e in init["ram"].as_array().unwrap() {
            bus.mem.write8(e[0].as_u64().unwrap() as u32, e[1].as_u64().unwrap() as u8);
        }
        cpu.step(&mut bus);

        let fin = &t["final"];
        let mut errs = Vec::new();
        let freg = fin["regs"].as_object().unwrap();
        let get = |n: &str, cur: u16| freg.get(n).map(|v| v.as_u64().unwrap() as u16).unwrap_or(cur);
        let init_reg = |n: &str| init["regs"][n].as_u64().unwrap() as u16;
        for (n, i) in REGS {
            let want = get(n, init_reg(n));
            if cpu.regs[i] != want {
                errs.push(format!("{n}={:04X} want {want:04X}", cpu.regs[i]));
            }
        }
        for (n, i) in SREGS {
            let want = get(n, init_reg(n));
            if cpu.sregs[i] != want {
                errs.push(format!("{n}={:04X} want {want:04X}", cpu.sregs[i]));
            }
        }
        let want_ip = get("ip", init_reg("ip"));
        if cpu.ip != want_ip {
            errs.push(format!("ip={:04X} want {want_ip:04X}", cpu.ip));
        }
        let want_f = get("flags", init_reg("flags"));
        if (cpu.flags ^ want_f) & mask != 0 {
            errs.push(format!("flags={:04X} want {want_f:04X} (mask {mask:04X})", cpu.flags));
        }
        // An exception (e.g. divide error) pushes FLAGS, whose undefined bits are masked too.
        let mut byte_mask: HashMap<u32, u8> = HashMap::new();
        if want_ip != init_reg("ip").wrapping_add(t["bytes"].as_array().unwrap().len() as u16) {
            let ss = get("ss", init_reg("ss"));
            let sp = get("sp", init_reg("sp"));
            let fa = linear(ss, sp.wrapping_add(4));
            byte_mask.insert(fa, mask as u8);
            byte_mask.insert(linear(ss, sp.wrapping_add(5)), (mask >> 8) as u8);
        }
        for e in fin["ram"].as_array().unwrap() {
            let a = e[0].as_u64().unwrap() as u32;
            let v = e[1].as_u64().unwrap() as u8;
            let got = bus.mem.read8(a);
            let m = byte_mask.get(&a).copied().unwrap_or(0xFF);
            if (got ^ v) & m != 0 {
                errs.push(format!("mem[{a:05X}]={got:02X} want {v:02X}"));
            }
        }
        if !errs.is_empty() {
            fails.push(format!("{} {:?}: {}", t["name"], t["bytes"], errs.join(", ")));
        }
    }
    (tests.len(), fails.len(), fails)
}

#[test]
#[ignore]
fn singlestep_8088() {
    let dir = data_dir();
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).expect("metadata.json")).unwrap();
    let filter = std::env::var("SST_FILTER").ok();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".json.gz"))
        .collect();
    files.sort();
    let (mut total, mut failed) = (0, 0);
    let mut bad_files = Vec::new();
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().trim_end_matches(".json.gz").to_string();
        if let Some(flt) = &filter
            && !name.starts_with(flt.as_str())
        {
            continue;
        }
        let Some(mask) = flags_mask(&meta, &name) else { continue };
        let (n, nf, fails) = run_file(&f, mask);
        total += n;
        failed += nf;
        if nf > 0 {
            println!("{name}: {nf}/{n} failed; e.g. {}", fails[0]);
            bad_files.push(name);
        }
    }
    println!("total {total}, failed {failed}; bad files: {bad_files:?}");
    assert_eq!(failed, 0);
}
