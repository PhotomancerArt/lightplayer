//! Direct probes of the soft-float ABI symbols the f32 lowering calls.
//!
//! This half of the harness bypasses the compiler entirely: it calls
//! `__addsf3`, `__ltsf2`, … the same way the JIT-generated code does, and
//! compares the raw result words to IEEE-754 binary32 reference values
//! **computed off-device and written here as bit patterns**.
//!
//! Baking the references in as constants is not laziness — it is the only way
//! the test means anything. On this chip a Rust `a + b` on two `f32`s *is* a
//! call to `__addsf3`, so computing the expected value on-device would compare
//! the routine to itself and pass no matter how wrong it was.
//!
//! What is actually under test here is **the ESP32-C6 mask ROM**. The linker
//! resolves these names through `esp-rom-sys`'s `esp32c6.rom.rvfp.ld` to
//! Espressif's ROM-resident `rvfplib` (`__addsf3 = 0x400009f8`), not to Rust's
//! `compiler_builtins` — so this is a different implementation from the one the
//! host emulator runs, and the two agreeing is a fact to establish rather than
//! assume.

use esp_println::println;

use super::report::Report;

// SAFETY (declaration only): the standard soft-float ABI entry points. On this
// target the linker binds them to ROM addresses; the harness calls them through
// the same C ABI the JIT-generated code uses.
unsafe extern "C" {
    fn __addsf3(a: f32, b: f32) -> f32;
    fn __subsf3(a: f32, b: f32) -> f32;
    fn __mulsf3(a: f32, b: f32) -> f32;
    fn __divsf3(a: f32, b: f32) -> f32;
    fn __eqsf2(a: f32, b: f32) -> i32;
    fn __nesf2(a: f32, b: f32) -> i32;
    fn __ltsf2(a: f32, b: f32) -> i32;
    fn __lesf2(a: f32, b: f32) -> i32;
    fn __gtsf2(a: f32, b: f32) -> i32;
    fn __gesf2(a: f32, b: f32) -> i32;
    fn __floatsisf(a: i32) -> f32;
    fn __floatunsisf(a: u32) -> f32;
    // Not called by our lowering — probed for data only. See
    // `lpvm_native::lower_f32::f32_ftoi_sat_s_symbol` for why float→int goes
    // through a LightPlayer builtin instead.
    fn __fixsfsi(a: f32) -> i32;
    fn __fixunssfsi(a: f32) -> u32;
}

const F_1_0: u32 = 0x3F80_0000;
const F_2_0: u32 = 0x4000_0000;
const F_3_0: u32 = 0x4040_0000;
const F_0_1: u32 = 0x3DCC_CCCD;
const F_0_2: u32 = 0x3E4C_CCCD;
const F_0_3_SUM: u32 = 0x3E99_999A;
const F_1_3RD: u32 = 0x3EAA_AAAB;
const F_1E30: u32 = 0x7149_F2CA;
const F_1E_M40: u32 = 0x0001_16C2;
const F_2E_M40: u32 = 0x0002_2D84;
const F_POS_INF: u32 = 0x7F80_0000;
const F_NEG_INF: u32 = 0xFF80_0000;
const F_QNAN: u32 = 0x7FC0_0000;
const F_ZERO: u32 = 0x0000_0000;
const F_NEG_ZERO: u32 = 0x8000_0000;
const F_2_POW_24: u32 = 0x4B80_0000;
const F_MIN_SUBNORMAL: u32 = 0x0000_0001;

fn bits(x: f32) -> u32 {
    x.to_bits()
}

fn of(b: u32) -> f32 {
    // `black_box` keeps the constant opaque to LLVM. Without it the optimizer is
    // free to constant-fold an operand into a call it can evaluate at compile
    // time on the *host*, which would silently move the test off the ROM.
    core::hint::black_box(f32::from_bits(b))
}

/// Run every ABI probe, appending to `report`.
pub fn run(report: &mut Report) {
    println!("[f32-soft] --- soft-float ABI probes (ESP32-C6 ROM rvfplib) ---");
    println!(
        "[f32-soft] symbol addresses: __addsf3={:#010x} __mulsf3={:#010x} __divsf3={:#010x} \
         __ltsf2={:#010x}",
        __addsf3 as *const () as usize,
        __mulsf3 as *const () as usize,
        __divsf3 as *const () as usize,
        __ltsf2 as *const () as usize,
    );

    arithmetic(report);
    edge_values(report);
    subnormals(report);
    comparisons(report);
    conversions(report);
    float_to_int_data_only();
}

