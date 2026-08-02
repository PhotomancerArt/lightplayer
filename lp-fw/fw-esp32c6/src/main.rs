//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![allow(
    unstable_features,
    reason = "alloc_error_handler required for custom OOM handler in no_std"
)]

extern crate alloc;
#[allow(
    unused_extern_crates,
    reason = "unwinding is used for panic recovery; extern crate needed for no_std"
)]
extern crate unwinding;

use core::alloc::Layout;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

// The build's self-description, embedded as a scannable blob (extracted by
// `lp-cli firmware show` and reported on ServerHello in M4). Feature truth
// comes from the engine's own cfg! derivation; only embedder facts are named
// here. `flashAppBytes` is parsed from partitions.csv by build.rs.
#[cfg(feature = "server")]
lpc_model::lp_embed_manifest_core! {
    package: env!("CARGO_PKG_NAME"),
    chip_family: "esp32",
    chip: "esp32c6",
    cargo_target: "riscv32imac-unknown-none-elf",
    profile: env!("LP_BUILD_PROFILE"),
    commit: env!("LP_BUILD_COMMIT"),
    dirty: lpc_model::manifest::str_eq(env!("LP_BUILD_DIRTY"), "true"),
    wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    features: [
        lpa_server::ENGINE_FEATURE_FRAGMENT,
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::GfxLpvm),
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::SvcButton),
        lpc_model::manifest::feature_fragment(
            cfg!(feature = "radio"),
            lpc_model::LpFeature::SvcRadioEspnow,
        ),
    ],
    limits_json: concat!("{\"flashAppBytes\":", env!("LP_FLASH_APP_BYTES"), "}"),
}

const OOM_STATE_NORMAL: u8 = 0;
const OOM_STATE_UNWINDING: u8 = 1;
const OOM_STATE_RECURSIVE: u8 = 2;

static OOM_STATE: AtomicU8 = AtomicU8::new(OOM_STATE_NORMAL);
static OOM_ALLOC_SIZE: AtomicUsize = AtomicUsize::new(0);
static OOM_ALLOC_ALIGN: AtomicUsize = AtomicUsize::new(0);
static OOM_FREE_BYTES: AtomicUsize = AtomicUsize::new(0);
static OOM_USED_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Custom panic handler that starts stack unwinding via the `unwinding` crate.
///
/// In no_std, `panic!()` routes directly to `#[panic_handler]` — there is no automatic
/// unwinding step. We must explicitly call `begin_panic` to start unwinding so that
/// `catch_unwind` (used for panic recovery in node render) can catch panics.
///
/// Before unwinding, a crash breadcrumb is staged into the `lp-recovery` persistent
/// region (zero-alloc). If no `catch_unwind` catches the panic, the breadcrumb is
/// committed and the device reboots via `software_reset` (see `fatal_reset_or_hang`);
/// the next boot reads the region and reports what crashed. See the crate `recovery`
/// module and `docs/adr/2026-07-04-crash-recovery-model.md`.
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");
    print_panic_frames();
    esp_println::println!();

    // Stage a breadcrumb in the persistent recovery region NOW (zero-alloc):
    // if unwinding fails or the device hangs mid-unwind, the next boot can
    // still report what happened. Layer-1 recovery voids it on catch.
    stage_recovery_crash(info);

    if is_esp_sync_reentrant_lock_panic(info) {
        esp_println::println!(
            "fatal: esp-sync lock reentry while panicking; aborting without heap allocation"
        );
        fatal_reset_or_hang();
    }

    let payload: alloc::boxed::Box<dyn core::any::Any + Send> = {
        #[cfg(feature = "server")]
        {
            let (file, line) = if let Some(loc) = info.location() {
                (Some(loc.file()), Some(loc.line()))
            } else {
                (None, None)
            };
            if OOM_STATE.load(Ordering::Relaxed) == OOM_STATE_UNWINDING {
                alloc::boxed::Box::new(lpc_shared::backtrace::PanicPayload::new_oom(
                    info.message(),
                    file,
                    line,
                    lpc_shared::backtrace::OomInfo {
                        requested: OOM_ALLOC_SIZE.load(Ordering::Relaxed),
                        align: OOM_ALLOC_ALIGN.load(Ordering::Relaxed),
                        free: OOM_FREE_BYTES.load(Ordering::Relaxed),
                        used: OOM_USED_BYTES.load(Ordering::Relaxed),
                        context: lpc_shared::backtrace::oom_context(),
                    },
                ))
            } else {
                alloc::boxed::Box::new(lpc_shared::backtrace::PanicPayload::new(
                    info.message(),
                    file,
                    line,
                ))
            }
        }
        #[cfg(not(feature = "server"))]
        {
            struct Dummy;
            alloc::boxed::Box::new(Dummy)
        }
    };
    OOM_STATE.store(OOM_STATE_NORMAL, Ordering::Relaxed);
    let code = unwinding::panic::begin_panic(payload);

    // begin_panic returns if no catch_unwind was found on the stack.
    esp_println::println!("unwinding failed: code={}", code.0);
    fatal_reset_or_hang();
}

