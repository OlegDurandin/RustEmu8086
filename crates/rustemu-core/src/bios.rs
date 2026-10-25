//! BIOS services: INT 10h, 13h, 15h, 16h, 1Ah, 33h.

use crate::bus::linear;
use crate::cpu::{AX, BP, BX, CX, DX, ES, flags};
use crate::machine::{Machine, Svc};
use crate::video::{self, Video};

impl Machine {
    fn ah(&self) -> u8 {
        (self.cpu.regs[AX] >> 8) as u8
    }

    pub(crate) fn int10(&mut self) -> Svc {
        let r = self.cpu.regs;
        let (al, bh, bl) = (r[AX] as u8, (r[BX] >> 8) as u8, r[BX] as u8);
        let (dh, dl) = ((r[DX] >> 8) as u8, r[DX] as u8);
        let ah = self.ah();
        let mem = &mut self.sys.mem;
        match ah {
            0x00 => {
                Video::set_mode(mem, al);
                self.sys.dev.palette = video::default_palette();
            }
            0x01 => mem.write16(video::BDA_CURSOR_SHAPE, r[CX]),
            0x02 => Video::set_cursor(mem, bh, dl as u16, dh as u16),
            0x03 => {
                let (c, row) = Video::cursor(mem, bh);
                self.cpu.regs[DX] = (row << 8) | c;
                self.cpu.regs[CX] = mem.read16(video::BDA_CURSOR_SHAPE);
            }
            0x05 => {
                if al < 8 {
                    mem.write8(video::BDA_PAGE, al);
                    let start = al as u16 * mem.read16(video::BDA_PAGE_SIZE);
                    mem.write16(video::BDA_PAGE_START, start);
                }
            }
            0x06 | 0x07 => {
                let (ch, cl) = (r[CX] >> 8, r[CX] & 0xFF);
                Video::scroll(mem, ah == 6, al, bh, ch, cl, dh as u16, dl as u16);
            }
            0x08 => {
                let (c, row) = Video::cursor(mem, bh);
                let (chr, attr) = Video::read_cell(mem, bh, c, row);
                self.cpu.regs[AX] = ((attr as u16) << 8) | chr as u16;
            }
            0x09 | 0x0A => {
                let (c, row) = Video::cursor(mem, bh);
                let cols = Video::cols(mem);
                let rows = Video::rows(mem);
                let attr = if ah == 9 || !Video::is_text(mem) { Some(bl) } else { None };
                let start = row as u32 * cols as u32 + c as u32;
                let end = (start + r[CX] as u32).min(cols as u32 * rows as u32);
                for pos in start..end {
                    Video::write_cell(mem, bh, (pos % cols as u32) as u16, (pos / cols as u32) as u16, al, attr);
                }
            }
            0x0B => {} // CGA palette
            0x0C => Video::put_pixel(mem, r[CX], r[DX], al),
            0x0D => {
                let v = Video::get_pixel(mem, r[CX], r[DX]);
                self.cpu.regs[AX] = (r[AX] & 0xFF00) | v as u16;
            }
            0x0E => {
                self.output.push(al);
                Video::teletype(&mut self.sys.mem, al, bl);
            }
            0x0F => {
                let mode = Video::mode(mem) as u16;
                let cols = Video::cols(mem);
                self.cpu.regs[AX] = (cols << 8) | mode;
                self.cpu.regs[BX] = (r[BX] & 0x00FF) | ((Video::page(mem) as u16) << 8);
            }
            0x10 => match al {
                // Set individual DAC register / blink control.
                0x03 => mem.write8(video::BDA + 0x65, if bl != 0 { 0x29 } else { 0x09 }),
                0x10 => {
                    let i = r[BX] as u8 as usize;
                    self.sys.dev.palette[i] = [dh & 0x3F, (r[CX] >> 8) as u8 & 0x3F, r[CX] as u8 & 0x3F];
                }
                0x15 => {
                    let p = self.sys.dev.palette[r[BX] as u8 as usize];
                    self.cpu.regs[DX] = ((p[0] as u16) << 8) | (r[DX] & 0xFF);
                    self.cpu.regs[CX] = ((p[1] as u16) << 8) | p[2] as u16;
                }
                _ => {}
            },
            0x11 | 0x12 => {}
            0x13 => {
                // Write string ES:BP, CX chars at DH,DL; AL bit0 = update cursor, bit1 = char/attr pairs.
                let (es, bp) = (self.cpu.sregs[ES], r[BP]);
                let (oc, or) = Video::cursor(mem, bh);
                Video::set_cursor(mem, bh, dl as u16, dh as u16);
                let mut o = bp;
                for _ in 0..r[CX] {
                    let ch = mem.read8(linear(es, o));
                    o = o.wrapping_add(1);
                    let attr = if al & 2 != 0 {
                        let a = mem.read8(linear(es, o));
                        o = o.wrapping_add(1);
                        a
                    } else {
                        bl
                    };
                    match ch {
                        7 | 8 | 10 | 13 => Video::teletype(mem, ch, attr),
                        _ => Video::put_char_advance(mem, ch, Some(attr)),
                    }
                }
                if al & 1 == 0 {
                    Video::set_cursor(mem, bh, oc, or);
                }
            }
            0x1A => {
                if al == 0 {
                    self.cpu.regs[AX] = (r[AX] & 0xFF00) | 0x1A;
                    self.cpu.regs[BX] = 0x0008;
                }
            }
            ah => self.log.push(format!("INT 10h/AH={ah:02X}h is not supported")),
        }
        Svc::Ret
    }

