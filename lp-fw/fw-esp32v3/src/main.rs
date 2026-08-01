//! LightPlayer firmware for the classic ESP32 ("v3", WROOM-32E, Xtensa LX6).
//!
//! Boots the LightPlayer app: `LpServer` over a **UART0** transport, backed by
//! a littlefs filesystem in the `lpfs` partition. M3-P1 of the classic-ESP32
//! bring-up roadmap
//! (`~/.photomancer/planning/lp2025/2026-07-31-1444-classic-esp32-bringup/`),
//! which replays fw-esp32s3's app-layer walk on this chip.
//!
//! ## What this build has, and what it deliberately does not
//!
//! Two of eight `lpa-server` node gates are on (see `Cargo.toml`):
//! `node-shader` and `node-fixture`, the same pair as fw-esp32s3. Every other
//! kind loads inert.
//!
//! The graphics backend is the real `TargetLpvmGraphics`, so a GLSL shader
//! pushed here *compiles* to Xtensa machine code on the board — but it cannot
//! yet *execute*. See the comment at its construction site in
//! [`boot_firmware`]; that wiring is M3-P2.
//!
//! Absent on purpose:
//!
//! - **Output.** No WS281x driver is registered, so `HardwareSystem` resolves
//!   no output endpoints and the output node reports a clean failure. M4.
//! - **Crash recovery.** No `lp-recovery` RTC ledger, no RWDT, so the panic
//!   handler below prints and resets rather than leaving a breadcrumb for the
//!   next boot. The server loop's `lp_recovery::snapshot()` /
//!   `mark_boot_complete()` calls degrade to no-ops with no backend installed.
//!   M7 ports the backend (it needs classic-specific RTC-fast-RAM and
//!   `SocResetReason` constants, which are not P1's scope).
//! - **Radio.** Linked only behind `radio_ram_probe`, which replaces the
//!   entrypoint entirely (M2-P3's RAM ledger).
//!
//! ## Panic posture
//!
//! Abort tier (ADR `2026-07-29-per-chip-fw-toolchains`), same tier as
//! fw-esp32s3: `panic=abort` (`.cargo/config.toml`), no `unwinding`, no
//! `.eh_frame`.

#![no_std]
#![no_main]

// The server path is the whole LightPlayer stack. The hello and probe
// entrypoints install the allocator but never name `alloc` themselves, and
// `unused_extern_crates` is deny-by-default in this workspace's lint table.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
extern crate alloc;

// `board::esp32v3::init` is the server path's sole `esp_hal::init` call site.
// The hello/probe entrypoint at the bottom of this file keeps its own inline
// init on purpose: it needs the `WIFI` peripheral that `init_board` does not
// hand back, and it is M2-P3's measured code, worth preserving byte for byte.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
mod board;
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
mod flash_storage;
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
mod serial;

#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
use {
    alloc::{boxed::Box, rc::Rc, string::String, sync::Arc},
    board::esp32v3::init::{init_board, start_runtime},
    core::cell::RefCell,
    flash_storage::{LpFlashStorage, LpfsPartition, lpfs_config},
    fw_esp32_common::hardware::manifest_loader::load_hardware_manifest,
    fw_esp32_common::output::provider::Esp32OutputProvider,
    fw_esp32_common::server_loop::run_server_loop,
    fw_esp32_common::time::Esp32TimeProvider,
    fw_esp32_common::{boot, logger, lp_fs, transport},
    lp_gfx_lpvm::TargetLpvmGraphics,
    lpa_server::{LpGraphics, LpServer},
    lpc_hardware::{HardwareSystem, HwRegistry},
    lpc_shared::output::OutputProvider,
    lpfs::LpFsMemory,
    lpfs::lp_path::AsLpPath,
    serial::io_task,
};

esp_bootloader_esp_idf::esp_app_desc!();

