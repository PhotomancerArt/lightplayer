//! Core 1 as a render helper (feature `two-core`): it takes the upper half
//! of every render-samples batch (`lpvm_native::helper_core`), so the
//! shader's sampling runs on both cores.
//!
//! # Its life
//!
//! Started once, at boot, after the filesystem has settled the flash's read
//! mode ([`start`]). Core 1 binds `FROM_CPU_INTR2` on itself and then parks
//! in `waiti` from IRAM ([`helper_loop`]). Core 0 publishes a job and raises
//! the line ([`start_job`]); core 1 wakes, runs the job, clears it and parks
//! again; core 0 spins on the job word ([`join`]). An idle core 1 fetches
//! nothing and costs no power beyond its clock, and the emulator gives a
//! parked core no host time.
//!
//! # The rules core 1 lives by
//!
//! A job is the JIT'd entry against its own vmctx and the batch's buffers:
//! no allocation, no lock, no log (`lpvm_native::helper_core::run_helper_job`).
//! Two things depend on it, and both are enforced, not assumed:
//!
//! - **A flash write stalls core 1 wherever it is.** Code it runs lives in
//!   flash (builtins) and PSRAM (the JIT), and a cache refill from flash
//!   during an erase hangs the chip (PSRAM spike). Every flash writer parks it
//!   with RTC_CNTL's stall key: esp-storage's `FlashStorage` through
//!   `multicore_auto_park` (`main.rs`), and the split image's raw ROM calls
//!   through [`with_core1_parked`] (`split_core`). A stalled core that held a
//!   lock would deadlock the writer, which is why it holds none.
//! - **Core 1 never takes the allocator's lock.** The global allocator is
//!   [`crate::core1_guarded_heap`]'s wrapper, which sends any request from
//!   core 1 to [`halt_core1`] instead of `esp_alloc`.
//!
//! # When something goes wrong on core 1
//!
//! A panic or an exception on core 1, and a forbidden allocation, end in
//! [`halt_core1`]: core 1 records why, masks its interrupts and parks for
//! good, taking no lock and printing nothing. Core 0's [`join`] times out
//! (2 s: long enough for a block erase another thread ran mid-join), runs the
//! half itself, turns the split off for the rest of the boot and says why
//! once (`[core1] stopped: …`). The board keeps rendering, on one core.
//!
//! Radio and driver interrupts stay on core 0: esp-hal maps an interrupt on
//! the core that enables it, and every driver enables from core 0. Core 1
//! enables only its wake line.

use core::ptr::{addr_of_mut, null_mut};
use core::sync::atomic::{AtomicPtr, AtomicU8, AtomicU32, Ordering};

use esp_hal::interrupt::software::SoftwareInterrupt;
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::system::{Cpu, CpuControl, Stack};
use lpvm_native::helper_core::{HelperCore, HelperJob};

/// Core 1's stack. The shader's frames run on it, so it is sized like the
/// render path's share of the main stack, not like an idle loop.
pub const STACK_BYTES: usize = 8 * 1024;
/// What this feature takes of `dram_seg`'s `.bss`, given back by the arena.
pub const BSS_BYTES: usize = STACK_BYTES;

/// How long [`join`] waits for core 1, in core-0 cycles at 240 MHz: 2 s.
const JOIN_TIMEOUT_CYCLES: u32 = 480_000_000;

static mut APP_CORE_STACK: Stack<STACK_BYTES> = Stack::new();

/// The job core 1 is running, or null. Internal RAM (`.bss`), as is all of
/// core 1's state: nothing it touches while idle is cached.
static JOB: AtomicPtr<HelperJob> = AtomicPtr::new(null_mut());
/// [`State`] as a byte.
static STATE: AtomicU8 = AtomicU8::new(State::Off as u8);
/// Why core 1 stopped ([`Stop`] as a byte), once it has.
static STOP: AtomicU8 = AtomicU8::new(0);
/// Jobs core 1 has finished since boot.
static JOBS_DONE: AtomicU32 = AtomicU32::new(0);

/// The wake line: `FROM_CPU_INTR2`. esp-rtos takes software interrupt 0 and
/// the emulator seams' wake 3; 1 and 2 are otherwise unclaimed.
const WAKE_LINE: usize = 2;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Not started, or starting.
    Off = 0,
    /// Parked or running jobs.
    Ready = 1,
    /// Halted for good ([`halt_core1`]), or given up on by [`join`].
    Stopped = 2,
}

