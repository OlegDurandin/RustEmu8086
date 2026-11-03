//! MIPS32 emulator core of RustEmu8086, with MARS (the MIPS Assembler and Runtime Simulator) as
//! the reference: its integer instruction set, memory layout, access rules and exceptions.
//!
//! - [`memory`]: a sparse 32-bit address space with MARS's segments and a journal of overwritten
//!   bytes for step back;
//! - [`cpu`]: registers, HI/LO, coprocessor 0 and instruction execution;
//! - [`machine`]: the computer around the core — loading a program, the text [`console`] in place
//!   of video memory, [`files`], MARS's system services ([`syscall`]), step back.
//!
//! The debugger's `rustemu_target::Target` interface comes next, with the disassembler.

#![warn(missing_docs)]

pub mod asm;
pub mod console;
pub mod cpu;
pub mod disasm;
pub mod files;
pub mod fpu;
pub mod machine;
pub mod memory;
pub mod random;
pub mod syscall;
pub mod target;

pub use console::Console;
pub use cpu::{Cpu, ExcCode, Exception, Step};
pub use machine::{Chunk, Finish, Image, Machine};
pub use memory::{Access, AccessError, Memory, Segment};
pub use target::AsMips;
