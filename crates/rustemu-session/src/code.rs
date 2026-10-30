//! Disassembly that knows the program: bytes the assembler emitted as data are shown as
//! `DB`/`DW`/`DD` (`DB 'Hello', 0Dh, 0Ah, '$'`), and runs of zeros outside the code (the memory
//! after the program) as one `DB n DUP (0)` row.

use rustemu_core::disasm::{disassemble, hex8, hex16};

use crate::Session;

/// One row of the disassembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRow {
    /// Number of bytes the row covers.
    pub len: u32,
    /// `MOV AX, 0005h` or `DB 'Hello', 0`.
    pub text: String,
    /// The row shows data, not an instruction.
    pub data: bool,
}

/// Bytes in a run of equal values that make it a `DUP` row.
const MIN_DUP: usize = 8;
/// A data row ends after about this many characters of operands.
const ROW_CHARS: usize = 40;

impl Session {
    /// The disassembly row at `addr`, whose offset is `ip` (for jump targets). With `data`, bytes
    /// the program declared as data are shown as data, and so are runs of zeros outside its code.
    /// The instruction at CS:IP is always decoded as code, even inside data.
    pub fn code_row(&self, addr: u32, ip: u16, data: bool) -> CodeRow {
        let pc = self.machine.cpu.pc();
        if data && addr != pc {
            // A data row stops before CS:IP, so that the current instruction gets its own row.
            let limit = if pc > addr { pc - addr } else { u32::MAX };
            let mem = &self.machine.sys.mem;
            match self.line_info_at(addr) {
                Some(l) if !l.is_code => {
                    let start = self.image_base().unwrap_or(0) + l.image_offset;
                    let len = (start + l.len - addr).min(limit);
                    let unit = l.data_size.max(1) as u32;
                    let unit = if (addr - start).is_multiple_of(unit) { unit } else { 1 };
                    let (len, text) = data_text(mem.slice(addr, len as usize), unit as usize);
                    return CodeRow { len: len as u32, text, data: true };
                }
                Some(_) => {}
                None => {
                    // Up to the next line of the program or the end of the segment.
                    let limit = limit.min(0x10000 - ip as u32);
                    let limit = self.next_line_addr(addr).map_or(limit, |n| limit.min(n - addr));
                    let zeros = mem.slice(addr, limit as usize).iter().take_while(|&&b| b == 0).count();
                    if zeros >= MIN_DUP {
                        return CodeRow { len: zeros as u32, text: format!("DB {zeros} DUP (0)"), data: true };
                    }
                }
            }
        }
        let ins = disassemble(self.machine.sys.mem.slice(addr, 8), ip);
        CodeRow { len: ins.len as u32, text: ins.text, data: false }
    }

    /// Address of the first program line that starts after `addr`.
    fn next_line_addr(&self, addr: u32) -> Option<u32> {
        let p = self.program.as_ref()?;
        let base = self.image_base()?;
        let off = addr.checked_sub(base)?;
        let i = self.lines_sorted.partition_point(|&i| p.lines[i].image_offset <= off);
        self.lines_sorted.get(i).map(|&i| base + p.lines[i].image_offset)
    }
}

fn printable(b: u8) -> bool {
    (0x20..0x7F).contains(&b) && b != b'\''
}

/// `DB`/`DW`/`DD` text for the start of `b` (elements of `unit` bytes): the number of bytes shown
/// and the text. A run of equal values (other than characters) becomes `n DUP (v)`.
fn data_text(b: &[u8], unit: usize) -> (usize, String) {
    let unit = if matches!(unit, 2 | 4) && b.len() >= unit { unit } else { 1 };
    let dir = match unit {
        2 => "DW",
        4 => "DD",
        _ => "DB",
    };
    let elems = b.len() / unit;
    let val = |i: usize| b[i * unit..(i + 1) * unit].iter().rev().fold(0u32, |v, &x| v << 8 | x as u32);
    let num = |v: u32| match unit {
        _ if v < 10 => v.to_string(),
        1 => hex8(v as u8),
        2 => hex16(v as u16),
        _ => {
            let s = format!("{v:08X}h");
            if s.starts_with(|c: char| c.is_ascii_alphabetic()) { format!("0{s}") } else { s }
        }
    };
    let is_char = |i: usize| unit == 1 && printable(b[i]);
    let run = |i: usize| if is_char(i) { 1 } else { (i..elems).take_while(|&j| val(j) == val(i)).count() };
    let min_run = MIN_DUP / unit;

    let r = run(0);
    if r >= min_run {
        return (r * unit, format!("{dir} {r} DUP ({})", num(val(0))));
    }
    let mut items: Vec<String> = Vec::new();
    let mut width = 0;
    let mut i = 0;
    while i < elems && width < ROW_CHARS {
        if i > 0 && run(i) >= min_run {
            break;
        }
        let item = if is_char(i) {
            let n = (i..elems).take_while(|&j| is_char(j)).count().min(ROW_CHARS - width);
            let s = format!("'{}'", String::from_utf8_lossy(&b[i..i + n]));
            i += n;
            s
        } else {
            i += 1;
            num(val(i - 1))
        };
        width += item.len() + 2;
        items.push(item);
    }
    (i * unit, format!("{dir} {}", items.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_rows() {
        assert_eq!(data_text(b"Hello, World!\r\n$", 1), (16, "DB 'Hello, World!', 0Dh, 0Ah, '$'".into()));
        assert_eq!(data_text(b"First number: \0", 1), (15, "DB 'First number: ', 0".into()));
        assert_eq!(data_text(&[0; 100], 1), (100, "DB 100 DUP (0)".into()));
        // A run is shown on its own row.
        assert_eq!(data_text(&[1, 2, 0, 0, 0, 0, 0, 0, 0, 0], 1), (2, "DB 1, 2".into()));
        // Spaces inside a string stay in the string.
        assert_eq!(data_text(b"a        b", 1), (10, "DB 'a        b'".into()));
        assert_eq!(data_text(b"it's", 1), (4, "DB 'it', 27h, 's'".into()));
        assert_eq!(data_text(&[0x34, 0x12, 5, 0, 0xFF, 0xFF], 2), (6, "DW 1234h, 5, 0FFFFh".into()));
        assert_eq!(data_text(&[0; 20], 2), (20, "DW 10 DUP (0)".into()));
        assert_eq!(data_text(&[0x78, 0x56, 0x34, 0x12], 4), (4, "DD 12345678h".into()));
        // A word cut by CS:IP or the end of the line falls back to bytes.
        assert_eq!(data_text(&[0x84], 2), (1, "DB 84h".into()));
        // Long text is split into rows.
        let long = [b'x'; 100];
        let (n, t) = data_text(&long, 1);
        assert_eq!((n, t.len()), (ROW_CHARS, ROW_CHARS + 5));
        let (n, _) = data_text(&[0xC3; 30], 1);
        assert_eq!(n, 30);
        let mixed: Vec<u8> = (1..=30).collect();
        let (n, t) = data_text(&mixed, 1);
        assert!(n < 30 && t.len() <= ROW_CHARS + 8, "{n} {t}");
    }
}
