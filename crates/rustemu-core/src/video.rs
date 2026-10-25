//! Video memory helpers (text modes 0-3 at B800:0000, mode 13h at A000:0000).
//! The video state lives in the BIOS Data Area so programs can inspect it like on a real PC.

use crate::bus::Memory;

pub const BDA: u32 = 0x400;
pub const BDA_MODE: u32 = BDA + 0x49;
pub const BDA_COLS: u32 = BDA + 0x4A;
pub const BDA_PAGE_SIZE: u32 = BDA + 0x4C;
pub const BDA_PAGE_START: u32 = BDA + 0x4E;
pub const BDA_CURSOR: u32 = BDA + 0x50;
pub const BDA_CURSOR_SHAPE: u32 = BDA + 0x60;
pub const BDA_PAGE: u32 = BDA + 0x62;
pub const BDA_ROWS: u32 = BDA + 0x84;
pub const BDA_CHAR_HEIGHT: u32 = BDA + 0x85;

pub const TEXT_BASE: u32 = 0xB8000;
pub const GFX_BASE: u32 = 0xA0000;

/// Default 256-colour VGA palette (6-bit DAC values).
pub fn default_palette() -> [[u8; 3]; 256] {
    let mut p = [[0u8; 3]; 256];
    const CGA: [[u8; 3]; 16] = [
        [0, 0, 0],
        [0, 0, 42],
        [0, 42, 0],
        [0, 42, 42],
        [42, 0, 0],
        [42, 0, 42],
        [42, 21, 0],
        [42, 42, 42],
        [21, 21, 21],
        [21, 21, 63],
        [21, 63, 21],
        [21, 63, 63],
        [63, 21, 21],
        [63, 21, 63],
        [63, 63, 21],
        [63, 63, 63],
    ];
    p[..16].copy_from_slice(&CGA);
    const GREY: [u8; 16] = [0, 5, 8, 11, 14, 17, 20, 24, 28, 32, 36, 40, 45, 50, 56, 63];
    for (i, g) in GREY.iter().enumerate() {
        p[16 + i] = [*g; 3];
    }
    // 9 groups of 24 hues (blue -> magenta -> red -> yellow -> green -> cyan), 5 levels each.
    const LEVELS: [[u8; 5]; 9] = [
        [0, 16, 31, 47, 63],
        [31, 39, 47, 55, 63],
        [45, 49, 54, 58, 63],
        [0, 7, 14, 21, 28],
        [14, 17, 21, 24, 28],
        [20, 22, 24, 26, 28],
        [0, 4, 8, 12, 16],
        [8, 10, 12, 14, 16],
        [11, 12, 13, 15, 16],
    ];
    const R: [usize; 24] = [0, 1, 2, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 3, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0];
    const G: [usize; 24] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 3, 2, 1];
    const B: [usize; 24] = [4, 4, 4, 4, 4, 3, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 4, 4, 4];
    for (g, lv) in LEVELS.iter().enumerate() {
        for h in 0..24 {
            p[32 + g * 24 + h] = [lv[R[h]], lv[G[h]], lv[B[h]]];
        }
    }
    p
}

pub struct Video;

impl Video {
    pub fn mode(mem: &Memory) -> u8 {
        mem.read8(BDA_MODE)
    }

    pub fn is_text(mem: &Memory) -> bool {
        Self::mode(mem) <= 3 || Self::mode(mem) == 7
    }

    pub fn cols(mem: &Memory) -> u16 {
        mem.read16(BDA_COLS).max(1)
    }

    pub fn rows(mem: &Memory) -> u16 {
        mem.read8(BDA_ROWS) as u16 + 1
    }

    pub fn page(mem: &Memory) -> u8 {
        mem.read8(BDA_PAGE)
    }

    pub fn page_base(mem: &Memory, page: u8) -> u32 {
        TEXT_BASE + page as u32 * mem.read16(BDA_PAGE_SIZE) as u32
    }

    pub fn cursor(mem: &Memory, page: u8) -> (u16, u16) {
        let a = BDA_CURSOR + (page as u32 & 7) * 2;
        (mem.read8(a) as u16, mem.read8(a + 1) as u16)
    }

