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
    target: crate::MANIFEST_TARGET,
    chip_family: "esp32",
    chip: crate::MANIFEST_CHIP,
    cargo_target: "riscv32imac-unknown-none-elf",
    profile: env!("LP_BUILD_PROFILE"),
    version: env!("LP_APP_VERSION"),
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
    // The split image takes over-the-air updates in layout 1; a plain
    // (single-image) build says nothing — only USB updates it.
    ota_layout: if cfg!(lp_split) {
        lpc_update::code_table::LAYOUT_1
    } else {
        0
    },
}

/// The manifest core's `target` and `platform.chip`, named once: the
/// embedded blob and the board manifest an update reports (`ota/`) read the
/// same constants, so the two can never say different things.
#[cfg(feature = "server")]
const MANIFEST_TARGET: &str = env!("LP_FW_TARGET");
#[cfg(feature = "server")]
const MANIFEST_CHIP: &str = "esp32c6";

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
    feature = "test_ble_coex",
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
#[cfg(all(feature = "io-thread", not(fw_harness)))]
mod io_thread;
#[cfg(all(feature = "io_thread_stack_diag", not(fw_harness)))]
mod io_thread_stack_diag;
// The Wi-Fi station and the network on `lp-net` (Wi-Fi roadmap M6). Its
// radio comes from the radio hub, which stress and desk-meter builds give to
// their load generators instead.
#[cfg(lp_net)]
mod net;
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
#[cfg(all(feature = "radio_dma_diag", lp_net))]
mod radio_dma_diag;
mod recovery;
mod seams;
#[cfg(all(feature = "diag_secure_link", not(fw_harness)))]
mod secure_link_probe;
mod serial;
#[cfg(not(fw_harness))]
mod stack_probe;
#[cfg(all(any(feature = "stress_s2", feature = "stress_s3"), not(fw_harness)))]
mod stress;
#[cfg(all(
    any(feature = "io_thread_stack_diag", feature = "net_thread_stack_diag"),
    not(fw_harness)
))]
mod thread_stack_diag;
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
mod flash_layout;
#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
mod flash_storage;
#[cfg(all(not(feature = "memory_fs"), not(fw_harness),))]
mod legacy_layout;
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
#[cfg(all(not(feature = "io-thread"), not(fw_harness)))]
use serial::usb_link_task;
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
    #[cfg(feature = "test_seam_abi")]
    pub mod test_seam_abi;
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
    // The heartbeat's stack lines are due; [`log_heartbeat_stack_lines`]
    // writes them once the heartbeat itself has gone out.
    HEARTBEAT_STACK_LINES_DUE.store(true, core::sync::atomic::Ordering::Relaxed);
    esp32_memory_stats().map(|(free_bytes, used_bytes)| lpc_wire::server::MemoryStats {
        free_bytes,
        used_bytes,
        total_bytes: used_bytes.saturating_add(free_bytes),
        largest_free_block: read_headroom_probe(),
        oom_retry_saves: None,
    })
}

/// Set when a heartbeat's figures are taken; cleared once its stack lines are
/// logged.
#[cfg(not(fw_harness))]
static HEARTBEAT_STACK_LINES_DUE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// The heartbeat's stack lines, logged AFTER the heartbeat went out: one scan
/// of the main stack per heartbeat, a line only when the mark grows (and the
/// link thread's, under `io_thread_stack_diag`). Run from the server loop's
/// per-iteration upkeep, which follows the heartbeat in the same iteration.
///
/// They used to be logged while the heartbeat was built, which put them on
/// the wire after it only because the link task ran later. With the link on
/// its own thread (`io_thread`) a record goes out the moment it is written,
/// so a line logged before the send overtook the heartbeat, and a capture
/// stopping on `[stack] heartbeat: high-water` (`boot-idle`'s sentinel)
/// would stop short of it.
#[cfg(not(fw_harness))]
fn log_heartbeat_stack_lines() {
    if HEARTBEAT_STACK_LINES_DUE.swap(false, core::sync::atomic::Ordering::Relaxed) {
        stack_probe::log_if_grown("heartbeat");
        c_heap::log_if_grown("heartbeat");
        #[cfg(feature = "io_thread_stack_diag")]
        io_thread_stack_diag::log_if_grown();
        #[cfg(feature = "net_thread_stack_diag")]
        net::net_thread_stack_diag::log_if_grown();
        #[cfg(lp_net)]
        net::net_heartbeat::log_line();
    }
}

