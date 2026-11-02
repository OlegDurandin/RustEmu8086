//! The 8086 side of the session: loading COM / EXE / BIN / BOOT programs (relocations, Emu8086
//! `#reg=#` / `#mem=#` directives, the DOS command line), virtual floppies, and where program
//! symbols live (`segment:offset`).

use std::path::{Path, PathBuf};

use rustemu_asm::{Format, Program};
use rustemu_core::machine::FLOPPY_SIZE;
use rustemu_core::memdrive::MemDrive;
use rustemu_core::{LoadKind, Machine, linear};

use crate::{Loadable, Session};

/// Error of an 8086 operation on a session with another processor.
const NOT_8086: &str = "the loaded machine is not an 8086";

impl Session {
    /// The 8086 machine, when the session emulates one.
    pub fn x86(&self) -> Option<&Machine> {
        rustemu_core::AsMachine::x86(&*self.target)
    }

    /// The 8086 machine, when the session emulates one, to change it.
    pub fn x86_mut(&mut self) -> Option<&mut Machine> {
        rustemu_core::AsMachine::x86_mut(&mut *self.target)
    }

    fn machine_mut(&mut self) -> Result<&mut Machine, String> {
        self.x86_mut().ok_or_else(|| NOT_8086.to_string())
    }

    /// How an assembled 8086 program is loaded.
    pub(crate) fn x86_loadable(prog: &Program) -> Loadable {
        match prog.format {
            Format::Com => Loadable::Com,
            Format::Exe => Loadable::Exe,
            Format::Boot => Loadable::Boot,
            Format::Bin => {
                // Emu8086: without LOAD_SEGMENT/LOAD_OFFSET the program is loaded at CS:IP,
                // without anything at 0100:0000.
                let reg = |n: &str| prog.emu.regs.iter().find(|(r, _)| r == n).map(|(_, v)| *v);
                Loadable::Bin {
                    seg: prog.emu.load_segment.or(reg("CS")).unwrap_or(0x0100),
                    off: prog.emu.load_offset.or(reg("IP")).unwrap_or(prog.origin),
                }
            }
        }
    }

    /// Loads executable bytes into the 8086 and applies the program's load-time settings
    /// (relocations of flat images, `#reg=#`, `#mem=#`, the command line).
    pub(crate) fn x86_load(&mut self, kind: Loadable, bytes: &[u8], program: Option<&Program>) -> Result<(), String> {
        let args = self.command_line.clone();
        let m = self.machine_mut()?;
        match kind {
            Loadable::Com => m.load_com(bytes)?,
            Loadable::Exe => m.load_exe(bytes)?,
            Loadable::Bin { seg, off } => m.load_bin(bytes, seg, off)?,
            Loadable::Boot => m.load_boot(bytes)?,
        }
        if let Some(p) = program {
            let info = m.loaded.clone().unwrap();
            // Segment fixups of flat programs (COM/BIN/BOOT) refer to the load segment.
            if p.format != Format::Exe {
                let seg = match kind {
                    Loadable::Bin { seg, .. } => seg,
                    _ => info.load_segment,
                };
                for &r in &p.relocs {
                    let a = info.image_base + r;
                    let v = m.sys.mem.read16(a).wrapping_add(seg);
                    m.sys.mem.write16(a, v);
                }
            }
            for (r, v) in &p.emu.regs {
                m.set_register(r, *v);
            }
            let mut mem = p.emu.mem.clone();
            if p.format == Format::Bin && p.emu.regs.is_empty() && p.emu.load_segment.is_none() && mem.is_empty() {
                // Like Emu8086's default.binf: RET from the program lands on a HLT.
                mem = vec![(0x10FFE, vec![0x00, 0xFF]), (0x10F00, vec![0xF4])];
            }
            for (a, bytes) in mem {
                m.sys.mem.load(a, &bytes);
            }
        }
        m.set_command_line(&args);
        Ok(())
    }

