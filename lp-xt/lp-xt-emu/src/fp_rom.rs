//! The measured semantics of the Xtensa FPU's divide/sqrt building blocks:
//! the estimate lookup ROMs and the helper instructions around them.
//!
//! # Provenance — silicon, exhaustively
//!
//! **Everything in this file is measured, not sourced from any document.** The
//! ISA Reference Manual's Table 4-46 does not list these instructions, and the
//! license rules keep binutils/GCC/QEMU source off the table — so the M6 P6
//! campaign read the behavior off the desk ESP32-S3 directly:
//!
//! - Board: ESP32-S3 chip rev v0.2, MAC `d8:3b:da:47:29:70`, 16 MB flash.
//! - ROMs: `tests/fixtures/fp/captures/tables.txt` — 60 run-length-encoded
//!   sweeps of the full 2²³ significand space over 15 `(sign, exponent)`
//!   planes per op, 2026-07-31, firmware commit `4e7a3da28728`. The model
//!   below reproduces **every run of every sweep exactly**
//!   (`tests/fp_silicon_replay.rs`).
//! - Helper semantics: `tests/fixtures/fp/captures/helpers.txt` — 5 328 probe
//!   points. `nexp01.s`, `mksadj.s`, `mkdadj.s`, `addexp.s`, `addexpm.s` and
//!   `maddn.s` reproduce **all** of their probe rows; `divn.s` is exact on the
//!   divide-sequence envelope (see its doc comment for the honest caveat).
//!
//! # The three ROMs
//!
//! Sixty sweeps, but only three underlying ROMs: `recip0.s` and `div0.s` read
//! the same 128-entry table (7 index bits — the top 7 significand bits), and
//! `rsqrt0.s`/`sqrt0.s` share a pair of 64-entry tables selected by the
//! *biased exponent's parity* (6 index bits). Entries carry exactly 7 result
//! bits, placed at `frac[22:16]`.
//!
//! Denormal inputs are **normalized first** (the index is taken from the
//! normalized significand, and the exponent arithmetic continues below biased
//! zero) — measured directly: the denormal planes' run boundaries shrink to
//! `0x4000`/single-significand granularity exactly as normalization predicts.

/// `recip0.s` / `div0.s` shared ROM: `frac[22:16]` by `significand[22:16]`.
pub const RECIP_DIV_ROM: [u8; 128] = [
    0x7f, 0x7d, 0x7b, 0x79, 0x77, 0x75, 0x74, 0x72, 0x70, 0x6e, 0x6d, 0x6b, 0x69, 0x68, 0x66, 0x64,
    0x63, 0x61, 0x60, 0x5e, 0x5d, 0x5b, 0x5a, 0x58, 0x57, 0x55, 0x54, 0x53, 0x51, 0x50, 0x4f, 0x4d,
    0x4c, 0x4b, 0x4a, 0x48, 0x47, 0x46, 0x45, 0x44, 0x42, 0x41, 0x40, 0x3f, 0x3e, 0x3d, 0x3c, 0x3b,
    0x3a, 0x39, 0x38, 0x37, 0x36, 0x35, 0x34, 0x33, 0x32, 0x31, 0x30, 0x2f, 0x2e, 0x2d, 0x2c, 0x2b,
    0x2a, 0x29, 0x28, 0x28, 0x27, 0x26, 0x25, 0x24, 0x23, 0x23, 0x22, 0x21, 0x20, 0x1f, 0x1f, 0x1e,
    0x1d, 0x1c, 0x1c, 0x1b, 0x1a, 0x19, 0x19, 0x18, 0x17, 0x17, 0x16, 0x15, 0x15, 0x14, 0x13, 0x13,
    0x12, 0x11, 0x11, 0x10, 0x0f, 0x0f, 0x0e, 0x0e, 0x0d, 0x0c, 0x0c, 0x0b, 0x0b, 0x0a, 0x09, 0x09,
    0x08, 0x08, 0x07, 0x07, 0x06, 0x05, 0x05, 0x04, 0x04, 0x03, 0x03, 0x02, 0x02, 0x01, 0x01, 0x01,
];

