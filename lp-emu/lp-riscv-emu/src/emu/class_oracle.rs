//! What this crate's executors say an instruction word *is*: its width, its
//! cost class, and whether they accept it at all.
//!
//! # Why this exists
//!
//! Decode in this crate is fused into execution — [`decode_execute`] picks the
//! category executor and the executor computes the result, the width and the
//! class together, and there is deliberately no separate decoder to consult
//! (the fusion is what removed an `Inst` allocation per instruction). That is
//! fine for an interpreter and a problem for anything that has to decode
//! *without* running: a translator, a disassembler, a coverage tool.
//!
//! `lp-emu-jit` is the first such thing, and M7 JD3 accepts its second decoder
//! **only** because an agreement test holds the two to the same answers. This
//! function is the ground truth that test compares against: not a second
//! classification maintained by hand — that would be a third decoder, and a
//! third decoder to keep in step is worse than the problem — but the executors
//! themselves, run once on scratch state and asked what they did.
//!
//! # What it is not
//!
//! Not a fast path, and not for the run loop. It builds a register file and a
//! bus per call and throws them away. `crate::mach::block::classify` is what
//! the block cache uses, and its `cost_bound` is deliberately *coarse*; this is
//! the exact answer, at a price no hot loop should pay.

use lp_emu_core::{Bus, InstClass, MemoryError};

use crate::emu::executor::ExecutionResult;
use crate::emu::fp_regs::FpRegs;
use crate::emu::{LoggingDisabled, decode_execute};

/// The pc the word is classified at.
///
/// Only the classes that depend on the *encoding* are reported (see
/// [`class_of`]), so the value cannot change an answer; a recognisable
/// constant makes an error message from a stray executor obvious.
const SCRATCH_PC: u32 = 0x4000_0000;

/// A bus on which no access can fail.
///
/// Every read answers zero and every write is dropped. That is not laziness:
/// this oracle distinguishes "the executors refuse this encoding" from
/// everything else, so a bus that could return [`MemoryError`] would let an
/// out-of-range address masquerade as an illegal instruction and quietly
/// weaken the agreement test.
#[derive(Default)]
struct InfallibleBus;

impl Bus for InfallibleBus {
    fn fetch_instruction(&mut self, _address: u32) -> Result<u32, MemoryError> {
        Ok(0)
    }
    fn read_word(&mut self, _address: u32) -> Result<i32, MemoryError> {
        Ok(0)
    }
    fn read_halfword(&mut self, _address: u32) -> Result<i16, MemoryError> {
        Ok(0)
    }
    fn read_byte(&mut self, _address: u32) -> Result<i8, MemoryError> {
        Ok(0)
    }
    fn read_u8(&mut self, _address: u32) -> Result<u8, MemoryError> {
        Ok(0)
    }
    fn write_word(&mut self, _address: u32, _value: i32) -> Result<(), MemoryError> {
        Ok(())
    }
    fn write_halfword(&mut self, _address: u32, _value: i16) -> Result<(), MemoryError> {
        Ok(())
    }
    fn write_byte(&mut self, _address: u32, _value: i8) -> Result<(), MemoryError> {
        Ok(())
    }
}

/// The width in bytes and the cost class this crate's executors give `word`, or
/// [`None`] if they refuse the encoding.
///
/// # Branches report the taken class
///
/// [`InstClass::BranchTaken`] and [`InstClass::BranchNotTaken`] are chosen by
/// the *operands* at run time, not by the encoding, so a decode-time answer can
/// only be one of them. This reports `BranchTaken`, which is the same choice
/// `crate::mach::block::classify` makes and the same one `lp-emu-jit`'s decoder
/// makes: the translator charges the not-taken class itself on the fall-through
/// path.
///
/// # It runs the instruction
///
/// On a scratch register file and an [`InfallibleBus`], both discarded. The
/// executors are pure with respect to everything else — no host clock, no
/// allocation that outlives the call — so the answer is a function of `word`
/// alone.
#[must_use]
pub fn class_of(word: u32) -> Option<(u8, InstClass)> {
    let mut regs = [0i32; 32];
    let mut bus = InfallibleBus;
    let mut fp = FpRegs::new();
    let result: ExecutionResult =
        decode_execute::<LoggingDisabled, _>(word, SCRATCH_PC, &mut regs, &mut bus, &mut fp)
            .ok()?;
    let class = match result.class {
        InstClass::BranchNotTaken => InstClass::BranchTaken,
        other => other,
    };
    Some((result.inst_size, class))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oracle_reports_the_width_and_the_class_the_executors_used() {
        assert_eq!(class_of(0x0000_0013), Some((4, InstClass::Alu)), "nop");
        assert_eq!(class_of(0x0505), Some((2, InstClass::Alu)), "c.addi a0, 1");
        assert_eq!(
            class_of(0x02c5_8533),
            Some((4, InstClass::Mul)),
            "mul a0, a1, a2 — Mul, not the block cache's coarser DivRem bound"
        );
        assert_eq!(class_of(0x02c5_c533), Some((4, InstClass::DivRem)), "div");
        assert_eq!(
            class_of(0x0000_8067),
            Some((4, InstClass::JalrReturn)),
            "ret"
        );
        assert_eq!(
            class_of(0x8082),
            Some((2, InstClass::JalrReturn)),
            "c.jr ra"
        );
    }

    #[test]
    fn a_branch_reports_the_taken_class_whichever_way_it_would_go() {
        // With a zeroed register file `beq` is taken and `bne` is not; both
        // must answer the same, because the encoding is what is being asked
        // about.
        assert_eq!(class_of(0x0000_0063), Some((4, InstClass::BranchTaken)));
        assert_eq!(class_of(0x0000_1063), Some((4, InstClass::BranchTaken)));
    }

    #[test]
    fn a_refused_encoding_is_none_and_a_wild_address_is_not() {
        assert_eq!(class_of(0x0000_0000), None, "the illegal RVC zero word");
        // `c.ebreak` IS accepted here — the user-mode executor answers it —
        // even though the block cache refuses to cache it and `lp-emu-jit`
        // refuses to translate it. The oracle reports what the executors do,
        // not what any consumer wishes they did.
        assert_eq!(class_of(0x9002), Some((2, InstClass::System)));
        // `lw a0, -1(x0)` — address 0xffffffff. An oracle backed by a real
        // memory would call this illegal; this one must not.
        assert_eq!(class_of(0xfff0_2503), Some((4, InstClass::Load)));
    }
}
