//! The RV32 side of the block cache: the slot, its classification, and the
//! decoder that turns a run of guest instructions into slots.
//!
//! `lp_emu_core::block` owns the table, the arena and invalidation and knows
//! nothing about RISC-V. This is the other half: what a slot *is* on this
//! architecture, which instructions may live in a block, and where a block
//! ends.
//!
//! # The slot, in Step A
//!
//! ```text
//! { word: u32, handler: fn(..) -> Result<ExecutionResult, _>, width: u8, bound: InstClass }
//! ```
//!
//! The handler is one of the crate's *existing* category executors, reached
//! directly instead of through [`crate::emu::decode_execute`]'s three-level
//! match (the compressed test, then the opcode arm, then — for RVC — the
//! quadrant and funct3 arms). One decoder, one set of semantics; only the
//! dispatch changes, and it becomes an indirect call.
//!
//! That is deliberately the *cheap* slot. It keeps the `ExecutionResult`
//! round-trip, which P1 measured at ~9 % of host time on its own (~17 % with
//! `charge` and the `Option<u32>` pc), and which M5 Step B (P4) is the phase
//! that removes. Step A exists to settle invalidation, exactness, the polling
//! contract, the arch seam, the flag and the oracles before that is written.
//!
//! # Where a block ends, and what never enters one
//!
//! A block ends **after** a control transfer and **before** a `SYSTEM`, an
//! atomic or a fence (M5 MD2). A plain store stays *inside* a block: P1
//! measured that terminating at stores costs 28–31 % of the mean block length
//! and 40–45 % more blocks, against a self-modifying-code pressure of 0.02 %
//! of stores across five pages.
//!
//! Classification is **conservative in the safe direction**. Calling a body
//! instruction a terminator only shortens a block. Calling a control transfer
//! a body instruction would be a bug — so every encoding that might transfer
//! control is a terminator, including the whole of RVC quadrant 2 funct3 100
//! (`c.jr`/`c.jalr` share it with `c.mv`/`c.add`). Anything not positively
//! recognised is refused, and refusing is always exact: the caller falls back
//! to the single-stepping path that has always run it.
//!
//! The block executor also re-derives each slot's next `pc` from the result
//! the executor returned and leaves the block the moment it is not the `pc`
//! the decoder expected. In a correct build that never fires; it is there so
//! a classification mistake is a slower block rather than a wrong one.

use lp_emu_core::{Bus, InstClass};

use crate::emu::EmulatorError;
use crate::emu::executor::{
    ExecutionResult, LoggingDisabled, arithmetic, branch, compressed, immediate, jump, load_store,
};

/// The executor a slot calls. Every category executor in the crate has this
/// shape once `LoggingDisabled` and the bus type are fixed.
pub(super) type Handler<B> = fn(u32, u32, &mut [i32; 32], &mut B) -> HRet;

/// One pre-decoded RV32 instruction.
pub(super) struct RvSlot<B: Bus> {
    /// The raw instruction word, exactly as the bus served it.
    pub word: u32,
    /// The category executor for it.
    pub handler: Handler<B>,
    /// 2 or 4.
    pub width: u8,
    /// An upper bound on the class this slot can charge; see
    /// [`lp_emu_core::block::Slot::cost_bound`].
    pub bound: InstClass,
    /// THROWAWAY (M5 P1b): the dense category tag `slot-dense` dispatches on.
    #[cfg(feature = "slot-dense")]
    pub op: Op,
    /// THROWAWAY (M5 P1b): this slot's EXACT cycle cost under the block's
    /// cycle model, resolved at decode time. Only meaningful for a body
    /// slot — a terminator's cost is a run-time choice (F1) and is still
    /// taken from the class the executor returns.
    #[cfg(feature = "bk-hoist")]
    pub cost: u8,
}

// Written out rather than derived: a derive would demand `B: Clone` / `B:
// Copy` for a type whose only mention of `B` is inside a function pointer,
// and a bus is never cloned to run a block.
impl<B: Bus> Clone for RvSlot<B> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<B: Bus> Copy for RvSlot<B> {}

