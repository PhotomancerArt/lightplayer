//! RV32F single-precision floating-point execution.
//!
//! Implemented from *The RISC-V Instruction Set Manual, Volume I: Unprivileged
//! Architecture*, version **20240411**, Chapter 21 (`"F" Standard Extension for
//! Single-Precision Floating-Point, Version 2.2`), cross-read with IEEE 754-2008
//! where the RISC-V text defers to it. Citations name the section title as well
//! as its number so they survive a renumbering. No GPL implementation (QEMU,
//! GDB, GCC, glibc soft-float) was read or transliterated; see
//! `docs/adr/2026-07-29-license-provenance-discipline.md`.
//!
//! ## Why this is a soft-float implementation
//!
//! Every arithmetic result below is computed with integer arithmetic on the
//! exact significand, then rounded once. Native host `f32` operations are not
//! used, for three reasons that are each individually disqualifying:
//!
//! 1. **Rounding modes.** RV32F has five (`RNE RTZ RDN RUP RMM`). Rust's `f32`
//!    operators only give the host's current mode, which is always `RNE`.
//! 2. **Exception flags.** `NV DZ OF UF NX` must be reported exactly. Rust
//!    exposes no access to the host FPU status word.
//! 3. **NaN canonicalization.** §21.3 requires the *canonical* NaN
//!    `0x7fc00000` out of every NaN-producing operation; hosts propagate
//!    operand payloads instead.
//!
//! The consequence is that this emulator's flags are **exact, not estimated**:
//! `NX`, `UF`, `OF`, `DZ` and `NV` come out of the same rounding step that
//! produces the result. There is no approximation to disclose.
//!
//! It also means the crate stays `no_std`-clean: `f32::sqrt` and
//! `f32::mul_add` live in `std`, and `lp-riscv-emu` is built without `std` by
//! `fw-emu`.
//!
//! ## Subnormals are *not* flushed
//!
//! §21.4 (`Subnormal Arithmetic`) requires full IEEE 754 subnormal behaviour:
//! subnormal inputs are used at their real value and subnormal results are
//! produced, never flushed to zero. `docs/design/float.md` §4 lists
//! flush-to-zero as *target-defined* — that latitude is about the **shader**
//! tier on wasm/GPU/S3, and explicitly records that "wasm and RV32F preserve
//! denormals (their specs require it)". Do not add an FTZ mode here.
//!
//! ## Fused multiply-add is fused
//!
//! The `FMADD.S` family rounds **once**, at the end (§21.6). The product is
//! kept exact (48 bits of significand) and added to the addend before any
//! rounding happens, which is what makes it differ from a separate `FMUL.S`
//! followed by `FADD.S`. `f32::mul_add` would be the host primitive for this if
//! `std` were available; the integer path here is the same operation.
//!
//! ## Zcf (`C.FLW` / `C.FSW`) is out of scope
//!
//! The compressed float loads and stores are a separate extension (`Zcf`) and
//! are not decoded here or in `compressed.rs`. Nothing in this repo emits them:
//! the shipping target is RV32IMAC, which has no `F` at all, and the F-bearing
//! successor we expect (`RV32IMAFC`, per the ESP32-S31 roadmap note) would need
//! `Zcf` added deliberately as its own change, with its own encodings and
//! tests. A `C.FLW` word reaching the decoder today falls through
//! `compressed.rs` as an unknown compressed instruction, which is the correct
//! answer for a hart that does not implement `Zcf`.

extern crate alloc;

use super::{ExecutionResult, InstClass, LoggingMode, read_reg};
use crate::emu::{
    error::EmulatorError,
    fp_regs::{FFLAG_DZ, FFLAG_NV, FFLAG_NX, FFLAG_OF, FFLAG_UF, FpRegs, RoundingMode},
};
use lp_emu_core::Memory;
use lp_riscv_inst::Gpr;

/// Opcode `LOAD-FP`: `FLW` (with `funct3` = `010`).
pub(super) const OPCODE_LOAD_FP: u8 = 0x07;
/// Opcode `STORE-FP`: `FSW` (with `funct3` = `010`).
pub(super) const OPCODE_STORE_FP: u8 = 0x27;
/// Opcode `MADD`: `FMADD.S`.
pub(super) const OPCODE_MADD: u8 = 0x43;
/// Opcode `MSUB`: `FMSUB.S`.
pub(super) const OPCODE_MSUB: u8 = 0x47;
/// Opcode `NMSUB`: `FNMSUB.S`.
pub(super) const OPCODE_NMSUB: u8 = 0x4b;
/// Opcode `NMADD`: `FNMADD.S`.
pub(super) const OPCODE_NMADD: u8 = 0x4f;
/// Opcode `OP-FP`: every remaining RV32F computational / compare / move.
pub(super) const OPCODE_OP_FP: u8 = 0x53;

/// The canonical quiet NaN, §21.3 `NaN Generation and Propagation`.
///
/// RISC-V does **not** propagate NaN payloads. Any operation that produces a
/// NaN — whether from non-NaN operands (`0/0`, `inf - inf`, `sqrt(-1)`) or by
/// receiving one — writes exactly this pattern. The only exceptions are the
/// pure bit movers (`FSGNJ`/`FMV`/loads/stores), which never interpret their
/// operand at all.
const CANONICAL_NAN: u32 = 0x7fc0_0000;
/// Sign bit of a binary32.
const SIGN_BIT: u32 = 0x8000_0000;
/// Significand (trailing) field of a binary32.
const FRAC_MASK: u32 = 0x007f_ffff;
/// The quiet bit — the significand's most significant bit. Set means quiet.
const QUIET_BIT: u32 = 0x0040_0000;
/// `+inf`.
const POS_INF: u32 = 0x7f80_0000;
/// The largest finite binary32, returned by overflow under the directed modes.
const MAX_FINITE: u32 = 0x7f7f_ffff;

