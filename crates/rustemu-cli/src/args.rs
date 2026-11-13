//! Typed CLI options. Option values are checked before I/O; architecture checks precede assembly and execution.

use rustemu_session::Arch;
use std::{collections::BTreeSet, path::PathBuf};

/// The three CLI operations, each with its own supported options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Asm,
    Run,
    Disasm,
}

/// A validated command invocation; defaults apply only to omitted options.
#[derive(Debug)]
pub struct Args {
    pub file: PathBuf,
    pub arch: Option<Arch>,
    pub syntax: Option<rustemu_asm::Dialect>,
    pub out: Option<PathBuf>,
    pub listing: bool,
    pub symbols: bool,
    pub input: Option<String>,
    pub keys: String,
    pub dos_args: String,
    pub max: u64,
    pub regs: bool,
    pub screen: bool,
    pub trace: bool,
    pub timer_irq: bool,
    pub org: Option<u32>,
    pub mars: bool,
    pub mips: rustemu_mips::Settings,
    pub project: bool,
    pub exception_handler: Option<PathBuf>,
    pub program_args: String,
    seen: BTreeSet<String>,
}

impl Args {
    /// Parses options and one file. None requests help, without running a command.
    pub fn parse(command: Command, tokens: &[String]) -> Result<Option<Self>, String> {
        let mut args = Self {
            file: PathBuf::new(),
            arch: None,
            syntax: None,
            out: None,
            listing: false,
            symbols: false,
            input: None,
            keys: String::new(),
            dos_args: String::new(),
            max: 100_000_000,
            regs: false,
            screen: false,
            trace: false,
            timer_irq: false,
            org: None,
            mars: false,
            mips: Default::default(),
            project: false,
            exception_handler: None,
            program_args: String::new(),
            seen: BTreeSet::new(),
        };
        let mut file = None;
        let mut positional = false;
        let mut it = tokens.iter();
        while let Some(token) = it.next() {
            if !positional && token == "--" {
                positional = true;
                continue;
            }
            if positional || !token.starts_with('-') || token == "-" {
                if file.replace(PathBuf::from(token)).is_some() {
                    return Err(format!("unexpected argument '{token}': expected exactly one file"));
                }
                continue;
            }
            let (name, inline) = token.split_once('=').map_or((token.as_str(), None), |(n, v)| (n, Some(v)));
            if matches!(name, "--help" | "-h") && inline.is_none() {
                return Ok(None);
            }
            let takes_value = match name {
                "--arch" => true,
                "--syntax" | "-o" if command == Command::Asm => true,
                "-l" | "-s" if command == Command::Asm => false,
                "--input" | "--keys" | "--args" | "--max" | "--program-args" if command == Command::Run => true,
                "--regs" | "--screen" | "--trace" | "--timer-irq" if command == Command::Run => false,
                "--org" if command == Command::Disasm => true,
                "--mars" if command == Command::Disasm => false,
                "--memory" | "--exception-handler" if command != Command::Disasm => true,
                "--delayed-branching"
                | "--self-modifying"
                | "--start-at-main"
                | "--no-pseudo"
                | "--warnings-are-errors"
                | "--project"
                    if command != Command::Disasm =>
                {
                    false
                }
                _ => return Err(format!("unknown or unsupported option '{name}' for {command:?}")),
            };
            if !args.seen.insert(name.into()) {
                return Err(format!("duplicate option '{name}'"));
            }
            let value = if takes_value {
                match inline {
                    Some(v) => v,
                    None => it.next().filter(|v| !v.starts_with('-')).map(String::as_str).ok_or_else(|| {
                        format!("missing value for {name}; use {name}=VALUE for a value starting with '-'")
                    })?,
                }
            } else {
                if inline.is_some() {
                    return Err(format!("option '{name}' does not take a value"));
                }
                ""
            };
            match name {
                "--arch" => {
                    args.arch = Some(match value.to_ascii_lowercase().as_str() {
                        "8086" | "x86" | "i8086" => Arch::I8086,
                        "mips" | "mips32" => Arch::Mips32,
                        _ => return Err(format!("invalid --arch '{value}': expected 8086 or mips")),
                    })
                }
                "--syntax" => {
                    args.syntax = Some(match value.to_ascii_lowercase().as_str() {
                        "nasm" => rustemu_asm::Dialect::Nasm,
                        "masm" | "emu8086" => rustemu_asm::Dialect::Masm,
                        _ => return Err(format!("invalid --syntax '{value}': expected nasm or masm")),
                    })
                }
                "-o" => {
                    if value.is_empty() {
                        return Err("-o requires a nonempty output path".into());
                    }
                    args.out = Some(value.into());
                }
                "-l" => args.listing = true,
                "-s" => args.symbols = true,
                "--input" => args.input = Some(value.into()),
                "--keys" => args.keys = value.into(),
                "--args" => args.dos_args = value.into(),
                "--max" => {
                    args.max = value
                        .parse()
                        .map_err(|_| format!("invalid --max '{value}': expected an unsigned decimal u64"))?
                }
                "--regs" => args.regs = true,
                "--screen" => args.screen = true,
                "--trace" => args.trace = true,
                "--timer-irq" => args.timer_irq = true,
                "--org" => args.org = Some(parse_org(value)?),
                "--mars" => args.mars = true,
                "--memory" => {
                    args.mips.memory = rustemu_mips::MemoryConfig::from_id(value).ok_or_else(|| {
                        format!("invalid --memory '{value}': expected Default, CompactDataAtZero or CompactTextAtZero")
                    })?
                }
                "--delayed-branching" => args.mips.delayed_branching = true,
                "--self-modifying" => args.mips.self_modifying = true,
                "--start-at-main" => args.mips.start_at_main = true,
                "--no-pseudo" => args.mips.pseudo_instructions = false,
                "--warnings-are-errors" => args.mips.warnings_are_errors = true,
                "--project" => args.project = true,
                "--exception-handler" => {
                    if value.is_empty() {
                        return Err("--exception-handler requires a nonempty path".into());
                    }
                    args.exception_handler = Some(value.into());
                }
                "--program-args" => args.program_args = value.into(),
                _ => unreachable!("validated option"),
            }
        }
        args.file = file.ok_or("missing input file")?;
        Ok(Some(args))
    }

