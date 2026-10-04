//! The USB link task on a preemptive esp-rtos thread of its own (`io-thread`,
//! on by default), pinned to core 0.
//!
//! The S3's copy of `fw-esp32c6/src/io_thread.rs` (a per-chip copy by
//! decision: `fw-esp32-common` names no esp-rtos thread API). Without it the
//! link task shares the one thread-mode executor with the server loop, so it
//! runs only in the loop's gap between frames: every receive, ACK, resend
//! timer and reply waits for the frame to finish. Here it gets a thread at
//! [`PRIORITY`] — above the main task's (0) — with its own
//! [`esp_rtos::embassy::Executor`], which runs the unchanged
//! [`crate::serial::usb_link_task`]. A woken link task (USB input, a timer,
//! the doorbell) preempts the render at once. The server answers a tick's
//! requests before it renders (`LpServer::set_messages_first`), so a reply is
//! on the wire while the frame renders instead of after it; the two halves
//! only move request latency together (M1, `docs/adr/2026-10-02-c6-link-io-thread.md`).
//!
//! - **Thread creation.** esp-rtos's own task creation is crate-private; the
//!   public face is [`esp_radio_rtos_driver::task_create`], which esp-rtos
//!   implements when its `esp-radio` feature is on (`io-thread` turns it on;
//!   this chip links no radio stack). The stack comes off the heap, so the
//!   thread is created early in boot.
//! - **Core 0, always.** The thread is pinned with `pin_to_core: Some(0)`,
//!   never `None`: in esp-rtos's `multi_core` build an unpinned task that is
//!   switched out can raise the software interrupt that schedules the *other*
//!   core (`esp-rtos-0.3.0/src/scheduler.rs`, `task::schedule_other_core`),
//!   and this firmware never starts core 1. Moving the thread to core 1 later
//!   is a change of pin (plus the flash-write handshake that move needs), not
//!   of lock.
//! - **The lock.** The link task and the server's transport share one
//!   `lp_link::Link` (`fw_esp32_common::usb_link::usb_link_shared`).
//!   [`link_lock`] keeps them apart with a priority-limited lock at
//!   [`Priority::Priority1`]. On core 0 it holds off esp-rtos's
//!   context-switch software interrupt and its timer tick (both priority 1,
//!   so no thread switch lands while either side holds the link) and the
//!   USB-Serial-JTAG handler (which only delays a wake); it never holds off
//!   the RMT refill ISR (`Priority::max()`, `output/rmt/shared_driver.rs`).
//!   It is already cross-core in this `multi_core` build (esp-sync takes an
//!   owner word by compare-and-swap, keyed by core id), so a later move to
//!   core 1 keeps it. The S3 runs no radio today; esp-radio's S3 Wi-Fi binds
//!   its MAC interrupt to CPU0 at priority 1
//!   (`third_party/esp-radio/src/wifi/os_adapter/esp32s3.rs`), so Wi-Fi here
//!   would put that interrupt under this lock — the radio-safe lock the C6
//!   needs (roadmap M6) would be needed here too. The lock asserts it is not
//!   entered inside a critical section, and the shared link keeps what runs
//!   under it short.
//! - **The stack** is [`STACK_BYTES`], a start above the C6's 3 KB because
//!   the Xtensa windowed ABI's frames are larger than RV32's.
//!   `io_thread_stack_diag` paints it and logs its high-water
//!   (`crate::io_thread_stack_diag`); a product build does neither.

use core::ffi::c_void;

use esp_hal::interrupt::Priority;
use esp_hal::sync::RawPriorityLimitedMutex;
use fw_esp32_common::usb_link::UsbLinkShared;

/// The thread's stack, bytes.
pub const STACK_BYTES: usize = 4 * 1024;
/// The thread's priority. The main task is 0.
pub const PRIORITY: u32 = 1;
/// The core the thread is pinned to (see the module docs: never unpinned).
pub const CORE: u32 = 0;

static LINK_LOCK: RawPriorityLimitedMutex = RawPriorityLimitedMutex::new(Priority::Priority1);

/// The cross-thread lock for the shared link (`UsbLinkShared::leak_locked`).
pub fn link_lock(f: &mut dyn FnMut()) {
    LINK_LOCK.lock(f);
}

/// What the thread takes ownership of, through `task_create`'s parameter.
struct Args {
    usb_device: esp_hal::peripherals::USB_DEVICE<'static>,
    shared: &'static UsbLinkShared,
}

/// `task_create` requires the parameter's data to be `Send`.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<Args>();
};

/// Start the link thread. It runs the USB link task on an executor of its
/// own for the life of the boot.
pub fn start(
    usb_device: esp_hal::peripherals::USB_DEVICE<'static>,
    shared: &'static UsbLinkShared,
) {
    let args = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Args { usb_device, shared }));
    esp_println::println!(
        "[INIT] io thread: stack {STACK_BYTES} B, priority {PRIORITY}, core {CORE}"
    );
    // SAFETY: `args` is a leaked `Box<Args>` (`Send`, asserted above) handed
    // to the new thread, which takes it back exactly once, in `entry`;
    // nothing else keeps the pointer. `entry` never returns.
    unsafe {
        esp_radio_rtos_driver::task_create(
            "lp-io",
            entry,
            args.cast::<c_void>(),
            PRIORITY,
            Some(CORE),
            STACK_BYTES,
        );
    }
}

extern "C" fn entry(param: *mut c_void) {
    #[cfg(feature = "io_thread_stack_diag")]
    crate::io_thread_stack_diag::paint();
    // SAFETY: `param` is the `Box<Args>` `start` leaked for this thread, and
    // this is the one place that takes it back.
    let args = unsafe { alloc::boxed::Box::from_raw(param.cast::<Args>()) };
    let Args { usb_device, shared } = *args;
    let executor =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(esp_rtos::embassy::Executor::new()));
    executor.run(move |spawner| {
        spawner.spawn(crate::serial::usb_link_task(usb_device, shared).unwrap());
    })
}