/// Stage the panic into the recovery breadcrumb region. Zero-alloc; a no-op
/// when no recovery global is installed (test builds, pre-init panics).
fn stage_recovery_crash(info: &PanicInfo) {
    let is_oom = OOM_STATE.load(Ordering::Relaxed) == OOM_STATE_UNWINDING;
    let cause = if is_oom {
        lp_recovery::CrashCause::Oom
    } else {
        lp_recovery::CrashCause::Panic
    };
    let oom = is_oom.then(|| lp_recovery::OomStats {
        requested: OOM_ALLOC_SIZE.load(Ordering::Relaxed) as u32,
        align: OOM_ALLOC_ALIGN.load(Ordering::Relaxed) as u32,
        free: OOM_FREE_BYTES.load(Ordering::Relaxed) as u32,
        used: OOM_USED_BYTES.load(Ordering::Relaxed) as u32,
    });
    let location = info.location().map(|loc| (loc.file(), loc.line()));
    let mut frames = [0u32; lpc_shared::backtrace::MAX_FRAMES];
    let count = lpc_shared::backtrace::capture_frames(&mut frames);
    let message = info.message();
    lp_recovery::stage_crash(cause, &message, location, &frames[..count], oom);
}

/// Dead-end failure: commit the staged breadcrumb and reset. When a
/// recovery global is installed (real firmware boots), this diverges via
/// `software_reset`; otherwise (test features, panics before recovery
/// init) it preserves the old hang-in-place behavior so dev boards don't
/// boot-loop.
fn fatal_reset_or_hang() -> ! {
    let _ = lp_recovery::finalize_crash_and_reset();
    loop {}
}

fn is_esp_sync_reentrant_lock_panic(info: &PanicInfo) -> bool {
    info.location()
        .is_some_and(|loc| loc.file().contains("esp-sync/src/lib.rs"))
}

fn print_panic_frames() {
    let mut frames = [0; lpc_shared::backtrace::MAX_FRAMES];
    let count = lpc_shared::backtrace::capture_frames(&mut frames);
    if count == 0 {
        return;
    }

    esp_println::print!("frames:");
    for frame in frames.iter().take(count) {
        esp_println::print!(" 0x{:08x}", frame);
    }
    esp_println::println!();
    esp_println::print!("decode: just decode-backtrace");
    for frame in frames.iter().take(count) {
        esp_println::print!(" 0x{:08x}", frame);
    }
    esp_println::println!();
}

