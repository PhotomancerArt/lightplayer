//! The classic's clock, register for register: TIMG0's three counters, the
//! RTC calibration that decides what the crystal is, the watchdog gates, and
//! (from the RTC_CNTL half) the stall key and the reset cause.
//!
//! Every driver sequence in here is copied from the code that runs on the
//! desk board — `esp-hal-1.1.1` and the mask ROM — and cited where it came
//! from, so a test failing means the model diverged from a driver rather
//! than from an assertion somebody wrote down once.

use lp_emu_esp_common::{Peripheral, Sandbox};
use lp_emu_esp32v3::memmap;
use lp_emu_esp32v3::periph::timg::{TIMG_LEN, Timg};

// TIMG0's register offsets, as `regs/timg0.rs` generates them. Spelled out
// here rather than imported so the test is an independent statement of the
// classic's layout — the phase file's `timg_int_offsets`.
const T0_CONFIG: u32 = 0x00;
const T0_LO: u32 = 0x04;
const T0_HI: u32 = 0x08;
const T0_UPDATE: u32 = 0x0c;
const T0_ALARMLO: u32 = 0x10;
const T0_LOAD: u32 = 0x20;
const T1_CONFIG: u32 = 0x24;
const T1_UPDATE: u32 = 0x30;
const WDTCONFIG0: u32 = 0x48;
const WDTFEED: u32 = 0x60;
const WDTWPROTECT: u32 = 0x64;
const RTCCALICFG: u32 = 0x68;
const RTCCALICFG1: u32 = 0x6c;
const LACTCONFIG: u32 = 0x70;
const LACTLO: u32 = 0x78;
const LACTHI: u32 = 0x7c;
const LACTUPDATE: u32 = 0x80;
const LACTALARMLO: u32 = 0x84;
const LACTALARMHI: u32 = 0x88;
const LACTLOAD: u32 = 0x94;
const INT_ENA: u32 = 0x98;
const INT_RAW: u32 = 0x9c;
const INT_ST: u32 = 0xa0;
const INT_CLR: u32 = 0xa4;

const CFG_ALARM_EN: u32 = 1 << 10;
const CFG_LEVEL_INT_EN: u32 = 1 << 11;
const CFG_DIVIDER_SHIFT: u32 = 13;
const CFG_AUTORELOAD: u32 = 1 << 29;
const CFG_INCREASE: u32 = 1 << 30;
const CFG_EN: u32 = 1 << 31;

const CALI_RDY: u32 = 1 << 15;
const CALI_START: u32 = 1 << 31;
const CALI_CLK_SEL_SHIFT: u32 = 13;
const CALI_MAX_SHIFT: u32 = 16;

const WDT_WKEY: u32 = 0x50D8_3AA1;

/// TIMG0's peripheral interrupt sources (`esp32-0.40.2/src/lib.rs`).
const TG0_T0_LEVEL: u16 = 14;
const TG0_T1_LEVEL: u16 = 15;
const TG0_LACT_LEVEL: u16 = 17;

/// `esp-hal-1.1.1/src/time.rs:723-735`, `#[cfg(esp32)] time_init` — the
/// sequence that makes LACT the classic's `Instant::now()`, with APB at
/// 80 MHz so `divider` is 5.
fn lact_time_init(sb: &mut Sandbox, t: &mut Timg) {
    let apb: u32 = 80_000_000;
    sb.write(t, LACTCONFIG, 0);
    sb.write(t, LACTALARMHI, u32::MAX);
    sb.write(t, LACTALARMLO, u32::MAX);
    sb.write(t, LACTLOAD, 1);
    let divider = apb / 16_000_000;
    sb.write(
        t,
        LACTCONFIG,
        (divider << CFG_DIVIDER_SHIFT) | CFG_INCREASE | CFG_AUTORELOAD | CFG_EN,
    );
}

