//! DOS services (INT 21h). Drive C: is a host directory (file access confined to it) or a drive in
//! memory ([`MemDrive`], the browser).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bus::linear;
use crate::cpu::{AX, BX, CX, DI, DS, DX, ES, SI, flags};
use crate::machine::{Machine, Svc};
use crate::memdrive::MemDrive;

type FindState = Option<(Arc<Vec<(String, u64)>>, usize)>;

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

/// Drive C: of DOS programs.
pub(crate) enum DosDrive {
    /// A host directory: files stay on disk, confined to it.
    Host(PathBuf),
    /// Files in memory (the browser).
    Memory(MemDrive),
}

/// A file opened by a DOS program.
pub(crate) enum OpenFile {
    Host(File),
    /// A file of the memory drive: its key, the position and the access of the open call.
    Mem {
        key: String,
        pos: u64,
        read: bool,
        write: bool,
    },
}

/// A file or folder of drive C:.
enum Target {
    Host(PathBuf),
    /// A key of the memory drive.
    Mem(String),
}

#[derive(Default)]
pub(crate) struct DosState {
    pub drive: Option<DosDrive>,
    /// Current directory inside the root, using `/` separators, no leading slash.
    pub cwd: String,
    pub files: HashMap<u16, OpenFile>,
    pub psp: u16,
    pub dta: (u16, u16),
    pub waiting: bool,
    /// Pending AH=0Ah line input.
    pub line: Option<Vec<u8>>,
    pub last_exit: u8,
    /// Memory allocation: next free paragraph for AH=48h.
    pub next_free: u16,
    /// Results of the current FindFirst/FindNext search: DOS name and file size.
    pub find: FindState,
}

#[derive(Clone)]
pub(crate) struct DosSnapshot {
    cwd: String,
    psp: u16,
    dta: (u16, u16),
    waiting: bool,
    line: Option<Vec<u8>>,
    last_exit: u8,
    next_free: u16,
    find: FindState,
}

impl PartialEq for DosSnapshot {
    fn eq(&self, other: &Self) -> bool {
        let same_find = match (&self.find, &other.find) {
            (None, None) => true,
            (Some((a, ai)), Some((b, bi))) => ai == bi && Arc::ptr_eq(a, b),
            _ => false,
        };
        self.cwd == other.cwd
            && self.psp == other.psp
            && self.dta == other.dta
            && self.waiting == other.waiting
            && self.line == other.line
            && self.last_exit == other.last_exit
            && self.next_free == other.next_free
            && same_find
    }
}

impl Eq for DosSnapshot {}

