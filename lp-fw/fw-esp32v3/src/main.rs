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
//! Output is real: `output::rmt` drives WS281x strips from the RMT
//! peripheral, up to four at once — two RMT memory blocks per channel, so the
//! chip's eight slots become four 128-word windows on slots 0/2/4/6, and the
//! channel count comes from the board manifest's `/rmt/ws281xK` resources
//! (M4-P1; ADR `2026-07-31-lp-ws281x-multi-channel-driver-adoption`).
//!
//! Crash recovery is present as of M4-P2: `src/recovery/` installs the
//! `lp-recovery` RTC-RAM ledger, so a fault stages a breadcrumb that survives
//! the reset and is reported on the next boot. It was pulled forward from M7
//! because the WS281x open path faults faster than this chip can print
//! (`docs/defects/2026-08-01-classic-rmt-open-fault.md`) — the ledger is the
//! only channel that can name it. There is still **no RWDT**; see
//! `recovery::mod`'s docs for why arming one belongs to M7.
//!
//! Absent on purpose:
//!
//! - **Radio.** Linked only behind `radio_ram_probe`, which replaces the
//!   entrypoint entirely (M2-P3's RAM ledger).
//!
//! ## Panic posture
//!
//! Abort tier (ADR `2026-08-02-rv32-firmwares-are-abort-tier`), same tier as
//! every other firmware: `panic=abort` (`.cargo/config.toml`), no unwinder, no
//! `.eh_frame`.

#![no_std]
#![no_main]
// The app image installs its own `#[alloc_error_handler]` so an allocation
// failure reaches the RTC ledger as an `Oom` record carrying free/used, rather
// than as a generic panic with no heap numbers. Same reason fw-esp32c6 takes
// the feature; the esp toolchain is nightly-based, so it is available here too.
// `asm_experimental_arch`: Xtensa inline asm is unstable, and the APP core's
// idle loop is one `waiti 0` (`output::rmt::shared_driver::app_core_main`) —
// the same feature xtensa-lx-rt itself builds with.
#![cfg_attr(
    all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)),
    feature(alloc_error_handler, asm_experimental_arch)
)]
#![allow(
    unstable_features,
    reason = "alloc_error_handler + asm_experimental_arch required in no_std; the esp toolchain is nightly-based"
)]

// The server path is the whole LightPlayer stack. The hello and probe
// entrypoints install the allocator but never name `alloc` themselves, and
// `unused_extern_crates` is deny-by-default in this workspace's lint table.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
extern crate alloc;

// The build's self-description, embedded as a scannable blob (extracted by
// `lp-cli firmware show`, reported on ServerHello). Feature truth comes from
// the engine's own cfg! derivation; only embedder facts are named here.
// `flashAppBytes` is parsed from partitions.csv by build.rs.
#[cfg(feature = "server")]
lpc_model::lp_embed_manifest_core! {
    package: env!("CARGO_PKG_NAME"),
    chip_family: "esp32",
    chip: "esp32",
    cargo_target: "xtensa-esp32-none-elf",
    profile: env!("LP_BUILD_PROFILE"),
    commit: env!("LP_BUILD_COMMIT"),
    dirty: lpc_model::manifest::str_eq(env!("LP_BUILD_DIRTY"), "true"),
    wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    features: [
        lpa_server::ENGINE_FEATURE_FRAGMENT,
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::GfxLpvm),
    ],
    limits_json: concat!("{\"flashAppBytes\":", env!("LP_FLASH_APP_BYTES"), "}"),
}

// Hardware harnesses, selected by `test_*` features (build.rs's `fw_harness`
// cfg). Each replaces the app entrypoint rather than extending it — the same
// shape `radio_ram_probe` above already uses, deliberately not a second
// mechanism.
//
// Unlike fw-esp32s3's `test_loopback` / `test_button`, no harness here needs an
// app module: the FP rig touches no driver, so nothing is un-gated for it. Do
// not copy the S3's exceptions without a harness that earns one.
#[cfg(fw_harness)]
mod tests;

// `board::esp32v3::init` is the server path's sole `esp_hal::init` call site.
// The hello/probe entrypoint at the bottom of this file keeps its own inline
// init on purpose: it needs the `WIFI` peripheral that `init_board` does not
// hand back, and it is M2-P3's measured code, worth preserving byte for byte.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
mod board;
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
mod flash_storage;
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
mod output;
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
mod recovery;
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
mod serial;