/// `esp-hal-1.1.1/src/time.rs:740-757`, `#[cfg(esp32)] now()` — the update
/// pulse, the poll on `lactlo`, and `ticks / 16` for microseconds.
fn lact_now_micros(sb: &mut Sandbox, t: &mut Timg) -> u64 {
    sb.write(t, LACTUPDATE, 1);
    let lo_initial = sb.read(t, LACTLO);
    let mut div = (sb.read(t, LACTCONFIG) >> CFG_DIVIDER_SHIFT) & 0xffff;
    let lo = loop {
        let lo = sb.read(t, LACTLO);
        if lo != lo_initial || div == 0 {
            break lo;
        }
        div -= 1;
    };
    let hi = sb.read(t, LACTHI);
    (((u64::from(hi)) << 32) | u64::from(lo)) / 16
}

/// `esp-hal-1.1.1/src/timer/timg.rs:474-486`, `now()` on a `T` timer.
fn timer_now(sb: &mut Sandbox, t: &mut Timg, update: u32, lo: u32, hi: u32) -> u64 {
    sb.write(t, update, 1);
    let lo = sb.read(t, lo);
    let hi = sb.read(t, hi);
    (u64::from(hi) << 32) | u64::from(lo)
}

#[test]
fn lact_counts_at_its_stated_rate() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    t.attached(3);
    sb.now = 0;
    lact_time_init(&mut sb, &mut t);
    assert_eq!(lact_now_micros(&mut sb, &mut t), 0);

    // The derivation: APB 80 MHz / divider 5 = 16 MHz, and `now()` divides
    // the count by 16. So one emulated millisecond — CPU_HZ/1000 guest
    // cycles — must read back as 1000 microseconds, and the tick count as
    // 16_000.
    sb.now = memmap::CPU_HZ / 1_000;
    assert_eq!(lact_now_micros(&mut sb, &mut t), 1_000);
    sb.write(&mut t, LACTUPDATE, 1);
    assert_eq!(sb.read(&mut t, LACTLO), 16_000);
    assert_eq!(sb.read(&mut t, LACTHI), 0, "still in the low word");

    // A full emulated second: 16 million ticks, and the counter is 64-bit,
    // so nothing wraps.
    sb.now = memmap::CPU_HZ;
    assert_eq!(lact_now_micros(&mut sb, &mut t), 1_000_000);
    sb.write(&mut t, LACTUPDATE, 1);
    assert_eq!(sb.read(&mut t, LACTLO), 16_000_000);

    // And the read is a *latch*: the count moves on, the registers do not,
    // until the next update pulse. `now()` leans on this — it polls `lactlo`
    // for a change and gives up after `divider` tries.
    let before = sb.read(&mut t, LACTLO);
    sb.now = memmap::CPU_HZ * 2;
    assert_eq!(sb.read(&mut t, LACTLO), before, "latched, not live");
    assert_eq!(lact_now_micros(&mut sb, &mut t), 2_000_000);
}

#[test]
fn timg0_three_counters() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    t.attached(3);
    sb.now = 0;
    // t0 at the reset prescaler (`t0config` = 0x6000_2000: divider 1 → 2 by
    // the TRM's rule, increase, auto-reload), t1 at divider 80, LACT at 5.
    sb.write(&mut t, T0_CONFIG, 0x6000_2000 | CFG_EN);
    sb.write(
        &mut t,
        T1_CONFIG,
        (80 << CFG_DIVIDER_SHIFT) | CFG_INCREASE | CFG_EN,
    );
    lact_time_init(&mut sb, &mut t);

    // One emulated millisecond.
    sb.now = memmap::CPU_HZ / 1_000;
    // t0: APB 80 MHz / 2 = 40 MHz → 40_000 ticks.
    assert_eq!(timer_now(&mut sb, &mut t, T0_UPDATE, T0_LO, T0_HI), 40_000);
    // t1: 80 MHz / 80 = 1 MHz → 1_000 ticks, the 1 ms io pacer's rate.
    assert_eq!(
        timer_now(
            &mut sb,
            &mut t,
            T1_UPDATE,
            T1_CONFIG + 0x04,
            T1_CONFIG + 0x08
        ),
        1_000
    );
    // LACT: 16 MHz → 16_000 ticks.
    assert_eq!(lact_now_micros(&mut sb, &mut t), 1_000);

    // Reloading t0 leaves the other two alone — the "three independent
    // counters over one engine" claim, on this chip's own registers.
    sb.write(&mut t, T0_LOAD, 1);
    assert_eq!(timer_now(&mut sb, &mut t, T0_UPDATE, T0_LO, T0_HI), 0);
    assert_eq!(
        timer_now(
            &mut sb,
            &mut t,
            T1_UPDATE,
            T1_CONFIG + 0x04,
            T1_CONFIG + 0x08
        ),
        1_000
    );
    assert_eq!(lact_now_micros(&mut sb, &mut t), 1_000);
}

