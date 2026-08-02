//! Firmware emulator application.
//!
//! This binary runs the LightPlayer server firmware in a RISC-V 32-bit emulator,
//! allowing testing and development without physical hardware. It provides syscall-based
//! implementations for serial I/O, time, and output operations.

#![no_std]
#![no_main]

extern crate alloc;

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

// The build's self-description, embedded as a scannable blob (extracted by
// `lp-cli firmware show` and reported on ServerHello in M4). fw-emu reaches
// lpc-engine directly, so the engine fragment comes from there.
lpc_model::lp_embed_manifest_core! {
    package: env!("CARGO_PKG_NAME"),
    chip_family: "emu",
    chip: "rv32imac",
    cargo_target: "riscv32imac-unknown-none-elf",
    profile: if cfg!(debug_assertions) { "debug" } else { "release" },
    commit: "unknown",
    dirty: false,
    wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    features: [
        lpc_engine::features::ENGINE_FEATURE_FRAGMENT,
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::GfxLpvm),
    ],
    limits_json: "{}",
}

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

    let serial_io = SyscallSerialIo::new();

    // Create filesystem (in-memory)
    let base_fs = alloc::boxed::Box::new(LpFsMemory::new());

    // Four timing channels, as the desk S3 has: a four-strip project should
    // light four strips here too, rather than one plus three endpoints that
    // never open.
    let hardware_registry = Rc::new(HwRegistry::new(HwManifest::virtual_quad_rmt_gpio_board()));
    let hardware_system = Rc::new(HardwareSystem::with_virtual_drivers(hardware_registry));

    // Create output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> = Rc::new(RefCell::new(
        SyscallOutputProvider::new_with_hardware_system(Rc::clone(&hardware_system)),
    ));

    // Create server (with time provider for shader comp timing)
    let time_provider_rc = Rc::new(SyscallTimeProvider::new());
    // GLSL frontend: the emulator matches the device product constant
    // (LpsGlsl); the crate's own `naga` feature is an explicit builder
    // opt-in mirroring fw-esp32c6's.
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
