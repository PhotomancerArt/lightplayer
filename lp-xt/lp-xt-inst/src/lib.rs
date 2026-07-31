// Encoding data (bit layouts, opcode field values, operand ranges) derived from
// espressif/llvm-project:
//   llvm/lib/Target/Xtensa/XtensaInstrFormats.td
//   llvm/lib/Target/Xtensa/XtensaInstrInfo.td
//   llvm/lib/Target/Xtensa/XtensaOperands.td
//   commit f6ee8246025cea8986ce90f5fe3660efcd66cb5f
// Apache License v2.0 WITH LLVM-exception; see
//   licenses/LLVM-Apache-2.0-with-LLVM-exception.txt
//
// PC-relative target formulas and instruction-length rules are facts from the
// Xtensa ISA Reference Manual, cross-checked against xtensa-esp32s3-elf-objdump.
// No GPL source (binutils xtensa-modules.c, QEMU) was copied — see
//   docs/adr/2026-07-28-license-provenance-discipline.md.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod decode;
pub mod disasm;
pub mod encode;
pub mod fp;
pub mod sr;

pub use decode::{DecodeError, decode};
pub use disasm::format_instruction;
pub use encode::encode;
pub use fp::{
    BReg, FReg, FpCmpOp, FpLsiOp, FpLsxOp, FpMovArOp, FpMovBrOp, FpRrOp, FpRrrOp, FpToIntOp,
    IntToFpOp,
};
pub use sr::{SpecialReg, SrOp, UrOp, UserReg};

/// An Xtensa address register `a0`..`a15`.
///
/// The inner value is always in `0..=15`; constructors enforce this.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(u8);

impl Reg {
    /// Create a register from a raw number, panicking if `>15`.
    #[inline]
    pub const fn new(n: u8) -> Reg {
        assert!(n < 16, "Xtensa address register out of range");
        Reg(n)
    }

    /// Create a register from the low 4 bits of `n` (for decode).
    #[inline]
    pub const fn from_nibble(n: u8) -> Reg {
        Reg(n & 0x0f)
    }

    /// The raw register number `0..=15`.
    #[inline]
    pub const fn num(self) -> u8 {
        self.0
    }
}

impl core::fmt::Debug for Reg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "a{}", self.0)
    }
}

/// Three-register ALU operations (`RRR` format, `op0 = 0`). Shape: `op rd, rs, rt`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AluRrr {
    And,
    Or,
    Xor,
    Add,
    Sub,
    Addx2,
    Addx4,
    Addx8,
    Subx2,
    Subx4,
    Subx8,
    Src,
    Mull,
    Muluh,
    Mulsh,
    Quou,
    Quos,
    Remu,
    Rems,
    Min,
    Max,
    Minu,
    Maxu,
    Mul16u,
    Mul16s,
    Moveqz,
    Movnez,
    Movltz,
    Movgez,
}

/// Two-register ops written `op rd, rt` (`RRR`, `op0 = 0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AluRt {
    Neg,
    Abs,
    Sra,
    Srl,
    Nsa,
    Nsau,
}

/// Two-register ops written `op rd, rs` (`RRR`, `op0 = 0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AluRs {
    Sll,
    Movsp,
}

/// One-register ops written `op rs` (`RRR`, `op0 = 0`): set-shift-amount.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftSetOp {
    Ssl,
    Ssr,
    Ssa8l,
    Ssa8b,
}

/// Load opcodes (`RRI8`, `op0 = 2`). Shape: `op rt, rs, offset`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadOp {
    L8ui,
    L16ui,
    L16si,
    L32i,
}

/// Store opcodes (`RRI8`, `op0 = 2`). Shape: `op rt, rs, offset`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreOp {
    S8i,
    S16i,
    S32i,
}

/// Register-register conditional branches (`RRI8`, `op0 = 7`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrRr {
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Ball,
    Bany,
    Bnall,
    Bnone,
    Bbc,
    Bbs,
}

/// Register-immediate conditional branches, signed `b4const` (`BRI8`, `op0 = 6`, `n = 2`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrRi {
    Beqi,
    Bnei,
    Blti,
    Bgei,
}

/// Register-immediate conditional branches, unsigned `b4constu` (`BRI8`, `op0 = 6`, `n = 3`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrRiu {
    Bltui,
    Bgeui,
}

/// Compare-against-zero branches (`BRI12`, `op0 = 6`, `n = 1`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrZ {
    Beqz,
    Bnez,
    Bltz,
    Bgez,
}

/// Windowed / call0 call opcodes taking a PC-relative target (`CALL`, `op0 = 5`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallOp {
    Call0,
    Call4,
    Call8,
    Call12,
}