/// Custom OOM handler that panics normally so catch_unwind can recover.
/// The default alloc_error_handler uses nounwind panic and cannot be caught.
#[alloc_error_handler]
fn on_alloc_error(layout: Layout) -> ! {
    if OOM_STATE
        .compare_exchange(
            OOM_STATE_NORMAL,
            OOM_STATE_UNWINDING,
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
        .is_err()
    {
        OOM_STATE.store(OOM_STATE_RECURSIVE, Ordering::Relaxed);
        esp_println::println!("\n\n====================== OOM ======================");
        esp_println::println!(
            "allocation failed while building OOM panic payload: requested={} align={} original_requested={} original_free={} original_used={}",
            layout.size(),
            layout.align(),
            OOM_ALLOC_SIZE.load(Ordering::Relaxed),
            OOM_FREE_BYTES.load(Ordering::Relaxed),
            OOM_USED_BYTES.load(Ordering::Relaxed),
        );
        lp_recovery::stage_crash(
            lp_recovery::CrashCause::Oom,
            &"recursive OOM while building panic payload",
            None,
            &[],
            Some(lp_recovery::OomStats {
                requested: layout.size() as u32,
                align: layout.align() as u32,
                free: OOM_FREE_BYTES.load(Ordering::Relaxed) as u32,
                used: OOM_USED_BYTES.load(Ordering::Relaxed) as u32,
            }),
        );
        fatal_reset_or_hang();
    }

    let free = esp_alloc::HEAP.free();
    let used = esp_alloc::HEAP.used();
    OOM_ALLOC_SIZE.store(layout.size(), Ordering::Relaxed);
    OOM_ALLOC_ALIGN.store(layout.align(), Ordering::Relaxed);
    OOM_FREE_BYTES.store(free, Ordering::Relaxed);
    OOM_USED_BYTES.store(used, Ordering::Relaxed);
    esp_println::println!("\n\n====================== OOM ======================");
    esp_println::println!(
        "allocation failed: requested={} align={} free={} used={} context={}",
        layout.size(),
        layout.align(),
        free,
        used,
        lpc_shared::backtrace::oom_context().unwrap_or("<unset>"),
    );
    panic!("memory allocation of {} bytes failed", layout.size());
}

mod board;
#[cfg(not(fw_harness))]
use fw_esp32_common::boot;
#[cfg(any(not(fw_harness), feature = "test_button", feature = "test_espnow"))]
mod hardware;
pub use fw_esp32_common::logger;
// jit_fns (JIT host-log symbol) now lives in fw-esp32-common; linked via the
// extern reference from the JIT builtin table.
// The app, plus the five harnesses that light a strip. `test_gpio` used to be
// in this list and drives pins directly, so it only pulled in an output tree
// nothing in that build touches.
#[cfg(any(
    not(fw_harness),
    feature = "test_rmt",
    feature = "test_dither",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_fluid_demo",
))]
mod output;
mod recovery;
mod serial;
#[cfg(all(any(feature = "stress_s2", feature = "stress_s3"), not(fw_harness)))]
mod stress;
#[cfg(not(fw_harness))]
use fw_esp32_common::server_loop;
#[cfg(not(fw_harness))]
use fw_esp32_common::time;
#[cfg(all(feature = "server", not(fw_harness),))]
use fw_esp32_common::transport;

#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
mod bootctl;
#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
mod flash_storage;
#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
use fw_esp32_common::lp_fs;

#[cfg(all(
    feature = "radio",
    not(any(feature = "stress_s2", feature = "stress_s3")),
    not(fw_harness)
))]
use hardware::espnow_radio_driver::Esp32EspNowRadioDriver;
#[cfg(not(fw_harness))]
use lpfs::lp_path::AsLpPath;
#[cfg(not(fw_harness))]
use {
    alloc::{boxed::Box, rc::Rc, sync::Arc},
    board::esp32c6::init::{init_board, start_runtime},
    core::cell::RefCell,
    hardware::button::Esp32GpioButtonDriver,
    hardware::manifest_loader::load_hardware_manifest,
    lp_gfx_lpvm::TargetLpvmGraphics,
    lpa_server::{ButtonService, LpGraphics, LpServer},
    lpc_hardware::{HardwareSystem, HwRegistry},
    lpc_shared::output::OutputProvider,
    lpfs::LpFsMemory,
    output::{Esp32C6RmtWs281xDriver, Esp32OutputProvider},
    serial::io_task,
    server_loop::run_server_loop,
    time::Esp32TimeProvider,
};