/// Decode and execute an RV32F instruction.
///
/// Dispatched from [`super::decode_execute`] for the five F-extension opcodes.
/// `lp-riscv-inst` has no F support, so the instruction word is decoded here.
pub(super) fn decode_execute_float<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    fp: &mut FpRegs,
) -> Result<ExecutionResult, EmulatorError> {
    match (inst_word & 0x7f) as u8 {
        OPCODE_LOAD_FP => execute_flw::<M>(inst_word, pc, regs, memory, fp),
        OPCODE_STORE_FP => execute_fsw::<M>(inst_word, pc, regs, memory, fp),
        OPCODE_MADD | OPCODE_MSUB | OPCODE_NMSUB | OPCODE_NMADD => {
            execute_fma_family::<M>(inst_word, pc, regs, fp)
        }
        OPCODE_OP_FP => execute_op_fp::<M>(inst_word, pc, regs, fp),
        opcode => Err(invalid(
            pc,
            inst_word,
            regs,
            alloc::format!("Not a floating-point opcode: 0x{opcode:02x}"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Loads and stores (§21.5 `Single-Precision Load and Store Instructions`)
// ---------------------------------------------------------------------------

/// `FLW rd, offset(rs1)` — opcode `LOAD-FP`, `funct3` = `010`, I-type.
///
/// The spec is explicit that `FLW` does not modify the bits it loads: with
/// FLEN = 32 it is a plain 32-bit move from memory into `f[rd]`.
fn execute_flw<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    fp: &mut FpRegs,
) -> Result<ExecutionResult, EmulatorError> {
    let funct3 = ((inst_word >> 12) & 0x7) as u8;
    if funct3 != 0b010 {
        return Err(invalid(
            pc,
            inst_word,
            regs,
            alloc::format!("Unknown LOAD-FP width: funct3=0b{funct3:03b} (only FLW on RV32F)"),
        ));
    }
    let rd = ((inst_word >> 7) & 0x1f) as u8;
    let rs1 = Gpr::new(((inst_word >> 15) & 0x1f) as u8);
    let imm = (inst_word as i32) >> 20;

    let base = read_reg(regs, rs1);
    let address = base.wrapping_add(imm) as u32;
    let error_regs = *regs;
    let value = memory
        .read_word(address)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;
    fp.write_single(rd, value as u32);

    Ok(float_result(InstClass::Load))
}

/// `FSW rs2, offset(rs1)` — opcode `STORE-FP`, `funct3` = `010`, S-type.
fn execute_fsw<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    fp: &mut FpRegs,
) -> Result<ExecutionResult, EmulatorError> {
    let funct3 = ((inst_word >> 12) & 0x7) as u8;
    if funct3 != 0b010 {
        return Err(invalid(
            pc,
            inst_word,
            regs,
            alloc::format!("Unknown STORE-FP width: funct3=0b{funct3:03b} (only FSW on RV32F)"),
        ));
    }
    let rs1 = Gpr::new(((inst_word >> 15) & 0x1f) as u8);
    let rs2 = ((inst_word >> 20) & 0x1f) as u8;
    // S-type immediate: imm[11:5] in bits 31:25, imm[4:0] in bits 11:7.
    let imm = (((inst_word as i32) >> 25) << 5) | (((inst_word >> 7) & 0x1f) as i32);

    let base = read_reg(regs, rs1);
    let address = base.wrapping_add(imm) as u32;
    let error_regs = *regs;
    memory
        .write_word(address, fp.read_single(rs2) as i32)
        .map_err(|e| EmulatorError::from_memory_error(e, pc, error_regs))?;

    Ok(float_result(InstClass::Store))
}

// ---------------------------------------------------------------------------
// Fused multiply-add family (§21.6)
// ---------------------------------------------------------------------------

/// `FMADD.S` / `FMSUB.S` / `FNMSUB.S` / `FNMADD.S` — R4-type.
///
/// §21.6 spells the four out individually, and the two negating forms are
/// **not** `-(a*b ± c)`: `FNMSUB.S` negates the product and *adds* `rs3`, and
/// `FNMADD.S` negates the product and *subtracts* `rs3`. The distinction is
/// observable in the sign of an exactly-zero result, so it is implemented as
/// written rather than by negating the final sum. The spec calls out the
/// naming as differing from other ISAs.
fn execute_fma_family<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    fp: &mut FpRegs,
) -> Result<ExecutionResult, EmulatorError> {
    let opcode = (inst_word & 0x7f) as u8;
    // fmt (bits 26:25) selects the format; 00 = S. RV32F implements no other.
    let fmt = (inst_word >> 25) & 0x3;
    if fmt != 0b00 {
        return Err(invalid(
            pc,
            inst_word,
            regs,
            alloc::format!("Unsupported FP format in fused multiply-add: fmt=0b{fmt:02b}"),
        ));
    }
    let rd = ((inst_word >> 7) & 0x1f) as u8;
    let rs1 = ((inst_word >> 15) & 0x1f) as u8;
    let rs2 = ((inst_word >> 20) & 0x1f) as u8;
    let rs3 = ((inst_word >> 27) & 0x1f) as u8;
    let rm = rounding_mode(inst_word, pc, regs, fp)?;

    let a = fp.read_single(rs1);
    let b = fp.read_single(rs2);
    let c = fp.read_single(rs3);

    let (a, c) = match opcode {
        OPCODE_MADD => (a, c),                     // ( a * b) + c
        OPCODE_MSUB => (a, c ^ SIGN_BIT),          // ( a * b) - c
        OPCODE_NMSUB => (a ^ SIGN_BIT, c),         // (-a * b) + c
        _ => (a ^ SIGN_BIT, c ^ SIGN_BIT),         // (-a * b) - c   (OPCODE_NMADD)
    };

    let (bits, flags) = f32_fma(a, b, c, rm);
    fp.write_single(rd, bits);
    fp.accrue(flags);
    Ok(float_result(InstClass::FloatMulAdd))
}

// ---------------------------------------------------------------------------
// OP-FP (§21.6 computational, §21.7 conversion/move, §21.8 compare, §21.9 class)
// ---------------------------------------------------------------------------

/// Decode and execute an `OP-FP` (opcode `0x53`) instruction.
///
/// `funct7` is `funct5 ‖ fmt`; `fmt` = `00` selects single precision, so every
/// RV32F `funct7` below has its low two bits clear.
fn execute_op_fp<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    fp: &mut FpRegs,
) -> Result<ExecutionResult, EmulatorError> {
    let funct7 = ((inst_word >> 25) & 0x7f) as u8;
    let rs2_field = ((inst_word >> 20) & 0x1f) as u8;
    let rs1_field = ((inst_word >> 15) & 0x1f) as u8;
    let rm_field = ((inst_word >> 12) & 0x7) as u8;
    let rd = ((inst_word >> 7) & 0x1f) as u8;

    match funct7 {
        // --- FADD.S / FSUB.S / FMUL.S / FDIV.S ---
        0b000_0000 | 0b000_0100 | 0b000_1000 | 0b000_1100 => {
            let rm = rounding_mode(inst_word, pc, regs, fp)?;
            let a = fp.read_single(rs1_field);
            let b = fp.read_single(rs2_field);
            let (bits, flags) = match funct7 {
                0b000_0000 => f32_add(a, b, rm),
                0b000_0100 => f32_add(a, b ^ SIGN_BIT, rm),
                0b000_1000 => f32_mul(a, b, rm),
                _ => f32_div(a, b, rm),
            };
            fp.write_single(rd, bits);
            fp.accrue(flags);
            Ok(float_result(InstClass::FloatArith))
        }

        // --- FSQRT.S (rs2 must be 00000) ---
        0b010_1100 => {
            if rs2_field != 0 {
                return Err(invalid(
                    pc,
                    inst_word,
                    regs,
                    alloc::format!("FSQRT.S requires rs2=00000, got 0b{rs2_field:05b}"),
                ));
            }
            let rm = rounding_mode(inst_word, pc, regs, fp)?;
            let (bits, flags) = f32_sqrt(fp.read_single(rs1_field), rm);
            fp.write_single(rd, bits);
            fp.accrue(flags);
            Ok(float_result(InstClass::FloatArith))
        }

        // --- FSGNJ.S / FSGNJN.S / FSGNJX.S ---
        //
        // §21.6, sign injection. Pure bit manipulation on the sign bit: the
        // significand and exponent of rs1 pass through untouched, NaNs are
        // *not* canonicalized, and no exception flag is ever raised. Routing
        // these through f32 arithmetic is the classic way to get them wrong.
        0b001_0000 => {
            let a = fp.read_single(rs1_field);
            let b = fp.read_single(rs2_field);
            let sign = match rm_field {
                0b000 => b & SIGN_BIT,                       // FSGNJ.S
                0b001 => !b & SIGN_BIT,                      // FSGNJN.S
                0b010 => (a ^ b) & SIGN_BIT,                 // FSGNJX.S
                _ => {
                    return Err(invalid(
                        pc,
                        inst_word,
                        regs,
                        alloc::format!("Unknown sign-injection funct3: 0b{rm_field:03b}"),
                    ));
                }
            };
            fp.write_single(rd, (a & !SIGN_BIT) | sign);
            Ok(float_result(InstClass::FloatArith))
        }

        // --- FMIN.S / FMAX.S ---
        0b001_0100 => {
            let a = fp.read_single(rs1_field);
            let b = fp.read_single(rs2_field);
            let is_max = match rm_field {
                0b000 => false,
                0b001 => true,
                _ => {
                    return Err(invalid(
                        pc,
                        inst_word,
                        regs,
                        alloc::format!("Unknown FMIN/FMAX funct3: 0b{rm_field:03b}"),
                    ));
                }
            };
            let (bits, flags) = f32_min_max(a, b, is_max);
            fp.write_single(rd, bits);
            fp.accrue(flags);
            Ok(float_result(InstClass::FloatCompare))
        }

        // --- FCVT.W.S / FCVT.WU.S (float -> integer) ---
        0b110_0000 => {
            let signed = match rs2_field {
                0b00000 => true,  // FCVT.W.S
                0b00001 => false, // FCVT.WU.S
                _ => {
                    return Err(invalid(
                        pc,
                        inst_word,
                        regs,
                        alloc::format!("Unknown FCVT.*.S selector: rs2=0b{rs2_field:05b}"),
                    ));
                }
            };
            let rm = rounding_mode(inst_word, pc, regs, fp)?;
            let (value, flags) = f32_to_i32(fp.read_single(rs1_field), signed, rm);
            write_gpr(regs, rd, value);
            fp.accrue(flags);
            Ok(float_result(InstClass::FloatConvert))
        }

        // --- FMV.X.W / FCLASS.S ---
        0b111_0000 => {
            if rs2_field != 0 {
                return Err(invalid(
                    pc,
                    inst_word,
                    regs,
                    alloc::format!("FMV.X.W/FCLASS.S require rs2=00000, got 0b{rs2_field:05b}"),
                ));
            }
            let a = fp.read_single(rs1_field);
            match rm_field {
                // FMV.X.W (§21.7): moves the raw bits, sign-extending nothing
                // on RV32 because both are 32 bits wide. No interpretation, no
                // canonicalization, no flags.
                0b000 => write_gpr(regs, rd, a as i32),
                // FCLASS.S (§21.9)
                0b001 => write_gpr(regs, rd, f32_classify(a)),
                _ => {
                    return Err(invalid(
                        pc,
                        inst_word,
                        regs,
                        alloc::format!("Unknown FMV.X.W/FCLASS.S funct3: 0b{rm_field:03b}"),
                    ));
                }
            }
            Ok(float_result(InstClass::FloatConvert))
        }

        // --- FEQ.S / FLT.S / FLE.S ---
        0b101_0000 => {
            let a = fp.read_single(rs1_field);
            let b = fp.read_single(rs2_field);
            let (result, flags) = match rm_field {
                0b010 => f32_eq(a, b),
                0b001 => f32_lt(a, b),
                0b000 => f32_le(a, b),
                _ => {
                    return Err(invalid(
                        pc,
                        inst_word,
                        regs,
                        alloc::format!("Unknown FP compare funct3: 0b{rm_field:03b}"),
                    ));
                }
            };
            write_gpr(regs, rd, i32::from(result));
            fp.accrue(flags);
            Ok(float_result(InstClass::FloatCompare))
        }

        // --- FCVT.S.W / FCVT.S.WU (integer -> float) ---
        0b110_1000 => {
            let signed = match rs2_field {
                0b00000 => true,  // FCVT.S.W
                0b00001 => false, // FCVT.S.WU
                _ => {
                    return Err(invalid(
                        pc,
                        inst_word,
                        regs,
                        alloc::format!("Unknown FCVT.S.* selector: rs2=0b{rs2_field:05b}"),
                    ));
                }
            };
            let rm = rounding_mode(inst_word, pc, regs, fp)?;
            let src = read_reg(regs, Gpr::new(rs1_field));
            let (bits, flags) = i32_to_f32(src, signed, rm);
            fp.write_single(rd, bits);
            fp.accrue(flags);
            Ok(float_result(InstClass::FloatConvert))
        }

        // --- FMV.W.X ---
        //
        // §21.7: moves the raw bits of the integer register into `f[rd]`
        // without interpreting them. In particular it does not quiet a
        // signaling NaN pattern.
        0b111_1000 => {
            if rs2_field != 0 || rm_field != 0b000 {
                return Err(invalid(
                    pc,
                    inst_word,
                    regs,
                    alloc::format!(
                        "FMV.W.X requires rs2=00000 and funct3=000, got rs2=0b{rs2_field:05b} funct3=0b{rm_field:03b}"
                    ),
                ));
            }
            let value = read_reg(regs, Gpr::new(rs1_field)) as u32;
            fp.write_single(rd, value);
            Ok(float_result(InstClass::FloatConvert))
        }

        _ => Err(invalid(
            pc,
            inst_word,
            regs,
            alloc::format!("Unknown OP-FP funct7: 0b{funct7:07b}"),
        )),
    }
}

/// Resolve the instruction's `rm` field, raising illegal-instruction for the
/// reserved encodings (§21.2).
fn rounding_mode(
    inst_word: u32,
    pc: u32,
    regs: &[i32; 32],
    fp: &FpRegs,
) -> Result<RoundingMode, EmulatorError> {
    let rm_field = ((inst_word >> 12) & 0x7) as u8;
    RoundingMode::resolve(rm_field, fp.frm()).ok_or_else(|| EmulatorError::InvalidInstruction {
        pc,
        instruction: inst_word,
        reason: alloc::format!(
            "Reserved floating-point rounding mode: rm=0b{rm_field:03b}, frm=0b{:03b}",
            fp.frm()
        ),
        regs: *regs,
    })
}

/// Common `ExecutionResult` for a float instruction.
///
/// `log` is always `None`: [`crate::emu::logging::InstLog`] has no float
/// variant, and `logging.rs` is outside this change. An `InstLog::Float`
/// variant belongs there before anyone relies on `LogLevel` traces covering
/// FP code.
fn float_result(class: InstClass) -> ExecutionResult {
    ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class,
        inst_size: 4,
        log: None,
    }
}

