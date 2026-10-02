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

use core::alloc::Layout;
use core::panic::PanicInfo;

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

/// Abort-tier panic handler: stage a breadcrumb into the RTC ledger, commit
/// it, report on serial, reset. See `recovery::panic_path` for why the ledger
/// is written before anything is printed, and for what the unwinding-tier
/// handler that used to live here cost.
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    write_link_text_mark();
    recovery::panic_path::stage_and_reset(info)
}

/// Allocation failure: record it as an `Oom` with the heap counters attached.
///
/// Rust's default handler panics, which the handler above would catch — but by
/// then the request size is only a formatted string and the free/used numbers
/// are gone. "How much was left, and was it in one piece" is the question an
/// OOM report has to answer, so it gets its own path.
#[alloc_error_handler]
fn on_alloc_error(layout: Layout) -> ! {
    write_link_text_mark();
    recovery::panic_path::stage_oom_and_reset(layout)
}

/// lp-link's text mark, before any raw text on a dying board: `0xFF` never
/// occurs inside a COBS-FF frame, so it abandons whatever frame the panic
/// interrupted on the host's side, and the panic report (and the ROM banner
/// after the reset) arrives as text. The comms lab proved it on silicon (M3);
/// its `0x00` predecessor did not survive — the rebooted board's first frame
/// followed too soon for the idle flush. The images whose host link is
/// lp-link write it; the other harnesses print plain text and need none.
#[inline(always)]
fn write_link_text_mark() {
    #[cfg(any(not(fw_harness), feature = "test_json", feature = "test_comms_lab"))]
    esp_println::Printer::write_bytes(&[0xFF, b'\r', b'\n']);
}

#[cfg(all(feature = "ble", not(fw_harness)))]
mod ble;
mod board;
mod c_heap;
#[cfg(all(feature = "desk_espnow_meter", not(fw_harness)))]
mod desk_espnow_meter;
#[cfg(not(fw_harness))]
use fw_esp32_common::boot;
#[cfg(any(
    not(fw_harness),
    feature = "test_button",
    feature = "test_espnow",
    feature = "test_espnow_broadcast",
    feature = "test_gpio_input",
))]
mod hardware;
#[cfg(all(feature = "heap_map_diag", not(fw_harness)))]
mod heap_map;
pub use fw_esp32_common::logger;
// jit_fns (JIT host-log symbol) now lives in fw-esp32-common; linked via the
// extern reference from the JIT builtin table.
// The app, plus the five harnesses that light a strip. `test_gpio` used to be
// in this list and drives pins directly, so it only pulled in an output tree
// nothing in that build touches.
#[cfg(all(lp_split, not(fw_harness)))]
mod ota;
#[cfg(any(
    not(fw_harness),
    feature = "test_rmt",
    feature = "test_rmt_rx",
    feature = "test_dither",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_fluid_demo",
))]
mod output;
mod recovery;
#[cfg(all(feature = "diag_secure_link", not(fw_harness)))]
mod secure_link_probe;
mod serial;
#[cfg(not(fw_harness))]
mod stack_probe;
#[cfg(all(any(feature = "stress_s2", feature = "stress_s3"), not(fw_harness)))]
mod stress;
#[cfg(not(fw_harness))]
use fw_esp32_common::server_loop;
#[cfg(not(fw_harness))]
use fw_esp32_common::time;

// The benchmark images (`bench_render_loop`): the shipped boot with a seeded
// filesystem and a bounded loop. Not under `tests/` — see `bench/mod.rs`.
#[cfg(all(feature = "bench_render_loop", not(fw_harness)))]
mod bench;
#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
mod bootctl;
#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
mod flash_storage;
#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
use fw_esp32_common::lp_fs;

#[cfg(all(
    feature = "radio",
    not(any(
        feature = "stress_s2",
        feature = "stress_s3",
        feature = "desk_espnow_meter"
    )),
    not(fw_harness)
))]
use hardware::espnow_radio_driver::Esp32EspNowRadioDriver;
#[cfg(not(fw_harness))]
use lpfs::lp_path::AsLpPath;
#[cfg(not(fw_harness))]
use {
    alloc::{boxed::Box, rc::Rc, sync::Arc},
    board::esp32c6::board_quirks::apply_board_quirks,
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
    serial::usb_link_task,
    time::Esp32TimeProvider,
};

// The unbounded loop is the product's entry point; the benchmark image calls
// `run_server_loop_bounded` instead and would carry this as an unused import.
#[cfg(all(not(feature = "bench_render_loop"), not(fw_harness)))]
use server_loop::run_server_loop;

