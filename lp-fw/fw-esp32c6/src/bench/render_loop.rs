//! The `render-loop` benchmark image's chip half: the project payload, the
//! seed, and the counters.
//!
//! The portable half — the frame accumulator, the record shapes, the done
//! marker — is `fw_checks::checks::render_loop`. What is here is what needs a
//! chip or a filesystem: the `include_bytes!` table, the write into the
//! in-memory filesystem, and the PMU cycle counter behind CSR 0x7E2.
//!
//! # This decorates the app path; it does not replace it
//!
//! Everything downstream of [`seed`] is the shipped firmware. `boot_firmware`
//! builds the same `LpServer`, `boot::auto_load_project` finds the project the
//! same way it finds one on a flashed board, the same RMT driver opens the
//! same endpoint, and `run_server_loop` runs the same loop. The image differs
//! from `boot-idle-memfs` in two facts and no third: its filesystem is not
//! empty, and its loop stops.
//!
//! That is why the feature is `bench_render_loop` and **not** `test_*`.
//! `build.rs` turns any `CARGO_FEATURE_TEST_*` into `cfg(fw_harness)`, which
//! `cfg`s out `main`, `boot_firmware` and the whole server stack and expects
//! the harness to rebuild them by hand — which is what
//! `tests/incremental_shader_compile/` and `tests/fluid_demo/` do, correctly,
//! because they are measuring something narrower than the product. A
//! render-loop image built that way would be measuring a re-implementation of
//! the render loop, which is the one thing it must not do. `frame-dump` and
//! `spike_uart0_link` are the precedents for a feature that decorates the app
//! path instead: every hunk `cfg`'d out of a default build, so
//! `just fw-esp32c6-size-check` still measures an unchanged image.
//!
//! # Why the project is baked in rather than uploaded
//!
//! `lp-emu-esp32c6`'s `upload-walk` payloads already put a project on the
//! device over the wire, and `tests/basic_pin.rs` renders `examples/basic`
//! that way. But an upload spends guest time — the committed walk paces its
//! chunks and lands the load at about 2.9 s — and every one of those seconds
//! is a second of the measurement that is not rendering. A baked-in project
//! starts the loop as early as the compile allows, which is what makes the
//! render the dominant term.
//!
//! # Two projects, and what each is for
//!
//! - **`basic`** (default): `projects/test/basic`, 241 lamps on `D10` =
//!   GPIO18, psrdnoise with five palettes and three phasors. The idiomatic
//!   image, and the one already proven end to end on this emulator by
//!   `lp-emu/esp/lp-emu-esp32c6/tests/basic_pin.rs` at 61 fps with
//!   byte-identical frame dumps.
//! - **`rocaille`** (`bench_project_rocaille`): `catalog/projects/rocaille`,
//!   the same 241 lamps on the same pad and the same hand-authored disc, with
//!   the heaviest per-lamp arithmetic in the catalogue — an iterated
//!   sine-fold with a `tanh` — plus a `Texture` node. The pressure test: same
//!   output path, more shader.
//!
//! Both resolve on this board because both address `D10`. That is not a
//! coincidence to rely on: the emulated C6 loads the real
//! `seeed/xiao-esp32-c6` manifest, whose endpoint table is built from
//! `display_label` alone, so only `D0`–`D3` and `D6`–`D10` resolve and the
//! `IO*` spellings the classic-board projects use do not resolve at all.

use alloc::format;
use fw_checks::checks::render_loop;
use lpfs::LpFs;
use lpfs::lp_path::AsLpPath;

use crate::board::esp32c6::constants::CPU_HZ;
use crate::board::esp32c6::cycle_counter;

/// How many frames the loop runs before printing its summary and stopping.
///
/// Sized from measurements rather than taste. Boot to the first served frame
/// is under half a second; `projects/test/basic`'s shader compile is 551 ms
/// (the `shader-compile-stress` transcript, which compiles this exact file);
/// and the project renders at about 61 fps on this emulator. 256 frames is
/// therefore ≈4.2 s of rendering on top of ≈1 s of boot and compile — inside
/// the ladder's existing 5.5 s identity window, with the compile at about a
/// tenth of the run rather than dominating it.
pub const FRAMES: u32 = 256;

