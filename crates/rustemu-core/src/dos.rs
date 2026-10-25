//! DOS services (INT 21h) with file access confined to a host directory.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::bus::linear;
use crate::cpu::{AX, BX, CX, DI, DS, DX, ES, SI, flags};
use crate::machine::{Machine, Svc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    /// 0 = Sunday
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub hundredths: u8,
}

#[derive(Default)]
pub(crate) struct DosState {
    pub root: Option<PathBuf>,
    /// Current directory inside the root, using `/` separators, no leading slash.
    pub cwd: String,
    pub files: HashMap<u16, File>,
    pub psp: u16,
    pub dta: (u16, u16),
    pub waiting: bool,
    /// Pending AH=0Ah line input.
    pub line: Option<Vec<u8>>,
    pub last_exit: u8,
    /// Memory allocation: next free paragraph for AH=48h.
    pub next_free: u16,
    pub find: Option<(Vec<String>, usize)>,
}

const E_FILE_NOT_FOUND: u16 = 2;
const E_PATH_NOT_FOUND: u16 = 3;
const E_TOO_MANY: u16 = 4;
const E_ACCESS: u16 = 5;
const E_HANDLE: u16 = 6;
const E_NO_MEM: u16 = 8;
const E_INVALID: u16 = 1;
const E_NO_MORE: u16 = 18;

impl Machine {
    pub(crate) fn now(&self) -> DateTime {
        if let Some(t) = self.fixed_time {
            return t;
        }
        host_now()
    }

    fn al(&self) -> u8 {
        self.cpu.regs[AX] as u8
    }

    fn set_al(&mut self, v: u8) {
        self.cpu.regs[AX] = (self.cpu.regs[AX] & 0xFF00) | v as u16;
    }

    fn ok(&mut self) {
        self.cpu.set_flag(flags::CF, false);
    }

    fn fail(&mut self, code: u16) {
        self.cpu.regs[AX] = code;
        self.cpu.set_flag(flags::CF, true);
    }

    fn read_key(&mut self) -> Option<u16> {
        self.sys.keys.pop_front()
    }