    /// Rejects options that would otherwise be silently ignored by the selected machine.
    /// Package assembly configuration is authoritative; execution controls and input may vary.
    pub fn validate_portable(&self, arch: Arch) -> Result<(), String> {
        self.validate_arch(arch)?;
        if self.arch.is_some_and(|a| a != arch) {
            return Err("--arch conflicts with project architecture".into());
        }
        for name in &self.seen {
            if matches!(
                name.as_str(),
                "--syntax"
                    | "--args"
                    | "--program-args"
                    | "--memory"
                    | "--delayed-branching"
                    | "--self-modifying"
                    | "--start-at-main"
                    | "--no-pseudo"
                    | "--warnings-are-errors"
                    | "--project"
                    | "--exception-handler"
            ) {
                return Err(format!("option '{name}' overrides packaged settings; edit the project instead"));
            }
        }
        Ok(())
    }

    pub fn validate_arch(&self, arch: Arch) -> Result<(), String> {
        for name in &self.seen {
            let supported = match name.as_str() {
                "--syntax" | "--args" | "--screen" | "--timer-irq" => arch == Arch::I8086,
                "--keys"
                | "--program-args"
                | "--mars"
                | "--memory"
                | "--delayed-branching"
                | "--self-modifying"
                | "--start-at-main"
                | "--no-pseudo"
                | "--warnings-are-errors"
                | "--project"
                | "--exception-handler" => arch == Arch::Mips32,
                _ => true,
            };
            if !supported {
                return Err(format!("option '{name}' is not supported for {}", arch.name()));
            }
        }
        if arch == Arch::I8086 && self.org.is_some_and(|v| v > u16::MAX as u32) {
            return Err("--org exceeds the 8086 16-bit offset range (FFFFh)".into());
        }
        Ok(())
    }
}