/// This chip's ProjectRead memory gate (`lpa_server::ReadGate`): refuse a
/// read when under 40 KiB is free in total or no 8 KiB block is left.
///
/// Since 2026-10-07 a read on a fragmented heap goes out in smaller frames
/// (`lpa_server::read_frame_budget`: half the largest block, at most the
/// link's 16 KiB), so the block floor only has to hold a read's largest
/// single ask, the 8 KB mapping slot JSON below, not twice it. A Wi-Fi-joined
/// C6 with a LAN link open sat at a 16,164-16,172 B block after a switch, and
/// the old 16 KiB floor refused every read there (emulated; the defect is
/// `docs/defects/2026-10-06-a-wifi-joined-c6-refuses-every-project-switch.md`).
///
/// Measured on the emulated C6 with Bluetooth on (`lp-emu:esp32c6:t1@4caa5b658`,
/// plan `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`, REPORT.md):
///
/// - a read's working set is 8.3–25.1 KB (the first sync's skeleton read is
///   the worst; the editor's repeating reads are 12–15.6 KB), and it keeps
///   0–176 B; 40 KiB is that plus room for the link and radio tasks;
/// - the largest single ask any read makes is 8 KB (a mapping file's slot
///   JSON, once `lpc-wire` sizes it exactly), 2.5 KB for the editor's reads;
///   8 KiB holds it, now that a frame is cut to half the block.
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
    min_largest_block_bytes: 8 * 1024,
};