impl DosState {
    pub(crate) fn snapshot(&self) -> DosSnapshot {
        DosSnapshot {
            cwd: self.cwd.clone(),
            psp: self.psp,
            dta: self.dta,
            waiting: self.waiting,
            line: self.line.clone(),
            last_exit: self.last_exit,
            next_free: self.next_free,
            find: self.find.clone(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: DosSnapshot) {
        self.cwd = snapshot.cwd;
        self.psp = snapshot.psp;
        self.dta = snapshot.dta;
        self.waiting = snapshot.waiting;
        self.line = snapshot.line;
        self.last_exit = snapshot.last_exit;
        self.next_free = snapshot.next_free;
        self.find = snapshot.find;
    }
}

pub(crate) const E_FILE_NOT_FOUND: u16 = 2;
pub(crate) const E_PATH_NOT_FOUND: u16 = 3;
const E_TOO_MANY: u16 = 4;
pub(crate) const E_ACCESS: u16 = 5;
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
        // Host filesystem changes and open-file cursor/handle changes cannot be
        // rolled back safely. They form a reverse-debugging history boundary.
        if matches!(ah, 0x39 | 0x3A | 0x3C | 0x3D | 0x41 | 0x56 | 0x5B)
            || matches!(ah, 0x3E | 0x3F | 0x40 | 0x42) && self.cpu.regs[BX] >= 5
        {
            self.mark_history_barrier();
        }
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

    /// Names of DOS path `dos` from the root of drive C:, with `.` and `..` resolved against the
    /// current directory; None when `..` leaves the root.
    fn dos_components(&self, dos: &str) -> Option<Vec<String>> {
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
        let mut parts = Vec::new();
        for c in full.split('/') {
            match c {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                c => parts.push(c.to_string()),
            }
        }
        Some(parts)
    }

    /// Maps a DOS path to a file or folder of drive C:.
    fn target(&self, dos: &str) -> Option<Target> {
        let parts = self.dos_components(dos)?;
        match self.dos.drive.as_ref()? {
            DosDrive::Host(_) => {
                // Work from the canonical root so a root configured through a symlink is treated
                // as the directory it points to. The final containment check is what prevents
                // symlinks inside the virtual drive from escaping to the host filesystem.
                let root = self.canonical_dos_root()?;
                let mut out = root.clone();
                for c in parts {
                    out = out.join(find_case_insensitive(&out, &c));
                }
                path_is_confined(&root, &out).then_some(Target::Host(out))
            }
            DosDrive::Memory(_) => Some(Target::Mem(parts.join("/").to_ascii_uppercase())),
        }
    }

    /// The host path of a DOS path (drive C: in a host folder).
    #[cfg(test)]
    fn host_path(&self, dos: &str) -> Option<PathBuf> {
        match self.target(dos)? {
            Target::Host(p) => Some(p),
            Target::Mem(_) => None,
        }
    }

    fn canonical_dos_root(&self) -> Option<PathBuf> {
        match self.dos.drive.as_ref()? {
            DosDrive::Host(root) => std::fs::canonicalize(root).ok(),
            DosDrive::Memory(_) => None,
        }
    }

    fn mem_drive(&mut self) -> Option<&mut MemDrive> {
        match self.dos.drive.as_mut()? {
            DosDrive::Memory(d) => Some(d),
            DosDrive::Host(_) => None,
        }
    }

    fn path_arg(&mut self, seg: u16, off: u16) -> Option<Target> {
        let s = self.read_asciiz(seg, off);
        match self.target(&s) {
            Some(t) => Some(t),
            None => {
                if self.dos.drive.is_none() {
                    self.log.push("DOS file access is disabled (no virtual drive configured)".into());
                }
                self.fail(E_PATH_NOT_FOUND);
                None
            }
        }
    }

    fn dos_path_op(&mut self, ah: u8) {
        let Some(t) = self.path_arg(self.cpu.sregs[DS], self.cpu.regs[DX]) else { return };
        let res = match t {
            Target::Host(p) => self.host_path_op(ah, &p),
            Target::Mem(key) => self.mem_path_op(ah, key),
        };
        match res {
            Ok(()) => self.ok(),
            Err(code) => self.fail(code),
        }
    }

    /// AH=39h-3Bh, 41h, 43h, 56h on a host folder.
    fn host_path_op(&mut self, ah: u8, p: &Path) -> Result<(), u16> {
        let not_found = || std::io::Error::from(std::io::ErrorKind::NotFound);
        let res = match ah {
            0x39 => std::fs::create_dir(p),
            0x3A => std::fs::remove_dir(p),
            0x3B => {
                if p.is_dir() {
                    let cwd =
                        self.canonical_dos_root().and_then(|root| p.strip_prefix(root).ok().map(Path::to_path_buf));
                    match cwd {
                        Some(cwd) => {
                            self.dos.cwd = cwd.to_string_lossy().replace('\\', "/");
                            Ok(())
                        }
                        None => Err(not_found()),
                    }
                } else {
                    Err(not_found())
                }
            }
            0x41 => std::fs::remove_file(p),
            0x43 => {
                if p.exists() {
                    self.cpu.regs[CX] = if p.is_dir() { 0x10 } else { 0x20 };
                    Ok(())
                } else {
                    Err(not_found())
                }
            }
            _ => {
                let to = self.read_asciiz(self.cpu.sregs[ES], self.cpu.regs[DI]);
                let Some(Target::Host(to)) = self.target(&to) else { return Err(E_PATH_NOT_FOUND) };
                std::fs::rename(p, to)
            }
        };
        res.map_err(|e| io_code(&e))
    }

    /// AH=39h-3Bh, 41h, 43h, 56h on the memory drive.
    fn mem_path_op(&mut self, ah: u8, key: String) -> Result<(), u16> {
        let to = (ah == 0x56).then(|| self.read_asciiz(self.cpu.sregs[ES], self.cpu.regs[DI]));
        let to = match to.map(|to| self.target(&to)) {
            Some(Some(Target::Mem(k))) => Some(k),
            Some(_) => return Err(E_PATH_NOT_FOUND),
            None => None,
        };
        let cwd = self.dos.cwd.clone();
        let Some(d) = self.mem_drive() else { return Err(E_PATH_NOT_FOUND) };
        match ah {
            0x39 => d.create_dir(&key),
            // The current directory cannot be removed.
            0x3A if key == cwd || cwd.starts_with(&format!("{key}/")) => Err(E_ACCESS),
            0x3A => d.remove_dir(&key),
            0x3B if d.is_dir(&key) => {
                self.dos.cwd = key;
                Ok(())
            }
            0x3B => Err(E_PATH_NOT_FOUND),
            0x41 => d.remove_file(&key),
            0x43 => {
                let attr = if d.is_dir(&key) {
                    0x10
                } else if d.file(&key).is_some() {
                    0x20
                } else {
                    return Err(E_FILE_NOT_FOUND);
                };
                self.cpu.regs[CX] = attr;
                Ok(())
            }
            _ => d.rename(&key, &to.unwrap_or_default()),
        }
    }

    fn alloc_handle(&self) -> Option<u16> {
        (5..64).find(|h| !self.dos.files.contains_key(h))
    }

    fn dos_open(&mut self, ah: u8) {
        let Some(t) = self.path_arg(self.cpu.sregs[DS], self.cpu.regs[DX]) else { return };
        let Some(h) = self.alloc_handle() else { return self.fail(E_TOO_MANY) };
        let mode = self.al() & 3;
        let r = match t {
            Target::Host(p) => match ah {
                0x3C => File::create(&p),
                0x5B => OpenOptions::new().write(true).read(true).create_new(true).open(&p),
                _ => match mode {
                    0 => File::open(&p),
                    1 => OpenOptions::new().write(true).open(&p),
                    _ => OpenOptions::new().read(true).write(true).open(&p),
                },
            }
            .map(OpenFile::Host)
            .map_err(|e| io_code(&e)),
            Target::Mem(key) => {
                let Some(d) = self.mem_drive() else { return self.fail(E_PATH_NOT_FOUND) };
                let (res, read, write) = match ah {
                    0x3C | 0x5B => (d.create(&key, ah == 0x5B), true, true),
                    _ => (d.open(&key), mode != 1, mode != 0),
                };
                res.map(|()| OpenFile::Mem { key, pos: 0, read, write })
            }
        };
        match r {
            Ok(f) => {
                self.dos.files.insert(h, f);
                self.cpu.regs[AX] = h;
                self.ok();
            }
            Err(code) => self.fail(code),
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
        let r = match self.dos.files.get_mut(&h) {
            None => Err(E_HANDLE),
            Some(OpenFile::Host(f)) => {
                let mut buf = vec![0u8; n];
                f.read(&mut buf)
                    .map(|got| {
                        buf.truncate(got);
                        buf
                    })
                    .map_err(|e| io_code(&e))
            }
            Some(OpenFile::Mem { read: false, .. }) => Err(E_ACCESS),
            Some(OpenFile::Mem { key, pos, .. }) => match &self.dos.drive {
                Some(DosDrive::Memory(d)) => match d.file(key) {
                    Some(data) => {
                        let start = (*pos as usize).min(data.len());
                        let got = data[start..(start + n).min(data.len())].to_vec();
                        *pos += got.len() as u64;
                        Ok(got)
                    }
                    // Deleted while open.
                    None => Err(E_ACCESS),
                },
                _ => Err(E_HANDLE),
            },
        };
        match r {
            Ok(buf) => {
                for (i, &b) in buf.iter().enumerate() {
                    self.sys.mem.write8(linear(ds, dx.wrapping_add(i as u16)), b);
                }
                self.cpu.regs[AX] = buf.len() as u16;
                self.ok();
            }
            Err(code) => self.fail(code),
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
                let r = match self.dos.files.get_mut(&h) {
                    None => Err(E_HANDLE),
                    // Writing 0 bytes truncates at the current position.
                    Some(OpenFile::Host(f)) => {
                        if n == 0 { f.stream_position().and_then(|p| f.set_len(p)).map(|_| 0) } else { f.write(&data) }
                            .map_err(|e| io_code(&e))
                    }
                    Some(OpenFile::Mem { write: false, .. }) => Err(E_ACCESS),
                    Some(OpenFile::Mem { key, pos, .. }) => match &mut self.dos.drive {
                        Some(DosDrive::Memory(d)) => match d.write_at(key, *pos, &data) {
                            Some(w) => {
                                *pos += w as u64;
                                Ok(w)
                            }
                            None => Err(E_ACCESS),
                        },
                        _ => Err(E_HANDLE),
                    },
                };
                match r {
                    Ok(w) => {
                        self.cpu.regs[AX] = w as u16;
                        self.ok();
                    }
                    Err(code) => self.fail(code),
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
        let r = match self.dos.files.get_mut(&h) {
            None => Err(E_HANDLE),
            Some(OpenFile::Host(f)) => f.seek(whence).map_err(|e| io_code(&e)),
            Some(OpenFile::Mem { key, pos, .. }) => {
                let len = match &self.dos.drive {
                    Some(DosDrive::Memory(d)) => d.file(key).map_or(0, |f| f.len() as i64),
                    _ => 0,
                };
                let new = match whence {
                    SeekFrom::Start(o) => o as i64,
                    SeekFrom::Current(o) => *pos as i64 + o,
                    SeekFrom::End(o) => len + o,
                };
                if new < 0 {
                    Err(E_ACCESS)
                } else {
                    *pos = new as u64;
                    Ok(*pos)
                }
            }
        };
        match r {
            Ok(p) => {
                self.cpu.regs[DX] = (p >> 16) as u16;
                self.cpu.regs[AX] = p as u16;
                self.ok();
            }
            Err(code) => self.fail(code),
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
            let mask = mask.to_ascii_uppercase();
            let mut entries: Vec<(String, u64)> = match self.target(if dir.is_empty() { "." } else { &dir }) {
                None => return self.fail(E_PATH_NOT_FOUND),
                Some(Target::Host(hdir)) => std::fs::read_dir(hdir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .filter(|e| e.path().is_file())
                            .filter_map(|e| {
                                let name = e.file_name().to_string_lossy().to_ascii_uppercase();
                                if !wildcard(&mask, &name) {
                                    return None;
                                }
                                let dos_path = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
                                let Target::Host(path) = self.target(&dos_path)? else { return None };
                                let size = std::fs::metadata(path).ok()?.len();
                                Some((name, size))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                Some(Target::Mem(key)) => {
                    let list = match &self.dos.drive {
                        Some(DosDrive::Memory(d)) => d.list(&key),
                        _ => None,
                    };
                    let Some(list) = list else { return self.fail(E_PATH_NOT_FOUND) };
                    list.into_iter().filter(|(name, _)| wildcard(&mask, name)).collect()
                }
            };
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            self.dos.find = Some((Arc::new(entries), 0));
        }
        let Some((entries, idx)) = self.dos.find.as_mut() else { return self.fail(E_NO_MORE) };
        let Some((name, size)) = entries.get(*idx).cloned() else { return self.fail(E_NO_MORE) };
        *idx += 1;
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

/// Checks the nearest existing ancestor because create/rename destinations do
/// not necessarily exist yet.  `symlink_metadata` distinguishes a genuinely
/// missing component from a broken symlink: the latter must be rejected, since
/// opening it could create its target outside the virtual drive.
fn path_is_confined(root: &Path, path: &Path) -> bool {
    let mut probe = path;
    loop {
        match std::fs::symlink_metadata(probe) {
            Ok(_) => return std::fs::canonicalize(probe).is_ok_and(|p| p.starts_with(root)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let Some(parent) = probe.parent() else { return false };
                probe = parent;
            }
            Err(_) => return false,
        }
    }
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
        web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
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

#[cfg(target_arch = "wasm32")]
fn local_utc_offset_secs() -> i64 {
    // getTimezoneOffset() is UTC minus local time, in minutes.
    -(js_sys::Date::new_0().get_timezone_offset() as i64) * 60
}

#[cfg(not(any(unix, target_arch = "wasm32")))]
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

    #[test]
    fn find_in_subdirectory_reports_the_file_size() {
        let unique = format!(
            "rustemu_dos_find_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("subdir")).unwrap();
        std::fs::write(root.join("subdir/example.txt"), b"seven!!").unwrap();

        let mut machine = Machine::new();
        machine.set_dos_root(root.clone());
        machine.dos.dta = (0x0200, 0);
        machine.cpu.sregs[DS] = 0x0100;
        machine.cpu.regs[DX] = 0;
        machine.sys.mem.load(linear(0x0100, 0), b"subdir\\*.txt\0");
        machine.dos_find(true);

        let dta = linear(0x0200, 0);
        let size = machine.sys.mem.read16(dta + 0x1A) as u32 | (machine.sys.mem.read16(dta + 0x1C) as u32) << 16;
        assert_eq!(size, 7);
        assert_eq!(machine.read_string(0x0200, 0x1E, 0, 13), b"EXAMPLE.TXT");
        let snapshot = machine.dos.snapshot();
        let entries = &machine.dos.find.as_ref().unwrap().0;
        let snap_entries = &snapshot.find.as_ref().unwrap().0;
        assert!(Arc::ptr_eq(entries, snap_entries));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn chdir_with_relative_root_keeps_the_relative_dos_path() {
        let child = std::fs::read_dir(".")
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .expect("the project directory must contain a subdirectory")
            .file_name()
            .to_string_lossy()
            .into_owned();

        let mut machine = Machine::new();
        machine.set_dos_root(PathBuf::from("."));
        machine.cpu.sregs[DS] = 0x0100;
        machine.cpu.regs[DX] = 0;
        let mut path = child.as_bytes().to_vec();
        path.push(0);
        machine.sys.mem.load(linear(0x0100, 0), &path);
        machine.dos_path_op(0x3B);

        assert_eq!(machine.dos.cwd, child);
        assert_eq!(machine.host_path("file.txt"), std::fs::canonicalize(&child).ok().map(|p| p.join("file.txt")));
    }

    #[cfg(unix)]
    #[test]
    fn dos_paths_cannot_escape_root_through_symlinks() {
        use std::os::unix::fs::symlink;

        let unique = format!(
            "rustemu_dos_symlink_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        let root = base.join("root");
        let linked_root = base.join("linked-root");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("subdir")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), b"secret").unwrap();
        symlink(&root, &linked_root).unwrap();
        symlink(&outside, root.join("external")).unwrap();
        symlink(outside.join("missing.txt"), root.join("broken")).unwrap();

        let mut machine = Machine::new();
        machine.set_dos_root(root.clone());

        assert!(machine.host_path("normal/new.txt").is_some());
        assert!(machine.host_path("external/secret.txt").is_none());
        assert!(machine.host_path("external/new.txt").is_none());
        assert!(machine.host_path("broken").is_none());

        machine.set_dos_root(linked_root);
        machine.cpu.sregs[DS] = 0x0100;
        machine.cpu.regs[DX] = 0;
        machine.sys.mem.load(linear(0x0100, 0), b"subdir\0");
        machine.dos_path_op(0x3B);
        assert_eq!(machine.dos.cwd, "subdir");

        std::fs::remove_dir_all(base).unwrap();
    }
}
