//! The USB link task on a preemptive esp-rtos thread of its own (`io-thread`,
//! on by default).
//!
//! Without it the link task shares the one thread-mode executor with the
//! server loop, so it runs only in the loop's gap between frames: every
//! receive, ACK, resend timer and reply waits for the frame to finish. Here it
//! gets a thread at [`PRIORITY`] — above the main task's (0), below
//! esp-radio's — with its own [`esp_rtos::embassy::Executor`], which runs the
//! unchanged [`crate::serial::usb_link_task`]. A woken link task (USB input, a
//! timer, the doorbell) preempts the render at once. The server answers a
//! tick's requests before it renders (`LpServer::set_messages_first`), so a
//! reply is on the wire while the frame renders instead of after it; the two
//! halves only move request latency together (`lp2025/2026-10-01-1200-io-thread-spike`).
//!
//! - **Thread creation.** esp-rtos's own task creation is crate-private; the
//!   public face is [`esp_radio_rtos_driver::task_create`], the call esp-radio
//!   starts its Wi-Fi and BLE threads through, which esp-rtos implements when
//!   its `esp-radio` feature is on (`radio` here, so `io-thread` needs it).
//!   The stack comes off the heap, so the thread is created early in boot.
//! - **The lock.** The link task and the server's transport share one
//!   `lp_link::Link` (`fw_esp32_common::usb_link::usb_link_shared`). [`link_lock`]
//!   keeps them apart with a priority-limited lock at [`Priority::Priority1`]:
//!   esp-rtos's context-switch software interrupt and its timer tick are both
//!   priority 1, so no thread switch can land while either side holds the
//!   link, and the RMT refill ISR (`Priority::max()`) is never held off. The
//!   lock asserts it is not entered inside a critical section, and the shared
//!   link keeps what runs under it short.
//! - **The stack** is [`STACK_BYTES`]: the spike's high-water was
//!   1,264–1,312 B on every run, emulated and silicon, idle and under load.
//!   `io_thread_stack_diag` paints it and logs its high-water
//!   ([`crate::io_thread_stack_diag`]); a product build does neither.

use core::ffi::c_void;

use esp_hal::interrupt::Priority;
use esp_hal::sync::RawPriorityLimitedMutex;
use fw_esp32_common::usb_link::UsbLinkShared;

/// The thread's stack, bytes.
pub const STACK_BYTES: usize = 3 * 1024;
/// The thread's priority. The main task is 0; esp-radio's threads sit far
/// above.
pub const PRIORITY: u32 = 1;

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
    esp_println::println!("[INIT] io thread: stack {STACK_BYTES} B, priority {PRIORITY}");
    // SAFETY: `args` is a leaked `Box<Args>` (`Send`, asserted above) handed
    // to the new thread, which takes it back exactly once, in `entry`;
    // nothing else keeps the pointer. `entry` never returns.
    unsafe {
        esp_radio_rtos_driver::task_create(
            "lp-io",
            entry,
            args.cast::<c_void>(),
            PRIORITY,
            None,
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
        // SPIKE ONLY (emulator seams M0 part B): the probe's second consumer,
        // on this thread's executor.
        #[cfg(feature = "spike_seam_wake_probe")]
        if crate::seams::wake_probe::engaged_by_byte() {
            spawner.spawn(crate::seams::wake_probe::probe_task(1).unwrap());
        }
    })
}