#[cfg(not(fw_harness))]
fn read_headroom_probe() -> Option<u32> {
    // At once when no project is loaded (the load gate's probe, after a
    // stop), else every few seconds.
    #[cfg(feature = "heap_map_diag")]
    heap_map::log_if_stopped_or_periodic("probe", 140_000);
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
    crate::board::esp32c6::restart::restart()
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

/// Everything the core hands the engine: what `core_boot` brought up, by
/// value, through `lp_engine_entry`. Core and engine come from one link, so
/// this crosses the boundary as an ordinary Rust value.
#[cfg(not(fw_harness))]
struct CoreBoot {
    spawner: embassy_executor::Spawner,
    usb_link: &'static fw_esp32_common::usb_link::UsbLinkShared,
    rmt_peripheral: esp_hal::peripherals::RMT<'static>,
    boot_control: lp_bootctl::DecodeOutcome,
    base_fs: Box<dyn lpfs::LpFs>,
    fs_boot_state: lpc_wire::FsBootState,
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
    /// The radio links' shared slots: the BLE task (started by the core)
    /// opens a connection's link there, and the engine's link mux serves it.
    #[cfg(feature = "ble")]
    radio_port: &'static fw_esp32_common::radio_link::RadioLinkPort,
    #[cfg(feature = "ble")]
    ble_started: bool,
    watchdog: recovery::watchdog::WatchdogFeeder,
    boot_guard: Option<lp_recovery::FrameGuard>,
    boot_assessment: lp_recovery::BootAssessment,
    #[cfg(lp_split)]
    ota_state: ota::BootState,
    /// The boot read a saved network with Wi-Fi on (`lp-net` joins one): a
    /// trial core then waits only so long for a host (`ota::core_only`).
    #[cfg(lp_split)]
    network_saved: bool,
}

/// The core's half of the boot: the board, recovery and the watchdog, the
/// host link, the boot-control sector, the filesystem, the hardware manifest
/// and quirks, the status LED, and the radios (ESP-NOW and BLE). Everything
/// a board needs to stay reachable — and, on a split image, everything that
/// is not the engine.
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
    let nonce = esp_hal::rng::Rng::new().random();
    // The link task on a thread of its own (`io_thread`), created this early
    // because its stack comes off the heap; the link is then shared across
    // two threads, so it takes the thread's lock.
    #[cfg(feature = "io-thread")]
    let usb_link = {
        let usb_link =
            fw_esp32_common::usb_link::UsbLinkShared::leak_locked(nonce, io_thread::link_lock);
        io_thread::start(usb_device, usb_link);
        usb_link
    };
    #[cfg(not(feature = "io-thread"))]
    let usb_link = {
        let usb_link = fw_esp32_common::usb_link::UsbLinkShared::leak(nonce);
        esp_println::println!("[INIT] Spawning USB link task...");
        spawner.spawn(usb_link_task(usb_device, usb_link).unwrap());
        esp_println::println!("[INIT] USB link task spawned");
        usb_link
    };

    fw_esp32_common::log_ring_logger::init();

    // The transcript header, before any record and as early as the logger
    // allows — the same sink-agnostic entry point every other C6 payload uses.
    #[cfg(feature = "bench_render_loop")]
    bench::render_loop::write_header();

    log::info!("[fw-esp32c6] Shader backend: native JIT (lpvm-native rt_jit)");

    // Boot-control sector: a flash-persisted instruction from a previous run
    // or from the host over esptool. Read (and consumed) before the
    // filesystem mounts, because it must survive the power cycle that wipes
    // the RTC recovery region — see docs/adr/2026-07-30-boot-control-sector.md.
    #[cfg(lp_split)]
    let ota_state;
    #[cfg(not(feature = "memory_fs"))]
    let (boot_control, flash, flash_layout) = {
        let mut flash_storage = esp_storage::FlashStorage::new(flash);
        // The partition table, read once: `lpfs` for the filesystem, and
        // `factory` for a split image's region end.
        let flash_layout = crate::flash_layout::FlashLayout::locate(&mut flash_storage);
        // Split builds: read the boot records — and a trial core marks itself
        // attempted — right here, before any radio or driver comes up (a new
        // core that dies in that bring-up must still read as a failed trial,
        // or the loader would keep retrying it). NOT before `FlashStorage::new`:
        // the split path reads flash through the ROM, and any ROM flash access
        // before it leaves esp-storage's SPI1 RDID size probe returning
        // garbage on silicon — every lpfs read then fails (two XIAO C6s,
        // 2026-10-02).
        #[cfg(lp_split)]
        {
            ota_state = ota::begin(flash_layout.factory.map(|f| (f.offset, f.len)));
        }
        let outcome = crate::bootctl::read_and_consume(&mut flash_storage);
        (outcome, flash_storage, flash_layout)
    };
    #[cfg(feature = "memory_fs")]
    let boot_control = lp_bootctl::DecodeOutcome::Blank;
    // No flash driver, no table: the boot state is untrusted (no writes).
    #[cfg(all(lp_split, feature = "memory_fs"))]
    {
        ota_state = ota::begin(None);
    }

    // Create filesystem before hardware providers so /hardware.json can override board policy.
    let (base_fs, fs_boot_state): (Box<dyn lpfs::LpFs>, lpc_wire::FsBootState) = {
        #[cfg(not(feature = "memory_fs"))]
        {
            use lpc_wire::FsBootState;
            let flash_storage = flash;
            match flash_layout.lpfs {
                // Not a runtime condition: the image was flashed without
                // `--partition-table lp-fw/fw-esp32c6/partitions.csv` and
                // espflash substituted its default. Say so rather than guess
                // an offset and mount across whatever is there.
                None => {
                    esp_println::println!(
                        "[ERROR] no `lpfs` partition in the flashed table — reflash with \
                         --partition-table lp-fw/fw-esp32c6/partitions.csv; using memory FS"
                    );
                    (Box::new(LpFsMemory::new()), FsBootState::Memory)
                }
                // The legacy guard (crate::legacy_layout): a partition that
                // will not mount is formatted only when no pre-repartition
                // filesystem is waiting at the old offset.
                Some(partition) => match lp_fs::LpFsFlash::init_guarded(
                    crate::flash_storage::LpFlashStorage::new(flash_storage, partition),
                    crate::flash_storage::lpfs_config,
                    |storage| {
                        if storage.legacy_lpfs_present() {
                            lp_fs::FormatVerdict::Hold
                        } else {
                            lp_fs::FormatVerdict::Format
                        }
                    },
                ) {
                    // One line for both: the format itself is logged by
                    // `lp_fs` ("Formatted and mounted fresh filesystem").
                    Ok(lp_fs::FlashFsInit::Mounted(fs)) => {
                        esp_println::println!("[INIT] Flash filesystem mounted");
                        (Box::new(fs), FsBootState::Mounted)
                    }
                    Ok(lp_fs::FlashFsInit::Formatted(fs)) => {
                        esp_println::println!("[INIT] Flash filesystem mounted");
                        (Box::new(fs), FsBootState::Formatted)
                    }
                    Ok(lp_fs::FlashFsInit::Held) => {
                        esp_println::println!(
                            "[FS] legacy-layout filesystem found at {:#x} — not formatting; \
                             files are held for migration; using memory FS",
                            crate::legacy_layout::LEGACY_LPFS_V1_OFFSET
                        );
                        (Box::new(LpFsMemory::new()), FsBootState::LegacyHeld)
                    }
                    Err(e) => {
                        esp_println::println!(
                            "[WARN] Flash FS failed: {e}, falling back to memory"
                        );
                        (Box::new(LpFsMemory::new()), FsBootState::Memory)
                    }
                },
            }
        }
        #[cfg(feature = "memory_fs")]
        {
            let _ = flash;
            esp_println::println!("[INIT] Creating in-memory filesystem...");
            (
                Box::new(LpFsMemory::new()) as Box<dyn lpfs::LpFs>,
                lpc_wire::FsBootState::Memory,
            )
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
    // The radio comes up in the core (a board must stay reachable with no
    // engine); the engine registers the driver with its hardware system.
    #[cfg(all(
        feature = "radio",
        not(any(
            feature = "stress_s2",
            feature = "stress_s3",
            feature = "desk_espnow_meter"
        ))
    ))]
    let (radio_driver, net_radio) = {
        // The radio's one bring-up (`hardware::radio_hub`): ESP-NOW's
        // interface to its driver; the controller and the station interface
        // to the station (`wifi`), or back to the driver to keep alive.
        let parts =
            hardware::radio_hub::bring_up(wifi).expect("Failed to initialize ESP-NOW radio");
        #[cfg(lp_net)]
        let (kept, net_radio) = (None, Some((parts.controller, parts.station)));
        #[cfg(not(lp_net))]
        let (kept, net_radio) = (Some(parts.controller), None::<()>);
        let radio_driver = Esp32EspNowRadioDriver::from_parts(
            Rc::clone(&hardware_registry),
            parts.esp_now,
            kept,
            hardware::espnow_radio_driver::DEFAULT_ESPNOW_CHANNEL,
        )
        .expect("Failed to initialize ESP-NOW radio");
        log::info!(
            "[fw-esp32c6] ESP-NOW radio ready: device_id={:?} channel={}",
            radio_driver.device_id(),
            radio_driver.default_channel()
        );
        (radio_driver, net_radio)
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
    // one is `locked()`: off; a board HOLDING its files for the layout
    // change is `locked()` too — its real store waits in the old region,
    // and it must not be more open than that store says). After the
    // Wi-Fi/ESP-NOW bring-up above (the order M2's Run G proved), after the
    // board quirks (the token), before the server exists. A board whose
    // store says off never touches the BLE controller.
    // The radio links' shared slots: the BLE task opens a connection's link
    // there and the link mux (below) serves it. On the heap, not `.bss`: its
    // slots hold `RefCell`s, which a `static` cannot. With the LAN's links
    // (served from `lp-net`) every borrow is taken under the port's lock.
    #[cfg(lp_net)]
    let (radio_port, lan_port) =
        fw_esp32_common::radio_link::RadioLinkPort::leak_locked(net::net_thread::port_lock);
    #[cfg(all(feature = "ble", not(lp_net)))]
    let radio_port = fw_esp32_common::radio_link::RadioLinkPort::leak();
    // What the radio links are for, decided before the BLE task may open
    // one (a link's SYN carries its receive window). A plain image always
    // serves them; a split image decides in `split_boot`, once it has chosen
    // engine or core-only — no link opens until then.
    #[cfg(all(feature = "ble", not(lp_split)))]
    radio_port.decide_mode(fw_esp32_common::radio_link::RadioLinkMode::Serve);
    #[cfg(feature = "ble")]
    let ble_started = {
        let store = lpa_server::access_store::device_store_at_boot(base_fs.as_ref(), fs_boot_state);
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
            (false, _) if fs_boot_state == lpc_wire::FsBootState::LegacyHeld => {
                log::info!(
                    "[ble] off (files held for the layout change: the device store waits with \
                     them)"
                );
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

    // The network on its own thread, in the core (plan MD6): after the radio
    // and BLE bring-up (M2 Run G's order). The network file is read here,
    // on the main thread, before any Radio node exists, so "set to use
    // Wi-Fi" holds from the first frame; the station never reads the file
    // itself (`net::station_probes`).
    #[cfg(lp_net)]
    let network_file = lpa_server::network_store::read_network_file(base_fs.as_ref());
    #[cfg(lp_net)]
    let network_saved = network_file.wifi && !network_file.networks.is_empty();
    #[cfg(not(lp_net))]
    let network_saved = false;
    #[cfg(lp_net)]
    {
        let file = network_file;
        net::station_probes::boot_settings(&file);
        // The relay's account entries, from the device store (Wi-Fi relay
        // plan P8); changes arrive through the server's `AccessChanged`.
        net::relay_probes::boot_access(&lpa_server::access_store::device_store_at_boot(
            base_fs.as_ref(),
            fs_boot_state,
        ));
        log::info!(
            "[wifi] {} network(s) saved, Wi-Fi {}, Cloud relay {}",
            file.networks.len(),
            if file.wifi { "on" } else { "off" },
            if file.cloud_relay { "on" } else { "off" }
        );
        if net::relay_task::relay_host_overridden() {
            esp_println::println!(
                "[INIT] desk image: the relay is {}:{} (LP_RELAY_HOST), not lightplayer.app",
                net::relay_task::RELAY_HOST,
                net::relay_task::relay_port()
            );
        }
        if let Some((controller, station)) = net_radio {
            let mac = net::net_thread::base_mac();
            let host = fw_esp32_common::net::mdns::mdns_host(mac);
            let seed = (u64::from(esp_hal::rng::Rng::new().random()) << 32)
                | u64::from(esp_hal::rng::Rng::new().random());
            // A board that will join allocates its socket buffers now.
            let will_join = file.wifi && !file.networks.is_empty();
            let relay = net::relay_task::relay_config(
                mac,
                lpa_server::device_identity::read_device_name(base_fs.as_ref()),
            );
            net::net_thread::start(controller, station, host, seed, lan_port, will_join, relay);
        }
    }
    #[cfg(all(
        feature = "radio",
        not(lp_net),
        not(any(
            feature = "stress_s2",
            feature = "stress_s3",
            feature = "desk_espnow_meter"
        ))
    ))]
    let _ = net_radio;
    #[cfg(not(lp_split))]
    let _ = network_saved;

    CoreBoot {
        spawner,
        usb_link,
        rmt_peripheral,
        boot_control,
        base_fs,
        fs_boot_state,
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
        radio_port,
        #[cfg(feature = "ble")]
        ble_started,
        watchdog,
        boot_guard,
        boot_assessment,
        #[cfg(lp_split)]
        ota_state,
        #[cfg(lp_split)]
        network_saved,
    }
}

