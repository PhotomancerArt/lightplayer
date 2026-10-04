//! The UART link task on a preemptive esp-rtos thread of its own (`io-thread`,
//! on by default), pinned to core 0.
//!
//! The classic's copy of `fw-esp32c6/src/io_thread.rs` and
//! `fw-esp32s3/src/io_thread.rs` (a per-chip copy by decision:
//! `fw-esp32-common` names no esp-rtos thread API). Without it the link task
//! shares the main thread executor with the server loop, so it runs only in
//! the loop's gap between frames: the I/O task keeps moving UART0's bytes
//! every millisecond, but every ACK, resend timer, transfer frame and log
//! record the `Link` produces waits for the frame to finish. Here it gets a
//! thread at [`PRIORITY`] — above the main task's (0) — with its own
//! [`esp_rtos::embassy::Executor`], which runs the unchanged
//! [`crate::serial::uart_link_task`]. A woken link task (the I/O task's news,
//! a timer, the doorbell) preempts the render at once. The server answers a
//! tick's requests before it renders (`LpServer::set_messages_first`), so a
//! reply is on the wire while the frame renders instead of after it.
//!
//! **What does not change.** io_task — the swi2 interrupt executor at
//! Priority2, its 1 ms TIMG0 pacer, `SendUart` and the two pipes — is
//! untouched, and is still the only thing that touches UART0. The `Link` is
//! never polled from io_task's executor (ruling DD20 of plan
//! `classic-uart-on-lp-link`; `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`):
//! io_task stays a byte shuttle with an ISR-scale stack, no embassy-time and
//! no logging, and this thread is where the link's timers and logs live —
//! embassy-time works here, because this is a thread and not an interrupt
//! executor.
//!
//! - **Thread creation.** esp-rtos's own task creation is crate-private; the
//!   public face is [`esp_radio_rtos_driver::task_create`], which esp-rtos
//!   implements when its `esp-radio` feature is on (`io-thread` turns it on;
//!   the product image links no radio stack). The stack comes off the heap,
//!   so the thread is created early in boot, before the engine has
//!   fragmented it.
//! - **Core 0, always.** The thread is pinned with `pin_to_core: Some(0)`,
//!   never `None`. In esp-rtos's `multi_core` build, switching out an
//!   unpinned task raises the software interrupt that schedules the *other*
//!   core (`esp-rtos-0.3.0/src/scheduler.rs`, `run_scheduler`'s
//!   `task::schedule_other_core`), and on this board that interrupt —
//!   SWI1, `FROM_CPU_INTR1` — is the APP-core wire pusher's frame doorbell
//!   (`output/rmt/wire_pusher.rs`; finding 3 of the executor ADR). The main
//!   task is pinned to core 0 by `esp_rtos::start` (`task/mod.rs`,
//!   `allocate_main_task`), so with this thread pinned too every esp-rtos
//!   task is pinned to core 0, and esp-rtos raises only SWI0: from core 0 as
//!   its own yield, and from core 1 (were anything there ever to wake a task)
//!   as the cross-core request *to* core 0. Core 1 never runs the esp-rtos
//!   scheduler, so its time-slice target stays unarmed and the timer ISR
//!   never schedules it either.
//! - **The lock.** The link task and the server's transport share one
//!   `lp_link::Link` (`fw_esp32_common::uart_link::uart_link_shared`).
//!   [`link_lock`] keeps them apart with a priority-limited lock at
//!   [`Priority::Priority1`]. On core 0 it holds off, for each closure's
//!   microseconds: esp-rtos's context-switch software interrupt and its timer
//!   tick (both priority 1, so no thread switch lands while either side holds
//!   the link), io_task's 1 ms pacer and UART0's interrupt. It never holds
//!   off io_task's executor (Priority2), the RMT refill ISR (level 3, on
//!   core 1, or on core 0 in the single-core fallback) or the wire-pusher
//!   doorbell — interrupt masks are per core. The lock asserts it is not
//!   entered inside a critical section, and the shared link keeps what runs
//!   under it short (its short-closure rule: the 128 B RX FIFO fills in
//!   ~1.4 ms while the pacer is held off). This board has no radio.
//! - **The stack** is [`STACK_BYTES`], sized from the diagnostic build's
//!   high-water (`io_thread_stack_diag` paints it and logs `[iostack]`;
//!   [`crate::io_thread_stack_diag`]); a product build does neither. P4 of
//!   `lp2025/2026-10-02-1918-io-thread-other-boards` started at 4 KB and
//!   measured 1,660 B (`lp-emu:esp32v3:t1`, `lp-cli link rtt` on `five-wire`
//!   and on a render-bound variant of it), the same at 3 KB, and 1,692 B under
//!   the `--uart-faults` soak — 55 % of 3 KB. The figure includes whatever
//!   interrupt frames landed on this thread (the 1 ms pacer and io_task's
//!   swi2 poll nest here when they preempt it), but only those the runs
//!   happened to catch at their deepest; the desk's diagnostic image is the
//!   silicon check. Each KB here is ~11 LEDs of this board's heap.

use core::ffi::c_void;

use esp_hal::interrupt::Priority;
use esp_hal::sync::RawPriorityLimitedMutex;
use fw_esp32_common::uart_link::UartLinkShared;

/// The thread's stack, bytes.
pub const STACK_BYTES: usize = 3 * 1024;
/// The thread's priority. The main task is 0.
pub const PRIORITY: u32 = 1;
/// The core the thread is pinned to (see the module docs: never unpinned).
pub const CORE: u32 = 0;

static LINK_LOCK: RawPriorityLimitedMutex = RawPriorityLimitedMutex::new(Priority::Priority1);

/// The cross-thread lock for the shared link (`UartLinkShared::leak_locked`).
pub fn link_lock(f: &mut dyn FnMut()) {
    LINK_LOCK.lock(f);
}

/// What the thread takes ownership of, through `task_create`'s parameter.
struct Args {
    shared: &'static UartLinkShared,
}

/// `task_create` requires the parameter's data to be `Send`.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<Args>();
};

/// Start the link thread. It runs the UART link task on an executor of its
/// own for the life of the boot.
pub fn start(shared: &'static UartLinkShared) {
    let args = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Args { shared }));
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
    let Args { shared } = *args;
    let executor =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(esp_rtos::embassy::Executor::new()));
    executor.run(move |spawner| {
        spawner.spawn(
            crate::serial::uart_link_task(
                shared,
                fw_esp32_common::uart_link::PassPacing::CLASSIC_LINK_THREAD,
            )
            .unwrap(),
        );
    })
}