/// `rsqrt0.s` / `sqrt0.s` shared ROM for **odd** biased exponents:
/// `frac[22:16]` by `significand[22:17]`.
pub const RSQRT_SQRT_ODD_ROM: [u8; 64] = [
    0x7f, 0x7d, 0x7b, 0x79, 0x77, 0x76, 0x74, 0x72, 0x71, 0x6f, 0x6d, 0x6c, 0x6a, 0x69, 0x67, 0x66,
    0x64, 0x63, 0x61, 0x60, 0x5f, 0x5d, 0x5c, 0x5b, 0x5a, 0x58, 0x57, 0x56, 0x55, 0x54, 0x53, 0x52,
    0x50, 0x4f, 0x4e, 0x4d, 0x4c, 0x4b, 0x4a, 0x49, 0x48, 0x47, 0x46, 0x46, 0x45, 0x44, 0x43, 0x42,
    0x41, 0x40, 0x3f, 0x3f, 0x3e, 0x3d, 0x3c, 0x3b, 0x3b, 0x3a, 0x39, 0x38, 0x38, 0x37, 0x36, 0x35,
];

/// `rsqrt0.s` / `sqrt0.s` shared ROM for **even** biased exponents.
pub const RSQRT_SQRT_EVEN_ROM: [u8; 64] = [
    0x34, 0x33, 0x32, 0x30, 0x2f, 0x2e, 0x2c, 0x2b, 0x2a, 0x29, 0x28, 0x27, 0x26, 0x25, 0x23, 0x22,
    0x21, 0x20, 0x1f, 0x1e, 0x1e, 0x1d, 0x1c, 0x1b, 0x1a, 0x19, 0x18, 0x17, 0x17, 0x16, 0x15, 0x14,
    0x13, 0x13, 0x12, 0x11, 0x10, 0x10, 0x0f, 0x0e, 0x0e, 0x0d, 0x0c, 0x0c, 0x0b, 0x0a, 0x0a, 0x09,
    0x09, 0x08, 0x07, 0x07, 0x06, 0x06, 0x05, 0x04, 0x04, 0x03, 0x03, 0x02, 0x02, 0x01, 0x01, 0x00,
];

const SIGN: u32 = 0x8000_0000;
const EXP: u32 = 0x7F80_0000;
const FRAC: u32 = 0x007F_FFFF;

/// FSR flags an operation raised, to be OR-ed into the sticky register.
pub type FsrBits = u32;

#[inline]
fn exp_of(v: u32) -> i32 {
    ((v >> 23) & 0xFF) as i32
}

#[inline]
fn frac_of(v: u32) -> u32 {
    v & FRAC
}

#[inline]
fn is_nan(v: u32) -> bool {
    v & EXP == EXP && v & FRAC != 0
}

#[inline]
fn is_inf(v: u32) -> bool {
    v & EXP == EXP && v & FRAC == 0
}

#[inline]
fn is_zero(v: u32) -> bool {
    v & !SIGN == 0
}

/// Normalize a nonzero finite value: `(biased exponent, 24-bit significand)`.
/// Denormals continue below biased zero, which is exactly what the measured
/// denormal-plane behavior requires.
fn norm(v: u32) -> (i32, u32) {
    let (e, f) = (exp_of(v), frac_of(v));
    if e != 0 {
        return (e, 0x0080_0000 | f);
    }
    let shift = f.leading_zeros() as i32 - 8;
    (1 - shift, f << shift)
}