#[cfg(fw_harness)]
mod tests {
    #[cfg(feature = "test_comms_lab")]
    pub mod comms_lab;
    #[cfg(feature = "test_cycle_probe")]
    pub mod cycle_probe;
    #[cfg(feature = "test_espnow_broadcast")]
    pub mod espnow_broadcast;
    #[cfg(feature = "test_f32_softfloat")]
    pub mod f32_softfloat;
    #[cfg(feature = "test_fluid_demo")]
    pub mod fluid_demo;
    #[cfg(feature = "test_gpio_input")]
    pub mod gpio_input;
    #[cfg(feature = "test_shader_compile_incremental")]
    pub mod incremental_shader_compile;
    #[cfg(feature = "test_jit_math_perf")]
    pub mod jit_math_perf;
    #[cfg(any(feature = "test_msafluid", feature = "test_fluid_demo"))]
    pub mod msafluid_solver;
    #[cfg(feature = "test_rmt_rx")]
    pub mod rmt_rx;
    #[cfg(feature = "test_ble")]
    pub mod test_ble;
    #[cfg(feature = "test_ble_coex")]
    pub mod test_ble_coex;
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
    #[cfg(feature = "test_uart_bridge")]
    pub mod uart_bridge;
}

esp_bootloader_esp_idf::esp_app_desc!();

/// Largest-free-block probe for the server's ProjectRead headroom gate
/// (refusal-not-reset).
/// Heartbeat memory report: free/used plus the largest allocatable block
/// (this chip has the probe; it has no retrying allocator, so that field
/// stays absent).
#[cfg(not(fw_harness))]
fn heartbeat_memory_stats() -> Option<lpc_wire::server::MemoryStats> {
    // Piggybacks on the heartbeat cadence: one scan of the main stack per
    // second, a log line only when the mark grows.
    stack_probe::log_if_grown("heartbeat");
    esp32_memory_stats().map(|(free_bytes, used_bytes)| lpc_wire::server::MemoryStats {
        free_bytes,
        used_bytes,
        total_bytes: used_bytes.saturating_add(free_bytes),
        largest_free_block: read_headroom_probe(),
        oom_retry_saves: None,
    })
}

/// This chip's ProjectRead memory gate (`lpa_server::ReadGate`): refuse a
/// read when under 40 KiB is free in total or no 16 KiB block is left.
///
/// Measured on the emulated C6 with Bluetooth on (`lp-emu:esp32c6:t1@4caa5b658`,
/// plan `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`, REPORT.md):
///
/// - a read's working set is 8.3–25.1 KB (the first sync's skeleton read is
///   the worst; the editor's repeating reads are 12–15.6 KB), and it keeps
///   0–176 B; 40 KiB is that plus room for the link and radio tasks;
/// - the largest single ask any read makes is 8 KB (a mapping file's slot
///   JSON, once `lpc-wire` sizes it exactly), 2.5 KB for the editor's reads;
///   16 KiB is twice it.
///
/// The old single floor (a 32 KiB block) refused 52 of 96 editor reads after
/// ten shader edits, because Bluetooth holds this chip's second heap region
/// to a ~19.5 KB block once the main region fragments; this gate refused
/// none, with no resets (`lp-cli/tests/emu_frag_reads.rs` holds that in CI).
///
/// ⚠️ Until reads cap their own allocations (the plan's PR B), a board whose
/// largest block is 16–32 KiB can reset, instead of refusing, on a read
/// with a single ask above ~15 KB: a slot value over ~15 KB of JSON, a
/// display layout over ~800 lamps, a render probe over ~2,048 px.
#[cfg(not(fw_harness))]
const READ_GATE: lpa_server::ReadGate = lpa_server::ReadGate {
    min_free_bytes: 40 * 1024,
    min_largest_block_bytes: 16 * 1024,
};

#[cfg(not(fw_harness))]
fn read_headroom_probe() -> Option<u32> {
    #[cfg(feature = "heap_map_diag")]
    heap_map::log_periodic("probe");
    Some(recovery::panic_path::largest_free_block().min(u32::MAX as usize) as u32)
}

/// The login challenge's randomness: the C6's hardware RNG. Its output is
/// true-random while the radio runs (the product's default image brings
/// ESP-NOW up), and still unpredictable enough for a single-use 32-byte
/// nonce when it does not — a nonce needs uniqueness, not secrecy.
#[cfg(not(fw_harness))]
fn fill_random(buf: &mut [u8]) {
    esp_hal::rng::Rng::new().read(buf);
}

/// The `ClientRequest::Reboot` action: the chip reset the chip-agnostic
/// server cannot perform itself.
///
/// Called after the ack is queued (`LpServer::tick_and_send`); on the lp-link
/// host link queued is not yet delivered, so the reset is left to the link
/// task, which does it once the host has acknowledged everything (or after a
/// second) — the client reads its answer, then this board's boot banner.
/// Not a crash path: the boot was marked complete on the first served frame,
/// long before any request could arrive, so this reset never counts toward
/// the boot-loop safe-mode gate.
#[cfg(not(fw_harness))]
fn reboot_now() {
    log::info!("[REBOOT] client requested a restart");
    fw_esp32_common::usb_link::when_drained(reset_now);
}

