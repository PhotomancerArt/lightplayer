//! M7 P7: the translated core as the **default**, and what that must not
//! change.
//!
//! R9's core claim is that the wasm build's engine is the translator and the
//! interpreter has two other jobs — the free differential oracle and the
//! `step_one` escape hatch. Flipping a default is the cheap half of that; the
//! expensive half is proving that a core which now appears *without being
//! asked for* still appears only where it may, disappears wherever it must,
//! and changes nothing a transcript can see.
//!
//! Five rules, one test each:
//!
//! 1. **The policy is the target family.** `TRANSLATED_BY_DEFAULT` is true in
//!    the wasm build and false natively (JD9, and JD24 deferred the P8 that
//!    would change the native half).
//! 2. **A run that asked for nothing installs one**, and says so in one line.
//! 3. **`--interpreter` vetoes it.** That is the way back from the flip, and
//!    it has to beat the default rather than merely coexist with it (JD15).
//! 4. **`BootMode::RomUp` keeps translation off**, as M5 keeps the block cache
//!    off there and for the same reason: the mask ROM and the second-stage
//!    bootloader copy code into RAM and jump into it without ever emitting a
//!    `fence.i`, so neither of JD5's two events can see what they publish.
//! 5. **Translated code is not architectural state.** A `fence.i` retires the
//!    module holding the bytes that changed, a snapshot restore and a reboot
//!    drop the core entirely — and a run that does all three produces exactly
//!    what the interpreter produces, register for register and cycle for
//!    cycle.
//!
//! Natively the default *is* the interpreter, so rules 2–5 are exercised by
//! asking for precisely what the wasm build's default asks for. The whole-image
//! proof that the flip moved nothing is `scripts/emu/p6-oracle.sh` and
//! `scripts/emu/oracle-sweep.sh` against the pinned images; this is the part
//! that runs on every machine with no toolchain and no ELF.
#![cfg(any(feature = "jit", target_family = "wasm"))]

use lp_emu_esp32c6::machine::{
    BootMode, Esp32C6Builder, Esp32C6Machine, StopCondition, TRANSLATED_BY_DEFAULT,
};
use lp_emu_esp32c6::memmap;

const CODE: u32 = memmap::HP_SRAM_BASE + 0x1000;
const SUB: u32 = 0x200;
const EBREAK: u32 = 0x0010_0073;
const FENCE_I: u32 = 0x0000_100f;

/// Enough blocks for a constructed guest, few enough that cranelift answers in
/// milliseconds. The product bound is `DEFAULT_JIT_BLOCKS` (unbounded since
/// P5); a test that installed the whole mask ROM would be measuring the native
/// compiler, which is exactly what JD9 keeps off the default path.
const FEW_BLOCKS: usize = 64;

// --- the assembler, by hand, as `jit_discovery.rs` does it ------------------

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

fn lui(rd: u32, imm: u32) -> u32 {
    (imm << 12) | (rd << 7) | 0x37
}

fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (0x2 << 12) | ((imm & 0x1f) << 7) | 0x23
}

fn jal(rd: u32, offset: i32) -> u32 {
    let o = offset as u32;
    let imm = ((o >> 20) & 1) << 31
        | ((o >> 1) & 0x3ff) << 21
        | ((o >> 11) & 1) << 20
        | ((o >> 12) & 0xff) << 12;
    imm | (rd << 7) | 0x6f
}

fn li(rd: u32, value: u32) -> [u32; 2] {
    let upper = (value.wrapping_add(0x800)) >> 12;
    let lower = (value as i32) - ((upper << 12) as i32);
    [lui(rd, upper), addi(rd, rd, lower)]
}

fn place_words(m: &mut Esp32C6Machine, at: u32, words: &[u32]) {
    let mut bytes = Vec::new();
    for w in words {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    m.bus.load_image(at, &bytes).unwrap();
}

/// Registers, pc and counters, so two runs can be compared whole.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Outcome {
    regs: [i32; 32],
    pc: u32,
    cycles: u64,
    retired: u64,
}

fn outcome(m: &Esp32C6Machine) -> Outcome {
    let hart = &m.harts[0];
    Outcome {
        regs: *hart.regs(),
        pc: hart.pc(),
        cycles: hart.cycle_count(),
        retired: hart.instruction_count(),
    }
}

// --- 1. the policy ----------------------------------------------------------

#[test]
fn the_default_is_the_translator_in_the_wasm_build_and_the_interpreter_natively() {
    assert_eq!(
        TRANSLATED_BY_DEFAULT,
        cfg!(target_family = "wasm"),
        "M7 P7 flips the default in the WASM build only. Natively the \
         interpreter stays the default (JD9): wasmtime needs minutes of \
         cranelift over these images and every test and CI job depends on this \
         binary starting fast. JD24 deferred the P8 that would revisit it."
    );
}

// --- 2. a run that asked for nothing ----------------------------------------

#[test]
fn a_run_that_asked_for_nothing_installs_a_core_and_says_so_in_one_line() {
    // Exactly what the wasm build's default asks for, asked for explicitly so
    // a native test can exercise it: `.jit(true)` with nothing else said.
    let m = Esp32C6Builder::new()
        .jit(true)
        .jit_blocks(FEW_BLOCKS)
        .build()
        .unwrap();
    assert!(
        m.harts[0].has_translated_core(),
        "the default installs a core at boot — JD5's first of two events"
    );
    let line = m
        .translated_core_summary()
        .expect("a default run prints one line, with no --jit-report");
    for field in [
        "translated core:",
        "built in",
        "escape hatch",
        "--interpreter",
    ] {
        assert!(
            line.contains(field),
            "the default line must carry `{field}` — the boot cost is a product \
             number (JD20), the escape hatch is not allowed to be unmeasured \
             (JD10), and the way back has to be in the line that announces the \
             flip: {line}"
        );
    }
}

