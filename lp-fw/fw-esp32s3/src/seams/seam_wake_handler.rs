//! The wake line's handler on the S3 (`lp_seam::wake`; S3 radio M5): the
//! C6's `seam_wake_handler`, with the S3's register.
//!
//! An emulator with something for an idle guest sets bits in the pending word
//! (`fw_esp32_common::seams::seam_wake::PENDING`, the table's `pending`), then
//! raises `FROM_CPU_INTR3`. This handler, at priority 1:
//!
//! 1. **clears the line first**: writes 0 to `SYSTEM.CPU_INTR_FROM_CPU_3`
//!    (`0x600C_003C`), exactly what esp-hal's `SoftwareInterrupt::<3>::reset`
//!    writes on this chip — the S3 keeps its software interrupts in
//!    `SYSTEM`, not in an `INTPRI` as the C6 does;
//! 2. **then** swaps the pending word to zero and, when it was non-zero,
//!    wakes the network seam's two waiters
//!    (`fw_esp32_common::seams::seam_wake::take_pending`).
//!
//! A raise between the two is read by the swap, or by the next handler run;
//! never lost.
//!
//! **Bound only when a capability seam is engaged** ([`bind`]): the `wifi`
//! image's network bring-up binds it when `net=lan` engaged
//! (`fw_esp32_radio::net::net_bringup`, through `NET_CHIP.bind_seam_wake`),
//! and the `test_seam_abi` harness binds it itself. Software interrupt 3
//! is otherwise unclaimed on this chip (esp-rtos is handed software
//! interrupt 0; nothing takes 1–3).
//!
//! **In RAM.** The handler is `#[ram]` and everything it does itself is
//! inlined into it: one store, one atomic swap, a branch.

use esp_hal::interrupt::software::SoftwareInterrupt;
use esp_hal::interrupt::{InterruptHandler, Priority};
use fw_esp32_common::seams::lp_seam;

/// The wake line's handler. See [the module docs](self).
#[esp_hal::ram]
extern "C" fn wake_handler() {
    let line = esp_hal::peripherals::SYSTEM::regs()
        .cpu_intr_from_cpu(WAKE_LINE)
        .as_ptr();
    // SAFETY: a 32-bit store of 0 to `CPU_INTR_FROM_CPU_3`, a live MMIO
    // register only this handler and the emulator's raise touch. Clears the
    // line before the word is read (the wake's protocol order).
    unsafe { core::ptr::write_volatile(line, 0) };
    fw_esp32_common::seams::seam_wake::take_pending();
}

/// Bind [`wake_handler`] on `FROM_CPU_INTR3` at priority 1 and enable it.
/// Call once, only when a capability seam is engaged.
pub fn bind() {
    // SAFETY: software interrupt 3 is the wake's line and nothing else on
    // this chip claims it (esp-rtos is handed software interrupt 0); this is
    // the one place that takes it, once per boot.
    let mut line = unsafe { SoftwareInterrupt::<'static, 3>::steal() };
    line.set_interrupt_handler(InterruptHandler::new(wake_handler, Priority::Priority1));
}

/// The wake line (`lp_seam::wake::WAKE_FROM_CPU_INTR`): `FROM_CPU_INTR3`,
/// the `SoftwareInterrupt::<3>` [`bind`] steals.
const WAKE_LINE: usize = 3;

const _: () = assert!(lp_seam::wake::WAKE_FROM_CPU_INTR as usize == WAKE_LINE);
const _: () = assert!(
    lp_seam::wake::WAKE_PRIORITY == 1,
    "the wake binds at priority 1"
);