fn arithmetic(report: &mut Report) {
    check(
        report,
        "__addsf3(1,2)",
        bits(unsafe { __addsf3(of(F_1_0), of(F_2_0)) }),
        F_3_0,
    );
    check(
        report,
        "__subsf3(3,1)",
        bits(unsafe { __subsf3(of(F_3_0), of(F_1_0)) }),
        F_2_0,
    );
    check(
        report,
        "__mulsf3(3,2)",
        bits(unsafe { __mulsf3(of(F_3_0), of(F_2_0)) }),
        0x40C0_0000,
    );
    // The classic: 0.1 + 0.2 must round to 0.30000001192092896, not 0.3.
    // Anything else means the routine is not round-to-nearest-even.
    check(
        report,
        "__addsf3(0.1,0.2)",
        bits(unsafe { __addsf3(of(F_0_1), of(F_0_2)) }),
        F_0_3_SUM,
    );
    // 1/3 is the one-bit-wrong detector for a reciprocal-multiply shortcut.
    check(
        report,
        "__divsf3(1,3)",
        bits(unsafe { __divsf3(of(F_1_0), of(F_3_0)) }),
        F_1_3RD,
    );
}

fn edge_values(report: &mut Report) {
    check(
        report,
        "__mulsf3(1e30,1e30) overflows to +inf",
        bits(unsafe { __mulsf3(of(F_1E30), of(F_1E30)) }),
        F_POS_INF,
    );
    check(
        report,
        "__divsf3(1,0) = +inf",
        bits(unsafe { __divsf3(of(F_1_0), of(F_ZERO)) }),
        F_POS_INF,
    );
    check(
        report,
        "__divsf3(1,-0) = -inf",
        bits(unsafe { __divsf3(of(F_1_0), of(F_NEG_ZERO)) }),
        F_NEG_INF,
    );
    // 0/0 is a NaN; the *payload* is not something we pin (float.md §5), so
    // this only asserts NaN-ness.
    let zero_over_zero = bits(unsafe { __divsf3(of(F_ZERO), of(F_ZERO)) });
    report.record(
        "__divsf3(0,0) is NaN",
        is_nan_bits(zero_over_zero),
        zero_over_zero,
        F_QNAN,
    );
}

fn subnormals(report: &mut Report) {
    // The reason this is here: flush-to-zero would make both of these come back
    // as 0x00000000. `docs/design/float.md` marks denormal FTZ target-defined,
    // so a `0` here is data rather than a defect — but it is data we must have
    // before the C6 can be trusted as an f32 oracle.
    check(
        report,
        "__addsf3(1e-40,1e-40) keeps the subnormal",
        bits(unsafe { __addsf3(of(F_1E_M40), of(F_1E_M40)) }),
        F_2E_M40,
    );
    check(
        report,
        "__mulsf3(min-subnormal,2) doubles it",
        bits(unsafe { __mulsf3(of(F_MIN_SUBNORMAL), of(F_2_0)) }),
        0x0000_0002,
    );
}

