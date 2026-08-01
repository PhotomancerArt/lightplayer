// Encoding data derived from espressif/llvm-project
//   llvm/lib/Target/Xtensa/{XtensaInstrFormats,XtensaInstrInfo,XtensaOperands}.td
//   commit f6ee8246025cea8986ce90f5fe3660efcd66cb5f
// Apache License v2.0 WITH LLVM-exception; see
//   licenses/LLVM-Apache-2.0-with-LLVM-exception.txt
//
//! Variable-length Xtensa decode. `decode` returns the instruction plus its byte
//! length (2 or 3 for the supported integer subset).

use crate::*;

/// Why a byte slice could not be decoded into a supported [`Inst`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecodeError {
    /// Fewer bytes were available than the instruction length requires.
    Truncated {
        /// Bytes needed for this instruction length.
        need: usize,
        /// Bytes actually present.
        got: usize,
    },
    /// The opcode is well-formed but outside the subset this crate models
    /// (ESP32-S3 DSP `ee.*`, most system/privileged registers, boolean logic
    /// ops, loop, atomics, windowed spill, etc.).
    Unsupported {
        /// The instruction word (up to 3 bytes, little-endian assembled).
        word: u32,
        /// The byte length determined by the density length rule.
        len: usize,
    },
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::Truncated { need, got } => {
                write!(f, "truncated instruction: need {need} bytes, got {got}")
            }
            DecodeError::Unsupported { word, len } => {
                write!(f, "unsupported opcode: word={word:#08x} len={len}")
            }
        }
    }
}

/// Assemble the little-endian instruction word from `len` bytes.
#[inline]
fn word_of(bytes: &[u8], len: usize) -> u32 {
    let mut w = 0u32;
    let mut i = 0;
    while i < len {
        w |= (bytes[i] as u32) << (8 * i);
        i += 1;
    }
    w
}

// --- field extractors (see XtensaInstrFormats.td) ---
#[inline]
fn op0(w: u32) -> u8 {
    (w & 0xf) as u8
}
#[inline]
fn t(w: u32) -> u8 {
    ((w >> 4) & 0xf) as u8
}
#[inline]
fn s(w: u32) -> u8 {
    ((w >> 8) & 0xf) as u8
}
#[inline]
fn r(w: u32) -> u8 {
    ((w >> 12) & 0xf) as u8
}
#[inline]
fn op1(w: u32) -> u8 {
    ((w >> 16) & 0xf) as u8
}
#[inline]
fn op2(w: u32) -> u8 {
    ((w >> 20) & 0xf) as u8
}
#[inline]
fn imm8(w: u32) -> u8 {
    ((w >> 16) & 0xff) as u8
}
#[inline]
fn reg_t(w: u32) -> Reg {
    Reg::from_nibble(t(w))
}
#[inline]
fn reg_s(w: u32) -> Reg {
    Reg::from_nibble(s(w))
}
#[inline]
fn reg_r(w: u32) -> Reg {
    Reg::from_nibble(r(w))
}
#[inline]
fn freg_t(w: u32) -> FReg {
    FReg::from_nibble(t(w))
}
#[inline]
fn freg_s(w: u32) -> FReg {
    FReg::from_nibble(s(w))
}
#[inline]
fn freg_r(w: u32) -> FReg {
    FReg::from_nibble(r(w))
}
#[inline]
fn breg_t(w: u32) -> BReg {
    BReg::from_nibble(t(w))
}
#[inline]
fn breg_s(w: u32) -> BReg {
    BReg::from_nibble(s(w))
}
#[inline]
fn breg_r(w: u32) -> BReg {
    BReg::from_nibble(r(w))
}

/// Sign-extend the low `bits` of `v`.
#[inline]
fn sext(v: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((v << shift) as i32) >> shift
}