/// Heap for the allocator.
///
/// The zero-sum split on this chip is tighter than anywhere else in the
/// family. esp-hal's `dram_seg` is `0x3FFB_0000..0x3FFE_0000` — **192 KB**,
/// against the S3's 341,760 B — and `.data`, `.bss` (which is where this
/// arena lives) and `.stack` all come out of it, `.stack` taking whatever is
/// left (esp-hal's `stack.x`).
///
/// 100 KB was M2's boot-skeleton figure, chosen when nothing on this board
/// allocated in anger. The server stack does: littlefs caches, the project
/// model, and — the number that matters for M3-P2 — GLSL compilation, which
/// needed a 240 KB heap on the S3 at its measured OOM. That is not reachable
/// here at any setting, which is exactly the measurement G-M3 exists to
/// evaluate.
///
/// ⚠️ **"As high as it links" is NOT the ceiling to aim for.** Measured on
/// this image: 160 KB fails the link by 4,064 B
/// (`stack.x:11 cannot move location counter backwards`), so the hard limit is
/// ≈155.9 KB — at which `.stack` is *zero* and the board cannot run. The real
/// constraint is stack headroom. At 110 KB the linked image is `.data` 15,212
/// + `.bss` 134,256 (incl. this 112,640 B arena) + `.stack` 47,136, which
/// keeps the stack near fw-esp32s3's proven 52,896 B — margin the Xtensa
/// windowed ABI's large frames and the recursive GLSL parser both want.
/// Booted and verified on the desk DOM-Z-102: 103,916 B free at idle.
///
/// M3-P2 may reasonably trade some of that stack for heap once it has
/// measured what an on-device compile actually needs of each.
///
/// The tempting next lever is `dram2_seg` (`0x3FFE_7E30`, 98,768 B) as a
/// second `esp_alloc` region — but see the ⚠️ at the graphics construction
/// site first: that segment *overlaps*
/// `lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT`
/// (`0x3FFE_8000..0x3FFF_F000`), so handing it to the allocator would hand the
/// JIT's code region to the heap.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
const HEAP_SIZE: usize = 110 * 1024;

/// Bare hello build (`--no-default-features --features esp32`): M2-P1's
/// skeleton, kept buildable as the minimal bring-up image.
#[cfg(all(not(feature = "server"), not(feature = "radio_ram_probe")))]
const HEAP_SIZE: usize = 100 * 1024;

/// Probe heap: the radio stack's own DRAM statics come out of the same 192 KB
/// `dram_seg` the arena does, so the arena must shrink for the image to link
/// at all. 72 KB is the experiment repo's proven radio-coexistent size on this
/// chip (led-lab-esp32 `test_stress`).
#[cfg(feature = "radio_ram_probe")]
const HEAP_SIZE: usize = 72 * 1024;

/// Abort-tier panic handler. Print what panicked, then reset so the next boot
/// starts clean — there is no ledger yet to stage a breadcrumb into (contrast
/// fw-esp32s3's `recovery::panic_path::stage_and_reset`).
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");
    esp_hal::system::software_reset()
}

/// Heap free/used for the heartbeat. A chip fact `fw-esp32-common` must not
/// know, so it is injected.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
fn esp32_memory_stats() -> Option<(u32, u32)> {
    Some((
        esp_alloc::HEAP.free().min(u32::MAX as usize) as u32,
        esp_alloc::HEAP.used().min(u32::MAX as usize) as u32,
    ))
}

/// Everything `main` needs to hand to the server loop.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
struct FirmwareApp {
    server: LpServer,
    transport: transport::StreamingMessageRouterTransport,
    time_provider: Esp32TimeProvider,
}

