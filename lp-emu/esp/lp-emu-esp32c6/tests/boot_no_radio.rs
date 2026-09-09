//! The P5 gate: the no-radio image runs 3 s of emulated time through
//! `esp_hal::init`, esp-rtos, the tick, SWI0 context switches and `wfi`,
//! with every register access modelled or accepted, under `--strict-bus`.
//!
//! `#[ignore]`d: needs the firmware ELF (`just test-emu-c6` builds it, or
//! set `LP_EMU_C6_ELF_ESP32C6_SERVER_MEMORY_FS`). See `test_support`.
//!
//! # The image
//!
//! `--no-default-features --features esp32c6,server,memory_fs`. The brief
//! named `esp32c6,server`; that image reads flash at boot
//! (`bootctl::read_and_consume` → `esp-storage` → the ROM's
//! `esp_rom_spiflash_read`), which spins on `SPI1.cmd` until M4 models the
//! flash controller — pinned by [`the_flash_image_spins_on_spi1_cmd_which_is_m4s`].
//! `memory_fs` is the firmware's own "no flash" switch and is the P5 image.
//!
//! # What the gate items became
//!
//! Every item from the brief, with two adjusted to what the firmware does:
//!
//! 4. the tick is **TIMG0 T0**, not a SYSTIMER comparator
//!    (`esp_rtos::start(timg0.timer0, …)`, `init.rs:76`), and esp-rtos
//!    0.3.0's tick is one-shot, armed to the next wakeup — there is no
//!    periodic 10 ms tick, so gaps are what the firmware programmed (up to
//!    its longest sleep), which the test checks against the alarm values;
//! 9. `t2` reaches the same *write* sequence up to esp-rtos start with the
//!    cycle column stripped; after that, time-derived values (targets) and
//!    the tick/SWI interleaving legitimately differ between grades.

use std::collections::BTreeMap;

use lp_emu_esp_common::Trace;
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::intmatrix::Esp32C6IntMatrix;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::regs::source;
use lp_emu_esp32c6::rom::jal_target;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};

/// The brief's trace filter, plus TIMG0 because the tick lives there.
const BLOCKS: &[&str] = &[
    "SYSTIMER",
    "PLIC_MX",
    "INTPRI",
    "INTERRUPT_CORE0",
    "LP_WDT",
    "TIMG0",
];

const GATE_US: u64 = 3_000_000;

/// T0 ticks at XTAL / 2 = 20 MHz: 8 CPU cycles per tick.
const CYCLES_PER_T0_TICK: u64 = 8;

fn machine(image: &FwImage, grade: TimeGrade, buf: &SharedBuffer) -> Option<Esp32C6Machine> {
    let elf = match fw_esp32c6_image(image) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("boot_no_radio", &reason);
            return None;
        }
    };
    Some(
        Esp32C6Builder::new()
            .app(AppSource::Path(elf))
            .strict(true)
            .time_grade(grade)
            .trace(
                Box::new(buf.clone()),
                BLOCKS.iter().map(|s| s.to_string()).collect(),
            )
            .build()
            .expect("the no-radio image builds a machine"),
    )
}

fn cycle_of(line: &str) -> u64 {
    line.split_whitespace().next().unwrap()["cyc=".len()..]
        .parse()
        .unwrap()
}

fn value_of(line: &str) -> u32 {
    let hex = line.rsplit("= 0x").next().unwrap();
    u32::from_str_radix(&hex[..8], 16).unwrap()
}

fn symbol(m: &Esp32C6Machine, name: &str) -> u32 {
    m.app()
        .unwrap()
        .symbol(name)
        .unwrap_or_else(|| panic!("the image has no `{name}`"))
        .address
}

/// The one 3 s run every gate item reads.
struct GateRun {
    m: Esp32C6Machine,
    outcome: Outcome,
    lines: Vec<String>,
}