#[cfg(fw_harness)]
mod tests {
    #[cfg(feature = "test_f32_softfloat")]
    pub mod f32_softfloat;
    #[cfg(feature = "test_fluid_demo")]
    pub mod fluid_demo;
    #[cfg(feature = "test_shader_compile_incremental")]
    pub mod incremental_shader_compile;
    #[cfg(feature = "test_jit_math_perf")]
    pub mod jit_math_perf;
    #[cfg(any(feature = "test_msafluid", feature = "test_fluid_demo"))]
    pub mod msafluid_solver;
    #[cfg(feature = "test_button")]
    pub mod test_button;
    #[cfg(feature = "test_dither")]
    pub mod test_dither;
    #[cfg(feature = "test_espnow")]
    pub mod test_espnow;
    #[cfg(feature = "test_gpio")]
    pub mod test_gpio;
    #[cfg(feature = "test_gpio_calibrate")]
    pub mod test_gpio_calibrate;
    #[cfg(feature = "test_json")]
    pub mod test_json;
    #[cfg(feature = "test_msafluid")]
    pub mod test_msafluid;
    #[cfg(feature = "test_rmt")]
    pub mod test_rmt;
    #[cfg(feature = "test_usb")]
    pub mod test_usb;
}

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(not(fw_harness))]
fn esp32_memory_stats() -> Option<(u32, u32)> {
    Some((
        esp_alloc::HEAP.free().min(u32::MAX as usize) as u32,
        esp_alloc::HEAP.used().min(u32::MAX as usize) as u32,
    ))
}

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
    // TODO: esp_println writes directly to USB-Serial-JTAG hardware, bypassing
    // io_task's connection monitor. May block if no USB host is connected during
    // boot. Hasn't been observed yet but worth investigating if boot hangs occur.

    // Initialize board (clock, heap, runtime) and get hardware peripherals
    esp_println::println!("[INIT] Initializing board...");
    let (sw_int, timg0, rmt_peripheral, usb_device, _gpio18, flash, _gpio4, _gpio20, wifi, rwdt) =
        init_board();
    esp_println::println!("[INIT] Board initialized, starting runtime...");

    // Crash recovery: analyze the previous run (reset reason + persistent
    // breadcrumb region) before anything crash-prone runs, then arm the
    // hardware watchdog so hangs from here on are attributable.
    let reset_cause = recovery::current_reset_cause();
    let (recovery_inst, boot_assessment) =
        lp_recovery::Recovery::init(recovery::Esp32RecoveryBackend::take(), reset_cause);
    lp_recovery::set_global(Box::leak(Box::new(recovery_inst)));
    recovery::log_boot_assessment(&boot_assessment);
    // Baseline 0 matches the server loop's time provider, which also starts
    // at ~0; the first io_task tick re-baselines within milliseconds.
    let watchdog = recovery::watchdog::WatchdogFeeder::start(rwdt, 0);
    let boot_guard = lp_recovery::enter(lp_recovery::FrameKind::Boot, "boot").ok();

    start_runtime(timg0, sw_int);
    esp_println::println!("[INIT] Runtime started");

    // Note: USB serial is handled by I/O task for transport
    // Logging will go through the transport serial (non-M! messages)
    // or can be disabled if USB host is not connected
    esp_println::println!("[INIT] fw-esp32 starting...");

    // Spawn I/O task (handles serial communication)
    esp_println::println!("[INIT] Spawning I/O task...");
    spawner.spawn(io_task(usb_device).unwrap());
    esp_println::println!("[INIT] I/O task spawned");

    // Initialize log crate to write to outgoing serial (host will see these)
    crate::logger::init(serial::io_task::log_write_to_outgoing);

    log::info!("[fw-esp32c6] Shader backend: native JIT (lpvm-native rt_jit)");

    #[cfg(feature = "test_oom")]
    {
        // Test 1: simple panic (not OOM) — validates basic unwinding
        esp_println::println!("[test_oom] Test 1: catching simple panic...");
        let r1 = unwinding::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            panic!("test panic");
        }));
        match r1 {
            Ok(_) => esp_println::println!("[test_oom] Test 1 FAIL: panic was not caught"),
            Err(_) => esp_println::println!("[test_oom] Test 1 OK: simple panic caught"),
        }

        // Test 2: OOM inside catch_unwind
        esp_println::println!("[test_oom] Test 2: catching OOM...");
        let r2 = unwinding::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            let mut vecs: alloc::vec::Vec<alloc::vec::Vec<u8>> = alloc::vec::Vec::new();
            loop {
                vecs.push(alloc::vec![0u8; 64 * 1024]);
            }
        }));
        match r2 {
            Ok(_) => esp_println::println!("[test_oom] Test 2 FAIL: did not OOM"),
            Err(_) => esp_println::println!("[test_oom] Test 2 OK: OOM caught, recovery works"),
        }

        esp_println::println!("[test_oom] Tests complete, continuing boot...");
    }

    // Create serial transport. Project-read responses stream through io_task;
    // small messages use the simpler full-message serializer.
    esp_println::println!("[INIT] Creating StreamingMessageRouterTransport...");
    let (incoming, _) = serial::io_task::get_message_channels();
    let (write_request, write_result) = serial::io_task::get_server_write_channels();
    let transport =
        transport::StreamingMessageRouterTransport::new(incoming, write_request, write_result);
    esp_println::println!("[INIT] StreamingMessageRouterTransport created");

    // The RMT peripheral becomes the WS281x driver's, clock and all. 80 MHz
    // with the per-channel divider of 1 gives the 12.5 ns tick
    // `lp_ws281x::PulseCodes` assumes — the same pair the legacy C6 driver's
    // `config.rs` encoded and drove strips with since the project started.
    esp_println::println!("[INIT] Initializing RMT peripheral at 80MHz...");
    let rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, output::rmt::shared_driver::RMT_CLOCK)
        .expect("Failed to initialize RMT");
    esp_println::println!("[INIT] RMT peripheral initialized");

    // Boot-control sector: a flash-persisted instruction from a previous run
    // or from the host over esptool. Read (and consumed) before the
    // filesystem mounts, because it must survive the power cycle that wipes
    // the RTC recovery region — see docs/adr/2026-07-30-boot-control-sector.md.
    #[cfg(not(feature = "memory_fs"))]
    let (boot_control, flash) = {
        let mut flash_storage = esp_storage::FlashStorage::new(flash);
        let outcome = crate::bootctl::read_and_consume(&mut flash_storage);
        (outcome, flash_storage)
    };
    #[cfg(feature = "memory_fs")]
    let boot_control = lp_bootctl::DecodeOutcome::Blank;

    // Create filesystem before hardware providers so /hardware.json can override board policy.
    let base_fs: Box<dyn lpfs::LpFs> = {
        #[cfg(not(feature = "memory_fs"))]
        {
            let flash_storage = flash;
            match lp_fs::LpFsFlash::init(
                crate::flash_storage::LpFlashStorage::new(flash_storage),
                crate::flash_storage::lpfs_config,
            ) {
                Ok(fs) => {
                    esp_println::println!("[INIT] Flash filesystem mounted");
                    Box::new(fs)
                }
                Err(e) => {
                    esp_println::println!("[WARN] Flash FS failed: {e}, falling back to memory");
                    Box::new(LpFsMemory::new())
                }
            }
        }
        #[cfg(feature = "memory_fs")]
        {
            let _ = flash;
            esp_println::println!("[INIT] Creating in-memory filesystem...");
            Box::new(LpFsMemory::new())
        }
    };
    #[cfg(feature = "memory_fs")]
    esp_println::println!("[INIT] In-memory filesystem created");

    let hardware_manifest = load_hardware_manifest(
        base_fs.as_ref(),
        lpc_hardware::default_esp32c6_hardware_manifest,
    );
    log::info!(
        "[fw-esp32c6] Hardware manifest: {} ({})",
        hardware_manifest.board_id(),
        hardware_manifest.board_name()
    );
    let hardware_registry = Rc::new(HwRegistry::new(hardware_manifest));
    let mut hardware_system = HardwareSystem::new(Rc::clone(&hardware_registry));
    // How many outputs appear is decided in two places and nowhere else: the
    // board manifest's `/rmt/ws281xK` resources (two on the XIAO C6), and
    // `output::rmt::c6_rmt::BLOCKS_PER_CHANNEL` = 1, which leaves both of the
    // chip's RMT TX slots usable. Under the `ws281x_2blocks` feature a channel
    // takes two blocks, absorbs its neighbour's, and only slot 0 remains;
    // manifest channel K drives slot K * SLOT_STRIDE and absorbed slots are
    // never configured.
    hardware_system.add_ws281x_driver(Box::new(Esp32C6RmtWs281xDriver::new(
        Rc::clone(&hardware_registry),
        rmt,
    )));
    hardware_system.add_button_driver(Box::new(Esp32GpioButtonDriver::new(Rc::clone(
        &hardware_registry,
    ))));
    #[cfg(all(
        feature = "radio",
        not(any(feature = "stress_s2", feature = "stress_s3"))
    ))]
    {
        let radio_driver = Esp32EspNowRadioDriver::new(Rc::clone(&hardware_registry), wifi)
            .expect("Failed to initialize ESP-NOW radio");
        log::info!(
            "[fw-esp32c6] ESP-NOW radio ready: device_id={:?} channel={}",
            radio_driver.device_id(),
            radio_driver.default_channel()
        );
        hardware_system.add_radio_driver(Box::new(radio_driver));
    }
    // P4 stress builds: the radio stack becomes a load generator instead of a
    // driver — `esp_radio::wifi::new` can only run once, and the stress tasks
    // own its controller/interface. See `stress.rs`.
    #[cfg(any(feature = "stress_s2", feature = "stress_s3"))]
    stress::start(spawner, wifi);
    #[cfg(all(
        not(feature = "radio"),
        not(any(feature = "stress_s2", feature = "stress_s3"))
    ))]
    let _ = wifi;
    let hardware_system = Rc::new(hardware_system);

    // Initialize output provider
    esp_println::println!("[INIT] Creating output provider...");
    let output_provider = Esp32OutputProvider::new(Rc::clone(&hardware_system));

    let output_provider: Rc<RefCell<dyn OutputProvider>> = Rc::new(RefCell::new(output_provider));
    esp_println::println!("[INIT] Output provider created");

    // Stamped device identity: read the fs-root `/.lp/device.json` once at
    // boot for the hello (missing file → unstamped, `None`). Hello REQUESTS
    // re-read it inside lpa-server, so a post-stamp hello is fresh anyway.
    let device_uid = lpa_server::device_identity::read_device_uid(base_fs.as_ref());

    // Create server (with time provider for shader comp timing). RV32 uses lpvm-native rt_jit.
    esp_println::println!("[INIT] Creating LpServer instance...");
    let time_provider_rc = Rc::new(Esp32TimeProvider::new());
    // GLSL frontend: the device ships lpa_server::DEVICE_SHADER_FRONTEND
    // (LpsGlsl). The crate's own `naga` feature is an explicit builder
    // opt-in (just demo-esp32c6-*-naga) switching this binary to the naga
    // frontend — a leaf-binary feature the builder chooses, immune to
    // workspace feature unification.
    let shader_frontend = if cfg!(feature = "naga") {
        lpa_server::ShaderFrontend::Naga
    } else {
        lpa_server::DEVICE_SHADER_FRONTEND
    };
    let graphics: Arc<dyn LpGraphics> = Arc::new(TargetLpvmGraphics::new(shader_frontend));
    let button_service: Rc<dyn ButtonService> = hardware_system.clone();
    let radio_service: Rc<dyn lpa_server::RadioService> = hardware_system.clone();
    let mut server = LpServer::new_with_hardware_services(
        output_provider,
        base_fs,
        "projects/".as_path(),
        Some(esp32_memory_stats),
        Some(time_provider_rc),
        Some(button_service),
        Some(radio_service),
        graphics,
    );
    // Wire hello identity: compile-time provenance from build.rs, injected
    // into the server (sans-IO: the server never reads env/git itself),
    // plus the boot-time read of the root-stamped device identity. The
    // hello's CAPABILITY half is derived inside the constructor above from
    // the engine's gates and the services just injected — never restated
    // here.
    server.set_hello_identity(
        lpc_wire::HelloIdentity::new(
            "fw-esp32c6",
            env!("LP_BUILD_COMMIT"),
            env!("LP_BUILD_DIRTY") == "true",
            env!("LP_BUILD_PROFILE"),
        )
        .with_device_uid(device_uid),
    );
    esp_println::println!("[INIT] LpServer created");

    // Auto-load project at boot (from config or lexical-first) — unless
    // something asks us not to. Two independent reasons can skip it, and the
    // log always says which one applied:
    //
    // - the boot-control sector (a host wrote "start once without loading a
    //   project", or a previous run latched it), which survives a power cycle;
    // - repeated incomplete boots, tracked in the RTC recovery region, which
    //   does not.
    //
    // The boot-control record was already consumed by the read, so this is a
    // one-shot: the next boot loads normally unless something says otherwise
    // again.
    // The boot-control record's action comes pre-decided by lp-bootctl's
    // precedence rule (clamp wins over skip — a dim, visible board beats a
    // dark one). An explicit user instruction outranks the ladder: the user
    // may be recovering exactly the loop the ladder saw.
    match boot_control.boot_action() {
        lp_bootctl::BootAction::LoadClamped { level } => {
            log::error!(
                "[BOOTCTL] SAFE MODE: output clamped to {level}/255 — loading the project dimmed"
            );
            server.set_safe_output_clamp(Some(level));
            boot::auto_load_project(&mut server);
        }
        lp_bootctl::BootAction::SkipAutoload => {
            log::error!("[BOOTCTL] SAFE BOOT: boot-control record — skipping project auto-load");
        }
        lp_bootctl::BootAction::Normal if boot_assessment.safe_mode => {
            let incomplete_boots = lp_recovery::snapshot()
                .map(|s| s.consecutive_incomplete_boots)
                .unwrap_or(0);
            log::error!(
                "[RECOVERY] SAFE MODE: {incomplete_boots} consecutive incomplete boots — skipping project auto-load"
            );
        }
        lp_bootctl::BootAction::Normal => {
            boot::auto_load_project(&mut server);
        }
    }

    // Create time provider
    esp_println::println!("[INIT] Creating time provider...");
    let time_provider = Esp32TimeProvider::new();
    esp_println::println!("[INIT] Time provider created");

    // Boot frame ends here; the boot-complete milestone is marked by the
    // server loop after the first successful frame.
    drop(boot_guard);

    FirmwareApp {
        server,
        transport,
        time_provider,
        watchdog,
    }
}