/// Why core 1 halted.
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Stop {
    /// A Rust panic, or an exception esp-hal turned into one.
    Panic = 1,
    /// A request to the global allocator.
    Allocation = 2,
    /// [`join`] waited [`JOIN_TIMEOUT_CYCLES`] and gave up.
    Timeout = 3,
}

impl Stop {
    fn words(code: u8) -> &'static str {
        match code {
            1 => "a panic on core 1",
            2 => "core 1 asked the allocator for memory, which it must never do",
            3 => "a job did not finish in 2 s",
            _ => "unknown",
        }
    }
}

static PERIPHERAL: critical_section::Mutex<
    core::cell::RefCell<Option<esp_hal::peripherals::CPU_CTRL<'static>>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(None));

/// Keep `CPU_CTRL` until [`start`]. From `init_board`.
pub fn stash(peripheral: esp_hal::peripherals::CPU_CTRL<'static>) {
    critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).replace(peripheral));
}

/// Start core 1 and install it as the render helper. Once, from `core_boot`.
pub fn start() {
    let Some(peripheral) = critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).take()) else {
        return;
    };
    let mut cpu_control = CpuControl::new(peripheral);
    // SAFETY: the one reference to the stack, taken once.
    let stack = unsafe { &mut *addr_of_mut!(APP_CORE_STACK) };
    match cpu_control.start_app_core(stack, core1_main) {
        // The guard parks core 1 on drop; it lives for the firmware's life.
        Ok(guard) => core::mem::forget(guard),
        Err(e) => {
            esp_println::println!("[core1] did not start: {e:?}");
            return;
        }
    }
    let begun = ccount();
    while STATE.load(Ordering::Acquire) == State::Off as u8 {
        if ccount().wrapping_sub(begun) > JOIN_TIMEOUT_CYCLES {
            esp_println::println!("[core1] did not reach its loop; rendering on one core");
            return;
        }
        core::hint::spin_loop();
    }
    lpvm_native::helper_core::set_helper_core(HelperCore {
        start: start_job,
        join,
    });
    esp_println::println!("[core1] render helper on: core 1 takes half of every batch");
}

/// Run `f` with core 1 stalled by RTC_CNTL's stall key, for a flash write
/// that does not go through esp-storage's `FlashStorage` (which parks it
/// itself). Nothing happens to a core 1 that is not running.
#[cfg_attr(
    not(lp_split),
    allow(dead_code, reason = "the split image's raw flash calls")
)]
pub fn with_core1_parked<R>(f: impl FnOnce() -> R) -> R {
    if !esp_hal::system::is_running(Cpu::AppCpu) {
        return f();
    }
    // SAFETY: the stall touches only RTC_CNTL's stall fields, which nothing
    // else on core 0 writes concurrently (esp-storage's own park runs inside
    // its own writes, never around ours); core 1 holds no lock to deadlock
    // on (the module docs).
    let mut cpu = CpuControl::new(unsafe { esp_hal::peripherals::CPU_CTRL::steal() });
    // SAFETY: called on core 0, never on the core it parks.
    unsafe { cpu.park_core(Cpu::AppCpu) };
    let r = f();
    cpu.unpark_core(Cpu::AppCpu);
    r
}

/// Halt core 1 for good: record `why`, mask every interrupt and park. Takes
/// no lock and prints nothing, so it is safe from core 1's panic handler and
/// from inside the allocator. Never returns.
#[esp_hal::ram]
pub fn halt_core1(why: Stop) -> ! {
    STOP.store(why as u8, Ordering::Release);
    STATE.store(State::Stopped as u8, Ordering::Release);
    loop {
        // SAFETY: masks this core's interrupts and waits; nothing ever wakes
        // it at level 15.
        unsafe { core::arch::asm!("rsil {0}, 15", "waiti 15", out(reg) _) };
    }
}

/// Is this code running on core 1?
#[inline(always)]
pub fn on_core1() -> bool {
    Cpu::current() == Cpu::AppCpu
}