#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
use {
    alloc::{boxed::Box, rc::Rc, sync::Arc},
    board::esp32v3::init::{init_board, start_runtime},
    core::cell::RefCell,
    flash_storage::{LpFlashStorage, LpfsPartition, lpfs_config},
    fw_esp32_common::hardware::manifest_loader::load_hardware_manifest,
    fw_esp32_common::server_loop::run_server_loop,
    fw_esp32_common::time::Esp32TimeProvider,
    fw_esp32_common::{boot, logger, lp_fs, transport},
    lp_gfx_lpvm::TargetLpvmGraphics,
    lpa_server::{LpGraphics, LpServer},
    lpc_hardware::{HardwareSystem, HwRegistry},
    lpc_shared::output::OutputProvider,
    lpfs::LpFsMemory,
    lpfs::lp_path::AsLpPath,
    output::{Esp32OutputProvider, Esp32V3RmtWs281xDriver},
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
/// ⚠️ This is no longer the whole heap. `dram2_seg`'s tail is now a **second**
/// `esp_alloc` region worth 72 KiB — see [`add_sram1_heap_region`]. This
/// constant sizes only the `dram_seg` arena, which is the one in zero-sum
/// competition with `.stack`; the second region costs `.stack` nothing,
/// which is exactly why it was worth reclaiming. Total heap is
/// `HEAP_SIZE + 73,728`.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
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

/// Abort-tier panic handler for the **app** image: stage a breadcrumb into the
/// RTC ledger, commit it, then print and reset.
///
/// The ordering — ledger before serial — is the whole point on this chip; see
/// `recovery::panic_path`'s module docs. There is no FIFO drain here: the drain
/// in the bare-image handler below exists to get bytes out before a reset this
/// handler controls, and once the record is already committed to RTC RAM,
/// spending 40 ms per line to protect the *serial copy* of information the next
/// boot will print anyway buys nothing.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    recovery::panic_path::stage_and_reset(info)
}

/// Allocation failure: record it as an `Oom` with the heap counters attached.
///
/// Rust's default handler panics, which the handler above would catch — but by
/// then the request size is only a formatted string and the free/used numbers
/// are gone. On a chip whose whole difficulty is a 110 KB arena, "how much was
/// left" is the question, so it gets its own path.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
#[alloc_error_handler]
fn on_alloc_error(layout: core::alloc::Layout) -> ! {
    recovery::panic_path::stage_oom_and_reset(layout)
}

/// The global allocator: `esp_alloc::HEAP` behind a bounded immediate retry.
///
/// Evidence, not caution: in every dome-scale OOM measured on this board
/// (2026-08-04, four failures at 1200/1500 LEDs and two at 900 with a heavy
/// shader), the OOM handler's *first* action — re-issuing the identical
/// `Layout` before touching anything — **succeeded** (`retry_ok=true`), yet
/// the null return had already been escalated to a device reset and, two
/// boots later, a quarantined project. Whatever releases memory between the
/// null and the handler (see `docs/defects/2026-08-02-classic-oom-retry-succeeds.md`
/// — the window is real, its occupant still unidentified), the caller's
/// request was servable and the device died anyway. Retrying at the site
/// converts those deaths into successful allocations; a genuine exhaustion
/// still fails after [`OOM_RETRIES`] attempts and reaches
/// [`on_alloc_error`] exactly as before, where the handler's own retry probe
/// remains as the diagnostic that says whether this wrapper's budget was too
/// small.
///
/// [`OOM_RETRY_SAVES`] counts conversions and rides the `[MEM]` heartbeat
/// line — a nonzero value on a healthy board is this wrapper paying rent; a
/// climbing one says the heap is running at the edge.
///
/// Unconditional across this crate's builds: esp-alloc's own
/// `global-allocator` default is off on every edge that reaches it (this
/// crate's declaration and esp-rtos's both say `default-features = false`),
/// so this is the one allocator everywhere, `radio_ram_probe` included.
struct RetryingHeap;

/// Immediate re-attempts after a null return before the failure is real.
/// Every observed save needed exactly one; the rest are margin at the cost
/// of a few loads on a path that otherwise ends in a reset.
const OOM_RETRIES: u32 = 4;

