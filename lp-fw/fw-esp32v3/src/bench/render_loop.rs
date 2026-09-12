//! The `render-loop` benchmark image's chip half: the project payload, the
//! seed, and the cycle counter.
//!
//! The portable half — the frame accumulator, the record shapes, the done
//! marker — is `fw_checks::checks::render_loop`, the same one the C6's image
//! reports through. What is here is what needs a chip or a filesystem: the
//! `include_bytes!` table, the write into the mounted filesystem, and CCOUNT.
//!
//! # This decorates the app path; it does not replace it
//!
//! Everything downstream of [`seed`] is the shipped firmware. `boot_firmware`
//! builds the same `LpServer`, `boot::auto_load_project` finds the project
//! the same way it finds one on a flashed board, the same RMT driver opens
//! the same endpoint, and `run_server_loop_bounded` is the body
//! `run_server_loop` runs with no end. The image differs from the shipped one
//! in two facts and no third: its filesystem is not empty, and its loop
//! stops.
//!
//! That is why the feature is `bench_render_loop` and **not** `test_*`.
//! `build.rs` turns any `CARGO_FEATURE_TEST_*` into `cfg(fw_harness)`, which
//! `cfg`s out `main`, `boot_firmware` and the whole server stack and expects
//! the harness to rebuild them by hand — which is what
//! `tests/shader_compile_incremental.rs` and `tests/test_rmt.rs` do,
//! correctly, because they are measuring something narrower than the product.
//! A render-loop image built that way would be measuring a re-implementation
//! of the render loop, which is the one thing it must not do. `frame-dump` is
//! this crate's own precedent for a feature that decorates the app path
//! instead: every hunk `cfg`'d out of a default build, so
//! `just build-fw-esp32v3` still measures an unchanged image.
//!
//! # Why the project is baked in rather than uploaded
//!
//! The classic's walk (`scripts/emu/m4-walk-esp32v3.sh`) already puts a
//! project on the machine over UART0, and `tests/shader_oracle_pin.rs`
//! renders one that way. But an upload spends guest time — the committed
//! script paces its chunks over seconds — and every one of those seconds is a
//! second of the measurement that is not rendering. A baked-in project starts
//! the loop as early as the compile allows, which is what makes the render
//! the dominant term.
//!
//! # Why this bakes the C6's project, retargeted
//!
//! `projects/test/basic` is the C6's bench image: 241 lamps, psrdnoise with
//! five palettes and three phasors. Its output addresses `ws281x:local:D10`,
//! which is the XIAO's silkscreen for **the same physical gpio18** the
//! DOM-Z-102 calls `IO18` — and the classic's endpoint table is built from
//! `display_label` alone, so `D10` resolves to nothing here.
//!
//! `build.rs` therefore rewrites the one string at build time, exactly as the
//! two walk scripts' `prepare_project` rewrites it before an upload, and this
//! module includes the staged bytes. Same shader, same fixture, same 241
//! lamps, same pin — so the two chips' real-time ratios are a ratio of two
//! machines and not a comparison of two workloads, which is the whole reason
//! M7's roadmap asks for this image and refuses to let a C6 number stand in.
//!
//! # Why there is no memory-filesystem variant
//!
//! The C6's bench pairs `bench_render_loop` with `memory_fs`, because the C6
//! machine's `boot-idle-memfs` image is the one its transcripts were taken
//! from. The classic has no such feature and should not grow one:
//! `scripts/emu/build-reference-image.sh` says it in as many words — the
//! classic "boots from a modelled flash chip with a real filesystem", and
//! that is what its only reference slug measures. The seed therefore lands in
//! whatever `mount_filesystem` returned, which on the emulator is the
//! modelled flash and on a board is the board's own `lpfs`. It costs the
//! measurement a littlefs write of the eight files at boot; the load and the
//! frames are reported separately, so that cost is visible rather than
//! folded into the per-frame mean.

use alloc::format;
use fw_checks::checks::render_loop;
use lpfs::LpFs;
use lpfs::lp_path::AsLpPath;

/// How many frames the loop runs before printing its summary and stopping.
///
/// Sized so the render dominates: the compile amortises to well under 1 % of
/// the run, which is the only thing this count actually has to buy. The C6
/// runs 256 frames of the same project for ≈4.3 s of emulated time; the
/// classic is a slower part per lamp, so the same count is the same work and
/// more emulated seconds. Kept at 256 rather than retuned, because a frame
/// count that differs between the two chips is one more thing a reader has to
/// hold in their head when comparing the two rows.
pub const FRAMES: u32 = 256;

/// The delta every frame is ticked with, replacing the measured one.
///
/// A fixed delta makes frame *content* a function of the frame index alone.
/// That is what lets the emulator's decoded frames be compared byte for byte
/// between two emulator binaries: with a wall-clock delta, any change to
/// cycle accounting would move the phasors and every frame would differ for a
/// reason that has nothing to do with correctness.
pub const DELTA_MS: u32 = 16;

/// Where the seeded project lands. `LpServer` is constructed with
/// `"projects/"` as its base, and `boot::auto_load_project` prefixes the
/// leading slash.
const PROJECT_DIR: &str = "/projects";

/// The project's on-device name.
pub const PROJECT_NAME: &str = "basic";

/// Where the bytes came from, for the seed line.
const SOURCE: &str = "projects/test/basic (D10 -> IO18 at build time)";

/// The staged project, from `build.rs`'s `OUT_DIR/bench-project/` rather than
/// from the source tree: `output.json` there is the retargeted one, and
/// reading the source tree directly would bake the C6's pad.
macro_rules! staged {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(env!("OUT_DIR"), "/bench-project/", $name)).as_slice(),
        )
    };
}

