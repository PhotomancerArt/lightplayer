//! SPIKE ONLY (feature `spike_seam_wake_probe`, never merges): the
//! wake-interrupt probe of emulator seams M0 part B.
//!
//! A capability seam's data arrives when the host has it, and the host never
//! calls into the guest. So the host **raises an interrupt** to wake the
//! guest, and the guest pulls:
//!
//! 1. the host sets bits in [`LP_SEAM_PENDING`] (a guest RAM word whose
//!    address is in the seam table), then raises `FROM_CPU_INTR3`
//!    ([`WAKE_SWI`]);
//! 2. [`wake_isr`] (in RAM) clears the line **first**, then atomically swaps
//!    the word to zero (`amoswap`) and wakes the matching waker — in that
//!    order, so a set-and-raise landing between the two is caught either by
//!    the swap or by the line it re-raised;
//! 3. each consumer task drains `lp_seam_probe_take` **until it returns 0**
//!    before it sleeps again, and checks that every event arrives exactly
//!    once and in order.
//!
//! Two consumers, one per executor: channel 0 on the main thread-mode
//! executor, channel 1 on the link IO thread's executor.
//!
//! Switch shape: nothing here runs unless the probe seam is engaged. Two
//! engaged checks are built side by side for the B3 comparison:
//! (i) [`lp_seam_engaged`], a hooked query whose silicon body returns 0, and
//! (ii) [`LP_SEAM_PROBE_ENGAGED`], a byte in flash `.rodata` (0 on silicon)
//! the emulator patches to 1 in the cache window, read with `read_volatile`.

use core::future::poll_fn;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::task::Poll;

use embassy_sync::waitqueue::AtomicWaker;
use esp_hal::interrupt::software::SoftwareInterrupt;
use esp_hal::interrupt::{InterruptHandler, Priority};

/// The software interrupt the probe wakes on. esp-rtos owns 0 (its context
/// switch) and names 1 for a second core; 2 and 3 are free in this image.
pub const WAKE_SWI: u8 = 3;
/// The wake's priority: the consumers are executors, which take a waker
/// from any priority; 1 is the lowest, so a wake never preempts the RMT
/// refill (`Priority::max()`).
pub const WAKE_PRIORITY: Priority = Priority::Priority1;

/// The pending word: bit `n` = channel `n` has events.
#[unsafe(no_mangle)]
pub static LP_SEAM_PENDING: AtomicU32 = AtomicU32::new(0);

/// Engaged check (ii): 0 in flash, 1 once the emulator patches it.
#[unsafe(no_mangle)]
pub static LP_SEAM_PROBE_ENGAGED: u8 = 0;

const CHANNELS: usize = 2;
static WAKERS: [AtomicWaker; CHANNELS] = [AtomicWaker::new(), AtomicWaker::new()];
static FLAGS: [AtomicBool; CHANNELS] = [AtomicBool::new(false), AtomicBool::new(false)];
/// Per channel: events received, out-of-order arrivals, duplicates, wakes.
pub static RECEIVED: [AtomicU32; CHANNELS] = [AtomicU32::new(0), AtomicU32::new(0)];
pub static OUT_OF_ORDER: [AtomicU32; CHANNELS] = [AtomicU32::new(0), AtomicU32::new(0)];
pub static WAKES: [AtomicU32; CHANNELS] = [AtomicU32::new(0), AtomicU32::new(0)];
pub static ISR_RUNS: AtomicU32 = AtomicU32::new(0);

// # Why these bodies are asm with register operands (K1, found by this probe)
//
// A seam that takes arguments or returns a value needs more than a kept
// call. A binary's LTO internalizes even `#[no_mangle]` functions, and then
// interprocedural constant propagation and dead-argument elimination see the
// body: a first build with `let _ = (channel, buf, cap); 0` kept every CALL,
// but dropped the argument set-up at the call site and folded the result to
// 0 in the caller (`n == 0` → the drain loop never read the buffer;
// `engaged_by_query()` → `false`). So the arguments go INTO the asm, bound to
// the ABI registers, and the result comes OUT of it: the compiler can prove
// nothing about either. No `nomem` on the take: the emulator writes `buf`.

/// Engaged check (i): a hooked query. On silicon: 0.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn lp_seam_engaged(id: u32) -> u32 {
    let out: u32;
    // SAFETY: an x0 hint and a load-immediate into the result register;
    // touches nothing else. The immediate makes the body unique.
    unsafe {
        core::arch::asm!(
            "addi zero, zero, 0x300",
            "li a0, 0",
            inout("a0") id => out,
            options(nomem, nostack, preserves_flags),
        );
    }
    out
}