fn gate_run(grade: TimeGrade) -> Option<GateRun> {
    let buf = SharedBuffer::new();
    let mut m = machine(&FwImage::NO_RADIO, grade, &buf)?;
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    Some(GateRun {
        m,
        outcome,
        lines: buf.lines(),
    })
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_no_radio_image_runs_three_seconds_strict_to_the_idle_loop() {
    let Some(GateRun {
        mut m,
        outcome,
        lines,
    }) = gate_run(TimeGrade::T1)
    else {
        return;
    };

    // 1. No strict-bus fault, no hart fault, exit 0 at the timeout, and
    //    not one access nothing claimed.
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "expected the emulated deadline, got {outcome:?}"
    );
    assert_eq!(outcome.exit_code(), 0);
    assert_eq!(m.bus.unmapped_reads(), 0, "UNMAPPED reads");
    assert_eq!(m.bus.unmapped_writes(), 0, "UNMAPPED writes");
    assert!(!lines.iter().any(|l| l.contains("UNMAPPED")));
    assert_eq!(m.micros(), GATE_US);

    // 2. `esp_hal::init` completed: `syscall_table_ptr` holds `&SYSCALL_TABLE`
    //    (`esp-rom-sys/src/syscall/mod.rs:194-199`, the last step of init
    //    before the trap-section protection) …
    let table_ptr = m
        .peek_word(memmap::SYSCALL_TABLE_PTR)
        .expect("syscall_table_ptr is in HP SRAM");
    let table = m
        .app()
        .unwrap()
        .symbols()
        .iter()
        .find(|s| s.name.ends_with("SYSCALL_TABLE"))
        .expect("the image has esp-rom-sys's SYSCALL_TABLE")
        .address;
    assert_eq!(table_ptr, table, "syscall_table_ptr = &SYSCALL_TABLE");

    //    … and `enable_main_stack_guard_monitoring` armed trigger 0 on
    //    `__stack_chk_guard`: tselect=0, tcontrol=8 (mte), tdata1=0xC2
    //    (store|m|NAPOT), tdata2=(guard&!3)|1 — resolved from the ELF.
    let guard = symbol(&m, "__stack_chk_guard");
    let tdata2 = (guard & !3) | 1;
    // esp-hal writes tdata1 before tdata2, so the slot is armed at address
    // 0 for one instruction first (true on silicon too); the line that
    // matters is the one carrying the guard.
    let watchpoints: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("WATCHPOINT slot=0"))
        .collect();
    assert!(
        watchpoints
            .iter()
            .any(|l| l.ends_with(&format!("armed at {tdata2:#010x} napot store"))),
        "no arming at the guard ({guard:#010x}): {watchpoints:?}"
    );
    let wp = m.harts[0]
        .triggers()
        .watchpoint(0)
        .expect("still armed at the end");
    assert_eq!(wp.address, tdata2);
    assert!(wp.napot && wp.on_store && !wp.on_load && !wp.on_execute);
    assert_eq!(m.harts[0].triggers().tcontrol(), 0x8);
    assert_eq!(
        m.harts[0].triggers().tdata1() & 0x0000_0FFF,
        0xC2,
        "tdata1 reads back store|m|match=NAPOT"
    );
    // The guard itself is intact: nothing wrote through it.
    assert_eq!(
        m.peek_word(guard),
        Some(0xDEED_BAAD),
        "stack-guard-value default"
    );

    // 3. SWI0: `core_0_intr_map[22] = 1`, `MXINT_PRI[1] = 1`, enable bit 1,
    //    and the self-patched vector slot is `jal x0, swint_handler_trampoline`.
    let matrix = m
        .bus
        .matrix()
        .as_any()
        .downcast_ref::<Esp32C6IntMatrix>()
        .unwrap();
    assert_eq!(matrix.map(source::FROM_CPU_INTR0), Some(1));
    assert_eq!(matrix.priority(1), 1);
    assert_eq!(matrix.enable() & 0b10, 0b10);
    assert_eq!(matrix.kind() & 0b10, 0, "Level");
    // And the tick: TG0_T0_LEVEL → CPU interrupt 16 (Priority1).
    assert_eq!(matrix.map(source::TG0_T0_LEVEL), Some(16));
    assert_eq!(matrix.priority(16), 1);
    let slot = memmap::HP_SRAM_BASE + 4;
    let word = m.peek_word(slot).unwrap();
    assert_eq!(
        jal_target(word, slot),
        Some(symbol(&m, "swint_handler_trampoline")),
        "vector slot 1 ({slot:#010x}) = {word:#010x} is not a jal to the trampoline"
    );
    assert!(m.harts[0].csr().mtvec_vectored());
    assert_eq!(m.harts[0].csr().mtvec_base(), memmap::HP_SRAM_BASE);

    // 4. The tick, on TIMG0 T0: `timer_tick_handler` clears `int_clr.t0`
    //    and re-arms (alarm written, load pulsed, en+alarm_en set). At least
    //    250 tick interrupts in 3 s, and every gap between consecutive
    //    handler entries is a sleep the firmware programmed: no gap exceeds
    //    the longest alarm ever armed (plus the slice cap and the handler's
    //    own latency), and the typical gap is well under 10 ms.
    // Every vectored interrupt entry clears its CPU interrupt in
    // `handle_interrupts` (`riscv.rs:567`); SWI0 is direct-bound and never
    // passes there, and nothing else fires. So one `mxint_clear` write is
    // one tick interrupt taken.
    let int_clr_writes = lines
        .iter()
        .filter(|l| l.contains("W4 TIMG0+0x07c int_clr"))
        .count();
    let handler_entries: Vec<u64> = lines
        .iter()
        .filter(|l| l.contains("W4 PLIC_MX+0x008 mxint_clear = 0x00010000"))
        .map(|l| cycle_of(l))
        .collect();
    assert!(
        handler_entries.len() >= 250,
        "{} tick interrupts in 3 s",
        handler_entries.len()
    );
    // `timer_tick_handler` clears once and `schedule()` clears once more
    // when it re-arms: two `int_clr` writes per tick.
    assert!(
        int_clr_writes >= 2 * handler_entries.len() - 2,
        "{int_clr_writes} int_clr writes for {} ticks",
        handler_entries.len()
    );
    let pcs: BTreeMap<&str, usize> = lines
        .iter()
        .filter(|l| l.contains("W4 TIMG0+0x07c int_clr"))
        .fold(BTreeMap::new(), |mut m, l| {
            *m.entry(l.split_whitespace().nth(1).unwrap()).or_default() += 1;
            m
        });
    assert_eq!(pcs.len(), 1, "one `clear_interrupt` site: {pcs:?}");
    let mut gaps: Vec<u64> = handler_entries.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_unstable();
    let max_gap = *gaps.last().unwrap();
    let median_gap = gaps[gaps.len() / 2];
    let longest_alarm_ticks = lines
        .iter()
        .filter(|l| l.contains("W4 TIMG0+0x010 t0.alarmlo"))
        .map(|l| u64::from(value_of(l)))
        .max()
        .unwrap();
    assert!(
        lines
            .iter()
            .filter(|l| l.contains("W4 TIMG0+0x014 t0.alarmhi"))
            .all(|l| value_of(l) == 0),
        "no alarm needs the high word"
    );
    let latency_allowance = 8_192 + 20_000;
    assert!(
        max_gap <= longest_alarm_ticks * CYCLES_PER_T0_TICK + latency_allowance,
        "the longest gap ({} us) exceeds the longest sleep the firmware armed ({} us)",
        max_gap / memmap::CYCLES_PER_US,
        longest_alarm_ticks * CYCLES_PER_T0_TICK / memmap::CYCLES_PER_US
    );
    assert!(
        median_gap <= 10 * 1_000 * memmap::CYCLES_PER_US,
        "median gap {} us",
        median_gap / memmap::CYCLES_PER_US
    );
    // Every tick entered `handle_interrupts`: threshold raised to 2 and
    // restored to 1, the CPU interrupt cleared, the status words read.
    let thresh_up = lines
        .iter()
        .filter(|l| l.contains("W4 PLIC_MX+0x090 mxint_thresh = 0x00000002"))
        .count();
    let thresh_down = lines
        .iter()
        .filter(|l| l.contains("W4 PLIC_MX+0x090 mxint_thresh = 0x00000001"))
        .count();
    assert!(
        thresh_up >= handler_entries.len(),
        "{thresh_up} vs {}",
        handler_entries.len()
    );
    assert!(thresh_down >= handler_entries.len());
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("R4 PLIC_MX+0x008 mxint_clear") && !l.ends_with("= 0x00000000")),
        "mxint_clear must read 0"
    );

    // 5. `wfi` reached, and the tick kept firing across the idle skips.
    assert!(m.idle_skips() >= 250, "{} idle skips", m.idle_skips());
    let first_skip_ticks = handler_entries
        .iter()
        .filter(|c| **c > handler_entries[handler_entries.len() / 4])
        .count();
    assert!(
        first_skip_ticks >= 100,
        "ticks after the first quarter of the run"
    );
    assert!(
        m.instructions() < 100_000_000,
        "{} instructions for 480 M cycles — idle was not skipped",
        m.instructions()
    );

    // 6. The RWDT: armed with the 30 s boot timeout, tightened to 8 s at the
    //    first feed, fed at least once and never expired.
    let feeds = lines
        .iter()
        .filter(|l| l.contains("W4 LP_WDT+0x014 wdtfeed = 0x80000000"))
        .count();
    assert!(feeds >= 1, "the feeder never fed");
    let holds: Vec<u32> = lines
        .iter()
        .filter(|l| l.contains("W4 LP_WDT+0x004 wdtconfig1"))
        .map(|l| value_of(l))
        .collect();
    // 30 s and 8 s at RC_SLOW 136 kHz, >> 1 (`Rwdt::set_timeout`).
    assert_eq!(holds, [30 * 136_000 / 2 + 3, 8 * 136_000 / 2], "{holds:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("W4 LP_WDT+0x000 wdtconfig0 = 0xc007e214"))
    );
    assert!(!lines.iter().any(|l| l.contains("EXPIRED")));

    // Nothing spun except esp-println's one 50,000-iteration USB wait,
    // which is the documented "no host" behaviour and lands in the trace
    // despite the block filter.
    let spins: Vec<&String> = lines.iter().filter(|l| l.contains(" SPIN ")).collect();
    assert_eq!(spins.len(), 1, "{spins:?}");
    assert!(spins[0].contains("USB_DEVICE+0x004 ep1_conf"));
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn two_runs_produce_byte_identical_traces() {
    // 8. Determinism (plan PD5).
    let (Some(a), Some(b)) = (gate_run(TimeGrade::T1), gate_run(TimeGrade::T1)) else {
        return;
    };
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.m.idle_skips(), b.m.idle_skips());
    assert_eq!(a.lines.len(), b.lines.len());
    assert!(a.lines == b.lines, "two runs of the same image diverged");
    assert!(a.lines.len() > 100_000);
}

