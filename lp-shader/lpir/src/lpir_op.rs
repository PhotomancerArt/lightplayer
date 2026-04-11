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

    // --- Control flow jumps ---
    Break,
    Continue,
    BrIfNot {
        cond: VReg,
    },

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