/// Indirect call opcodes taking a register (`CALLX`, `op0 = 0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallxOp {
    Callx0,
    Callx4,
    Callx8,
    Callx12,
}

/// Zero-operand barrier / sync / nop opcodes (`RRR`, `op0 = 0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NullaryOp {
    Memw,
    Extw,
    Isync,
    Rsync,
    Esync,
    Dsync,
    Nop,
    Ret,
    Retw,
    Ill,
    /// `syscall` — raises a system-call exception on hardware; the emulator
    /// dispatches it to a host `SyscallHandler` (guest ABI, see lp-xt-elf).
    /// Assembler-verified encoding: `00 50 00`.
    Syscall,
}

/// Zero-operand narrow (16-bit) opcodes (`RRRN`, `op0 = 0xD`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NullaryNarrowOp {
    RetN,
    RetwN,
    NopN,
    IllN,
}

/// A decoded Xtensa instruction (the integer subset lp-xt targets).
///
/// PC-relative operands store the *raw encoded immediate* (sign-extended where the
/// field is signed), never an absolute address, so that `encode(decode(w)) == w`
/// holds independent of program counter. Absolute targets are resolved by
/// [`format_instruction`] when a PC is supplied.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inst {
    /// `op rd, rs, rt`
    Rrr(AluRrr, Reg, Reg, Reg),
    /// `op rd, rt`
    Rt(AluRt, Reg, Reg),
    /// `op rd, rs`
    Rs(AluRs, Reg, Reg),
    /// `op rs`
    ShiftSet(ShiftSetOp, Reg),
    /// `ssai imm` (0..=31)
    Ssai(u8),
    /// `slli rd, rs, sa` (sa 0..=31)
    Slli(Reg, Reg, u8),
    /// `srli rd, rt, sa` (sa 0..=15)
    Srli(Reg, Reg, u8),
    /// `srai rd, rt, sa` (sa 0..=31)
    Srai(Reg, Reg, u8),
    /// `extui rd, rt, shiftimm, maskimm` (shiftimm 0..=31, maskimm 1..=16)
    Extui(Reg, Reg, u8, u8),
    /// `sext rd, rs, imm` (imm 7..=22)
    Sext(Reg, Reg, u8),
    /// `mov.n rt, rs` (16-bit)
    MovN(Reg, Reg),
    /// `add.n rd, rs, rt` (16-bit)
    AddN(Reg, Reg, Reg),
    /// `addi.n rd, rs, imm` (16-bit; imm -1..=15, non-zero)
    AddiN(Reg, Reg, i32),
    /// `addi rt, rs, imm8` (imm -128..=127)
    Addi(Reg, Reg, i32),
    /// `addmi rt, rs, imm` (imm -32768..=32512, multiple of 256)
    Addmi(Reg, Reg, i32),
    /// `movi rt, imm` (imm -2048..=2047)
    Movi(Reg, i32),
    /// `movi.n rt, imm` (16-bit; imm -32..=95)
    MoviN(Reg, i32),
    /// `op rt, rs, offset` (byte offset already unscaled)
    Load(LoadOp, Reg, Reg, u32),
    /// `op rt, rs, offset` (byte offset already unscaled)
    Store(StoreOp, Reg, Reg, u32),
    /// `l32i.n rt, rs, offset` (16-bit; offset 0..=60, multiple of 4)
    L32iN(Reg, Reg, u32),
    /// `s32i.n rt, rs, offset` (16-bit; offset 0..=60, multiple of 4)
    S32iN(Reg, Reg, u32),
    /// `l32r rt, label`. Stores the raw 16-bit field; target is backward-only.
    L32r(Reg, u16),
    /// `op rs, rt, target`. Stores signed 8-bit PC-relative offset.
    BranchRr(BrRr, Reg, Reg, i32),
    /// `op rs, imm, target`. `imm` is the decoded `b4const` value; offset is signed 8-bit.
    BranchRi(BrRi, Reg, i32, i32),
    /// `op rs, imm, target`. `imm` is the decoded `b4constu` value; offset is signed 8-bit.
    BranchRiu(BrRiu, Reg, i32, i32),
    /// `op rs, target`. Stores signed 12-bit PC-relative offset.
    BranchZ(BrZ, Reg, i32),
    /// `op rs, imm, target` bit-test-immediate branch. offset signed 8-bit.
    BranchBiI(bool /* set? bbsi:true, bbci:false */, Reg, u8, i32),
    /// `beqz.n`/`bnez.n rs, target` (16-bit). Stores unsigned 6-bit forward offset.
    BranchZN(bool /* nez? */, Reg, u32),
    /// `j target`. Stores signed 18-bit byte offset.
    J(i32),
    /// `jx rs`
    Jx(Reg),
    /// `op target`. Stores signed 18-bit *word* offset field (as decoded, sign-extended).
    Call(CallOp, i32),
    /// `op rs`
    Callx(CallxOp, Reg),
    /// `entry rs, imm` (imm 0..=32760, multiple of 8)
    Entry(Reg, u32),
    /// zero-operand barrier/sync/return (24-bit)
    Nullary(NullaryOp),
    /// zero-operand narrow return/nop (16-bit)
    NullaryN(NullaryNarrowOp),

    // --- floating point (see the [`fp`] module doc for the normative subset) ---
    /// `op fr, fs, ft`
    FpRrr(FpRrrOp, FReg, FReg, FReg),
    /// `op fr, fs`
    FpRr(FpRrOp, FReg, FReg),
    /// `const.s fr, imm` (imm 0..=15 selects a constant, not a value)
    ConstS(FReg, u8),
    /// `rfr ar, fs` — FR → AR bit-for-bit
    Rfr(Reg, FReg),
    /// `wfr fr, as` — AR → FR bit-for-bit
    Wfr(FReg, Reg),
    /// `op fr, fs, at` — FP conditional move on an address register
    FpMovAr(FpMovArOp, FReg, FReg, Reg),
    /// `op fr, fs, bt` — FP conditional move on a boolean register
    FpMovBr(FpMovBrOp, FReg, FReg, BReg),
    /// `op br, fs, ft` — FP compare, result to a boolean register
    FpCmp(FpCmpOp, BReg, FReg, FReg),
    /// `op ar, fs, imm` (imm 0..=15 is a binary pre-scale)
    FpToInt(FpToIntOp, Reg, FReg, u8),
    /// `op fr, as, imm` (imm 0..=15 is a binary post-scale)
    IntToFp(IntToFpOp, FReg, Reg, u8),
    /// `op fr, as, at` — indexed FP load/store
    FpLsx(FpLsxOp, FReg, Reg, Reg),
    /// `op ft, as, offset` (offset 0..=1020, multiple of 4)
    FpLsi(FpLsiOp, FReg, Reg, u32),

    // --- boolean register file (the Boolean core option) ---
    /// `movt`/`movf ar, as, bt` — conditional AR move on a boolean register
    MovBool(bool /* set? movt:movf */, Reg, Reg, BReg),
    /// `bt`/`bf bs, target`. Stores the signed 8-bit PC-relative offset.
    BranchBool(bool /* set? bt:bf */, BReg, i32),

    // --- special / user registers (see the [`sr`] module doc) ---
    /// `rsr.<sr>`/`wsr.<sr>`/`xsr.<sr> at`
    Sr(SrOp, SpecialReg, Reg),
    /// `rur.<ur>`/`wur.<ur> at`
    Ur(UrOp, UserReg, Reg),
}

