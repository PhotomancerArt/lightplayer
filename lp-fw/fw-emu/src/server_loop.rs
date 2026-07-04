//! Server loop for emulator firmware
//!
//! Main loop that runs in the emulator and streams server responses through a
//! transport.

use crate::serial::SyscallSerialIo;
use crate::time::SyscallTimeProvider;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use fw_core::transport::SerialTransport;
use fw_core::{drain_client_messages, tick_server_frame};
use log;
use lp_riscv_emu_guest::sys_yield;
use lpa_server::LpServer;
use lpc_shared::time::TimeProvider;

/// Block on a future until completion. Uses sys_yield when pending.
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = unsafe {
        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});
        Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE))
    };
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => sys_yield(),
        }
    }
}

/// Run the server loop
///
/// This is the main loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to host after each tick using SYSCALL_YIELD.
pub fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<SyscallSerialIo>,
    time_provider: SyscallTimeProvider,
) -> ! {
    let mut last_tick = time_provider.now_ms();
    let mut boot_completed = false;

    loop {
        // Host-injected faults execute inside recovery frames, before the
        // normal frame work.
        crate::fault_injection::check_and_run_pending_fault();

        let frame_start = time_provider.now_ms();

        log::debug!(
            "run_server_loop: Starting server loop iteration (time: {}ms)",
            frame_start
        );

        let drained = block_on(drain_client_messages(&mut transport));
        if let Some(error) = drained.error {
            log::warn!("run_server_loop: Transport error: {error:?}");
        }
        log::trace!(
            "run_server_loop: Collected {} messages this loop iteration",
            drained.messages.len()
        );

        let tick = block_on(tick_server_frame(
            &mut server,
            &mut transport,
            &time_provider,
            frame_start,
            last_tick,
            drained.messages,
        ));
        if let Some(error) = tick.server_error {
            log::warn!("run_server_loop: Server tick error: {error:?}");
        } else if !boot_completed {
            boot_completed = true;
            lp_recovery::mark_boot_complete();
            log::info!("[fw-emu][RECOVERY] boot complete (first frame served)");
        } else {
            log::trace!(
                "run_server_loop: Server sent {} response(s)",
                tick.response_count
            );
        }

        last_tick = frame_start;

        // Yield control back to host
        // This allows the host to process serial output, update time, add serial input, etc.
        sys_yield();
    }
}
