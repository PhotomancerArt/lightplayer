//! SPIKE (feature `core1-render`): core 1 (the APP CPU) as a render helper.
//!
//! Not product code. Core 1 is started once, at boot, into [`helper_loop`] —
//! a spin on one word in internal RAM, from IRAM (`#[ram]`), taking no
//! interrupts (esp-hal's `start_core1_init` clears `INTENABLE`, then
//! `init_vectoring` enables only its five vectored CPU lines, and no source is
//! mapped to core 1: every driver enables from core 0). It touches neither flash nor PSRAM while idle,
//! which the PSRAM spike measured as immune to MSPI contention and to a flash
//! erase on core 0.
//!
//! While it runs a job it fetches from flash (the JIT's builtins, lpvm-native's
//! call path) and from wherever the JIT buffer and the point/sample buffers
//! live. So a flash write must not overlap a job: `FlashStorage` is built with
//! esp-storage's `multicore_auto_park` (the RTC_CNTL stall key, 0x86 — the
//! classic's `with_app_core_stalled` in hardware form), which stops core 1
//! wherever it is for the write and lets it go after. Core 1 never takes a lock
//! (no allocation, no logging, no critical section) so a park can never hold
//! one core 0 is about to need. The split image's raw `ll::` flash calls
//! (updates) bypass `FlashStorage` and are NOT parked here — a gap the spike
//! names, not one it fixes.
//!
//! The hand-off is [`lpvm_native::helper_core`]: the instance splits a
//! render-samples batch, core 0 starts the upper half here and runs the lower
//! half itself, then waits.

use core::ptr::{addr_of_mut, null_mut};
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use esp_hal::system::{CpuControl, Stack};
use lpvm_native::helper_core::{HelperCore, HelperJob};

/// Core 1's stack. The shader's frames run on it, so it is sized like the
/// render thread's share of the main stack, not like an idle loop.
static mut APP_CORE_STACK: Stack<16384> = Stack::new();

/// The job core 1 is running, or null when idle. Internal RAM (`.bss`).
static JOB: AtomicPtr<HelperJob> = AtomicPtr::new(null_mut());
/// Jobs core 1 has finished (a heartbeat for the spike's log line).
static JOBS_DONE: AtomicU32 = AtomicU32::new(0);
/// Set by core 1 once it reaches the loop.
static ALIVE: AtomicU32 = AtomicU32::new(0);

static PERIPHERAL: critical_section::Mutex<
    core::cell::RefCell<Option<esp_hal::peripherals::CPU_CTRL<'static>>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(None));

/// Keep `CPU_CTRL` for [`start`]. Called from `init_board`.
pub fn stash(peri: esp_hal::peripherals::CPU_CTRL<'static>) {
    critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).replace(peri));
}

/// Start core 1 and install it as the render helper. `LP_CORE1_RENDER=0` at
/// build time starts nothing (the single-core control on the same bytes
/// otherwise).
pub fn start() {
    lpvm_native::helper_core::set_render_clock(ccount);
    fw_esp32_common::stage_probe::set_clock(ccount);
    fw_esp32_common::stage_probe::set_extra(|| lpvm_native::helper_core::render_stats().2);
    if option_env!("LP_CORE1_RENDER") == Some("0") {
        esp_println::println!("[core1] off (LP_CORE1_RENDER=0)");
        return;
    }
    let Some(peri) = critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).take()) else {
        return;
    };
    let mut cpu_control = CpuControl::new(peri);
    // SAFETY: the one reference to the stack, taken once.
    let stack = unsafe { &mut *addr_of_mut!(APP_CORE_STACK) };
    match cpu_control.start_app_core(stack, || {
        // Core 1 has its own CPENABLE: a Float-mode shader's FP ops fault
        // without coprocessor 0 armed (`board::esp32s3::fpu`).
        crate::board::esp32s3::fpu::arm();
        helper_loop()
    }) {
        Ok(guard) => {
            // The guard parks core 1 on drop; core 1 lives for the firmware's life.
            core::mem::forget(guard);
        }
        Err(e) => {
            esp_println::println!("[core1] start failed: {e:?}");
            return;
        }
    }
    // Wait (bounded) for core 1 to say it is in the loop.
    let mut spins = 0u32;
    while ALIVE.load(Ordering::Acquire) == 0 && spins < 10_000_000 {
        spins += 1;
        core::hint::spin_loop();
    }
    if ALIVE.load(Ordering::Acquire) == 0 {
        esp_println::println!("[core1] did not reach its loop");
        return;
    }
    if let Some(min) = option_env!("LP_CORE1_MIN_SPLIT").and_then(|s| s.parse().ok()) {
        lpvm_native::helper_core::set_helper_min_split(min);
    }
    lpvm_native::helper_core::set_helper_core(HelperCore {
        start: start_job,
        join: join_job,
    });
    esp_println::println!("[core1] render helper on");
}

fn ccount() -> u32 {
    let c: u32;
    // SAFETY: reads the core's cycle counter, nothing else.
    unsafe { core::arch::asm!("rsr.ccount {0}", out(reg) c) };
    c
}

/// Jobs core 1 has finished since boot.
pub fn jobs_done() -> u32 {
    JOBS_DONE.load(Ordering::Relaxed)
}

fn start_job(job: *const HelperJob) -> bool {
    JOB.compare_exchange(
        null_mut(),
        job.cast_mut(),
        Ordering::AcqRel,
        Ordering::Relaxed,
    )
    .is_ok()
}

fn join_job() {
    while !JOB.load(Ordering::Acquire).is_null() {
        core::hint::spin_loop();
    }
}

/// Core 1's whole life: wait for a job, run it, say so. From IRAM, so an idle
/// core 1 fetches nothing through the cache.
#[esp_hal::ram]
fn helper_loop() {
    ALIVE.store(1, Ordering::Release);
    loop {
        let job = JOB.load(Ordering::Acquire);
        if job.is_null() {
            continue;
        }
        // SAFETY: core 0 keeps the job alive until JOB is null again.
        unsafe { lpvm_native::helper_core::run_helper_job(&*job) };
        JOBS_DONE.fetch_add(1, Ordering::Relaxed);
        JOB.store(null_mut(), Ordering::Release);
    }
}
