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

/// The synchronising word accesses (`RRI8`, `op0 = 2`), kept apart from the
/// plain [`LoadOp`]/[`StoreOp`] families because their *semantics* differ, not
/// their shape: all three are `op at, as, offset` with a 4-scaled unsigned
/// 8-bit offset (0..=1020).
///
/// This crate holds no semantics for any of them — see the module doc — but a
/// machine that does needs them told apart from `l32i`/`s32i` at decode time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AtomicLsOp {
    /// `l32ai at, as, off` — load 32 bits with acquire ordering; `r = 0xB`.
    L32ai,
    /// `s32ri at, as, off` — store 32 bits with release ordering; `r = 0xF`.
    S32ri,
    /// `s32c1i at, as, off` — store-conditional against `SCOMPARE1`; `r = 0xE`.
    S32c1i,
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

/// The zero-overhead loop opcodes (`BRI8`, `op0 = 6`, `n = 3`, `m = 1`).
///
/// All three take `op as, label`: `as` is the trip count and the label is the
/// first instruction *after* the loop body, which the hardware latches into
/// `LEND`. See [`crate::disasm::loop_end`] for the address formula.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoopOp {
    /// `loop as, label` — `r = 8`. Always executes the body `AR[s]` times.
    Loop,
    /// `loopnez as, label` — `r = 9`. Skips the body entirely if `AR[s] == 0`.
    Loopnez,
    /// `loopgtz as, label` — `r = 0xA`. Skips the body if `AR[s] <= 0` signed.
    Loopgtz,
}

/// The zero-operand exception returns (`RRR`, `op0 = 0`, `op1 = 0`, `op2 = 0`,
/// `r = 3`, `t = 0`, sub-selected by `s`).
///
/// Kept out of [`NullaryOp`] deliberately: those are instructions the user-mode
/// runner executes, these are privileged control transfers the machine-mode
/// hart owns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RfOp {
    /// `rfe` — return from a level-1 exception; `s = 0`. Clears `PS.EXCM`.
    Rfe,
    /// `rfde` — return from a double exception; `s = 2`.
    Rfde,
    /// `rfwo` — return from a window **overflow** handler; `s = 4`.
    Rfwo,
    /// `rfwu` — return from a window **underflow** handler; `s = 5`.
    Rfwu,
}

/// The windowed spill/reload accesses (`RRR`, `op0 = 0`, `op1 = 9`).
///
/// `op at, as, offset`, offset a **negative** multiple of 4 in -64..=-4 held in
/// the `r` field as `(offset / 4) + 16`. These are the instructions the
/// `_WindowOverflow*` / `_WindowUnderflow*` vectors are made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowLsOp {
    /// `l32e at, as, off` — window-reload load; `op2 = 0`.
    L32e,
    /// `s32e at, as, off` — window-spill store; `op2 = 4`.
    S32e,
}

/// Boolean-file logic ops (`RRR`, `op0 = 0`, `op1 = 2`). Shape: `op br, bs, bt`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoolOp {
    /// `andb br, bs, bt` — `op2 = 0`.
    Andb,
    /// `andbc br, bs, bt` — `bs AND NOT bt`; `op2 = 1`.
    Andbc,
    /// `orb br, bs, bt` — `op2 = 2`.
    Orb,
    /// `orbc br, bs, bt` — `bs OR NOT bt`; `op2 = 3`.
    Orbc,
    /// `xorb br, bs, bt` — `op2 = 4`.
    Xorb,
}

/// Boolean-file reductions (`RRR`, `op0 = 0`, `op1 = 0`, `op2 = 0`, by `r`).
///
/// Shape: `op br, bs` where `bs` names the **first** register of a 4- or
/// 8-register aligned group; objdump renders the whole range
/// (`all4 b0, b4:b5:b6:b7`), which is the same field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoolAllOp {
    /// `any4 br, bs` — `r = 8`.
    Any4,
    /// `all4 br, bs` — `r = 9`.
    All4,
    /// `any8 br, bs` — `r = 0xA`.
    Any8,
    /// `all8 br, bs` — `r = 0xB`.
    All8,
}

/// Region-protection / TLB accesses taking `at, as` (`RRR`, `op0 = 0`,
/// `op1 = 0`, `op2 = 5`, sub-selected by `r`).
///
/// **Decode and disassembly only.** This crate holds no semantics for any of
/// them, and the "accept and remember" model a machine needs is P3's or later.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TlbOp {
    /// `ritlb0 at, as` — `r = 3`.
    Ritlb0,
    /// `pitlb at, as` — `r = 5`.
    Pitlb,
    /// `witlb at, as` — `r = 6`.
    Witlb,
    /// `ritlb1 at, as` — `r = 7`.
    Ritlb1,
    /// `rdtlb0 at, as` — `r = 0xB`.
    Rdtlb0,
    /// `pdtlb at, as` — `r = 0xD`.
    Pdtlb,
    /// `wdtlb at, as` — `r = 0xE`.
    Wdtlb,
    /// `rdtlb1 at, as` — `r = 0xF`.
    Rdtlb1,
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
    /// `clamps rd, rs, imm` (imm 7..=22) — saturate to a signed `imm+1`-bit range.
    Clamps(Reg, Reg, u8),
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
    /// `op at, as, offset` — the synchronising word accesses (`l32ai`,
    /// `s32ri`, `s32c1i`). Byte offset already unscaled, 0..=1020.
    AtomicLs(AtomicLsOp, Reg, Reg, u32),
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
    /// `op as, label` — a zero-overhead loop. Stores the **unsigned** 8-bit
    /// encoded offset, not an address; [`crate::disasm::loop_end`] turns it
    /// into the `LEND` value.
    Loop(LoopOp, Reg, u8),
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
    /// `rfe`/`rfde`/`rfwo`/`rfwu` — privileged exception returns.
    Rf(RfOp),
    /// `rfi level` (level 0..=15) — return from a level-`n` interrupt.
    Rfi(u8),
    /// `rsil at, level` — read PS and set `PS.INTLEVEL` to `level` (0..=15).
    Rsil(Reg, u8),
    /// `waiti level` (0..=15) — wait for an interrupt above `level`.
    Waiti(u8),
    /// `rotw imm` (imm -8..=7) — rotate the register window by `imm` groups.
    Rotw(i8),
    /// `break imms, immt` (both 0..=15) — raise a debug exception.
    Break(u8, u8),
    /// `break.n imms` (0..=15, 16-bit) — the density form.
    BreakN(u8),
    /// `op at, as, offset` — windowed spill/reload; offset -64..=-4, step 4.
    WindowLs(WindowLsOp, Reg, Reg, i32),
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
    /// `op br, bs, bt` — boolean-file logic.
    BoolLogic(BoolOp, BReg, BReg, BReg),
    /// `op br, bs` — boolean-file reduction over an aligned 4- or 8-group.
    BoolAll(BoolAllOp, BReg, BReg),

    // --- region protection (decode and disassembly only) ---
    /// `op at, as` — a TLB read/probe/write.
    Tlb(TlbOp, Reg, Reg),
    /// `idtlb as` (`data = true`, `r = 0xC`) / `iitlb as` (`r = 4`) — invalidate.
    TlbInv(bool /* data? idtlb:iitlb */, Reg),
    /// `rer at, as` (`write = false`) / `wer at, as` — external-register access.
    ExtReg(bool /* write? wer:rer */, Reg, Reg),

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