impl<B: Bus> core::fmt::Debug for RvSlot<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RvSlot")
            .field("word", &format_args!("{:#010x}", self.word))
            .field("width", &self.width)
            .field("bound", &self.bound)
            .finish()
    }
}

impl<B: Bus> lp_emu_core::block::Slot for RvSlot<B> {
    #[inline]
    fn width(&self) -> u8 {
        self.width
    }
    #[inline]
    fn cost_bound(&self) -> InstClass {
        self.bound
    }
}

/// What the decoder decided about one instruction word.
pub(super) enum Class<B: Bus> {
    /// May live inside a block and never changes `pc` by itself.
    Body(RvSlot<B>),
    /// May live inside a block, and is the last thing in it.
    Terminator(RvSlot<B>),
    /// Must not be cached. The block ends *before* it, and if the block would
    /// then be empty the address is not cacheable at all.
    Refused,
}

/// RV32 opcodes, named where the classification reads them.
const OP_LOAD: u8 = 0x03;
const OP_IMM: u8 = 0x13;
const OP_AUIPC: u8 = 0x17;
const OP_STORE: u8 = 0x23;
const OP_REG: u8 = 0x33;
const OP_LUI: u8 = 0x37;
const OP_BRANCH: u8 = 0x63;
const OP_JALR: u8 = 0x67;
const OP_JAL: u8 = 0x6f;

/// `funct7` of the M extension: `mul`/`div`/`rem` and friends.
const FUNCT7_M: u32 = 0x01;

/// Classify one instruction word.
///
/// The cost bounds are deliberately coarse (see
/// [`lp_emu_core::block::Slot::cost_bound`]): they gate only the whole-block
/// budget test, never a charge, so being generous costs a sliver of fast-path
/// coverage at a slice deadline while being *exact* would mean maintaining a
/// second copy of the executors' classification and hoping the two never
/// disagree.
pub(super) fn classify<B: Bus>(word: u32) -> Class<B> {
    if (word & 0b11) != 0b11 {
        return classify_compressed(word);
    }

    let opcode = (word & 0x7f) as u8;
    let slot = |handler: Handler<B>, bound: InstClass| RvSlot {
        word,
        handler,
        width: 4,
        bound,
        #[cfg(feature = "slot-dense")]
        op: op_of(word).expect("classify's own arms agree with op_of"),
        #[cfg(feature = "bk-hoist")]
        cost: 0,
    };
    match opcode {
        OP_REG => {
            // `mul`/`div`/`rem` share the opcode with the base ALU ops; the
            // divides are 32 cycles under the C6 model and the multiplies 1,
            // so the M-extension funct7 takes the larger of the two.
            let bound = if (word >> 25) & 0x7f == FUNCT7_M {
                InstClass::DivRem
            } else {
                InstClass::Alu
            };
            Class::Body(slot(
                h_rtype::<B>,
                bound,
            ))
        }
        OP_IMM => Class::Body(slot(
            h_itype::<B>,
            InstClass::Alu,
        )),
        OP_LOAD => Class::Body(slot(
            h_load::<B>,
            InstClass::Load,
        )),
        OP_STORE => Class::Body(slot(
            h_store::<B>,
            InstClass::Store,
        )),
        OP_LUI => Class::Body(slot(
            h_lui::<B>,
            InstClass::Lui,
        )),
        OP_AUIPC => Class::Body(slot(
            h_auipc::<B>,
            InstClass::Auipc,
        )),

        // The control transfers: in the block, and last.
        //
        // THROWAWAY (M5 P1b), `superblock`: a conditional branch stays INSIDE
        // the block and the decoder keeps going down its fall-through. The
        // block executor already handles the taken case exactly — its
        // straight-on check leaves the block the moment the hart is not at
        // `pc + width` — so this is a three-line change to the classifier and
        // nothing else. P1b's census says 38.7 % of `render-basic`'s block
        // ends are a NOT-TAKEN conditional branch, which is the largest
        // single block-length lever on the workload.
        #[cfg(feature = "superblock")]
        OP_BRANCH => Class::Body(slot(h_branch::<B>, InstClass::BranchTaken)),
        #[cfg(not(feature = "superblock"))]
        OP_BRANCH => Class::Terminator(slot(
            h_branch::<B>,
            // `BranchTaken` (2) is the larger of the two the executor can
            // return; the charge still uses whichever it actually returns.
            InstClass::BranchTaken,
        )),
        OP_JAL => Class::Terminator(slot(
            h_jal::<B>,
            InstClass::JalCall,
        )),
        OP_JALR => Class::Terminator(slot(
            h_jalr::<B>,
            InstClass::JalrCall,
        )),

        // `SYSTEM` is the hart's own (`ecall`, `ebreak`, `mret`, `wfi`, the
        // CSR instructions) and must keep behaving exactly as it does;
        // `MISC-MEM` carries the `fence.i` the cache is invalidated by, and a
        // fence is where a block ends by definition; an atomic is a store
        // with an interrupt-visible side-band. None of the three is cached.
        // Everything unrecognised — including RV32F, which this hart rejects
        // before `decode_execute` — is refused, which is always safe.
        _ => Class::Refused,
    }
}

