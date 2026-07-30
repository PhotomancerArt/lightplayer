//! Confirms the Xtensa hardware-risk corpus' committed goldens on `lp-xt-emu`.
//!
//! This is the **oracle half** of M1. The ESP32-S3 harness runs the exact same
//! [`lpvm_native::xt_corpus::CASES`] on silicon and compares against the exact
//! same goldens; this test proves those goldens describe what the compiler is
//! supposed to produce, independently of any hardware.
//!
//! Ordering matters and is the whole point: goldens are committed constants,
//! confirmed here, **before** anything is flashed. If a device run disagrees,
//! that is a finding to triage — never a reason to edit a golden. Refreshing a
//! golden from device output would turn the hardware comparison into a
//! tautology that passes forever.
//!
//! Both sides reach the code through the identical
//! `LpvmEngine`/`LpvmModule`/`LpvmInstance` path, so a device mismatch is a
//! real emulator-vs-silicon difference and not an artefact of two harnesses.

#![cfg(feature = "emu-xt")]

use lpir::FloatMode;
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;
use lpvm_native::rt_emu::NativeEmuEngine;
use lpvm_native::xt_corpus::{CASES, XtCase};

fn engine() -> NativeEmuEngine {
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    NativeEmuEngine::new_for_isa(opts, IsaTarget::Xtensa)
}

/// Run every invocation of one case, returning `(checked, skipped_reason)`.
fn check_case(case: &XtCase) -> usize {
    let (ir, sig) = (case.build)();
    let engine = engine();
    let module = engine
        .compile(&ir, &sig)
        .unwrap_or_else(|e| panic!("[{}] xtensa compile failed: {e}", case.name));

    let mut checked = 0;
    for (i, inv) in case.invocations.iter().enumerate() {
        let mut inst = module
            .instantiate()
            .unwrap_or_else(|e| panic!("[{}] instantiate failed: {e}", case.name));
        let got = inst
            .call_q32(case.entry, inv.args)
            .unwrap_or_else(|e| panic!("[{}] invocation {i} trapped: {e}", case.name));
        assert_eq!(
            got, inv.golden,
            "[{}] invocation {i} (risk {}) args={:?}: emulator disagrees with the committed golden.\n\
             This is an EMITTER or EMULATOR bug — do not edit the golden to match.",
            case.name, case.risk, inv.args,
        );
        checked += 1;
    }
    checked
}

#[test]
fn every_corpus_golden_holds_on_the_emulator() {
    // The image gates EVERY case, not just the builtin ones: `rt_emu` links
    // all shader code against the builtins base image, so there is no
    // builtins-free path on the host (the same property the rv32 engine has).
    // `XtCase::needs_builtins` still marks which cases *call* a builtin, which
    // is what the device side cares about — on hardware, builtins come from
    // `lps_builtins::jit_builtin_code_ptr` and need no image at all.
    if !lps_builtins_xt_image::is_available() {
        eprintln!(
            "SKIP: Xtensa builtins image absent — cannot confirm goldens. Build with {} (needs the esp toolchain)",
            lps_builtins_xt_image::BUILD_COMMAND
        );
        return;
    }

    let mut checked = 0;
    for case in CASES {
        checked += check_case(case);
    }

    eprintln!("xt corpus: {checked} invocations confirmed on lp-xt-emu");
    assert!(checked > 0, "no corpus invocations ran");
}

/// Every case must also pass on **rv32**, which proves the hand-built LPIR
/// modules are well-formed rather than accidentally Xtensa-shaped.
///
/// Method note from the filetest work (PR #194): a hand-built module that
/// fails on both ISAs is an *invalid module*, not a finding — and it reads as
/// an engine bug to anyone who does not check. Their own first repro was
/// missing `VMCTX_VREG` from the call argument list and briefly looked like
/// "no Xtensa bug".
///
/// The goldens are ISA-independent by construction (they describe what the
/// LPIR means), so rv32 must produce exactly the same values. If a case ever
/// disagrees *between* ISAs, that is the interesting case: the shared
/// middle-end is `config-masked-defect` territory, where rv32 can be correct
/// only because its argument registers and allocatable pool happen to be
/// disjoint while Xtensa's overlap.
#[test]
fn every_corpus_golden_also_holds_on_rv32() {
    // Deliberately NOT a skip. The Xtensa image legitimately skips — it is a
    // cross-target artifact needing the esp toolchain. The rv32 image is a
    // plain dev artifact that `just test` already builds (`test:
    // build-rv32-builtins`), so an absent one means the harness was invoked
    // outside the normal flow. Skipping there would silently delete this
    // differential check in exactly the environment that needs it most: CI.
    //
    // (#194's author hit the same edge from the other side — their
    // `validate-xtensa` job built only the Xtensa image and every rv32-arm
    // test failed. Invisible locally, because a dev machine always has both.)
    assert!(
        lps_builtins_emu_image_available(),
        "rv32 builtins base image missing — run `just build-rv32-builtins` \
         (or use `just test`, which depends on it). NOT skipping: that would \
         remove the cross-ISA check precisely where it matters."
    );
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    let mut checked = 0;
    for case in CASES {
        let (ir, sig) = (case.build)();
        let engine = NativeEmuEngine::new_for_isa(opts.clone(), IsaTarget::Rv32imac);
        let module = engine
            .compile(&ir, &sig)
            .unwrap_or_else(|e| panic!("[{}] rv32 compile failed: {e}", case.name));
        for (i, inv) in case.invocations.iter().enumerate() {
            let mut inst = module.instantiate().expect("rv32 instantiate");
            let got = inst
                .call_q32(case.entry, inv.args)
                .unwrap_or_else(|e| panic!("[{}] rv32 invocation {i} trapped: {e}", case.name));
            assert_eq!(
                got, inv.golden,
                "[{}] invocation {i}: rv32 disagrees with the committed golden. \
                 The module is probably malformed (both ISAs would then be wrong), \
                 OR the goldens are not ISA-independent.",
                case.name
            );
            checked += 1;
        }
    }
    eprintln!("xt corpus: {checked} invocations also confirmed on rv32");
}

/// The rv32 emulation path links against its own builtins base image, built by
/// `scripts/build-builtins.sh`. Absent, compilation fails rather than skipping,
/// so probe cheaply first.
fn lps_builtins_emu_image_available() -> bool {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/riscv32imac-unknown-none-elf/release/lps-builtins-emu-app")
        .exists()
}

/// The corpus must actually cover the risk surface M1 enumerated. Without this,
/// a case could be quietly dropped and the transcript would still look clean.
#[test]
fn corpus_covers_every_enumerated_risk() {
    let mut seen: Vec<&str> = CASES
        .iter()
        .map(|c| c.risk.split(':').next().unwrap_or(""))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen,
        ["1", "2", "3", "4", "5", "6", "7"],
        "corpus must cover all 7 enumerated risk-surface items; got {seen:?}"
    );
}

/// Names are how a hardware transcript is read. Duplicates would make a
/// PASS/FAIL line ambiguous about which case it refers to.
#[test]
fn case_names_are_unique() {
    let mut names: Vec<&str> = CASES.iter().map(|c| c.name).collect();
    let before = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "duplicate case name in the corpus");
}