fn invalid(
    pc: u32,
    inst_word: u32,
    regs: &[i32; 32],
    reason: alloc::string::String,
) -> EmulatorError {
    EmulatorError::InvalidInstruction {
        pc,
        instruction: inst_word,
        reason,
        regs: *regs,
    }
}

/// Write an integer register, honouring `x0`'s hardwired zero.
#[inline]
fn write_gpr(regs: &mut [i32; 32], rd: u8, value: i32) {
    if rd != 0 {
        regs[rd as usize] = value;
    }
}

// ===========================================================================
// Soft binary32 core
// ===========================================================================

/// True for any NaN (quiet or signaling).
#[inline]
fn is_nan(bits: u32) -> bool {
    bits & 0x7fff_ffff > POS_INF
}

/// True for a signaling NaN: NaN with the quiet bit clear (§21.3).
#[inline]
fn is_snan(bits: u32) -> bool {
    is_nan(bits) && bits & QUIET_BIT == 0
}

/// True for ±inf.
#[inline]
fn is_inf(bits: u32) -> bool {
    bits & 0x7fff_ffff == POS_INF
}

/// True for ±0.
#[inline]
fn is_zero(bits: u32) -> bool {
    bits & 0x7fff_ffff == 0
}

/// True when the sign bit is set.
#[inline]
fn sign_of(bits: u32) -> bool {
    bits & SIGN_BIT != 0
}

