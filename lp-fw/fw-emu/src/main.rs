//! Firmware emulator application.
//!
//! This binary runs the LightPlayer server firmware in a RISC-V 32-bit emulator,
//! allowing testing and development without physical hardware. It provides syscall-based
//! implementations for serial I/O, time, and output operations.

#![no_std]
#![no_main]

extern crate alloc;
#[cfg(feature = "test_unwind")]
extern crate unwinding;

mod fault_injection;
mod output;
mod recovery_area;
mod serial;
mod server_loop;
mod time;

use alloc::{rc::Rc, sync::Arc};
use core::cell::RefCell;

use fw_core::log::init_emu_logger;
use fw_core::transport::SerialTransport;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_riscv_emu_guest::allocator;
use lpa_server::{LpGraphics, LpServer};
use lpc_hardware::{HardwareSystem, HwManifest, HwRegistry};
use lpc_model::AsLpPath;
use lpc_shared::output::OutputProvider;
use lpfs::LpFsMemory;
use lps_builtins::host_debug;

use output::SyscallOutputProvider;
use serial::SyscallSerialIo;
use server_loop::run_server_loop;
use time::SyscallTimeProvider;

/// Main entry point for firmware emulator
///
/// This function is called by `_code_entry` from `lp-riscv-emu-guest` after
/// memory initialization (.bss and .data sections).
#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() -> ! {
    // Initialize global heap allocator
    unsafe {
        allocator::init_heap();
    }

    // Initialize logger first
    init_emu_logger();

    host_debug!("[fw-emu] Starting firmware emulator...");

    // Crash recovery: analyze the previous (simulated) run before anything
    // crash-prone. The host harness preserves the region and sets the
    // reset cause across simulated reboots.
    let reset_cause = recovery_area::boot_reset_cause();
    let (recovery_inst, boot_assessment) =
        lp_recovery::Recovery::init(recovery_area::EmuRecoveryBackend, reset_cause);
    lp_recovery::set_global(alloc::boxed::Box::leak(alloc::boxed::Box::new(
        recovery_inst,
    )));
    log::info!(
        "[fw-emu][RECOVERY] boot: cause={} level={} safe_mode={} prior_boot_complete={}",
        boot_assessment.cause.as_str(),
        boot_assessment.level.as_str(),
        boot_assessment.safe_mode,
        boot_assessment.prior_boot_complete,
    );
    let boot_guard = lp_recovery::enter(lp_recovery::FrameKind::Boot, "boot").ok();
    // Host-injected boot faults fire here, inside the Boot frame and
    // before the boot-complete milestone.
    fault_injection::check_boot_fault();

    log::info!("[fw-emu] Shader backend: native JIT (lpvm-native rt_jit)");

    // Create serial I/O first (needed for test_unwind check)
    let serial_io = SyscallSerialIo::new();

    #[cfg(feature = "test_unwind")]
    {
        use lp_riscv_emu_guest::{
            sys_serial_has_data, sys_serial_read, sys_serial_write, sys_yield,
        };

        // Check for __test_unwind command from host before entering server loop.
        // Host sends "__test_unwind\n", we run catch_unwind test and write result.
        if sys_serial_has_data() {
            let mut line = alloc::string::String::new();
            let mut buf = [0u8; 1];
            while sys_serial_has_data() {
                let n = sys_serial_read(&mut buf);
                if n <= 0 {
                    break;
                }
                if buf[0] == b'\n' {
                    break;
                }
                line.push(buf[0] as char);
            }
            if line == "__test_unwind" {
                #[inline(never)]
                fn trigger_unwind() {
                    panic!("unwind test");
                }
                let result = unwinding::panic::catch_unwind(trigger_unwind);
                let msg = match result {
                    Err(_) => "unwind: ok",
                    Ok(_) => "unwind: fail",
                };
                let _ = sys_serial_write(msg.as_bytes());
                let _ = sys_serial_write(b"\n");
                sys_yield();
            }
        }
    }

    // Create filesystem (in-memory)
    let base_fs = alloc::boxed::Box::new(LpFsMemory::new());

    let hardware_registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
    let hardware_system = Rc::new(HardwareSystem::with_virtual_drivers(hardware_registry));

    // Create output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> = Rc::new(RefCell::new(
        SyscallOutputProvider::new_with_hardware_system(Rc::clone(&hardware_system)),
    ));

    // Create server (with time provider for shader comp timing)
    let time_provider_rc = Rc::new(SyscallTimeProvider::new());
    // GLSL frontend: the emulator matches the device product constant
    // (LpsGlsl); the crate's own `naga` feature is an explicit builder
    // opt-in mirroring fw-esp32's.
    let shader_frontend = if cfg!(feature = "naga") {
        lpa_server::ShaderFrontend::Naga
    } else {
        lpa_server::DEVICE_SHADER_FRONTEND
    };
    let graphics: Arc<dyn LpGraphics> = Arc::new(TargetLpvmGraphics::new(shader_frontend));
    let button_service: Rc<dyn lpa_server::ButtonService> = hardware_system.clone();
    let radio_service: Rc<dyn lpa_server::RadioService> = hardware_system.clone();
    let server = LpServer::new_with_hardware_services(
        output_provider,
        base_fs,
        "projects/".as_path(),
        None,
        Some(time_provider_rc),
        Some(button_service),
        Some(radio_service),
        graphics,
    );

    let transport = SerialTransport::new(serial_io);

    // Create time provider for server loop frame timing
    let time_provider = SyscallTimeProvider::new();

    // Boot frame ends here; the boot-complete milestone is marked by the
    // server loop after the first successful frame.
    drop(boot_guard);

    // Run server loop (never returns)
    run_server_loop(server, transport, time_provider);
}