    pub fn set_cursor(mem: &mut Memory, page: u8, col: u16, row: u16) {
        let a = BDA_CURSOR + (page as u32 & 7) * 2;
        mem.write8(a, col as u8);
        mem.write8(a + 1, row as u8);
    }

    pub fn set_mode(mem: &mut Memory, mode: u8) {
        let clear = mode & 0x80 == 0;
        let mode = mode & 0x7F;
        let (cols, rows) = match mode {
            0 | 1 => (40, 25),
            4..=6 => (40, 25),
            0x0D => (40, 25),
            0x13 => (40, 25),
            0x12 | 0x11 => (80, 30),
            _ => (80, 25),
        };
        mem.write8(BDA_MODE, mode);
        mem.write16(BDA_COLS, cols);
        mem.write8(BDA_ROWS, rows - 1);
        mem.write16(BDA_PAGE_SIZE, if cols == 40 { 0x800 } else { 0x1000 });
        mem.write16(BDA_PAGE_START, 0);
        mem.write8(BDA_PAGE, 0);
        mem.write8(BDA_CHAR_HEIGHT, if mode == 0x13 { 8 } else { 16 });
        mem.write16(BDA_CURSOR_SHAPE, 0x0607);
        for p in 0..8 {
            Self::set_cursor(mem, p, 0, 0);
        }
        if clear {
            if mode == 0x13 {
                for i in 0..64000u32 {
                    mem.write8(GFX_BASE + i, 0);
                }
            } else {
                for i in 0..0x4000u32 {
                    mem.write16(TEXT_BASE + i * 2, 0x0720);
                }
            }
        }
    }

    fn cell(mem: &Memory, page: u8, col: u16, row: u16) -> u32 {
        Self::page_base(mem, page) + (row as u32 * Self::cols(mem) as u32 + col as u32) * 2
    }

    pub fn read_cell(mem: &Memory, page: u8, col: u16, row: u16) -> (u8, u8) {
        let a = Self::cell(mem, page, col, row);
        (mem.read8(a), mem.read8(a + 1))
    }

    pub fn write_cell(mem: &mut Memory, page: u8, col: u16, row: u16, ch: u8, attr: Option<u8>) {
        if Self::is_text(mem) {
            let a = Self::cell(mem, page, col, row);
            mem.write8(a, ch);
            if let Some(at) = attr {
                mem.write8(a + 1, at);
            }
        } else if Self::mode(mem) == 0x13 {
            Self::draw_glyph(mem, col, row, ch, attr.unwrap_or(15));
        }
    }

    /// Draws a character in mode 13h using the built-in 8x8 font.
    fn draw_glyph(mem: &mut Memory, col: u16, row: u16, ch: u8, color: u8) {
        let glyph = crate::font::glyph8x8(ch);
        for (y, bits) in glyph.iter().enumerate() {
            for x in 0..8 {
                let px = col as u32 * 8 + x;
                let py = row as u32 * 8 + y as u32;
                if px < 320 && py < 200 {
                    let on = bits & (0x80 >> x) != 0;
                    mem.write8(GFX_BASE + py * 320 + px, if on { color } else { 0 });
                }
            }
        }
    }

    /// Scrolls a window up (`lines` > 0) or down; `lines == 0` clears it.
    #[allow(clippy::too_many_arguments)]
    pub fn scroll(mem: &mut Memory, up: bool, lines: u8, attr: u8, top: u16, left: u16, bottom: u16, right: u16) {
        let page = Self::page(mem);
        let cols = Self::cols(mem);
        let rows = Self::rows(mem);
        let bottom = bottom.min(rows - 1);
        let right = right.min(cols - 1);
        if top > bottom || left > right {
            return;
        }
        if Self::mode(mem) == 0x13 {
            Self::scroll_gfx(mem, up, lines, attr, top, bottom);
            return;
        }
        let height = bottom - top + 1;
        let n = if lines == 0 || lines as u16 >= height { height } else { lines as u16 };
        let rows_iter: Vec<u16> = if up { (top..=bottom).collect() } else { (top..=bottom).rev().collect() };
        for &r in &rows_iter {
            let src = if up { r + n } else { r.wrapping_sub(n) };
            let in_window = if up { src <= bottom } else { r >= top + n };
            for c in left..=right {
                let a = Self::cell(mem, page, c, r);
                if in_window {
                    let b = Self::cell(mem, page, c, src);
                    let w = mem.read16(b);
                    mem.write16(a, w);
                } else {
                    mem.write16(a, ((attr as u16) << 8) | 0x20);
                }
            }
        }
    }