#[inline]
fn sign_bit_of(sign: bool) -> u32 {
    if sign { SIGN_BIT } else { 0 }
}

/// Decompose a **finite** binary32 into `(sign, exp, sig)` such that the value
/// is exactly `(-1)^sign * sig * 2^exp`.
///
/// Both normals and subnormals come out in the same integer-significand form,
/// which is what lets the arithmetic below treat them uniformly — the
/// subnormal-preserving behaviour §21.4 requires falls out of the
/// representation rather than needing a special case.
#[inline]
fn unpack(bits: u32) -> (bool, i32, u128) {
    let sign = sign_of(bits);
    let biased = ((bits >> 23) & 0xff) as i32;
    let frac = u128::from(bits & FRAC_MASK);
    if biased == 0 {
        // Subnormal: 0.frac * 2^-126 == frac * 2^-149.
        (sign, -149, frac)
    } else {
        // Normal: 1.frac * 2^(biased-127) == (2^23 + frac) * 2^(biased-150).
        (sign, biased - 150, frac | (1 << 23))
    }
}

/// Round the exact value `(-1)^sign * (sig + eps) * 2^exp` to binary32,
/// where `eps` is in `[0, 1)` and is nonzero exactly when `extra_sticky`.
///
/// Returns the result bits and the exception flags it raised (`NX`, `UF`,
/// `OF`). This is the single rounding step of every arithmetic instruction —
/// including the fused multiply-add, which is why it takes an arbitrarily wide
/// `sig`.
///
/// Underflow follows the IEEE 754 default that §21.2 adopts: `UF` is raised
/// when the result is tiny **after** rounding *and* inexact. A value that
/// rounds up to the smallest normal is therefore not an underflow.
fn round_pack(
    sign: bool,
    exp: i32,
    sig: u128,
    extra_sticky: bool,
    rm: RoundingMode,
) -> (u32, u8) {
    let sign_bit = sign_bit_of(sign);

    if sig == 0 {
        if !extra_sticky {
            return (sign_bit, 0);
        }
        // A nonzero magnitude smaller than the least significant bit we kept.
        // Rounds to zero, or to the smallest subnormal under a directed mode
        // that rounds away from zero in this direction.
        let away = matches!(
            (rm, sign),
            (RoundingMode::Rdn, true) | (RoundingMode::Rup, false)
        );
        let bits = if away { sign_bit | 1 } else { sign_bit };
        return (bits, FFLAG_UF | FFLAG_NX);
    }

    let msb = 127 - sig.leading_zeros() as i32;
    // Unbiased exponent of the exact value: it lies in [2^e, 2^(e+1)).
    let e = exp + msb;
    // Scale of the result's least significant bit. Normals keep 24 bits;
    // subnormals are pinned to 2^-149, which is what makes gradual underflow
    // come out right.
    let mut q = if e >= -126 { e - 23 } else { -149 };

    let shift = q - exp;
    let (mut m, guard, mut sticky) = if shift <= 0 {
        (sig << (-shift) as u32, false, false)
    } else if shift >= 128 {
        // Everything is below the rounding position; `sig` is nonzero so the
        // whole value is sticky and the guard bit is 0.
        (0u128, false, true)
    } else {
        let s = shift as u32;
        let dropped = sig & ((1u128 << s) - 1);
        let guard = (dropped >> (s - 1)) & 1 == 1;
        let sticky = dropped & ((1u128 << (s - 1)) - 1) != 0;
        (sig >> s, guard, sticky)
    };
    sticky |= extra_sticky;
    let inexact = guard || sticky;

    let round_up = match rm {
        RoundingMode::Rne => guard && (sticky || m & 1 == 1),
        RoundingMode::Rtz => false,
        RoundingMode::Rdn => inexact && sign,
        RoundingMode::Rup => inexact && !sign,
        RoundingMode::Rmm => guard,
    };
    if round_up {
        m += 1;
    }
    // Rounding up out of the binade: 0x1000000 renormalizes exactly.
    if m >> 24 != 0 {
        m >>= 1;
        q += 1;
    }

    let mut flags = if inexact { FFLAG_NX } else { 0 };

    if q + 23 > 127 {
        // Overflow. §21.2 defers to IEEE 754: the result is ±inf under the
        // round-to-nearest modes, and the largest finite magnitude under a
        // directed mode that rounds toward zero in this direction.
        flags |= FFLAG_OF | FFLAG_NX;
        let to_infinity = match rm {
            RoundingMode::Rne | RoundingMode::Rmm => true,
            RoundingMode::Rtz => false,
            RoundingMode::Rdn => sign,
            RoundingMode::Rup => !sign,
        };
        let magnitude = if to_infinity { POS_INF } else { MAX_FINITE };
        return (sign_bit | magnitude, flags);
    }

    if q == -149 && m < (1 << 23) {
        // Subnormal (or zero) result: tiny after rounding.
        if inexact {
            flags |= FFLAG_UF;
        }
        return (sign_bit | m as u32, flags);
    }

    let biased_exp = (q + 150) as u32;
    (
        sign_bit | (biased_exp << 23) | (m as u32 & FRAC_MASK),
        flags,
    )
}

