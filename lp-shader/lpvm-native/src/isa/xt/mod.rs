//! Xtensa (ESP32-S3 / LX7 and classic ESP32 / LX6) ISA-specific code:
//! register model, windowed ABI, per-opcode immediate legality, emission.
//!
//! Ported from the experiment repo's `xt-mini-emit` (hardware-proven on both
//! chips; see its README's MiniVInst↔VInst mapping table and the ADR
//! `2026-07-28-xtensa-abi-contract.md` there). Instruction encoding is
//! delegated to the `lp-xt-inst` crate — this module never packs bytes.

pub mod abi;
pub mod emit;
pub mod gpr;
pub mod imm;
pub mod link;