    fn scroll_gfx(mem: &mut Memory, up: bool, lines: u8, color: u8, top: u16, bottom: u16) {
        let (y0, y1) = (top as u32 * 8, (bottom as u32 + 1) * 8);
        let n = if lines == 0 { y1 - y0 } else { (lines as u32 * 8).min(y1 - y0) };
        let rows: Vec<u32> = if up { (y0..y1).collect() } else { (y0..y1).rev().collect() };
        for y in rows {
            let src = if up { y + n } else { y.wrapping_sub(n) };
            let valid = if up { src < y1 } else { y >= y0 + n };
            for x in 0..320 {
                let v = if valid { mem.read8(GFX_BASE + src * 320 + x) } else { color };
                mem.write8(GFX_BASE + y * 320 + x, v);
            }
        }
    }

    /// BIOS teletype output (INT 10h/0Eh) on the active page.
    pub fn teletype(mem: &mut Memory, ch: u8, color: u8) {
        let page = Self::page(mem);
        let (mut col, mut row) = Self::cursor(mem, page);
        match ch {
            7 => {}
            8 => col = col.saturating_sub(1),
            b'\n' => row += 1,
            b'\r' => col = 0,
            _ => {
                let attr = if Self::is_text(mem) { None } else { Some(color) };
                return Self::put_char_advance(mem, ch, attr);
            }
        }
        Self::move_cursor(mem, page, col, row);
    }

    /// Writes a character at the cursor of the active page and advances it (wrapping/scrolling).
    pub fn put_char_advance(mem: &mut Memory, ch: u8, attr: Option<u8>) {
        let page = Self::page(mem);
        let (mut col, mut row) = Self::cursor(mem, page);
        Self::write_cell(mem, page, col, row, ch, attr);
        col += 1;
        if col >= Self::cols(mem) {
            col = 0;
            row += 1;
        }
        Self::move_cursor(mem, page, col, row);
    }

    /// Sets the cursor, scrolling the screen up when it moves below the last row.
    fn move_cursor(mem: &mut Memory, page: u8, col: u16, mut row: u16) {
        let cols = Self::cols(mem);
        let rows = Self::rows(mem);
        if row >= rows {
            // New line gets the attribute of the cell at the cursor (BIOS behaviour).
            let attr = if Self::is_text(mem) { Self::read_cell(mem, page, col.min(cols - 1), rows - 1).1 } else { 0 };
            Self::scroll(mem, true, 1, if attr == 0 { 7 } else { attr }, 0, 0, rows - 1, cols - 1);
            row = rows - 1;
        }
        Self::set_cursor(mem, page, col, row);
    }

    pub fn put_pixel(mem: &mut Memory, x: u16, y: u16, color: u8) {
        if Self::mode(mem) == 0x13 && x < 320 && y < 200 {
            mem.write8(GFX_BASE + y as u32 * 320 + x as u32, color);
        }
    }

    pub fn get_pixel(mem: &Memory, x: u16, y: u16) -> u8 {
        if Self::mode(mem) == 0x13 && x < 320 && y < 200 { mem.read8(GFX_BASE + y as u32 * 320 + x as u32) } else { 0 }
    }

    /// Text of the active page (for headless output / tests), trailing spaces trimmed.
    pub fn screen_text(mem: &Memory) -> String {
        let page = Self::page(mem);
        let mut out = String::new();
        for r in 0..Self::rows(mem) {
            let mut line = String::new();
            for c in 0..Self::cols(mem) {
                let (ch, _) = Self::read_cell(mem, page, c, r);
                line.push(crate::font::cp437_to_char(ch));
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}