/// Allocations that failed once and succeeded on an immediate retry — each
/// of these was a device reset before [`RetryingHeap`] existed.
static OOM_RETRY_SAVES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[global_allocator]
static GLOBAL_ALLOC: RetryingHeap = RetryingHeap;

unsafe impl core::alloc::GlobalAlloc for RetryingHeap {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        // SAFETY: same contract as the wrapped allocator; this wrapper adds
        // only repetition, never a different layout or pointer.
        let p = unsafe { core::alloc::GlobalAlloc::alloc(&esp_alloc::HEAP, layout) };
        if !p.is_null() {
            return p;
        }
        for _ in 0..OOM_RETRIES {
            let p = unsafe { core::alloc::GlobalAlloc::alloc(&esp_alloc::HEAP, layout) };
            if !p.is_null() {
                OOM_RETRY_SAVES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                return p;
            }
        }
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        // SAFETY: forwarded verbatim to the allocator that produced `ptr`.
        unsafe { core::alloc::GlobalAlloc::dealloc(&esp_alloc::HEAP, ptr, layout) }
    }
    // `alloc_zeroed`/`realloc` deliberately stay at the trait defaults:
    // `EspHeap`'s own `GlobalAlloc` impl defines only `alloc`/`dealloc`, so
    // the defaults compose identically — and route through the retry above.
}

/// Abort-tier panic handler for the bare-hello and radio-probe images, which
/// link no ledger: print what panicked, then reset so the next boot starts
/// clean.
///
/// ⚠️ The spin before the reset is load-bearing, not politeness. `esp-println`'s
/// `uart` feature writes UART0's TX FIFO and returns; `software_reset()`
/// discards whatever has not been clocked out yet. Without the drain, a panic
/// on this board printed exactly `panicked at /` and rebooted — the message
/// that says WHICH file and WHY was still sitting in the FIFO (observed
/// 2026-08-01, M4 bring-up: a full boot-panic diagnosis was invisible). A
/// panic message you cannot read is worth nothing, and this is the only
/// channel this chip has.
///
/// The spin is deliberately a dumb cycle count rather than a `Delay` or a
/// FIFO-empty poll: the panic path must not touch peripherals or clocks whose
/// state is exactly what may have just gone wrong. 240 MHz × ~40 ms is far
/// more than the ~1.4 ms a full 128-byte FIFO needs at 921600 baud, and the
/// cost is paid only on a boot that is already dead.
#[cfg(not(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness))))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    /// Give the TX FIFO time to clock out before the next print or the reset.
    fn drain() {
        for _ in 0..2_000_000u32 {
            core::hint::spin_loop();
        }
    }
    // FIRST, before printing anything: mask interrupts.
    //
    // The RMT refill ISR runs continuously while strips are transmitting. If
    // the panic came from inside it — or from anything it can re-enter — the
    // next interrupt lands in the middle of this handler and panics again,
    // and a double panic aborts mid-sentence. That is not hypothetical: it
    // is why this board printed `at /U` and rebooted while a driver fault was
    // being diagnosed, hiding the file and line for three flash cycles. A
    // panic report is only trustworthy if nothing else can run during it.
    esp_hal::xtensa_lx::interrupt::disable();

    // ⚠️ This channel is NOT trustworthy for a fault inside the RMT path.
    //
    // Measured 2026-08-01 (M4-P2): a fault raised while WS281x channels are
    // opening resets the chip in well under a millisecond, no matter what
    // this handler does — masking interrupts, draining the FIFO first,
    // printing in 4-byte chunks, and trading 46 KB of heap for stack all
    // yielded the same ~5 characters. That is the signature of a second
    // exception taken inside exception context (window overflow or a
    // flash-mapped read with `PS.EXCM` set), which vectors straight to reset
    // and cannot be out-run from Rust.
    //
    // The app image's answer is the RTC ledger above, which does not depend on
    // this channel at all. These images have no ledger and no RMT driver, so
    // the limitation is recorded rather than fixed: print-and-reset is the
    // whole diagnostic budget of a hello image, and it is adequate for one.
    // `docs/defects/2026-08-01-classic-rmt-open-fault.md`.
    drain();
    esp_println::println!("\n\n====================== PANIC ======================");
    drain();
    if let Some(loc) = info.location() {
        esp_println::println!("at {}:{}", loc.file(), loc.line());
    } else {
        esp_println::println!("at <unknown location>");
    }
    drain();
    esp_println::println!("msg: {}", info.message());
    drain();
    esp_hal::system::software_reset()
}