/// RVC (v2.0 §16). Quadrant and funct3 decide; anything else is refused.
fn classify_compressed<B: Bus>(word: u32) -> Class<B> {
    // The illegal all-zero encoding, and `c.ebreak`, which the hart handles
    // itself.
    if word & 0xffff == 0 || word & 0xffff == 0x9002 {
        return Class::Refused;
    }
    let quadrant = word & 0b11;
    let funct3 = (word >> 13) & 0b111;
    let slot = |bound: InstClass| RvSlot {
        word,
        handler: h_compressed::<B> as Handler<B>,
        width: 2,
        bound,
        #[cfg(feature = "slot-dense")]
        op: Op::Compressed,
        #[cfg(feature = "bk-hoist")]
        cost: 0,
    };
    match (quadrant, funct3) {
        // Q0: c.addi4spn (000), c.lw (010), c.sw (110). 001/011/101/111 are
        // the FP and reserved encodings this hart has no answer for.
        (0b00, 0b000) => Class::Body(slot(InstClass::Alu)),
        (0b00, 0b010) => Class::Body(slot(InstClass::Load)),
        (0b00, 0b110) => Class::Body(slot(InstClass::Store)),

        // Q1: c.addi (000), c.li (010), c.lui/c.addi16sp (011), the misc-ALU
        // group (100). c.jal (001), c.j (101), c.beqz (110), c.bnez (111) all
        // transfer control.
        (0b01, 0b000) | (0b01, 0b010) | (0b01, 0b100) => Class::Body(slot(InstClass::Alu)),
        (0b01, 0b011) => Class::Body(slot(InstClass::Lui)),
        (0b01, 0b001) => Class::Terminator(slot(InstClass::JalCall)),
        (0b01, 0b101) => Class::Terminator(slot(InstClass::JalTail)),
        #[cfg(feature = "superblock")]
        (0b01, 0b110) | (0b01, 0b111) => Class::Body(slot(InstClass::BranchTaken)),
        #[cfg(not(feature = "superblock"))]
        (0b01, 0b110) | (0b01, 0b111) => Class::Terminator(slot(InstClass::BranchTaken)),

        // Q2: c.slli (000), c.lwsp (010), c.swsp (110).
        (0b10, 0b000) => Class::Body(slot(InstClass::Alu)),
        (0b10, 0b010) => Class::Body(slot(InstClass::Load)),
        (0b10, 0b110) => Class::Body(slot(InstClass::Store)),
        // funct3 100 is four instructions sharing one encoding, split by
        // `rs2` (RVC v2.0 §16.5): `rs2 != 0` is `c.mv`/`c.add`, which move a
        // register and go straight on; `rs2 == 0` is `c.jr`/`c.jalr` (and
        // `c.ebreak`, already refused above), which do not.
        //
        // Splitting them matters: `c.mv` and `c.add` are two of the most
        // common instructions the compiler emits, and sweeping the whole
        // group in with the jumps dropped the render loop's mean realised
        // block length from 4.5 to 3.75 — measured, not guessed.
        (0b10, 0b100) if (word >> 2) & 0x1f != 0 => Class::Body(slot(InstClass::Alu)),
        (0b10, 0b100) => Class::Terminator(slot(InstClass::JalrCall)),

        _ => Class::Refused,
    }
}