/// Encode `(sign, biased exponent, 24-bit significand)`, denormalizing by a
/// truncating right-shift below biased 1 and saturating to infinity at 255 —
/// both behaviors measured on the `exp = 253/254` and denormal-input planes.
fn encode(sign: u32, e: i32, sig: u32) -> u32 {
    if e >= 255 {
        return sign | EXP;
    }
    if e <= 0 {
        let shift = 1 - e;
        if shift > 31 {
            return sign;
        }
        return sign | (sig >> shift);
    }
    sign | ((e as u32) << 23) | (sig & FRAC)
}

/// `recip0.s` — reciprocal estimate.
///
/// Measured behavior: `±0 → ±inf` (+ divide-by-zero flag), `±inf → ±0`,
/// NaN → NaN with payload `(implied1 | ROM) >> 1` (quiet bit set by the
/// shifted-in implied 1); otherwise `exponent → 253 − e`, ROM significand,
/// with subnormal/overflow encoding at the range edges.
pub fn recip0(v: u32) -> (u32, FsrBits) {
    let sgn = v & SIGN;
    if is_nan(v) {
        let payload = (0x0080_0000 | rom_recip(frac_of(v) >> 16)) >> 1;
        return (sgn | EXP | payload, 0);
    }
    if is_inf(v) {
        return (sgn, 0);
    }
    if is_zero(v) {
        // No divide-by-zero flag: the sequences' Z flag comes from
        // `mkdadj.s`, and the 0/0 rows (INVALID only, no Z) falsify any
        // flag here. P1's coarse 0x400 reading is attributable to its
        // mkdadj probe; the estimate-on-zero flag question is recorded as
        // not separably probed.
        return (sgn | EXP, 0);
    }
    let (e, sig) = norm(v);
    let idx = (sig >> 16) & 0x7F;
    (encode(sgn, 253 - e, 0x0080_0000 | rom_recip(idx)), 0)
}

/// `div0.s` — divide-sequence seed: same ROM as `recip0.s`, but the output
/// exponent carries only the input exponent's **parity** (`125 + (e & 1)`);
/// the divide sequence reconstructs the true exponent through `mkdadj.s`.
/// No special classes at all — NaN and infinity go straight through the
/// table (measured on the `exp = 255` planes), and zero reads the table's
/// first entry (+ divide-by-zero flag).
pub fn div0(v: u32) -> (u32, FsrBits) {
    let sgn = v & SIGN;
    if is_zero(v) {
        // Flag-free, like every estimate: the x/0 sequences' Z flag is
        // mkdadj.s's, not this instruction's (see recip0).
        return (sgn | (126 << 23) | rom_recip(0), 0);
    }
    if exp_of(v) == 255 {
        let idx = frac_of(v) >> 16;
        return (sgn | (126 << 23) | rom_recip(idx), 0);
    }
    let (e, sig) = norm(v);
    let idx = (sig >> 16) & 0x7F;
    let oe = (125 + (e & 1)) as u32;
    (sgn | (oe << 23) | rom_recip(idx), 0)
}

/// `rsqrt0.s` — reciprocal-square-root estimate. Parity-selected ROM,
/// `exponent → (380 − e) >> 1`; `+inf → +0`, `±0 → ±inf` (+ divide-by-zero),
/// and NaN or negative input → **quieted** NaN carrying the table value
/// (`| 0x400000` — which is why the negative plane appears to "restart" at
/// table entries below `0x40`).
pub fn rsqrt0(v: u32) -> (u32, FsrBits) {
    let sgn = v & SIGN;
    if is_zero(v) {
        return (sgn | EXP, 0);
    }
    if sgn != 0 {
        let payload = if exp_of(v) == 255 {
            rom_rsqrt_odd(frac_of(v) >> 17)
        } else {
            let (e, sig) = norm(v);
            rom_rsqrt(e, (sig >> 17) & 0x3F)
        };
        return (sgn | EXP | 0x0040_0000 | payload, 0);
    }
    if is_inf(v) {
        return (sgn, 0);
    }
    if is_nan(v) {
        return (sgn | EXP | 0x0040_0000 | rom_rsqrt_odd(frac_of(v) >> 17), 0);
    }
    let (e, sig) = norm(v);
    let idx = (sig >> 17) & 0x3F;
    (
        encode(sgn, (380 - e) >> 1, 0x0080_0000 | rom_rsqrt(e, idx)),
        0,
    )
}