#[test]
fn timg_int_offsets() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    t.attached(3);
    // The classic's interrupt registers are at 0x98..0xa4 — where the C6 and
    // the S3 keep their calibration registers, and 0x28 past where they keep
    // these.
    assert_eq!(t.reg_name(INT_ENA), Some("int_ena"));
    assert_eq!(t.reg_name(INT_RAW), Some("int_raw"));
    assert_eq!(t.reg_name(INT_ST), Some("int_st"));
    assert_eq!(t.reg_name(INT_CLR), Some("int_clr"));
    assert_eq!(t.reg_name(0x70), Some("lactconfig"));
    assert_eq!(t.reg_name(0x68), Some("rtccalicfg"));
    assert_eq!(t.reg_name(0x24), Some("t1.config"));

    // t0's alarm sets bit 0 of `int_raw` and — because `int_ena` does not
    // gate an interrupt on this chip — drives the level source from
    // `tconfig.level_int_en` (esp-hal `timer/timg.rs:517-526`).
    sb.now = 0;
    sb.write(&mut t, T0_CONFIG, 0x6000_2000 | CFG_EN);
    sb.write(&mut t, T0_ALARMLO, 40_000);
    let cfg = sb.read(&mut t, T0_CONFIG);
    sb.write(&mut t, T0_CONFIG, cfg | CFG_ALARM_EN | CFG_LEVEL_INT_EN);
    // 40_000 ticks at 40 MHz = 1 ms.
    let due = memmap::CPU_HZ / 1_000;
    assert_eq!(sb.sched.next_deadline(), Some(due));
    sb.run_to(&mut t, due - 1);
    assert!(!sb.irq.level(TG0_T0_LEVEL));
    sb.run_to(&mut t, due);
    assert!(sb.irq.level(TG0_T0_LEVEL), "the level source is asserted");
    assert_eq!(sb.read(&mut t, INT_RAW) & 1, 1);
    assert_eq!(
        sb.read(&mut t, INT_ST),
        0,
        "int_st is int_raw & int_ena, and nothing enabled it — which is why \
         esp-hal uses level_int_en on this chip instead"
    );
    assert_eq!(
        sb.read(&mut t, T0_CONFIG) & CFG_ALARM_EN,
        0,
        "alarm_en clears itself when the alarm occurs"
    );
    sb.write(&mut t, INT_CLR, 1);
    assert!(!sb.irq.level(TG0_T0_LEVEL));
    assert_eq!(sb.read(&mut t, INT_CLR), 0, "a pulse reads 0");

    // t1 and LACT have their own bits and their own sources.
    assert!(!sb.irq.level(TG0_T1_LEVEL));
    assert!(!sb.irq.level(TG0_LACT_LEVEL));
}

#[test]
fn wdt_write_protect() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    // `wdtwprotect` resets to the key itself in the PAC, so the block comes
    // out of reset UNLOCKED. That is the part, not a mistake — the C6's
    // TIMG0 does the same and its LP_WDT does not.
    assert_eq!(sb.read(&mut t, WDTWPROTECT), WDT_WKEY);
    let pac_cfg0 = sb.read(&mut t, WDTCONFIG0);
    assert_eq!(pac_cfg0, 0x0004_c000, "the PAC's reset for wdtconfig0");

    // esp_hal::init's disable: unlock, clear wdt_en, lock.
    sb.write(&mut t, WDTWPROTECT, WDT_WKEY);
    sb.write(&mut t, WDTCONFIG0, 0);
    sb.write(&mut t, WDTWPROTECT, 0);
    assert_eq!(sb.read(&mut t, WDTCONFIG0), 0);

    // Locked: a write with the wrong key is DROPPED, not taken. "The disable
    // silently did nothing" is exactly the failure a lenient model hides.
    sb.write(&mut t, WDTCONFIG0, 0xffff_ffff);
    assert_eq!(sb.read(&mut t, WDTCONFIG0), 0, "locked");
    sb.write(&mut t, WDTWPROTECT, 0xdead_beef);
    sb.write(&mut t, WDTCONFIG0, 0xffff_ffff);
    assert_eq!(sb.read(&mut t, WDTCONFIG0), 0, "still locked");
    // Unlock again and it takes.
    sb.write(&mut t, WDTWPROTECT, WDT_WKEY);
    sb.write(&mut t, WDTCONFIG0, 0x1234);
    assert_eq!(sb.read(&mut t, WDTCONFIG0), 0x1234);
    // `wdtfeed` is a pulse: written, never stored.
    sb.write(&mut t, WDTFEED, 1);
    assert_eq!(sb.read(&mut t, WDTFEED), 0);
}

