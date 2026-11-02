//! The text console of the MIPS machine (MARS's "Run I/O" pane): what the program prints and the
//! input it reads, in place of the PC's video memory.
//!
//! The console keeps bytes. They are shown as UTF-8 where they form valid UTF-8 (strings of a
//! program written in UTF-8) and byte by byte as Latin-1 otherwise (MARS shows each byte as a
//! character). Only the last [`CONSOLE_LIMIT`] bytes are kept: a program printing forever does
//! not fill the memory.

/// Bytes the console keeps (older output is dropped).
pub const CONSOLE_LIMIT: usize = 1 << 20;

/// What the program has printed and the input it has read (echoed as typed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Console {
    /// The kept output: the last bytes of everything written.
    bytes: Vec<u8>,
    /// Bytes dropped from the front.
    dropped: usize,
}

impl Console {
    /// An empty console.
    pub fn new() -> Console {
        Console::default()
    }

    /// Appends `bytes` to the console.
    pub fn write(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
        // Dropped in large pieces, so that output does not shift on every write.
        if self.bytes.len() > CONSOLE_LIMIT + CONSOLE_LIMIT / 4 {
            let cut = self.bytes.len() - CONSOLE_LIMIT;
            self.bytes.drain(..cut);
            self.dropped += cut;
        }
    }

    /// Number of bytes written since the console was cleared (including dropped ones).
    pub fn len(&self) -> usize {
        self.dropped + self.bytes.len()
    }

    /// Nothing was written.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The kept bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Number of old bytes no longer kept.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// Goes back to the state when [`Console::len`] was `len` (step back).
    pub fn truncate(&mut self, len: usize) {
        if len <= self.dropped {
            self.bytes.clear();
            self.dropped = len;
        } else {
            self.bytes.truncate(len - self.dropped);
        }
    }

    /// Empties the console.
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.dropped = 0;
    }

    /// The kept output as text (see the module documentation).
    pub fn text(&self) -> String {
        text(&self.bytes)
    }
}

/// Bytes as text: valid UTF-8 as it is, other bytes as Latin-1 characters.
pub fn text(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        s.push_str(chunk.valid());
        s.extend(chunk.invalid().iter().map(|&b| b as char));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_end_and_steps_back() {
        let mut c = Console::new();
        c.write(b"hello ");
        let at = c.len();
        c.write(b"world");
        assert_eq!(c.text(), "hello world");
        c.truncate(at);
        assert_eq!(c.text(), "hello ");
        c.write(&vec![b'x'; CONSOLE_LIMIT * 2]);
        assert!(c.bytes().len() <= CONSOLE_LIMIT + CONSOLE_LIMIT / 4);
        assert_eq!(c.len(), 6 + CONSOLE_LIMIT * 2);
        c.truncate(3);
        assert_eq!((c.len(), c.bytes().len()), (3, 0));
    }

    #[test]
    fn utf8_or_latin1() {
        assert_eq!(text("Привет".as_bytes()), "Привет");
        assert_eq!(text(&[b'A', 0xE9, b'B']), "AéB");
    }
}