    pub(crate) fn int16(&mut self) -> Svc {
        match self.ah() {
            0x00 | 0x10 => match self.sys.keys.pop_front() {
                Some(k) => {
                    self.cpu.regs[AX] = k;
                    Svc::Ret
                }
                None => Svc::Block,
            },
            0x01 | 0x11 => {
                match self.sys.keys.front() {
                    Some(&k) => {
                        self.cpu.regs[AX] = k;
                        self.cpu.set_flag(flags::ZF, false);
                    }
                    None => self.cpu.set_flag(flags::ZF, true),
                }
                Svc::Ret
            }
            0x02 | 0x12 => {
                self.cpu.regs[AX] &= 0xFF00;
                Svc::Ret
            }
            0x05 => {
                let cx = self.cpu.regs[CX];
                self.push_key((cx >> 8) as u8, cx as u8);
                self.cpu.regs[AX] &= 0xFF00;
                Svc::Ret
            }
            _ => Svc::Ret,
        }
    }

    pub(crate) fn int1a(&mut self) -> Svc {
        let t = self.now();
        match self.ah() {
            0x00 => {
                let secs = t.hour as u32 * 3600 + t.minute as u32 * 60 + t.second as u32;
                let ticks = (secs as u64 * 1_193_180 / 65536) as u32 + t.hundredths as u32 * 18 / 100;
                self.cpu.regs[CX] = (ticks >> 16) as u16;
                self.cpu.regs[DX] = ticks as u16;
                self.cpu.regs[AX] &= 0xFF00;
            }
            0x02 => {
                self.cpu.regs[CX] = ((bcd(t.hour) as u16) << 8) | bcd(t.minute) as u16;
                self.cpu.regs[DX] = (bcd(t.second) as u16) << 8;
                self.cpu.set_flag(flags::CF, false);
            }
            0x04 => {
                self.cpu.regs[CX] = ((bcd((t.year / 100) as u8) as u16) << 8) | bcd((t.year % 100) as u8) as u16;
                self.cpu.regs[DX] = ((bcd(t.month) as u16) << 8) | bcd(t.day) as u16;
                self.cpu.set_flag(flags::CF, false);
            }
            _ => {}
        }
        Svc::Ret
    }