/// The engine door: the RMT driver, the hardware system, the output
/// provider and the server, the project's auto-load, and the server loop as
/// its own task.
///
/// A plain build calls this directly after `core_boot`. A split build
/// reaches it only through the engine header's `entry` (`ENGINE_HEADER`),
/// which the core reads through a plain address — so everything reachable
/// from here and not from the core's own roots links into the engine, the
/// shader compiler included.
#[cfg(not(fw_harness))]
#[inline(never)]
fn lp_engine_entry(core: CoreBoot) {
    let CoreBoot {
        spawner,
        usb_link,
        rmt_peripheral,
        boot_control,
        base_fs,
        fs_boot_state,
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
        radio_port,
        #[cfg(feature = "ble")]
        ble_started,
        watchdog,
        boot_guard,
        boot_assessment,
        ..
    } = core;

    // The server's side of the host link: whole wire messages on the link's
    // proto channel.
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
    // A request the heap cannot decode is refused in words before it is
    // decoded (`server_payload::request_refusal`), on every link.
    fw_esp32_common::serial::server_payload::set_request_headroom_probe(|| {
        Some((
            esp_alloc::HEAP.free(),
            recovery::panic_path::largest_free_block(),
        ))
    });
    server.set_read_gate(Some(READ_GATE));
    // The station's probes and its settings hook (`wifi`): the server reads
    // what the station publishes, and hands it the network file after every
    // change (`net::station_probes`).
    #[cfg(lp_net)]
    {
        server.set_station_probe(Some(net::station_probes::station_probe));
        server.set_scan_probe(Some(net::station_probes::scan_probe));
        server.set_last_attempt_probe(Some(net::station_probes::last_attempt_probe));
        server.set_network_changed(Some(net::station_probes::network_changed));
        // The relay's probe and its account entries' hook (Wi-Fi relay P8).
        server.set_relay_probe(Some(net::relay_probes::relay_probe));
        server.set_access_changed(Some(net::relay_probes::access_changed));
    }
    // With the link on its own thread, answer a tick's requests before its
    // render: the replies then go out while the frame renders (`io_thread`).
    #[cfg(feature = "io-thread")]
    server.set_messages_first(true);
    // Wire hello identity: compile-time provenance from build.rs, injected
    // into the server (sans-IO: the server never reads env/git itself),
    // plus the boot-time read of the root-stamped device identity. The
    // hello's CAPABILITY half is derived inside the constructor above from
    // the engine's gates and the services just injected — never restated
    // here.
    server.set_hello_identity(
        lpc_wire::HelloIdentity::new(
            "fw-esp32c6",
            crate::manifest_version(),
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
    server.set_fs_boot_state(fs_boot_state);
    // The board this firmware is running as, from the loaded manifest — the
    // catalog key a card needs to re-flash or wire a new project for it.
    server.set_board_id(Some(alloc::string::String::from(
        hardware_registry.manifest().board_id(),
    )));
    server.set_reboot_hook(Some(Rc::new(reboot_now)));
    // A split image's hello carries its board manifest (wire proto 38): the
    // core's update session says it, as it answers `Q` on channel 3.
    #[cfg(lp_split)]
    server.set_firmware_manifest(Some(ota::running_manifest));
    // ...and records the strip its update light may drive.
    #[cfg(lp_split)]
    server_loop::set_frame_hook(output::status_light_note::persist);
    // JSON Pack: answer a host's opt-in with what this image's transport
    // can write (`fw-esp32-common/json-pack`).
    server.set_packed_encoding_supported(
        fw_esp32_common::serial::server_msg::PACKED_ENCODING_SUPPORTED,
    );
    // Login challenges draw from the chip's hardware RNG; the server itself
    // never draws randomness (sans-IO).
    server.set_entropy_source(Some(fill_random));
    // Every link's access state reserved now, the USB link's and each radio
    // slot's, so a link's first sight grows nothing above its own memory.
    #[cfg(feature = "ble")]
    server.reserve_links(1 + fw_esp32_common::radio_link::LINK_SLOTS);
    // A PowerButton node deep-sleeps the chip through this (EXT1 wake).
    server.set_power_platform(Some(Rc::new(
        crate::hardware::power::Esp32C6PowerPlatform::new(Rc::clone(&hardware_system)),
    )));
    esp_println::println!("[INIT] LpServer created");

    // USB plus the radio links. The advertised-name hook only when BLE runs;
    // on a split image, each Bluetooth link's channel 3 goes to the core's
    // update session with the tier the link was granted (a LAN link's is not
    // served). Built BEFORE the boot project loads: its per-link lists hold
    // memory for the board's whole life, and allocated after the project
    // they sat above it and split the space it frees (first fit; silicon
    // N7, 2026-10-06).
    #[cfg(feature = "ble")]
    let transport = {
        let mux = fw_esp32_common::radio_link::LinkMuxTransport::new(
            transport,
            radio_port,
            embassy_time::Delay,
        );
        #[cfg(lp_split)]
        let mux = mux.with_update_hook(ota::radio_update_hook);
        if ble_started {
            mux.with_upkeep_hook(ble::refresh_advertised_name)
        } else {
            mux
        }
    };

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
    // loop" intact: two readiness classifiers grep for it
    // (lpa-studio-core browser_serial_readiness, lp-cli fwcheck). The
    // version suffix is additive only.
    esp_println::println!(
        "[INIT] fw-esp32 initialized, starting server loop... proto={} commit={} dirty={}",
        lpc_wire::WIRE_PROTO_VERSION,
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
    );
    spawner.spawn(engine_task(app).unwrap());
}

/// The server loop, as its own task (spawned by the engine door): the door
/// returns to the core, which has nothing left to do.
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
            move |now_ms| {
                watchdog.feed(now_ms);
                log_heartbeat_stack_lines();
            },
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
            move |now_ms| {
                watchdog.feed(now_ms);
                log_heartbeat_stack_lines();
            },
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

    #[cfg(feature = "test_seam_abi")]
    {
        use tests::test_seam_abi::run_seam_abi;
        run_seam_abi(spawner).await;
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

/// A split build after `core_boot`: mark a trial started, then enter the
/// engine the header and the boot records agree on, or stay core-only.
#[cfg(all(lp_split, not(fw_harness)))]
async fn split_boot(mut core: CoreBoot) {
    let mut state = core::mem::replace(&mut core.ota_state, ota::BootState::placeholder());
    // Test images only (never in a build def): a trial core that dies, or
    // hangs, after marking itself attempted and before it starts — what the
    // loader's rollback and cold cap exist for.
    #[cfg(feature = "fixture-trial-dies")]
    if state.on_trial() {
        panic!("fixture-trial-dies: dying on trial before started");
    }
    #[cfg(feature = "fixture-trial-hangs")]
    if state.on_trial() {
        log::error!("fixture-trial-hangs: hanging on trial before started");
        loop {
            core::hint::spin_loop();
        }
    }
    // Radios and links are up: from here a power cycle never counts
    // against a trial.
    ota::mark_started(&mut state);

    let engine = if state.on_trial() {
        Err(ota::CoreOnlyReason::OnTrial)
    } else {
        find_engine(&state).map_err(|e| ota::CoreOnlyReason::NoEngine(e.describe()))
    };
    // The engine guard (DD34): the first boot after a USB flash hashes the
    // engine once against the digest slot before entering it.
    let engine = match engine {
        Ok((entry, len)) if !state.confirmed() => {
            match ota::engine_guard(&mut state, len, &engine_digest()) {
                Ok(()) => Ok((entry, len)),
                Err(why) => Err(ota::CoreOnlyReason::NoEngine(why)),
            }
        }
        other => other,
    };
    let incomplete = lp_recovery::snapshot()
        .map(|s| s.consecutive_incomplete_boots)
        .unwrap_or(0);
    let engine = match engine {
        Ok((_, engine_len)) if incomplete >= ota::INCOMPLETE_BOOTS_TO_CORE_ONLY => {
            Err(ota::CoreOnlyReason::EngineKeepsCrashing {
                boots: incomplete,
                engine_len,
            })
        }
        other => other,
    };
    log_boot_state(&state, &engine);
    // What the update session needs, in both modes: the image's identity,
    // the device store's `secrets` and `open` (the core reads only those,
    // and never writes the file), and how far the USB link is trusted.
    let identity = core_identity();
    let access = lpc_update::board::AccessFacts::from_file(
        &lpa_server::access_store::device_store_at_boot(core.base_fs.as_ref(), core.fs_boot_state),
    );
    let usb_trust = if cfg!(feature = "fixture-usb-untrusted") {
        log::warn!("[OTA] fixture-usb-untrusted: the USB link is NOT trusted (a test image)");
        lpc_update::board::LinkTrust::Untrusted
    } else {
        lpc_update::board::LinkTrust::Trusted
    };
    // Where the update session answers: USB, and the radio links.
    let links = ota::UpdateLinks {
        usb: core.usb_link,
        #[cfg(feature = "ble")]
        radio: core.radio_port,
    };
    // The radio links' mode follows the choice, decided here — in the same
    // synchronous run as `core_boot`, so before the BLE task has run at all,
    // and a connection that subscribes waits for it (`wait_for_mode`):
    // core-only's links advertise the wide receive window from their SYN.
    #[cfg(feature = "ble")]
    core.radio_port.decide_mode(match engine {
        Ok(_) => fw_esp32_common::radio_link::RadioLinkMode::Serve,
        Err(_) => fw_esp32_common::radio_link::RadioLinkMode::Update,
    });
    match engine {
        Ok((entry, len)) => {
            ota::install_running_hook(links, state, identity, len, access, usb_trust);
            entry(core)
        }
        Err(why) => {
            let CoreBoot {
                spawner,
                usb_link,
                watchdog,
                rmt_peripheral,
                base_fs,
                #[cfg(feature = "ble")]
                radio_port,
                #[cfg(all(
                    feature = "radio",
                    not(any(
                        feature = "stress_s2",
                        feature = "stress_s3",
                        feature = "desk_espnow_meter"
                    ))
                ))]
                radio_driver,
                ..
            } = core;
            // The ESP-NOW driver owns the Wi-Fi controller, and dropping it
            // deinitializes Wi-Fi — which, with the radios in coexistence,
            // took Bluetooth off the air too: core-only logged "advertising"
            // and no central ever saw it (the fixture C6, 2026-10-06, the
            // OTA M7 pre-walk). Core-only serves radio links now, so it
            // keeps the driver for good; it never returns (every committed
            // piece ends in a reset), so leaking it is holding it.
            #[cfg(all(
                feature = "radio",
                not(any(
                    feature = "stress_s2",
                    feature = "stress_s3",
                    feature = "desk_espnow_meter"
                ))
            ))]
            core::mem::forget(radio_driver);
            // The update light: the strip the engine recorded, if any.
            let record = base_fs
                .read_file(lpc_update::STATUS_LIGHT_PATH.as_path())
                .ok();
            let light = ota::StatusLight::new(record.as_deref(), rmt_peripheral);
            // Its own task, like the engine's server loop: awaited here, the
            // core-only future (its session and window) would be built in
            // the main task's poll frame, and the engine path — which runs
            // nested in that frame — would start that much deeper.
            spawner.spawn(
                core_only_task(ota::CoreOnly {
                    usb_link,
                    #[cfg(feature = "ble")]
                    radio_port,
                    watchdog,
                    state,
                    why,
                    identity,
                    access,
                    usb_trust,
                    entropy: fill_random,
                    light,
                    network_saved: core.network_saved,
                })
                .unwrap(),
            );
        }
    }
}

