//! LightPlayer firmware for ESP32-S3 (Xtensa LX7).
//!
//! Boots the LightPlayer app: `LpServer` over the USB-Serial-JTAG transport,
//! backed by a littlefs filesystem in the `lpfs` partition, with abort-tier
//! crash recovery live from the first instruction.
//!
//! ## What this build deliberately does *not* have
//!
//! Every `lpa-server` node gate is **off** (see `Cargo.toml`), so a pushed
//! project loads with every node kind inert. The graphics backend is
//! `lp_gfx::NullGraphics`, so the on-device shader compiler is not linked at
//! all. There is no RMT/ws281x driver, no button, and no radio; the output
//! driver reports frames over serial instead (`output::readout_driver`).
//!
//! That is the point rather than a shortfall: the next milestone turns on
//! exactly one thing — the shader node — and a single-variable change is the
//! only kind whose result is unambiguous.
//!
//! ## Shape versus fw-esp32c6
//!
//! `boot_firmware` below is the S3's counterpart to the C6's function of the
//! same name and follows its order. What is absent is absent on purpose: no
//! `unwinding`, no `catch_unwind`, no `alloc_error_handler` routing a panic
//! through an unwinder. On the abort tier a panic is terminal for the boot, and
//! all the crash path can do is make the *next* boot able to say what died —
//! see `recovery::panic_path`.

#![no_std]
#![no_main]
// `rsr.ccount` in board::esp32s3::cycle_counter — Xtensa inline asm is still
// behind this gate upstream. Scoped to harness builds so the app path keeps a
// clean feature set.
#![cfg_attr(fw_harness, feature(asm_experimental_arch))]
#![cfg_attr(
    fw_harness,
    allow(
        unstable_features,
        reason = "asm_experimental_arch is required to read Xtensa's CCOUNT \
                  cycle counter; harness builds only"
    )
)]

// The JIT harness allocates (JIT buffers, module tables); the app path is the
// whole server stack. `test_backtrace_oracle` is the exception — it is
// deliberately allocation-free, because it exercises a walk the panic path
// takes, and the panic path must not allocate.
#[cfg(any(not(fw_harness), feature = "test_xt_jit_corpus"))]
extern crate alloc;

mod board;
#[cfg(not(fw_harness))]
mod flash_storage;
#[cfg(not(fw_harness))]
mod output;
#[cfg(not(fw_harness))]
mod recovery;
#[cfg(not(fw_harness))]
mod serial;
#[cfg(fw_harness)]
mod tests;

#[cfg(not(fw_harness))]
use {
    alloc::{boxed::Box, rc::Rc, string::String, sync::Arc},
    board::esp32s3::init::{init_board, start_runtime},
    core::cell::RefCell,
    flash_storage::{LpFlashStorage, LpfsPartition, lpfs_config},
    fw_esp32_common::hardware::manifest_loader::load_hardware_manifest,
    fw_esp32_common::server_loop::run_server_loop,
    fw_esp32_common::time::Esp32TimeProvider,
    fw_esp32_common::{boot, logger, lp_fs, transport},
    lpa_server::{LpGraphics, LpServer},
    lpc_hardware::{HardwareSystem, HwRegistry},
    lpc_shared::output::OutputProvider,
    lpfs::LpFsMemory,
    lpfs::lp_path::AsLpPath,
    output::{Esp32OutputProvider, SerialReadoutWs281xDriver},
    serial::io_task,
};

esp_bootloader_esp_idf::esp_app_desc!();

/// Heap for the allocator.
///
/// Measured on silicon at this size: ~92 KB free idle, ~66 KB free with a small
/// project loaded. Deliberately far below the C6's 300 KB — this build runs no
/// shader compiler, and the heap comes straight out of the region the main task
/// stack occupies, where the C6's own comment warns that over-reserving corrupts
/// execution before it ever reports OOM. **This is the knob to revisit when the
/// shader node and a real graphics backend arrive**, with a stack measurement
/// rather than by guessing upward.
const HEAP_SIZE: usize = 96 * 1024;

/// Abort-tier panic handler (ADR 2026-07-29-per-chip-fw-toolchains): stage a
/// breadcrumb into the `lp-recovery` RTC ledger, then reset, so the next boot
/// can report what died.
///
/// Deliberately NOT the C6's shape — that one calls `unwinding::begin_panic` so
/// `catch_unwind` can recover a failing node render, and it needs
/// `panic = "unwind"` plus retained `.eh_frame`. This chip takes the abort tier
/// instead, so a panic is terminal for the boot. See `recovery::panic_path` for
/// the rest of the reasoning, including why the C6's esp-sync reentrant-lock
/// guard has no counterpart here.
///
/// Harness builds never boot recovery — no RTC ledger is installed and there is
/// no next boot that would read one — so they take the bare print-and-reset
/// path rather than linking the whole subsystem for a no-op.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(fw_harness)]
    {
        esp_println::println!("\n\n====================== PANIC ======================");
        esp_println::println!("{info}");
        esp_hal::system::software_reset()
    }
    #[cfg(not(fw_harness))]
    recovery::panic_path::stage_and_reset(info)
}