    pub(crate) fn int15(&mut self) -> Svc {
        match self.ah() {
            // Wait CX:DX microseconds (real time).
            0x86 => {
                let us = ((self.cpu.regs[CX] as u64) << 16) | self.cpu.regs[DX] as u64;
                let now = std::time::Instant::now();
                match self.wait_until {
                    _ if self.instant_waits => {}
                    None => {
                        self.wait_until = Some(now + std::time::Duration::from_micros(us));
                        return Svc::Sleep;
                    }
                    Some(t) if now < t => return Svc::Sleep,
                    Some(_) => {}
                }
                self.wait_until = None;
                self.cpu.set_flag(flags::CF, false);
            }
            0x88 => self.cpu.regs[AX] = 0,
            0xC0 => {
                self.cpu.set_flag(flags::CF, true);
                self.cpu.regs[AX] = 0x8600;
            }
            _ => self.cpu.set_flag(flags::CF, true),
        }
        Svc::Ret
    }

    pub(crate) fn int13(&mut self) -> Svc {
        let ah = self.ah();
        match ah {
            0x00 => {
                self.cpu.regs[AX] &= 0x00FF;
                self.cpu.set_flag(flags::CF, false);
            }
            0x02 | 0x03 => match self.disk_rw(ah == 3) {
                Ok(n) => {
                    self.cpu.regs[AX] = n as u16;
                    self.cpu.set_flag(flags::CF, false);
                }
                Err(code) => {
                    self.cpu.regs[AX] = (code as u16) << 8;
                    self.cpu.set_flag(flags::CF, true);
                }
            },
            0x08 => {
                // 1.44M floppy geometry.
                self.cpu.regs[AX] = 0;
                self.cpu.regs[BX] = 0x0004;
                self.cpu.regs[CX] = (79 << 8) | 18;
                self.cpu.regs[DX] = (1 << 8) | 1;
                self.cpu.set_flag(flags::CF, false);
            }
            _ => {
                self.cpu.regs[AX] = 0x0100;
                self.cpu.set_flag(flags::CF, true);
            }
        }
        Svc::Ret
    }

    /// Floppy sector transfer with the image in `self.floppy` (CHS 80/2/18).
    fn disk_rw(&mut self, write: bool) -> Result<u8, u8> {
        let r = self.cpu.regs;
        let count = r[AX] as u8 as usize;
        let cyl = ((r[CX] >> 8) | ((r[CX] & 0xC0) << 2)) as usize;
        let sector = (r[CX] & 0x3F) as usize;
        let head = (r[DX] >> 8) as usize;
        if r[DX] as u8 != 0 {
            return Err(0x80);
        }
        if sector == 0 {
            return Err(0x04);
        }
        let lba = (cyl * 2 + head) * 18 + sector - 1;
        let (es, bx) = (self.cpu.sregs[ES], r[BX]);
        let floppy = &mut self.floppy;
        let need = (lba + count) * 512;
        if need > 1_474_560 {
            return Err(0x04);
        }
        if floppy.len() < need {
            floppy.resize(1_474_560, 0);
        }
        for i in 0..count * 512 {
            let a = linear(es, bx.wrapping_add(i as u16));
            if write {
                floppy[lba * 512 + i] = self.sys.mem.read8(a);
            } else {
                let v = floppy[lba * 512 + i];
                self.sys.mem.write8(a, v);
            }
        }
        Ok(count as u8)
    }

    pub(crate) fn int33(&mut self) -> Svc {
        match self.cpu.regs[AX] {
            0x0000 => {
                self.cpu.regs[AX] = 0xFFFF;
                self.cpu.regs[BX] = 2;
                self.mouse_visible = false;
            }
            0x0001 => self.mouse_visible = true,
            0x0002 => self.mouse_visible = false,
            0x0003 => {
                self.cpu.regs[BX] = self.mouse.2;
                self.cpu.regs[CX] = self.mouse.0;
                self.cpu.regs[DX] = self.mouse.1;
            }
            0x0004 => {
                self.mouse.0 = self.cpu.regs[CX];
                self.mouse.1 = self.cpu.regs[DX];
            }
            _ => {}
        }
        Svc::Ret
    }
}

fn bcd(v: u8) -> u8 {
    ((v / 10) << 4) | (v % 10)
}
