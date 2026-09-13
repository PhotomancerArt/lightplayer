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
// Xtensa asm is still unstable upstream, and two modules need it:
// `board::esp32s3::cycle_counter` (`rsr.ccount`, harness-only) and
// `board::esp32s3::fpu` (`wsr.cpenable`, **both** paths). It was scoped to
// `fw_harness` while only the former existed; arming the FPU is app-path work
// (M7 D6 / P5), and no safe wrapper for `CPENABLE` exists — xtensa-lx 0.13
// exposes interrupts and the timer, and neither it nor esp-hal 1.1.1 touches
// the register at all. So the gate is unconditional now, deliberately.
#![feature(asm_experimental_arch)]
#![allow(
    unstable_features,
    reason = "asm_experimental_arch is required for Xtensa's CPENABLE (arming \
              the FPU for compiled float code) and CCOUNT (cycle counter)"
)]

// The JIT harness allocates (JIT buffers, module tables); the app path is the
// whole server stack. `test_button` also needs it: the button driver's
// registry/endpoint types are alloc-based, same as on fw-esp32c6. So does the
// compile harness, which is the one that allocates hardest — the heap either
// side of every slice is the figure that payload exists to report.
// `test_backtrace_oracle` is the exception — it is deliberately
// allocation-free, because it exercises a walk the panic path takes, and the
// panic path must not allocate.
#[cfg(any(
    not(fw_harness),
    feature = "test_xt_jit_corpus",
    feature = "test_button",
    feature = "test_shader_compile_incremental"
))]
extern crate alloc;

// The build's self-description, embedded as a scannable blob (extracted by
// `lp-cli firmware show` and reported on ServerHello in M4). Feature truth
// comes from the engine's own cfg! derivation; only embedder facts are named
// here. `flashAppBytes` is parsed from partitions.csv by build.rs.
#[cfg(feature = "server")]
lpc_model::lp_embed_manifest_core! {
    package: env!("CARGO_PKG_NAME"),
    chip_family: "esp32",
    chip: "esp32s3",
    cargo_target: "xtensa-esp32s3-none-elf",
    profile: env!("LP_BUILD_PROFILE"),
    commit: env!("LP_BUILD_COMMIT"),
    dirty: lpc_model::manifest::str_eq(env!("LP_BUILD_DIRTY"), "true"),
    wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    features: [
        lpa_server::ENGINE_FEATURE_FRAGMENT,
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::GfxLpvm),
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::SvcButton),
        lpc_model::manifest::feature_fragment(
            cfg!(feature = "float-f32"),
            lpc_model::LpFeature::ShaderF32,
        ),
    ],
    limits_json: concat!("{\"flashAppBytes\":", env!("LP_FLASH_APP_BYTES"), "}"),
}

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
#[cfg(not(fw_harness))]
mod stack_probe;
#[cfg(fw_harness)]
mod tests;

#[cfg(not(fw_harness))]
use {
    alloc::{boxed::Box, rc::Rc, sync::Arc},
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

    #[cfg(feature = "test_xt_fp_conformance")]
    {
        drop(peripherals);
        tests::xt_fp_conformance::run_all();
    }

    // The compile harness builds its engine and compiles out of the heap this
    // function already installed; it claims no peripheral of its own, so the
    // raw singleton is simply dropped (the same shape as the JIT corpus).
    #[cfg(feature = "test_shader_compile_incremental")]
    {
        drop(peripherals);
        tests::incremental_shader_compile::run();
    }
}

/// Heap free/used, and the board's three-line memory ledger. A chip fact
/// `fw-esp32-common` must not know, so it is injected.
///
/// **The ledger is elicited, never periodic.** The server calls this injected
/// probe on a project load, a project unload, a stop-all, a client
/// `runtime_status`, and either side of a shader compile (through
/// `lpc_shared::memory`'s global, which `LpServer::new_*` installs from this
/// same function pointer) — and nowhere else. The five-second heartbeat takes
/// [`heartbeat_memory_stats`] instead, which reads the same counters and
/// prints nothing. That split is the classic's
/// (`fw-esp32v3/src/main.rs`), and it is deliberate: a periodic printer floods
/// the very USB-Serial-JTAG link the transport runs on, while an elicited one
/// brackets the events whose memory cost anyone wants to read.
///
/// The three lines are the classic's, field for field and in its order, so
/// that one replay comparator reads all three chips' transcripts:
///
/// ```text
/// [stack] heartbeat: high-water <used> B of <total> B (<headroom> B headroom)
/// [MEM] free=<free> used=<used> largest_free=<largest> retry_saves=<n>
/// [JIT] used=… peak=… cap=… spans=… peak_spans=… allocs=… frees=… fails=… largest_free=…
/// ```
///
/// Two fields on this chip are **structural**: they hold the line shape and
/// carry no counter, because this image has nothing that could fill them
/// honestly. Neither is invented, and neither may be "improved" by finding
/// something nearby to print.
///
/// - **`retry_saves` is always `0`.** The classic has an OOM retry allocator
///   (`OOM_RETRY_SAVES`, incremented in its `handle_alloc_error` wrapper when
///   a retry rescues a failed allocation). This image has no such wrapper, so
///   there is no retry and nothing to count — `0` is the true number of saves,
///   not a missing one.
/// - **The whole `[JIT]` line is zeros.** The classic JITs into a *fixed SRAM0
///   code region* (`lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT`)
///   with its own arena and residency counters, reached through the
///   `xt-placed-code` feature. This chip does not enable that feature and does
///   not have a region: the S3's SRAM1 is dual-mapped, so a heap buffer is
///   executable through its I-bus alias (`lpvm_native::exec_addr`'s
///   `+0x6F_0000`) and the JIT allocates code straight from the `esp_alloc`
///   heap. Its residency is therefore already inside the `[MEM]` line's
///   `used`, and `cap=0` says exactly what is true — there is no reserved
///   region. Filling these from a global stats surface would mean adding one
///   to `lpvm-native`, which is a separate decision and not this phase's.
#[cfg(not(fw_harness))]
fn esp32_memory_stats() -> Option<(u32, u32)> {
    // One scan of the main stack per elicitation, a log line only when the
    // mark grows. On this chip `.stack` is the residual of `dram_seg` after
    // `HEAP_SIZE`, so its high-water mark is what sizes the heap — see
    // `stack_probe`.
    stack_probe::log_if_grown("heartbeat");
    let free = esp_alloc::HEAP.free();
    let used = esp_alloc::HEAP.used();
    let largest = recovery::panic_path::largest_free_block();
    // Structural — see this function's doc comment. No OOM retry allocator on
    // this chip, so the honest count of retries that saved an allocation is 0.
    let retry_saves = 0u32;
    esp_println::println!(
        "[MEM] free={free} used={used} largest_free={largest} retry_saves={retry_saves}"
    );
    // Structural — see this function's doc comment. The S3 JITs into the heap,
    // not into a reserved code region, so there is no arena to report and the
    // residency these fields would carry is already in `used` above. Printed
    // anyway, and unconditionally, because the triple is the unit the replay
    // comparator reads: a chip that prints two lines where another prints
    // three is a diff in the transcript, not a gap in the data.
    esp_println::println!(
        "[JIT] used=0 peak=0 cap=0 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 largest_free=0"
    );
    Some((
        free.min(u32::MAX as usize) as u32,
        used.min(u32::MAX as usize) as u32,
    ))
}