/// Decode one instruction from the front of `bytes`.
///
/// Returns the decoded [`Inst`] and its length in bytes. The length is derived
/// from the first byte via the density rule *before* opcode recognition, so an
/// [`DecodeError::Unsupported`] still carries the correct length to advance by.
pub fn decode(bytes: &[u8]) -> Result<(Inst, usize), DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::Truncated { need: 2, got: 0 });
    }
    let len = base_inst_len(bytes[0]);
    if bytes.len() < len {
        return Err(DecodeError::Truncated {
            need: len,
            got: bytes.len(),
        });
    }
    let w = word_of(bytes, len);
    let unsupported = || DecodeError::Unsupported { word: w, len };

    let inst = if len == 2 {
        decode16(w).ok_or_else(unsupported)?
    } else {
        decode24(w).ok_or_else(unsupported)?
    };
    Ok((inst, len))
}

/// Decode a 24-bit instruction word. `None` means "not in the supported subset".
fn decode24(w: u32) -> Option<Inst> {
    match op0(w) {
        0x0 => decode_qrst(w),
        0x1 => Some(Inst::L32r(reg_t(w), ((w >> 8) & 0xffff) as u16)),
        0x2 => decode_rri8_ls_movi(w),
        0x3 => decode_fp_lsi(w),
        0x5 => {
            // CALL format: call0/4/8/12
            let n = (w >> 4) & 0x3;
            let offset = sext((w >> 6) & 0x3ffff, 18);
            let op = match n {
                0 => CallOp::Call0,
                1 => CallOp::Call4,
                2 => CallOp::Call8,
                3 => CallOp::Call12,
                _ => unreachable!(),
            };
            Some(Inst::Call(op, offset))
        }
        0x6 => decode_op0_6(w),
        0x7 => decode_op0_7(w),
        _ => None,
    }
}

/// `op0 = 0`: the QRST major opcode, sub-decoded by `op1`.
fn decode_qrst(w: u32) -> Option<Inst> {
    match op1(w) {
        0x0 => decode_rst0(w),
        0x1 => decode_rst1(w),
        0x2 => decode_rst2(w),
        0x3 => decode_rst3(w),
        0x4 | 0x5 => {
            // EXTUI: dest = r field, src = t field, shiftimm = s | (op1{0}<<4),
            // maskimm = op2 + 1.
            let shiftimm = s(w) | (((w >> 16) & 0x1) as u8) << 4; // 0..31
            let maskimm = op2(w) + 1; // 1..16
            Some(Inst::Extui(reg_r(w), reg_t(w), shiftimm, maskimm))
        }
        0x8 => decode_fp_lsx(w),
        0xa => decode_fp0(w),
        0xb => decode_fp1(w),
        _ => None,
    }
}

/// `op0 = 0, op1 = 8`: indexed FP load/store, sub-decoded by `op2`.
fn decode_fp_lsx(w: u32) -> Option<Inst> {
    let op = match op2(w) {
        0x0 => FpLsxOp::Lsx,
        0x1 => FpLsxOp::Lsxp,
        0x4 => FpLsxOp::Ssx,
        0x5 => FpLsxOp::Ssxp,
        _ => return None,
    };
    Some(Inst::FpLsx(op, freg_r(w), reg_s(w), reg_t(w)))
}

/// `op0 = 0, op1 = 0xA`: FP arithmetic and conversions, sub-decoded by `op2`.
/// `op2 = 0xF` is the FP1 sub-group, keyed by `t` instead.
fn decode_fp0(w: u32) -> Option<Inst> {
    let rrr = |op| Some(Inst::FpRrr(op, freg_r(w), freg_s(w), freg_t(w)));
    // Conversions carry a 0..=15 binary scale in the `t` field.
    let to_int = |op| Some(Inst::FpToInt(op, reg_r(w), freg_s(w), t(w)));
    let to_fp = |op| Some(Inst::IntToFp(op, freg_r(w), reg_s(w), t(w)));
    match op2(w) {
        0x0 => rrr(FpRrrOp::AddS),
        0x1 => rrr(FpRrrOp::SubS),
        0x2 => rrr(FpRrrOp::MulS),
        0x4 => rrr(FpRrrOp::MaddS),
        0x5 => rrr(FpRrrOp::MsubS),
        0x6 => rrr(FpRrrOp::MaddnS),
        0x7 => rrr(FpRrrOp::DivnS),
        0x8 => to_int(FpToIntOp::RoundS),
        0x9 => to_int(FpToIntOp::TruncS),
        0xa => to_int(FpToIntOp::FloorS),
        0xb => to_int(FpToIntOp::CeilS),
        0xc => to_fp(IntToFpOp::FloatS),
        0xd => to_fp(IntToFpOp::UfloatS),
        0xe => to_int(FpToIntOp::UtruncS),
        0xf => decode_fp1_unary(w),
        _ => None,
    }
}