/// Harness entrypoint. Harnesses own the peripheral singleton themselves
/// because they do not run `init_board`; the app path must not reach this
/// function (see the `esp_hal::init` note on `boot_firmware`).
#[cfg(fw_harness)]
#[esp_hal::main]
fn boot() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    esp_println::println!("[INIT] fw-esp32s3 boot");
    esp_println::println!("[INIT] chip=esp32s3 arch=xtensa heap={HEAP_SIZE}");
    esp_println::println!("[INIT] ready");

    // Both harnesses diverge, so there is no tail to park in — a harness build
    // with neither feature cannot exist (`fw_harness` is set by build.rs from
    // the presence of one).
    #[cfg(feature = "test_xt_jit_corpus")]
    tests::xt_jit_corpus::run_all();

    #[cfg(feature = "test_backtrace_oracle")]
    tests::backtrace_oracle::run_all();
}

/// Heap free/used for the heartbeat. A chip fact `fw-esp32-common` must not
/// know, so it is injected.
#[cfg(not(fw_harness))]
fn esp32_memory_stats() -> Option<(u32, u32)> {
    Some((
        esp_alloc::HEAP.free().min(u32::MAX as usize) as u32,
        esp_alloc::HEAP.used().min(u32::MAX as usize) as u32,
    ))
}

/// Everything `main` needs to hand to the server loop.
#[cfg(not(fw_harness))]
struct FirmwareApp {
    server: LpServer,
    transport: transport::StreamingMessageRouterTransport,
    time_provider: Esp32TimeProvider,
    watchdog: recovery::watchdog::WatchdogFeeder,
}

#[cfg(not(fw_harness))]
#[inline(never)]
fn boot_firmware(spawner: embassy_executor::Spawner) -> FirmwareApp {
    // ⚠️ `init_board` takes the `esp_hal` peripheral singleton, and taking it
    // twice panics. This is the app path's ONLY call to `esp_hal::init` — the
    // boot skeleton that used to call it directly here is gone, and the harness
    // entrypoint above (which never runs `init_board`) is cfg-exclusive with
    // this function. Do not add a second `esp_hal::init` anywhere on this path.
    let (sw_int, timg0, usb_device, flash, rwdt) = init_board();
    // The heap is main.rs's, not the board's — `init_board` deliberately does
    // not allocate it (unlike the C6's). Recovery leaks its instance into a
    // `&'static mut`, so it cannot run before this line.
    esp_alloc::heap_allocator!(size: HEAP_SIZE);
    esp_println::println!("[INIT] fw-esp32s3 boot");
    esp_println::println!("[INIT] chip=esp32s3 arch=xtensa heap={HEAP_SIZE}");

    // Crash recovery first, before anything crash-prone runs: this both reports
    // the previous run and gives everything after it somewhere to leave a
    // breadcrumb.
    let boot_assessment = recovery::boot_report::init_and_report();

    // Arm the RWDT here and not a line earlier: the feeder withholds its feed
    // whenever the I/O task has gone silent, so arming it before there is an
    // I/O task to spawn would reset the board every `BOOT_TIMEOUT_MS` forever.
    // Baseline 0 matches the server loop's time provider, which also starts at
    // ~0; the first io_task tick re-baselines within milliseconds.
    let watchdog = recovery::watchdog::WatchdogFeeder::start(rwdt, 0);
    let boot_guard = lp_recovery::enter(lp_recovery::FrameKind::Boot, "boot").ok();

    start_runtime(timg0, sw_int);
    esp_println::println!("[INIT] runtime started");

    spawner.spawn(io_task(usb_device).unwrap());
    esp_println::println!("[INIT] I/O task spawned");

    // From here on `log::*` reaches the host over the same serial link; the
    // `esp_println!` lines above are the pre-transport ones.
    logger::init(serial::io_task::log_write_to_outgoing);

    let (incoming, _) = serial::io_task::get_message_channels();
    let (write_request, write_result) = serial::io_task::get_server_write_channels();
    let transport =
        transport::StreamingMessageRouterTransport::new(incoming, write_request, write_result);

    let base_fs = mount_filesystem(flash);

    // The compiled-in fallback is the XIAO ESP32-S3 Plus profile — the desk
    // board. It is deliberately partial (no user LED, no castellated pads); see
    // `default_esp32s3_hardware_manifest`. An `/hardware.json` on the device
    // overrides it, which is how a different S3 carrier gets described.
    let hardware_manifest = load_hardware_manifest(
        base_fs.as_ref(),
        lpc_hardware::default_esp32s3_hardware_manifest,
    );
    log::info!(
        "[fw-esp32s3] hardware manifest: {} ({})",
        hardware_manifest.board_id(),
        hardware_manifest.board_name()
    );
    let hardware_registry = Rc::new(HwRegistry::new(hardware_manifest));
    let mut hardware_system = HardwareSystem::new(Rc::clone(&hardware_registry));
    hardware_system.add_ws281x_driver(Box::new(SerialReadoutWs281xDriver::new(Rc::clone(
        &hardware_registry,
    ))));
    // No button and no radio driver: neither is ported, and `LpServer` takes
    // both services as `Option`, so they are simply absent rather than stubbed.
    let hardware_system = Rc::new(hardware_system);

    // The provider itself is chip-agnostic and comes from fw-esp32-common
    // untouched; only the driver registered above is chip-side.
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(Esp32OutputProvider::new(hardware_system)));

    // Stamped device identity: read the fs-root `/.lp/device.json` once at boot
    // for the hello (missing file → unstamped, `None`).
    let device_uid = lpa_server::device_identity::read_device_uid(base_fs.as_ref());

    // ⚠️ `NullGraphics`, never `TargetLpvmGraphics`. `LpServer` requires a
    // backend, and the real one links the entire on-device JIT compiler into an
    // image that would never call it — 743,216 B measured on the C6. See the
    // `lp-gfx` dependency line.
    let graphics: Arc<dyn LpGraphics> = Arc::new(lp_gfx::NullGraphics::new());

    let time_provider_rc = Rc::new(Esp32TimeProvider::new());
    let mut server = LpServer::new_with_hardware_services(
        output_provider,
        base_fs,
        "projects/".as_path(),
        Some(esp32_memory_stats),
        Some(time_provider_rc),
        None,
        None,
        graphics,
    );
    server.set_hello(lpc_wire::ServerHello {
        proto: lpc_wire::WIRE_PROTO_VERSION,
        fw: lpc_wire::FwProvenance {
            package: String::from("fw-esp32s3"),
            commit: String::from(env!("LP_BUILD_COMMIT")),
            dirty: env!("LP_BUILD_DIRTY") == "true",
            profile: String::from(env!("LP_BUILD_PROFILE")),
        },
        device_uid,
    });

    // Auto-load a project at boot — unless repeated incomplete boots put us in
    // safe mode, in which case the server comes up reachable but nothing
    // crash-prone is loaded.
    if boot_assessment.safe_mode {
        let incomplete_boots = lp_recovery::snapshot()
            .map(|s| s.consecutive_incomplete_boots)
            .unwrap_or(0);
        log::error!(
            "[RECOVERY] SAFE MODE: {incomplete_boots} consecutive incomplete boots — skipping project auto-load"
        );
    } else {
        boot::auto_load_project(&mut server);
    }

    let time_provider = Esp32TimeProvider::new();

    // Boot frame ends here. The boot-complete milestone is NOT marked here: the
    // server loop marks it after the first successfully served frame, which is
    // a far stronger claim than "reached the end of boot".
    drop(boot_guard);

    FirmwareApp {
        server,
        transport,
        time_provider,
        watchdog,
    }
}