/// Exact sum of two signed integer-significand values.
///
/// Returns `(sign, exp, sig, sticky)` describing
/// `(-1)^sign * (sig + eps) * 2^exp` with `eps` in `[0, 1)`, nonzero exactly
/// when `sticky`. `sig == 0 && !sticky` means the sum is exactly zero, and the
/// caller must choose the zero's sign from the rounding mode.
///
/// The alignment keeps `GUARD` extra bits below the smaller operand's original
/// position. When the exponent difference exceeds that, the bits that fall off
/// are folded into `sticky`; for an effective subtraction the difference is
/// additionally decremented by one so the returned pair still brackets the true
/// value from below (`sig < true <= sig + 1`), which is exactly the form
/// [`round_pack`] consumes.
fn add_significands(
    sa: bool,
    ea: i32,
    ma: u128,
    sb: bool,
    eb: i32,
    mb: u128,
) -> (bool, i32, u128, bool) {
    /// Extra bits kept below the larger operand's least significant bit.
    ///
    /// Must be at least the widest significand we ever align (48 bits, from a
    /// fused multiply-add's exact product) so the shifted-out operand can never
    /// exceed the retained one. 64 also keeps `ma << GUARD` inside `u128`
    /// (48 + 64 = 112 bits).
    const GUARD: u32 = 64;

    // Order so the first operand has the larger exponent.
    let ((sa, ea, ma), (sb, eb, mb)) = if ea >= eb {
        ((sa, ea, ma), (sb, eb, mb))
    } else {
        ((sb, eb, mb), (sa, ea, ma))
    };

    let shift = (ea - eb) as u32;
    let a_scaled = ma << GUARD;
    let (b_scaled, lost) = if shift <= GUARD {
        (mb << (GUARD - shift), false)
    } else {
        let s = shift - GUARD;
        if s >= 128 {
            (0u128, mb != 0)
        } else {
            (mb >> s, mb & ((1u128 << s) - 1) != 0)
        }
    };
    let exp = ea - GUARD as i32;

    if sa == sb {
        // True value is (a_scaled + b_scaled) + eps.
        (sa, exp, a_scaled + b_scaled, lost)
    } else if a_scaled > b_scaled {
        // True value is (a_scaled - b_scaled) - eps. Rewrite as
        // (a_scaled - b_scaled - 1) + (1 - eps) so the fractional part is
        // again in (0, 1). `lost` implies a_scaled >= 2^64 > b_scaled, so the
        // decrement cannot underflow.
        let diff = a_scaled - b_scaled - u128::from(lost);
        (sa, exp, diff, lost)
    } else if a_scaled < b_scaled {
        // `lost` cannot hold here (it implies a_scaled > b_scaled), so exact.
        (sb, exp, b_scaled - a_scaled, false)
    } else {
        (false, exp, 0, false)
    }
}