/// `op0 = 0, op1 = 0xA, op2 = 0xF`: the FP1 unary group, keyed by `t`.
///
/// `t = 2` is unassigned as far as `xtensa-esp32s3-elf-objdump` is concerned;
/// it stays unsupported rather than being guessed at. (`t = 0xC` was once on
/// that list too — wrongly: objdump disassembles it as `mksadj.s`, and the
/// toolchain's `__ieee754_sqrtf` sequence uses it. Found by M6 P6.)
fn decode_fp1_unary(w: u32) -> Option<Inst> {
    let rr = |op| Some(Inst::FpRr(op, freg_r(w), freg_s(w)));
    match t(w) {
        0x0 => rr(FpRrOp::MovS),
        0x1 => rr(FpRrOp::AbsS),
        // const.s takes an immediate in the `s` field, not a source register.
        0x3 => Some(Inst::ConstS(freg_r(w), s(w))),
        0x4 => Some(Inst::Rfr(reg_r(w), freg_s(w))),
        0x5 => Some(Inst::Wfr(freg_r(w), reg_s(w))),
        0x6 => rr(FpRrOp::NegS),
        0x7 => rr(FpRrOp::Div0S),
        0x8 => rr(FpRrOp::Recip0S),
        0x9 => rr(FpRrOp::Sqrt0S),
        0xa => rr(FpRrOp::Rsqrt0S),
        0xb => rr(FpRrOp::Nexp01S),
        0xc => rr(FpRrOp::MksadjS),
        0xd => rr(FpRrOp::MkdadjS),
        0xe => rr(FpRrOp::AddexpS),
        0xf => rr(FpRrOp::AddexpmS),
        _ => None,
    }
}

/// `op0 = 0, op1 = 0xB`: FP compares (result → BR) and conditional moves,
/// sub-decoded by `op2`.
fn decode_fp1(w: u32) -> Option<Inst> {
    let cmp = |op| Some(Inst::FpCmp(op, breg_r(w), freg_s(w), freg_t(w)));
    let mov_ar = |op| Some(Inst::FpMovAr(op, freg_r(w), freg_s(w), reg_t(w)));
    let mov_br = |op| Some(Inst::FpMovBr(op, freg_r(w), freg_s(w), breg_t(w)));
    match op2(w) {
        0x1 => cmp(FpCmpOp::UnS),
        0x2 => cmp(FpCmpOp::OeqS),
        0x3 => cmp(FpCmpOp::UeqS),
        0x4 => cmp(FpCmpOp::OltS),
        0x5 => cmp(FpCmpOp::UltS),
        0x6 => cmp(FpCmpOp::OleS),
        0x7 => cmp(FpCmpOp::UleS),
        0x8 => mov_ar(FpMovArOp::MoveqzS),
        0x9 => mov_ar(FpMovArOp::MovnezS),
        0xa => mov_ar(FpMovArOp::MovltzS),
        0xb => mov_ar(FpMovArOp::MovgezS),
        0xc => mov_br(FpMovBrOp::MovfS),
        0xd => mov_br(FpMovBrOp::MovtS),
        _ => None,
    }
}

/// `op0 = 3`: immediate-offset FP load/store (`RRI8`), disambiguated by `r`.
/// The `imm8` field holds `offset / 4`.
fn decode_fp_lsi(w: u32) -> Option<Inst> {
    let op = match r(w) {
        0x0 => FpLsiOp::Lsi,
        0x4 => FpLsiOp::Ssi,
        0x8 => FpLsiOp::Lsip,
        0xc => FpLsiOp::Ssip,
        _ => return None,
    };
    Some(Inst::FpLsi(op, freg_t(w), reg_s(w), imm8(w) as u32 * 4))
}