    pub(crate) fn int21(&mut self) -> Svc {
        let ah = (self.cpu.regs[AX] >> 8) as u8;
        let dl = self.cpu.regs[DX] as u8;
        match ah {
            0x00 => return Svc::Terminate(0),
            0x01 => match self.read_key() {
                Some(k) => {
                    let c = k as u8;
                    self.console_out(c);
                    self.set_al(c);
                }
                None => return Svc::Block,
            },
            0x02 => {
                self.console_out(dl);
                self.set_al(dl);
            }
            0x05 => self.sys.dev.printer.push(dl),
            0x06 => {
                if dl == 0xFF {
                    match self.read_key() {
                        Some(k) => {
                            self.set_al(k as u8);
                            self.cpu.set_flag(flags::ZF, false);
                        }
                        None => {
                            self.set_al(0);
                            self.cpu.set_flag(flags::ZF, true);
                        }
                    }
                } else {
                    self.console_out(dl);
                    self.set_al(dl);
                }
            }
            0x07 | 0x08 => match self.read_key() {
                Some(k) => self.set_al(k as u8),
                None => return Svc::Block,
            },
            0x09 => {
                let s = self.read_string(self.cpu.sregs[DS], self.cpu.regs[DX], b'$', 0x10000);
                for c in s {
                    self.console_out(c);
                }
                self.set_al(b'$');
            }
            0x0A => return self.buffered_input(),
            0x0B => {
                let v = if self.sys.keys.is_empty() { 0 } else { 0xFF };
                self.set_al(v);
            }
            0x0C => {
                if self.dos.line.is_none() && !self.dos.waiting {
                    self.sys.keys.clear();
                }
                let f = self.al();
                if matches!(f, 0x01 | 0x06 | 0x07 | 0x08 | 0x0A) {
                    self.cpu.regs[AX] = ((f as u16) << 8) | (self.cpu.regs[AX] & 0xFF);
                    let r = self.int21();
                    if !matches!(r, Svc::Block) {
                        self.cpu.regs[AX] = (0x0C << 8) | (self.cpu.regs[AX] & 0xFF);
                    } else {
                        self.cpu.regs[AX] = (0x0C << 8) | f as u16;
                    }
                    return r;
                }
            }
            0x0D => {}
            0x0E => self.set_al(26),
            0x19 => self.set_al(2),
            0x1A => self.dos.dta = (self.cpu.sregs[DS], self.cpu.regs[DX]),
            0x25 => {
                let v = self.al() as u32 * 4;
                self.sys.mem.write16(v, self.cpu.regs[DX]);
                self.sys.mem.write16(v + 2, self.cpu.sregs[DS]);
            }
            0x2A => {
                let t = self.now();
                self.cpu.regs[CX] = t.year;
                self.cpu.regs[DX] = ((t.month as u16) << 8) | t.day as u16;
                self.set_al(t.weekday);
            }
            0x2B | 0x2D => self.set_al(0),
            0x2C => {
                let t = self.now();
                self.cpu.regs[CX] = ((t.hour as u16) << 8) | t.minute as u16;
                self.cpu.regs[DX] = ((t.second as u16) << 8) | t.hundredths as u16;
            }
            0x2F => {
                self.cpu.sregs[ES] = self.dos.dta.0;
                self.cpu.regs[BX] = self.dos.dta.1;
            }
            0x30 => {
                self.cpu.regs[AX] = 0x0005;
                self.cpu.regs[BX] = 0xFF00;
                self.cpu.regs[CX] = 0;
            }
            0x31 => return Svc::Terminate(self.al()),
            0x33 => self.cpu.regs[DX] &= 0xFF00,
            0x35 => {
                let v = self.al() as u32 * 4;
                self.cpu.regs[BX] = self.sys.mem.read16(v);
                self.cpu.sregs[ES] = self.sys.mem.read16(v + 2);
            }
            0x36 => {
                self.cpu.regs[AX] = 1;
                self.cpu.regs[BX] = 0x7FFF;
                self.cpu.regs[CX] = 512;
                self.cpu.regs[DX] = 0xFFFF;
            }
            0x39..=0x3B | 0x41 | 0x43 | 0x56 => self.dos_path_op(ah),
            0x3C | 0x3D | 0x5B => self.dos_open(ah),
            0x3E => match self.dos.files.remove(&self.cpu.regs[BX]) {
                Some(_) => self.ok(),
                None if self.cpu.regs[BX] < 5 => self.ok(),
                None => self.fail(E_HANDLE),
            },
            0x3F => return self.dos_read(),
            0x40 => self.dos_write(),
            0x42 => self.dos_seek(),
            0x44 => {
                // IOCTL get device info: handles 0-4 are character devices.
                if self.al() == 0 {
                    let h = self.cpu.regs[BX];
                    if h < 5 {
                        self.cpu.regs[DX] = 0x80D3;
                        self.ok();
                    } else if self.dos.files.contains_key(&h) {
                        self.cpu.regs[DX] = 0x0002;
                        self.ok();
                    } else {
                        self.fail(E_HANDLE);
                    }
                } else {
                    self.fail(E_INVALID);
                }
            }
            0x47 => {
                let s = self.dos.cwd.replace('/', "\\").to_ascii_uppercase();
                let (ds, si) = (self.cpu.sregs[DS], self.cpu.regs[SI]);
                for (i, b) in s.bytes().chain(std::iter::once(0)).enumerate() {
                    self.sys.mem.write8(linear(ds, si.wrapping_add(i as u16)), b);
                }
                self.ok();
            }
            0x48 => {
                let want = self.cpu.regs[BX];
                if self.dos.next_free == 0 {
                    self.dos.next_free = 0x2000;
                }
                let avail = 0x9FFF_u16.saturating_sub(self.dos.next_free);
                if want > avail {
                    self.cpu.regs[BX] = avail;
                    self.fail(E_NO_MEM);
                } else {
                    self.cpu.regs[AX] = self.dos.next_free;
                    self.dos.next_free += want.max(1);
                    self.ok();
                }
            }
            0x49 | 0x4A => self.ok(),
            0x4C => {
                self.dos.last_exit = self.al();
                return Svc::Terminate(self.al());
            }
            0x4D => self.cpu.regs[AX] = self.dos.last_exit as u16,
            0x4E | 0x4F => self.dos_find(ah == 0x4E),
            0x50 => self.dos.psp = self.cpu.regs[BX],
            0x51 | 0x62 => self.cpu.regs[BX] = self.dos.psp,
            0x54 => self.set_al(0),
            0x57 => {
                self.cpu.regs[CX] = 0;
                self.cpu.regs[DX] = 0x5B59;
                self.ok();
            }
            _ => {
                self.log.push(format!("INT 21h/AH={ah:02X}h is not supported"));
                self.cpu.set_flag(flags::CF, true);
                self.cpu.regs[AX] = E_INVALID;
            }
        }
        Svc::Ret
    }