/// Heap free/used for the heartbeat. A chip fact `fw-esp32-common` must not
/// know, so it is injected.
///
/// It also prints `[MEM]`, which the heartbeat's own `memory` field cannot
/// carry: the wire type is two numbers, and the third one — the largest block
/// the heap could actually hand out — is the one that matters on a 110 KB arena.
/// A board with 12 KB free in 400-byte pieces cannot compile a shader, and from
/// free/used alone it looks healthy. See
/// [`recovery::panic_path::largest_free_block`]; the OOM report prints the same
/// figure at the moment of failure, and this prints it on the way there so the
/// trend is visible before the crash rather than only after it.
///
/// The probe is ~17 first-fit walks, once per heartbeat (5 s) — invisible next
/// to a frame.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
fn esp32_memory_stats() -> Option<(u32, u32)> {
    let free = esp_alloc::HEAP.free();
    let used = esp_alloc::HEAP.used();
    let largest = recovery::panic_path::largest_free_block();
    let retry_saves = OOM_RETRY_SAVES.load(core::sync::atomic::Ordering::Relaxed);
    esp_println::println!(
        "[MEM] free={free} used={used} largest_free={largest} retry_saves={retry_saves}"
    );
    // The JIT code region is NOT part of the heap above — it is a separate
    // fixed SRAM1 reservation, and its residency is the number that decides
    // how much of it can be handed back. `peak` is the high-water mark of
    // concurrent residency since boot; `allocs`/`frees` diverging while the
    // board sits idle is a span leak, which must be read before any figure
    // here is used to justify a smaller region.
    if let Some((jit, jit_largest)) = lpvm_native::codemem_esp32::global::stats() {
        esp_println::println!(
            "[JIT] used={} peak={} cap={} spans={} peak_spans={} allocs={} frees={} fails={} largest_free={jit_largest}",
            jit.used,
            jit.peak_used,
            lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT.len_bytes,
            jit.live_spans,
            jit.peak_spans,
            jit.allocs,
            jit.frees,
            jit.alloc_failures,
        );
    }
    Some((
        free.min(u32::MAX as usize) as u32,
        used.min(u32::MAX as usize) as u32,
    ))
}

/// Everything `main` needs to hand to the server loop.
/// Hand the SRAM1 tail — everything `CodeRegion::ESP32_DEFAULT` does *not*
/// use — to the allocator as a second region, and return its size.
///
/// This is the memory the classic has always owned and never been able to
/// spend. `dram_seg` gives the arena above only 192 KB to share between
/// `.data`, `.bss` and `.stack`; esp-hal also declares `dram2_seg`
/// (`0x3FFE_7E30`, 98,768 B), which no linker section targets — esp-idf uses
/// the same span as heap. It could not join the heap here because the JIT
/// code region sat in the middle of it. Now that the region is measured down
/// to 24 KiB (32 KiB until the 2026-08-04 dome work), 72 KiB of it is free.
///
/// ⚠️ The boundary is **computed from the region**, never restated: the base
/// and length come from [`CodeRegion::reclaimable_heap_span`], whose
/// const-asserts pin it to abut `dbus_end()` exactly. That is deliberate.
/// Before this, the rule lived as prose warnings in two files, and prose does
/// not fail to compile when someone changes the region and forgets — which
/// would hand the allocator and the JIT the same bytes, a corruption whose
/// symptom is a shader overwriting the heap.
///
/// The span carries no ROM hazard. esp-hal's four ROM reservations
/// (`reserved_rom_data_pro/app`, `reserved_rom_stack_pro/app`) all sit
/// *below* `dram2_seg`, the last of them ending exactly at its origin — the
/// ROM's stacks and data are in the middle of SRAM1, and `dram2_seg` is
/// precisely the part above them.
///
/// # Safety
/// The span is `'static` (a fixed hardware address), exclusively the
/// allocator's (the code region is the only other claimant on SRAM1, and the
/// const-asserts prove they abut without overlap), and non-empty.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
fn add_sram1_heap_region() -> usize {
    let (base, len) = lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT.reclaimable_heap_span();
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            base as *mut u8,
            len as usize,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
    len as usize
}

