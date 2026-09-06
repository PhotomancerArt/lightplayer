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
        OPCODE_MADD => (a, c),             // ( a * b) + c
        OPCODE_MSUB => (a, c ^ SIGN_BIT),  // ( a * b) - c
        OPCODE_NMSUB => (a ^ SIGN_BIT, c), // (-a * b) + c
        _ => (a ^ SIGN_BIT, c ^ SIGN_BIT), // (-a * b) - c   (OPCODE_NMADD)
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
                0b000 => b & SIGN_BIT,       // FSGNJ.S
                0b001 => !b & SIGN_BIT,      // FSGNJN.S
                0b010 => (a ^ b) & SIGN_BIT, // FSGNJX.S
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
fn round_pack(sign: bool, exp: i32, sig: u128, extra_sticky: bool, rm: RoundingMode) -> (u32, u8) {
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
    let product_invalid = (is_inf(a) && is_zero(b)) || (is_zero(a) && is_inf(b));
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

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;

    use super::*;
    use crate::emu::executor::{LoggingDisabled, decode_execute};
    use crate::emu::fp_regs::FFLAGS_MASK;
    use lp_emu_core::{DEFAULT_RAM_START, Memory};

    const RNE: RoundingMode = RoundingMode::Rne;
    const RTZ: RoundingMode = RoundingMode::Rtz;
    const RDN: RoundingMode = RoundingMode::Rdn;
    const RUP: RoundingMode = RoundingMode::Rup;
    const RMM: RoundingMode = RoundingMode::Rmm;
    const ALL_MODES: [RoundingMode; 5] = [RNE, RTZ, RDN, RUP, RMM];

    const ONE: u32 = 0x3f80_0000;
    const NEG_ONE: u32 = 0xbf80_0000;
    const TWO: u32 = 0x4000_0000;
    const THREE: u32 = 0x4040_0000;
    const NINE: u32 = 0x4110_0000;
    const NEG_ZERO: u32 = 0x8000_0000;
    const NEG_INF: u32 = 0xff80_0000;
    /// A quiet NaN carrying a payload, to prove payloads are not propagated.
    const QNAN_PAYLOAD: u32 = 0x7fc0_1234;
    /// A signaling NaN (quiet bit clear, nonzero significand).
    const SNAN: u32 = 0x7f80_0001;
    /// `2^-23`, one ulp of 1.0.
    const ULP_OF_ONE: u32 = 0x3400_0000;
    /// `2^-24`, exactly half an ulp of 1.0 — the tie case.
    const HALF_ULP_OF_ONE: u32 = 0x3380_0000;
    /// The smallest positive subnormal, `2^-149`.
    const MIN_SUBNORMAL: u32 = 0x0000_0001;
    /// The largest subnormal, `2^-126 - 2^-149`.
    const MAX_SUBNORMAL: u32 = 0x007f_ffff;
    /// The smallest positive normal, `2^-126`.
    const MIN_NORMAL: u32 = 0x0080_0000;

    // -- arithmetic ---------------------------------------------------------

    /// Every `+ - * /` result agrees, bit for bit, with the host FPU's
    /// correctly-rounded binary32. The host is a legitimate oracle for RNE:
    /// IEEE 754 pins these four operations exactly, so any disagreement is a
    /// bug in the soft-float path. NaN results are excluded — that is the one
    /// place RISC-V deliberately differs from the host.
    #[test]
    fn arithmetic_matches_the_host_fpu_bit_for_bit() {
        let values: [f32; 22] = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            2.0,
            3.0,
            0.1,
            0.2,
            0.3,
            1e-5,
            1e5,
            1e20,
            1e-20,
            1e38,
            1e-38,
            f32::MIN_POSITIVE,
            16_777_216.0,
            8_388_609.0,
            123.456,
            -987.654,
            -7.5,
        ];
        let subnormals = [
            f32::from_bits(MIN_SUBNORMAL),
            f32::from_bits(0x0040_0000),
            f32::from_bits(0x0000_ffff),
        ];

        for a in values.iter().chain(subnormals.iter()).copied() {
            for b in values.iter().chain(subnormals.iter()).copied() {
                let (ab, bb) = (a.to_bits(), b.to_bits());
                let cases = [
                    ("add", f32_add(ab, bb, RNE).0, a + b),
                    ("sub", f32_add(ab, bb ^ SIGN_BIT, RNE).0, a - b),
                    ("mul", f32_mul(ab, bb, RNE).0, a * b),
                    ("div", f32_div(ab, bb, RNE).0, a / b),
                ];
                for (name, got, want) in cases {
                    if want.is_nan() {
                        // The host picks its own NaN sign and payload; RISC-V
                        // pins the canonical NaN instead.
                        assert_eq!(
                            got, CANONICAL_NAN,
                            "{name}({a}, {b}) should be the canonical NaN"
                        );
                        continue;
                    }
                    assert_eq!(
                        got,
                        want.to_bits(),
                        "{name}({a}, {b}): got {got:#010x}, host {:#010x}",
                        want.to_bits()
                    );
                }
            }
        }
    }

    /// Exact halves and the `0.1 + 0.2` identity, asserted on bit patterns.
    #[test]
    fn exact_and_representative_sums_have_the_expected_bit_patterns() {
        assert_eq!(f32_add(0x3f00_0000, 0x3f00_0000, RNE), (ONE, 0)); // 0.5 + 0.5
        assert_eq!(f32_add(ONE, ONE, RNE), (TWO, 0));
        assert_eq!(f32_mul(THREE, 0x40a0_0000, RNE), (0x4170_0000, 0)); // 3 * 5
        assert_eq!(f32_add(THREE, 0x40a0_0000, RNE), (0x4100_0000, 0)); // 3 + 5
        // 0.1f32 + 0.2f32 is inexact and must report NX.
        let (bits, flags) = f32_add(0.1f32.to_bits(), 0.2f32.to_bits(), RNE);
        assert_eq!(bits, (0.1f32 + 0.2f32).to_bits());
        assert_eq!(flags, FFLAG_NX);
    }

    #[test]
    fn adding_half_an_ulp_ties_to_even() {
        // 1.0 + 2^-24 is exactly halfway between 1.0 and 1.0 + 2^-23.
        // RNE breaks the tie toward the even significand, which is 1.0.
        assert_eq!(f32_add(ONE, HALF_ULP_OF_ONE, RNE), (ONE, FFLAG_NX));
        // From an odd significand, the same tie goes the other way.
        assert_eq!(
            f32_add(0x3f80_0001, HALF_ULP_OF_ONE, RNE),
            (0x3f80_0002, FFLAG_NX)
        );
        // One whole ulp is exact.
        assert_eq!(f32_add(ONE, ULP_OF_ONE, RNE), (0x3f80_0001, 0));
    }

    #[test]
    fn subnormals_are_preserved_not_flushed() {
        // §21.4: full IEEE subnormal arithmetic. Contrast with the
        // target-defined FTZ latitude in docs/design/float.md §4, which is
        // about the shader tier and not about this emulator.
        assert_eq!(f32_add(MIN_SUBNORMAL, MIN_SUBNORMAL, RNE), (0x0000_0002, 0));
        // Halving the smallest normal lands exactly on a subnormal: exact, so
        // no underflow flag despite being tiny.
        assert_eq!(f32_div(MIN_NORMAL, TWO, RNE), (0x0040_0000, 0));
        // A subnormal operand keeps its full value in a product.
        assert_eq!(f32_mul(MIN_SUBNORMAL, TWO, RNE), (0x0000_0002, 0));
        // And the largest subnormal plus one ulp is exactly the smallest normal.
        assert_eq!(f32_add(MAX_SUBNORMAL, MIN_SUBNORMAL, RNE), (MIN_NORMAL, 0));
    }

    #[test]
    fn gradual_underflow_sets_uf_and_nx() {
        // 2^-150 is exactly halfway between 0 and the smallest subnormal.
        assert_eq!(
            f32_div(MIN_SUBNORMAL, TWO, RNE),
            (0, FFLAG_UF | FFLAG_NX),
            "ties-to-even rounds down to zero"
        );
        assert_eq!(
            f32_div(MIN_SUBNORMAL, TWO, RUP),
            (MIN_SUBNORMAL, FFLAG_UF | FFLAG_NX)
        );
        assert_eq!(
            f32_div(MIN_SUBNORMAL, TWO, RMM),
            (MIN_SUBNORMAL, FFLAG_UF | FFLAG_NX)
        );
        assert_eq!(f32_div(MIN_SUBNORMAL, TWO, RTZ), (0, FFLAG_UF | FFLAG_NX));
        // A negative tiny result under RDN rounds away from zero.
        assert_eq!(
            f32_div(MIN_SUBNORMAL | SIGN_BIT, TWO, RDN),
            (MIN_SUBNORMAL | SIGN_BIT, FFLAG_UF | FFLAG_NX)
        );
    }

    #[test]
    fn tininess_is_detected_after_rounding() {
        // largest_subnormal + 2^-150 is exactly halfway between the largest
        // subnormal and the smallest normal. Ties-to-even reaches the normal,
        // which is *not* tiny — so NX without UF. Truncating stays subnormal,
        // which is tiny and inexact — NX *and* UF. The half-ulp addend is only
        // expressible as an exact fused product.
        let (a, b) = (MIN_SUBNORMAL, 0x3f00_0000); // 2^-149 * 0.5
        assert_eq!(f32_fma(a, b, MAX_SUBNORMAL, RNE), (MIN_NORMAL, FFLAG_NX));
        assert_eq!(
            f32_fma(a, b, MAX_SUBNORMAL, RTZ),
            (MAX_SUBNORMAL, FFLAG_UF | FFLAG_NX)
        );
    }

    #[test]
    fn overflow_saturates_per_rounding_mode() {
        let expected = [
            (RNE, POS_INF),
            (RTZ, MAX_FINITE),
            (RDN, MAX_FINITE),
            (RUP, POS_INF),
            (RMM, POS_INF),
        ];
        for (rm, want) in expected {
            assert_eq!(
                f32_add(MAX_FINITE, MAX_FINITE, rm),
                (want, FFLAG_OF | FFLAG_NX),
                "positive overflow under {rm:?}"
            );
        }
        let expected_negative = [
            (RNE, NEG_INF),
            (RTZ, MAX_FINITE | SIGN_BIT),
            (RDN, NEG_INF),
            (RUP, MAX_FINITE | SIGN_BIT),
            (RMM, NEG_INF),
        ];
        for (rm, want) in expected_negative {
            assert_eq!(
                f32_add(MAX_FINITE | SIGN_BIT, MAX_FINITE | SIGN_BIT, rm),
                (want, FFLAG_OF | FFLAG_NX),
                "negative overflow under {rm:?}"
            );
        }
    }

    #[test]
    fn divide_by_zero_sets_dz_not_nv() {
        assert_eq!(f32_div(ONE, 0, RNE), (POS_INF, FFLAG_DZ));
        assert_eq!(f32_div(NEG_ONE, 0, RNE), (NEG_INF, FFLAG_DZ));
        assert_eq!(f32_div(ONE, NEG_ZERO, RNE), (NEG_INF, FFLAG_DZ));
        assert_eq!(f32_div(NEG_ONE, NEG_ZERO, RNE), (POS_INF, FFLAG_DZ));
        // 0/0 is invalid, not a divide by zero.
        assert_eq!(f32_div(0, 0, RNE), (CANONICAL_NAN, FFLAG_NV));
    }

    // -- rounding modes -----------------------------------------------------

    #[test]
    fn every_rounding_mode_is_distinguished() {
        // A positive inexact quotient: 1/3. Down = 0x3eaaaaaa, up = 0x3eaaaaab.
        let positive = [
            (RNE, 0x3eaa_aaab),
            (RTZ, 0x3eaa_aaaa),
            (RDN, 0x3eaa_aaaa),
            (RUP, 0x3eaa_aaab),
            (RMM, 0x3eaa_aaab),
        ];
        for (rm, want) in positive {
            assert_eq!(
                f32_div(ONE, THREE, rm),
                (want, FFLAG_NX),
                "1/3 under {rm:?}"
            );
        }
        // The same magnitude, negated: RDN and RUP swap, RTZ still truncates.
        let negative = [
            (RNE, 0xbeaa_aaab),
            (RTZ, 0xbeaa_aaaa),
            (RDN, 0xbeaa_aaab),
            (RUP, 0xbeaa_aaaa),
            (RMM, 0xbeaa_aaab),
        ];
        for (rm, want) in negative {
            assert_eq!(
                f32_div(NEG_ONE, THREE, rm),
                (want, FFLAG_NX),
                "-1/3 under {rm:?}"
            );
        }
        // An exact tie separates RNE (to even) from RMM (away from zero),
        // which no non-tie case can.
        let tie = [
            (RNE, ONE),
            (RTZ, ONE),
            (RDN, ONE),
            (RUP, 0x3f80_0001),
            (RMM, 0x3f80_0001),
        ];
        for (rm, want) in tie {
            assert_eq!(
                f32_add(ONE, HALF_ULP_OF_ONE, rm),
                (want, FFLAG_NX),
                "1 + 2^-24 under {rm:?}"
            );
        }
    }

    #[test]
    fn reserved_rounding_mode_encodings_are_rejected() {
        for rm in [0b101u8, 0b110] {
            assert_eq!(RoundingMode::resolve(rm, 0), None);
        }
        // DYN against an invalid frm.
        for frm in [0b101u8, 0b110, 0b111] {
            assert_eq!(RoundingMode::resolve(0b111, frm), None);
        }
    }

    // -- NaN handling -------------------------------------------------------

    #[test]
    fn every_nan_producing_operation_yields_the_canonical_nan() {
        // From non-NaN operands.
        assert_eq!(f32_add(POS_INF, NEG_INF, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(
            f32_add(POS_INF, POS_INF ^ SIGN_BIT, RNE),
            (CANONICAL_NAN, FFLAG_NV)
        );
        assert_eq!(f32_mul(0, POS_INF, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_mul(NEG_ZERO, NEG_INF, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_div(0, 0, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_div(POS_INF, POS_INF, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_sqrt(NEG_ONE, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_sqrt(NEG_INF, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_fma(POS_INF, 0, ONE, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(
            f32_fma(TWO, POS_INF, NEG_INF, RNE),
            (CANONICAL_NAN, FFLAG_NV)
        );
    }

    #[test]
    fn nan_payloads_are_never_propagated() {
        // §21.3: RISC-V returns the canonical NaN; it does not forward the
        // operand's payload the way IEEE 754 recommends.
        for (bits, _) in [
            f32_add(ONE, QNAN_PAYLOAD, RNE),
            f32_add(QNAN_PAYLOAD, ONE, RNE),
            f32_mul(QNAN_PAYLOAD, TWO, RNE),
            f32_div(QNAN_PAYLOAD, TWO, RNE),
            f32_sqrt(QNAN_PAYLOAD, RNE),
            f32_fma(QNAN_PAYLOAD, ONE, ONE, RNE),
        ] {
            assert_eq!(bits, CANONICAL_NAN);
        }
        // A quiet NaN operand alone does not set any flag.
        assert_eq!(f32_add(ONE, QNAN_PAYLOAD, RNE).1, 0);
        // Not even a *negative* quiet NaN keeps its sign.
        assert_eq!(f32_add(ONE, QNAN_PAYLOAD | SIGN_BIT, RNE).0, CANONICAL_NAN);
    }

    #[test]
    fn signaling_nan_operands_set_nv() {
        assert_eq!(f32_add(ONE, SNAN, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_mul(SNAN, ONE, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_div(SNAN, ONE, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_sqrt(SNAN, RNE), (CANONICAL_NAN, FFLAG_NV));
        assert_eq!(f32_fma(ONE, ONE, SNAN, RNE), (CANONICAL_NAN, FFLAG_NV));
        // A negative signaling NaN is still signaling.
        assert_eq!(
            f32_add(ONE, SNAN | SIGN_BIT, RNE),
            (CANONICAL_NAN, FFLAG_NV)
        );
    }

    #[test]
    fn infinities_pass_through_arithmetic() {
        assert_eq!(f32_add(POS_INF, ONE, RNE), (POS_INF, 0));
        assert_eq!(f32_add(NEG_INF, ONE, RNE), (NEG_INF, 0));
        assert_eq!(f32_mul(POS_INF, NEG_ONE, RNE), (NEG_INF, 0));
        assert_eq!(f32_div(ONE, POS_INF, RNE), (0, 0));
        assert_eq!(f32_div(NEG_ONE, POS_INF, RNE), (NEG_ZERO, 0));
        assert_eq!(f32_sqrt(POS_INF, RNE), (POS_INF, 0));
    }

    // -- zeros --------------------------------------------------------------

    #[test]
    fn zero_signs_follow_ieee_and_the_rounding_mode() {
        // IEEE 754-2008 §6.3: exact cancellation is +0 except under RDN.
        assert_eq!(f32_add(ONE, NEG_ONE, RNE), (0, 0));
        assert_eq!(f32_add(ONE, NEG_ONE, RDN), (NEG_ZERO, 0));
        assert_eq!(f32_add(0, NEG_ZERO, RNE), (0, 0));
        assert_eq!(f32_add(0, NEG_ZERO, RDN), (NEG_ZERO, 0));
        assert_eq!(f32_add(NEG_ZERO, NEG_ZERO, RNE), (NEG_ZERO, 0));
        assert_eq!(f32_add(0, 0, RNE), (0, 0));
        // Signed zero survives multiplication.
        assert_eq!(f32_mul(NEG_ONE, 0, RNE), (NEG_ZERO, 0));
        assert_eq!(f32_mul(NEG_ONE, NEG_ZERO, RNE), (0, 0));
        // sqrt(-0) is -0 (IEEE 754 §5.4.1) — the one negative input that is
        // not an invalid operation.
        assert_eq!(f32_sqrt(NEG_ZERO, RNE), (NEG_ZERO, 0));
        assert_eq!(f32_sqrt(0, RNE), (0, 0));
    }

    // -- FSQRT --------------------------------------------------------------

    #[test]
    fn sqrt_is_exact_on_perfect_squares() {
        for (input, want) in [
            (ONE, ONE),
            (0x4080_0000, TWO),         // sqrt(4) = 2
            (NINE, THREE),              // sqrt(9) = 3
            (0x3e80_0000, 0x3f00_0000), // sqrt(0.25) = 0.5
            (0x4b80_0000, 0x4580_0000), // sqrt(2^24) = 2^12
        ] {
            assert_eq!(f32_sqrt(input, RNE), (want, 0), "sqrt({input:#010x})");
        }
    }

    #[test]
    fn sqrt_is_correctly_rounded() {
        for value in [
            2.0f32, 3.0, 5.0, 10.0, 0.1, 0.3, 1e-20, 1e20, 1.5, 123.456, 1e-38,
        ] {
            let (bits, flags) = f32_sqrt(value.to_bits(), RNE);
            assert_eq!(
                bits,
                reference_sqrt(value).to_bits(),
                "sqrt({value}) got {bits:#010x}"
            );
            // None of these are perfect squares, so all are inexact.
            assert_eq!(flags, FFLAG_NX, "sqrt({value}) flags");
        }
        // The classic constant, pinned by literal.
        assert_eq!(f32_sqrt(TWO, RNE).0, 0x3fb5_04f3);
    }

    #[test]
    fn sqrt_honours_the_rounding_mode() {
        let down = f32_sqrt(TWO, RTZ).0;
        let up = f32_sqrt(TWO, RUP).0;
        assert_eq!(up, down + 1, "RTZ and RUP must straddle the exact root");
        assert_eq!(f32_sqrt(TWO, RDN).0, down);
        let nearest = f32_sqrt(TWO, RNE).0;
        assert!(nearest == down || nearest == up);
        // Perfect squares are unaffected by the mode.
        for rm in ALL_MODES {
            assert_eq!(f32_sqrt(NINE, rm), (THREE, 0), "sqrt(9) under {rm:?}");
        }
    }

    // -- fused multiply-add -------------------------------------------------

    #[test]
    fn fma_differs_from_an_unfused_multiply_then_add() {
        // a*b is just below the midpoint between 1.0 and 1.0 + 2^-23, so an
        // unfused multiply rounds it to exactly 1.0 and the subsequent add
        // cancels to zero. Fused, the product keeps its low bits and the sum
        // is a small nonzero number — exactly representable, so no flags.
        let a = 0x3f80_0001; // 1 + 2^-23
        let b = 0x3f7f_ffff; // 1 - 2^-24
        let c = NEG_ONE;

        let unfused_product = f32_mul(a, b, RNE);
        assert_eq!(unfused_product, (ONE, FFLAG_NX));
        assert_eq!(f32_add(unfused_product.0, c, RNE), (0, 0));

        assert_eq!(f32_fma(a, b, c, RNE), (0x337f_fffe, 0));
    }

    #[test]
    fn fma_rounds_exactly_once() {
        // 1*1 + 2^-24 is an exact tie, so the single rounding is observable:
        // ties-to-even keeps 1.0 while round-up reaches the next float.
        assert_eq!(f32_fma(ONE, ONE, HALF_ULP_OF_ONE, RNE), (ONE, FFLAG_NX));
        assert_eq!(
            f32_fma(ONE, ONE, HALF_ULP_OF_ONE, RUP),
            (0x3f80_0001, FFLAG_NX)
        );
        // Exactly representable results agree with a straightforward f64
        // evaluation, which does no rounding at all for these inputs.
        for (a, b, c) in [
            (1.5f32, 2.25f32, 0.75f32),
            (-3.5, 7.25, 100.0),
            (2.0, 4.0, -0.5),
        ] {
            let (bits, flags) = f32_fma(a.to_bits(), b.to_bits(), c.to_bits(), RNE);
            let want = ((a as f64) * (b as f64) + (c as f64)) as f32;
            assert_eq!(bits, want.to_bits(), "fma({a}, {b}, {c})");
            assert_eq!(flags, 0, "fma({a}, {b}, {c}) is exact");
        }
    }

    #[test]
    fn fma_family_sign_conventions() {
        // §21.6 spells the four out separately; FNMADD.S is -(a*b) - c, which
        // is not the same as -(a*b + c) when the result is an exact zero.
        let (a, b, c) = (TWO, THREE, NINE);
        assert_eq!(f32_fma(a, b, c, RNE).0, 0x4170_0000); // FMADD:   6 + 9 = 15
        assert_eq!(f32_fma(a, b, c ^ SIGN_BIT, RNE).0, 0xc040_0000); // FMSUB:  6 - 9 = -3
        assert_eq!(f32_fma(a ^ SIGN_BIT, b, c, RNE).0, 0x4040_0000); // FNMSUB: -6 + 9 = 3
        assert_eq!(f32_fma(a ^ SIGN_BIT, b, c ^ SIGN_BIT, RNE).0, 0xc170_0000); // FNMADD

        // The observable difference: FNMADD.S(1, 1, -1) is -(1*1) - (-1) = +0,
        // while -(1*1 + -1) would be -0.
        assert_eq!(f32_fma(NEG_ONE, ONE, ONE, RNE), (0, 0));
    }

    #[test]
    fn fma_signals_invalid_for_inf_times_zero_even_with_a_quiet_nan_addend() {
        // §21.6 pins the case IEEE 754-2008 §7.2 leaves implementation-defined.
        assert_eq!(
            f32_fma(POS_INF, 0, QNAN_PAYLOAD, RNE),
            (CANONICAL_NAN, FFLAG_NV)
        );
        assert_eq!(
            f32_fma(NEG_ZERO, NEG_INF, QNAN_PAYLOAD, RNE),
            (CANONICAL_NAN, FFLAG_NV)
        );
    }

    #[test]
    fn fma_handles_zero_and_infinite_operands() {
        // Zero product plus a value is the value.
        assert_eq!(f32_fma(0, TWO, THREE, RNE), (THREE, 0));
        // (+0 * +1) + -0 cancels; the sign follows the rounding mode.
        assert_eq!(f32_fma(0, ONE, NEG_ZERO, RNE), (0, 0));
        assert_eq!(f32_fma(0, ONE, NEG_ZERO, RDN), (NEG_ZERO, 0));
        // Infinite product with a finite addend.
        assert_eq!(f32_fma(POS_INF, TWO, ONE, RNE), (POS_INF, 0));
        // Finite product with an infinite addend.
        assert_eq!(f32_fma(TWO, THREE, NEG_INF, RNE), (NEG_INF, 0));
        // A nonzero product plus a zero addend is the rounded product.
        assert_eq!(f32_fma(THREE, THREE, NEG_ZERO, RNE), (NINE, 0));
    }

    // -- FMIN / FMAX --------------------------------------------------------

    #[test]
    fn min_max_return_the_non_nan_operand() {
        assert_eq!(f32_min_max(ONE, QNAN_PAYLOAD, false), (ONE, 0));
        assert_eq!(f32_min_max(QNAN_PAYLOAD, ONE, false), (ONE, 0));
        assert_eq!(f32_min_max(ONE, QNAN_PAYLOAD, true), (ONE, 0));
        assert_eq!(f32_min_max(QNAN_PAYLOAD, ONE, true), (ONE, 0));
    }

    #[test]
    fn min_max_of_two_nans_is_the_canonical_nan() {
        assert_eq!(
            f32_min_max(QNAN_PAYLOAD, 0x7fc0_5678, false),
            (CANONICAL_NAN, 0)
        );
        assert_eq!(
            f32_min_max(QNAN_PAYLOAD, 0x7fc0_5678, true),
            (CANONICAL_NAN, 0)
        );
    }

    #[test]
    fn min_max_signaling_nan_sets_nv_even_with_a_numeric_result() {
        assert_eq!(f32_min_max(SNAN, ONE, false), (ONE, FFLAG_NV));
        assert_eq!(f32_min_max(ONE, SNAN, true), (ONE, FFLAG_NV));
        assert_eq!(f32_min_max(SNAN, SNAN, false), (CANONICAL_NAN, FFLAG_NV));
    }

    #[test]
    fn min_max_treat_negative_zero_as_less_than_positive_zero() {
        // "For the purposes of these instructions only" (§21.6) — FLT.S still
        // says the two zeros are equal, which the compare tests check.
        assert_eq!(f32_min_max(NEG_ZERO, 0, false), (NEG_ZERO, 0));
        assert_eq!(f32_min_max(0, NEG_ZERO, false), (NEG_ZERO, 0));
        assert_eq!(f32_min_max(NEG_ZERO, 0, true), (0, 0));
        assert_eq!(f32_min_max(0, NEG_ZERO, true), (0, 0));
    }

    #[test]
    fn min_max_order_ordinary_values() {
        assert_eq!(f32_min_max(ONE, TWO, false), (ONE, 0));
        assert_eq!(f32_min_max(ONE, TWO, true), (TWO, 0));
        assert_eq!(f32_min_max(NEG_ONE, ONE, false), (NEG_ONE, 0));
        assert_eq!(f32_min_max(NEG_INF, POS_INF, false), (NEG_INF, 0));
        assert_eq!(f32_min_max(NEG_INF, POS_INF, true), (POS_INF, 0));
        assert_eq!(
            f32_min_max(MIN_SUBNORMAL, MIN_NORMAL, false),
            (MIN_SUBNORMAL, 0)
        );
    }

    // -- comparisons --------------------------------------------------------

    #[test]
    fn feq_is_a_quiet_comparison() {
        // §21.8: only a signaling NaN sets NV.
        assert_eq!(f32_eq(QNAN_PAYLOAD, ONE), (false, 0));
        assert_eq!(f32_eq(ONE, QNAN_PAYLOAD), (false, 0));
        assert_eq!(f32_eq(QNAN_PAYLOAD, QNAN_PAYLOAD), (false, 0));
        assert_eq!(f32_eq(SNAN, ONE), (false, FFLAG_NV));
        assert_eq!(f32_eq(ONE, SNAN), (false, FFLAG_NV));
    }

    #[test]
    fn flt_and_fle_are_signaling_comparisons() {
        // §21.8: any NaN operand sets NV.
        assert_eq!(f32_lt(QNAN_PAYLOAD, ONE), (false, FFLAG_NV));
        assert_eq!(f32_le(QNAN_PAYLOAD, ONE), (false, FFLAG_NV));
        assert_eq!(f32_lt(ONE, QNAN_PAYLOAD), (false, FFLAG_NV));
        assert_eq!(f32_le(QNAN_PAYLOAD, QNAN_PAYLOAD), (false, FFLAG_NV));
        assert_eq!(f32_lt(SNAN, ONE), (false, FFLAG_NV));
    }

    #[test]
    fn comparisons_treat_the_two_zeros_as_equal() {
        assert_eq!(f32_eq(NEG_ZERO, 0), (true, 0));
        assert_eq!(f32_lt(NEG_ZERO, 0), (false, 0));
        assert_eq!(f32_le(NEG_ZERO, 0), (true, 0));
        assert_eq!(f32_le(0, NEG_ZERO), (true, 0));
    }

    #[test]
    fn comparisons_order_ordinary_values() {
        assert_eq!(f32_lt(ONE, TWO), (true, 0));
        assert_eq!(f32_lt(TWO, ONE), (false, 0));
        assert_eq!(f32_le(ONE, ONE), (true, 0));
        assert_eq!(f32_eq(ONE, ONE), (true, 0));
        assert_eq!(f32_lt(NEG_ONE, ONE), (true, 0));
        assert_eq!(f32_lt(NEG_INF, NEG_ONE), (true, 0));
        assert_eq!(f32_lt(ONE, POS_INF), (true, 0));
    }

    // -- sign injection -----------------------------------------------------

    #[test]
    fn sign_injection_is_bit_exact_and_never_canonicalizes() {
        // Decoded through the executor, because the sign-injection logic lives
        // in the OP-FP arm rather than in a helper.
        let cases = [
            // (funct3, rs1, rs2, expected)
            (0b000u8, QNAN_PAYLOAD, NEG_ZERO, QNAN_PAYLOAD | SIGN_BIT),
            (0b000, QNAN_PAYLOAD | SIGN_BIT, 0, QNAN_PAYLOAD),
            (0b001, QNAN_PAYLOAD, 0, QNAN_PAYLOAD | SIGN_BIT),
            (0b001, QNAN_PAYLOAD, NEG_ZERO, QNAN_PAYLOAD),
            (0b010, NEG_ONE, NEG_ONE, ONE),
            (0b010, NEG_ONE, ONE, NEG_ONE),
            // A signaling NaN stays signaling: no quieting, no NV.
            (0b000, SNAN | SIGN_BIT, 0, SNAN),
        ];
        for (funct3, a, b, want) in cases {
            let mut h = Harness::new();
            h.fp.write_single(1, a);
            h.fp.write_single(2, b);
            h.run(enc_op_fp(0b001_0000, 2, 1, funct3, 3));
            assert_eq!(
                h.fp.read_single(3),
                want,
                "sign injection funct3={funct3:03b}"
            );
            assert_eq!(h.fp.fflags(), 0, "sign injection must raise no flags");
        }
    }

    // -- FCLASS -------------------------------------------------------------

    #[test]
    fn fclass_covers_all_ten_classes() {
        // §21.9, in the spec's table order.
        let cases = [
            (NEG_INF, 1 << 0),
            (NEG_ONE, 1 << 1),
            (MIN_SUBNORMAL | SIGN_BIT, 1 << 2),
            (NEG_ZERO, 1 << 3),
            (0, 1 << 4),
            (MIN_SUBNORMAL, 1 << 5),
            (ONE, 1 << 6),
            (POS_INF, 1 << 7),
            (SNAN, 1 << 8),
            (QNAN_PAYLOAD, 1 << 9),
        ];
        for (bits, want) in cases {
            assert_eq!(f32_classify(bits), want, "FCLASS.S({bits:#010x})");
            assert_eq!(f32_classify(bits).count_ones(), 1);
        }
        // A NaN classifies by quiet/signaling, not by sign.
        assert_eq!(f32_classify(SNAN | SIGN_BIT), 1 << 8);
        assert_eq!(f32_classify(QNAN_PAYLOAD | SIGN_BIT), 1 << 9);
        // The boundaries between the subnormal and normal classes.
        assert_eq!(f32_classify(MAX_SUBNORMAL), 1 << 5);
        assert_eq!(f32_classify(MIN_NORMAL), 1 << 6);
        assert_eq!(f32_classify(MAX_FINITE), 1 << 6);
    }

    // -- conversions --------------------------------------------------------

    #[test]
    fn fcvt_w_s_saturates_at_both_ends_and_for_nan() {
        // §21.7's table of invalid-input behaviour. NaN goes to the *maximum*,
        // which is neither what a Rust `as` cast does (0) nor what C promises.
        assert_eq!(f32_to_i32(QNAN_PAYLOAD, true, RNE), (i32::MAX, FFLAG_NV));
        assert_eq!(f32_to_i32(SNAN, true, RNE), (i32::MAX, FFLAG_NV));
        assert_eq!(f32_to_i32(POS_INF, true, RNE), (i32::MAX, FFLAG_NV));
        assert_eq!(f32_to_i32(NEG_INF, true, RNE), (i32::MIN, FFLAG_NV));
        // 2^31 is one past the top.
        assert_eq!(f32_to_i32(0x4f00_0000, true, RNE), (i32::MAX, FFLAG_NV));
        // -2^31 is exactly representable and therefore valid.
        assert_eq!(f32_to_i32(0xcf00_0000, true, RNE), (i32::MIN, 0));
        // One float below -2^31 is out of range.
        assert_eq!(f32_to_i32(0xcf00_0001, true, RNE), (i32::MIN, FFLAG_NV));
        // The largest in-range float, 2^31 - 128.
        assert_eq!(f32_to_i32(0x4eff_ffff, true, RNE), (2_147_483_520, 0));
    }

    #[test]
    fn fcvt_wu_s_saturates_at_both_ends_and_for_nan() {
        let umax = u32::MAX as i32;
        assert_eq!(f32_to_i32(QNAN_PAYLOAD, false, RNE), (umax, FFLAG_NV));
        assert_eq!(f32_to_i32(POS_INF, false, RNE), (umax, FFLAG_NV));
        assert_eq!(f32_to_i32(NEG_INF, false, RNE), (0, FFLAG_NV));
        assert_eq!(f32_to_i32(NEG_ONE, false, RNE), (0, FFLAG_NV));
        // 2^32 is one past the top; 2^32 - 256 is the largest in-range float.
        assert_eq!(f32_to_i32(0x4f80_0000, false, RNE), (umax, FFLAG_NV));
        assert_eq!(
            f32_to_i32(0x4f7f_ffff, false, RNE),
            (4_294_967_040u32 as i32, 0)
        );
        // -0.0 converts to 0 without complaint.
        assert_eq!(f32_to_i32(NEG_ZERO, false, RNE), (0, 0));
    }

    #[test]
    fn fcvt_range_check_applies_to_the_rounded_result() {
        // -0.5 truncates to 0, which is representable: valid, inexact.
        assert_eq!(f32_to_i32(0xbf00_0000, false, RTZ), (0, FFLAG_NX));
        // The same input rounded down is -1, which is not: invalid, and NV
        // suppresses NX.
        assert_eq!(f32_to_i32(0xbf00_0000, false, RDN), (0, FFLAG_NV));
        // RNE takes -0.5 to zero (ties to even), also in range.
        assert_eq!(f32_to_i32(0xbf00_0000, false, RNE), (0, FFLAG_NX));
    }

    #[test]
    fn fcvt_to_integer_honours_every_rounding_mode() {
        let one_and_a_half = 0x3fc0_0000u32;
        let two_and_a_half = 0x4020_0000u32;
        let minus_one_and_a_half = one_and_a_half | SIGN_BIT;
        let expected = [
            (RNE, 2, 2, -2),
            (RTZ, 1, 2, -1),
            (RDN, 1, 2, -2),
            (RUP, 2, 3, -1),
            (RMM, 2, 3, -2),
        ];
        for (rm, want_1_5, want_2_5, want_neg_1_5) in expected {
            assert_eq!(
                f32_to_i32(one_and_a_half, true, rm),
                (want_1_5, FFLAG_NX),
                "1.5 under {rm:?}"
            );
            assert_eq!(
                f32_to_i32(two_and_a_half, true, rm),
                (want_2_5, FFLAG_NX),
                "2.5 under {rm:?}"
            );
            assert_eq!(
                f32_to_i32(minus_one_and_a_half, true, rm),
                (want_neg_1_5, FFLAG_NX),
                "-1.5 under {rm:?}"
            );
        }
        // Exact integers set no flag.
        assert_eq!(f32_to_i32(THREE, true, RTZ), (3, 0));
        assert_eq!(f32_to_i32(0, true, RNE), (0, 0));
    }

    #[test]
    fn fcvt_from_integer_is_exact_below_two_to_the_24() {
        assert_eq!(i32_to_f32(0, true, RNE), (0, 0));
        assert_eq!(i32_to_f32(1, true, RNE), (ONE, 0));
        assert_eq!(i32_to_f32(-1, true, RNE), (NEG_ONE, 0));
        assert_eq!(i32_to_f32(123, true, RNE), (0x42f6_0000, 0));
        assert_eq!(i32_to_f32(16_777_216, true, RNE), (0x4b80_0000, 0));
        assert_eq!(i32_to_f32(i32::MIN, true, RNE), (0xcf00_0000, 0));
    }

    #[test]
    fn fcvt_from_integer_rounds_above_two_to_the_24() {
        // 2^24 + 1 is a tie between 2^24 and 2^24 + 2; RNE picks the even one.
        assert_eq!(i32_to_f32(16_777_217, true, RNE), (0x4b80_0000, FFLAG_NX));
        assert_eq!(i32_to_f32(16_777_217, true, RUP), (0x4b80_0001, FFLAG_NX));
        assert_eq!(i32_to_f32(16_777_217, true, RTZ), (0x4b80_0000, FFLAG_NX));
        // i32::MAX rounds up to 2^31 under RNE, down under RTZ.
        assert_eq!(i32_to_f32(i32::MAX, true, RNE), (0x4f00_0000, FFLAG_NX));
        assert_eq!(i32_to_f32(i32::MAX, true, RTZ), (0x4eff_ffff, FFLAG_NX));
        // Unsigned reads the same register as a u32.
        assert_eq!(i32_to_f32(-1, false, RNE), (0x4f80_0000, FFLAG_NX));
        assert_eq!(i32_to_f32(-1, false, RTZ), (0x4f7f_ffff, FFLAG_NX));
    }

    #[test]
    fn integer_and_float_conversions_round_trip() {
        for value in [0i32, 1, -1, 42, -42, 65_536, -65_536, 8_388_607, -8_388_607] {
            let (bits, _) = i32_to_f32(value, true, RNE);
            assert_eq!(
                f32_to_i32(bits, true, RTZ),
                (value, 0),
                "round trip {value}"
            );
        }
    }

    // -- decode / dispatch --------------------------------------------------

    #[test]
    fn flw_and_fsw_round_trip_a_raw_bit_pattern_through_memory() {
        // A signaling NaN with a payload: loads and stores must not touch it.
        let pattern = 0xff80_1234u32;
        let mut h = Harness::new();
        h.regs[1] = DEFAULT_RAM_START as i32;
        h.fp.write_single(5, pattern);

        h.run(enc_s(OPCODE_STORE_FP, 0b010, 1, 5, 8)); // FSW f5, 8(x1)
        assert_eq!(
            h.memory.read_word(DEFAULT_RAM_START + 8).unwrap() as u32,
            pattern
        );

        h.run(enc_i(OPCODE_LOAD_FP, 6, 0b010, 1, 8)); // FLW f6, 8(x1)
        assert_eq!(h.fp.read_single(6), pattern);
        assert_eq!(h.fp.fflags(), 0);
    }

    #[test]
    fn flw_and_fsw_accept_negative_offsets() {
        let mut h = Harness::new();
        h.regs[1] = (DEFAULT_RAM_START + 64) as i32;
        h.fp.write_single(5, ONE);
        h.run(enc_s(OPCODE_STORE_FP, 0b010, 1, 5, -16));
        h.run(enc_i(OPCODE_LOAD_FP, 6, 0b010, 1, -16));
        assert_eq!(h.fp.read_single(6), ONE);
    }

    #[test]
    fn non_word_float_load_and_store_widths_are_illegal() {
        // funct3 = 011 would be FLD/FSD, which RV32F does not implement.
        let mut h = Harness::new();
        assert!(h.try_run(enc_i(OPCODE_LOAD_FP, 1, 0b011, 0, 0)).is_err());
        assert!(h.try_run(enc_s(OPCODE_STORE_FP, 0b011, 0, 1, 0)).is_err());
    }

    #[test]
    fn op_fp_arithmetic_writes_the_destination_and_accrues_flags() {
        let mut h = Harness::new();
        h.fp.write_single(1, ONE);
        h.fp.write_single(2, THREE);
        h.run(enc_op_fp(0b000_1100, 2, 1, 0b000, 3)); // FDIV.S f3, f1, f2
        assert_eq!(h.fp.read_single(3), 0x3eaa_aaab);
        assert_eq!(h.fp.fflags(), FFLAG_NX);
    }

    #[test]
    fn every_op_fp_arithmetic_encoding_dispatches() {
        let cases = [
            (0b000_0000u8, NINE),      // FADD.S 6 + 3 = 9
            (0b000_0100, THREE),       // FSUB.S 6 - 3 = 3
            (0b000_1000, 0x4190_0000), // FMUL.S 6 * 3 = 18
            (0b000_1100, TWO),         // FDIV.S 6 / 3 = 2
        ];
        for (funct7, want) in cases {
            let mut h = Harness::new();
            h.fp.write_single(1, 0x40c0_0000); // 6.0
            h.fp.write_single(2, THREE);
            h.run(enc_op_fp(funct7, 2, 1, 0b000, 3));
            assert_eq!(h.fp.read_single(3), want, "funct7={funct7:07b}");
        }
        // FSQRT.S takes a single source.
        let mut h = Harness::new();
        h.fp.write_single(1, NINE);
        h.run(enc_op_fp(0b010_1100, 0, 1, 0b000, 2));
        assert_eq!(h.fp.read_single(2), THREE);
        // FMIN.S / FMAX.S.
        let mut h = Harness::new();
        h.fp.write_single(1, ONE);
        h.fp.write_single(2, TWO);
        h.run(enc_op_fp(0b001_0100, 2, 1, 0b000, 3));
        h.run(enc_op_fp(0b001_0100, 2, 1, 0b001, 4));
        assert_eq!(h.fp.read_single(3), ONE);
        assert_eq!(h.fp.read_single(4), TWO);
    }

    #[test]
    fn integer_conversion_encodings_dispatch() {
        let mut h = Harness::new();
        h.fp.write_single(1, 0xc0c0_0000); // -6.0
        h.run(enc_op_fp(0b110_0000, 0b00000, 1, 0b001, 2)); // FCVT.W.S  (RTZ)
        assert_eq!(h.regs[2], -6);
        h.run(enc_op_fp(0b110_0000, 0b00001, 1, 0b001, 3)); // FCVT.WU.S (RTZ)
        assert_eq!(h.regs[3], 0);
        assert_eq!(h.fp.fflags(), FFLAG_NV);

        let mut h = Harness::new();
        h.regs[1] = -6;
        h.run(enc_op_fp(0b110_1000, 0b00000, 1, 0b000, 2)); // FCVT.S.W
        assert_eq!(h.fp.read_single(2), 0xc0c0_0000);
        h.run(enc_op_fp(0b110_1000, 0b00001, 1, 0b000, 3)); // FCVT.S.WU
        assert_eq!(h.fp.read_single(3), 0x4f80_0000); // 2^32 - 6, rounded to 2^32
    }

    #[test]
    fn exception_flags_accrue_and_are_never_cleared_by_arithmetic() {
        let mut h = Harness::new();
        h.fp.write_single(1, ONE);
        h.fp.write_single(2, THREE);
        h.fp.write_single(3, 0);
        h.fp.write_single(7, NEG_ONE);

        // An inexact divide sets NX.
        h.run(enc_op_fp(0b000_1100, 2, 1, 0b000, 4));
        assert_eq!(h.fp.fflags(), FFLAG_NX);
        // An exact add must not clear it.
        h.run(enc_op_fp(0b000_0000, 1, 1, 0b000, 5)); // FADD.S f5, f1, f1
        assert_eq!(h.fp.fflags(), FFLAG_NX);
        // A divide by zero adds DZ on top.
        h.run(enc_op_fp(0b000_1100, 3, 1, 0b000, 6));
        assert_eq!(h.fp.fflags(), FFLAG_NX | FFLAG_DZ);
        // And an invalid operation adds NV.
        h.run(enc_op_fp(0b010_1100, 0, 7, 0b000, 8)); // FSQRT.S f8, f7  (f7 < 0)
        assert_eq!(h.fp.fflags(), FFLAG_NX | FFLAG_DZ | FFLAG_NV);
        // Nothing outside the five defined bits is ever set.
        assert_eq!(h.fp.fflags() & !FFLAGS_MASK, 0);
    }

    #[test]
    fn dynamic_rounding_mode_comes_from_frm() {
        let mut h = Harness::new();
        h.fp.write_single(1, ONE);
        h.fp.write_single(2, THREE);
        h.fp.set_frm(0b001); // RTZ
        h.run(enc_op_fp(0b000_1100, 2, 1, 0b111, 3)); // FDIV.S with rm = DYN
        assert_eq!(h.fp.read_single(3), 0x3eaa_aaaa);
    }

    #[test]
    fn reserved_rounding_modes_raise_illegal_instruction() {
        for rm in [0b101u8, 0b110] {
            let mut h = Harness::new();
            let err = h.try_run(enc_op_fp(0b000_0000, 2, 1, rm, 3)).unwrap_err();
            assert!(matches!(err, EmulatorError::InvalidInstruction { .. }));
        }
        // DYN with frm holding an invalid value is equally illegal.
        let mut h = Harness::new();
        h.fp.set_frm(0b111);
        let err = h
            .try_run(enc_op_fp(0b000_0000, 2, 1, 0b111, 3))
            .unwrap_err();
        assert!(matches!(err, EmulatorError::InvalidInstruction { .. }));
        // The fused multiply-add family checks the same field.
        let mut h = Harness::new();
        assert!(h.try_run(enc_r4(OPCODE_MADD, 3, 2, 1, 0b101, 4)).is_err());
    }

    #[test]
    fn fmv_moves_raw_bits_in_both_directions() {
        let pattern = 0xff80_1234u32; // a negative signaling NaN
        let mut h = Harness::new();
        h.regs[1] = pattern as i32;
        h.run(enc_op_fp(0b111_1000, 0, 1, 0b000, 4)); // FMV.W.X f4, x1
        assert_eq!(h.fp.read_single(4), pattern);
        h.run(enc_op_fp(0b111_0000, 0, 4, 0b000, 2)); // FMV.X.W x2, f4
        assert_eq!(h.regs[2] as u32, pattern);
        assert_eq!(h.fp.fflags(), 0, "moves raise no flags");
    }

    #[test]
    fn fclass_and_compares_write_the_integer_register() {
        let mut h = Harness::new();
        h.fp.write_single(1, NEG_INF);
        h.fp.write_single(2, ONE);
        h.run(enc_op_fp(0b111_0000, 0, 1, 0b001, 3)); // FCLASS.S x3, f1
        assert_eq!(h.regs[3], 1);
        h.run(enc_op_fp(0b101_0000, 2, 1, 0b001, 4)); // FLT.S x4, f1, f2
        assert_eq!(h.regs[4], 1);
        h.run(enc_op_fp(0b101_0000, 1, 2, 0b010, 5)); // FEQ.S x5, f2, f1
        assert_eq!(h.regs[5], 0);
        h.run(enc_op_fp(0b101_0000, 2, 2, 0b000, 6)); // FLE.S x6, f2, f2
        assert_eq!(h.regs[6], 1);
    }

    #[test]
    fn integer_destination_x0_is_never_written() {
        let mut h = Harness::new();
        h.fp.write_single(1, ONE);
        h.run(enc_op_fp(0b111_0000, 0, 1, 0b000, 0)); // FMV.X.W x0, f1
        assert_eq!(h.regs[0], 0);
        h.run(enc_op_fp(0b101_0000, 1, 1, 0b010, 0)); // FEQ.S x0, f1, f1
        assert_eq!(h.regs[0], 0);
    }

    #[test]
    fn f0_is_an_ordinary_writable_register() {
        // Unlike x0, no floating-point register is hardwired to zero (§21.1).
        let mut h = Harness::new();
        h.fp.write_single(1, ONE);
        h.run(enc_op_fp(0b000_0000, 1, 1, 0b000, 0)); // FADD.S f0, f1, f1
        assert_eq!(h.fp.read_single(0), TWO);
    }

    #[test]
    fn fused_multiply_add_opcodes_decode_to_the_right_signs() {
        let cases = [
            (OPCODE_MADD, 0x4170_0000u32), //  6 + 9 = 15
            (OPCODE_MSUB, 0xc040_0000),    //  6 - 9 = -3
            (OPCODE_NMSUB, 0x4040_0000),   // -6 + 9 = 3
            (OPCODE_NMADD, 0xc170_0000),   // -6 - 9 = -15
        ];
        for (opcode, want) in cases {
            let mut h = Harness::new();
            h.fp.write_single(1, TWO);
            h.fp.write_single(2, THREE);
            h.fp.write_single(3, NINE);
            h.run(enc_r4(opcode, 3, 2, 1, 0b000, 4));
            assert_eq!(h.fp.read_single(4), want, "opcode {opcode:#04x}");
        }
    }

    #[test]
    fn unrecognized_float_encodings_are_illegal_instructions() {
        let mut h = Harness::new();
        // An OP-FP funct7 RV32F does not define.
        assert!(h.try_run(enc_op_fp(0b011_1111, 0, 0, 0b000, 0)).is_err());
        // FSQRT.S with a nonzero rs2.
        assert!(h.try_run(enc_op_fp(0b010_1100, 1, 0, 0b000, 0)).is_err());
        // A compare with an undefined funct3.
        assert!(h.try_run(enc_op_fp(0b101_0000, 0, 0, 0b100, 0)).is_err());
        // A sign-injection variant that does not exist.
        assert!(h.try_run(enc_op_fp(0b001_0000, 0, 0, 0b011, 0)).is_err());
        // FMIN/FMAX with an undefined funct3.
        assert!(h.try_run(enc_op_fp(0b001_0100, 0, 0, 0b010, 0)).is_err());
        // FCVT selectors RV32F does not define (rs2 = 2 is an RV64 form).
        assert!(
            h.try_run(enc_op_fp(0b110_0000, 0b00010, 0, 0b000, 0))
                .is_err()
        );
        assert!(
            h.try_run(enc_op_fp(0b110_1000, 0b00010, 0, 0b000, 0))
                .is_err()
        );
        // Double precision (fmt = 01) in the fused multiply-add family.
        assert!(
            h.try_run(enc_r4(OPCODE_MADD, 0, 0, 0, 0b000, 0) | (0b01 << 25))
                .is_err()
        );
        // FMV.W.X with a nonzero rs2 field.
        assert!(h.try_run(enc_op_fp(0b111_1000, 1, 0, 0b000, 0)).is_err());
        // FMV.X.W / FCLASS.S with an undefined funct3.
        assert!(h.try_run(enc_op_fp(0b111_0000, 0, 0, 0b010, 0)).is_err());
    }

    #[test]
    fn a_float_program_runs_through_the_emulator() {
        use crate::Riscv32Emulator;
        use lp_emu_core::StepResult;

        // FLW f1, 0(x1); FLW f2, 4(x1); FADD.S f3, f1, f2; FSW f3, 8(x1); EBREAK
        let program = [
            enc_i(OPCODE_LOAD_FP, 1, 0b010, 1, 0),
            enc_i(OPCODE_LOAD_FP, 2, 0b010, 1, 4),
            enc_op_fp(0b000_0000, 2, 1, 0b000, 3),
            enc_s(OPCODE_STORE_FP, 0b010, 1, 3, 8),
            lp_riscv_inst::encode::ebreak(),
        ];
        let mut code = alloc::vec::Vec::new();
        for word in program {
            code.extend_from_slice(&word.to_le_bytes());
        }
        let mut ram = vec![0u8; 1024];
        ram[0..4].copy_from_slice(&0.1f32.to_bits().to_le_bytes());
        ram[4..8].copy_from_slice(&0.2f32.to_bits().to_le_bytes());

        let mut emu = Riscv32Emulator::new(code, ram);
        emu.set_register(Gpr::new(1), DEFAULT_RAM_START as i32);
        emu.set_pc(0);
        loop {
            match emu.step().expect("step") {
                StepResult::Continue => {}
                StepResult::Halted => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(
            emu.memory().read_word(DEFAULT_RAM_START + 8).unwrap() as u32,
            (0.1f32 + 0.2f32).to_bits()
        );
        assert_eq!(emu.get_fp_register(3), (0.1f32 + 0.2f32).to_bits());
        assert_eq!(emu.fp_regs().fflags(), FFLAG_NX);
    }

    #[test]
    fn all_rounding_modes_stay_wired_up() {
        // A guard against silently dropping a mode from the tables above.
        assert_eq!(ALL_MODES.len(), 5);
        for rm in ALL_MODES {
            assert_eq!(f32_add(ONE, ONE, rm), (TWO, 0), "exact add under {rm:?}");
            assert_eq!(f32_mul(TWO, THREE, rm), (0x40c0_0000, 0));
        }
    }

    // -- helpers ------------------------------------------------------------

    /// A minimal decode-and-execute rig: integer registers, RAM, and FP state.
    struct Harness {
        regs: [i32; 32],
        memory: Memory,
        fp: FpRegs,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                regs: [0i32; 32],
                memory: Memory::with_default_addresses(vec![], vec![0u8; 1024]),
                fp: FpRegs::new(),
            }
        }

        fn try_run(&mut self, inst_word: u32) -> Result<ExecutionResult, EmulatorError> {
            decode_execute::<LoggingDisabled>(
                inst_word,
                0,
                &mut self.regs,
                &mut self.memory,
                &mut self.fp,
            )
        }

        fn run(&mut self, inst_word: u32) -> ExecutionResult {
            self.try_run(inst_word).expect("instruction should execute")
        }
    }

    fn enc_i(opcode: u8, rd: u8, funct3: u8, rs1: u8, imm: i32) -> u32 {
        (((imm as u32) & 0xfff) << 20)
            | (u32::from(rs1) << 15)
            | (u32::from(funct3) << 12)
            | (u32::from(rd) << 7)
            | u32::from(opcode)
    }

    fn enc_s(opcode: u8, funct3: u8, rs1: u8, rs2: u8, imm: i32) -> u32 {
        let imm = imm as u32;
        (((imm >> 5) & 0x7f) << 25)
            | (u32::from(rs2) << 20)
            | (u32::from(rs1) << 15)
            | (u32::from(funct3) << 12)
            | ((imm & 0x1f) << 7)
            | u32::from(opcode)
    }

    fn enc_op_fp(funct7: u8, rs2: u8, rs1: u8, rm: u8, rd: u8) -> u32 {
        (u32::from(funct7) << 25)
            | (u32::from(rs2) << 20)
            | (u32::from(rs1) << 15)
            | (u32::from(rm) << 12)
            | (u32::from(rd) << 7)
            | u32::from(OPCODE_OP_FP)
    }

    fn enc_r4(opcode: u8, rs3: u8, rs2: u8, rs1: u8, rm: u8, rd: u8) -> u32 {
        (u32::from(rs3) << 27)
            | (u32::from(rs2) << 20)
            | (u32::from(rs1) << 15)
            | (u32::from(rm) << 12)
            | (u32::from(rd) << 7)
            | u32::from(opcode)
    }

    /// An independent square root, for the correctly-rounded check.
    ///
    /// Newton–Raphson in `f64` from a bit-twiddled seed, using only `core`
    /// arithmetic (`f32::sqrt` lives in `std`, which this crate does not
    /// require). Rounding the `f64` root to `f32` is safe: `f64` carries
    /// 53 >= 2*24 + 2 bits, the classical width at which a narrowing double
    /// rounding of a square root cannot differ from rounding the exact result.
    fn reference_sqrt(x: f32) -> f32 {
        let a = x as f64;
        if a <= 0.0 {
            return x;
        }
        let mut g = f64::from_bits((a.to_bits() + 0x3ff0_0000_0000_0000) >> 1);
        for _ in 0..10 {
            g = 0.5 * (g + a / g);
        }
        g as f32
    }
}