/// The probe's take seam. On silicon: nothing pending.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn lp_seam_probe_take(channel: u32, buf: *mut u8, cap: u32) -> u32 {
    let n: u32;
    // SAFETY: as above; not `nomem`, because the emulator's answer writes
    // up to `cap` bytes at `buf`, and the compiler must assume this did.
    unsafe {
        core::arch::asm!(
            "addi zero, zero, 0x301",
            "li a0, 0",
            inout("a0") channel => n,
            in("a1") buf,
            in("a2") cap,
            options(nostack, preserves_flags),
        );
    }
    n
}

const _: lp_seam::engaged::Signature = lp_seam_engaged;
const _: lp_seam::probe_take::Signature = lp_seam_probe_take;

/// Mechanism (i).
#[inline(never)]
pub fn engaged_by_query() -> bool {
    lp_seam_engaged(lp_seam::probe_take::ID as u32) != 0
}

/// Mechanism (ii).
#[inline(never)]
pub fn engaged_by_byte() -> bool {
    // SAFETY: a plain byte static; volatile so the compiler cannot fold the
    // flash constant (0) into a `false`.
    unsafe { core::ptr::read_volatile(&LP_SEAM_PROBE_ENGAGED) != 0 }
}

#[esp_hal::ram]
extern "C" fn wake_isr() {
    // SAFETY: the probe owns software interrupt 3 (see WAKE_SWI).
    unsafe { SoftwareInterrupt::<'static, 3>::steal() }.reset();
    let bits = LP_SEAM_PENDING.swap(0, Ordering::AcqRel);
    ISR_RUNS.fetch_add(1, Ordering::Relaxed);
    for ch in 0..CHANNELS {
        if bits & (1 << ch) != 0 {
            FLAGS[ch].store(true, Ordering::Release);
            WAKERS[ch].wake();
        }
    }
}

/// Bind the wake handler, when the probe seam is engaged. Returns whether it
/// is. Call once, from the main thread, after the runtime started.
pub fn install() -> bool {
    let by_query = engaged_by_query();
    let by_byte = engaged_by_byte();
    esp_println::println!(
        "[probe] engaged: query={by_query} byte={by_byte} (swi {WAKE_SWI}, priority {:?})",
        WAKE_PRIORITY
    );
    if !by_byte {
        return false;
    }
    // SAFETY: nothing else in this image uses software interrupt 3.
    let mut swi = unsafe { SoftwareInterrupt::<'static, 3>::steal() };
    swi.set_interrupt_handler(InterruptHandler::new(wake_isr, WAKE_PRIORITY));
    true
}

/// One consumer: wait for a wake, drain until 0, check the sequence.
#[embassy_executor::task(pool_size = 2)]
pub async fn probe_task(channel: usize) {
    let mut next: u32 = 0;
    let mut buf = [0u8; 64];
    loop {
        poll_fn(|cx| {
            WAKERS[channel].register(cx.waker());
            if FLAGS[channel].swap(false, Ordering::AcqRel) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
        let wakes = WAKES[channel].fetch_add(1, Ordering::Relaxed) + 1;
        if wakes % 256 == 0 {
            report();
        }
        loop {
            let n = lp_seam_probe_take(channel as u32, buf.as_mut_ptr(), buf.len() as u32);
            if n == 0 {
                break;
            }
            for chunk in buf[..n as usize].chunks_exact(4) {
                let seq = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if seq != next {
                    OUT_OF_ORDER[channel].fetch_add(1, Ordering::Relaxed);
                    esp_println::println!("[probe] ch{channel} expected {next} got {seq}");
                }
                next = seq.wrapping_add(1);
                RECEIVED[channel].fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// The counters, for the heartbeat.
pub fn report() {
    esp_println::println!(
        "[probe] isr={} ch0 got={} wakes={} ooo={} ch1 got={} wakes={} ooo={}",
        ISR_RUNS.load(Ordering::Relaxed),
        RECEIVED[0].load(Ordering::Relaxed),
        WAKES[0].load(Ordering::Relaxed),
        OUT_OF_ORDER[0].load(Ordering::Relaxed),
        RECEIVED[1].load(Ordering::Relaxed),
        WAKES[1].load(Ordering::Relaxed),
        OUT_OF_ORDER[1].load(Ordering::Relaxed),
    );
}