/// Core-only, as a task of its own (see `split_boot`).
#[cfg(all(lp_split, not(fw_harness)))]
#[embassy_executor::task]
async fn core_only_task(ctx: ota::CoreOnly) {
    ota::core_only(ctx).await
}

/// The image's identity, as the update session reports it: every field from
/// the build itself (the build id static, the patched digest slot, the
/// manifest core's words).
#[cfg(all(lp_split, not(fw_harness)))]
fn core_identity() -> ota::CoreIdentity {
    ota::CoreIdentity {
        build_id: BUILD_ID,
        digest: engine_digest(),
        target: crate::MANIFEST_TARGET,
        chip: crate::MANIFEST_CHIP,
        version: env!("LP_APP_VERSION"),
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    }
}

/// The one boot-state line: where this core is and how it came to run, the
/// loader, the region, the page, the engine (or why not) and the digest the
/// core carries for it.
#[cfg(all(lp_split, not(fw_harness)))]
fn log_boot_state(
    state: &ota::BootState,
    engine: &Result<(fn(CoreBoot), u32), ota::CoreOnlyReason>,
) {
    let digest = engine_digest();
    let region_end = state.layout.map_or(0, |l| l.region_end);
    let id = &BUILD_ID;
    let id_len = id.iter().position(|b| *b == 0).unwrap_or(id.len());
    let build = core::str::from_utf8(&id[..id_len]).unwrap_or("?");
    let engine_words: alloc::string::String = match engine {
        Ok((_, len)) => alloc::format!("engine {len} B"),
        Err(ota::CoreOnlyReason::OnTrial) => "engine not started (trial)".into(),
        Err(ota::CoreOnlyReason::NoEngine(why)) => alloc::format!("no engine: {why}"),
        Err(ota::CoreOnlyReason::EngineKeepsCrashing { boots, .. }) => {
            alloc::format!("engine keeps crashing ({boots} incomplete boots)")
        }
    };
    log::info!(
        "[CORE] core @{:#x} +{} ({}) build {build} · loader v{} · region end {region_end:#x} · \
         page {:#x} · {engine_words} · digest {:02x}{:02x}{:02x}{:02x}",
        state.core_off,
        state.core_len,
        state.standing(),
        state.loader_version,
        ota::page_size(),
        digest[0],
        digest[1],
        digest[2],
        digest[3],
    );
    if let Some(why) = state.untrusted {
        log::error!("[CORE] boot state not trusted ({why}) — nothing is written this boot");
    }
    if state.rolled_back() {
        log::warn!(
            "[CORE] rolled back: the newer core (build {:#010x}) failed its trial",
            state.failed_build.unwrap_or(0)
        );
    }
    if state.counted_cold_retry {
        log::warn!("[CORE] trial cold retry counted: the last boot died before it started");
    }
}