    /// AH=0Ah: DS:DX -> [max][count][chars... CR]
    fn buffered_input(&mut self) -> Svc {
        let (ds, dx) = (self.cpu.sregs[DS], self.cpu.regs[DX]);
        let max = self.sys.mem.read8(linear(ds, dx)).max(1) as usize;
        let mut line = self.dos.line.take().unwrap_or_default();
        while let Some(k) = self.read_key() {
            let c = k as u8;
            match c {
                b'\r' => {
                    self.console_out(b'\r');
                    for (i, &b) in line.iter().enumerate() {
                        self.sys.mem.write8(linear(ds, dx.wrapping_add(2 + i as u16)), b);
                    }
                    self.sys.mem.write8(linear(ds, dx.wrapping_add(2 + line.len() as u16)), b'\r');
                    self.sys.mem.write8(linear(ds, dx.wrapping_add(1)), line.len() as u8);
                    return Svc::Ret;
                }
                8 => {
                    if line.pop().is_some() {
                        self.console_out(8);
                        self.console_out(b' ');
                        self.console_out(8);
                    }
                }
                0 => {}
                _ => {
                    if line.len() + 1 < max {
                        line.push(c);
                        self.console_out(c);
                    }
                }
            }
        }
        self.dos.line = Some(line);
        Svc::Block
    }

    // ---- files -------------------------------------------------------------------

    fn read_asciiz(&self, seg: u16, off: u16) -> String {
        String::from_utf8_lossy(&self.read_string(seg, off, 0, 256)).into_owned()
    }

    /// Maps a DOS path to a host path inside the root directory.
    fn host_path(&self, dos: &str) -> Option<PathBuf> {
        let root = self.dos.root.clone()?;
        let mut p = dos.trim().replace('\\', "/");
        if p.len() >= 2 && p.as_bytes()[1] == b':' {
            p = p[2..].to_string();
        }
        let full = if let Some(abs) = p.strip_prefix('/') {
            abs.to_string()
        } else if self.dos.cwd.is_empty() {
            p
        } else {
            format!("{}/{}", self.dos.cwd, p)
        };
        let mut parts: Vec<&str> = Vec::new();
        for c in full.split('/') {
            match c {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                c => parts.push(c),
            }
        }
        let mut out = root;
        for c in parts {
            out = out.join(find_case_insensitive(&out, c));
        }
        Some(out)
    }

    fn path_arg(&mut self, seg: u16, off: u16) -> Option<PathBuf> {
        let s = self.read_asciiz(seg, off);
        match self.host_path(&s) {
            Some(p) => Some(p),
            None => {
                if self.dos.root.is_none() {
                    self.log.push("DOS file access is disabled (no virtual drive configured)".into());
                }
                self.fail(E_PATH_NOT_FOUND);
                None
            }
        }
    }

    fn dos_path_op(&mut self, ah: u8) {
        let Some(p) = self.path_arg(self.cpu.sregs[DS], self.cpu.regs[DX]) else { return };
        let res = match ah {
            0x39 => std::fs::create_dir(&p),
            0x3A => std::fs::remove_dir(&p),
            0x3B => {
                if p.is_dir() {
                    let root = self.dos.root.clone().unwrap();
                    self.dos.cwd = p.strip_prefix(&root).unwrap_or(Path::new("")).to_string_lossy().replace('\\', "/");
                    Ok(())
                } else {
                    Err(std::io::Error::from(std::io::ErrorKind::NotFound))
                }
            }
            0x41 => std::fs::remove_file(&p),
            0x43 => {
                if p.exists() {
                    self.cpu.regs[CX] = if p.is_dir() { 0x10 } else { 0x20 };
                    Ok(())
                } else {
                    Err(std::io::Error::from(std::io::ErrorKind::NotFound))
                }
            }
            _ => {
                let Some(to) = self.path_arg(self.cpu.sregs[ES], self.cpu.regs[DI]) else { return };
                std::fs::rename(&p, to)
            }
        };
        match res {
            Ok(()) => self.ok(),
            Err(e) => self.fail(io_code(&e)),
        }
    }

    fn alloc_handle(&self) -> Option<u16> {
        (5..64).find(|h| !self.dos.files.contains_key(h))
    }

