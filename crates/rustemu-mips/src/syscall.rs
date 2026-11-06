//! MARS's system services: `li $v0, n` / `syscall`, arguments in `$a0`..`$a3`, results in `$v0`
//! (and `$a0`, `$a1` for time and random numbers).
//!
//! | `$v0` | service |
//! |---|---|
//! | 1, 4, 11, 34, 35, 36 | print an integer, a string (zero-terminated at `$a0`), a character, an integer in hex, binary, unsigned |
//! | 5, 8, 12 | read an integer, a string (`$a0` buffer, `$a1` its size), a character |
//! | 9 | `sbrk`: `$a0` bytes of heap, the address in `$v0` |
//! | 10, 17 | exit; exit with the code in `$a0` |
//! | 13, 14, 15, 16 | open (`$a0` name, `$a1` flags 0 / 1 / 9), read, write (`$a0` descriptor, `$a1` buffer, `$a2` length), close |
//! | 30, 32 | time in milliseconds (`$a0` low, `$a1` high); sleep `$a0` milliseconds |
//! | 31, 33 | MIDI (no sound here; 33 waits for the duration in `$a1`) |
//! | 40, 41, 42 | random: set the seed `$a1` of stream `$a0`; an integer; an integer below `$a1` (into `$a0`) |
//!
//! Input is read as in MARS's Run I/O pane: what is typed is echoed to the console, and a read
//! ends with Enter or when the requested number of characters is typed (one for 12). Errors stop
//! the program with MARS's messages (`invalid integer input (syscall 5)`), as exceptions of the
//! `syscall` instruction — except the errors of the file services (13, 14, 15): as in MARS, they
//! return −1 in `$v0` and the program goes on (it is its job to check); only a bad address of the
//! name or the buffer raises an exception. Floating-point services (2, 3, 6, 7, 43, 44) use `$f12`
//! and `$f0` and print numbers as Java does (`1.0E7`). The dialog services (50..59) wait for the
//! interface to show the window and answer (see [`crate::dialog`]).

use rustemu_target::Event;

use crate::cpu::{ExcCode, V0};
use crate::files::MAX_FILES;
use crate::machine::{Finish, Machine};
use crate::memory::{self, Access, AccessError};
use crate::random::JavaRandom;

const A0: usize = 4;
const A1: usize = 5;
const A2: usize = 6;

/// How a service ended.
enum Done {
    /// Done; the program goes on.
    Continue,
    /// No complete input yet: the `syscall` waits.
    Block,
    /// Wait this many milliseconds.
    Sleep(u64),
    /// The program ends.
    Exit(Finish),
}

/// A failed service: the exception it raises.
struct Fail {
    code: ExcCode,
    message: String,
    bad: Option<u32>,
}

impl Fail {
    fn syscall(message: String) -> Fail {
        Fail { code: ExcCode::Syscall, message, bad: None }
    }
}

impl From<AccessError> for Fail {
    fn from(e: AccessError) -> Fail {
        let code = match e.access {
            Access::Load => ExcCode::AddressLoad,
            Access::Store => ExcCode::AddressStore,
        };
        Fail { code, message: e.message, bad: Some(e.addr) }
    }
}