/// The engine's header, the first bytes of the engine region (split
/// builds): the `#[repr(C)]` mirror of `lp_bootctl::engine_header`'s v1
/// layout. The core never names it — it reads it through a plain address —
/// so no relocation in the core points into the engine, and the split tool's
/// reachability walk from the core's roots never crosses into it.
#[cfg(all(lp_split, not(fw_harness)))]
#[repr(C)]
struct EngineHeader {
    magic: u32,
    version: u16,
    header_len: u16,
    entry: fn(CoreBoot),
    /// Patched by the packager: `engine.bin`'s length.
    len: u32,
    build_id: [u8; lp_bootctl::engine_header::ENGINE_BUILD_ID_LEN],
    /// Patched by the packager: CRC-32 of the bytes before it.
    crc: u32,
    commit: u32,
}

#[cfg(all(lp_split, not(fw_harness)))]
const _: () = {
    use core::mem::{offset_of, size_of};
    use lp_bootctl::engine_header as eh;
    assert!(offset_of!(EngineHeader, entry) == eh::ENGINE_ENTRY_OFFSET);
    assert!(offset_of!(EngineHeader, len) == eh::ENGINE_LEN_OFFSET);
    assert!(offset_of!(EngineHeader, build_id) == eh::ENGINE_BUILD_ID_OFFSET);
    assert!(offset_of!(EngineHeader, crc) == eh::ENGINE_CRC_OFFSET);
    assert!(offset_of!(EngineHeader, commit) == eh::ENGINE_COMMIT_OFFSET);
    assert!(size_of::<EngineHeader>() == eh::ENGINE_HEADER_LEN);
};

