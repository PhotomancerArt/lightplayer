//! Runs the Xtensa hardware-risk corpus through the on-device JIT.
//!
//! The first execution of LightPlayer-compiled code on Xtensa silicon. Every
//! case comes from [`lpvm_native::xt_corpus`], shared verbatim with the host
//! golden test (`lpvm-native/tests/xt_corpus_goldens.rs`) so the two cannot
//! drift — a mismatch here is a real emulator-vs-silicon difference, not two
//! harnesses disagreeing.
//!
//! Goldens are committed constants, already confirmed on `lp-xt-emu` **and**
//! on rv32 before this ever ran. A failure is therefore a finding to triage,
//! never a reason to touch a golden. Triage key is in the milestone file; the
//! short version:
//!
//! - wrong value with many call args / high register pressure → known #194
//!   spilling cluster, not a hardware finding
//! - `EXC_INTEGER_DIVIDE_BY_ZERO` (cause 6) → known divide-by-zero cluster
//! - `EXC_LOAD_STORE_ERROR` (cause 3) → **NOT** a known issue as of #194
//! - anything on `deep_call_chain_20` → genuinely uncovered ground; the GLSL
//!   corpus cannot reach depth >16, so this is a new finding either way
//! - `EXC_COPROCESSOR0_DISABLED` (cause 32) on an `f32_*` case → the FPU is not
//!   armed for this context. Not a compiler finding: see [`fpu::arm`] and M7 D6.
//!
//! # Two corpora, two engines, two entry points
//!
//! `float-f32` adds the f32 half. It is a separate table and a separate call
//! because a *word* means something else in `FloatMode::F32` — there, a word is
//! an IEEE-754 bit pattern rather than Q16.16 — and both runtime entry points
//! refuse the other mode outright so a mix-up is an error rather than a
//! plausible wrong number.

use alloc::sync::Arc;

