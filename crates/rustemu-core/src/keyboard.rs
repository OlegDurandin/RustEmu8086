//! PC keyboard scan codes (set 1) for characters and special keys.

/// Scan code of the key that produces ASCII character `c` on a US layout.
pub fn scan_code_for(c: u8) -> u8 {
    const ROW1: &[u8] = b"1234567890-=";
    const ROW2: &[u8] = b"qwertyuiop[]";
    const ROW3: &[u8] = b"asdfghjkl;'`";
    const ROW4: &[u8] = b"\\zxcvbnm,./";
    const ROW1S: &[u8] = b"!@#$%^&*()_+";
    const ROW2S: &[u8] = b"QWERTYUIOP{}";
    const ROW3S: &[u8] = b"ASDFGHJKL:\"~";
    const ROW4S: &[u8] = b"|ZXCVBNM<>?";
    let find = |row: &[u8], base: u8| row.iter().position(|&x| x == c).map(|i| base + i as u8);
    match c {
        27 => 0x01,
        8 => 0x0E,
        9 => 0x0F,
        13 | 10 => 0x1C,
        b' ' => 0x39,
        1..=26 => scan_code_for(c + b'a' - 1),
        _ => find(ROW1, 0x02)
            .or_else(|| find(ROW1S, 0x02))
            .or_else(|| find(ROW2, 0x10))
            .or_else(|| find(ROW2S, 0x10))
            .or_else(|| find(ROW3, 0x1E))
            .or_else(|| find(ROW3S, 0x1E))
            .or_else(|| find(ROW4, 0x2B))
            .or_else(|| find(ROW4S, 0x2B))
            .unwrap_or(0),
    }
}

/// Extended (non-ASCII) keys: scan codes with ASCII 0.
pub mod keys {
    /// F1 (F2..F10 follow: F1 + n).
    pub const F1: u8 = 0x3B;
    /// F10.
    pub const F10: u8 = 0x44;
    /// Home.
    pub const HOME: u8 = 0x47;
    /// Up arrow.
    pub const UP: u8 = 0x48;
    /// Page Up.
    pub const PGUP: u8 = 0x49;
    /// Left arrow.
    pub const LEFT: u8 = 0x4B;
    /// Right arrow.
    pub const RIGHT: u8 = 0x4D;
    /// End.
    pub const END: u8 = 0x4F;
    /// Down arrow.
    pub const DOWN: u8 = 0x50;
    /// Page Down.
    pub const PGDN: u8 = 0x51;
    /// Insert.
    pub const INS: u8 = 0x52;
    /// Delete.
    pub const DEL: u8 = 0x53;
}