/// `sqrt0.s` — square-root-sequence seed. Pure parity-ROM transform: the
/// output exponent is **always 126** and the sign passes through — even for
/// NaN, infinity and zero inputs (measured; the sequence's specials come from
/// `mksadj.s`, not from here).
pub fn sqrt0(v: u32) -> (u32, FsrBits) {
    let sgn = v & SIGN;
    let payload = if exp_of(v) == 255 {
        rom_rsqrt_odd(frac_of(v) >> 17)
    } else if is_zero(v) {
        rom_rsqrt(0, 0)
    } else {
        let (e, sig) = norm(v);
        rom_rsqrt(e, (sig >> 17) & 0x3F)
    };
    (sgn | (126 << 23) | payload, 0)
}

#[inline]
fn rom_recip(idx: u32) -> u32 {
    u32::from(RECIP_DIV_ROM[(idx & 0x7F) as usize]) << 16
}

#[inline]
fn rom_rsqrt(e: i32, idx: u32) -> u32 {
    if e & 1 != 0 {
        rom_rsqrt_odd(idx)
    } else {
        u32::from(RSQRT_SQRT_EVEN_ROM[(idx & 0x3F) as usize]) << 16
    }
}

#[inline]
fn rom_rsqrt_odd(idx: u32) -> u32 {
    u32::from(RSQRT_SQRT_ODD_ROM[(idx & 0x3F) as usize]) << 16
}

// ---------------------------------------------------------------------------
// The divide/sqrt helper instructions
// ---------------------------------------------------------------------------

/// `nexp01.s` — normalize-and-negate: the significand mapped into `[1, 4)` by
/// the biased exponent's parity (odd → `[2, 4)`), with the **sign inverted**.
/// NaN passes through with only its sign flipped (payload untouched, sNaN
/// stays signalling); zero and infinity map to `∓1.0`. Never sets a flag.
pub fn nexp01(s: u32) -> u32 {
    if is_nan(s) {
        return s ^ SIGN;
    }
    let sgn = (s ^ SIGN) & SIGN;
    if is_zero(s) || is_inf(s) {
        return sgn | 0x3F80_0000;
    }
    let (e, sig) = norm(s);
    let odd = ((e - 127) & 1) as u32;
    sgn | ((127 + odd) << 23) | (sig & FRAC)
}

/// The `mksadj.s`/`mkdadj.s` encoding: an exponent adjustment `A` split into
/// two mod-256 byte fields — `A mod 32` scaled by 8 into the **exponent**
/// field (consumed by `addexp.s`) and `A >> 5` scaled by 8 into `frac[21:14]`
/// (consumed by `addexpm.s`, whose `+129` bias cancels the `127 + …` here).
/// `frac[15:14]` is always `0b11`. Special results are encoded as `m = 223`
/// with a class code in the exponent field.
fn adj_encode(exp_field: i32, m: i32) -> u32 {
    (((exp_field & 0xFF) as u32) << 23) | (((m & 0xFF) as u32) << 14) | 0xC000
}

fn adj_split(a: i32) -> u32 {
    adj_encode(127 + 8 * (a.rem_euclid(32)), 127 + 8 * (a.div_euclid(32)))
}

/// Result-class codes for the special encodings (`m = 223`), measured:
/// `127 + 8·{+0, −0, +inf, −inf, +NaN, −NaN}`.
fn adj_class(code: i32) -> u32 {
    adj_encode(127 + 8 * code, 223)
}