/// The sign of an exactly-zero sum.
///
/// IEEE 754-2008 §6.3: when operands of opposite sign cancel exactly, the sum
/// is `+0` under every rounding attribute except `roundTowardNegative`, where
/// it is `-0`.
#[inline]
fn cancelled_zero(rm: RoundingMode) -> u32 {
    if rm == RoundingMode::Rdn { SIGN_BIT } else { 0 }
}

/// `FADD.S` (and `FSUB.S`, which pre-negates `rs2`).
fn f32_add(a: u32, b: u32, rm: RoundingMode) -> (u32, u8) {
    if is_nan(a) || is_nan(b) {
        return (CANONICAL_NAN, invalid_if_snan(a, b));
    }
    if is_inf(a) || is_inf(b) {
        if is_inf(a) && is_inf(b) && sign_of(a) != sign_of(b) {
            // inf + (-inf) is invalid (§21.6 / IEEE 754 §7.2).
            return (CANONICAL_NAN, FFLAG_NV);
        }
        return (if is_inf(a) { a } else { b }, 0);
    }
    if is_zero(a) && is_zero(b) {
        let bits = if sign_of(a) == sign_of(b) {
            a
        } else {
            cancelled_zero(rm)
        };
        return (bits, 0);
    }
    if is_zero(a) {
        return (b, 0);
    }
    if is_zero(b) {
        return (a, 0);
    }

    let (sa, ea, ma) = unpack(a);
    let (sb, eb, mb) = unpack(b);
    let (sign, exp, sig, sticky) = add_significands(sa, ea, ma, sb, eb, mb);
    if sig == 0 && !sticky {
        return (cancelled_zero(rm), 0);
    }
    round_pack(sign, exp, sig, sticky, rm)
}

/// `FMUL.S`.
fn f32_mul(a: u32, b: u32, rm: RoundingMode) -> (u32, u8) {
    if is_nan(a) || is_nan(b) {
        return (CANONICAL_NAN, invalid_if_snan(a, b));
    }
    let sign = sign_of(a) ^ sign_of(b);
    if is_inf(a) || is_inf(b) {
        if is_zero(a) || is_zero(b) {
            // 0 * inf is invalid.
            return (CANONICAL_NAN, FFLAG_NV);
        }
        return (sign_bit_of(sign) | POS_INF, 0);
    }
    if is_zero(a) || is_zero(b) {
        return (sign_bit_of(sign), 0);
    }

    let (_, ea, ma) = unpack(a);
    let (_, eb, mb) = unpack(b);
    // Exact: two 24-bit significands make at most a 48-bit product.
    round_pack(sign, ea + eb, ma * mb, false, rm)
}

/// `FDIV.S`.
fn f32_div(a: u32, b: u32, rm: RoundingMode) -> (u32, u8) {
    if is_nan(a) || is_nan(b) {
        return (CANONICAL_NAN, invalid_if_snan(a, b));
    }
    let sign = sign_of(a) ^ sign_of(b);
    if is_inf(a) {
        if is_inf(b) {
            return (CANONICAL_NAN, FFLAG_NV);
        }
        return (sign_bit_of(sign) | POS_INF, 0);
    }
    if is_inf(b) {
        return (sign_bit_of(sign), 0);
    }
    if is_zero(a) {
        if is_zero(b) {
            // 0/0 is invalid, not a divide-by-zero.
            return (CANONICAL_NAN, FFLAG_NV);
        }
        return (sign_bit_of(sign), 0);
    }
    if is_zero(b) {
        // Finite nonzero divided by zero: the DZ flag, §21.2.
        return (sign_bit_of(sign) | POS_INF, FFLAG_DZ);
    }

    let (_, ea, ma) = unpack(a);
    let (_, eb, mb) = unpack(b);
    // 64 extra quotient bits: `ma` is at most 24 bits and `mb` at least 1, so
    // the quotient carries at least 40 significant bits, which is more than
    // the 24 + guard + sticky the rounding step needs. The remainder is the
    // exact sticky.
    let numerator = ma << 64;
    let quotient = numerator / mb;
    let remainder = numerator % mb;
    round_pack(sign, ea - eb - 64, quotient, remainder != 0, rm)
}

/// `FSQRT.S`.
fn f32_sqrt(a: u32, rm: RoundingMode) -> (u32, u8) {
    if is_nan(a) {
        return (CANONICAL_NAN, invalid_if_snan(a, a));
    }
    if is_zero(a) {
        // IEEE 754 §5.4.1: sqrt(-0) is -0, and sqrt(+0) is +0.
        return (a, 0);
    }
    if sign_of(a) {
        // sqrt of any negative nonzero (including -inf) is invalid.
        return (CANONICAL_NAN, FFLAG_NV);
    }
    if is_inf(a) {
        return (a, 0);
    }

    let (_, exp, sig) = unpack(a);
    // Force an even exponent so the square root of the scale is exact.
    let (sig, exp) = if exp & 1 != 0 {
        (sig << 1, exp - 1)
    } else {
        (sig, exp)
    };
    // 80 extra bits give at least 40 significant root bits, by the same
    // argument as FDIV.S. `sig` is at most 25 bits here, so `sig << 80` stays
    // inside u128.
    let (root, exact) = isqrt(sig << 80);
    round_pack(false, exp / 2 - 40, root, !exact, rm)
}