#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
struct FirmwareApp {
    server: LpServer,
    transport: transport::StreamingMessageRouterTransport,
    time_provider: Esp32TimeProvider,
}

#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
#[inline(never)]
fn boot_firmware(spawner: embassy_executor::Spawner) -> FirmwareApp {
    // ⚠️ `init_board` takes the `esp_hal` peripheral singleton, and taking it
    // twice panics. This is the app path's ONLY call to `esp_hal::init`.
    let (sw_int, timg0, uart0, flash, rmt_peripheral, cpu_ctrl) = init_board();
    // The heap is main.rs's, not the board's — mirroring fw-esp32s3.
    esp_alloc::heap_allocator!(size: HEAP_SIZE);
    let sram1_heap = add_sram1_heap_region();
    esp_println::println!("[INIT] fw-esp32v3 boot");
    esp_println::println!(
        "[INIT] chip=esp32 arch=xtensa heap={HEAP_SIZE}+{sram1_heap} (dram_seg arena + SRAM1 tail)"
    );

    // Crash recovery first, before anything crash-prone runs: this both reports
    // the previous run and gives everything after it somewhere to leave a
    // breadcrumb. It cannot run before `heap_allocator!` — `init_and_report`
    // leaks its instance into a `&'static mut`.
    //
    // No RWDT is armed alongside it (contrast fw-esp32s3, which starts a
    // `WatchdogFeeder` on the next line); see `recovery`'s module docs.
    let boot_assessment = recovery::boot_report::init_and_report();
    let boot_guard = lp_recovery::enter(lp_recovery::FrameKind::Boot, "boot").ok();

    start_runtime(timg0, sw_int);
    esp_println::println!("[INIT] runtime started");

    match uart0 {
        Ok(uart) => {
            spawner.spawn(io_task(uart).unwrap());
            esp_println::println!("[INIT] I/O task spawned (uart0 921600 8N1)");
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
    // Extracted before the manifest moves into the registry: the hello's
    // soft-limit fact (see `set_total_led_budget` below).
    let total_led_budget = hardware_manifest
        .soft_limits()
        .and_then(|limits| limits.total_leds.as_ref())
        .map(|limit| limit.value);
    let hardware_registry = Rc::new(HwRegistry::new(hardware_manifest));
    let mut hardware_system = HardwareSystem::new(Rc::clone(&hardware_registry));

    // Start the APP core with the RMT refill ISR on it, BEFORE the RMT driver
    // exists: `install_isr` (inside the driver constructor below) decides its
    // shape from the flag this sets. A dedicated core services refills no
    // matter how long the render core masks interrupts, which is what lets
    // transmission overlap the engine instead of hiding under quiet spins —
    // and a failure here is a downgrade to those single-core (M4) semantics,
    // never a boot failure.
    if output::rmt::shared_driver::start_app_core_isr(cpu_ctrl) {
        esp_println::println!("[INIT] RMT ISR on APP core");
    } else {
        esp_println::println!(
            "[INIT] APP core unavailable; RMT ISR on PRO core (single-core semantics)"
        );
    }

    // The RMT peripheral becomes the WS281x driver's, clock and all. The
    // classic's RMT runs off APB and esp-hal's `validate_clock` for this chip
    // accepts only the source frequency itself, so 80 MHz is not a preference
    // — with the per-channel divider of 1 it gives the 12.5 ns tick
    // `lp_ws281x::PulseCodes` assumes. A failure here is a clock-tree problem,
    // and it costs the board its output rather than its boot, so it is logged
    // and not fatal.
    //
    // How many outputs appear is decided in one place: the board manifest's
    // `/rmt/ws281xK` resources (four on the DOM-Z-102). The RMT block plan
    // follows from that count at driver init — four declared channels get two
    // 64-word blocks each (slots 0/2/4/6, the same split the old compile-time
    // constant produced), fewer get wider windows. See
    // `output::rmt::v3_rmt::plan_for_declared`; absorbed slots are never
    // configured.
    match esp_hal::rmt::Rmt::new(rmt_peripheral, output::rmt::shared_driver::RMT_CLOCK) {
        Ok(rmt) => {
            hardware_system.add_ws281x_driver(Box::new(Esp32V3RmtWs281xDriver::new(
                Rc::clone(&hardware_registry),
                rmt,
            )));
        }
        Err(error) => {
            esp_println::println!("[ERROR] RMT init failed ({error:?}); no LED output this boot");
        }
    }
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

    // `TargetLpvmGraphics` resolves to `lpvm-native`'s `NativeJitEngine` on
    // Xtensa: GLSL pushed here compiles to Xtensa machine code ON THE BOARD.
    // On this chip the engine's link step takes the **placed** path
    // (`lpvm-native` feature `xt-placed-code`, see Cargo.toml): the heap has
    // no I-bus view, so every module is linked at a span of the fixed SRAM1
    // code region and installed through the word-mirrored D-bus walk. The
    // install below hands the engine that region; without it the first
    // compile fails with a clean "arena not installed" error.
    //
    // Region facts (measured; see `lpvm_native::codemem_esp32`):
    //   * `CodeRegion::ESP32_DEFAULT` = D-bus `0x3FFE_8000..0x3FFF_0000`,
    //     I-bus image `0x400B_0000..0x400B_8000` (32 KiB of JIT code), sized
    //     from the measured shader corpus — 2.06× the keep-last-good peak.
    //   * The linker cannot collide with it — esp-hal's `dram_seg` ends at
    //     `0x3FFE_0000`.
    //   * The rest of `dram2_seg` above it (`0x3FFF_0000..0x4000_0000`,
    //     64 KiB) IS the heap's second region, added at boot by
    //     `add_sram1_heap_region`. The two abut exactly and the split is
    //     const-asserted in `codemem_esp32`, so this is a fact the compiler
    //     keeps rather than a rule a reader has to remember.
    //   * The frontend is passed, never defaulted: `LpGraphics::glsl_frontend`
    //     has no default impl so every host states its choice, and the device
    //     ships `LpsGlsl`.
    lpvm_native::codemem_esp32::global::install(
        lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT,
    );
    esp_println::println!(
        "[INIT] JIT code region: ibus {:#010x}..{:#010x} ({} KiB, placed); \
         SRAM1 heap tail dbus {:#010x}+{} B",
        lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT.ibus_base(),
        lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT.ibus_end(),
        lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT.len_bytes / 1024,
        lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT
            .reclaimable_heap_span()
            .0,
        lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT
            .reclaimable_heap_span()
            .1,
    );
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
    // Identity only — capabilities (build.features, hardware facts) are
    // computed inside the constructor from the engine's gates and the
    // services just injected — never restated here.
    server.set_hello_identity(
        lpc_wire::HelloIdentity::new(
            "fw-esp32v3",
            env!("LP_BUILD_COMMIT"),
            env!("LP_BUILD_DIRTY") == "true",
            env!("LP_BUILD_PROFILE"),
        )
        .with_device_uid(device_uid),
    );
    // The chip's own permanent identity (efuse): the factory MAC and the
    // silicon revision. The server cannot derive either.
    server.set_hardware_identity(chip_identity());
    // The board manifest's measured LED envelope (a SOFT limit — evidence,
    // never a refusal), and the per-wire telemetry collector the heartbeat
    // polls. Both are this embedder's to provide: only it holds the manifest
    // and the pusher's mailboxes.
    server.set_total_led_budget(total_led_budget);
    fw_esp32_common::output::wire_stats_source::install(collect_wire_stats);

    // Auto-load a project at boot — unless repeated incomplete boots put us in
    // safe mode, in which case the server comes up reachable but nothing
    // crash-prone is loaded.
    //
    // On this chip that branch is not a formality. `quad-strips-v3` currently
    // reset-loops the board from inside the WS281x open path
    // (`docs/defects/2026-08-01-classic-rmt-open-fault.md`), and before the
    // ledger landed the loop was unbreakable from the host: the board never
    // stayed up long enough to accept a delete, and clearing it meant erasing
    // the lpfs partition with espflash. Safe mode is what turns that into a
    // board you can talk to.
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

    // Boot frame ends here. The boot-complete milestone is NOT marked here: the
    // server loop marks it after the first successfully served frame, which is
    // a far stronger claim than "reached the end of boot".
    drop(boot_guard);

    FirmwareApp {
        server,
        transport,
        time_provider: Esp32TimeProvider::new(),
    }
}

/// Mount the `lpfs` partition, falling back to RAM so an unformattable or
/// mis-flashed board still comes up reachable and can say so over the wire.
#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
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

#[cfg(all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)))]
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

/// Harness entrypoint. Replaces the app entirely — a `test_*` build is a rig,
/// not the firmware plus a rig.
///
/// It takes `esp_hal::init` and the UART0 construction below for one reason
/// each: the clock, and legibility. Everything else the app boots (filesystem,
/// server, RMT, recovery ledger) is dead weight to a harness that executes FP
/// instructions and prints, and is gated off.
#[cfg(fw_harness)]
#[esp_hal::main]
fn main() -> ! {
    let peripherals =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));

    // ⚠️ Load-bearing, and the baudrate is half of what makes it work.
    // `esp-println`'s `uart` feature writes UART0's FIFO but never programs the
    // divisor, and `esp_hal::init` has just moved the clock — so constructing
    // this `Uart` is what makes the `[FPCONF]` lines legible at all, and on this
    // chip the capture IS the result.
    //
    // 921600 to match `board::esp32v3::init::init_board`
    // (`lpc_model::DEFAULT_SERIAL_BAUD_RATE`) and the `--monitor-baud` in
    // `just fwtest-xt-fp-esp32v3`. Measured the hard way: with the 115200
    // default here against a 921600 monitor, the capture is 18 lines of binary
    // noise — which reads exactly like a dead board rather than a wrong divisor.
    // It must stay bound, not dropped.
    let _uart0 = esp_hal::uart::Uart::new(
        peripherals.UART0,
        esp_hal::uart::Config::default().with_baudrate(921_600),
    )
    .expect("uart0 config")
    .with_tx(peripherals.GPIO1)
    .with_rx(peripherals.GPIO3);

    #[cfg(feature = "test_xt_fp_conformance")]
    tests::xt_fp_conformance::run_all()
}