/// `mksadj.s` — the square-root sequence's exponent adjustment:
/// `A = ⌊(e − 127) / 2⌋` for positive finite input; class encodings for
/// zero/infinity/negative/NaN. Sets INVALID for negative (including −inf)
/// and signalling-NaN inputs — but not for quiet NaN or −0.
pub fn mksadj(s: u32) -> (u32, FsrBits) {
    let invalid = crate::cpu::FSR_INVALID;
    if is_zero(s) {
        return (adj_class(if s >> 31 != 0 { 1 } else { 0 }), 0);
    }
    if is_nan(s) {
        let signalling = s & 0x0040_0000 == 0;
        return (adj_class(4), if signalling { invalid } else { 0 });
    }
    if s >> 31 != 0 {
        return (adj_class(4), invalid);
    }
    if is_inf(s) {
        return (adj_class(2), 0);
    }
    let (e, _) = norm(s);
    (adj_split((e - 127).div_euclid(2)), 0)
}

/// `mkdadj.s fr, fs` — the divide sequence's exponent adjustment, where `fr`
/// holds the **denominator** and `fs` the **numerator** (that is how the
/// toolchain's `__divsf3` stages it). For finite nonzero operands
/// `A = 2⌊(e_num − 127)/2⌋ − 2⌊(e_den − 127)/2⌋` — only the even parts,
/// because `nexp01.s`/`div0.s` carry both parities in the significand path.
/// Class encodings otherwise, with the result sign `sign_num ^ sign_den`.
/// Flags: divide-by-zero for `finite/0`, INVALID for `0/0`, `inf/inf`, or a
/// signalling-NaN operand.
pub fn mkdadj(den: u32, num: u32) -> (u32, FsrBits) {
    let rs = ((den ^ num) >> 31) as i32;
    let nan_in = is_nan(den) || is_nan(num);
    let snan_in =
        (is_nan(den) && den & 0x0040_0000 == 0) || (is_nan(num) && num & 0x0040_0000 == 0);
    if nan_in || (is_zero(num) && is_zero(den)) || (is_inf(num) && is_inf(den)) {
        let invalid = if !nan_in || snan_in {
            crate::cpu::FSR_INVALID
        } else {
            0
        };
        return (adj_class(4 + rs), invalid);
    }
    if is_zero(num) || is_inf(den) {
        return (adj_class(rs), 0);
    }
    if is_inf(num) || is_zero(den) {
        // IEEE's divide-by-zero condition, measured: the flag is for a
        // FINITE nonzero dividend over zero. inf/0 is an exact infinity and
        // raises nothing (the inf/±0 sequence rows read FSR 0).
        let dbz = if is_zero(den) && !is_inf(num) {
            crate::cpu::FSR_DIV_BY_ZERO
        } else {
            0
        };
        return (adj_class(2 + rs), dbz);
    }
    let (en, _) = norm(num);
    let (ed, _) = norm(den);
    let a = 2 * (en - 127).div_euclid(2) - 2 * (ed - 127).div_euclid(2);
    (adj_split(a), 0)
}

/// `addexp.s fr, fs` — pure bit operation: `fr.exp += e_s − 127 (mod 256)`,
/// `fr.sign ^= fs.sign`; the fraction is untouched. Never sets a flag.
pub fn addexp(r: u32, s: u32) -> u32 {
    let d = exp_of(s) - 127;
    let ne = ((exp_of(r) + d) & 0xFF) as u32;
    (r ^ (s & SIGN)) & !EXP | (ne << 23)
}

/// `addexpm.s fr, fs` — the other half of the split encoding:
/// `fr.sign ^= fs.frac[22]`, `fr.exp += 129 + fs.frac[21:14] (mod 256)`.
/// Never sets a flag.
pub fn addexpm(r: u32, s: u32) -> u32 {
    let m = ((frac_of(s) >> 14) & 0xFF) as i32;
    let flip = (frac_of(s) >> 22) & 1;
    let ne = ((exp_of(r) + 129 + m) & 0xFF) as u32;
    (r ^ (flip << 31)) & !EXP | (ne << 23)
}