use esp_println::println;
use lpir::FloatMode;
use lpvm::{LpvmCompileParams, LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::native_options::NativeCompileOptions;
use lpvm_native::rt_jit::{BuiltinTable, NativeJitEngine};
use lpvm_native::xt_corpus::CASES;

use crate::board::esp32s3::{cycle_counter, fpu};

/// Marker every result line carries, so a transcript can be grepped.
const TAG: &str = "[XT-JIT]";

/// Running tallies, threaded through both corpus loops so the final
/// `RESULT passed=N failed=M` line covers everything that ran.
struct Tally {
    passed: u32,
    failed: u32,
}

fn engine_for(mode: FloatMode) -> NativeJitEngine {
    // Device builtins are compiled into this firmware and resolved by address
    // — no builtins image, unlike the host emulator path.
    let mut table = BuiltinTable::new();
    table.populate();
    let options = NativeCompileOptions {
        float_mode: mode,
        ..Default::default()
    };
    NativeJitEngine::new(Arc::new(table), options)
}

pub fn run_all() -> ! {
    cycle_counter::setup();

    // Arm coprocessor 0 before any compiled float code runs (M7 D6). Printed
    // rather than assumed: M6-P1 measured this silicon arriving with every
    // coprocessor already enabled under the esp-hal boot chain, but the write's
    // provenance is unpinned, so the transcript should say what was actually
    // true on the board that produced it.
    let cpenable = fpu::arm();
    println!("{TAG} cpenable after arming = {cpenable:#010x}");

    println!("{TAG} corpus start: {} cases", CASES.len());

    let mut tally = Tally {
        passed: 0,
        failed: 0,
    };

    run_q32_cases(&mut tally);
    #[cfg(feature = "float-f32")]
    run_f32_cases(&mut tally);
    #[cfg(feature = "float-f32")]
    run_per_compile_float_mode(&mut tally);

    println!("{TAG} ==================================================");
    println!(
        "{TAG} RESULT passed={} failed={}",
        tally.passed, tally.failed
    );
    if tally.failed == 0 {
        // Deliberately not celebratory: the corpus is a ceiling on what can be
        // found, and the milestone treats a clean sweep as suspicious until the
        // hard cases are shown to have really happened.
        println!("{TAG} all cases matched their goldens — verify the hard cases actually occurred");
    }
    println!("{TAG} corpus done");

    loop {
        core::hint::spin_loop();
    }
}

fn run_q32_cases(tally: &mut Tally) {
    let engine = engine_for(FloatMode::Q32);
    println!("{TAG} --- Q32 corpus: {} cases ---", CASES.len());

    for case in CASES {
        let (ir, sig) = (case.build)();

        let t0 = cycle_counter::read();
        let module = match engine.compile(&ir, &sig) {
            Ok(m) => m,
            Err(e) => {
                println!("{TAG} FAIL {} — compile failed: {e}", case.name);
                tally.failed += 1;
                continue;
            }
        };
        let compile_cycles = cycle_counter::read().wrapping_sub(t0);
        println!(
            "{TAG} {} compiled in {} us (risk {})",
            case.name,
            cycle_counter::cycles_to_us(compile_cycles as u64),
            case.risk,
        );

        for (i, inv) in case.invocations.iter().enumerate() {
            let mut inst = match module.instantiate() {
                Ok(inst) => inst,
                Err(e) => {
                    println!("{TAG} FAIL {}#{i} — instantiate failed: {e}", case.name);
                    tally.failed += 1;
                    continue;
                }
            };
            match inst.call_q32(case.entry, inv.args) {
                Ok(got) => {
                    if got.as_slice() == inv.golden {
                        println!(
                            "{TAG} PASS {}#{i} args={:?} -> {:?}",
                            case.name, inv.args, got
                        );
                        tally.passed += 1;
                    } else {
                        // Do NOT adjust the golden to match. See the module doc.
                        println!(
                            "{TAG} FAIL {}#{i} args={:?} expected={:?} got={:?}",
                            case.name, inv.args, inv.golden, got
                        );
                        tally.failed += 1;
                    }
                }
                Err(e) => {
                    // A trap is as much a result as a wrong value — and for the
                    // exec-alias case specifically, a fetch fault IS the bug.
                    println!(
                        "{TAG} FAIL {}#{i} args={:?} trapped: {e}",
                        case.name, inv.args
                    );
                    tally.failed += 1;
                }
            }
        }
    }
}

/// The f32 half. Values print as hex bit patterns, not as decimals: a wrong
/// answer here is usually wrong in a *structured* way — a swapped operand, a
/// sign bit, an exponent off by one, a whole word that never got written — and
/// hex shows that where a rounded decimal hides it.
#[cfg(feature = "float-f32")]
fn run_f32_cases(tally: &mut Tally) {
    use lpvm_native::xt_corpus::F32_CASES;

    let engine = engine_for(FloatMode::F32);
    println!("{TAG} --- f32 corpus: {} cases ---", F32_CASES.len());

    for case in F32_CASES {
        let (ir, sig) = (case.build)();

        let t0 = cycle_counter::read();
        let module = match engine.compile(&ir, &sig) {
            Ok(m) => m,
            Err(e) => {
                println!("{TAG} FAIL {} — compile failed: {e}", case.name);
                tally.failed += 1;
                continue;
            }
        };
        let compile_cycles = cycle_counter::read().wrapping_sub(t0);
        println!(
            "{TAG} {} compiled in {} us (risk {})",
            case.name,
            cycle_counter::cycles_to_us(compile_cycles as u64),
            case.risk,
        );

        for (i, inv) in case.invocations.iter().enumerate() {
            let mut inst = match module.instantiate() {
                Ok(inst) => inst,
                Err(e) => {
                    println!("{TAG} FAIL {}#{i} — instantiate failed: {e}", case.name);
                    tally.failed += 1;
                    continue;
                }
            };
            match inst.call_f32_words(case.entry, inv.args) {
                Ok(got) => {
                    if got.as_slice() == inv.golden {
                        println!(
                            "{TAG} PASS {}#{i} args={:08X?} -> {:08X?}",
                            case.name, inv.args, got
                        );
                        tally.passed += 1;
                    } else {
                        // Do NOT adjust the golden to match. See the module doc.
                        println!(
                            "{TAG} FAIL {}#{i} args={:08X?} expected={:08X?} got={:08X?}",
                            case.name, inv.args, inv.golden, got
                        );
                        tally.failed += 1;
                    }
                }
                Err(e) => {
                    // Cause 32 here means the FPU is not armed for this context,
                    // which is a firmware finding rather than a compiler one.
                    println!(
                        "{TAG} FAIL {}#{i} args={:08X?} trapped: {e}",
                        case.name, inv.args
                    );
                    tally.failed += 1;
                }
            }
        }
    }
}

/// The app's engine, asked for Float **per compile**.
///
/// The two corpora above each build an engine already set to the mode they
/// want. The app does not: `TargetLpvmGraphics::new` constructs exactly one
/// engine at boot, with `NativeCompileOptions::default()` — Q32 — and every
/// project that runs on that boot compiles through it. So the mode a shader
/// gets has to arrive with the *compile*, from its authored `float_mode` slot
/// (`docs/adr/2026-08-01-float-mode-reaches-the-device.md`).
///
/// This is the only place that path executes on silicon, and its failure mode
/// is the reason it is worth a section: if `LpvmCompileParams::float_mode`
/// stopped reaching `NativeCompileOptions`, nothing would error. The engine
/// would compile Q32, the module would run, and every number would be wrong in
/// a way that looks like a shader bug. So this asserts the *disclosed*
/// implementation, not just the value — `HardwareF32` is the module saying it
/// emitted FP instructions, and a Q32 compile cannot say that.
#[cfg(feature = "float-f32")]
fn run_per_compile_float_mode(tally: &mut Tally) {
    use lpvm::FloatImpl;
    use lpvm_native::xt_corpus::F32_CASES;

    println!("{TAG} --- per-compile float mode (Q32-constructed engine) ---");

    // Constructed exactly as the app constructs it: default options, Q32.
    let engine = engine_for(FloatMode::Q32);

    if !engine.supports_float_mode(FloatMode::F32) {
        println!(
            "{TAG} FAIL per-compile — this image linked float-f32 but the engine \
             says it cannot compile F32"
        );
        tally.failed += 1;
        return;
    }

    let params = LpvmCompileParams {
        config: Default::default(),
        float_mode: FloatMode::F32,
    };

    for case in F32_CASES {
        let (ir, sig) = (case.build)();
        let module = match engine.compile_with_params(&ir, &sig, &params) {
            Ok(m) => m,
            Err(e) => {
                println!("{TAG} FAIL per-compile {} — compile failed: {e}", case.name);
                tally.failed += 1;
                continue;
            }
        };

        // The disclosure check. A Q32 module reports `Fixed` on every target,
        // so this distinguishes "compiled in the mode I asked for" from
        // "compiled, ran, and quietly used the engine's own mode".
        let impl_reported = module.float_impl();
        if impl_reported != FloatImpl::HardwareF32 {
            println!(
                "{TAG} FAIL per-compile {} — module reports {impl_reported:?}, \
                 expected HardwareF32",
                case.name
            );
            tally.failed += 1;
            continue;
        }

        for (i, inv) in case.invocations.iter().enumerate() {
            let mut inst = match module.instantiate() {
                Ok(inst) => inst,
                Err(e) => {
                    println!(
                        "{TAG} FAIL per-compile {}#{i} — instantiate failed: {e}",
                        case.name
                    );
                    tally.failed += 1;
                    continue;
                }
            };
            match inst.call_f32_words(case.entry, inv.args) {
                Ok(got) if got.as_slice() == inv.golden => {
                    println!(
                        "{TAG} PASS per-compile {}#{i} args={:08X?} -> {:08X?}",
                        case.name, inv.args, got
                    );
                    tally.passed += 1;
                }
                Ok(got) => {
                    // Same goldens as the f32 corpus above, on purpose: a
                    // disagreement between the two sections would mean the
                    // per-compile route emits different code from the
                    // per-engine one, which is exactly what must not happen.
                    println!(
                        "{TAG} FAIL per-compile {}#{i} args={:08X?} expected={:08X?} got={:08X?}",
                        case.name, inv.args, inv.golden, got
                    );
                    tally.failed += 1;
                }
                Err(e) => {
                    println!(
                        "{TAG} FAIL per-compile {}#{i} args={:08X?} trapped: {e}",
                        case.name, inv.args
                    );
                    tally.failed += 1;
                }
            }
        }
    }
}