/// The reboot itself, run by the link task once the answer is out.
#[cfg(not(fw_harness))]
fn reset_now() -> ! {
    esp_hal::system::software_reset()
}

#[cfg(not(fw_harness))]
fn esp32_memory_stats() -> Option<(u32, u32)> {
    Some((
        esp_alloc::HEAP.free().min(u32::MAX as usize) as u32,
        esp_alloc::HEAP.used().min(u32::MAX as usize) as u32,
    ))
}

/// The server's transport: the USB host link (lp-link) alone, or — with the
/// `ble` feature — the link mux carrying it plus up to two BLE links. The mux
/// is there whether or not the device store turns BLE on; with BLE off no
/// radio link ever opens and every frame takes the USB path.
#[cfg(all(feature = "ble", not(fw_harness)))]
type AppTransport = fw_esp32_common::radio_link::LinkMuxTransport<
    fw_esp32_common::usb_link::UsbLinkTransport,
    embassy_time::Delay,
>;
#[cfg(all(not(feature = "ble"), not(fw_harness)))]
type AppTransport = fw_esp32_common::usb_link::UsbLinkTransport;

#[cfg(not(fw_harness))]
struct FirmwareApp {
    server: LpServer,
    transport: AppTransport,
    time_provider: Esp32TimeProvider,
    watchdog: recovery::watchdog::WatchdogFeeder,
    /// What `auto_load_project` cost, in cycles — parse, resolve, map and
    /// above all the shader compile. The one part of the run that does not
    /// repeat, so the benchmark reports it apart from the frames.
    #[cfg(feature = "bench_render_loop")]
    load_cycles: u32,
}

