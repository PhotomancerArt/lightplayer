//! Silicon rig for the interrupt-executor wake assumptions behind
//! `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`.
//!
//! The ADR's load-bearing claim (finding 1) is that esp-rtos never delivers
//! embassy-time wakes to tasks on an `InterruptExecutor` — the reason
//! `serial::io_task` paces on a hardware tick instead of `Timer::after`. That
//! claim was measured on swi1, which finding 3 later revealed to be the APP
//! core wire-pusher's doorbell, so the original observation is confounded.
//! This rig re-asks the question with every confound removed: **swi3** (owned
//! by nobody), no io_task, no engine, no APP core, prints only from thread
//! context.
//!
//! It is also the **esp-rtos upgrade canary**: run it after every esp-rtos
//! bump. `timer_wake=NONE` means the pacer workaround is still required;
//! `timer_wake=DELIVERED` means upstream changed and the pacer can be retired
//! (deliberately, with this rig as the gate — see the ADR's finding 1).
//!
//! Facts print as `[IEXEC]` lines; `[IEXEC] END-IEXEC` ends the capture
//! (`just fwtest-iexec-esp32v3 <port>`). The rig asserts nothing — the
//! capture is the result, interpreted against the ADR.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::interrupt::Priority;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;

/// Thread → probe doorbell for the signal-wake check.
static SIG: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// Wakes the probe actually received, per mechanism, read from thread context.
static SIG_WAKES: AtomicU32 = AtomicU32::new(0);
static TIMER_WAKES: AtomicU32 = AtomicU32::new(0);
/// Where the probe last was: 1 = awaiting SIG, 2 = awaiting Timer.
static PROBE_STAGE: AtomicU32 = AtomicU32::new(0);

/// The task under test. Lives on the swi3 interrupt executor; its awaits are
/// exactly the two wake mechanisms the ADR distinguishes.
#[embassy_executor::task]
async fn iexec_probe() {
    // Phase 1 — signal wakes (the pacer mechanism): five direct waker wakes.
    PROBE_STAGE.store(1, Ordering::Relaxed);
    for _ in 0..5 {
        SIG.wait().await;
        SIG_WAKES.fetch_add(1, Ordering::Relaxed);
    }

    // Phase 2 — embassy-time wakes (the ADR-prohibited mechanism): count how
    // many 10 ms sleeps complete. If the ADR's finding 1 holds, the first
    // await parks forever and the counter stays 0.
    PROBE_STAGE.store(2, Ordering::Relaxed);
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
        TIMER_WAKES.fetch_add(1, Ordering::Relaxed);
    }
}

/// Entry: brings up the minimal esp-rtos world (no app), runs both checks,
/// prints the verdict, and idles.
pub async fn run(_spawner: embassy_executor::Spawner) -> ! {
    esp_println::println!(
        "[IEXEC] interrupt-executor wake rig: chip=esp32 esp-rtos=0.3.0 swi=3 prio=2 commit={} dirty={}",
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
    );

    // The interrupt executor under test. Same static-mut pattern as the app's
    // (see main.rs IO_EXECUTOR): `start` needs `&'static mut self`.
    // SAFETY: the one and only reference; this fn runs once and never returns.
    static mut EXECUTOR: Option<esp_rtos::embassy::InterruptExecutor<3>> = None;
    let executor: &'static mut Option<esp_rtos::embassy::InterruptExecutor<3>> =
        unsafe { &mut *core::ptr::addr_of_mut!(EXECUTOR) };
    let spawner = executor
        .insert(esp_rtos::embassy::InterruptExecutor::new(unsafe {
            // SAFETY: swi3 is claimed nowhere else in this firmware — swi0 is
            // the scheduler's, swi1 the wire pusher's doorbell (app builds
            // only; absent here), swi2 the app io executor's (also absent
            // here). The harness replaces the whole app, so even those
            // claimants do not exist in this image.
            esp_hal::interrupt::software::SoftwareInterrupt::<3>::steal()
        }))
        .start(Priority::Priority2);
    spawner.spawn(iexec_probe().unwrap());

    // Phase 1 — signal wakes. Thread-side timers are the proven mechanism
    // (ADR context), so pacing the doorbell with them is sound here.
    for _ in 0..5 {
        embassy_time::Timer::after(embassy_time::Duration::from_millis(20)).await;
        SIG.signal(());
    }
    embassy_time::Timer::after(embassy_time::Duration::from_millis(50)).await;
    let sig = SIG_WAKES.load(Ordering::Relaxed);
    esp_println::println!(
        "[IEXEC] signal_wake: {}/5 {}",
        sig,
        if sig == 5 { "DELIVERED" } else { "LOST" }
    );

    // Phase 2a — embassy-time wakes while the thread executor sleeps (idle
    // scheduler; the tick handler has every chance to run the timer queue).
    embassy_time::Timer::after(embassy_time::Duration::from_millis(500)).await;
    let idle_wakes = TIMER_WAKES.load(Ordering::Relaxed);

    // Phase 2b — the same while the thread context spins busily for 500 ms
    // (the engine-tick shape: thread never yields, only interrupts run).
    let spin = esp_hal::delay::Delay::new();
    spin.delay_millis(500);
    let busy_wakes = TIMER_WAKES.load(Ordering::Relaxed) - idle_wakes;

    let stage = PROBE_STAGE.load(Ordering::Relaxed);
    esp_println::println!(
        "[IEXEC] timer_wake: idle_500ms={idle_wakes} busy_500ms={busy_wakes} probe_stage={stage} verdict={}",
        if idle_wakes == 0 && busy_wakes == 0 {
            "NONE — ADR finding 1 CONFIRMED in isolation; the io pacer stays required"
        } else {
            "DELIVERED — finding 1 does NOT hold on a clean SWI; pacer is a simplification candidate (gate on this rig)"
        }
    );
    esp_println::println!("[IEXEC] END-IEXEC");

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}

/// Board bring-up for the rig: clock, the UART0 baud-divisor fix that makes
/// `esp_println` legible (same load-bearing construction as the FP harness —
/// see main.rs's harness entrypoint), heap, and the esp-rtos scheduler.
pub fn init() {
    let peripherals =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));

    // ⚠️ Load-bearing, not decorative: reprograms the divisor esp-println
    // depends on after the clock change above. Must stay bound.
    let _uart0 = esp_hal::uart::Uart::new(
        peripherals.UART0,
        esp_hal::uart::Config::default().with_baudrate(921_600),
    )
    .expect("uart0 config")
    .with_tx(peripherals.GPIO1)
    .with_rx(peripherals.GPIO3);

    esp_alloc::heap_allocator!(size: 32 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    esp_println::println!("[IEXEC] runtime started");
}