    fn dos_open(&mut self, ah: u8) {
        let Some(p) = self.path_arg(self.cpu.sregs[DS], self.cpu.regs[DX]) else { return };
        let Some(h) = self.alloc_handle() else { return self.fail(E_TOO_MANY) };
        let r = match ah {
            0x3C => File::create(&p),
            0x5B => OpenOptions::new().write(true).read(true).create_new(true).open(&p),
            _ => match self.al() & 3 {
                0 => File::open(&p),
                1 => OpenOptions::new().write(true).open(&p),
                _ => OpenOptions::new().read(true).write(true).open(&p),
            },
        };
        match r {
            Ok(f) => {
                self.dos.files.insert(h, f);
                self.cpu.regs[AX] = h;
                self.ok();
            }
            Err(e) => self.fail(io_code(&e)),
        }
    }

    fn dos_read(&mut self) -> Svc {
        let (h, n) = (self.cpu.regs[BX], self.cpu.regs[CX] as usize);
        let (ds, dx) = (self.cpu.sregs[DS], self.cpu.regs[DX]);
        if h == 0 {
            // Standard input: line-buffered keyboard input, terminated with CR LF.
            let mut line = self.dos.line.take().unwrap_or_default();
            while let Some(k) = self.read_key() {
                let c = k as u8;
                match c {
                    b'\r' => {
                        self.console_out(b'\r');
                        self.console_out(b'\n');
                        line.extend_from_slice(b"\r\n");
                        let cnt = line.len().min(n);
                        for (i, &b) in line[..cnt].iter().enumerate() {
                            self.sys.mem.write8(linear(ds, dx.wrapping_add(i as u16)), b);
                        }
                        self.cpu.regs[AX] = cnt as u16;
                        self.ok();
                        return Svc::Ret;
                    }
                    8 => {
                        if line.pop().is_some() {
                            self.console_out(8);
                            self.console_out(b' ');
                            self.console_out(8);
                        }
                    }
                    _ => {
                        line.push(c);
                        self.console_out(c);
                    }
                }
            }
            self.dos.line = Some(line);
            return Svc::Block;
        }
        let Some(f) = self.dos.files.get_mut(&h) else {
            self.fail(E_HANDLE);
            return Svc::Ret;
        };
        let mut buf = vec![0u8; n];
        match f.read(&mut buf) {
            Ok(got) => {
                for (i, &b) in buf[..got].iter().enumerate() {
                    self.sys.mem.write8(linear(ds, dx.wrapping_add(i as u16)), b);
                }
                self.cpu.regs[AX] = got as u16;
                self.ok();
            }
            Err(e) => self.fail(io_code(&e)),
        }
        Svc::Ret
    }

    fn dos_write(&mut self) {
        let (h, n) = (self.cpu.regs[BX], self.cpu.regs[CX] as usize);
        let (ds, dx) = (self.cpu.sregs[DS], self.cpu.regs[DX]);
        let data: Vec<u8> = (0..n).map(|i| self.sys.mem.read8(linear(ds, dx.wrapping_add(i as u16)))).collect();
        match h {
            1 | 2 => {
                for c in data {
                    self.console_out(c);
                }
                self.cpu.regs[AX] = n as u16;
                self.ok();
            }
            4 => {
                self.sys.dev.printer.extend_from_slice(&data);
                self.cpu.regs[AX] = n as u16;
                self.ok();
            }
            _ => {
                let Some(f) = self.dos.files.get_mut(&h) else { return self.fail(E_HANDLE) };
                // Writing 0 bytes truncates at the current position.
                let r = if n == 0 { f.stream_position().and_then(|p| f.set_len(p)).map(|_| 0) } else { f.write(&data) };
                match r {
                    Ok(w) => {
                        self.cpu.regs[AX] = w as u16;
                        self.ok();
                    }
                    Err(e) => self.fail(io_code(&e)),
                }
            }
        }
    }

    fn dos_seek(&mut self) {
        let h = self.cpu.regs[BX];
        let off = ((self.cpu.regs[CX] as u32) << 16) | self.cpu.regs[DX] as u32;
        let whence = match self.al() {
            0 => SeekFrom::Start(off as u64),
            1 => SeekFrom::Current(off as i32 as i64),
            2 => SeekFrom::End(off as i32 as i64),
            _ => return self.fail(E_INVALID),
        };
        let Some(f) = self.dos.files.get_mut(&h) else { return self.fail(E_HANDLE) };
        match f.seek(whence) {
            Ok(p) => {
                self.cpu.regs[DX] = (p >> 16) as u16;
                self.cpu.regs[AX] = p as u16;
                self.ok();
            }
            Err(e) => self.fail(io_code(&e)),
        }
    }