#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) {
    #[cfg(feature = "test_gpio")]
    {
        use tests::test_gpio::run_gpio_test;
        run_gpio_test(spawner).await;
    }

    #[cfg(feature = "test_gpio_calibrate")]
    {
        use tests::test_gpio_calibrate::run_gpio_calibration_test;
        run_gpio_calibration_test(spawner).await;
    }

    #[cfg(feature = "test_button")]
    {
        use tests::test_button::run_button_test;
        run_button_test(spawner).await;
    }

    #[cfg(feature = "test_rmt")]
    {
        use tests::test_rmt::run_rmt_test;
        run_rmt_test(spawner).await;
    }

    #[cfg(feature = "test_dither")]
    {
        use tests::test_dither::run_dithering_test;
        run_dithering_test(spawner).await;
    }

    #[cfg(feature = "test_usb")]
    {
        use tests::test_usb::run_usb_test;
        run_usb_test(spawner).await;
    }

    #[cfg(feature = "test_json")]
    {
        use tests::test_json::run_test_json;
        run_test_json(spawner).await;
    }

    #[cfg(feature = "test_msafluid")]
    {
        use tests::test_msafluid::run_msafluid_test;
        run_msafluid_test(spawner).await;
    }

    #[cfg(feature = "test_fluid_demo")]
    {
        use tests::fluid_demo::runner::run_fluid_demo;
        run_fluid_demo(spawner).await;
    }

    #[cfg(feature = "test_jit_math_perf")]
    {
        use tests::jit_math_perf::run_jit_math_perf;
        run_jit_math_perf(spawner).await;
    }

    #[cfg(feature = "test_shader_compile_incremental")]
    {
        use tests::incremental_shader_compile::run_incremental_shader_compile;
        run_incremental_shader_compile(spawner).await;
    }

    #[cfg(feature = "test_espnow")]
    {
        use tests::test_espnow::run_espnow_test;
        run_espnow_test(spawner).await;
    }

    #[cfg(feature = "test_f32_softfloat")]
    {
        use tests::f32_softfloat::run_f32_softfloat_test;
        run_f32_softfloat_test(spawner).await;
    }

    #[cfg(not(fw_harness))]
    {
        let app = boot_firmware(spawner);
        // Keep the marker substring "fw-esp32c6 initialized, starting server
        // loop" intact: two readiness classifiers grep for it
        // (lpa-studio-core browser_serial_readiness, lp-cli fwcheck). The
        // version suffix is additive only.
        esp_println::println!(
            "[INIT] fw-esp32 initialized, starting server loop... proto={} commit={} dirty={}",
            lpc_wire::WIRE_PROTO_VERSION,
            env!("LP_BUILD_COMMIT"),
            env!("LP_BUILD_DIRTY"),
        );

        // Run server loop (never returns)
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
}
