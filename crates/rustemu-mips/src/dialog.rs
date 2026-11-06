//! MARS's dialog services (`syscall` 50..59): a confirmation, input of an integer, a float, a
//! double or a string, and messages, shown by MARS with Swing's `JOptionPane`.
//!
//! The machine does not draw windows: a dialog service with no answer yet puts the program
//! counter back on its `syscall`, sets [`Machine::dialog`] to what must be shown and reports
//! [`Event::WaitingInput`](rustemu_target::Event::WaitingInput); the interface shows the window
//! and gives the answer to [`Machine::answer_dialog`], and the repeated step takes it, setting
//! the registers and the buffer as MARS does. Without windows (the command line,
//! [`Machine::dialogs_in_console`]) messages go to the console and answers are lines of input.

use crate::machine::Machine;

/// What a dialog asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    /// Yes / No / Cancel (50): the choice in `$a0` (0, 1, 2; −1 when the window is closed).
    Confirm,
    /// An integer (51): `$a0` the value, `$a1` the status.
    InputInt,
    /// A float (52): `$f0` the value, `$a1` the status.
    InputFloat,
    /// A double (53): `$f0`:`$f1` the value, `$a1` the status.
    InputDouble,
    /// A string (54) into the buffer at `$a1` of `$a2` bytes: `$a1` the status.
    InputString,
    /// A message with OK (55..59).
    Message(MessageType),
    /// The input of a reading service (5..8, 12) with MARS's setting "popup dialog for input
    /// syscalls": a text and OK (its window is "MIPS Keyboard Input").
    KeyboardInput,
}

/// The icon of a message (`$a1` of service 55; 56..59 are information).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// 0.
    Error,
    /// 1.
    Information,
    /// 2.
    Warning,
    /// 3.
    Question,
    /// Any other value: no icon.
    Plain,
}

impl MessageType {
    /// The title of its window.
    pub fn title(self) -> &'static str {
        match self {
            MessageType::Error => "Error",
            MessageType::Information => "Information",
            MessageType::Warning => "Warning",
            MessageType::Question => "Question",
            MessageType::Plain => "Message",
        }
    }
}

/// A dialog a program waits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    /// The service (50..59).
    pub service: u32,
    /// What it asks.
    pub kind: DialogKind,
    /// The text: the string at `$a0` (and for 56..59 the value after it, as MARS shows it).
    pub message: String,
}

/// The answer to a dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAnswer {
    /// Yes (confirmation).
    Yes,
    /// No (confirmation).
    No,
    /// Cancel (confirmation, input).
    Cancel,
    /// The window was closed without a choice (confirmation: −1; input: as Cancel).
    Closed,
    /// OK with the text typed (input), or OK of a message.
    Ok(String),
}