#[cfg(not(fw_harness))]
#[inline(never)]
fn core_boot(spawner: embassy_executor::Spawner) -> CoreBoot {
    // TODO: esp_println writes directly to USB-Serial-JTAG hardware, outside
    // the link task. May block if no USB host is connected during boot.
    // Hasn't been observed yet but worth investigating if boot hangs occur.

    // Initialize board (clock, heap, runtime) and get hardware peripherals
    esp_println::println!("[INIT] Initializing board...");
    let (sw_int, timg0, rmt_peripheral, usb_device, _gpio18, flash, _gpio4, _gpio20, wifi, rwdt) =
        init_board();
    // Paint the main stack before anything deep runs, so the heartbeat's
    // high-water report measures the whole app (see `stack_probe`).
    stack_probe::paint();
    esp_println::println!(
        "[INIT] Board initialized, starting runtime... (main stack {} B)",
        stack_probe::total_bytes()
    );

    // Crash recovery: analyze the previous run (reset reason + persistent
    // breadcrumb region) before anything crash-prone runs, then arm the
    // hardware watchdog so hangs from here on are attributable.
    let reset_cause = recovery::current_reset_cause();
    let (recovery_inst, boot_assessment) =
        lp_recovery::Recovery::init(recovery::Esp32RecoveryBackend::take(), reset_cause);
    lp_recovery::set_global(Box::leak(Box::new(recovery_inst)));
    recovery::log_boot_assessment(&boot_assessment);
    // Baseline 0 matches the server loop's time provider, which also starts
    // at ~0; the first link-task tick re-baselines within milliseconds.
    let watchdog = recovery::watchdog::WatchdogFeeder::start(rwdt, 0);
    let boot_guard = lp_recovery::enter(lp_recovery::FrameKind::Boot, "boot").ok();

    start_runtime(timg0, sw_int);
    esp_println::println!("[INIT] Runtime started");

    // The host link runs lp-link over USB-Serial-JTAG: its task owns the
    // peripheral, and `log` records ride its log channel from here on. The
    // `esp_println!` lines before and around this are raw text outside
    // frames, which a host sees as text (and so does a plain monitor).
    esp_println::println!("[INIT] fw-esp32 starting...");

    // The session nonce: random per boot, so a host learns the board
    // restarted (the RNG is the same one the login challenges draw from).
    let usb_link =
        fw_esp32_common::usb_link::UsbLinkShared::leak(esp_hal::rng::Rng::new().random());
    esp_println::println!("[INIT] Spawning USB link task...");
    spawner.spawn(usb_link_task(usb_device, usb_link).unwrap());
    esp_println::println!("[INIT] USB link task spawned");

    fw_esp32_common::log_ring_logger::init();

    // The transcript header, before any record and as early as the logger
    // allows — the same sink-agnostic entry point every other C6 payload uses.
    #[cfg(feature = "bench_render_loop")]
    bench::render_loop::write_header();

    log::info!("[fw-esp32c6] Shader backend: native JIT (lpvm-native rt_jit)");

    // The server's side of the host link: whole wire messages on the link's
    // proto channel.

    // Boot-control sector: a flash-persisted instruction from a previous run
    // or from the host over esptool. Read (and consumed) before the
    // filesystem mounts, because it must survive the power cycle that wipes
    // the RTC recovery region — see docs/adr/2026-07-30-boot-control-sector.md.
    #[cfg(lp_split)]
    let ota_state;
    #[cfg(not(feature = "memory_fs"))]
    let (boot_control, flash) = {
        let mut flash_storage = esp_storage::FlashStorage::new(flash);
        // Split builds: read the boot records — and a trial core marks itself
        // attempted — right here, before any radio or driver comes up (a new
        // core that dies in that bring-up must still read as a failed trial,
        // or the loader would keep retrying it). NOT before this line: the
        // OTA path reads flash through the ROM, and any ROM flash access
        // before `FlashStorage::new` leaves esp-storage's SPI1 RDID size
        // probe returning garbage on silicon — every lpfs read then fails
        // (two XIAO C6s, 2026-10-02).
        #[cfg(lp_split)]
        {
            ota_state = ota::begin();
        }
        let outcome = crate::bootctl::read_and_consume(&mut flash_storage);
        (outcome, flash_storage)
    };
    #[cfg(feature = "memory_fs")]
    let boot_control = lp_bootctl::DecodeOutcome::Blank;
    #[cfg(all(lp_split, feature = "memory_fs"))]
    {
        ota_state = ota::begin();
    }

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

    // The render-loop benchmark's whole firmware difference, part one: the
    // filesystem is not empty. Everything after this line — the manifest, the
    // server, `auto_load_project`, the RMT open, the render — is the shipped
    // boot finding a project where a flashed board would have one.
    #[cfg(feature = "bench_render_loop")]
    bench::render_loop::seed(base_fs.as_ref());

    let hardware_manifest = load_hardware_manifest(
        base_fs.as_ref(),
        lpc_hardware::default_esp32c6_hardware_manifest,
    );
    log::info!(
        "[fw-esp32c6] Hardware manifest: {} ({})",
        hardware_manifest.board_id(),
        hardware_manifest.board_name()
    );
    // Before any radio init: the XIAO C6's RF switch is dead until its pins
    // are driven. Keyed on the manifest in effect, compiled-in fallback
    // included; any other board id is left alone.
    let quirks_applied = apply_board_quirks(hardware_manifest.board_id());
    // As early as the board is known: steady on (`Booting`) until the server loop
    // starts. A board with no status LED gets nothing.
    board::esp32c6::status_led::start(spawner, hardware_manifest.board_id());
    let hardware_registry = Rc::new(HwRegistry::new(hardware_manifest));
    #[cfg(all(
        feature = "radio",
        not(any(
            feature = "stress_s2",
            feature = "stress_s3",
            feature = "desk_espnow_meter"
        ))
    ))]
    let radio_driver = {
        let radio_driver = Esp32EspNowRadioDriver::new(Rc::clone(&hardware_registry), wifi)
            .expect("Failed to initialize ESP-NOW radio");
        log::info!(
            "[fw-esp32c6] ESP-NOW radio ready: device_id={:?} channel={}",
            radio_driver.device_id(),
            radio_driver.default_channel()
        );
        radio_driver
    };
    // P4 stress builds: the radio stack becomes a load generator instead of a
    // driver — `esp_radio::wifi::new` can only run once, and the stress tasks
    // own its controller/interface. See `stress.rs`.
    #[cfg(any(feature = "stress_s2", feature = "stress_s3"))]
    stress::start(spawner, wifi);
    // The desk's ESP-NOW loss meter (BLE M4's steady-state check): the radio
    // becomes a sequence-numbered broadcaster and counter instead of a driver,
    // for the same reason. Never shipped. See `desk_espnow_meter.rs`.
    #[cfg(feature = "desk_espnow_meter")]
    desk_espnow_meter::start(spawner, wifi);
    #[cfg(all(
        not(feature = "radio"),
        not(any(
            feature = "stress_s2",
            feature = "stress_s3",
            feature = "desk_espnow_meter"
        ))
    ))]
    let _ = wifi;

    // BLE: on unless the device store turns it off, read once here (a
    // missing store is `fresh()`: Bluetooth on, locked, no keys; a damaged
    // one is `locked()`: off). After the Wi-Fi/ESP-NOW bring-up above (the
    // order M2's Run G proved), after the board quirks (the token), before
    // the server exists. A board whose store says off never touches the BLE
    // controller.
    // The radio links' shared slots: the BLE task opens a connection's link
    // there and the link mux (below) serves it. On the heap, not `.bss`: its
    // slots hold `RefCell`s (one thread executor), which a `static` cannot.
    #[cfg(feature = "ble")]
    let radio_port = fw_esp32_common::radio_link::RadioLinkPort::leak();
    #[cfg(feature = "ble")]
    let ble_started = {
        let store = lpa_server::access_store::read_device_store(base_fs.as_ref());
        #[cfg(feature = "desk_ble_params")]
        if let Ok(bytes) = base_fs.read_file(ble::desk_params_path().as_path()) {
            ble::configure_desk_params(core::str::from_utf8(&bytes).unwrap_or(""));
        }
        match (store.ble_enabled, board::esp32c6::init::take_bt()) {
            (true, Some(bt)) => {
                log::info!("[ble] enabled (device store, or none: on by default) — starting");
                ble::start(spawner, bt, radio_port, quirks_applied);
                true
            }
            (true, None) => {
                log::error!("[ble] enabled, but the BT peripheral is gone — BLE off");
                false
            }
            (false, _) => {
                log::info!("[ble] off (device store: bleEnabled=false)");
                #[cfg(feature = "heap_diag_ble_standin")]
                for size in [19_436usize, 3_500, 1_372] {
                    core::mem::forget(alloc::vec![0u8; size]);
                }
                false
            }
        }
    };
    #[cfg(feature = "heap_map_diag")]
    heap_map::log("after-ble");
    #[cfg(not(feature = "ble"))]
    let _ = quirks_applied;

    CoreBoot {
        spawner,
        usb_link,
        rmt_peripheral,
        boot_control,
        base_fs,
        hardware_registry,
        #[cfg(all(
            feature = "radio",
            not(any(
                feature = "stress_s2",
                feature = "stress_s3",
                feature = "desk_espnow_meter"
            ))
        ))]
        radio_driver,
        #[cfg(feature = "ble")]
        ble_started,
        #[cfg(feature = "ble")]
        radio_port,
        watchdog,
        boot_guard,
        boot_assessment,
        #[cfg(lp_split)]
        ota_state,
    }
}

