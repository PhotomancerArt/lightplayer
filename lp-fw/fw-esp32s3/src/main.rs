//! LightPlayer firmware for ESP32-S3 (Xtensa LX7).
//!
//! Boots the LightPlayer app: `LpServer` over the USB-Serial-JTAG transport,
//! backed by a littlefs filesystem in the `lpfs` partition, with abort-tier
//! crash recovery live from the first instruction.
//!
//! ## What this build has, and what it deliberately does not
//!
//! Two of eight `lpa-server` node gates are on (see `Cargo.toml`):
//! `node-shader`, which is the point, and `node-fixture`, which is the only
//! runtime that turns the shader's visual product into the control product the
//! output node consumes. Every other kind loads inert.
//!
//! The graphics backend is the real `TargetLpvmGraphics`, so GLSL pushed to
//! this board is compiled to **Xtensa machine code on the board** and executed
//! from RAM.
//!
//! Output is real: `output::rmt` drives WS281x strips from the RMT peripheral
//! on up to four channels at once, over the portable `lp-ws281x` transmitter.
//! The board-manifest-driven GPIO button is real too (`hardware::button`).
//! Still absent: the radio, a future milestone.
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
// whole server stack. `test_button` also needs it: the button driver's
// registry/endpoint types are alloc-based, same as on fw-esp32c6.
// `test_backtrace_oracle` is the exception — it is deliberately
// allocation-free, because it exercises a walk the panic path takes, and the
// panic path must not allocate.
#[cfg(any(not(fw_harness), feature = "test_xt_jit_corpus", feature = "test_button"))]
extern crate alloc;

mod board;
#[cfg(not(fw_harness))]
mod flash_storage;
// Not simply `not(fw_harness)`: the `test_button` harness drives the same
// registry-facing driver the app path registers, and a self-test against a
// different driver would prove nothing.
#[cfg(any(not(fw_harness), feature = "test_button"))]
mod hardware;
// Not simply `not(fw_harness)`: the `test_loopback` harness drives the same RMT
// backend and the same shared driver the app path does, and a self-test against
// a different transmitter would prove nothing. The registry-facing driver
// inside is still app-only.
#[cfg(any(not(fw_harness), feature = "test_loopback"))]
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
    hardware::button::Esp32GpioButtonDriver,
    lp_gfx_lpvm::TargetLpvmGraphics,
    lpa_server::{ButtonService, LpGraphics, LpServer},
    lpc_hardware::{HardwareSystem, HwRegistry},
    lpc_shared::output::OutputProvider,
    lpfs::LpFsMemory,
    lpfs::lp_path::AsLpPath,
    output::{Esp32OutputProvider, Esp32S3RmtWs281xDriver},
    serial::io_task,
};

esp_bootloader_esp_idf::esp_app_desc!();

