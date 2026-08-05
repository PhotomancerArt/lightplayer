//! The APP-core pusher deployment: mailboxes, the doorbell, and the loop
//! that replaced `waiti`-only idling.
//!
//! The scheduler itself is [`lp_ws281x::Pusher`] — chip-free, host-tested,
//! Miri-modelled. This module is everything the deployment adds on the
//! classic ESP32:
//!
//! * the `static` per-wire [`WireMailbox`] array the PRO-core outputs post
//!   into;
//! * the **doorbell** (software interrupt `FROM_CPU_INTR1` — interrupt 0 is
//!   the Embassy executor's): `waiti` wakes only on interrupts, and with
//!   nothing transmitting a mailbox post from the PRO core would otherwise
//!   never wake the pusher — the first frame after an idle period would
//!   sleep forever;
//! * the **lost-wakeup guard**: an interrupt landing between the pusher's
//!   last scan and its `waiti` would be consumed *before* the wait and the
//!   wakeup lost, so every wake source sets [`WAKE_PENDING`] and the idle
//!   path masks interrupts (`rsil`), re-checks the flag, and lets `waiti`
//!   atomically unmask-and-wait. The mask spans two instructions — the
//!   "nothing on this core ever masks interrupts" contract in
//!   `shared_driver` amends to "never masks across work"; the ~ns window
//!   here cannot blow an 80 µs refill deadline.
//!
//! Slot channels are published by the RMT driver's init on the PRO core —
//! later than this core boots — so [`run`] idles behind the doorbell until
//! [`publish_slots`] rings it.
//!
//! Everything on this path is `#[esp_hal::ram]`: the PRO core's flash
//! traffic stalls a core-1 flash fetch, and while a *stalled pusher* is only
//! a delayed wave (thread context has no deadline), the idle/wake machinery
//! is cheap to keep resident. `with_app_core_stalled` (flash writes) freezes
//! the pusher mid-instruction and resumes it in place; no protocol state
//! straddles the stall in a way a resume cannot finish.

use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize};

use esp_hal::interrupt::software::SoftwareInterrupt;
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::peripherals::Interrupt;
use lp_ws281x::{PadOps, Pusher, WireMailbox};

use super::shared_driver::DRIVER;
use super::v3_rmt::{self, POOLED_SLOT_CAP, TX_CHANNELS};

/// One mailbox per manifest wire — the entire PRO⇄APP shared surface.
pub static MAILBOXES: [WireMailbox; TX_CHANNELS] = [const { WireMailbox::new() }; TX_CHANNELS];

/// Slot channels the block plan configured, published by driver init.
static SLOT_CHANNELS: [AtomicU8; POOLED_SLOT_CAP] = [const { AtomicU8::new(0) }; POOLED_SLOT_CAP];
static SLOT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Set by every wake source (RMT trampoline, doorbell) so the idle path can
/// close the scan→`waiti` window. See the module docs.
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);

/// Dual-core concurrency cap — the measured-clean ISR duty maximum. The
/// single-core fallback never runs the pusher, so its cap 2 never meets
/// this constant.
const PUSHER_CAP: usize = 4;

/// Publish the configured slot channels to the pusher and wake it. Called
/// once, from the RMT driver's init on the PRO core.
pub fn publish_slots(channels: &[u8]) {
    let count = channels.len().min(POOLED_SLOT_CAP);
    for (slot, &ch) in SLOT_CHANNELS.iter().zip(channels) {
        slot.store(ch, Relaxed);
    }
    SLOT_COUNT.store(count, Release);
    ring_doorbell();
}

/// Wake the pusher from the PRO core: raise `FROM_CPU_INTR1` on the APP
/// core. Call after every mailbox post or request.
///
/// SAFETY (steal): software interrupt 1 is used by exactly this pair of
/// functions — 0 belongs to the Embassy executor, 2/3 are unused — and
/// `raise`/`reset` are single register writes with no shared state.
#[esp_hal::ram]
pub fn ring_doorbell() {
    unsafe { SoftwareInterrupt::<1>::steal() }.raise();
}