/// The calibration `detect_xtal_freq` drives, arithmetic included
/// (`esp-hal-1.1.1/src/soc/esp32/clocks.rs:136-162`).
#[test]
fn the_calibration_answers_a_forty_megahertz_crystal() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    t.attached(3);
    const CALIBRATION_CYCLES: u32 = 10;
    // RC_FAST/256 = 8 MHz / 256.
    const RC_FAST_DIV_HZ: u32 = 8_000_000 / 256;

    sb.now = 1_000;
    // `rtc_cali_clk_sel` = 1 (RcFastDivClk), max = 10, start.
    sb.write(
        &mut t,
        RTCCALICFG,
        (1 << CALI_CLK_SEL_SHIFT) | (CALIBRATION_CYCLES << CALI_MAX_SHIFT) | CALI_START,
    );
    assert_eq!(sb.read(&mut t, RTCCALICFG) & CALI_RDY, 0, "counting");
    let Some(due) = sb.sched.next_deadline() else {
        panic!("the measurement was scheduled");
    };
    sb.run_to(&mut t, due);
    assert!(sb.read(&mut t, RTCCALICFG) & CALI_RDY != 0, "ready");

    let xtal_cycles = sb.read(&mut t, RTCCALICFG1) >> 7;
    assert_eq!(xtal_cycles, 12_800, "40 MHz over 10 cycles of 31_250 Hz");
    // esp-hal's own line: `(calibration_clock_frequency * xtal_cycles /
    // CALIBRATION_CYCLES).as_mhz()`.
    let hz = RC_FAST_DIV_HZ * xtal_cycles / CALIBRATION_CYCLES;
    assert_eq!(hz, 40_000_000);
    let mhz = hz / 1_000_000;
    assert!(
        mhz.abs_diff(40) < mhz.abs_diff(26),
        "detect_xtal_freq takes the 40 MHz arm"
    );
}

/// The other caller: `calibrate_rtc_slow_clock`, 1024 cycles of RC_SLOW.
#[test]
fn the_calibration_answers_the_slow_clocks_period_too() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    t.attached(3);
    sb.now = 0;
    // `rtc_cali_clk_sel` = 0 (RcSlowClk), max = 1024, start.
    sb.write(&mut t, RTCCALICFG, (1024 << CALI_MAX_SHIFT) | CALI_START);
    let due = sb.sched.next_deadline().expect("scheduled");
    sb.run_to(&mut t, due);
    let cal = u64::from(sb.read(&mut t, RTCCALICFG1) >> 7);
    // 40 MHz XTAL cycles counted over 1024 cycles of a 150 kHz clock.
    assert_eq!(cal, 40_000_000u64 * 1024 / 150_000);
    assert_eq!(cal, 273_066);
    // …which is 150 kHz back out, to the rounding of an integer ratio.
    let slow_hz = 40_000_000u64 * 1024 / cal;
    assert_eq!(slow_hz, 150_000);
}

#[test]
fn timg1_is_the_same_block_at_its_own_interrupt_sources() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg1();
    t.attached(5);
    assert_eq!(Peripheral::name(&t), "TIMG1");
    assert_eq!(t.reg_name(LACTCONFIG), Some("lactconfig"));
    sb.now = 0;
    sb.write(&mut t, T0_CONFIG, 0x6000_2000 | CFG_EN);
    sb.write(&mut t, T0_ALARMLO, 40_000);
    let cfg = sb.read(&mut t, T0_CONFIG);
    sb.write(&mut t, T0_CONFIG, cfg | CFG_ALARM_EN | CFG_LEVEL_INT_EN);
    let due = memmap::CPU_HZ / 1_000;
    sb.run_to(&mut t, due);
    assert!(sb.irq.level(18), "TG1_T0_LEVEL, not TG0's 14");
    assert!(!sb.irq.level(TG0_T0_LEVEL));
}