/// `op0 = 0, op1 = 0`: RST0, sub-decoded by `op2`.
fn decode_rst0(w: u32) -> Option<Inst> {
    let alu = |op| Some(Inst::Rrr(op, reg_r(w), reg_s(w), reg_t(w)));
    match op2(w) {
        0x0 => decode_st0(w),
        0x1 => alu(AluRrr::And),
        0x2 => alu(AluRrr::Or),
        0x3 => alu(AluRrr::Xor),
        0x4 => decode_st1(w),
        0x6 => match s(w) {
            0x0 => Some(Inst::Rt(AluRt::Neg, reg_r(w), reg_t(w))),
            0x1 => Some(Inst::Rt(AluRt::Abs, reg_r(w), reg_t(w))),
            _ => None,
        },
        0x8 => alu(AluRrr::Add),
        0x9 => alu(AluRrr::Addx2),
        0xa => alu(AluRrr::Addx4),
        0xb => alu(AluRrr::Addx8),
        0xc => alu(AluRrr::Sub),
        0xd => alu(AluRrr::Subx2),
        0xe => alu(AluRrr::Subx4),
        0xf => alu(AluRrr::Subx8),
        _ => None,
    }
}

/// `op0 = 0, op1 = 0, op2 = 0`: ST0, sub-decoded by `r`.
fn decode_st0(w: u32) -> Option<Inst> {
    match r(w) {
        0x0 => {
            // SNM0 (CALLX format): sub by m = bits 7..6, n = bits 5..4. The `s`
            // field is the register; there is no `t` field here (bits 7..4 == m,n).
            let m = (w >> 6) & 0x3;
            let n = (w >> 4) & 0x3;
            match m {
                0x0 if n == 0 && s(w) == 0 => Some(Inst::Nullary(NullaryOp::Ill)),
                0x2 => match n {
                    0x0 if s(w) == 0 => Some(Inst::Nullary(NullaryOp::Ret)),
                    0x1 if s(w) == 0 => Some(Inst::Nullary(NullaryOp::Retw)),
                    0x2 => Some(Inst::Jx(reg_s(w))),
                    _ => None,
                },
                0x3 => {
                    let op = match n {
                        0x0 => CallxOp::Callx0,
                        0x1 => CallxOp::Callx4,
                        0x2 => CallxOp::Callx8,
                        0x3 => CallxOp::Callx12,
                        _ => unreachable!(),
                    };
                    Some(Inst::Callx(op, reg_s(w)))
                }
                _ => None,
            }
        }
        0x1 => Some(Inst::Rs(AluRs::Movsp, reg_t(w), reg_s(w))),
        // SYSCALL: r=5, s=0, t=0 (assembler golden bytes `00 50 00`).
        0x5 if s(w) == 0 && t(w) == 0 => Some(Inst::Nullary(NullaryOp::Syscall)),
        0x2 => {
            // SYNC group: distinguished by t, s must be 0.
            if s(w) != 0 {
                return None;
            }
            let op = match t(w) {
                0x0 => NullaryOp::Isync,
                0x1 => NullaryOp::Rsync,
                0x2 => NullaryOp::Esync,
                0x3 => NullaryOp::Dsync,
                0xc => NullaryOp::Memw,
                0xd => NullaryOp::Extw,
                0xf => NullaryOp::Nop,
                _ => return None,
            };
            Some(Inst::Nullary(op))
        }
        _ => None,
    }
}

/// `op0 = 0, op1 = 0, op2 = 4`: ST1, sub-decoded by `r`.
fn decode_st1(w: u32) -> Option<Inst> {
    match r(w) {
        0x0 if t(w) == 0 => Some(Inst::ShiftSet(ShiftSetOp::Ssr, reg_s(w))),
        0x1 if t(w) == 0 => Some(Inst::ShiftSet(ShiftSetOp::Ssl, reg_s(w))),
        0x2 if t(w) == 0 => Some(Inst::ShiftSet(ShiftSetOp::Ssa8l, reg_s(w))),
        0x3 if t(w) == 0 => Some(Inst::ShiftSet(ShiftSetOp::Ssa8b, reg_s(w))),
        0x4 => {
            // SSAI: imm = s | (t{0} << 4)
            let imm = s(w) | ((t(w) & 0x1) << 4);
            Some(Inst::Ssai(imm))
        }
        0xe => Some(Inst::Rt(AluRt::Nsa, reg_t(w), reg_s(w))),
        0xf => Some(Inst::Rt(AluRt::Nsau, reg_t(w), reg_s(w))),
        _ => None,
    }
}