/// `divn.s fr, fs, ft` — the sequences' final correctly-rounded step, and the
/// piece that reassembles `mkdadj.s`/`mksadj.s`'s split exponent encoding.
///
/// Measured semantics (fitted on the sequence-instrumented operand dumps and
/// validated against the probe grid and all 272 end-to-end sequence rows):
///
/// 1. Both `fr`'s and `ft`'s exponents are **decomposed** as
///    `exc = 8k + p` with `p ∈ [-4, 3]`, where `exc` is the biased-exponent
///    excess over 127 **sign-extended as an 8-bit value** (so exponent field
///    255 reads as −128, which is why a NaN accumulator underflows to zero).
/// 2. The value computed is `(sig_r·2^p_r + fs · sig_t·2^p_t) · 2^A` with
///    `A = 32·k_r + (k_t mod 32)` — exactly undoing the `8·(A mod 32)` /
///    `8·(A div 32)` split that `addexp.s`/`addexpm.s` applied. `fs` is used
///    at face value; both `sig` terms get the implied 1 forced regardless of
///    the exponent field (a zero or denormal encoding reads as `1.frac`).
/// 3. The sum is rounded once to f32, round-to-nearest-even with the exact
///    residual as sticky — which is what makes the divide sequence correctly
///    rounded. INEXACT/UNDERFLOW/OVERFLOW are raised accordingly; INVALID
///    never is.
/// 4. An **exactly cancelling** sum does not produce zero: silicon returns
///    `+2^(A-50)` — the 50-bit normalization window running off the end.
pub fn divn(r: u32, s: u32, t: u32) -> (u32, FsrBits) {
    #[inline]
    fn exc8(v: u32) -> i32 {
        ((((v >> 23) & 0xFF) as i32 - 127 + 128) & 0xFF) - 128
    }
    #[inline]
    fn split(exc: i32) -> (i32, i32) {
        let k = (exc + 4).div_euclid(8);
        (k, exc - 8 * k)
    }
    #[inline]
    fn signed_sig(v: u32, p: i32) -> f64 {
        let sig = f64::from(0x0080_0000 | (v & FRAC)) / f64::from(0x0080_0000u32);
        let m = sig * (p as f64).exp2();
        if v & SIGN != 0 { -m } else { m }
    }

    // A NaN in `fs` propagates — it is the one operand divn uses at face
    // value, and the x/NaN sequence rows show its bits (sign and payload
    // included) coming straight through with no flags. `fr` and `ft` NaN
    // *encodings* do not propagate: their exponent fields read as −128 and
    // their significands as numbers, exactly like every other bit pattern
    // (measured in the round-1 probe grid).
    if (s & EXP) == EXP && (s & FRAC) != 0 {
        return (s | 0x0040_0000, 0);
    }

    let (kr, pr) = split(exc8(r));
    let (kt, pt) = split(exc8(t));
    let a = 32 * kr + kt.rem_euclid(32);

    // The class window. mkdadj.s/mksadj.s encode special results as m = 223
    // (which addexpm.s turns into a +96 exponent kick, i.e. k_r = 12 →
    // A = 384) plus a class code in k_t: measured across the F5 sequence
    // sweep, A − 384 ∈ {0,1} produces a zero, {2,3} an infinity — both taking
    // the sign of the sum — and {4,5} the canonical quiet NaN with INVALID.
    // The window edges beyond the six codes are unreachable from the
    // sequences and are recorded as unprobed in the campaign record.
    if (384..390).contains(&a) {
        // The code's low bit IS the result sign (mkdadj/mksadj folded the
        // operand signs into the code) — the operand signs at divn are
        // ignored, which is what sqrt(-0) = -0 pins: its sum is positive
        // but its class code is 1.
        let code = a - 384;
        let sign = ((code & 1) as u32) << 31;
        return match code / 2 {
            0 => (sign, 0),
            1 => (sign | EXP, 0),
            _ => (0x7FC0_0000, crate::cpu::FSR_INVALID),
        };
    }

    let q = signed_sig(r, pr);
    // `fs` at face value. Zero/denormal encodings still get the implied 1 —
    // measured on the t-operand; fs in the sequences is a rounded remainder
    // and always a true value, so the same reading is used.
    let sv = f64::from(f32::from_bits(s));
    let corr_mag = signed_sig(t, pt);
    let (head, residual) = {
        // Both terms are exact in f64 (24-bit and 48-bit significands).
        let prod = sv * corr_mag;
        let sum = q + prod;
        let bb = sum - q;
        let err = (q - (sum - bb)) + (prod - bb);
        (sum, err)
    };

    if head == 0.0 && residual == 0.0 {
        // Exact cancellation: the 50-bit normalization window runs dry and
        // silicon emits +2^(A-50).
        let e32 = a - 50 + 127;
        return (encode(0, e32, 0x0080_0000), 0);
    }

    let (bits, fsr) = round_scaled_rne(head, residual, a);
    (bits, fsr)
}