#[test]
fn the_block_is_the_generated_table_and_its_pac_resets() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    // Spot-checks against `regs/timg0.rs`, which is generated from the PAC:
    // nothing in this view carries a hand-written reset value.
    assert_eq!(sb.read(&mut t, T0_CONFIG), 0x6000_2000);
    assert_eq!(sb.read(&mut t, T1_CONFIG), 0x6000_2000);
    assert_eq!(sb.read(&mut t, LACTCONFIG), 0x6000_2300);
    assert_eq!(sb.read(&mut t, WDTWPROTECT), 0x50d8_3aa1);
    assert_eq!(sb.read(&mut t, 0x04c), 0x0001_0000, "wdtconfig1");
    assert_eq!(sb.read(&mut t, 0x050), 0x018c_ba80, "wdtconfig2");
    // `rtccalicfg` resets with `start_cycling` set, `clk_sel` = 1 and
    // `max` = 1, so the block's first answer is a cycling calibration that
    // has already produced a result: 40e6 / 31_250 = 1_280 XTAL cycles.
    assert_eq!(sb.read(&mut t, RTCCALICFG) & !CALI_RDY, 0x0001_3000);
    assert_eq!(sb.read(&mut t, RTCCALICFG1) >> 7, 1_280);
    assert_eq!(t.reg_name(TIMG_LEN - 4), Some("timgclk"));
}

#[test]
fn the_state_round_trips() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    t.attached(3);
    sb.now = 777;
    lact_time_init(&mut sb, &mut t);
    sb.write(&mut t, T0_CONFIG, 0x6000_2000 | CFG_EN);
    sb.now = 500_000;
    lact_now_micros(&mut sb, &mut t);
    timer_now(&mut sb, &mut t, T0_UPDATE, T0_LO, T0_HI);
    let blob = t.save_state();

    let mut other = Timg::timg0();
    other.load_state(&blob);
    assert_eq!(other.lact_micros(), t.lact_micros());
    assert_eq!(other.count(0, sb.now), t.count(0, sb.now));
    assert_eq!(other.save_state(), blob);

    // A short blob applies nothing rather than half-loading.
    let mut third = Timg::timg0();
    third.load_state(&blob[..8]);
    assert_eq!(third.lact_micros(), 0);
}

/// **The LACT cost, measured.** `Instant::now()` reads a modelled counter,
/// and every counter read costs two extra `RegFile` reads (the alarm pair)
/// plus two more (the load pair) inside [`Timg::counter_config`], which is
/// rebuilt at every call because any of it can have changed. This is the
/// number the phase was asked to produce; it is `#[ignore]`d because a
/// timing assertion in CI is a flake generator (`m3/notes.md`), and it
/// prints rather than asserts.
///
/// Run it with:
/// `cargo test -p lp-emu-esp32v3 --release --test clock -- --ignored --nocapture`
#[test]
#[ignore = "a timing measurement, not a gate; run with --ignored --nocapture"]
fn what_a_lact_timestamp_costs() {
    let mut sb = Sandbox::new();
    let mut t = Timg::timg0();
    t.attached(3);
    sb.now = 0;
    lact_time_init(&mut sb, &mut t);

    const N: u32 = 200_000;
    let start = std::time::Instant::now();
    let mut acc = 0u64;
    for i in 0..N {
        sb.now = u64::from(i) * 240;
        acc = acc.wrapping_add(lact_now_micros(&mut sb, &mut t));
    }
    let elapsed = start.elapsed();
    println!(
        "LACT Instant::now(): {N} timestamps in {elapsed:?} = {:.1} ns each \
         ({:.1} ns per guest MMIO access, 8 accesses per timestamp); acc={acc}",
        elapsed.as_nanos() as f64 / f64::from(N),
        elapsed.as_nanos() as f64 / f64::from(N) / 8.0,
    );
}