#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
#[inline(never)]
fn boot_firmware(spawner: embassy_executor::Spawner) -> FirmwareApp {
    // ⚠️ `init_board` takes the `esp_hal` peripheral singleton, and taking it
    // twice panics. This is the app path's ONLY call to `esp_hal::init`.
    let (sw_int, timg0, uart0, flash) = init_board();
    // The heap is main.rs's, not the board's — mirroring fw-esp32s3.
    esp_alloc::heap_allocator!(size: HEAP_SIZE);
    esp_println::println!("[INIT] fw-esp32v3 boot");
    esp_println::println!("[INIT] chip=esp32 arch=xtensa heap={HEAP_SIZE}");

    start_runtime(timg0, sw_int);
    esp_println::println!("[INIT] runtime started");

    match uart0 {
        Ok(uart) => {
            spawner.spawn(io_task(uart).unwrap());
            esp_println::println!("[INIT] I/O task spawned (uart0 115200 8N1)");
        }
        Err(error) => {
            // The board keeps booting: `esp_println` writes UART0's FIFO
            // directly and still reaches a monitor, so a reachable-but-mute
            // device that says why beats a reset loop.
            esp_println::println!(
                "[ERROR] UART0 config failed ({error:?}); no host link this boot"
            );
        }
    }

    // From here on `log::*` reaches the host over the same serial link; the
    // `esp_println!` lines above are the pre-transport ones.
    logger::init(serial::io_task::log_write_to_outgoing);

    let (incoming, _) = serial::io_task::get_message_channels();
    let (write_request, write_result) = serial::io_task::get_server_write_channels();
    let transport =
        transport::StreamingMessageRouterTransport::new(incoming, write_request, write_result);

    let base_fs = mount_filesystem(flash);

    // The compiled-in fallback is the DOM-Z-102 profile — the desk board and
    // the roadmap's WLED-class exemplar. An `/hardware.json` on the device
    // overrides it, which is how a different classic carrier gets described.
    let hardware_manifest = load_hardware_manifest(
        base_fs.as_ref(),
        lpc_hardware::default_esp32v3_hardware_manifest,
    );
    log::info!(
        "[fw-esp32v3] hardware manifest: {} ({})",
        hardware_manifest.board_id(),
        hardware_manifest.board_name()
    );
    let hardware_registry = Rc::new(HwRegistry::new(hardware_manifest));
    // No WS281x driver, no button, no radio: none is ported. `HardwareSystem`
    // resolves the manifest's `/rmt/ws281xK` endpoints to nothing, so opening
    // an output channel fails cleanly rather than rendering into a stub. M4
    // registers the real driver here, the way fw-esp32s3 registers
    // `Esp32S3RmtWs281xDriver`.
    let hardware_system = Rc::new(HardwareSystem::new(Rc::clone(&hardware_registry)));

    // The provider itself is chip-agnostic and comes from fw-esp32-common
    // untouched.
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(Esp32OutputProvider::new(hardware_system)));

    // Stamped device identity: read the fs-root `/.lp/device.json` once at boot
    // for the hello (missing file → unstamped, `None`).
    let device_uid = lpa_server::device_identity::read_device_uid(base_fs.as_ref());

    // ⚠️⚠️ M3-P2 READ THIS. `TargetLpvmGraphics` resolves to `lpvm-native`'s
    // `NativeJitEngine` on Xtensa, so pushing a shader here runs the full
    // GLSL → LPIR → Xtensa emitter path **on the board**. What it does NOT do
    // on this chip is produce executable code:
    //
    //   * `rt_jit`'s default buffer is heap-backed and in-place, and this
    //     chip's heap (SRAM2, `0x3FFB_0000..`) has no I-bus view at all —
    //     fetching from it faults with EXCCAUSE=2. The S3's uniform
    //     `+0x6F_0000` alias does not exist here.
    //   * `lpvm_native::exec_addr`'s Xtensa arm knows this: a heap address
    //     hits its fall-through `debug_assert!` (compiled out in this release
    //     profile) and is returned as identity, which then faults on the first
    //     call.
    //
    // The fix is the *placed* path — `lpvm_native::codemem_esp32` +
    // `JitBuffer::Placed` + `link::link_jit_at` — which M1 landed and M3-P2
    // wires in. P1 deliberately does NOT touch it, so on this build a shader
    // COMPILES and then faults when the frame loop first calls it.
    //
    // Two facts P2 needs from here:
    //   1. Nothing in this crate reserves `CodeRegion::ESP32_DEFAULT`
    //      (`0x3FFE_8000 .. 0x3FFF_F000` D-bus). It does not need reserving
    //      from the *linker* — esp-hal's `dram_seg` ends at `0x3FFE_0000` — but
    //      it DOES overlap esp-hal's `dram2_seg` (`0x3FFE_7E30`, 98,768 B).
    //      If anyone adds dram2_seg as a second `esp_alloc` region to buy heap
    //      headroom, it must stop below `0x3FFE_8000` or the allocator and the
    //      JIT will hand out the same bytes.
    //   2. The frontend is passed, never defaulted: `LpGraphics::glsl_frontend`
    //      has no default impl so every host states its choice, and the device
    //      ships `LpsGlsl`.
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));

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
            package: String::from("fw-esp32v3"),
            commit: String::from(env!("LP_BUILD_COMMIT")),
            dirty: env!("LP_BUILD_DIRTY") == "true",
            profile: String::from(env!("LP_BUILD_PROFILE")),
        },
        device_uid,
    });

    // Auto-load a project at boot. fw-esp32s3 gates this on its recovery
    // subsystem's safe-mode verdict; with no ledger here there is no verdict
    // to consult, so the load is unconditional. That is the concrete cost of
    // deferring recovery to M7: a project that crashes the boot will keep
    // crashing it.
    boot::auto_load_project(&mut server);

    FirmwareApp {
        server,
        transport,
        time_provider: Esp32TimeProvider::new(),
    }
}