    /// Assembles (for `.asm`) or reads an executable file and loads it; DOS file functions then
    /// see the file's folder as drive C:.
    pub fn load_path(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.load_named(&path.to_string_lossy(), bytes)?;
        let root =
            path.parent().map(|dir| if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir.to_path_buf() });
        if let Some(root) = root {
            self.machine_mut()?.set_dos_root(root);
        }
        Ok(())
    }

    /// Loads the contents of file `name` (an executable by its extension, otherwise assembly source),
    /// e.g. a file opened in the browser.
    pub fn load_named(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), String> {
        let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let mut floppy = None;
        let result = match ext.as_str() {
            "com" => self.load_bytes(Loadable::Com, bytes),
            "exe" => self.load_bytes(Loadable::Exe, bytes),
            "bin" => self.load_bytes(Loadable::Bin { seg: 0x0100, off: 0 }, bytes),
            "boot" | "img" => {
                let n = bytes.len().min(512);
                if bytes.len() > 512 {
                    floppy = Some(bytes.clone());
                }
                self.load_bytes(Loadable::Boot, bytes[..n].to_vec())
            }
            _ => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let mut loader = rustemu_asm::FsLoader { include_dirs: Vec::new() };
                let prog = rustemu_asm::assemble(name, &text, &mut loader).map_err(|e| e.to_string())?;
                self.load_program(prog)
            }
        };
        if result.is_ok()
            && let Some(floppy) = floppy
        {
            self.machine_mut()?.floppies[0] = floppy;
        }
        result
    }

    /// Image of floppy drive `drive` (FLOPPY_n), resized to a full 1.44 MB disk.
    fn floppy_mut(&mut self, drive: usize) -> Result<&mut Vec<u8>, String> {
        let fl = self.machine_mut()?.floppies.get_mut(drive).ok_or(format!("there is no FLOPPY_{drive}"))?;
        if fl.len() < FLOPPY_SIZE {
            fl.resize(FLOPPY_SIZE, 0);
        }
        Ok(fl)
    }

    /// Boots from a virtual floppy: its sector 1 is loaded at 0000:7C00 (like the BIOS does) and
    /// DL holds the drive number. When the sector is the boot program assembled last, its source
    /// stays: the boot code is debugged line by line.
    pub fn boot_from_floppy(&mut self, drive: usize) -> Result<(), String> {
        let m = self.x86().ok_or(NOT_8086)?;
        let fl = m.floppies.get(drive).ok_or(format!("there is no FLOPPY_{drive}"))?;
        if fl.len() < 512 {
            return Err(format!("FLOPPY_{drive} is empty"));
        }
        let sector = fl[..512].to_vec();
        let program = self.program.clone().filter(|p| {
            let image = p.file_bytes();
            p.format == Format::Boot && sector.get(..image.len()) == Some(&image[..])
        });
        self.load_bytes_with_program(Loadable::Boot, sector, program)?;
        self.machine_mut()?.cpu.regs[rustemu_core::cpu::DX] = drive as u16;
        Ok(())
    }

    /// Copies the 512 bytes at 0000:7C00 to the boot sector of a virtual floppy.
    pub fn write_boot_sector(&mut self, drive: usize) -> Result<(), String> {
        let data = self.x86().ok_or(NOT_8086)?.sys.mem.slice(0x7C00, 512).to_vec();
        self.floppy_mut(drive)?[..512].copy_from_slice(&data);
        Ok(())
    }

    /// Writes `data` to a virtual floppy from cylinder / head / sector (sector numbers start at
    /// 1; 80 cylinders, 2 heads, 18 sectors). Returns the number of sectors written.
    pub fn write_floppy(
        &mut self,
        drive: usize,
        cyl: usize,
        head: usize,
        sector: usize,
        data: &[u8],
    ) -> Result<usize, String> {
        if cyl >= 80 || head >= 2 || !(1..=18).contains(&sector) {
            return Err("cylinder 0..79, head 0..1, sector 1..18".into());
        }
        let start = ((cyl * 2 + head) * 18 + sector - 1) * 512;
        if start + data.len() > FLOPPY_SIZE {
            return Err(format!("{} bytes do not fit on the floppy from this sector", data.len()));
        }
        self.floppy_mut(drive)?[start..start + data.len()].copy_from_slice(data);
        Ok(data.len().div_ceil(512))
    }

    /// While CS:IP is in the BIOS/DOS code: CS:IP of the `INT nn` that called it (the return
    /// address on the stack, less the two bytes of the instruction).
    pub fn bios_caller(&self) -> Option<(u16, u16)> {
        self.x86()?.bios_caller()
    }

    /// The DOS drive C: kept in memory (the browser), when the 8086 has one.
    pub fn dos_memory_drive(&self) -> Option<&MemDrive> {
        self.x86()?.dos_memory_drive()
    }

    /// The DOS drive C: kept in memory, to change it.
    pub fn dos_memory_drive_mut(&mut self) -> Option<&mut MemDrive> {
        self.x86_mut()?.dos_memory_drive_mut()
    }

    /// Gives the 8086's DOS a drive C: in memory (nothing for another processor).
    pub fn set_dos_memory_drive(&mut self, drive: MemDrive) {
        if let Some(m) = self.x86_mut() {
            m.set_dos_memory_drive(drive);
        }
    }

    /// Physical address of a program symbol (procedures, labels, variables).
    pub(crate) fn x86_symbol_addr(&self, s: &rustemu_asm::Symbol) -> Option<u32> {
        let (p, info) = (self.program.as_ref()?, self.x86()?.loaded.as_ref()?);
        Some(if info.kind == LoadKind::Exe {
            linear(info.load_segment.wrapping_add(s.segment.unwrap_or(0)), s.value as u16)
        } else {
            info.image_base.wrapping_add((s.value as u32).wrapping_sub(p.origin as u32))
        })
    }

    /// Segment and offset where a program symbol lives (variables and labels).
    pub(crate) fn x86_symbol_seg_off(&self, s: &rustemu_asm::Symbol) -> Option<(u16, u16)> {
        let (p, info) = (self.program.as_ref()?, self.x86()?.loaded.as_ref()?);
        if info.kind == LoadKind::Exe {
            return Some((info.load_segment.wrapping_add(s.segment.unwrap_or(0)), s.value as u16));
        }
        let addr = info.image_base.wrapping_add((s.value as u32).wrapping_sub(p.origin as u32));
        let base = addr.wrapping_sub(s.value as u16 as u32);
        if base % 16 == 0 && base < 0x10_0000 {
            Some(((base >> 4) as u16, s.value as u16))
        } else {
            Some(((addr >> 4) as u16, (addr & 15) as u16))
        }
    }

    /// The segment a `SEGMENT` symbol of the program is loaded at (its value is relative).
    pub(crate) fn x86_load_segment(&self) -> u16 {
        self.x86().and_then(|m| m.loaded.as_ref()).map_or(0, |l| l.load_segment)
    }
}
