//! The shipped `fw-esp32c6` image, direct-loaded and run.
//!
//! These are `#[ignore]`d. `cargo test --workspace` must not start a
//! cross-target firmware build (the director log's CI cost rule), and a test
//! that silently passed because it found no ELF would be worse than one that
//! is not run. `just test-emu-c6` sets the environment and runs them; see
//! `test_support` for the resolution order.
//!
//! What they pin is the P4 gate:
//!
//! 1. the boot reaches the documented sequence, and the first access nothing
//!    claims is reported with its block, its PC and a symbol;
//! 2. two runs produce byte-identical traces (PD5 determinism);
//! 3. snapshot, run on, restore, run again → the same trace.

use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp_common::Trace;

use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{fw_esp32c6_elf, skip_notice, SHIPPED_FEATURES};

/// LP_AON's reset-cause register — the first MMIO access of every boot,
/// from inside the ROM's `rtc_get_reset_reason`.
const LP_AON_RESET_CAUSE: u32 = 0x600B_0410;
/// `INTERRUPT_CORE0.core_0_intr_map[0]`. `_setup_interrupts` writes 31
/// (`DISABLED`) to all 77 of them, ending at `+0x130`.
const INTR_MAP_BASE: u32 = 0x6001_0000;
const INTR_MAP_LAST: u32 = 0x6001_0130;
/// `LP_APM.func_ctrl`, the access the brief predicted `esp_hal::init` would
/// reach.
const LP_APM_FUNC_CTRL: u32 = 0x600B_38C4;

/// Build a machine on the shipped image, or `None` with a printed reason.
fn shipped(strict: bool, buf: &SharedBuffer) -> Option<Esp32C6Machine> {
    let elf = match fw_esp32c6_elf(SHIPPED_FEATURES) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("boot", &reason);
            return None;
        }
    };
    Some(
        Esp32C6Builder::new()
            .app(AppSource::Path(elf))
            .strict(strict)
            .trace(Box::new(buf.clone()), Vec::new())
            .build()
            .expect("the shipped image builds a machine"),
    )
}

fn run_for(machine: &mut Esp32C6Machine, micros: u64) -> Outcome {
    machine.run_until(&StopCondition::after_micros(micros))
}

