//! Flat LPIR operation stream. Control flow uses marker ops and skip offsets.

use crate::types::{CalleeRef, SlotId, VReg, VRegRange};

/// One instruction in the flat per-function op stream.
#[derive(Clone, Debug)]
pub enum LpirOp {
    // --- Float arithmetic ---
    Fadd {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fsub {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fmul {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fdiv {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fneg {
        dst: VReg,
        src: VReg,
    },

    // --- Float math ---
    Fabs {
        dst: VReg,
        src: VReg,
    },
    Fsqrt {
        dst: VReg,
        src: VReg,
    },
    Fmin {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fmax {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Ffloor {
        dst: VReg,
        src: VReg,
    },
    Fceil {
        dst: VReg,
        src: VReg,
    },
    Ftrunc {
        dst: VReg,
        src: VReg,
    },
    Fnearest {
        dst: VReg,
        src: VReg,
    },

    // --- Integer arithmetic ---
    Iadd {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Isub {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Imul {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IdivS {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IdivU {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IremS {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IremU {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Ineg {
        dst: VReg,
        src: VReg,
    },

    // --- Float comparisons ---
    Feq {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fne {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Flt {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fle {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fgt {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Fge {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },

    // --- Integer comparisons (signed) ---
    Ieq {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Ine {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IltS {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IleS {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IgtS {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IgeS {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },

    // --- Integer comparisons (unsigned) ---
    IltU {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IleU {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IgtU {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IgeU {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },

    // --- Logic / bitwise ---
    Iand {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Ior {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Ixor {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    Ibnot {
        dst: VReg,
        src: VReg,
    },
    Ishl {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IshrS {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    IshrU {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },

    // --- Constants ---
    FconstF32 {
        dst: VReg,
        value: f32,
    },
    IconstI32 {
        dst: VReg,
        value: i32,
    },

    // --- Immediate variants ---
    IaddImm {
        dst: VReg,
        src: VReg,
        imm: i32,
    },
    IsubImm {
        dst: VReg,
        src: VReg,
        imm: i32,
    },
    ImulImm {
        dst: VReg,
        src: VReg,
        imm: i32,
    },
    IshlImm {
        dst: VReg,
        src: VReg,
        imm: i32,
    },
    IshrSImm {
        dst: VReg,
        src: VReg,
        imm: i32,
    },
    IshrUImm {
        dst: VReg,
        src: VReg,
        imm: i32,
    },
    IeqImm {
        dst: VReg,
        src: VReg,
        imm: i32,
    },

    // --- Casts ---
    FtoiSatS {
        dst: VReg,
        src: VReg,
    },
    FtoiSatU {
        dst: VReg,
        src: VReg,
    },
    ItofS {
        dst: VReg,
        src: VReg,
    },
    ItofU {
        dst: VReg,
        src: VReg,
    },
    /// Reinterpret [`IrType::I32`] bits as [`IrType::F32`] (Q32 lanes stay raw `i32`;
    /// native F32 uses an `i32`→`f32` bitcast).
    FfromI32Bits {
        dst: VReg,
        src: VReg,
    },

    // --- Select / copy ---
    Select {
        dst: VReg,
        cond: VReg,
        if_true: VReg,
        if_false: VReg,
    },
    Copy {
        dst: VReg,
        src: VReg,
    },

    // --- Memory ---
    SlotAddr {
        dst: VReg,
        slot: SlotId,
    },
    Load {
        dst: VReg,
        base: VReg,
        offset: u32,
    },
    Store {
        base: VReg,
        offset: u32,
        value: VReg,
    },
    /// 8-bit store: writes the low 8 bits of `value` to `[base + offset]`.
    Store8 {
        base: VReg,
        offset: u32,
        value: VReg,
    },
    /// 16-bit store: writes the low 16 bits of `value` to `[base + offset]`.
    Store16 {
        base: VReg,
        offset: u32,
        value: VReg,
    },
    /// 8-bit zero-extending load: `dst = u8[base + offset]`.
    Load8U {
        dst: VReg,
        base: VReg,
        offset: u32,
    },
    /// 8-bit sign-extending load: `dst = i8[base + offset]` (sign-extended to i32).
    Load8S {
        dst: VReg,
        base: VReg,
        offset: u32,
    },
    /// 16-bit zero-extending load.
    Load16U {
        dst: VReg,
        base: VReg,
        offset: u32,
    },
    /// 16-bit sign-extending load.
    Load16S {
        dst: VReg,
        base: VReg,
        offset: u32,
    },
    Memcpy {
        dst_addr: VReg,
        src_addr: VReg,
        size: u32,
    },

    // --- Control flow markers ---
    /// If `cond` is false, jump to `else_offset`; if true, fall through. `end_offset` is after the whole construct.
    IfStart {
        cond: VReg,
        else_offset: u32,
        end_offset: u32,
    },
    /// False branch target; if reached by fall-through from the then-arm, jump to the enclosing `IfStart`'s `end_offset`.
    Else,
    LoopStart {
        continuing_offset: u32,
        end_offset: u32,
    },
    SwitchStart {
        selector: VReg,
        end_offset: u32,
    },
    /// If selector matches, run body until `end_offset`; else skip to `end_offset`.
    CaseStart {
        value: i32,
        end_offset: u32,
    },
    DefaultStart {
        end_offset: u32,
    },
    End,

    /// Forward-only region: [`LpirOp::ExitBlock`] jumps to the instruction at `end_offset`
    /// (first op after the matching [`LpirOp::End`]). Closed by [`LpirOp::End`], same pattern
    /// as `IfStart` / `LoopStart` / `SwitchStart`.
    Block {
        end_offset: u32,
    },

    // --- Control flow jumps ---
    Break,
    Continue,
    BrIfNot {
        cond: VReg,
    },
    /// Jump to the end of the nearest enclosing [`LpirOp::Block`] (skips `If`/`Loop`/`Switch` frames).
    ExitBlock,

    // --- Call / return ---
    Call {
        callee: CalleeRef,
        args: VRegRange,
        results: VRegRange,
    },
    Return {
        values: VRegRange,
    },
}

impl LpirOp {
    /// Returns the destination VReg defined by this operation, if any.
    pub fn def_vreg(&self) -> Option<crate::VReg> {
        match self {
            LpirOp::Fadd { dst, .. }
            | LpirOp::Fsub { dst, .. }
            | LpirOp::Fmul { dst, .. }
            | LpirOp::Fdiv { dst, .. }
            | LpirOp::Fneg { dst, .. }
            | LpirOp::Fabs { dst, .. }
            | LpirOp::Fsqrt { dst, .. }
            | LpirOp::Fmin { dst, .. }
            | LpirOp::Fmax { dst, .. }
            | LpirOp::Ffloor { dst, .. }
            | LpirOp::Fceil { dst, .. }
            | LpirOp::Ftrunc { dst, .. }
            | LpirOp::Fnearest { dst, .. }
            | LpirOp::Iadd { dst, .. }
            | LpirOp::Isub { dst, .. }
            | LpirOp::Imul { dst, .. }
            | LpirOp::IdivS { dst, .. }
            | LpirOp::IdivU { dst, .. }
            | LpirOp::IremS { dst, .. }
            | LpirOp::IremU { dst, .. }
            | LpirOp::Ineg { dst, .. }
            | LpirOp::Feq { dst, .. }
            | LpirOp::Fne { dst, .. }
            | LpirOp::Flt { dst, .. }
            | LpirOp::Fle { dst, .. }
            | LpirOp::Fgt { dst, .. }
            | LpirOp::Fge { dst, .. }
            | LpirOp::Ieq { dst, .. }
            | LpirOp::Ine { dst, .. }
            | LpirOp::IltS { dst, .. }
            | LpirOp::IleS { dst, .. }
            | LpirOp::IgtS { dst, .. }
            | LpirOp::IgeS { dst, .. }
            | LpirOp::IltU { dst, .. }
            | LpirOp::IleU { dst, .. }
            | LpirOp::IgtU { dst, .. }
            | LpirOp::IgeU { dst, .. }
            | LpirOp::Iand { dst, .. }
            | LpirOp::Ior { dst, .. }
            | LpirOp::Ixor { dst, .. }
            | LpirOp::Ibnot { dst, .. }
            | LpirOp::Ishl { dst, .. }
            | LpirOp::IshrS { dst, .. }
            | LpirOp::IshrU { dst, .. }
            | LpirOp::IaddImm { dst, .. }
            | LpirOp::IsubImm { dst, .. }
            | LpirOp::ImulImm { dst, .. }
            | LpirOp::IshlImm { dst, .. }
            | LpirOp::IshrSImm { dst, .. }
            | LpirOp::IshrUImm { dst, .. }
            | LpirOp::IeqImm { dst, .. }
            | LpirOp::FtoiSatS { dst, .. }
            | LpirOp::FtoiSatU { dst, .. }
            | LpirOp::ItofS { dst, .. }
            | LpirOp::ItofU { dst, .. }
            | LpirOp::FfromI32Bits { dst, .. }
            | LpirOp::Select { dst, .. }
            | LpirOp::Copy { dst, .. }
            | LpirOp::SlotAddr { dst, .. }
            | LpirOp::Load { dst, .. }
            | LpirOp::Load8U { dst, .. }
            | LpirOp::Load8S { dst, .. }
            | LpirOp::Load16U { dst, .. }
            | LpirOp::Load16S { dst, .. }
            | LpirOp::FconstF32 { dst, .. }
            | LpirOp::IconstI32 { dst, .. } => Some(*dst),
            LpirOp::Store { .. }
            | LpirOp::Store8 { .. }
            | LpirOp::Store16 { .. }
            | LpirOp::Memcpy { .. }
            | LpirOp::Return { .. }
            | LpirOp::Call { .. }
            | LpirOp::IfStart { .. }
            | LpirOp::Else
            | LpirOp::End
            | LpirOp::LoopStart { .. }
            | LpirOp::Break
            | LpirOp::Continue
            | LpirOp::BrIfNot { .. }
            | LpirOp::SwitchStart { .. }
            | LpirOp::CaseStart { .. }
            | LpirOp::DefaultStart { .. }
            | LpirOp::Block { .. }
            | LpirOp::ExitBlock => None,
        }
    }
}
