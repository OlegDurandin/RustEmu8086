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
//! `syscall` instruction. The floating-point services (2, 3, 6, 7, 43, 44) need coprocessor 1 and
//! the dialog services (50..59) MARS's dialog windows; neither exists yet.

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
    fn c_string(&self, mut addr: u32) -> Result<Vec<u8>, Fail> {
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
            4 => {
                let s = self.c_string(a0)?;
                self.console.write(&s);
                Ok(Done::Continue)
            }
            5 => {
                let Some(line) = self.take_input(None) else { return Ok(Done::Block) };
                let v =
                    parse_java_int(&line).ok_or_else(|| Fail::syscall("invalid integer input (syscall 5)".into()))?;
                self.cpu.set_reg(V0, v as u32);
                Ok(Done::Continue)
            }
            8 => {
                let max = (a1 as i32).saturating_sub(1);
                let (max, add_null) = if max < 0 { (0, false) } else { (max as usize, true) };
                let Some(mut line) = self.take_input(Some(max)) else { return Ok(Done::Block) };
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
                if next >= memory::DATA_LIMIT as u64 {
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
                let Some(line) = self.take_input(Some(1)) else { return Ok(Done::Block) };
                let c = *line.first().ok_or_else(|| Fail::syscall("invalid char input (syscall 12)".into()))?;
                self.cpu.set_reg(V0, c as u32);
                Ok(Done::Continue)
            }
            13 => {
                let name = String::from_utf8_lossy(&self.c_string(a0)?).into_owned();
                self.barrier = true;
                match self.files.open(&name, a1) {
                    Ok(fd) => {
                        self.cpu.set_reg(V0, fd);
                        Ok(Done::Continue)
                    }
                    Err(e) => {
                        self.cpu.set_reg(V0, u32::MAX);
                        Err(Fail::syscall(format!("{e} (syscall 13)")))
                    }
                }
            }
            14 => {
                let len = a2 as i32;
                if len < 0 {
                    self.cpu.set_reg(V0, u32::MAX);
                    return Err(Fail::syscall(format!("negative length {len} (syscall 14)")));
                }
                let data = if a0 == 0 {
                    let Some(d) = self.take_input(Some(len as usize)) else { return Ok(Done::Block) };
                    d
                } else {
                    self.barrier = true;
                    match self.files.read(a0, len as usize) {
                        Ok(d) => d,
                        Err(e) => {
                            self.cpu.set_reg(V0, u32::MAX);
                            return Err(Fail::syscall(format!("{e} (syscall 14)")));
                        }
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
                    self.cpu.set_reg(V0, u32::MAX);
                    return Err(Fail::syscall(format!("negative length {len} (syscall 15)")));
                }
                let data =
                    (0..len as u32).map(|i| self.mem.load8(a1.wrapping_add(i))).collect::<Result<Vec<u8>, _>>()?;
                if a0 == 1 || a0 == 2 {
                    self.console.write(&data);
                    self.cpu.set_reg(V0, len as u32);
                    return Ok(Done::Continue);
                }
                self.barrier = true;
                match self.files.write(a0, &data) {
                    Ok(n) => {
                        self.cpu.set_reg(V0, n as u32);
                        Ok(Done::Continue)
                    }
                    Err(e) => {
                        self.cpu.set_reg(V0, u32::MAX);
                        Err(Fail::syscall(e))
                    }
                }
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
            2 | 3 | 6 | 7 | 43 | 44 => Err(Fail::syscall(format!(
                "syscall {service} works with floating-point registers: coprocessor 1 is not emulated yet"
            ))),
            50..=59 => Err(Fail::syscall(format!("syscall {service} shows a dialog window, which is not supported"))),
            _ => Err(Fail::syscall(format!("invalid or unimplemented syscall service: {service} "))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