/// The most slots one block may hold.
///
/// A block runs without a per-slot deadline compare on the fast path, so one
/// block must not be able to dwarf a slice: the C6's slice cap is 8,192
/// cycles (1,024 under `--strict-bus`), and P1 measured a mean realised block
/// length of 4.69 on `render-basic` and 6.97 on `render-rocaille` with 83 % of
/// instructions in blocks of 16 or fewer. 64 is far past the distribution and
/// still bounded.
pub(super) const MAX_BLOCK_SLOTS: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_core::Memory;

    /// What the slot's handler returns by value, in bytes.
    ///
    /// Not a claim about behaviour — the number M5's G3 report is built on,
    /// pinned here so P4 can watch it fall. Inside `decode_execute` the
    /// optimizer never materialises any of this: `LoggingDisabled` folds the
    /// `log` field to `None` and the whole struct lives in registers. Across
    /// the slot's **real call boundary** it becomes a 200-byte value returned
    /// through memory on every cached instruction, and that — not the block
    /// structure — is what Step A's speedup is spent on.
    #[test]
    fn the_slot_pays_for_a_200_byte_return_value() {
        assert_eq!(
            core::mem::size_of::<RvSlot<Memory>>(),
            16,
            "the slot itself"
        );
        assert_eq!(core::mem::size_of::<ExecutionResult>(), 56);
        assert_eq!(
            core::mem::size_of::<Result<ExecutionResult, EmulatorError>>(),
            200,
            "`EmulatorError`'s largest variant sets this, and every cached \
             instruction returns it: P4's first job"
        );
    }

    fn class_of(word: u32) -> &'static str {
        match classify::<Memory>(word) {
            Class::Body(_) => "body",
            Class::Terminator(_) => "terminator",
            Class::Refused => "refused",
        }
    }

    fn bound_of(word: u32) -> Option<InstClass> {
        match classify::<Memory>(word) {
            Class::Body(s) | Class::Terminator(s) => Some(s.bound),
            Class::Refused => None,
        }
    }

    #[test]
    fn the_hart_owned_and_unsupported_encodings_are_refused() {
        assert_eq!(class_of(0x0000_0073), "refused", "ecall");
        assert_eq!(class_of(0x0010_0073), "refused", "ebreak");
        assert_eq!(class_of(0x3020_0073), "refused", "mret");
        assert_eq!(class_of(0x1050_0073), "refused", "wfi");
        assert_eq!(class_of(0x3400_2573), "refused", "csrrs a0, mscratch");
        assert_eq!(class_of(0x0000_100f), "refused", "fence.i");
        assert_eq!(class_of(0x0ff0_000f), "refused", "fence");
        assert_eq!(class_of(0x1000_212f), "refused", "lr.w");
        assert_eq!(class_of(0x0000_2087), "refused", "flw (RV32F)");
        assert_eq!(class_of(0x0031_00d3), "refused", "fadd.s (RV32F)");
        assert_eq!(class_of(0x0000_9002), "refused", "c.ebreak");
        assert_eq!(
            class_of(0x0000_0000),
            "refused",
            "the illegal RVC zero word"
        );
    }

    #[test]
    fn every_control_transfer_is_a_terminator() {
        assert_eq!(class_of(0x0000_0063), "terminator", "beq");
        assert_eq!(class_of(0x0000_006f), "terminator", "jal");
        assert_eq!(class_of(0x0000_0067), "terminator", "jalr");
        assert_eq!(class_of(0x0000_2001), "terminator", "c.jal");
        assert_eq!(class_of(0x0000_a001), "terminator", "c.j");
        assert_eq!(class_of(0x0000_c101), "terminator", "c.beqz");
        assert_eq!(class_of(0x0000_e101), "terminator", "c.bnez");
        assert_eq!(class_of(0x0000_8082), "terminator", "c.jr ra (ret)");
        assert_eq!(class_of(0x0000_9082), "terminator", "c.jalr ra");
    }

    #[test]
    fn the_ordinary_arithmetic_and_memory_encodings_are_block_bodies() {
        assert_eq!(class_of(0x0000_0013), "body", "nop (addi x0,x0,0)");
        assert_eq!(class_of(0x0000_0033), "body", "add x0,x0,x0");
        assert_eq!(class_of(0x0000_2003), "body", "lw x0,0(x0)");
        assert_eq!(class_of(0x0000_2023), "body", "sw x0,0(x0)");
        assert_eq!(class_of(0x0000_0037), "body", "lui x0,0");
        assert_eq!(class_of(0x0000_0017), "body", "auipc x0,0");
        assert_eq!(class_of(0x0000_4108), "body", "c.lw");
        assert_eq!(class_of(0x0000_c108), "body", "c.sw");
        assert_eq!(class_of(0x0000_0505), "body", "c.addi a0,1");
        // The other half of quadrant 2 funct3 100, split by `rs2 != 0`.
        assert_eq!(class_of(0x0000_8506), "body", "c.mv a0, ra");
        assert_eq!(class_of(0x0000_9506), "body", "c.add a0, ra");
    }

    #[test]
    fn the_m_extension_takes_the_larger_cost_bound() {
        // `mul a0, a1, a2` — funct7 0x01.
        assert_eq!(bound_of(0x02c5_8533), Some(InstClass::DivRem));
        // `add a0, a1, a2` — funct7 0.
        assert_eq!(bound_of(0x00c5_8533), Some(InstClass::Alu));
    }

    #[test]
    fn a_compressed_slot_is_two_bytes_wide_and_a_full_one_is_four() {
        let Class::Body(s) = classify::<Memory>(0x0000_0505) else {
            panic!("c.addi is a body slot")
        };
        assert_eq!(s.width, 2);
        let Class::Body(s) = classify::<Memory>(0x0000_0013) else {
            panic!("addi is a body slot")
        };
        assert_eq!(s.width, 4);
    }
}

