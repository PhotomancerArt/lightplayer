//! Xtensa (ESP32-S3 / LX7 and classic ESP32 / LX6) ISA-specific code:
//! register model, windowed ABI, per-opcode immediate legality, emission.
//!
//! Ported from the experiment repo's `xt-mini-emit` (hardware-proven on both
//! chips; see its README's MiniVInst↔VInst mapping table and the ADR
//! `2026-07-28-xtensa-abi-contract.md` there). Instruction encoding is
//! delegated to the `lp-xt-inst` crate — this module never packs bytes.

pub mod abi;
pub mod emit;
// The float half of the emitter, gated with the register model below.
#[cfg(feature = "float-f32")]
pub mod emit_fp;
// The float register model. Gated at module granularity (M7 D9) so a
// Fixed-only image links no float tables; the `VInst` float variants
// themselves are unconditional, because `cfg` on enum variants matched
// exhaustively across five helpers and the text ser/de costs more than the
// residual it saves.
#[cfg(feature = "float-f32")]
pub mod fpr;
pub mod gpr;
pub mod imm;
pub mod link;
