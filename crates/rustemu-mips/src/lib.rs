//! MIPS32 emulator core of RustEmu8086, with MARS (the MIPS Assembler and Runtime Simulator) as
//! the reference: its integer instruction set, memory layout, access rules and exceptions.
//!
//! - [`memory`]: a sparse 32-bit address space with MARS's segments and a journal of overwritten
//!   bytes for step back;
//! - [`cpu`]: registers, HI/LO, coprocessor 0 and instruction execution.
//!
//! System calls, loading programs and the debugger's `rustemu_target::Target` interface come with
//! the machine built on this core.

#![warn(missing_docs)]

pub mod cpu;
pub mod memory;

pub use cpu::{Cpu, ExcCode, Exception, Step};
pub use memory::{Access, AccessError, Memory, Segment};
