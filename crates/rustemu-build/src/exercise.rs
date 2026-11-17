//! Bounded, optional exercise metadata for portable projects. All test cases are public.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A versioned offline exercise; never a live machine snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exercise {
    /// Exercise schema version (1).
    pub version: u32,
    /// Stable identifier.
    pub id: String,
    /// Short human-readable title.
    pub title: String,
    /// Goal, initial conditions and checked conditions.
    pub description: String,
    /// Must match the enclosing project's architecture.
    pub architecture: String,
    /// Original starter code; the editable solution remains in the project entry file.
    pub starter_code: String,
    /// Instructions per case, at most 200000.
    pub step_limit: u64,
    /// Wall time per case in milliseconds, at most 5000. The build watchdog is separate.
    pub time_limit_ms: u32,
    /// One or two public hints.
    pub hint: String,
    /// Public input and expected result cases (1 to 16).
    pub cases: Vec<Case>,
}

/// One isolated execution with its own initial registers and console input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Display name.
    pub name: String,
    /// Prepared ASCII console input, LF means Enter.
    pub input: String,
    /// Initial named registers (not arbitrary debugger expressions).
    #[serde(default)]
    pub initial_registers: BTreeMap<String, u64>,
    /// All conditions are conjunctive.
    pub expected: Expected,
}

/// Explicit end condition; exhausting input or a budget is never success.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Termination {
    /// HLT, useful for observing arithmetic flags without a DOS exit stub.
    Halted,
    /// Program exit with the given code.
    Exited(u8),
}

/// A contiguous range of physical memory to compare.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryCheck {
    /// Physical byte address (MIPS MMIO is not allowed).
    pub address: u32,
    /// Expected bytes, at most 256 across the case.
    pub bytes: Vec<u8>,
}

/// Observable end-state assertions, without executable expressions or host paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    /// Exact output, when supplied. CRLF is normalized to LF for comparison.
    pub output: Option<String>,
    /// Named registers and flags.
    #[serde(default)]
    pub registers: BTreeMap<String, u64>,
    /// Bounded memory ranges.
    #[serde(default)]
    pub memory: Vec<MemoryCheck>,
    /// Required termination event.
    pub termination: Termination,
}

impl Exercise {
    /// Validates every length, execution bound and memory range before a checker is created.
    pub fn validate(&self, architecture: &str) -> Result<(), String> {
        let invalid = || Err("Invalid exercise or exercise budget exceeded".to_string());
        if self.version != 1
            || self.architecture != architecture
            || self.id.is_empty()
            || self.id.len() > 128
            || self.title.is_empty()
            || self.title.len() > 256
            || self.description.len() > 8192
            || self.hint.len() > 4096
            || self.starter_code.len() > 65536
            || !(1..=200000).contains(&self.step_limit)
            || !(1..=5000).contains(&self.time_limit_ms)
            || self.cases.is_empty()
            || self.cases.len() > 16
        {
            return invalid();
        }
        for case in &self.cases {
            if case.name.is_empty()
                || case.name.len() > 128
                || case.input.len() > 4096
                || !case.input.is_ascii()
                || case.expected.output.as_ref().is_some_and(|s| s.len() > 4096)
                || case.initial_registers.len() > 32
                || case.expected.registers.len() > 64
                || case.expected.memory.len() > 16
            {
                return invalid();
            }
            for name in case.initial_registers.keys().chain(case.expected.registers.keys()) {
                if name.is_empty() || name.len() > 16 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'$') {
                    return invalid();
                }
            }
            let mut bytes = 0usize;
            let end = if architecture == "8086" { 0x100000 } else { 0x80000000 };
            for mem in &case.expected.memory {
                bytes = bytes.saturating_add(mem.bytes.len());
                if mem.bytes.is_empty()
                    || bytes > 256
                    || mem.address.checked_add(mem.bytes.len() as u32).is_none_or(|a| a > end)
                {
                    return invalid();
                }
            }
        }
        Ok(())
    }
}