/// `Float.toString` / `Double.toString` of Java (19 and later): the shortest digits that give the
/// value back, as `1.0`, `0.001` or `1.0E7`, `1.2E-5` (scientific outside `1e-3..1e7`).
fn java_number(digits_exp: String, negative: bool, plain: bool) -> String {
    // `digits_exp` is Rust's shortest scientific form: `1.2345e-5`.
    let (mantissa, exp) = digits_exp.split_once('e').unwrap_or((&digits_exp, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    let sign = if negative { "-" } else { "" };
    if plain {
        let point = exp + 1;
        let (int, frac) = if point <= 0 {
            ("0".to_string(), format!("{}{digits}", "0".repeat((-point) as usize)))
        } else if point as usize >= digits.len() {
            (format!("{digits}{}", "0".repeat(point as usize - digits.len())), String::new())
        } else {
            (digits[..point as usize].to_string(), digits[point as usize..].to_string())
        };
        let frac = if frac.is_empty() { "0".to_string() } else { frac };
        format!("{sign}{int}.{frac}")
    } else {
        let rest = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{sign}{}.{rest}E{exp}", &digits[..1])
    }
}

/// Java's `Float.toString`.
pub fn java_float(x: f32) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let a = x.abs();
    java_number(format!("{a:e}"), x < 0.0, (1e-3..1e7).contains(&a))
}

/// Java's `Double.toString`.
pub fn java_double(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let a = x.abs();
    java_number(format!("{a:e}"), x < 0.0, (1e-3..1e7).contains(&a))
}

/// `Integer.parseInt(text.trim())`: optional sign, decimal digits, in range.
fn parse_java_int(text: &[u8]) -> Option<i32> {
    let t = std::str::from_utf8(text).ok()?.trim_matches(|c: char| c <= ' ');
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

impl Machine {
    /// Performs the service in `$v0` for the `syscall` just executed (the program counter is past it).
    pub(crate) fn syscall(&mut self) -> Event {
        let at = self.cpu.pc.wrapping_sub(4);
        let service = self.cpu.reg(V0);
        match self.service(service) {
            Ok(Done::Continue) => {
                self.waiting = false;
                Event::Continue
            }
            Ok(Done::Block) => {
                self.cpu.pc = at;
                self.cpu.instr_count -= 1;
                self.waiting = true;
                Event::WaitingInput
            }
            Ok(Done::Sleep(ms)) => {
                self.waiting = false;
                if self.instant_waits || ms == 0 {
                    return Event::Continue;
                }
                self.sleep_until = Some(web_time::Instant::now() + std::time::Duration::from_millis(ms));
                Event::Sleeping
            }
            Ok(Done::Exit(f)) => {
                self.waiting = false;
                self.finished = Some(f);
                Event::Terminated(f.code())
            }
            Err(f) => {
                self.waiting = false;
                let e = self.cpu.raise(&self.mem, f.code, f.message, at, f.bad, false);
                if e.handled { Event::Continue } else { Event::Error(e.report()) }
            }
        }
    }

    fn arg(&self, r: usize) -> u32 {
        self.cpu.reg(r)
    }

    /// The zero-terminated string at `addr`.
    fn c_string(&self, addr: u32) -> Result<Vec<u8>, Fail> {
        Ok(self.c_string_raw(addr)?)
    }

    /// The zero-terminated string at `addr`, read as the program would (an address error stops).
    pub(crate) fn c_string_raw(&self, mut addr: u32) -> Result<Vec<u8>, AccessError> {
        let mut s = Vec::new();
        loop {
            match self.mem.load8(addr)? {
                0 => return Ok(s),
                b => s.push(b),
            }
            addr = addr.wrapping_add(1);
        }
    }

    /// Takes typed input the way MARS's Run I/O pane hands it over: up to and including Enter, or
    /// `max` bytes (None: no limit) when that many were typed first. None until then. What is
    /// taken is echoed to the console.
    fn take_input(&mut self, max: Option<usize>) -> Option<Vec<u8>> {
        let limit = max.unwrap_or(usize::MAX);
        if limit == 0 {
            return Some(Vec::new());
        }
        let n = match self.input.iter().position(|&b| b == b'\n') {
            Some(i) if i < limit => i + 1,
            _ if self.input.len() >= limit => limit,
            _ => return None,
        };
        let line: Vec<u8> = self.input.drain(..n).collect();
        self.console.write(&line);
        Some(line)
    }

    /// Input for service `service`: from the Run I/O console ([`Machine::take_input`]), or with
    /// MARS's setting "popup dialog for input syscalls" from its "MIPS Keyboard Input" window
    /// (asking `prompt`; the console then shows `**** user input : text`). None until there.
    fn read_input(&mut self, service: u32, max: Option<usize>, prompt: &str) -> Option<Vec<u8>> {
        if !self.settings.popup_input || self.dialogs_in_console {
            return self.take_input(max);
        }
        let text = match self.dialog_answer.take() {
            Some(crate::dialog::DialogAnswer::Ok(t)) => t,
            // Swing hands over its "no value" object, a string.
            Some(_) => "uninitializedValue".to_string(),
            None => {
                let kind = crate::dialog::DialogKind::KeyboardInput;
                self.dialog = Some(crate::dialog::Dialog { service, kind, message: prompt.to_string() });
                return None;
            }
        };
        self.console.write(format!("**** user input : {text}\n").as_bytes());
        Some(text.into_bytes())
    }

    /// The result of a file service in `$v0`: the value, or −1 for an error (whose message is
    /// kept in [`Machine::file_error`], as MARS keeps it for its own use). The program goes on.
    fn file_result(&mut self, r: Result<i32, String>) -> Result<Done, Fail> {
        let v = r.unwrap_or_else(|e| {
            self.file_error = Some(e);
            -1
        });
        self.cpu.set_reg(V0, v as u32);
        Ok(Done::Continue)
    }

    fn print(&mut self, text: &str) -> Result<Done, Fail> {
        self.console.write(text.as_bytes());
        Ok(Done::Continue)
    }

    /// A random stream; a stream used before its seed is set is seeded from the clock, as
    /// `new Random()`.
    fn stream(&mut self, id: i32) -> &mut JavaRandom {
        let clock = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64);
        let n = self.random.len() as i64;
        self.random.entry(id).or_insert_with(|| JavaRandom::new(clock ^ n.wrapping_mul(0x2545_F491_4F6C_DD1D)))
    }

    fn service(&mut self, service: u32) -> Result<Done, Fail> {
        let (a0, a1, a2) = (self.arg(A0), self.arg(A1), self.arg(A2));
        match service {
            1 => self.print(&(a0 as i32).to_string()),
            2 => self.print(&java_float(f32::from_bits(self.cpu.fpr[12]))),
            3 => self.print(&java_double(self.cpu.f64(12))),
            6 => {
                let Some(line) = self.read_input(6, None, "Enter a float value (syscall 6)") else {
                    return Ok(Done::Block);
                };
                let v: f32 = crate::java_float::parse_float(&line)
                    .ok_or_else(|| Fail::syscall("invalid float input (syscall 6)".into()))?;
                self.cpu.fpr[0] = v.to_bits();
                Ok(Done::Continue)
            }
            7 => {
                let Some(line) = self.read_input(7, None, "Enter a double value (syscall 7)") else {
                    return Ok(Done::Block);
                };
                let v: f64 = crate::java_float::parse_double(&line)
                    .ok_or_else(|| Fail::syscall("invalid double input (syscall 7)".into()))?;
                let b = v.to_bits();
                (self.cpu.fpr[0], self.cpu.fpr[1]) = (b as u32, (b >> 32) as u32);
                Ok(Done::Continue)
            }
            4 => {
                let s = self.c_string(a0)?;
                self.console.write(&s);
                Ok(Done::Continue)
            }
            5 => {
                let Some(line) = self.read_input(5, None, "Enter an integer value (syscall 5)") else {
                    return Ok(Done::Block);
                };
                let v =
                    parse_java_int(&line).ok_or_else(|| Fail::syscall("invalid integer input (syscall 5)".into()))?;
                self.cpu.set_reg(V0, v as u32);
                Ok(Done::Continue)
            }
            8 => {
                let max = (a1 as i32).saturating_sub(1);
                let (max, add_null) = if max < 0 { (0, false) } else { (max as usize, true) };
                let prompt = format!("Enter a string of maximum length {max} (syscall 8)");
                let Some(mut line) = self.read_input(8, Some(max), &prompt) else { return Ok(Done::Block) };
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                line.truncate(max);
                let mut out = line;
                if out.len() < max {
                    out.push(b'\n');
                }
                if add_null {
                    out.push(0);
                }
                for (i, &b) in out.iter().enumerate() {
                    self.mem.store8(a0.wrapping_add(i as u32), b)?;
                }
                Ok(Done::Continue)
            }
            9 => {
                let n = a0 as i32;
                if n < 0 {
                    return Err(Fail::syscall(format!("request ({n}) is negative heap amount (syscall 9)")));
                }
                let next = (self.heap as u64 + n as u64).next_multiple_of(4);
                if next >= self.mem.layout.data_limit as u64 {
                    return Err(Fail::syscall(format!("request ({n}) exceeds available heap storage (syscall 9)")));
                }
                self.cpu.set_reg(V0, self.heap);
                self.heap = next as u32;
                Ok(Done::Continue)
            }
            10 => Ok(Done::Exit(Finish::Exit(0))),
            11 => {
                self.console.write(&[a0 as u8]);
                Ok(Done::Continue)
            }
            12 => {
                let Some(line) = self.read_input(12, Some(1), "Enter a character value (syscall 12)") else {
                    return Ok(Done::Block);
                };
                let c = *line.first().ok_or_else(|| Fail::syscall("invalid char input (syscall 12)".into()))?;
                self.cpu.set_reg(V0, c as u32);
                Ok(Done::Continue)
            }
            13 => {
                let name = String::from_utf8_lossy(&self.c_string(a0)?).into_owned();
                self.barrier = true;
                let fd = self.files.open(&name, a1);
                self.file_result(fd.map(|fd| fd as i32))
            }
            14 => {
                let len = a2 as i32;
                if len < 0 {
                    return self.file_result(Err(format!("negative length {len}")));
                }
                let data = if a0 == 0 {
                    let Some(d) = self.take_input(Some(len as usize)) else { return Ok(Done::Block) };
                    d
                } else {
                    self.barrier = true;
                    match self.files.read(a0, len as usize) {
                        Ok(d) => d,
                        Err(e) => return self.file_result(Err(e)),
                    }
                };
                for (i, &b) in data.iter().enumerate() {
                    self.mem.store8(a1.wrapping_add(i as u32), b)?;
                }
                self.cpu.set_reg(V0, data.len() as u32);
                Ok(Done::Continue)
            }
            15 => {
                let len = a2 as i32;
                if len < 0 {
                    return self.file_result(Err(format!("negative length {len}")));
                }
                let data =
                    (0..len as u32).map(|i| self.mem.load8(a1.wrapping_add(i))).collect::<Result<Vec<u8>, _>>()?;
                if a0 == 1 || a0 == 2 {
                    self.console.write(&data);
                    self.cpu.set_reg(V0, len as u32);
                    return Ok(Done::Continue);
                }
                self.barrier = true;
                let n = self.files.write(a0, &data);
                self.file_result(n.map(|n| n as i32))
            }
            16 => {
                if (3..MAX_FILES as u32).contains(&a0) {
                    self.barrier = true;
                }
                self.files.close(a0);
                Ok(Done::Continue)
            }
            17 => Ok(Done::Exit(Finish::Exit(a0 as i32))),
            30 => {
                let ms = self.fixed_time.unwrap_or_else(|| {
                    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
                });
                self.cpu.set_reg(A0, ms as u32);
                self.cpu.set_reg(A1, (ms >> 32) as u32);
                Ok(Done::Continue)
            }
            31 => Ok(Done::Continue),
            32 => Ok(Done::Sleep((a0 as i32).max(0) as u64)),
            33 => {
                // MARS plays the tone and waits for it (1000 ms for a negative duration).
                let ms = if (a1 as i32) < 0 { 1000 } else { a1 as u64 };
                Ok(Done::Sleep(ms))
            }
            34 => self.print(&memory::hex(a0)),
            35 => self.print(&format!("{a0:032b}")),
            36 => self.print(&a0.to_string()),
            40 => {
                self.stream(a0 as i32).set_seed(a1 as i32 as i64);
                Ok(Done::Continue)
            }
            41 => {
                let v = self.stream(a0 as i32).next_int();
                self.cpu.set_reg(A0, v as u32);
                Ok(Done::Continue)
            }
            42 => {
                let v = self
                    .stream(a0 as i32)
                    .next_int_below(a1 as i32)
                    .ok_or_else(|| Fail::syscall("Upper bound of range cannot be negative (syscall 42)".into()))?;
                self.cpu.set_reg(A0, v as u32);
                Ok(Done::Continue)
            }
            43 => {
                let v = self.stream(a0 as i32).next_float();
                self.cpu.fpr[0] = crate::fpu::f32_bits(v);
                Ok(Done::Continue)
            }
            44 => {
                let b = crate::fpu::f64_bits(self.stream(a0 as i32).next_double());
                (self.cpu.fpr[0], self.cpu.fpr[1]) = (b as u32, (b >> 32) as u32);
                Ok(Done::Continue)
            }
            50..=59 => {
                let d = self.dialog_of(service)?;
                let answer = match self.dialog_answer.take() {
                    Some(a) => a,
                    None if self.dialogs_in_console => match self.console_dialog(&d) {
                        Some(a) => a,
                        None => return Ok(Done::Block),
                    },
                    None => {
                        self.dialog = Some(d);
                        return Ok(Done::Block);
                    }
                };
                self.apply_dialog(&d, answer)?;
                Ok(Done::Continue)
            }
            _ => Err(Fail::syscall(format!("invalid or unimplemented syscall service: {service} "))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_floats() {
        let d = |x: f64| java_double(x);
        assert_eq!(
            [d(1.0), d(0.1), d(-2.5), d(1e7), d(1.5e-5), d(0.001), d(1234567.0)],
            ["1.0", "0.1", "-2.5", "1.0E7", "1.5E-5", "0.001", "1234567.0"]
        );
        assert_eq!(
            [d(f64::NAN), d(f64::INFINITY), d(-0.0), d(123456789.0)],
            ["NaN", "Infinity", "-0.0", "1.23456789E8"]
        );
        assert_eq!([java_float(0.1), java_float(1.0 / 3.0), java_float(3e10)], ["0.1", "0.33333334", "3.0E10"]);
    }

    #[test]
    fn java_integers() {
        assert_eq!(parse_java_int(b" 42\n"), Some(42));
        assert_eq!(parse_java_int(b"-7"), Some(-7));
        assert_eq!(parse_java_int(b"+7"), Some(7));
        assert_eq!(parse_java_int(b"2147483648"), None);
        assert_eq!(parse_java_int(b"-2147483648"), Some(i32::MIN));
        assert_eq!(parse_java_int(b"12a"), None);
        assert_eq!(parse_java_int(b"\n"), None);
        assert_eq!(parse_java_int(b"0x10"), None);
    }
}
