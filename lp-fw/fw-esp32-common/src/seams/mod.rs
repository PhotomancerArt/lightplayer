//! Emulator seams: the firmware's descriptor table and the chip-neutral
//! seams (`lp-base/lp-seam`; ADR `docs/adr/2026-10-05-emulator-seams.md`).
//!
//! A seam is a named function in the one shipped firmware where an emulator
//! may answer. On silicon its real body runs and nothing else changes. The
//! emulator finds the table a chip crate instantiates with [`seam_table!`]
//! by scanning the flash image, and arms a seam only when a run asked for it.
//!
//! RISC-V only (`target_arch = "riscv32"`): the generated seam functions are
//! RV32 assembly, and the Xtensa firmwares that also build this crate get
//! their seams in the roadmap's M7.

pub mod seam_table;
pub mod ws281x_wait_step;

#[doc(hidden)]
pub use lp_seam;