/// OTA split-link spike: everything the core hands the engine.
#[cfg(not(fw_harness))]
struct CoreBoot {
    spawner: embassy_executor::Spawner,
    usb_link: &'static fw_esp32_common::usb_link::UsbLinkShared,
    rmt_peripheral: esp_hal::peripherals::RMT<'static>,
    boot_control: lp_bootctl::DecodeOutcome,
    base_fs: Box<dyn lpfs::LpFs>,
    hardware_registry: Rc<HwRegistry>,
    #[cfg(all(
        feature = "radio",
        not(any(
            feature = "stress_s2",
            feature = "stress_s3",
            feature = "desk_espnow_meter"
        ))
    ))]
    radio_driver: Esp32EspNowRadioDriver,
    #[cfg(feature = "ble")]
    ble_started: bool,
    #[cfg(feature = "ble")]
    radio_port: &'static fw_esp32_common::radio_link::RadioLinkPort,
    watchdog: recovery::watchdog::WatchdogFeeder,
    boot_guard: Option<lp_recovery::FrameGuard>,
    boot_assessment: lp_recovery::BootAssessment,
    #[cfg(lp_split)]
    ota_state: ota::BootState,
}

/// The engine's header, the first bytes of the engine region (split builds).
/// The core never names it — it reads it through a plain address — so no
/// relocation in the core points into the engine, and the split tool's
/// reachability walk from the core's roots never crosses into it.
#[cfg(all(lp_split, not(fw_harness)))]
#[repr(C)]
struct EngineHeader {
    magic: [u8; 8],
    build_id: [u8; 48],
    entry: fn(CoreBoot),
}

#[cfg(all(lp_split, not(fw_harness)))]
const ENGINE_MAGIC: [u8; 8] = *b"LPENGIN1";
#[cfg(all(lp_split, not(fw_harness)))]
use ota::ENGINE_VADDR;

/// Lockstep: core and engine carry the same id because they come from the
/// same link: commit + dirty flag, plus `LP_BUILD_TAG` when set (two builds
/// of one commit told apart — the OTA tests' X and Y).
#[cfg(all(lp_split, not(fw_harness)))]
const fn build_id() -> [u8; 48] {
    const fn append(out: &mut [u8; 48], at: usize, src: &[u8]) -> usize {
        let mut i = 0;
        while i < src.len() && at + i < 48 {
            out[at + i] = src[i];
            i += 1;
        }
        at + i
    }
    let mut out = [0u8; 48];
    let at = append(
        &mut out,
        0,
        concat!(env!("LP_BUILD_COMMIT"), "-", env!("LP_BUILD_DIRTY")).as_bytes(),
    );
    if let Some(tag) = option_env!("LP_BUILD_TAG") {
        let at = append(&mut out, at, b"+");
        append(&mut out, at, tag.as_bytes());
    }
    out
}

#[cfg(all(lp_split, not(fw_harness)))]
#[unsafe(link_section = ".engine_header")]
#[used]
static ENGINE_HEADER: EngineHeader = EngineHeader {
    magic: ENGINE_MAGIC,
    build_id: build_id(),
    entry: lp_engine_entry,
};

