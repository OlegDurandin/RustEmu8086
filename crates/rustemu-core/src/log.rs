//! The emulator log: messages about the program (an unsupported service, a limit of drive C:).
//! A program can call an unsupported service in a loop millions of times, so a message repeated
//! in a row is counted instead of stored again, and at most [`MAX_ENTRIES`] messages are kept
//! (the oldest go). Step back returns the log to a [`Mark`].

use std::collections::VecDeque;

/// Messages kept; older ones are dropped (and counted).
pub const MAX_ENTRIES: usize = 1000;

/// The emulator log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Log {
    /// Messages, oldest first, with how many times each came in a row.
    entries: VecDeque<(String, u64)>,
    /// Messages added since the start (repeats in a row are one).
    total: u64,
    /// Messages dropped from the front.
    dropped: u64,
}

/// The state of the log at a moment, to come back to it (see [`Log::restore`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    total: u64,
    /// Repeats of the last message then.
    last_count: u64,
}

impl Log {
    /// Adds a message (a repeat of the last one only counts it).
    pub fn push(&mut self, message: impl Into<String>) {
        let message = message.into();
        if let Some((last, count)) = self.entries.back_mut()
            && *last == message
        {
            *count = count.saturating_add(1);
            return;
        }
        self.entries.push_back((message, 1));
        self.total += 1;
        if self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
            self.dropped += 1;
        }
    }

    /// The messages as shown: a repeated one ends with `(×N)`; when older ones were dropped, the
    /// first line says how many.
    pub fn lines(&self) -> impl Iterator<Item = String> + '_ {
        let dropped = (self.dropped > 0).then(|| format!("({} older messages not kept)", self.dropped));
        dropped
            .into_iter()
            .chain(self.entries.iter().map(|(m, n)| if *n > 1 { format!("{m} (×{n})") } else { m.clone() }))
    }

    /// No message.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.dropped == 0
    }

    /// Forgets every message.
    pub fn clear(&mut self) {
        *self = Log::default();
    }

    /// The state now.
    pub fn mark(&self) -> Mark {
        Mark { total: self.total, last_count: self.entries.back().map_or(0, |e| e.1) }
    }

    /// Returns to `mark`: removes the messages added since and the repeats counted since. (A
    /// message dropped from the front since then does not come back.)
    pub fn restore(&mut self, mark: Mark) {
        let newer = self.total.saturating_sub(mark.total) as usize;
        let keep = self.entries.len().saturating_sub(newer);
        self.entries.truncate(keep);
        self.total = mark.total.min(self.total);
        if let Some(last) = self.entries.back_mut()
            && mark.last_count > 0
        {
            last.1 = mark.last_count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_are_counted_and_the_log_is_bounded() {
        let mut log = Log::default();
        for _ in 0..1_000_000 {
            log.push("INT 21h/AH=FFh is not supported");
        }
        assert_eq!(log.lines().collect::<Vec<_>>(), ["INT 21h/AH=FFh is not supported (×1000000)"]);
        for i in 0..MAX_ENTRIES + 5 {
            log.push(format!("message {i}"));
        }
        let lines: Vec<String> = log.lines().collect();
        assert_eq!(lines.len(), MAX_ENTRIES + 1);
        assert_eq!(lines[0], "(6 older messages not kept)");
        assert_eq!(lines[1], "message 5");
    }

    #[test]
    fn step_back_restores_the_log() {
        let mut log = Log::default();
        log.push("a");
        log.push("b");
        let mark = log.mark();
        log.push("b");
        log.push("c");
        log.push("c");
        log.restore(mark);
        assert_eq!(log.lines().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(log.mark(), mark);
    }
}