// ============================================================================
// THROWAWAY (M5 P1b) — never merge.
//
// Two experimental slot payloads, each its own cargo feature, so the four
// combinations can be timed against one another natively and in wasm from a
// single tree.  Neither is a correct emulator on the error path: the packed
// return has no room for an `EmulatorError`, so a diagnostic build panics
// where the real one would deliver a trap.  They exist to be TIMED.
//
//   --features slot-packed   the handler returns a packed `u64` instead of a
//                            200-byte `Result<ExecutionResult, EmulatorError>`.
//                            §7 of PR #637 measured this at 1.008x NATIVELY;
//                            P1b asks what it is worth in wasm, where the
//                            struct is written to and read back from linear
//                            memory across a real call boundary.
//   --features slot-dense    the slot carries a dense category tag and
//                            `run_block` dispatches on it with a `match`
//                            (a wasm `br_table`) instead of an indirect call
//                            through the function table (`call_indirect`).
// ============================================================================

/// The packed form of a successful [`ExecutionResult`], for `slot-packed`.
///
/// ```text
///  bit 63      error sentinel
///  bit 32      `new_pc` is `Some`
///  bits 0..32  `new_pc`'s value
///  bits 33..41 `class` as its discriminant
///  bits 41..49 `inst_size`
/// ```
#[cfg(feature = "slot-packed")]
pub(super) const PACKED_ERR: u64 = 1 << 63;