/// The engine's entry, if a matching engine is mapped.
#[cfg(all(lp_split, not(fw_harness)))]
fn engine_entry() -> Option<fn(CoreBoot)> {
    let header = ENGINE_VADDR as *const EngineHeader;
    // SAFETY: the window is mapped (erased flash reads as 0xff); volatile so
    // the compiler cannot assume anything about bytes it did not write.
    let (magic, id) = unsafe {
        (
            core::ptr::read_volatile(core::ptr::addr_of!((*header).magic)),
            core::ptr::read_volatile(core::ptr::addr_of!((*header).build_id)),
        )
    };
    if magic != ENGINE_MAGIC {
        ota::say!("[CORE] no engine at {ENGINE_VADDR:#x} — core-only mode");
        return None;
    }
    if id != build_id() {
        ota::say!("[CORE] engine build id mismatch — core-only mode");
        return None;
    }
    // SAFETY: magic and build id match, so this header came from this link.
    Some(unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*header).entry)) })
}

/// The engine door. Split builds reach it only through `ENGINE_HEADER`, so
/// everything reachable from here and not from the core's roots is engine;
/// monolithic builds call it directly.
#[cfg(not(fw_harness))]
#[inline(never)]
fn lp_engine_entry(core: CoreBoot) {
    let CoreBoot {
        spawner,
        usb_link,
        rmt_peripheral,
        boot_control,
        base_fs,
        hardware_registry,
        #[cfg(all(
            feature = "radio",
            not(any(
                feature = "stress_s2",
                feature = "stress_s3",
                feature = "desk_espnow_meter"
            ))
        ))]
        radio_driver,
        #[cfg(feature = "ble")]
        ble_started,
        #[cfg(feature = "ble")]
        radio_port,
        watchdog,
        boot_guard,
        boot_assessment,
        ..
    } = core;
    let transport = fw_esp32_common::usb_link::UsbLinkTransport::new(usb_link);
    // The RMT peripheral becomes the WS281x driver's, clock and all. 80 MHz
    // with the per-channel divider of 1 gives the 12.5 ns tick
    // `lp_ws281x::PulseCodes` assumes — the same pair the legacy C6 driver's
    // `config.rs` encoded and drove strips with since the project started.
    esp_println::println!("[INIT] Initializing RMT peripheral at 80MHz...");
    let rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, output::rmt::shared_driver::RMT_CLOCK)
        .expect("Failed to initialize RMT");
    esp_println::println!("[INIT] RMT peripheral initialized");
    let mut hardware_system = HardwareSystem::new(Rc::clone(&hardware_registry));
    // How many outputs appear is decided in one place: the board manifest's
    // `/rmt/ws281xK` resources (two on the XIAO C6). The RMT block plan
    // follows from that count at driver init — two declared channels get one
    // 48-word block each; a single declared channel absorbs the whole
    // 192-word RMT RAM (RX blocks included) for legacy-class refill margin.
    // See `output::rmt::c6_rmt::plan_for_declared`; absorbed slots are never
    // configured.
    hardware_system.add_ws281x_driver(Box::new(Esp32C6RmtWs281xDriver::new(
        Rc::clone(&hardware_registry),
        rmt,
    )));
    hardware_system.add_button_driver(Box::new(Esp32GpioButtonDriver::new(Rc::clone(
        &hardware_registry,
    ))));
    #[cfg(all(
        feature = "radio",
        not(any(
            feature = "stress_s2",
            feature = "stress_s3",
            feature = "desk_espnow_meter"
        ))
    ))]
    hardware_system.add_radio_driver(Box::new(radio_driver));
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
    server.set_read_headroom_probe(Some(read_headroom_probe));
    server.set_read_gate(Some(READ_GATE));
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
    // The chip's own permanent identity (efuse): the factory MAC, the
    // silicon revision, and — the C6 has an 802.15.4 radio — its EUI-64.
    // The server cannot derive any of it.
    server.set_hardware_identity(chip_identity());
    // The board this firmware is running as, from the loaded manifest — the
    // catalog key a card needs to re-flash or wire a new project for it.
    server.set_board_id(Some(alloc::string::String::from(
        hardware_registry.manifest().board_id(),
    )));
    server.set_reboot_hook(Some(Rc::new(reboot_now)));
    // JSON Pack: answer a host's opt-in with what this image's transport
    // can write (`fw-esp32-common/json-pack`).
    server.set_packed_encoding_supported(
        fw_esp32_common::serial::server_msg::PACKED_ENCODING_SUPPORTED,
    );
    // Login challenges draw from the chip's hardware RNG; the server itself
    // never draws randomness (sans-IO).
    server.set_entropy_source(Some(fill_random));
    // A PowerButton node deep-sleeps the chip through this (EXT1 wake).
    server.set_power_platform(Some(Rc::new(
        crate::hardware::power::Esp32C6PowerPlatform::new(Rc::clone(&hardware_system)),
    )));
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
    #[cfg(feature = "bench_render_loop")]
    let load_from = board::esp32c6::cycle_counter::read();
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
    #[cfg(feature = "bench_render_loop")]
    let load_cycles = board::esp32c6::cycle_counter::read().wrapping_sub(load_from);

    // Create time provider
    esp_println::println!("[INIT] Creating time provider...");
    let time_provider = Esp32TimeProvider::new();
    esp_println::println!("[INIT] Time provider created");

    // Boot frame ends here; the boot-complete milestone is marked by the
    // server loop after the first successful frame.
    drop(boot_guard);

    // USB plus the radio links. The advertised-name hook only when BLE runs.
    #[cfg(feature = "ble")]
    let transport = {
        let mux = fw_esp32_common::radio_link::LinkMuxTransport::new(
            transport,
            radio_port,
            embassy_time::Delay,
        );
        if ble_started {
            mux.with_upkeep_hook(ble::refresh_advertised_name)
        } else {
            mux
        }
    };

    let app = FirmwareApp {
        server,
        transport,
        time_provider,
        watchdog,
        #[cfg(feature = "bench_render_loop")]
        load_cycles,
    };
    #[cfg(feature = "diag_secure_link")]
    secure_link_probe::run();
    board::esp32c6::status_led::show(lpc_hardware::StatusLedState::Running);
    // Keep the marker substring "fw-esp32c6 initialized, starting server
    // loop" intact: two readiness classifiers grep for it.
    esp_println::println!(
        "[INIT] fw-esp32 initialized, starting server loop... proto={} commit={} dirty={}",
        lpc_wire::WIRE_PROTO_VERSION,
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
    );
    spawner.spawn(engine_task(app).unwrap());
}