/// `FMADD.S`-family kernel: `(a * b) + c` with a **single** rounding.
///
/// The product's 48-bit significand is never rounded before the addend is
/// folded in — that is the whole content of "fused" (§21.6).
fn f32_fma(a: u32, b: u32, c: u32, rm: RoundingMode) -> (u32, u8) {
    // §21.6: the fused multiply-add instructions must set NV when the
    // multiplicands are inf and zero, **even when the addend is a quiet NaN**.
    // IEEE 754-2008 §7.2 leaves that case implementation-defined; RISC-V pins
    // it, so the check goes ahead of the NaN check.
    let product_invalid =
        (is_inf(a) && is_zero(b)) || (is_zero(a) && is_inf(b));
    if product_invalid {
        return (CANONICAL_NAN, FFLAG_NV);
    }
    if is_nan(a) || is_nan(b) || is_nan(c) {
        let nv = if is_snan(a) || is_snan(b) || is_snan(c) {
            FFLAG_NV
        } else {
            0
        };
        return (CANONICAL_NAN, nv);
    }

    let product_sign = sign_of(a) ^ sign_of(b);
    if is_inf(a) || is_inf(b) {
        // The product is ±inf; neither multiplicand is zero (checked above).
        if is_inf(c) && sign_of(c) != product_sign {
            return (CANONICAL_NAN, FFLAG_NV);
        }
        return (sign_bit_of(product_sign) | POS_INF, 0);
    }
    if is_inf(c) {
        return (c, 0);
    }
    if is_zero(a) || is_zero(b) {
        // The product is a signed zero; reuse the addition's zero rules so the
        // sign of a (+0) + (-0) result follows the rounding mode.
        return f32_add(sign_bit_of(product_sign), c, rm);
    }

    let (_, ea, ma) = unpack(a);
    let (_, eb, mb) = unpack(b);
    let product_exp = ea + eb;
    let product_sig = ma * mb;

    if is_zero(c) {
        // Adding a zero cannot change a nonzero product's value, and the
        // product is nonzero here, so the addend's sign is irrelevant.
        return round_pack(product_sign, product_exp, product_sig, false, rm);
    }

    let (sc, ec, mc) = unpack(c);
    let (sign, exp, sig, sticky) =
        add_significands(product_sign, product_exp, product_sig, sc, ec, mc);
    if sig == 0 && !sticky {
        return (cancelled_zero(rm), 0);
    }
    round_pack(sign, exp, sig, sticky, rm)
}

/// `FMIN.S` / `FMAX.S`, §21.6.
///
/// Three departures from a naive comparison, all spelled out by the spec:
///
/// - If exactly one operand is NaN, the result is the **other** operand.
/// - If both are NaN, the result is the canonical NaN.
/// - A signaling NaN operand sets `NV` **even when the result is not a NaN**.
///
/// And, "for the purposes of these instructions only", `-0.0` is considered
/// less than `+0.0` — so `FMIN.S(-0.0, +0.0)` is `-0.0`, unlike the `FLT.S`
/// comparison, where the two are equal.
fn f32_min_max(a: u32, b: u32, is_max: bool) -> (u32, u8) {
    let flags = if is_snan(a) || is_snan(b) {
        FFLAG_NV
    } else {
        0
    };
    if is_nan(a) && is_nan(b) {
        return (CANONICAL_NAN, flags);
    }
    if is_nan(a) {
        return (b, flags);
    }
    if is_nan(b) {
        return (a, flags);
    }
    let ka = min_max_key(a);
    let kb = min_max_key(b);
    let take_a = if is_max { ka >= kb } else { ka <= kb };
    (if take_a { a } else { b }, flags)
}

/// A monotone integer key over non-NaN binary32 values in which `-0 < +0`.
///
/// Only [`f32_min_max`] uses it; the comparison instructions use
/// [`compare_key`], where `-0 == +0`.
#[inline]
fn min_max_key(bits: u32) -> i64 {
    let magnitude = i64::from(bits & 0x7fff_ffff);
    if sign_of(bits) {
        -magnitude - 1
    } else {
        magnitude
    }
}

/// A monotone integer key over non-NaN binary32 values in which `-0 == +0`.
#[inline]
fn compare_key(bits: u32) -> i64 {
    let magnitude = i64::from(bits & 0x7fff_ffff);
    if sign_of(bits) { -magnitude } else { magnitude }
}

/// `FEQ.S`, §21.8: a **quiet** comparison — only a signaling NaN sets `NV`.
fn f32_eq(a: u32, b: u32) -> (bool, u8) {
    if is_nan(a) || is_nan(b) {
        return (false, invalid_if_snan(a, b));
    }
    (compare_key(a) == compare_key(b), 0)
}

/// `FLT.S`, §21.8: a **signaling** comparison — any NaN operand sets `NV`.
fn f32_lt(a: u32, b: u32) -> (bool, u8) {
    if is_nan(a) || is_nan(b) {
        return (false, FFLAG_NV);
    }
    (compare_key(a) < compare_key(b), 0)
}

/// `FLE.S`, §21.8: a **signaling** comparison — any NaN operand sets `NV`.
fn f32_le(a: u32, b: u32) -> (bool, u8) {
    if is_nan(a) || is_nan(b) {
        return (false, FFLAG_NV);
    }
    (compare_key(a) <= compare_key(b), 0)
}