fn start_job(job: *const HelperJob) -> bool {
    if STATE.load(Ordering::Acquire) != State::Ready as u8 {
        return false;
    }
    if JOB
        .compare_exchange(
            null_mut(),
            job.cast_mut(),
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_err()
    {
        return false;
    }
    // SAFETY: software interrupt 2 is core 1's wake line and only this
    // module raises it.
    unsafe { SoftwareInterrupt::<'static, { WAKE_LINE as u8 }>::steal() }.raise();
    true
}

fn join() -> bool {
    let begun = ccount();
    while !JOB.load(Ordering::Acquire).is_null() {
        if STATE.load(Ordering::Acquire) == State::Stopped as u8
            || ccount().wrapping_sub(begun) > JOIN_TIMEOUT_CYCLES
        {
            give_up();
            return false;
        }
        core::hint::spin_loop();
    }
    true
}

/// Core 1 stopped or never answered: stall it (in case it is merely slow),
/// stop splitting, and say so once.
#[cold]
fn give_up() {
    let _ = STOP.compare_exchange(0, Stop::Timeout as u8, Ordering::AcqRel, Ordering::Relaxed);
    STATE.store(State::Stopped as u8, Ordering::Release);
    lpvm_native::helper_core::clear_helper_core();
    // SAFETY: as in `with_core1_parked`; a core that stays stalled for the
    // rest of the boot never wakes into a job that is gone.
    let mut cpu = CpuControl::new(unsafe { esp_hal::peripherals::CPU_CTRL::steal() });
    unsafe { cpu.park_core(Cpu::AppCpu) };
    JOB.store(null_mut(), Ordering::Release);
    log::error!(
        "[core1] stopped: {}; rendering on one core for the rest of this boot",
        Stop::words(STOP.load(Ordering::Acquire))
    );
}

/// One `[core1]` line for the heartbeat: jobs finished since boot, or why it
/// stopped.
pub fn log_heartbeat() {
    match STATE.load(Ordering::Acquire) {
        s if s == State::Ready as u8 => {
            log::info!("[core1] jobs={}", JOBS_DONE.load(Ordering::Relaxed));
        }
        s if s == State::Stopped as u8 => log::info!(
            "[core1] stopped: {}",
            Stop::words(STOP.load(Ordering::Acquire))
        ),
        _ => {}
    }
}

/// Core 1's entry: its FPU, its wake line, then the loop. Runs once.
fn core1_main() {
    // Core 1 has its own CPENABLE: a Float-mode shader's FP ops fault
    // without coprocessor 0 armed (`board::esp32s3::fpu`).
    crate::board::esp32s3::fpu::arm();
    // SAFETY: software interrupt 2 is core 1's wake line; binding it here,
    // on core 1, maps it on core 1's interrupt matrix only.
    let mut line = unsafe { SoftwareInterrupt::<'static, { WAKE_LINE as u8 }>::steal() };
    line.set_interrupt_handler(InterruptHandler::new(wake_handler, Priority::Priority1));
    STATE.store(State::Ready as u8, Ordering::Release);
    helper_loop()
}

/// The wake line's handler, on core 1: clear the line. The work happens in
/// [`helper_loop`] after `waiti` returns.
#[esp_hal::ram]
extern "C" fn wake_handler() {
    let line = esp_hal::peripherals::SYSTEM::regs()
        .cpu_intr_from_cpu(WAKE_LINE)
        .as_ptr();
    // SAFETY: a store of 0 to `CPU_INTR_FROM_CPU_2`, a live MMIO register
    // only this handler and `start_job`'s raise touch.
    unsafe { core::ptr::write_volatile(line, 0) };
}

/// Core 1's whole life after start: park until there is a job, run it, say
/// so. From IRAM, touching only internal RAM while idle.
#[esp_hal::ram]
fn helper_loop() -> ! {
    loop {
        // Mask the wake line, look for a job, and only then `waiti`, which
        // unmasks and waits in one instruction: a raise after the look is
        // pending when `waiti` runs, so it returns at once. Never lost.
        let ps: u32;
        // SAFETY: raises this core's interrupt level to 1; restored below or
        // by `waiti`.
        unsafe { core::arch::asm!("rsil {0}, 1", out(reg) ps) };
        let job = JOB.load(Ordering::Acquire);
        if job.is_null() {
            // SAFETY: waits for an interrupt at level 0; the handler clears
            // the line and returns here.
            unsafe { core::arch::asm!("waiti 0") };
            continue;
        }
        // SAFETY: restores the interrupt level `rsil` saved.
        unsafe { core::arch::asm!("wsr.ps {0}", "rsync", in(reg) ps) };
        // SAFETY: core 0 keeps the job alive until JOB is null again, and a
        // job is the JIT entry against its own vmctx (the module docs).
        unsafe { lpvm_native::helper_core::run_helper_job(&*job) };
        JOBS_DONE.fetch_add(1, Ordering::Relaxed);
        JOB.store(null_mut(), Ordering::Release);
    }
}

#[inline(always)]
fn ccount() -> u32 {
    let c: u32;
    // SAFETY: reads this core's cycle counter, nothing else.
    unsafe { core::arch::asm!("rsr.ccount {0}", out(reg) c) };
    c
}