/// The server loop, as its own task (spawned by the engine door).
#[cfg(not(fw_harness))]
#[embassy_executor::task]
async fn engine_task(app: FirmwareApp) {
    // Run server loop (never returns)
    #[cfg(not(feature = "bench_render_loop"))]
    {
        let mut watchdog = app.watchdog;
        run_server_loop(
            app.server,
            app.transport,
            app.time_provider,
            heartbeat_memory_stats,
            move |now_ms| watchdog.feed(now_ms),
        )
        .await;
    }

    // The render-loop benchmark's whole firmware difference, part two:
    // the same loop, with an end. `run_server_loop` is a wrapper around
    // this call with `FrameBudget::UNBOUNDED` — the frames below are the
    // product's frames, not a re-implementation of them.
    #[cfg(feature = "bench_render_loop")]
    {
        use fw_checks::checks::render_loop::{FrameStats, cycles_to_us};

        let heap_after_load = esp32_memory_stats().unwrap_or((0, 0));
        let load_us = cycles_to_us(app.load_cycles as u64, board::esp32c6::constants::CPU_HZ);
        let mut stats = FrameStats::new();
        // The guest's own clock, bracketing the loop: `Esp32TimeProvider`
        // measures from its own construction and is about to be moved
        // into the loop, so the bracket is taken on `Instant` directly.
        let started = embassy_time::Instant::now();

        // The same watchdog the product arms and the same feed policy:
        // the bounded loop yields once a frame like the unbounded one, so
        // the I/O task stays provably alive and the RWDT never bites. An
        // image that disarmed it would differ from the product in a third
        // way, for no measurement.
        let mut watchdog = app.watchdog;
        let server = server_loop::run_server_loop_bounded(
            app.server,
            app.transport,
            app.time_provider,
            heartbeat_memory_stats,
            move |now_ms| watchdog.feed(now_ms),
            bench::render_loop::budget(),
            |cycles| stats.record(cycles),
        )
        .await;

        let uptime_us = started.elapsed().as_micros();
        // Report BEFORE the server is dropped: the summary's heap figures
        // are meant to describe a machine with the project loaded, and
        // dropping it first would report one that had just unloaded.
        bench::render_loop::report(&stats, load_us, uptime_us, heap_after_load);
        drop(server);

        // Idle, yielding, so the I/O task drains the records and the
        // marker to the host link. `--exit-on` fires on those bytes; a
        // loop that stopped yielding here would print the sentinel into a
        // queue nobody pumps.
        loop {
            embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
        }
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

    #[cfg(feature = "test_uart_bridge")]
    {
        use tests::uart_bridge::run_uart_bridge;
        run_uart_bridge(spawner).await;
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

    #[cfg(feature = "test_comms_lab")]
    {
        use tests::comms_lab::run_comms_lab;
        run_comms_lab(spawner).await;
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

    #[cfg(feature = "test_cycle_probe")]
    {
        use tests::cycle_probe::run_cycle_probe;
        run_cycle_probe(spawner).await;
    }

    #[cfg(feature = "test_gpio_input")]
    {
        use tests::gpio_input::run_gpio_input;
        run_gpio_input(spawner).await;
    }

    #[cfg(feature = "test_rmt_rx")]
    {
        use tests::rmt_rx::run_rmt_rx;
        run_rmt_rx(spawner).await;
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

    #[cfg(feature = "test_ble")]
    {
        use tests::test_ble::run_ble_test;
        run_ble_test(spawner).await;
    }

    #[cfg(feature = "test_espnow_broadcast")]
    {
        use tests::espnow_broadcast::run_espnow_broadcast;
        run_espnow_broadcast(spawner).await;
    }

    #[cfg(feature = "test_f32_softfloat")]
    {
        use tests::f32_softfloat::run_f32_softfloat_test;
        run_f32_softfloat_test(spawner).await;
    }

    #[cfg(not(fw_harness))]
    {
        let core = core_boot(spawner);
        #[cfg(not(lp_split))]
        lp_engine_entry(core);
        #[cfg(lp_split)]
        split_boot(core).await;
        // The server loop runs in its own task now; main has nothing left to
        // do. A future that never completes arms no timer (a long sleep here
        // would arm an alarm the boot gates rightly refuse).
        core::future::pending::<()>().await;
    }
}

/// A split build after `core_boot`: enter the engine the boot records and
/// the header agree on, or stay core-only and take an update.
#[cfg(all(lp_split, not(fw_harness)))]
async fn split_boot(mut core: CoreBoot) {
    let state = core::mem::replace(&mut core.ota_state, ota::BootState::placeholder());
    let id = build_id();
    let id_len = id.iter().position(|b| *b == 0).unwrap_or(id.len());
    ota::say!(
        "[CORE] build {} @{:#x}{}",
        core::str::from_utf8(&id[..id_len]).unwrap_or("?"),
        state.core_off,
        if state.on_trial() { " (trial)" } else { "" }
    );
    if state.rolled_back() {
        ota::say!("[OTA] rolled back: the newer core never confirmed");
    }
    // Test builds only: a core that dies on its trial boot, before it could
    // confirm — what the loader's rollback exists for.
    if option_env!("LP_OTA_TEST_DIE_ON_TRIAL").is_some() && state.on_trial() {
        ota::say!("[TEST] dying on trial");
        ota::system_reset();
    }
    ota::map_engine(state.engine_extent());
    let incomplete = lp_recovery::snapshot()
        .map(|s| s.consecutive_incomplete_boots)
        .unwrap_or(0);
    let engine_crashing = incomplete >= ota::INCOMPLETE_BOOTS_TO_CORE_ONLY;
    let entry = engine_entry();
    match entry {
        Some(entry) if !engine_crashing && !state.on_trial() => {
            fw_esp32_common::usb_link::set_update_hook(ota::on_update_while_running);
            entry(core);
        }
        _ => {
            if engine_crashing && entry.is_some() {
                ota::say!("[OTA] {incomplete} incomplete boots — not starting the engine");
            }
            let CoreBoot {
                usb_link, watchdog, ..
            } = core;
            ota::core_only(usb_link, watchdog, state, engine_crashing).await;
        }
    }
}

// Same gate as its only caller, `boot_firmware`: the hardware harnesses
// replace `main` with their own entrypoint and never send a hello.
#[cfg(not(fw_harness))]
/// This chip's permanent identity, read from efuse.
///
/// Injected rather than derived: the server cannot read silicon, and the
/// chip-generic firmware layer deliberately has no `esp_hal`
/// (ADR 2026-07-29-per-chip-fw-toolchains). See
/// `fw_esp32_common::chip_identity` for why this reports the BASE MAC
/// rather than a per-interface list.
fn chip_identity() -> lpc_wire::HardwareIdentity {
    use esp_hal::efuse;
    let revision = efuse::chip_revision();
    lpc_wire::HardwareIdentity {
        base_mac: Some(fw_esp32_common::chip_identity::hex_bytes(
            efuse::base_mac_address().as_bytes(),
        )),
        chip_revision: Some(alloc::format!("{}.{}", revision.major, revision.minor)),
        // The C6 has an 802.15.4 radio, so it carries a 64-bit EUI: the
        // 48-bit base MAC followed by the chip's 16-bit `MAC_EXT` efuse
        // field. Parts without that radio have no `MAC_EXT` at all.
        eui64: Some({
            let base = efuse::base_mac_address();
            let base = base.as_bytes();
            let ext: [u8; 2] = efuse::read_field_le(efuse::MAC_EXT);
            let mut bytes = [0u8; 8];
            bytes[..6].copy_from_slice(base);
            bytes[6..].copy_from_slice(&ext);
            fw_esp32_common::chip_identity::hex_bytes(&bytes)
        }),
    }
}