#[cfg(feature = "slot-packed")]
#[inline(always)]
pub(super) fn pack(r: Result<ExecutionResult, EmulatorError>) -> u64 {
    match r {
        Ok(r) => {
            let pc = match r.new_pc {
                Some(p) => (1u64 << 32) | u64::from(p),
                None => 0,
            };
            pc | (u64::from(class_code(r.class)) << 33) | (u64::from(r.inst_size) << 41)
        }
        Err(_) => PACKED_ERR,
    }
}

#[cfg(feature = "slot-packed")]
#[inline(always)]
pub(super) fn class_code(c: InstClass) -> u8 {
    match c {
        InstClass::Alu => 0,
        InstClass::Mul => 1,
        InstClass::DivRem => 2,
        InstClass::Load => 3,
        InstClass::Store => 4,
        InstClass::BranchTaken => 5,
        InstClass::BranchNotTaken => 6,
        InstClass::JalCall => 7,
        InstClass::JalTail => 8,
        InstClass::JalrCall => 9,
        InstClass::JalrReturn => 10,
        InstClass::JalrIndirect => 11,
        InstClass::Lui => 12,
        InstClass::Auipc => 13,
        InstClass::System => 14,
        InstClass::Fence => 15,
        InstClass::Atomic => 16,
        InstClass::FloatArith => 17,
        InstClass::FloatMulAdd => 18,
        InstClass::FloatConvert => 19,
        InstClass::FloatCompare => 20,
        InstClass::FloatEstimate => 21,
    }
}

#[cfg(feature = "slot-packed")]
#[inline(always)]
pub(super) fn class_of_code(c: u8) -> InstClass {
    match c {
        0 => InstClass::Alu,
        1 => InstClass::Mul,
        2 => InstClass::DivRem,
        3 => InstClass::Load,
        4 => InstClass::Store,
        5 => InstClass::BranchTaken,
        6 => InstClass::BranchNotTaken,
        7 => InstClass::JalCall,
        8 => InstClass::JalTail,
        9 => InstClass::JalrCall,
        10 => InstClass::JalrReturn,
        11 => InstClass::JalrIndirect,
        12 => InstClass::Lui,
        13 => InstClass::Auipc,
        14 => InstClass::System,
        15 => InstClass::Fence,
        16 => InstClass::Atomic,
        17 => InstClass::FloatArith,
        18 => InstClass::FloatMulAdd,
        19 => InstClass::FloatConvert,
        20 => InstClass::FloatCompare,
        21 => InstClass::FloatEstimate,
        _ => unreachable!("P1b: class code out of range"),
    }
}

/// The dense category tag, for `slot-dense`.
#[cfg(feature = "slot-dense")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Op {
    RType,
    IType,
    Load,
    Store,
    Lui,
    Auipc,
    Branch,
    Jal,
    Jalr,
    Compressed,
}

/// `slot-dense`: the whole dispatch, as one `match` over the slot's tag.
#[cfg(feature = "slot-dense")]
#[inline(always)]
pub(super) fn dispatch_dense<B: Bus>(
    op: Op,
    word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    bus: &mut B,
) -> Result<ExecutionResult, EmulatorError> {
    match op {
        Op::RType => arithmetic::decode_execute_rtype::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::IType => immediate::decode_execute_itype::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Load => load_store::decode_execute_load::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Store => load_store::decode_execute_store::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Lui => jump::decode_execute_lui::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Auipc => jump::decode_execute_auipc::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Branch => branch::decode_execute_branch::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Jal => jump::decode_execute_jal::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Jalr => jump::decode_execute_jalr::<LoggingDisabled, B>(word, pc, regs, bus),
        Op::Compressed => {
            compressed::decode_execute_compressed::<LoggingDisabled, B>(word, pc, regs, bus)
        }
    }
}