/// The index of the first trace line containing `needle`.
fn line_with(lines: &[String], needle: &str) -> usize {
    lines
        .iter()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no trace line contains `{needle}`"))
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_first_access_nothing_claims_stops_a_strict_run_and_names_itself() {
    let buf = SharedBuffer::new();
    let Some(mut m) = shipped(true, &buf) else {
        return;
    };

    let outcome = run_for(&mut m, 100_000);
    let Outcome::StrictBus { violation } = outcome else {
        panic!("expected a strict-bus stop, got {outcome:?}");
    };

    // The ROM's own reset-cause read, before `.bss` is even zeroed.
    assert_eq!(violation.address, LP_AON_RESET_CAUSE);
    assert_eq!(violation.pc, 0x4001_9684);
    assert_eq!(
        m.symbolize(violation.pc).as_deref(),
        Some("rtc_get_reset_reason+0x4"),
        "the fault names the routine, not just an address"
    );
    assert!(
        violation.in_mmio_window,
        "and says it is an unmodelled block rather than a wild pointer"
    );
    assert_eq!(outcome.exit_code(), 3);

    // Strict mode stops promptly instead of spinning in `_default_abort` to
    // the end of the budget.
    assert!(
        m.cycles() < 100_000,
        "stopped at {} cycles, which is not prompt",
        m.cycles()
    );
    assert_eq!(m.bus.unmapped_reads(), 1, "and refused exactly one access");
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_documented_boot_sequence_appears_in_the_trace_in_order() {
    // Not strict: with unmapped reads answering 0 the boot carries on, which
    // is how far the machine gets before P5 models a single block. Every
    // step the phase brief names is here, in the order it names them.
    let buf = SharedBuffer::new();
    let Some(mut m) = shipped(false, &buf) else {
        return;
    };

    let outcome = run_for(&mut m, 10_000);
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the machine survived 10 ms of emulated time: {outcome:?}"
    );

    let lines = buf.lines();
    let rom_read = line_with(&lines, &format!("R4 UNMAPPED+{LP_AON_RESET_CAUSE:#010x}"));
    assert_eq!(rom_read, 0, "the ROM's reset-cause read comes first of all");

    let first_map = line_with(&lines, &format!("W4 UNMAPPED+{INTR_MAP_BASE:#010x}"));
    let last_map = line_with(&lines, &format!("W4 UNMAPPED+{INTR_MAP_LAST:#010x}"));
    assert!(rom_read < first_map);
    assert_eq!(
        last_map - first_map + 1,
        77,
        "`_setup_interrupts` writes all 77 core_0_intr_map entries"
    );
    for line in &lines[first_map..=last_map] {
        assert!(
            line.ends_with("= 0x0000001f [unmapped,dropped]"),
            "every one of them is 31 (DISABLED): {line}"
        );
    }

    // Then PLIC_MX, in the low MMIO window: `init_vectoring`'s per-interrupt
    // kind/priority/enable writes.
    let plic = lines
        .iter()
        .position(|l| l.contains("UNMAPPED+0x2000"))
        .expect("PLIC_MX is reached");
    assert!(last_map < plic, "the map writes come before the PLIC setup");

    // And then `esp_hal::init` proper, exactly where the brief predicted.
    let apm = line_with(&lines, &format!("+{LP_APM_FUNC_CTRL:#010x}"));
    assert!(plic < apm);
    assert_eq!(
        m.symbolize(0x4001_9684).as_deref(),
        Some("rtc_get_reset_reason+0x4")
    );

    // Every line is stamped with the instruction that made it, not with the
    // slice it happened to fall in.
    assert!(lines[0].starts_with("cyc=29 pc=0x40019684 "), "{}", lines[0]);
    assert!(
        lines[first_map].contains("pc=0x420971e0"),
        "{}",
        lines[first_map]
    );
    let cycle_of = |line: &str| -> u64 {
        line.split_whitespace().next().unwrap()["cyc=".len()..]
            .parse()
            .unwrap()
    };
    assert!(
        cycle_of(&lines[first_map]) > cycle_of(&lines[0]),
        "cycles advance across the trace"
    );
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn two_runs_of_the_same_image_produce_byte_identical_traces() {
    // Plan PD5: wall clock never enters the machine, so this is not a
    // "usually" — it is the property the whole time model rests on.
    let one = SharedBuffer::new();
    let two = SharedBuffer::new();
    let (Some(mut a), Some(mut b)) = (shipped(false, &one), shipped(false, &two)) else {
        return;
    };

    let outcome_a = run_for(&mut a, 4_000);
    let outcome_b = run_for(&mut b, 4_000);

    assert_eq!(outcome_a, outcome_b);
    assert_eq!(a.cycles(), b.cycles());
    assert_eq!(a.instructions(), b.instructions());
    assert_eq!(a.bus.unmapped_reads(), b.bus.unmapped_reads());
    assert_eq!(
        one.contents(),
        two.contents(),
        "two runs of the same image diverged"
    );
    assert!(!one.contents().is_empty(), "and the trace is not empty");
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn a_restored_snapshot_replays_the_same_trace() {
    // The brief's shape: run to N, snapshot, run to M, restore, run to M
    // again, and require the two stretches to be identical. Anything the
    // snapshot forgot shows up here as a diverging line, because the trace
    // is a function of every access the machine makes.
    let start = SharedBuffer::new();
    let Some(mut m) = shipped(false, &start) else {
        return;
    };

    const N_US: u64 = 1_500;
    const M_US: u64 = 4_000;

    run_for(&mut m, N_US);
    let at_n = m.cycles();
    assert!(at_n >= N_US * memmap::CYCLES_PER_US);
    let snap = m.snapshot();
    assert_eq!(snap.cycle(), at_n);

    let first = SharedBuffer::new();
    m.bus.trace = Trace::to_sink(Box::new(first.clone()));
    run_for(&mut m, M_US);
    let at_m = m.cycles();
    let unmapped_at_m = m.bus.unmapped_reads();

    m.restore(&snap);
    assert_eq!(m.cycles(), at_n, "the machine went back");

    let second = SharedBuffer::new();
    m.bus.trace = Trace::to_sink(Box::new(second.clone()));
    run_for(&mut m, M_US);

    assert_eq!(m.cycles(), at_m);
    assert_eq!(m.bus.unmapped_reads(), unmapped_at_m);
    assert_eq!(
        first.contents(),
        second.contents(),
        "the replay after restore diverged from the first run"
    );
    assert!(!first.contents().is_empty());
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_shipped_image_loads_where_the_memory_map_says() {
    let buf = SharedBuffer::new();
    let Some(m) = shipped(false, &buf) else {
        return;
    };

    // The ROM first, then the app on top of it.
    assert_eq!(m.rom_segments().len(), 4);
    assert_eq!(m.app_segments().len(), 7);

    let app = m.app().expect("an app was loaded");
    assert_eq!(
        m.symbolize(app.entry).as_deref(),
        Some("_start"),
        "the entry point is `_start`, inside a placed segment"
    );

    // The three regions the app spans, and nothing outside them.
    for seg in m.app_segments() {
        assert!(
            seg.regions
                .iter()
                .all(|r| matches!(*r, "hp-sram" | "flash-cache" | "lp-sram")),
            "segment at {:#010x} landed in {:?}",
            seg.vaddr,
            seg.regions
        );
    }

    // `dram2_seg` is the second heap region and it is plain zeroed RAM: the
    // bootloader's loader segment is gone, which is the whole point.
    let mut m = m;
    assert_eq!(m.peek_word(memmap::DRAM2_BASE), Some(0));
    assert_eq!(m.peek_word(memmap::DRAM2_END - 4), Some(0));
    // The stack top is where the linker script puts it.
    assert_eq!(memmap::APP_RAM_END, 0x4086_E610);
}