/// Bind the doorbell handler into the calling core's matrix. Called from
/// `app_core_main`, ON the APP core, alongside the RMT bind.
pub(super) fn bind_doorbell() {
    esp_hal::interrupt::bind_handler(
        Interrupt::FROM_CPU_INTR1,
        InterruptHandler::new(doorbell_isr, Priority::Priority1),
    );
}

/// Note a wake from the RMT trampoline (every service pass may have
/// completed a frame the pusher should harvest).
#[esp_hal::ram]
pub(super) fn note_wake() {
    WAKE_PENDING.store(true, Release);
}

/// The doorbell handler: acknowledge and flag. Waking `waiti` is its entire
/// job.
#[esp_hal::ram]
extern "C" fn doorbell_isr() {
    // SAFETY: see `ring_doorbell`.
    unsafe { SoftwareInterrupt::<1>::steal() }.reset();
    WAKE_PENDING.store(true, Release);
}

/// [`PadOps`] over the GPIO matrix. Runs only on the pusher thread; the
/// lease preconditions of the two `v3_rmt` calls are met because every
/// gpio the pusher ever sees arrived through a post from a lease-holding
/// output, and a close (which precedes lease release) forgets the binding
/// before the ack — see the register rules in `v3_rmt`.
struct MatrixPads;

impl PadOps for MatrixPads {
    fn route_to(&mut self, slot_channel: u8, gpio: u8) {
        v3_rmt::route_rmt_to_gpio(slot_channel, gpio);
    }
    fn park(&mut self, gpio: u8) {
        v3_rmt::park_gpio(gpio);
    }
}

/// The pusher's clock: µs since boot, wrapping u32 — the queue-wait
/// measurement's end edge (the poster stamps the start edge at post).
#[esp_hal::ram]
fn now_us() -> u32 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_micros() as u32
}

/// The APP core's forever-loop: schedule until quiet, then idle behind the
/// doorbell. Replaces the bare `waiti` loop of the pre-pusher deployment —
/// and like it, this function must NEVER return (a returned core-1 entry is
/// hardware-parked; see `app_core_main`).
#[esp_hal::ram]
pub(super) fn run() -> ! {
    // Slots arrive when the RMT driver initialises on the PRO core.
    while SLOT_COUNT.load(Acquire) == 0 {
        idle_once();
    }
    let count = SLOT_COUNT.load(Acquire).min(POOLED_SLOT_CAP);
    let mut channels = [0u8; POOLED_SLOT_CAP];
    for (slot, ch) in channels.iter_mut().zip(SLOT_CHANNELS.iter()) {
        *slot = ch.load(Relaxed);
    }

    let mut pusher: Pusher<'static, _, _, _, TX_CHANNELS, TX_CHANNELS> = Pusher::new(
        &DRIVER,
        &MAILBOXES,
        MatrixPads,
        now_us,
        &channels[..count],
        PUSHER_CAP,
    );
    loop {
        while pusher.service() {}
        idle_once();
    }
}

/// Idle until a wake source fires, without losing a wakeup that lands
/// between the caller's last scan and the wait.
///
/// The mask level (3) covers every vectored priority esp-hal dispatches
/// (`Priority::max()` is `Priority3` on Xtensa), so no wake source can slip
/// in between the flag check and `waiti` — which atomically drops the mask
/// to 0 and waits, taking any pended interrupt immediately.
#[esp_hal::ram]
fn idle_once() {
    // SAFETY (asm): `rsil` raises PS.INTLEVEL to 3 (masking the wake
    // sources) and clobbers only the scratch register; `waiti 0` lowers it
    // to 0 and waits — the architecturally atomic unmask-and-wait. The
    // masked window is the flag check alone.
    unsafe {
        core::arch::asm!("rsil {0}, 3", out(reg) _);
    }
    if WAKE_PENDING.swap(false, Acquire) {
        unsafe {
            core::arch::asm!("rsil {0}, 0", out(reg) _);
        }
    } else {
        unsafe {
            core::arch::asm!("waiti 0");
        }
    }
}
