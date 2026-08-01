//! End-to-end f32 shader cases: GLSL → LPIR → RV32 machine code → execution,
//! all on the C6, in `FloatMode::F32`.
//!
//! Where [`super::abi_probe`] measures the ROM library on its own, this measures
//! the whole stack the product would use: our `lps-glsl` frontend, `lpvm-native`
//! lowering the float ops to `__addsf3`-class calls, the JIT relocating those
//! calls against the ROM addresses, and the argument/return marshalling that
//! carries an f32 as its bit pattern in an integer register.
//!
//! Expected values are IEEE bit patterns computed off-device, for the same
//! reason as in `abi_probe`.

use alloc::sync::Arc;

use esp_println::println;
use lps_shared::LpsValueF32;
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine};

use super::report::Report;

/// One `f(args) -> f32` case, checked by result bit pattern.
struct Case {
    name: &'static str,
    func: &'static str,
    args: &'static [f32],
    want_bits: u32,
}

/// The shader. Every function is deliberately small: a failure should point at
/// one lowering arm, not at a composition of five.
const SOURCE: &str = r#"
// Plain arithmetic — __addsf3 / __mulsf3 / __divsf3.
float add(float a, float b) { return a + b; }
float mul(float a, float b) { return a * b; }
float div(float a, float b) { return a / b; }

// The capability f32 is being added for: Q16.16 tops out near 32768, so this
// result is unrepresentable in the fixed-point mode and would wrap.
float big(float x) { return x * 1000000.0; }

// Comparisons go through __ltsf2/__gtsf2 and a sign test.
float pick_smaller(float a, float b) { return a < b ? a : b; }

// int -> float (__floatsisf) and back (a LightPlayer builtin, not compiler-rt).
float from_int(int n) { return float(n); }
int to_int(float x) { return int(x); }

// Ops with no soft-float ABI symbol: these reach the native-f32 builtin family.
float root(float x) { return sqrt(x); }
float down(float x) { return floor(x); }

// Sign-bit ops that lower to integer masks, no call at all.
float magnitude(float x) { return abs(x); }
float flip(float x) { return -x; }
"#;

const CASES: &[Case] = &[
    Case {
        name: "add(1,2) == 3",
        func: "add",
        args: &[1.0, 2.0],
        want_bits: 0x4040_0000,
    },
    Case {
        name: "add(0.1,0.2) rounds to 0.30000001",
        func: "add",
        args: &[0.1, 0.2],
        want_bits: 0x3E99_999A,
    },
    Case {
        name: "mul(3,2) == 6",
        func: "mul",
        args: &[3.0, 2.0],
        want_bits: 0x40C0_0000,
    },
    Case {
        name: "div(1,3) == 0.33333334",
        func: "div",
        args: &[1.0, 3.0],
        want_bits: 0x3EAA_AAAB,
    },
    // 12345.678 * 1e6 = 1.2345678e10 — four orders of magnitude past Q16.16's
    // ceiling. In Fixed mode this shader is meaningless; that is the point.
    Case {
        name: "big(12345.678) reaches 1.2345678e10",
        func: "big",
        args: &[12345.678],
        want_bits: 0x5037_F706,
    },
    Case {
        name: "pick_smaller(2,1) == 1",
        func: "pick_smaller",
        args: &[2.0, 1.0],
        want_bits: 0x3F80_0000,
    },
    Case {
        name: "pick_smaller(-1.5,1) == -1.5",
        func: "pick_smaller",
        args: &[-1.5, 1.0],
        want_bits: 0xBFC0_0000,
    },
    Case {
        name: "root(7) == 2.6457514",
        func: "root",
        args: &[7.0],
        want_bits: 0x4029_53FD,
    },
    Case {
        name: "down(-1.5) == -2",
        func: "down",
        args: &[-1.5],
        want_bits: 0xC000_0000,
    },
    Case {
        name: "magnitude(-2.5) == 2.5",
        func: "magnitude",
        args: &[-2.5],
        want_bits: 0x4020_0000,
    },
    // The sign-bit mask must be exact on -0.0: `0.0 - x` would answer +0.0.
    Case {
        name: "flip(0.0) == -0.0 (sign bit, not subtraction)",
        func: "flip",
        args: &[0.0],
        want_bits: 0x8000_0000,
    },
];

/// Compile [`SOURCE`] in f32 mode on device and run [`CASES`].
pub fn run(report: &mut Report) {
    println!("[f32-soft] --- end-to-end f32 shader (on-device JIT) ---");

    let mut table = BuiltinTable::new();
    table.populate();
    println!("[f32-soft] builtin table: {} symbols", table.len());

    let options = NativeCompileOptions {
        float_mode: lpir::FloatMode::F32,
        ..Default::default()
    };
    let engine = NativeJitEngine::new(Arc::new(table), options);

    let output = match lps_glsl::compile(SOURCE, &lps_glsl::CompileOptions::default()) {
        Ok(o) => o,
        Err(d) => {
            report.record("glsl compiles", false, 0, 0);
            println!("[f32-soft] frontend error: {}", d.render(SOURCE));
            return;
        }
    };
    report.record("glsl compiles", true, 0, 0);

    let module = match engine.compile(&output.ir, &output.meta) {
        Ok(m) => m,
        Err(e) => {
            report.record("f32 backend compiles", false, 0, 0);
            println!("[f32-soft] backend error: {e}");
            return;
        }
    };
    report.record("f32 backend compiles", true, 0, 0);
    if let Some(size) = module.code_size_bytes() {
        println!("[f32-soft] jit code size: {size} bytes");
    }

    let mut inst = match module.instantiate() {
        Ok(i) => i,
        Err(e) => {
            report.record("instantiates", false, 0, 0);
            println!("[f32-soft] instantiate error: {e}");
            return;
        }
    };
    report.record("instantiates", true, 0, 0);

    for case in CASES {
        let args: alloc::vec::Vec<LpsValueF32> =
            case.args.iter().map(|a| LpsValueF32::F32(*a)).collect();
        match inst.call(case.func, &args) {
            Ok(LpsValueF32::F32(got)) => {
                report.record(
                    case.name,
                    got.to_bits() == case.want_bits,
                    got.to_bits(),
                    case.want_bits,
                );
            }
            Ok(other) => {
                println!(
                    "[f32-soft] FAIL {}: unexpected return shape {other:?}",
                    case.name
                );
                report.record(case.name, false, 0, case.want_bits);
            }
            Err(e) => {
                println!("[f32-soft] FAIL {}: call error: {e}", case.name);
                report.record(case.name, false, 0, case.want_bits);
            }
        }
    }

    // int <-> float round trips, whose returns are not `float`.
    match inst.call("from_int", &[LpsValueF32::I32(16_777_217)]) {
        Ok(LpsValueF32::F32(got)) => report.record(
            "from_int(2^24+1) rounds to 2^24",
            got.to_bits() == 0x4B80_0000,
            got.to_bits(),
            0x4B80_0000,
        ),
        other => {
            println!("[f32-soft] FAIL from_int: {other:?}");
            report.record("from_int(2^24+1) rounds to 2^24", false, 0, 0x4B80_0000);
        }
    }
    match inst.call("to_int", &[LpsValueF32::F32(1_000_000.5)]) {
        Ok(LpsValueF32::I32(got)) => report.record(
            "to_int(1000000.5) truncates to 1000000",
            got == 1_000_000,
            got as u32,
            1_000_000,
        ),
        other => {
            println!("[f32-soft] FAIL to_int: {other:?}");
            report.record(
                "to_int(1000000.5) truncates to 1000000",
                false,
                0,
                1_000_000,
            );
        }
    }
}