/// `op0 = 0, op1 = 1`: RST1, sub-decoded by `op2` (shifts, mul16).
fn decode_rst1(w: u32) -> Option<Inst> {
    match op2(w) {
        0x0 | 0x1 => {
            // SLLI: the 5-bit field holds (32 - shift); field 0 == shift 32.
            let field = t(w) | ((op2(w) & 0x1) << 4);
            let shift = 32 - field as u16;
            Some(Inst::Slli(reg_r(w), reg_s(w), shift as u8))
        }
        0x2 | 0x3 => {
            // SRAI: sa = s | (op2{0} << 4)
            let sa = s(w) | ((op2(w) & 0x1) << 4);
            Some(Inst::Srai(reg_r(w), reg_t(w), sa))
        }
        0x4 => Some(Inst::Srli(reg_r(w), reg_t(w), s(w))),
        // XSR sits in RST1, unlike RSR/WSR which sit in RST3.
        0x6 => sr_access(w, SrOp::Xsr),
        0x8 => Some(Inst::Rrr(AluRrr::Src, reg_r(w), reg_s(w), reg_t(w))),
        0x9 if s(w) == 0 => Some(Inst::Rt(AluRt::Srl, reg_r(w), reg_t(w))),
        0xa if t(w) == 0 => Some(Inst::Rs(AluRs::Sll, reg_r(w), reg_s(w))),
        0xb if s(w) == 0 => Some(Inst::Rt(AluRt::Sra, reg_r(w), reg_t(w))),
        0xc => Some(Inst::Rrr(AluRrr::Mul16u, reg_r(w), reg_s(w), reg_t(w))),
        0xd => Some(Inst::Rrr(AluRrr::Mul16s, reg_r(w), reg_s(w), reg_t(w))),
        _ => None,
    }
}

/// `op0 = 0, op1 = 2`: RST2, sub-decoded by `op2` (mul32, div32).
fn decode_rst2(w: u32) -> Option<Inst> {
    let alu = |op| Some(Inst::Rrr(op, reg_r(w), reg_s(w), reg_t(w)));
    match op2(w) {
        0x8 => alu(AluRrr::Mull),
        0xa => alu(AluRrr::Muluh),
        0xb => alu(AluRrr::Mulsh),
        0xc => alu(AluRrr::Quou),
        0xd => alu(AluRrr::Quos),
        0xe => alu(AluRrr::Remu),
        0xf => alu(AluRrr::Rems),
        _ => None,
    }
}

/// `op0 = 0, op1 = 3`: RST3, sub-decoded by `op2` (sext, min/max, cmov).
fn decode_rst3(w: u32) -> Option<Inst> {
    let alu = |op| Some(Inst::Rrr(op, reg_r(w), reg_s(w), reg_t(w)));
    match op2(w) {
        0x0 => sr_access(w, SrOp::Rsr),
        0x1 => sr_access(w, SrOp::Wsr),
        0x2 => Some(Inst::Sext(reg_r(w), reg_s(w), t(w) + 7)),
        0x4 => alu(AluRrr::Min),
        0x5 => alu(AluRrr::Max),
        0x6 => alu(AluRrr::Minu),
        0x7 => alu(AluRrr::Maxu),
        0x8 => alu(AluRrr::Moveqz),
        0x9 => alu(AluRrr::Movnez),
        0xa => alu(AluRrr::Movltz),
        0xb => alu(AluRrr::Movgez),
        0xc => Some(Inst::MovBool(false, reg_r(w), reg_s(w), breg_t(w))),
        0xd => Some(Inst::MovBool(true, reg_r(w), reg_s(w), breg_t(w))),
        // RUR reads the user register from (s << 4) | t, and writes `r`.
        0xe => UserReg::from_num((s(w) << 4) | t(w))
            .map(|ur| Inst::Ur(UrOp::Rur, ur, Reg::from_nibble(r(w)))),
        // WUR takes the user register from (r << 4) | s, and reads `t`.
        0xf => UserReg::from_num((r(w) << 4) | s(w))
            .map(|ur| Inst::Ur(UrOp::Wur, ur, Reg::from_nibble(t(w)))),
        _ => None,
    }
}