// --- 3. `--interpreter` beats the default -----------------------------------

#[test]
fn interpreter_vetoes_the_default_rather_than_coexisting_with_it() {
    let m = Esp32C6Builder::new()
        .jit(true)
        .jit_blocks(FEW_BLOCKS)
        .translate(false)
        .build()
        .unwrap();
    assert!(
        !m.harts[0].has_translated_core(),
        "`--interpreter` is the way back from P7's flip and the free oracle \
         every later change is judged against (JD15); a build that asked for \
         both must get the interpreter"
    );
    assert!(
        m.translated_core_summary().is_none(),
        "and it prints no translation line, because there is no translation"
    );
}

// --- 4. rom-up ---------------------------------------------------------------

#[test]
fn a_rom_up_boot_keeps_translation_off() {
    let m = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .jit(true)
        .jit_blocks(FEW_BLOCKS)
        .build()
        .unwrap();
    assert!(
        !m.harts[0].has_translated_core(),
        "the mask ROM and the second-stage bootloader copy code into RAM and \
         jump into it without ever emitting a `fence.i`, so neither of JD5's \
         two events can see what they publish — M5 turns the block cache off \
         here for the same reason"
    );
}

// --- 5. not architectural state ---------------------------------------------

/// A guest that rewrites a subroutine, publishes it with a `fence.i`, and calls
/// it again — so the run touches the second of JD5's two translation events and
/// a core that ignored it would give a different answer.
///
/// `t0` ends at 11: the subroutine adds 1 before the rewrite and 10 after.
fn image_that_rewrites_its_own_code() -> (Vec<u32>, Vec<u32>) {
    let mut main = Vec::new();
    main.push(jal(1, SUB as i32)); // +0   call the subroutine
    let patch = li(6, addi(5, 5, 10)); // the word it will be rewritten to
    let addr = li(7, CODE + SUB);
    main.extend_from_slice(&patch); // +4
    main.extend_from_slice(&addr); // +12
    main.push(sw(6, 7, 0)); // +20  publish the new instruction
    main.push(FENCE_I); // +24  and say so
    main.push(jal(1, (SUB - 28) as i32)); // +28  call it again
    main.push(EBREAK); // +32
    let sub = vec![addi(5, 5, 1), jal(0, 0).wrapping_add(0), 0];
    (main, sub)
}

/// The subroutine's return, spelled out: `jalr x0, x1, 0`.
fn ret() -> u32 {
    (1 << 15) | 0x67
}

fn run_with_a_restore_and_a_reboot(translated: bool) -> (Outcome, u64) {
    let (main, mut sub) = image_that_rewrites_its_own_code();
    sub[1] = ret();
    sub[2] = 0;

    let mut m = Esp32C6Builder::bare()
        .reboot_on_reset(true)
        .build()
        .unwrap();
    place_words(&mut m, CODE, &main);
    place_words(&mut m, CODE + SUB, &sub);
    m.harts[0].set_pc(CODE);
    if translated {
        m.translate_from_seeds(&[CODE, CODE + SUB], false, FEW_BLOCKS)
            .unwrap();
        assert!(m.harts[0].has_translated_core());
    }

    // Half the run, then a snapshot taken with a core installed and a restore
    // that has to put the machine back exactly as it was — minus the core,
    // which is not part of "as it was".
    m.run_until(&StopCondition::after_micros(20));
    if translated {
        assert!(
            m.translated_coverage()
                .is_some_and(|(covered, _)| covered > 0),
            "the translated leg has to have actually run inside translated \
             code, or the comparison below is two interpreter runs agreeing"
        );
    }
    let mid = m.snapshot();
    m.run_until(&StopCondition::after_micros(200));
    let ran_on = outcome(&m);

    m.restore(&mid);
    assert!(
        !m.harts[0].has_translated_core(),
        "a restore invalidates: a core holds host code compiled from guest \
         bytes the restored regions have just replaced"
    );
    m.run_until(&StopCondition::after_micros(200));
    assert_eq!(
        outcome(&m),
        ran_on,
        "and the restored run has to reach the same place the first one did"
    );

    let fences = m.harts[0].fence_i_count();
    assert!(
        m.reboot(lp_emu_esp_common::Strap::App),
        "a reboot was armed"
    );
    assert!(
        !m.harts[0].has_translated_core(),
        "a reboot goes back to the power-on snapshot, which never had a core"
    );
    (ran_on, fences)
}

#[test]
fn a_default_core_survives_a_fence_i_a_restore_and_a_reboot_without_being_seen() {
    let (interpreted, interpreted_fences) = run_with_a_restore_and_a_reboot(false);
    let (translated, translated_fences) = run_with_a_restore_and_a_reboot(true);
    assert_eq!(
        interpreted.regs[5], 11,
        "the guest runs the old instruction once and the published one once"
    );
    assert_eq!(
        interpreted_fences, 1,
        "the guest publishes exactly once, so JD5's second translation event \
         is on this run's path rather than merely available to it"
    );
    assert_eq!(
        translated, interpreted,
        "translated code is not architectural state: the same guest, through a \
         `fence.i`, a snapshot restore and a reboot, has to end at the same pc \
         with the same registers, the same cycle count and the same minstret"
    );
    assert_eq!(
        translated_fences, interpreted_fences,
        "and the same number of publishes reached the machine"
    );
}