/// Mount the `lpfs` partition, falling back to RAM so an unformattable or
/// mis-flashed board still comes up reachable and can say so over the wire.
#[cfg(not(fw_harness))]
fn mount_filesystem(flash: esp_hal::peripherals::FLASH<'static>) -> Box<dyn lpfs::LpFs> {
    let mut flash_storage = esp_storage::FlashStorage::new(flash);
    let Some(partition) = LpfsPartition::locate(&mut flash_storage) else {
        // Not a runtime condition: it means the image was flashed without
        // `--partition-table lp-fw/fw-esp32s3/partitions.csv` and espflash
        // silently substituted its own default. Say so rather than guess an
        // offset and erase running code.
        esp_println::println!(
            "[ERROR] no `lpfs` partition in the flashed table — reflash with \
             --partition-table lp-fw/fw-esp32s3/partitions.csv; using memory FS"
        );
        return Box::new(LpFsMemory::new());
    };
    match lp_fs::LpFsFlash::init(LpFlashStorage::new(flash_storage, partition), lpfs_config) {
        Ok(fs) => {
            esp_println::println!("[INIT] flash filesystem mounted");
            Box::new(fs)
        }
        Err(e) => {
            esp_println::println!("[WARN] flash FS failed: {e}, falling back to memory");
            Box::new(LpFsMemory::new())
        }
    }
}

#[cfg(not(fw_harness))]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) {
    let app = boot_firmware(spawner);

    // ⚠️ The substring "fw-esp32 initialized, starting server loop" is matched
    // literally by `lpa_link::device_session::device_readiness` (and by lp-cli
    // fwcheck through it). It is chip-agnostic on purpose — writing
    // "fw-esp32s3" here would leave the host waiting for a readiness line that
    // never arrives. The provenance suffix is additive only.
    esp_println::println!(
        "[INIT] fw-esp32 initialized, starting server loop... proto={} commit={} dirty={}",
        lpc_wire::WIRE_PROTO_VERSION,
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
    );

    let mut watchdog = app.watchdog;
    run_server_loop(
        app.server,
        app.transport,
        app.time_provider,
        esp32_memory_stats,
        move |now_ms| watchdog.feed(now_ms),
    )
    .await;
}