/// Hexadecimal origin, with optional 0x prefix and/or h suffix, checked to fit a MIPS address.
fn parse_org(value: &str) -> Result<u32, String> {
    let digits = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")).unwrap_or(value);
    let digits = digits.strip_suffix(['h', 'H']).unwrap_or(digits);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("invalid --org '{value}': expected a hexadecimal address"));
    }
    u32::from_str_radix(digits, 16).map_err(|_| format!("invalid --org '{value}': address exceeds FFFFFFFFh"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(command: Command, text: &[&str]) -> Result<Args, String> {
        Args::parse(command, &text.iter().map(|s| (*s).into()).collect::<Vec<_>>()).map(|v| v.unwrap())
    }
    #[test]
    fn values_are_not_rescanned_as_flags() {
        let a = parse(Command::Run, &["a.s", "--input=--trace", "--program-args=--regs", "--keys="]).unwrap();
        assert_eq!(a.input.as_deref(), Some("--trace"));
        assert!(!a.trace);
        assert!(!a.regs);
        assert_eq!(a.program_args, "--regs");
        assert_eq!(a.keys, "");
    }
    #[test]
    fn explicit_defaults_and_boundaries_are_valid() {
        let a = parse(Command::Run, &["--max", "18446744073709551615", "a.s", "--arch", "MIPS32"]).unwrap();
        assert_eq!(a.max, u64::MAX);
        assert_eq!(a.arch, Some(Arch::Mips32));
        assert_eq!(parse(Command::Run, &["a.s", "--max", "0"]).unwrap().max, 0);
        for v in ["0xFFFF", "0Xffff", "FFFFh", "ffffH", "0xFFFFh"] {
            let a = parse(Command::Disasm, &["a.bin", "--org", v]).unwrap();
            a.validate_arch(Arch::I8086).unwrap();
            assert_eq!(a.org, Some(65535));
        }
        let a = parse(Command::Disasm, &["a.bin", "--arch", "mips", "--org", "ffffffff"]).unwrap();
        a.validate_arch(Arch::Mips32).unwrap();
        assert_eq!(a.org, Some(u32::MAX));
        assert_eq!(parse(Command::Run, &["--max=1", "--", "-a.s"]).unwrap().file, PathBuf::from("-a.s"));
    }
    #[test]
    fn rejected_options_never_fall_back_to_defaults() {
        for tokens in [
            vec!["a.s", "--max"],
            vec!["a.s", "--max", "--regs"],
            vec!["a.s", "--max", "nonsense"],
            vec!["a.s", "--max=-1"],
            vec!["a.s", "--max", "18446744073709551616"],
            vec!["a.s", "--max="],
            vec!["a.s", "--unknown"],
            vec!["a.s", "--trace=true"],
            vec!["a.s", "other.s"],
            vec!["a.s", "--max=1", "--max=2"],
        ] {
            assert!(parse(Command::Run, &tokens).is_err(), "{tokens:?}");
        }
        for v in ["xyz", "", "100000000", "-1", "100hh"] {
            assert!(parse(Command::Disasm, &["a.bin", "--org", v]).is_err(), "{v}");
        }
        assert!(parse(Command::Run, &["a.s", "--syntax=nasm"]).is_err());
        assert!(parse(Command::Asm, &["a.s", "--memory=wrong"]).is_err());
        assert!(parse(Command::Asm, &["a.s", "--syntax=wrong"]).is_err());
        assert!(parse(Command::Run, &["a.s", "--arch=z80"]).is_err());
        let a = parse(Command::Disasm, &["a.bin", "--org=10000"]).unwrap();
        assert!(a.validate_arch(Arch::I8086).is_err());
        let a = parse(Command::Run, &["a.asm", "--keys=A"]).unwrap();
        assert!(a.validate_arch(Arch::I8086).is_err());
    }
}