const FILES: &[(&str, &[u8])] = &[
    staged!("project.json"),
    staged!("module.json"),
    staged!("clock.json"),
    staged!("shader.json"),
    staged!("shader.glsl"),
    staged!("fixture.json"),
    staged!("fixture.map2d.json"),
    staged!("output.json"),
];

/// The hand-authored disc both chips map: one centre lamp and eight rings of
/// 60/48/40/32/24/16/12/8. Reported rather than derived because the count the
/// *engine* resolved is what the emulator's decoded frame proves — if this
/// constant and the pad ever disagree, the pad is right.
pub const LAMPS: u32 = 241;

/// One `Output` node, one port, `ws281x:local:IO18`.
pub const OUTPUTS: u32 = 1;

/// The LX6's own free-running cycle counter.
///
/// The C6 needs a `setup()` to arm the Andes PMU behind CSR 0x7E2; CCOUNT
/// free-runs from reset, so this chip's half of that seam is one read. Kept
/// here rather than in `board::esp32v3` because there is no cycle-counter
/// module there to extend and the bench is the only app-path caller — the
/// three other copies of this line in the crate are all harness or telemetry
/// code (`tests/cycle_probe.rs`, `tests/shader_compile_incremental.rs`,
/// `output/rmt/refill_floor_probe.rs`).
pub fn read_cycles() -> u32 {
    esp_hal::xtensa_lx::timer::get_cycle_count()
}

/// `CpuClock::max()` on this part, set by `board::esp32v3::init`. `u64`
/// because that is what `fw_checks`' conversions take.
pub const CPU_HZ: u64 = 240_000_000;

/// Write the project and the startup-project config into the filesystem
/// `mount_filesystem` returned, before the hardware manifest is loaded or the
/// server is built.
///
/// Takes `&dyn LpFs` rather than a concrete backend: `write_file` is a
/// `&self` method on the trait (the backends carry their own interior
/// mutability), so this works against whatever `main` mounted and does not
/// care which it was.
pub fn seed(fs: &dyn LpFs) {
    let dir = format!("{PROJECT_DIR}/{PROJECT_NAME}");
    for (name, bytes) in FILES {
        let path = format!("{dir}/{name}");
        if let Err(e) = fs.write_file(path.as_str().as_path(), bytes) {
            // Loud, and then keep going: a partial project fails at load with
            // a better message than a half-written one fails here, and the
            // run is about to say so anyway.
            esp_println::println!("[render-loop] seed FAILED for {path}: {e:?}");
        }
    }

    // Name the project explicitly instead of relying on `auto_load_project`'s
    // lexical-first fallback. With one project seeded the two are the same
    // choice, but the config is what a real board carries after a load, and a
    // benchmark that depends on there being exactly one directory is a
    // benchmark with a tripwire in it.
    let config = format!(r#"{{"startup_project":"{PROJECT_NAME}"}}"#);
    if let Err(e) = fs.write_file(
        lpc_model::server::server_config::ServerConfig::PATH.as_path(),
        config.as_bytes(),
    ) {
        esp_println::println!("[render-loop] seed FAILED for lightplayer.json: {e:?}");
    }

    esp_println::println!(
        "[render-loop] seeded {} file(s) from {SOURCE} into {dir}",
        FILES.len(),
    );
}

/// The transcript header, before any record — the same sink-agnostic entry
/// point every other classic payload uses.
///
/// `chip` is `"esp32v3"`, matching `tests::CHIP` and every other payload this
/// crate emits. The C6's render-loop header says `"esp32c6"`; a reader
/// comparing the two rows is comparing two chips and the header is where that
/// is stated.
pub fn write_header() {
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "render-loop",
            chip: "esp32v3",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
}

/// The frame budget the server loop runs under, with this chip's cycle
/// counter injected.
pub fn budget() -> fw_esp32_common::server_loop::FrameBudget {
    fw_esp32_common::server_loop::FrameBudget {
        frames: Some(FRAMES),
        fixed_delta_ms: Some(DELTA_MS),
        cycles: Some(read_cycles),
    }
}

/// Everything the run measured, in two records and one marker.
///
/// Called once, after the last frame. Goes through the logger rather than
/// `esp_println` so that the records and the marker reach the host link in
/// the order they were produced — a marker printed straight to the hardware
/// could overtake records still queued in the I/O task, and `--exit-on` would
/// then cut the capture in half.
pub fn report(
    stats: &render_loop::FrameStats,
    load_us: u64,
    uptime_us: u64,
    heap_after_load: (u32, u32),
) {
    render_loop::emit_load_record(
        PROJECT_NAME,
        LAMPS,
        OUTPUTS,
        load_us,
        heap_after_load.0,
        heap_after_load.1,
    );

    let (free, used) = crate::esp32_memory_stats().unwrap_or((0, 0));
    render_loop::emit_summary_record(
        stats,
        CPU_HZ,
        DELTA_MS,
        uptime_us,
        free,
        used,
        crate::recovery::panic_path::largest_free_block().min(u32::MAX as usize) as u32,
    );

    let mean_us = render_loop::cycles_to_us(stats.mean_cycles(), CPU_HZ);
    let fps = render_loop::fps_centi(stats.mean_cycles(), CPU_HZ);
    log::info!(
        "[render-loop] project={PROJECT_NAME} lamps={LAMPS} frames={} mean={mean_us}us min={}us max={}us first={}us fps={}.{:02}",
        stats.frames(),
        render_loop::cycles_to_us(stats.min_cycles() as u64, CPU_HZ),
        render_loop::cycles_to_us(stats.max_cycles() as u64, CPU_HZ),
        render_loop::cycles_to_us(stats.first_cycles() as u64, CPU_HZ),
        fps / 100,
        fps % 100,
    );
    log::info!("{}", render_loop::DONE_MARKER);
}