/// Round `(head + residual) · 2^scale` to f32, round-to-nearest-even with the
/// residual as the tie-breaking sticky, across the full f32 range (subnormals,
/// overflow). Returns the bits and the I/U/O flags of the rounding.
fn round_scaled_rne(head: f64, residual: f64, scale: i32) -> (u32, FsrBits) {
    let sign = if head < 0.0 || (head == 0.0 && head.is_sign_negative()) {
        1u32
    } else {
        0
    };
    if head == 0.0 {
        // Residual-only value: far below every grid; rounds to ±0, inexact
        // if the residual is real.
        let fsr = if residual != 0.0 {
            crate::cpu::FSR_INEXACT | crate::cpu::FSR_UNDERFLOW
        } else {
            0
        };
        return ((sign << 31) | 0, fsr);
    }
    let hb = head.abs().to_bits();
    let e2 = ((hb >> 52) & 0x7FF) as i32 - 1023;
    let m = (hb & 0xF_FFFF_FFFF_FFFF) | (1 << 52);
    // Residual direction in magnitude terms.
    let mdir = if sign == 1 {
        -sign_of(residual)
    } else {
        sign_of(residual)
    };

    let e32 = e2 + 127 + scale;
    // Bits below the target grid: 29 for a normal result, more when
    // subnormal.
    let drop = if e32 >= 1 { 29 } else { 29 + (1 - e32) };
    if e32 >= 255 + 1 || drop >= 64 {
        return extreme_rne(sign, e32 >= 255);
    }
    let kept = m >> drop;
    let dropped = m & ((1u64 << drop) - 1);
    let half = 1u64 << (drop - 1);
    let round_up = match dropped.cmp(&half) {
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Equal => match mdir {
            1 => true,
            -1 => false,
            _ => kept & 1 == 1, // ties to even
        },
    };
    let mut sig = kept + u64::from(round_up);
    let mut e32 = e32;
    let inexact = dropped != 0 || residual != 0.0;
    if e32 >= 1 {
        if sig == 1 << 24 {
            sig = 1 << 23;
            e32 += 1;
        }
        if e32 >= 255 {
            return extreme_rne(sign, true);
        }
        let bits = (sign << 31) | ((e32 as u32) << 23) | ((sig as u32) & FRAC);
        let fsr = if inexact { crate::cpu::FSR_INEXACT } else { 0 };
        return (bits, fsr);
    }
    // Subnormal grid; `sig` reaching 2^23 is exactly the smallest normal.
    let bits = (sign << 31) | (sig as u32);
    let fsr = if inexact {
        let tiny = (sig as u32) < (1 << 23);
        crate::cpu::FSR_INEXACT | if tiny { crate::cpu::FSR_UNDERFLOW } else { 0 }
    } else {
        0
    };
    (bits, fsr)
}