/// This build's id, `"<version>+<commit>"`: core and engine carry the same
/// one because they come from the same link. A named static, so the split
/// tool can read the core's copy and check the engine header's against it.
#[cfg(all(lp_split, not(fw_harness)))]
#[unsafe(no_mangle)]
static LP_BUILD_ID: [u8; lp_bootctl::engine_header::ENGINE_BUILD_ID_LEN] =
    lp_bootctl::engine_header::build_id_field(
        concat!(env!("LP_APP_VERSION"), "+", env!("LP_BUILD_COMMIT")).as_bytes(),
    );
#[cfg(all(lp_split, not(fw_harness)))]
use LP_BUILD_ID as BUILD_ID;

/// The engine digest slot: SHA-256 of this build's `engine.bin` exactly as
/// flashed, patched in by the packager (`lp_bootctl::engine_digest`). Named
/// so the split tool finds it, and a core root there, so it is never placed
/// in the engine.
#[cfg(all(lp_split, not(fw_harness)))]
#[unsafe(no_mangle)]
static LP_ENGINE_DIGEST: [u8; lp_bootctl::engine_digest::ENGINE_DIGEST_LEN] =
    lp_bootctl::engine_digest::ENGINE_DIGEST_UNPATCHED;

/// The digest the core carries: read volatile, so the compiler cannot fold
/// the zeros it linked.
#[cfg(all(lp_split, not(fw_harness)))]
fn engine_digest() -> [u8; 32] {
    // SAFETY: a plain read of a static the packager patched in flash.
    let slot = unsafe { core::ptr::read_volatile(&raw const LP_ENGINE_DIGEST) };
    lp_bootctl::engine_digest::decode(&slot).unwrap_or([0; 32])
}

