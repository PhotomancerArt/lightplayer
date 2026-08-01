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

use alloc::sync::Arc;

use esp_println::println;
use lpir::FloatMode;
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::native_options::NativeCompileOptions;
use lpvm_native::rt_jit::{BuiltinTable, NativeJitEngine};
use lpvm_native::xt_corpus::CASES;

use crate::board::esp32s3::cycle_counter;

/// Marker every result line carries, so a transcript can be grepped.
const TAG: &str = "[XT-JIT]";

pub fn run_all() -> ! {
    cycle_counter::setup();

    println!("{TAG} corpus start: {} cases", CASES.len());

    // Device builtins are compiled into this firmware and resolved by address
    // — no builtins image, unlike the host emulator path.
    let mut table = BuiltinTable::new();
    table.populate();
    println!("{TAG} builtin table: {} symbols", table.len());

    let options = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    let engine = NativeJitEngine::new(Arc::new(table), options);

    let mut passed = 0u32;
    let mut failed = 0u32;

    for case in CASES {
        let (ir, sig) = (case.build)();

        let t0 = cycle_counter::read();
        let module = match engine.compile(&ir, &sig) {
            Ok(m) => m,
            Err(e) => {
                println!("{TAG} FAIL {} — compile failed: {e}", case.name);
                failed += 1;
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
                    failed += 1;
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
                        passed += 1;
                    } else {
                        // Do NOT adjust the golden to match. See the module doc.
                        println!(
                            "{TAG} FAIL {}#{i} args={:?} expected={:?} got={:?}",
                            case.name, inv.args, inv.golden, got
                        );
                        failed += 1;
                    }
                }
                Err(e) => {
                    // A trap is as much a result as a wrong value — and for the
                    // exec-alias case specifically, a fetch fault IS the bug.
                    println!(
                        "{TAG} FAIL {}#{i} args={:?} trapped: {e}",
                        case.name, inv.args
                    );
                    failed += 1;
                }
            }
        }
    }

    println!("{TAG} ==================================================");
    println!("{TAG} RESULT passed={passed} failed={failed}");
    if failed == 0 {
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
