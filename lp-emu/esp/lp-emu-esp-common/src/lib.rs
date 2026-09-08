//! The Espressif SoC substrate: a bus with regions and an MMIO decode
//! table, a peripheral model, an accept-and-remember register file, a real
//! bus trace, host byte streams, and an ELF program-header view.
//!
//! **This crate holds no chip numbers.** Not one base address, not one
//! register offset, not one interrupt source. A chip crate
//! (`lp-emu-esp32c6`, and whatever follows) builds a [`bus::SocBus`] by
//! registering its own regions, MMIO windows and peripherals; everything
//! here works the same for a C6, an S3 and a classic ESP32. The generated
//! register-name tables live in the chip crate for the same reason
//! ([`regnames`] holds the type and the lookup, never a table).
//!
//! # The layering
//!
//! ```text
//!   machine (chip crate)   reset, memory map, interrupt matrix, CLI
//!         |
//!   SocBus                 RAM regions + MMIO decode + watchpoints
//!    |        |            + the unmapped policy + sideband
//!    |    Peripheral       read/write/on_event against a BusCx
//!    |        |
//!    |     RegFile         accept-and-remember, with a table of exceptions
//!    |
//!   Trace                  every MMIO access, with the PC, plus SPIN
//!   HostSinks              where a UART's bytes actually go
//!   Scheduler              (in lp-emu-core) guest time, PD5
//! ```
//!
//! A peripheral sees a [`periph::BusCx`] and nothing else: cycles, the
//! issuing PC and hart, the scheduler, the interrupt **source** lines, the
//! trace, and the host streams. It never sees the hart's registers and never
//! sees another peripheral. Turning source levels into a CPU interrupt
//! number is the chip's matrix, one layer up (plan PD6).
//!
//! # Time
//!
//! Guest time is [`lp_emu_core::sched::Scheduler`] and nothing else: wall
//! clock never enters the machine (plan PD5), so two runs of the same image
//! with the same scripted host input are byte-identical.

pub mod bus;
pub mod elf;
pub mod host;
pub mod periph;
pub mod regfile;
pub mod regnames;
pub mod trace;

pub use bus::{
    BusScalars, RamRegion, SocBus, StrictViolation, event_id, event_local, event_peripheral,
};
pub use elf::{ElfImage, LoadSegment};
pub use host::{ByteLog, ByteSink, ByteSource, HostSinks, ScriptedSource, StreamId, TcpHost};
pub use periph::{
    BusCx, CpuIntMatrix, IrqLines, MachineRequest, NoCpuInterrupts, Peripheral, RegGrade,
    RegGrades, Sandbox, Strap, Width,
};
pub use regfile::RegFile;
pub use regnames::RegNames;
pub use trace::{Access, MmioEvent, Trace};

// The crate is `std` (it hosts stdout sinks and sockets), but
// the module bodies are written against `alloc` types so a `no_std` split
// stays cheap if a wasm host ever needs one.
extern crate alloc;