#[cfg(all(lp_split, not(fw_harness)))]
#[unsafe(link_section = ".engine_header")]
#[used]
static ENGINE_HEADER: EngineHeader = EngineHeader {
    magic: lp_bootctl::engine_header::ENGINE_MAGIC,
    version: lp_bootctl::engine_header::ENGINE_HEADER_VERSION,
    header_len: lp_bootctl::engine_header::ENGINE_HEADER_LEN as u16,
    entry: lp_engine_entry,
    len: 0,
    build_id: lp_bootctl::engine_header::build_id_field(
        concat!(env!("LP_APP_VERSION"), "+", env!("LP_BUILD_COMMIT")).as_bytes(),
    ),
    crc: 0,
    commit: lp_bootctl::engine_header::ENGINE_COMMITTED,
};

/// The engine's entry and length, if a committed engine of this build is in
/// the room the layout leaves it. Maps the header's page, validates the
/// header, then maps exactly its length.
#[cfg(all(lp_split, not(fw_harness)))]
fn find_engine(
    state: &ota::BootState,
) -> Result<(fn(CoreBoot), u32), lp_bootctl::EngineHeaderError> {
    use lp_bootctl::EngineHeaderError;
    use lp_bootctl::engine_header::ENGINE_HEADER_LEN;
    let room = state.engine_room();
    if room.is_empty() || room.start % ota::page_size() != 0 {
        return Err(EngineHeaderError::DoesNotFit);
    }
    ota::map_engine(room.start, ENGINE_HEADER_LEN as u32);
    let mut bytes = [0u8; ENGINE_HEADER_LEN];
    for (i, b) in bytes.iter_mut().enumerate() {
        // SAFETY: the header's page is mapped (erased flash reads as 0xff);
        // volatile so the compiler cannot assume anything about bytes it
        // did not write.
        *b = unsafe { core::ptr::read_volatile((ota::ENGINE_VADDR as *const u8).add(i)) };
    }
    let header = lp_bootctl::EngineHeader::decode(&bytes)?;
    header.validate(&BUILD_ID, room.len())?;
    ota::map_engine(room.start, header.len);
    // SAFETY: a committed header of this build, from this link: its entry
    // is `lp_engine_entry`'s address in the engine now mapped behind it.
    let entry: fn(CoreBoot) = unsafe { core::mem::transmute(header.entry as usize) };
    Ok((entry, header.len))
}

// Same gate as its only caller, `lp_engine_entry`: the hardware harnesses
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