/// The `b4const` lookup table (signed branch immediates), indexed by the 4-bit field.
///
/// Derived from `XtensaOperands.td` `b4const`.
pub const B4CONST: [i32; 16] = [-1, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 16, 32, 64, 128, 256];

/// The `b4constu` lookup table (unsigned branch immediates), indexed by the 4-bit field.
///
/// Derived from `XtensaOperands.td` `b4constu`.
pub const B4CONSTU: [i32; 16] = [
    32768, 65536, 2, 3, 4, 5, 6, 7, 8, 10, 12, 16, 32, 64, 128, 256,
];

/// Map a decoded `b4const` value back to its 4-bit field index, if representable.
pub fn b4const_index(value: i32) -> Option<u8> {
    B4CONST.iter().position(|&v| v == value).map(|i| i as u8)
}

/// Map a decoded `b4constu` value back to its 4-bit field index, if representable.
pub fn b4constu_index(value: i32) -> Option<u8> {
    B4CONSTU.iter().position(|&v| v == value).map(|i| i as u8)
}

/// The number of bytes an instruction beginning with `byte0` occupies, per the
/// Xtensa Code Density length rule (core ISA + density option only).
///
/// This is the base-ISA rule: `op0` (bits 3..0 of the first byte) in `0x8..=0xD`
/// selects a 16-bit instruction, everything else a 24-bit instruction. It does
/// **not** account for the ESP32-S3 `ee.*` 32-bit DSP forms, which this crate
/// does not decode.
#[inline]
pub const fn base_inst_len(byte0: u8) -> usize {
    match byte0 & 0x0f {
        0x8..=0xd => 2,
        _ => 3,
    }
}