    /// AH=4Eh/4Fh: find first/next using the DTA layout of DOS.
    fn dos_find(&mut self, first: bool) {
        if first {
            let pat = self.read_asciiz(self.cpu.sregs[DS], self.cpu.regs[DX]);
            let pat = pat.replace('\\', "/");
            let (dir, mask) = match pat.rfind('/') {
                Some(i) => (pat[..i].to_string(), pat[i + 1..].to_string()),
                None => (String::new(), pat.clone()),
            };
            let Some(hdir) = self.host_path(if dir.is_empty() { "." } else { &dir }) else {
                return self.fail(E_PATH_NOT_FOUND);
            };
            let mut names: Vec<String> = std::fs::read_dir(hdir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| e.path().is_file())
                        .map(|e| e.file_name().to_string_lossy().to_ascii_uppercase())
                        .filter(|n| wildcard(&mask.to_ascii_uppercase(), n))
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            self.dos.find = Some((names, 0));
        }
        let Some((names, idx)) = self.dos.find.as_mut() else { return self.fail(E_NO_MORE) };
        let Some(name) = names.get(*idx).cloned() else { return self.fail(E_NO_MORE) };
        *idx += 1;
        let size = self.host_path(&name).and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
        let (seg, off) = self.dos.dta;
        let base = |i: u16| linear(seg, off.wrapping_add(i));
        self.sys.mem.write8(base(0x15), 0x20);
        self.sys.mem.write16(base(0x16), 0);
        self.sys.mem.write16(base(0x18), 0x5B59);
        self.sys.mem.write16(base(0x1A), size as u16);
        self.sys.mem.write16(base(0x1C), (size >> 16) as u16);
        let short: Vec<u8> = name.bytes().take(12).collect();
        for i in 0..13u16 {
            let b = short.get(i as usize).copied().unwrap_or(0);
            self.sys.mem.write8(base(0x1E + i), b);
        }
        self.cpu.regs[AX] = 0;
        self.ok();
    }
}

fn wildcard(pat: &str, name: &str) -> bool {
    if pat == "*.*" || pat == "*" {
        return true;
    }
    fn m(p: &[u8], n: &[u8]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some(b'*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some(b'?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    m(pat.as_bytes(), name.as_bytes())
}

fn find_case_insensitive(dir: &Path, name: &str) -> String {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.eq_ignore_ascii_case(name) {
                return n;
            }
        }
    }
    name.to_string()
}

fn io_code(e: &std::io::Error) -> u16 {
    match e.kind() {
        std::io::ErrorKind::NotFound => E_FILE_NOT_FOUND,
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::AlreadyExists => E_ACCESS,
        _ => E_ACCESS,
    }
}

/// Local time of the host.
fn host_now() -> DateTime {
    let secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let offset = local_utc_offset_secs();
    let ms = secs + offset * 1000;
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    DateTime {
        year: y as u16,
        month: m as u8,
        day: d as u8,
        weekday: ((days + 4).rem_euclid(7)) as u8,
        hour: (rem / 3_600_000) as u8,
        minute: (rem / 60_000 % 60) as u8,
        second: (rem / 1000 % 60) as u8,
        hundredths: (rem % 1000 / 10) as u8,
    }
}

#[cfg(unix)]
fn local_utc_offset_secs() -> i64 {
    // UTC offset of the local time zone via libc's localtime_r (tm_gmtoff).
    unsafe extern "C" {
        fn time(t: *mut i64) -> i64;
        fn localtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm;
    }
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: std::ffi::c_long,
        tm_zone: *const std::ffi::c_char,
    }
    unsafe {
        let mut t = 0i64;
        time(&mut t);
        let mut tm: Tm = std::mem::zeroed();
        if localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i64
    }
}

#[cfg(not(unix))]
fn local_utc_offset_secs() -> i64 {
    0
}

/// Days since 1970-01-01 -> (year, month, day). (Howard Hinnant's algorithm.)
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_386), (2025, 10, 25));
        assert!(wildcard("*.TXT", "A.TXT"));
        assert!(!wildcard("*.TXT", "A.BIN"));
        assert!(wildcard("A?C.*", "ABC.D"));
    }
}