/// Heap for the allocator.
///
/// Raised from M3's 96 KB **on a measured failure, not a guess**: with the
/// shader node on, the first on-device compile died in `handle_alloc_error`
/// (`memory allocation of 3072 bytes failed`, inside `shader-compile:glsl`),
/// and the recovery ledger then quarantined the frame after three crashes.
/// 96 KB was chosen when nothing on this board allocated in anger; compiling
/// GLSL to Xtensa does.
///
/// The size is not free-floating — it is one side of a zero-sum split. esp-hal's
/// `stack.x` gives `.stack` whatever is left of `dram_seg`
/// (`0x3FC88000..0x3FCDB700`, 341,760 B), so every byte added here is a byte
/// taken from the stack. Asking for too much fails at *link* time ("cannot move
/// location counter backwards") rather than silently, which is the one mercy
/// here: 300 KB — the C6's figure — does not link on this chip.
///
/// This split leaves 52,896 B of stack against fw-esp32c6's proven 35,784 B
/// (both read off the linked ELFs), which is the margin the Xtensa windowed
/// ABI's larger frames deserve. The heartbeat's free-heap figure is the number
/// to watch if a future node kind pushes it.
///
/// The next lever, if one is needed, is `dram2_seg`
/// (`0x3FCDB700..0x3FCED710`, ~72 KB) as a second `esp_alloc` region — not
/// taking more from the stack.
const HEAP_SIZE: usize = 240 * 1024;

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
    // ⚠️ `CpuClock::max()` is required, not cosmetic. esp-hal's S3
    // `CpuClock::default()` is **80 MHz** — only `max()` is 240 MHz — while
    // `board::esp32s3::constants::CPU_HZ` (the divisor in
    // `cycle_counter::cycles_to_us`) hardcodes 240 MHz. Booting the harness at
    // the default would make every cycle→µs figure it prints understate real
    // elapsed time by 3×. This matches the app path's `init_board`. The
    // loopback harness additionally needs the fast clock to drain four RX
    // transactions inside a 24-item (30 µs) capture window.
    let peripherals =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    esp_println::println!("[INIT] fw-esp32s3 boot");
    esp_println::println!("[INIT] chip=esp32s3 arch=xtensa heap={HEAP_SIZE}");
    esp_println::println!("[INIT] ready");

    // Every harness diverges, so there is no tail to park in — a harness build
    // with no feature cannot exist (`fw_harness` is set by build.rs from the
    // presence of one).
    #[cfg(feature = "test_loopback")]
    tests::loopback::run(peripherals);

    #[cfg(feature = "test_xt_jit_corpus")]
    {
        drop(peripherals);
        tests::xt_jit_corpus::run_all();
    }

    // The button driver claims its pin through the hardware registry lease
    // (`AnyPin::steal`, same as the WS281x driver), not through a peripheral
    // handed down from here, so the raw singleton is simply dropped.
    #[cfg(feature = "test_button")]
    {
        drop(peripherals);
        tests::test_button::run();
    }

    #[cfg(feature = "test_backtrace_oracle")]
    {
        drop(peripherals);
        tests::backtrace_oracle::run_all();
    }
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
    let (sw_int, timg0, usb_device, flash, rwdt, rmt_peripheral) = init_board();
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

    // The RMT peripheral becomes the WS281x driver's, clock and all. 80 MHz
    // with divider 1 gives the 12.5 ns tick `lp_ws281x::PulseCodes` assumes; a
    // failure here is a clock-tree problem, and it costs the board its output
    // rather than its boot, so it is logged and not fatal.
    match esp_hal::rmt::Rmt::new(rmt_peripheral, output::rmt::shared_driver::RMT_CLOCK) {
        Ok(rmt) => {
            hardware_system.add_ws281x_driver(Box::new(Esp32S3RmtWs281xDriver::new(
                Rc::clone(&hardware_registry),
                rmt,
            )));
        }
        Err(error) => {
            esp_println::println!("[ERROR] RMT init failed ({error:?}); no LED output this boot");
        }
    }
    hardware_system.add_button_driver(Box::new(Esp32GpioButtonDriver::new(Rc::clone(
        &hardware_registry,
    ))));
    // Still no radio driver: `LpServer` takes it as an `Option`, so it is
    // simply absent rather than stubbed. Radio is a future milestone.
    let hardware_system = Rc::new(hardware_system);

    // The provider itself is chip-agnostic and comes from fw-esp32-common
    // untouched; only the driver registered above is chip-side. Cloned, not
    // moved: `hardware_system` is also the button service handed to
    // `LpServer` below.
    let output_provider: Rc<RefCell<dyn OutputProvider>> = Rc::new(RefCell::new(
        Esp32OutputProvider::new(Rc::clone(&hardware_system)),
    ));

    // Stamped device identity: read the fs-root `/.lp/device.json` once at boot
    // for the hello (missing file → unstamped, `None`).
    let device_uid = lpa_server::device_identity::read_device_uid(base_fs.as_ref());

    // The on-device JIT. `TargetLpvmGraphics` resolves to `lpvm-native`'s
    // `NativeJitEngine` on Xtensa, so a shader pushed to this board is compiled
    // to Xtensa machine code here and executed from RAM — no host step.
    //
    // ⚠️ The frontend is passed, never defaulted. `LpGraphics::glsl_frontend`
    // deliberately has no default impl so that every host states its choice;
    // the device ships `LpsGlsl`, and silently taking Naga would change what
    // the shader means without changing a line of it.
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));

    let time_provider_rc = Rc::new(Esp32TimeProvider::new());
    let button_service: Rc<dyn ButtonService> = hardware_system.clone();
    let mut server = LpServer::new_with_hardware_services(
        output_provider,
        base_fs,
        "projects/".as_path(),
        Some(esp32_memory_stats),
        Some(time_provider_rc),
        Some(button_service),
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