/// Boot-to-hello entrypoint: the M2-P1 skeleton (bare build) and the M2-P3
/// radio RAM probe. Both replace the server app rather than extending it.
#[cfg(all(
    not(fw_harness),
    any(
        feature = "radio_ram_probe",
        all(not(feature = "server"), not(feature = "radio_ram_probe"))
    )
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

/// Collect per-wire output telemetry for the heartbeat, from the pusher's
/// mailboxes. Empty (→ absent heartbeat field) until a wire posts, and in
/// the single-core fallback (whose inline path keeps no per-wire
/// attribution).
///
/// `not(fw_harness)` alongside `server`: this reads `output::rmt`'s mailboxes
/// and returns a `Vec`, and a harness build has neither — the app modules and
/// `extern crate alloc` are both gated out under it.
#[cfg(all(feature = "server", not(fw_harness)))]
fn collect_wire_stats() -> alloc::vec::Vec<lpc_wire::server::OutputWireStatus> {
    let mut out = alloc::vec::Vec::new();
    if !output::rmt::shared_driver::isr_on_app_core() {
        return out;
    }
    for (wire, mailbox) in output::rmt::wire_pusher::MAILBOXES.iter().enumerate() {
        let stats = mailbox.wire_stats();
        if stats.posted == 0 {
            continue;
        }
        out.push(lpc_wire::server::OutputWireStatus {
            wire: wire as u8,
            gpio: stats.gpio,
            posted: stats.posted,
            sent: stats.transmitted,
            torn: stats.torn,
            waved: stats.waved,
            mux: stats.takeovers,
            queue_wait_max_us: stats.queue_wait_max_us,
        });
    }
    out
}

// Now gated like the c6/s3 twins. This used to read `server` alone, with a
// comment explaining that the divergence was because this crate had no harness
// cfg; it has one as of the FP conformance rig, so the exception expired. The
// `server` half stays: the wire deps (`lpc_wire`, the common chip_identity
// module) are optional behind it, and the no-server build must not touch them.
#[cfg(all(feature = "server", not(fw_harness)))]
/// This chip's permanent identity, read from efuse.
///
/// Injected rather than derived: the server cannot read silicon, and the
/// chip-generic firmware layer deliberately has no `esp_hal`
/// (ADR 2026-07-29-per-chip-fw-toolchains). See
/// `fw_esp32_common::chip_identity` for why this reports the BASE MAC
/// rather than a per-interface list.
#[cfg(feature = "server")]
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
