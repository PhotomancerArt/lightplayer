//! Host-requested fault execution, inside real recovery frames.
//!
//! Faults run through `catch_node_panic_framed` — the same wrapper the
//! engine uses around node execution — so injected crashes exercise the
//! full stack: frame entry/gating, `unwinding` panic + catch, breadcrumb
//! staging (guest panic handler), recovered-crash ledger accounting, and
//! for uncaught faults the finalize-and-reset path.
//!
//! Only the host test harness can trigger these (it must write the fault
//! word into `LP_RECOVERY_AREA` in guest RAM); the code is inert otherwise.

extern crate alloc;

use alloc::string::ToString;
use lp_riscv_emu_shared::recovery_handshake as hs;
use lpc_engine::node::NodeError;
use lpc_engine::node::catch_node_panic::catch_node_panic_framed;

use crate::recovery_area;

/// Check for a pending host fault request and execute it. Called once per
/// server-loop frame.
pub fn check_and_run_pending_fault() {
    let Some((code, arg)) = recovery_area::take_fault() else {
        return;
    };
    let result = run_fault(code, arg);
    recovery_area::set_fault_result(result);
}

/// Boot-time variant: only executes [`hs::FAULT_BOOT_PANIC`] (leaves other
/// requests pending for the server loop).
pub fn check_boot_fault() {
    // SAFETY of peek-then-take: single-threaded guest.
    let Some((code, arg)) = recovery_area::take_fault() else {
        return;
    };
    if code == hs::FAULT_BOOT_PANIC {
        panic!("injected boot panic");
    }
    // Not a boot fault: put the result of running it at boot anyway — the
    // frame context is the boot path, which is fine for these tests.
    let result = run_fault(code, arg);
    recovery_area::set_fault_result(result);
}

fn child_name(arg: u32) -> &'static str {
    match arg {
        1 => "fault/a",
        2 => "fault/b",
        _ => "fault/c",
    }
}

fn run_fault(code: u32, arg: u32) -> u32 {
    match code {
        hs::FAULT_RECOVERED_PANIC => in_frames(arg, || -> Result<(), NodeError> {
            begin_guest_panic("injected panic");
        }),
        hs::FAULT_OOM_PANIC => in_frames(arg, || -> Result<(), NodeError> {
            begin_guest_panic("memory allocation of 999999999 bytes failed (injected oom)");
        }),
        hs::FAULT_HANG => {
            // Enter frames eagerly, then never return: the host's fuel
            // budget expires (the emulator analog of the hardware
            // watchdog) with the frames live in the region.
            let _parent = lp_recovery::enter(lp_recovery::FrameKind::NodeRender, "fault-parent");
            let _child = lp_recovery::enter(lp_recovery::FrameKind::NodeRender, child_name(arg));
            loop {
                core::hint::spin_loop();
            }
        }
        hs::FAULT_HARD_PANIC => {
            // Frames stay live (leaked guards), and there is no catch
            // boundary here: the panic escapes to the panic handler, which
            // commits the staged breadcrumb and requests a reset.
            match lp_recovery::enter(lp_recovery::FrameKind::NodeRender, "fault-parent") {
                Ok(parent) => core::mem::forget(parent),
                Err(_) => return hs::FAULT_RESULT_GATED,
            }
            match lp_recovery::enter(lp_recovery::FrameKind::NodeRender, child_name(arg)) {
                Ok(child) => core::mem::forget(child),
                Err(_) => return hs::FAULT_RESULT_GATED,
            }
            begin_guest_panic("injected hard panic");
        }
        hs::FAULT_CLEAN_CHILD => in_frames(arg, || Ok(())),
        hs::FAULT_BOOT_PANIC => {
            // Reached only if requested after boot; treat like a hard panic.
            begin_guest_panic("injected boot panic");
        }
        _ => hs::FAULT_RESULT_NONE,
    }
}

/// Run `f` inside parent+child recovery frames via the engine's boundary
/// wrapper, classifying the outcome for the host.
fn in_frames(arg: u32, f: impl FnOnce() -> Result<(), NodeError>) -> u32 {
    let outcome =
        catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, "fault-parent", || {
            catch_node_panic_framed(lp_recovery::FrameKind::NodeRender, child_name(arg), f)
        });
    match outcome {
        Ok(()) => hs::FAULT_RESULT_OK,
        Err(error) => {
            let message = error.to_string();
            if message.starts_with("recovery:") {
                hs::FAULT_RESULT_GATED
            } else {
                hs::FAULT_RESULT_ERROR
            }
        }
    }
}

/// Panic through the real machinery: `panic!` → guest `#[panic_handler]`
/// (stages the breadcrumb) → `begin_panic` unwinding.
fn begin_guest_panic(message: &'static str) -> ! {
    panic!("{message}");
}