/// `slot-dense`: which category a word belongs to, or `None` if it is not
/// cacheable. Mirrors [`classify`]'s own arms exactly.
#[cfg(feature = "slot-dense")]
pub(super) fn op_of(word: u32) -> Option<Op> {
    if (word & 0b11) != 0b11 {
        return match classify::<lp_emu_core::Memory>(word) {
            Class::Refused => None,
            _ => Some(Op::Compressed),
        };
    }
    Some(match (word & 0x7f) as u8 {
        OP_REG => Op::RType,
        OP_IMM => Op::IType,
        OP_LOAD => Op::Load,
        OP_STORE => Op::Store,
        OP_LUI => Op::Lui,
        OP_AUIPC => Op::Auipc,
        OP_BRANCH => Op::Branch,
        OP_JAL => Op::Jal,
        OP_JALR => Op::Jalr,
        _ => return None,
    })
}

/// What a slot's handler returns: the merged shape, or `slot-packed`'s `u64`.
#[cfg(not(feature = "slot-packed"))]
pub(super) type HRet = Result<ExecutionResult, EmulatorError>;
#[cfg(feature = "slot-packed")]
pub(super) type HRet = u64;

#[cfg(not(feature = "slot-packed"))]
#[inline(always)]
fn finish(r: Result<ExecutionResult, EmulatorError>) -> HRet {
    r
}
#[cfg(feature = "slot-packed")]
#[inline(always)]
fn finish(r: Result<ExecutionResult, EmulatorError>) -> HRet {
    pack(r)
}

macro_rules! hnd {
    ($name:ident, $m:ident, $f:ident) => {
        #[inline(always)]
        pub(super) fn $name<B: Bus>(w: u32, pc: u32, regs: &mut [i32; 32], bus: &mut B) -> HRet {
            finish($m::$f::<LoggingDisabled, B>(w, pc, regs, bus))
        }
    };
}

hnd!(h_rtype, arithmetic, decode_execute_rtype);
hnd!(h_itype, immediate, decode_execute_itype);
hnd!(h_load, load_store, decode_execute_load);
hnd!(h_store, load_store, decode_execute_store);
hnd!(h_lui, jump, decode_execute_lui);
hnd!(h_auipc, jump, decode_execute_auipc);
hnd!(h_branch, branch, decode_execute_branch);
hnd!(h_jal, jump, decode_execute_jal);
hnd!(h_jalr, jump, decode_execute_jalr);
hnd!(h_compressed, compressed, decode_execute_compressed);

/// THROWAWAY (M5 P1b), for `bk-hoist`: the **exact** class a body slot's
/// executor will return, resolved from the word at decode time.
///
/// This is the "second source of truth" the phase file warns about, and it is
/// exactly why this is a diagnostic and not a patch: if one arm here
/// disagrees with the executor it charges a different number of cycles and
/// the byte-identity oracle says so immediately. Terminators are *not*
/// covered — a branch's cost is a run-time choice (F1) and stays with the
/// class the executor returns.
#[cfg(feature = "bk-hoist")]
pub(super) fn exact_class(word: u32) -> InstClass {
    if (word & 0b11) != 0b11 {
        let q = word & 0b11;
        let f3 = (word >> 13) & 0b111;
        return match (q, f3) {
            (0b00, 0b010) | (0b10, 0b010) => InstClass::Load,
            (0b00, 0b110) | (0b10, 0b110) => InstClass::Store,
            (0b01, 0b011) => InstClass::Lui,
            _ => InstClass::Alu,
        };
    }
    match (word & 0x7f) as u8 {
        OP_LOAD => InstClass::Load,
        OP_STORE => InstClass::Store,
        OP_LUI => InstClass::Lui,
        OP_AUIPC => InstClass::Auipc,
        OP_REG if (word >> 25) & 0x7f == FUNCT7_M => {
            if (word >> 12) & 0b111 < 4 {
                InstClass::Mul
            } else {
                InstClass::DivRem
            }
        }
        _ => InstClass::Alu,
    }
}