/// The delta every frame is ticked with, replacing the measured one.
///
/// A fixed delta makes frame *content* a function of the frame index alone.
/// That is what lets the emulator's `--dump-frames` output be compared byte
/// for byte between two emulator binaries: with a wall-clock delta, any change
/// to cycle accounting would move the phasors and every frame would differ for
/// a reason that has nothing to do with correctness.
pub const DELTA_MS: u32 = 16;

/// Where the seeded project lands. `LpServer` is constructed with
/// `"projects/"` as its base, and `boot::auto_load_project` prefixes the
/// leading slash.
const PROJECT_DIR: &str = "/projects";

#[cfg(not(feature = "bench_project_rocaille"))]
mod payload {
    pub const NAME: &str = "basic";
    pub const SOURCE: &str = "projects/test/basic";
    macro_rules! file {
        ($name:literal) => {
            (
                $name,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../projects/test/basic/",
                    $name
                ))
                .as_slice(),
            )
        };
    }
    pub const FILES: &[(&str, &[u8])] = &[
        file!("project.json"),
        file!("module.json"),
        file!("clock.json"),
        file!("shader.json"),
        file!("shader.glsl"),
        file!("fixture.json"),
        file!("fixture.map2d.json"),
        file!("output.json"),
    ];
}

#[cfg(feature = "bench_project_rocaille")]
mod payload {
    pub const NAME: &str = "rocaille";
    pub const SOURCE: &str = "catalog/projects/rocaille";
    macro_rules! file {
        ($name:literal) => {
            (
                $name,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../catalog/projects/rocaille/",
                    $name
                ))
                .as_slice(),
            )
        };
    }
    pub const FILES: &[(&str, &[u8])] = &[
        file!("project.json"),
        file!("module.json"),
        file!("clock.json"),
        file!("shader.json"),
        file!("shader.glsl"),
        file!("texture.json"),
        file!("fixture.json"),
        file!("fixture.map2d.json"),
        file!("output.json"),
    ];
}

pub use payload::NAME as PROJECT_NAME;

/// Both projects map the same hand-authored disc: one centre lamp and eight
/// rings of 60/48/40/32/24/16/12/8. Reported rather than derived because the
/// count the *engine* resolved is what the emulator's decoded frame proves —
/// if this constant and the pad ever disagree, the pad is right.
pub const LAMPS: u32 = 241;

/// One `Output` node, one port, `ws281x:local:D10`.
pub const OUTPUTS: u32 = 1;

/// Write the project and the startup-project config into the freshly created
/// filesystem, before the hardware manifest is loaded or the server is built.
///
/// Takes `&dyn LpFs` rather than the concrete memory filesystem: `write_file`
/// is a `&self` method on the trait (the backends carry their own interior
/// mutability), so this works against whatever `main` built and does not care
/// which it was.
pub fn seed(fs: &dyn LpFs) {
    // Armed here, at the earliest point the bench owns, so that the project
    // load — the compile, which is the whole of it — can be timed on the same
    // counter the frames are.
    cycle_counter::setup();

    let dir = format!("{PROJECT_DIR}/{}", payload::NAME);
    for (name, bytes) in payload::FILES {
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
    let config = format!(r#"{{"startup_project":"{}"}}"#, payload::NAME);
    if let Err(e) = fs.write_file(
        lpc_model::server::server_config::ServerConfig::PATH.as_path(),
        config.as_bytes(),
    ) {
        esp_println::println!("[render-loop] seed FAILED for lightplayer.json: {e:?}");
    }

    esp_println::println!(
        "[render-loop] seeded {} file(s) from {} into {dir}",
        payload::FILES.len(),
        payload::SOURCE,
    );
}

/// The transcript header, before any record — the same sink-agnostic entry
/// point every other C6 payload uses.
pub fn write_header() {
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "render-loop",
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
}

/// The frame budget the server loop runs under, with the chip's cycle counter
/// injected.
pub fn budget() -> fw_esp32_common::server_loop::FrameBudget {
    fw_esp32_common::server_loop::FrameBudget {
        frames: Some(FRAMES),
        fixed_delta_ms: Some(DELTA_MS),
        cycles: Some(cycle_counter::read),
    }
}

/// Everything the run measured, in two records and one marker.
///
/// Called once, after the last frame. Goes through the logger rather than
/// `esp_println` so that the records and the marker reach the host link in the
/// order they were produced — a marker printed straight to the hardware could
/// overtake records still queued in the I/O task, and `--exit-on` would then
/// cut the capture in half.
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
