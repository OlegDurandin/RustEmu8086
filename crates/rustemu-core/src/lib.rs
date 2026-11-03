//! RustEmu8086 emulator core: 8086 CPU, memory, BIOS/DOS services and virtual devices. The
//! [`Machine`] is the 8086 [`Target`] (see [`target`]).

#![warn(missing_docs)]

mod bios;
pub mod bus;
pub mod cpu;
pub mod devices;
pub mod disasm;
pub mod dos;
pub mod font;
pub mod fpu;
pub mod keyboard;
pub mod log;
pub mod machine;
pub mod memdrive;
pub mod robot;
pub mod target;
pub mod video;

pub use bus::{Bus, Memory, linear};
pub use cpu::{AluOp, AluTrace, Cpu, StepResult};
pub use machine::{Event, LoadInfo, LoadKind, Machine, MachineState};
pub use rustemu_target::{Arch, Target};
pub use target::{AsMachine, Checkpoint};