/// Decode an `RSR`/`WSR`/`XSR` word: the special-register number is
/// `(r << 4) | s` and `t` is the address register. `None` for any register
/// outside [`SpecialReg`]'s narrow modeled set.
fn sr_access(w: u32, op: SrOp) -> Option<Inst> {
    SpecialReg::from_num((r(w) << 4) | s(w)).map(|sreg| Inst::Sr(op, sreg, reg_t(w)))
}

/// `op0 = 2`: RRI8 loads/stores plus movi/addi/addmi (disambiguated by `r`).
fn decode_rri8_ls_movi(w: u32) -> Option<Inst> {
    let base = reg_s(w);
    let dst = reg_t(w);
    let off8 = imm8(w) as u32;
    match r(w) {
        0x0 => Some(Inst::Load(LoadOp::L8ui, dst, base, off8)),
        0x1 => Some(Inst::Load(LoadOp::L16ui, dst, base, off8 * 2)),
        0x2 => Some(Inst::Load(LoadOp::L32i, dst, base, off8 * 4)),
        0x9 => Some(Inst::Load(LoadOp::L16si, dst, base, off8 * 2)),
        0x4 => Some(Inst::Store(StoreOp::S8i, dst, base, off8)),
        0x5 => Some(Inst::Store(StoreOp::S16i, dst, base, off8 * 2)),
        0x6 => Some(Inst::Store(StoreOp::S32i, dst, base, off8 * 4)),
        0xa => {
            // MOVI: imm = sext12( (s << 8) | imm8 )
            let v = ((s(w) as u32) << 8) | (imm8(w) as u32);
            Some(Inst::Movi(dst, sext(v, 12)))
        }
        0xc => Some(Inst::Addi(dst, base, imm8(w) as i8 as i32)),
        0xd => {
            // ADDMI: imm = sext8(imm8) << 8
            let v = (imm8(w) as i8 as i32) << 8;
            Some(Inst::Addmi(dst, base, v))
        }
        _ => None,
    }
}