/// `FCLASS.S`, §21.9 (`Single-Precision Floating-Point Classify Instruction`).
///
/// Exactly one bit of the 10-bit mask is set. Bit order, from the spec's table:
///
/// | Bit | Meaning                     |
/// |----:|-----------------------------|
/// | 0   | `-inf`                      |
/// | 1   | negative normal             |
/// | 2   | negative subnormal          |
/// | 3   | `-0`                        |
/// | 4   | `+0`                        |
/// | 5   | positive subnormal          |
/// | 6   | positive normal             |
/// | 7   | `+inf`                      |
/// | 8   | signaling NaN               |
/// | 9   | quiet NaN                   |
///
/// Sets no exception flags — not even for a signaling NaN, which is the point
/// of having a classify instruction.
fn f32_classify(bits: u32) -> i32 {
    let negative = sign_of(bits);
    let biased_exp = (bits >> 23) & 0xff;
    let frac = bits & FRAC_MASK;

    let bit = if biased_exp == 0xff {
        if frac == 0 {
            if negative { 0 } else { 7 }
        } else if frac & QUIET_BIT == 0 {
            8
        } else {
            9
        }
    } else if biased_exp == 0 {
        if frac == 0 {
            if negative { 3 } else { 4 }
        } else if negative {
            2
        } else {
            5
        }
    } else if negative {
        1
    } else {
        6
    };
    1 << bit
}

/// `FCVT.W.S` (`signed`) / `FCVT.WU.S`, §21.7.
///
/// The spec's rule is *saturating*, and the range check applies to the
/// **rounded** result: "if the rounded result is not representable in the
/// destination format, it is clipped to the nearest value and the invalid flag
/// is set". So `-0.5` with `RTZ` is a valid `FCVT.WU.S` producing `0` with
/// `NX`, while the same value with `RDN` rounds to `-1`, is out of range, and
/// produces `0` with `NV`.
///
/// NaN converts to the **maximum** of the destination type (`2^31 - 1` signed,
/// `2^32 - 1` unsigned) with `NV`, per the spec's table of invalid-input
/// behaviour. This is *not* what a Rust `as` cast does — `as` maps NaN to `0`
/// — nor what C's undefined behaviour permits, which is why the case is
/// handled explicitly rather than delegated to a cast.
///
/// An out-of-range conversion reports `NV` only: IEEE 754 does not also signal
/// inexact when invalid is signalled.
fn f32_to_i32(bits: u32, signed: bool, rm: RoundingMode) -> (i32, u8) {
    let clipped = |negative: bool| -> i32 {
        match (signed, negative) {
            (true, true) => i32::MIN,
            (true, false) => i32::MAX,
            (false, true) => 0,
            (false, false) => u32::MAX as i32,
        }
    };

    if is_nan(bits) {
        // NaN is not negative for this purpose: it saturates to the maximum.
        return (clipped(false), FFLAG_NV);
    }
    if is_inf(bits) {
        return (clipped(sign_of(bits)), FFLAG_NV);
    }
    if is_zero(bits) {
        return (0, 0);
    }

    let (sign, exp, sig) = unpack(bits);
    if exp > 40 {
        // |value| >= 2^40, far outside both destination ranges. The bound also
        // keeps `sig << exp` inside u128 below (24 + 40 bits).
        return (clipped(sign), FFLAG_NV);
    }

    let (magnitude, guard, sticky) = if exp >= 0 {
        (sig << exp as u32, false, false)
    } else {
        let s = (-exp) as u32;
        if s >= 128 {
            (0u128, false, true)
        } else {
            let dropped = sig & ((1u128 << s) - 1);
            let guard = (dropped >> (s - 1)) & 1 == 1;
            let sticky = dropped & ((1u128 << (s - 1)) - 1) != 0;
            (sig >> s, guard, sticky)
        }
    };
    let inexact = guard || sticky;
    let round_up = match rm {
        RoundingMode::Rne => guard && (sticky || magnitude & 1 == 1),
        RoundingMode::Rtz => false,
        RoundingMode::Rdn => inexact && sign,
        RoundingMode::Rup => inexact && !sign,
        RoundingMode::Rmm => guard,
    };
    let magnitude = magnitude + u128::from(round_up);

    let value = if sign {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };

    let (lo, hi) = if signed {
        (i128::from(i32::MIN), i128::from(i32::MAX))
    } else {
        (0, i128::from(u32::MAX))
    };
    if value < lo {
        return (clipped(true), FFLAG_NV);
    }
    if value > hi {
        return (clipped(false), FFLAG_NV);
    }

    let out = if signed {
        value as i32
    } else {
        (value as u32) as i32
    };
    (out, if inexact { FFLAG_NX } else { 0 })
}

/// `FCVT.S.W` (`signed`) / `FCVT.S.WU`, §21.7.
///
/// Always exact for magnitudes below `2^24`; larger values round per `rm` and
/// set `NX`. Cannot overflow: `2^32` is far inside binary32's range.
fn i32_to_f32(value: i32, signed: bool, rm: RoundingMode) -> (u32, u8) {
    let (sign, magnitude) = if signed {
        (value < 0, u128::from(i64::from(value).unsigned_abs()))
    } else {
        (false, u128::from(value as u32))
    };
    if magnitude == 0 {
        return (0, 0);
    }
    round_pack(sign, 0, magnitude, false, rm)
}

/// `NV` if either operand is a signaling NaN, for the quiet operations.
#[inline]
fn invalid_if_snan(a: u32, b: u32) -> u8 {
    if is_snan(a) || is_snan(b) {
        FFLAG_NV
    } else {
        0
    }
}

/// Integer square root of a `u128`: returns `(floor(sqrt(n)), n_is_a_square)`.
///
/// Digit-by-digit (two bits per step) restoring square root — the schoolbook
/// algorithm, written from its definition. `sqrt` is not available in `core`,
/// and this form gives the exact remainder, which is precisely the sticky bit
/// [`round_pack`] needs to round `FSQRT.S` correctly in every mode.
fn isqrt(n: u128) -> (u128, bool) {
    let mut remainder: u128 = 0;
    let mut root: u128 = 0;
    for i in (0..64).rev() {
        root <<= 1;
        remainder = (remainder << 2) | ((n >> (2 * i)) & 0x3);
        if root < remainder {
            remainder -= root + 1;
            root += 2;
        }
    }
    (root >> 1, remainder == 0)
}