fn extreme_rne(sign: u32, huge: bool) -> (u32, FsrBits) {
    if huge {
        (
            (sign << 31) | EXP,
            crate::cpu::FSR_INEXACT | crate::cpu::FSR_OVERFLOW,
        )
    } else {
        (
            sign << 31,
            crate::cpu::FSR_INEXACT | crate::cpu::FSR_UNDERFLOW,
        )
    }
}

fn sign_of(x: f64) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handful of pinned silicon values, transcribed by hand from the
    /// captures — the exhaustive checks live in `tests/fp_silicon_replay.rs`,
    /// but these keep the module self-evidently wired even without fixtures.
    #[test]
    fn pinned_silicon_samples() {
        assert_eq!(recip0(0x3F80_0000).0, 0x3F7F_0000, "recip0(1.0)");
        assert_eq!(recip0(0x7F80_0000).0, 0x0000_0000, "recip0(inf) = 0");
        assert_eq!(recip0(0x0000_0000), (0x7F80_0000, 0));
        assert_eq!(
            recip0(0x7E80_0000).0,
            0x007F_8000,
            "recip0 at exp 253 denormalizes"
        );
        assert_eq!(
            div0(0x7FC0_0000).0,
            0x3F2A_0000,
            "div0(NaN) goes through the table"
        );
        assert_eq!(
            rsqrt0(0xBF80_0000).0,
            0xFFFF_0000,
            "rsqrt0(-1) = quieted table NaN"
        );
        assert_eq!(rsqrt0(0x3F80_0000).0, 0x3F7F_0000, "rsqrt0(1.0)");
        assert_eq!(
            sqrt0(0x7F80_0000).0,
            0x3F7F_0000,
            "sqrt0(inf) is just the table"
        );
        assert_eq!(sqrt0(0x4000_0000).0, 0x3F34_0000, "sqrt0(2.0) even ROM");
        assert_eq!(nexp01(0x4049_0FDB), 0xC049_0FDB, "nexp01(pi) = -pi");
        assert_eq!(nexp01(0x3F00_0000), 0xC000_0000, "nexp01(0.5) = -2.0");
        assert_eq!(
            nexp01(0x7FD5_AA55),
            0xFFD5_AA55,
            "nexp01 flips only the NaN sign"
        );
        assert_eq!(mksadj(0x3F80_0000).0, 0x3F9F_C000, "mksadj(1.0)");
        assert_eq!(
            mksadj(0xBF80_0000),
            (0x4FB7_C000, crate::cpu::FSR_INVALID),
            "mksadj(-1) = NaN class + INVALID"
        );
        assert_eq!(
            mkdadj(0x3F80_0000, 0x3F00_0000).0,
            0x379D_C000,
            "mkdadj(1.0, 0.5)"
        );
        assert_eq!(
            mkdadj(0x1234_5678, 0x3F80_0000).0,
            0x2FA3_C000,
            "mkdadj far exponents"
        );
        assert_eq!(addexp(0x1234_5678, 0x1F80_0000), 0x7234_5678, "addexp -64");
        assert_eq!(
            addexpm(0x1234_5678, 0x4049_0FDB),
            0xE4B4_5678,
            "addexpm from pi's frac"
        );
    }

    /// The three ROM shapes: 7 significant bits each, monotone non-increasing
    /// (they approximate decreasing functions of the significand).
    #[test]
    fn rom_shape_invariants() {
        assert!(RECIP_DIV_ROM.windows(2).all(|w| w[0] >= w[1]));
        assert!(RSQRT_SQRT_ODD_ROM.windows(2).all(|w| w[0] >= w[1]));
        assert!(RSQRT_SQRT_EVEN_ROM.windows(2).all(|w| w[0] >= w[1]));
        assert!(RECIP_DIV_ROM.iter().all(|&e| e < 0x80));
    }
}