/// `op0 = 6`: J / BZ / BI0 / BI1 / ENTRY, sub-decoded by `n` then `m`.
fn decode_op0_6(w: u32) -> Option<Inst> {
    let n = (w >> 4) & 0x3;
    let m = (w >> 6) & 0x3;
    match n {
        0x0 => {
            // J: 18-bit signed byte offset
            Some(Inst::J(sext((w >> 6) & 0x3ffff, 18)))
        }
        0x1 => {
            // BZ (BRI12): beqz/bnez/bltz/bgez
            let off = sext((w >> 12) & 0xfff, 12);
            let op = match m {
                0x0 => BrZ::Beqz,
                0x1 => BrZ::Bnez,
                0x2 => BrZ::Bltz,
                0x3 => BrZ::Bgez,
                _ => unreachable!(),
            };
            Some(Inst::BranchZ(op, reg_s(w), off))
        }
        0x2 => {
            // BI0 (BRI8): beqi/bnei/blti/bgei with b4const
            let off = imm8(w) as i8 as i32;
            let val = B4CONST[r(w) as usize];
            let op = match m {
                0x0 => BrRi::Beqi,
                0x1 => BrRi::Bnei,
                0x2 => BrRi::Blti,
                0x3 => BrRi::Bgei,
                _ => unreachable!(),
            };
            Some(Inst::BranchRi(op, reg_s(w), val, off))
        }
        0x3 => match m {
            0x0 => {
                // ENTRY (BRI12): imm = imm12 << 3
                let imm12 = (w >> 12) & 0xfff;
                Some(Inst::Entry(reg_s(w), imm12 << 3))
            }
            0x2 => {
                let off = imm8(w) as i8 as i32;
                Some(Inst::BranchRiu(
                    BrRiu::Bltui,
                    reg_s(w),
                    B4CONSTU[r(w) as usize],
                    off,
                ))
            }
            0x3 => {
                let off = imm8(w) as i8 as i32;
                Some(Inst::BranchRiu(
                    BrRiu::Bgeui,
                    reg_s(w),
                    B4CONSTU[r(w) as usize],
                    off,
                ))
            }
            // BI1: bf/bt boolean branches (r = 0/1); the loop family
            // (r = 8/9/0xA) stays unsupported.
            0x1 => match r(w) {
                0x0 => Some(Inst::BranchBool(false, breg_s(w), imm8(w) as i8 as i32)),
                0x1 => Some(Inst::BranchBool(true, breg_s(w), imm8(w) as i8 as i32)),
                _ => None,
            },
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

/// `op0 = 7`: RRI8 register-register / bit-test branches, disambiguated by `r`.
fn decode_op0_7(w: u32) -> Option<Inst> {
    let off = imm8(w) as i8 as i32;
    let br = |op| Some(Inst::BranchRr(op, reg_s(w), reg_t(w), off));
    match r(w) {
        0x0 => br(BrRr::Bnone),
        0x1 => br(BrRr::Beq),
        0x2 => br(BrRr::Blt),
        0x3 => br(BrRr::Bltu),
        0x4 => br(BrRr::Ball),
        0x5 => br(BrRr::Bbc),
        0x8 => br(BrRr::Bany),
        0x9 => br(BrRr::Bne),
        0xa => br(BrRr::Bge),
        0xb => br(BrRr::Bgeu),
        0xc => br(BrRr::Bnall),
        0xd => br(BrRr::Bbs),
        0x6 | 0x7 => {
            // BBCI: r{3-1} = 3, imm = t | (r{0} << 4)
            let imm = t(w) | ((r(w) & 0x1) << 4);
            Some(Inst::BranchBiI(false, reg_s(w), imm, off))
        }
        0xe | 0xf => {
            // BBSI: r{3-1} = 7, imm = t | (r{0} << 4)
            let imm = t(w) | ((r(w) & 0x1) << 4);
            Some(Inst::BranchBiI(true, reg_s(w), imm, off))
        }
        _ => None,
    }
}

/// Decode a 16-bit (narrow / density) instruction word.
fn decode16(w: u32) -> Option<Inst> {
    match op0(w) {
        0x8 => {
            // L32I.N: offset = r_field * 4
            Some(Inst::L32iN(reg_t(w), reg_s(w), (r(w) as u32) * 4))
        }
        0x9 => Some(Inst::S32iN(reg_t(w), reg_s(w), (r(w) as u32) * 4)),
        0xa => Some(Inst::AddN(reg_r(w), reg_s(w), reg_t(w))),
        0xb => {
            // ADDI.N: imm = t, with t==0 meaning -1
            let imm = if t(w) == 0 { -1 } else { t(w) as i32 };
            Some(Inst::AddiN(reg_r(w), reg_s(w), imm))
        }
        0xc => {
            // op0 = C: bit 7 (i) distinguishes MOVI.N (0) from Bxxx.N (1)
            if (w >> 7) & 0x1 == 0 {
                // MOVI.N: 7-bit field = imm7{3-0}=r, imm7{6-4}=bits 6..4
                let field = (r(w) as u32) | (((w >> 4) & 0x7) << 4);
                let imm = if field < 96 {
                    field as i32
                } else {
                    field as i32 - 128
                };
                Some(Inst::MoviN(reg_s(w), imm))
            } else {
                // BEQZ.N / BNEZ.N: imm6{3-0}=r, imm6{5-4}=bits 5..4; z=bit6
                let imm6 = (r(w) as u32) | (((w >> 4) & 0x3) << 4);
                let nez = (w >> 6) & 0x1 == 1;
                Some(Inst::BranchZN(nez, reg_s(w), imm6))
            }
        }
        0xd => {
            // op0 = D: MOV.N / RET.N / RETW.N / NOP.N / ILL.N by r,t
            match r(w) {
                0x0 => Some(Inst::MovN(reg_t(w), reg_s(w))),
                0xf => match t(w) {
                    0x0 if s(w) == 0 => Some(Inst::NullaryN(NullaryNarrowOp::RetN)),
                    0x1 if s(w) == 0 => Some(Inst::NullaryN(NullaryNarrowOp::RetwN)),
                    0x3 if s(w) == 0 => Some(Inst::NullaryN(NullaryNarrowOp::NopN)),
                    0x6 if s(w) == 0 => Some(Inst::NullaryN(NullaryNarrowOp::IllN)),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}