/// The write sequence up to esp-rtos's first SWI0 raise, cycle column
/// stripped, time-derived values masked.
fn boot_writes(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .take_while(|l| !l.contains("W4 INTPRI+0x090 cpu_intr_from_cpu0 = 0x00000001"))
        .filter(|l| {
            l.split_whitespace()
                .nth(2)
                .is_some_and(|t| t.starts_with('W'))
        })
        .map(|l| {
            let rest = l.split_once(' ').unwrap().1.to_string();
            if rest.contains("t0.alarm") || rest.contains("trgt") {
                rest.rsplit_once(" = ").unwrap().0.to_string() + " = <time>"
            } else {
                rest
            }
        })
        .collect()
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_t2_grade_reaches_esp_rtos_through_the_same_writes() {
    // 9. Time-grade parity, to the point where the two grades' timelines
    //    start to interleave differently. Up to esp-rtos start the boot is
    //    straight-line code, so the writes match exactly.
    let buf1 = SharedBuffer::new();
    let buf2 = SharedBuffer::new();
    let (Some(mut a), Some(mut b)) = (
        machine(&FwImage::NO_RADIO, TimeGrade::T1, &buf1),
        machine(&FwImage::NO_RADIO, TimeGrade::T2, &buf2),
    ) else {
        return;
    };
    let oa = a.run_until(&StopCondition::after_micros(100_000));
    let ob = b.run_until(&StopCondition::after_micros(100_000));
    assert!(matches!(oa, Outcome::Deadline { .. }), "{oa:?}");
    assert!(matches!(ob, Outcome::Deadline { .. }), "{ob:?}");
    // Same emulated time, a different number of instructions: the grades
    // really are different clocks.
    assert_eq!(a.cycles(), b.cycles());
    assert_ne!(a.instructions(), b.instructions());
    let wa = boot_writes(&buf1.lines());
    let wb = boot_writes(&buf2.lines());
    // 160 with the block filter: the 77 map writes, the PLIC setup, the
    // watchdog disables, the calibration, the RWDT arming.
    assert!(wa.len() > 100, "{} boot writes under t1", wa.len());
    for (i, (x, y)) in wa.iter().zip(&wb).enumerate() {
        assert_eq!(x, y, "boot write {i} differs between t1 and t2");
    }
    assert_eq!(wa.len(), wb.len());
    // And both reach the tick.
    for buf in [&buf1, &buf2] {
        assert!(
            buf.lines()
                .iter()
                .any(|l| l.contains("W4 TIMG0+0x07c int_clr"))
        );
    }
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn a_restored_snapshot_replays_the_same_trace_through_the_tick() {
    let start = SharedBuffer::new();
    let Some(mut m) = machine(&FwImage::NO_RADIO, TimeGrade::T1, &start) else {
        return;
    };
    // Past esp-rtos start (≈12 ms), through several ticks and idle skips.
    const N_US: u64 = 40_000;
    const M_US: u64 = 140_000;
    m.run_until(&StopCondition::after_micros(N_US));
    let snap = m.snapshot();
    let first = SharedBuffer::new();
    m.bus.trace = Trace::to_sink(Box::new(first.clone()))
        .with_block_filter(BLOCKS.iter().map(|s| s.to_string()));
    m.run_until(&StopCondition::after_micros(M_US));
    let (at_m, skips_m) = (m.cycles(), m.idle_skips());
    m.restore(&snap);
    let second = SharedBuffer::new();
    m.bus.trace = Trace::to_sink(Box::new(second.clone()))
        .with_block_filter(BLOCKS.iter().map(|s| s.to_string()));
    m.run_until(&StopCondition::after_micros(M_US));
    assert_eq!(m.cycles(), at_m);
    assert_eq!(
        first.contents(),
        second.contents(),
        "the replay after restore diverged"
    );
    assert!(
        first.lines().iter().any(|l| l.contains("int_clr")),
        "the window holds ticks"
    );
    let _ = skips_m;
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_flash_image_mounts_lpfs_and_reaches_the_idle_loop() {
    // M3's version of this test asserted the opposite, and named itself
    // after it: the brief's `esp32c6,server` image reads flash at boot, and
    // with SPI1 merely *accepted* the ROM's `esp_rom_spiflash_read` spun on
    // `cmd` at 11 ms — `SPIN SPI1+0x000 cmd = 0x10000000`, `idle_skips == 0`.
    // M4 models the controller, and the same image now formats `lpfs` and
    // idles. This is the no-radio twin of `boot_idle`'s flash-backed gate:
    // no radio blob, so nothing here depends on the WiFi stub.
    let buf = SharedBuffer::new();
    let Some(mut m) = machine(&FwImage::NO_RADIO_FLASH, TimeGrade::T1, &buf) else {
        return;
    };
    m.bus.trace = Trace::to_sink(Box::new(buf.clone())).with_block_filter(["NOTHING"]);
    let outcome = m.run_until(&StopCondition::after_micros(3_000_000));
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert_eq!(m.bus.unmapped_reads(), 0);

    let lines = buf.lines();
    let spins: Vec<&String> = lines.iter().filter(|l| l.contains(" SPIN ")).collect();
    assert!(
        !spins.iter().any(|l| l.contains("SPI1")),
        "the flash controller is modelled now: {spins:?}"
    );
    let census = m.flash_census();
    assert!(
        census.reads > 0 && census.sector_erases > 0 && census.programs > 0,
        "an erased chip is read, erased and programmed: {census}"
    );
    assert!(m.idle_skips() > 100, "{} idle skips", m.idle_skips());
}

/// The trace excerpts the PR body quotes, printed so `--nocapture` gives
/// them without a second run.
#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn print_gate_excerpts() {
    let Some(GateRun { m, outcome, lines }) = gate_run(TimeGrade::T1) else {
        return;
    };
    println!("outcome: {outcome:?}");
    println!(
        "cycles={} instructions={} idle_skips={} unmapped={}r/{}w lines={}",
        m.cycles(),
        m.instructions(),
        m.idle_skips(),
        m.bus.unmapped_reads(),
        m.bus.unmapped_writes(),
        lines.len()
    );
    for needle in [
        "WATCHPOINT",
        " SPIN ",
        "core_0_intr_map22",
        "mxint1_pri",
        "core_0_intr_map51",
        "wdtconfig1",
        "wdtconfig0 = 0xc007e214",
    ] {
        for l in lines.iter().filter(|l| l.contains(needle)).take(3) {
            println!("{l}");
        }
    }
    let ticks: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("W4 TIMG0+0x07c int_clr"))
        .collect();
    println!("int_clr writes: {}", ticks.len());
    for l in ticks.iter().take(2).chain(ticks.iter().rev().take(2).rev()) {
        println!("{l}");
    }
    let feeds: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("W4 LP_WDT+0x014 wdtfeed"))
        .collect();
    println!("wdtfeed writes: {}", feeds.len());
    for l in feeds.iter().take(1).chain(feeds.iter().rev().take(1)) {
        println!("{l}");
    }
}