/// The `ClientRequest::Reboot` action: the chip reset the chip-agnostic
/// server cannot perform itself.
///
/// Called only after the ack frame is written (`LpServer::tick_and_send`),
/// so the client reads its answer and then this board's boot banner. Not a
/// crash path: the boot was marked complete on the first served frame, long
/// before any request could arrive, so this reset never counts toward the
/// boot-loop safe-mode gate.
#[cfg(not(fw_harness))]
fn reboot_now() {
    log::info!("[REBOOT] client requested a restart");
    esp_hal::system::software_reset()
}

/// Heartbeat memory report: free/used plus the fragmentation evidence
/// (largest allocatable block) that previously reached nothing at all.
///
/// ⚠️ Reads the counters itself rather than calling [`esp32_memory_stats`],
/// and that is load-bearing, not duplication: this runs on the five-second
/// heartbeat, and the other function *prints*. Routing this through it would
/// put the three-line ledger on a periodic and flood the serial link — see
/// [`esp32_memory_stats`] for why the ledger is elicited only.
///
/// `oom_retry_saves` stays `None` — this chip has no retry allocator, and
/// `None` is the wire's way of saying "no such counter here" (the printed
/// `[MEM]` line has no `None`, so it says `retry_saves=0` instead; both are
/// structural, neither is invented).
#[cfg(not(fw_harness))]
fn heartbeat_memory_stats() -> Option<lpc_wire::server::MemoryStats> {
    let free = esp_alloc::HEAP.free().min(u32::MAX as usize) as u32;
    let used = esp_alloc::HEAP.used().min(u32::MAX as usize) as u32;
    Some(lpc_wire::server::MemoryStats {
        free_bytes: free,
        used_bytes: used,
        total_bytes: used.saturating_add(free),
        largest_free_block: Some(
            recovery::panic_path::largest_free_block().min(u32::MAX as usize) as u32,
        ),
        oom_retry_saves: None,
    })
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
    // Paint the main stack before anything deep runs, so the high-water report
    // measures the whole app (see `stack_probe`). After the arena is carved,
    // because the paint runs on the main stack and the arena is `.bss`, not
    // stack. Mirrors the classic's placement exactly.
    stack_probe::paint();
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
    server.set_hello_identity(
        lpc_wire::HelloIdentity::new(
            "fw-esp32s3",
            env!("LP_BUILD_COMMIT"),
            env!("LP_BUILD_DIRTY") == "true",
            env!("LP_BUILD_PROFILE"),
        )
        .with_device_uid(device_uid),
    );
    // The chip's own permanent identity (efuse): the factory MAC and the
    // silicon revision. The server cannot derive either.
    server.set_hardware_identity(chip_identity());
    // The board this firmware is running as, from the loaded manifest — the
    // catalog key a card needs to re-flash or wire a new project for it.
    server.set_board_id(Some(alloc::string::String::from(
        hardware_registry.manifest().board_id(),
    )));
    server.set_reboot_hook(Some(Rc::new(reboot_now)));
    // The one feature the server cannot see: whether the shader engine
    // linked into THIS image does native f32 math (`float-f32` is a fact of
    // this crate's Cargo graph, invisible from `Arc<dyn LpGraphics>`).
    // Same shape as the manifest macro above, which names it the same way.
    if cfg!(feature = "float-f32") {
        server.declare_embedder_features(&[lpc_model::LpFeature::ShaderF32]);
    }

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
        heartbeat_memory_stats,
        move |now_ms| watchdog.feed(now_ms),
    )
    .await;
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
        // No 802.15.4 radio on this part, so no EUI-64 (and no
        // `MAC_EXT` efuse field to build one from).
        eui64: None,
    }
}