/// The text as MARS shows it (it takes each byte as a character; strings here are UTF-8).
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `Integer.parseInt(s)` (no trimming): an optional sign and decimal digits, in range.
fn parse_int(s: &str) -> Option<i32> {
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Status of an input dialog in `$a1`: 0 OK, −1 not a number, −2 cancelled, −3 OK with nothing
/// typed, −4 (string) longer than the buffer.
const OK: i32 = 0;
const NOT_A_NUMBER: i32 = -1;
const CANCELLED: i32 = -2;
const EMPTY: i32 = -3;
const TOO_LONG: i32 = -4;

const A0: usize = 4;
const A1: usize = 5;
const A2: usize = 6;

impl Machine {
    /// The dialog the program waits on (show it, then [`Machine::answer_dialog`]).
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    /// Answers the dialog the program waits on; the next step takes the answer.
    pub fn answer_dialog(&mut self, answer: DialogAnswer) {
        if self.dialog.take().is_some() {
            self.dialog_answer = Some(answer);
        }
    }

    /// What dialog service `service` shows, its text read from memory (MARS reads the strings
    /// before it shows the window: a bad address is an exception then).
    pub(crate) fn dialog_of(&self, service: u32) -> Result<Dialog, crate::memory::AccessError> {
        let (a0, a1) = (self.cpu.reg(A0), self.cpu.reg(A1));
        let message = text(&self.c_string_raw(a0)?);
        let (kind, message) = match service {
            50 => (DialogKind::Confirm, message),
            51 => (DialogKind::InputInt, message),
            52 => (DialogKind::InputFloat, message),
            53 => (DialogKind::InputDouble, message),
            54 => (DialogKind::InputString, message),
            55 => {
                let t = match a1 as i32 {
                    0 => MessageType::Error,
                    1 => MessageType::Information,
                    2 => MessageType::Warning,
                    3 => MessageType::Question,
                    _ => MessageType::Plain,
                };
                (DialogKind::Message(t), message)
            }
            56 => (DialogKind::Message(MessageType::Information), message + &(a1 as i32).to_string()),
            57 => (
                DialogKind::Message(MessageType::Information),
                message + &crate::syscall::java_float(f32::from_bits(self.cpu.fpr[12])),
            ),
            58 => (
                DialogKind::Message(MessageType::Information),
                message + &crate::syscall::java_double(self.cpu.f64(12)),
            ),
            _ => (DialogKind::Message(MessageType::Information), message + &text(&self.c_string_raw(a1)?)),
        };
        Ok(Dialog { service, kind, message })
    }

    /// Sets the results of dialog `d` answered with `answer`, as MARS's services do.
    pub(crate) fn apply_dialog(&mut self, d: &Dialog, answer: DialogAnswer) -> Result<(), crate::memory::AccessError> {
        let typed = match &answer {
            DialogAnswer::Ok(t) => Some(t.as_str()),
            _ => None,
        };
        let status = |t: Option<&str>| match t {
            None => CANCELLED,
            Some("") => EMPTY,
            Some(_) => OK,
        };
        match d.kind {
            DialogKind::Confirm => {
                let choice = match answer {
                    DialogAnswer::Yes => 0,
                    DialogAnswer::No => 1,
                    DialogAnswer::Cancel => 2,
                    _ => -1,
                };
                self.cpu.set_reg(A0, choice as u32);
            }
            DialogKind::InputInt => {
                let (value, s) = match typed {
                    Some(t) if !t.is_empty() => parse_int(t).map_or((0, NOT_A_NUMBER), |v| (v, OK)),
                    t => (0, status(t)),
                };
                self.cpu.set_reg(A0, value as u32);
                self.cpu.set_reg(A1, s as u32);
            }
            DialogKind::InputFloat => {
                self.cpu.fpr[0] = 0;
                let s = match typed {
                    Some(t) if !t.is_empty() => match crate::java_float::parse_float(t.as_bytes()) {
                        Some(v) => {
                            self.cpu.fpr[0] = crate::fpu::f32_bits(v);
                            OK
                        }
                        None => NOT_A_NUMBER,
                    },
                    t => status(t),
                };
                self.cpu.set_reg(A1, s as u32);
            }
            DialogKind::InputDouble => {
                (self.cpu.fpr[0], self.cpu.fpr[1]) = (0, 0);
                let s = match typed {
                    Some(t) if !t.is_empty() => match crate::java_float::parse_double(t.as_bytes()) {
                        Some(v) => {
                            let b = crate::fpu::f64_bits(v);
                            (self.cpu.fpr[0], self.cpu.fpr[1]) = (b as u32, (b >> 32) as u32);
                            OK
                        }
                        None => NOT_A_NUMBER,
                    },
                    t => status(t),
                };
                self.cpu.set_reg(A1, s as u32);
            }
            DialogKind::InputString => {
                let (buf, max) = (self.cpu.reg(A1), self.cpu.reg(A2) as i32);
                let s = match typed {
                    Some(t) if !t.is_empty() => {
                        // MARS's loop, in its int arithmetic (the length in bytes here: UTF-8).
                        let b = t.as_bytes();
                        let len = b.len() as i32;
                        let at = |i: i32| buf.wrapping_add(i as u32);
                        for (i, &c) in b.iter().enumerate().take(max.saturating_sub(1).max(0) as usize) {
                            self.mem.store8(at(i as i32), c)?;
                        }
                        if len < max - 1 {
                            self.mem.store8(at(len.min(max - 2)), b'\n')?;
                        }
                        self.mem.store8(at((len + 1).min(max - 1)), 0)?;
                        if len > max - 1 { TOO_LONG } else { OK }
                    }
                    t => status(t),
                };
                self.cpu.set_reg(A1, s as u32);
            }
            DialogKind::Message(_) | DialogKind::KeyboardInput => {}
        }
        Ok(())
    }

    /// A dialog without windows: a message goes to the console; a question is answered by a
    /// line of input (echoed after the text): `y`/`yes`/`0` Yes, `n`/`no`/`1` No, anything else
    /// Cancel; an input dialog takes the line as typed. None while no line is typed.
    pub(crate) fn console_dialog(&mut self, d: &Dialog) -> Option<DialogAnswer> {
        let message = match d.kind {
            DialogKind::Message(_) => {
                self.console.write(format!("{}\n", d.message).as_bytes());
                return Some(DialogAnswer::Ok(String::new()));
            }
            _ => format!("{} ", d.message),
        };
        let n = self.input.iter().position(|&b| b == b'\n')? + 1;
        let line: Vec<u8> = self.input.drain(..n).collect();
        self.console.write(message.as_bytes());
        self.console.write(&line);
        let line = text(&line[..line.len() - 1]);
        Some(match d.kind {
            DialogKind::Confirm => match line.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" | "0" => DialogAnswer::Yes,
                "n" | "no" | "1" => DialogAnswer::No,
                _ => DialogAnswer::Cancel,
            },
            _ => DialogAnswer::Ok(line),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_as_java_parses_them() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("-2147483648"), Some(i32::MIN));
        assert_eq!(parse_int(" 42"), None, "the dialog does not trim");
        assert_eq!(parse_int("2147483648"), None);
    }
}