/// Mount the `lpfs` partition, falling back to RAM so an unformattable or
/// mis-flashed board still comes up reachable and can say so over the wire.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
fn mount_filesystem(flash: esp_hal::peripherals::FLASH<'static>) -> Box<dyn lpfs::LpFs> {
    let mut flash_storage = esp_storage::FlashStorage::new(flash);
    let Some(partition) = LpfsPartition::locate(&mut flash_storage) else {
        // Not a runtime condition: it means the image was flashed without
        // `--partition-table lp-fw/fw-esp32v3/partitions.csv` and espflash
        // silently substituted its own default. Say so rather than guess an
        // offset and erase running code.
        esp_println::println!(
            "[ERROR] no `lpfs` partition in the flashed table — reflash with \
             --partition-table lp-fw/fw-esp32v3/partitions.csv; using memory FS"
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

#[cfg(all(feature = "server", not(feature = "radio_ram_probe")))]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) {
    let app = boot_firmware(spawner);

    // ⚠️ The substring "fw-esp32 initialized, starting server loop" is matched
    // literally by `lpa_link::device_session::device_readiness` (and by lp-cli
    // fwcheck through it). It is chip-agnostic on purpose.
    esp_println::println!(
        "[INIT] fw-esp32 initialized, starting server loop... proto={} commit={} dirty={}",
        lpc_wire::WIRE_PROTO_VERSION,
        env!("LP_BUILD_COMMIT"),
        env!("LP_BUILD_DIRTY"),
    );

    // The watchdog feed is a no-op: no RWDT is armed (see the module docs),
    // and the server loop takes the feeder as a closure precisely so a chip
    // without one pays nothing.
    run_server_loop(
        app.server,
        app.transport,
        app.time_provider,
        esp32_memory_stats,
        |_now_ms| {},
    )
    .await;
}

/// Boot-to-hello entrypoint: the M2-P1 skeleton (bare build) and the M2-P3
/// radio RAM probe. Both replace the server app rather than extending it.
#[cfg(any(
    feature = "radio_ram_probe",
    all(not(feature = "server"), not(feature = "radio_ram_probe"))
))]
#[esp_hal::main]
fn main() -> ! {
    // `esp_hal::init` disables the RTC super watchdog (where present), the RTC
    // watchdog (RWDT), and both TIMG watchdogs unconditionally (esp-hal 1.1.1
    // `lib.rs::init`). `CpuClock::max()` is 240 MHz on this chip; printed and
    // measured timings assume the fast clock.
    let peripherals =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));

    // ⚠️ Load-bearing, not decorative — the baud-divisor fix. See the same
    // comment (at length) in `board::esp32v3::init::init_board`, which is the
    // server path's copy: `esp-println`'s `uart` feature writes UART0's FIFO
    // but never programs the divisor, and `esp_hal::init` above has just
    // moved the clock. Constructing this `Uart` is what makes every
    // `esp_println!` below legible. It must stay bound, not dropped.
    let _uart0 = esp_hal::uart::Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
        .expect("uart0 config")
        .with_tx(peripherals.GPIO1)
        .with_rx(peripherals.GPIO3);

    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    esp_println::println!("[INIT] fw-esp32v3 boot");
    esp_println::println!(
        "[INIT] chip=esp32 arch=xtensa heap_free={}",
        esp_alloc::HEAP.free()
    );

    // M2-P3 RAM probe: bring the radio stack all the way up to an initialised
    // STA controller — the deployment-shaped memory layout — printing the heap
    // ledger at each stage. `[PROBE]` lines are the phase deliverable; the
    // stage traces make a wedged boot attributable.
    #[cfg(feature = "radio_ram_probe")]
    {
        esp_println::println!(
            "[PROBE] stage=pre_rtos heap_size={HEAP_SIZE} heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let sw_int =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
        esp_println::println!(
            "[PROBE] stage=rtos_started heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        let (mut controller, _interfaces) =
            esp_radio::wifi::new(peripherals.WIFI, Default::default())
                .expect("radio probe: wifi init");
        esp_println::println!(
            "[PROBE] stage=wifi_new heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        let station_config = esp_radio::wifi::sta::StationConfig::default();
        controller
            .set_config(&esp_radio::wifi::Config::Station(station_config))
            .expect("radio probe: sta config");
        esp_println::println!(
            "[PROBE] stage=sta_started heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        // Keep the controller alive so the heartbeat below reports the
        // steady-state radio-on ledger, not a post-drop one.
        core::mem::forget(controller);
        esp_println::println!("[PROBE] done — heartbeat shows steady-state radio-on heap");
    }

    esp_println::println!("[INIT] ready");

    let delay = esp_hal::delay::Delay::new();
    let mut uptime_s: u32 = 0;
    loop {
        delay.delay_millis(1000);
        uptime_s += 1;
        esp_println::println!(
            "[HEARTBEAT] uptime_s={uptime_s} heap_free={}",
            esp_alloc::HEAP.free()
        );
    }
}