fn comparisons(report: &mut Report) {
    // The lowering tests each result's SIGN. What matters is that the sign is on
    // the correct side of zero, including for NaN, where every comparison but
    // `!=` must come out false.
    let lt = unsafe { __ltsf2(of(F_1_0), of(F_2_0)) };
    report.record("__ltsf2(1,2) < 0", lt < 0, lt as u32, 0);
    let lt_nan = unsafe { __ltsf2(of(F_QNAN), of(F_1_0)) };
    report.record("__ltsf2(NaN,1) not < 0", !(lt_nan < 0), lt_nan as u32, 0);

    let le = unsafe { __lesf2(of(F_2_0), of(F_2_0)) };
    report.record("__lesf2(2,2) <= 0", le <= 0, le as u32, 0);
    let le_nan = unsafe { __lesf2(of(F_QNAN), of(F_1_0)) };
    report.record("__lesf2(NaN,1) not <= 0", !(le_nan <= 0), le_nan as u32, 0);

    let gt = unsafe { __gtsf2(of(F_2_0), of(F_1_0)) };
    report.record("__gtsf2(2,1) > 0", gt > 0, gt as u32, 0);
    let gt_nan = unsafe { __gtsf2(of(F_QNAN), of(F_1_0)) };
    report.record("__gtsf2(NaN,1) not > 0", !(gt_nan > 0), gt_nan as u32, 0);

    let ge = unsafe { __gesf2(of(F_2_0), of(F_2_0)) };
    report.record("__gesf2(2,2) >= 0", ge >= 0, ge as u32, 0);
    let ge_nan = unsafe { __gesf2(of(F_QNAN), of(F_1_0)) };
    report.record("__gesf2(NaN,1) not >= 0", !(ge_nan >= 0), ge_nan as u32, 0);

    let eq = unsafe { __eqsf2(of(F_2_0), of(F_2_0)) };
    report.record("__eqsf2(2,2) == 0", eq == 0, eq as u32, 0);
    let eq_nan = unsafe { __eqsf2(of(F_QNAN), of(F_QNAN)) };
    report.record("__eqsf2(NaN,NaN) != 0", eq_nan != 0, eq_nan as u32, 0);
    // IEEE: `-0.0 == 0.0` is true even though the bit patterns differ.
    let eq_zeros = unsafe { __eqsf2(of(F_NEG_ZERO), of(F_ZERO)) };
    report.record("__eqsf2(-0,+0) == 0", eq_zeros == 0, eq_zeros as u32, 0);

    // `!=` is the one comparison that is TRUE for unordered operands.
    let ne_nan = unsafe { __nesf2(of(F_QNAN), of(F_QNAN)) };
    report.record("__nesf2(NaN,NaN) != 0", ne_nan != 0, ne_nan as u32, 0);
}

fn conversions(report: &mut Report) {
    check(
        report,
        "__floatsisf(1)",
        bits(unsafe { __floatsisf(core::hint::black_box(1)) }),
        F_1_0,
    );
    // 2^24 + 1 is not representable; round-to-nearest-even gives 2^24.
    check(
        report,
        "__floatsisf(2^24+1) rounds to 2^24",
        bits(unsafe { __floatsisf(core::hint::black_box(16_777_217)) }),
        F_2_POW_24,
    );
    check(
        report,
        "__floatsisf(i32::MIN)",
        bits(unsafe { __floatsisf(core::hint::black_box(i32::MIN)) }),
        0xCF00_0000,
    );
    check(
        report,
        "__floatunsisf(u32::MAX)",
        bits(unsafe { __floatunsisf(core::hint::black_box(u32::MAX)) }),
        0x4F80_0000,
    );
}

/// Probe `__fixsfsi`/`__fixunssfsi` and **print** what the ROM does, without
/// asserting anything.
///
/// The lowering deliberately does not call these: the soft-float ABI leaves
/// out-of-range and NaN conversion undefined, so the ROM is free to differ from
/// `compiler_builtins` (which the host emulator uses) at exactly the edges the
/// corpus tests. This exists so that decision can be revisited against measured
/// behavior instead of a guess.
fn float_to_int_data_only() {
    let nan = unsafe { __fixsfsi(of(F_QNAN)) };
    let huge = unsafe { __fixsfsi(of(F_1E30)) };
    let neg_huge = unsafe { __fixsfsi(of(F_1E30 | 0x8000_0000)) };
    let u_neg = unsafe { __fixunssfsi(of(F_1_0 | 0x8000_0000)) };
    println!(
        "[f32-soft] DATA (not asserted) __fixsfsi: NaN->{nan} 1e30->{huge} -1e30->{neg_huge} \
         __fixunssfsi(-1.0)->{u_neg}"
    );
    println!(
        "[f32-soft] DATA compiler_builtins/Rust-`as` would give: NaN->0 1e30->{} -1e30->{} \
         __fixunssfsi(-1.0)->0",
        i32::MAX,
        i32::MIN,
    );
}

fn check(report: &mut Report, what: &str, got: u32, want: u32) {
    report.record(what, got == want, got, want);
}

fn is_nan_bits(b: u32) -> bool {
    (b & 0x7F80_0000) == 0x7F80_0000 && (b & 0x007F_FFFF) != 0
}
