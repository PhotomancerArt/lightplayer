//! Host oracle for the on-device shader render.
//!
//! Renders `examples/shader-oracle` through the host LPVM engine and prints the
//! exact bytes a device's ws281x driver must receive. A `fw-esp32s3` **or**
//! `fw-esp32v3` built with its `frame-dump` feature prints the same bytes off
//! its RMT write path (`output::rmt::frame_dump`, ported between the two
//! firmwares with identical line shapes), so the transcripts can be diffed
//! character for character.
//!
//! Nothing here is chip-specific. The `default_esp32s3_hardware_manifest()`
//! below is not an S3 assumption to be parameterised: it is simply a manifest
//! that offers the `D10` endpoint this project's output node names, and an
//! endpoint chooses a wire, never a colour. `scripts/m4-hardware-walk.sh`
//! retargets that label per board before uploading (the classic ESP32 has no
//! `D10` pad) and the pixels are unaffected.
//!
//! This is only an oracle because the two sides share *nothing* below the
//! project file: the host runs LPIR → WASM → wasmtime, the device runs LPIR →
//! Xtensa machine code through `lpvm-native`'s `isa/xt` JIT. Q32 semantics are
//! specified to be bit-exact across engines — that is the premise the filetest
//! suite rests on — so a byte difference here is a real finding, not tolerance.
//!
//! The project is deliberately clock-free, so every frame is identical and the
//! comparison needs no time synchronisation.
//!
//! ## Why two host engines
//!
//! One host number can only ever say *that* the device disagrees. Rendering the
//! same project through wasmtime **and** through `lpvm-native`'s rv32 emulation
//! engine says *which side* of the codegen split a disagreement falls on: rv32
//! emu is the same native code generator the ESP32-S3 JITs, differing only in
//! ISA, so a device byte that matches rv32-emu and not wasmtime is a
//! native-versus-wasm difference rather than an Xtensa bug. That distinction is
//! the whole triage, and it is cheap enough to bake in.
//!
//! ```bash
//! cargo test -p lpa-server --test shader_oracle_frame -- --nocapture
//! ```

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use lp_gfx_lpvm::{LpvmGraphics, TargetLpvmGraphics};
use lpa_server::{LpGraphics, LpServer};
use lpc_hardware::default_esp32s3_hardware_manifest;
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFs;
use lpfs::lp_path::LpPathBuf;
use lpvm_native::{NativeCompileOptions, NativeEmuEngine};

/// Where the project lands inside the server's filesystem. Mirrors the device,
/// which serves projects out of `projects/`.
const PROJECT_NAME: &str = "shader-oracle";

/// Frames to render before sampling. The device dumps its first frame after
/// the output channel opens; more than one frame here proves the host render
/// is stable rather than a first-frame artifact, and the assertion below is
/// what makes that claim.
const FRAMES: u32 = 4;

/// Matches `output::rmt::frame_dump::MAX_DUMP_LEDS` in **both** `fw-esp32s3`
/// and `fw-esp32v3`, so the printed dump lines up with the device transcript
/// without slicing either by hand. Three crates that never link to each other,
/// so this is a transcribed constant: change one and change all three.
const MAX_DUMP_LEDS: usize = 64;

#[test]
fn shader_oracle_frame_matches_across_frames() {
    let wasmtime = render_project(Arc::new(TargetLpvmGraphics::new(
        lpa_server::DEVICE_SHADER_FRONTEND,
    )));
    print_transcript("ORACLE", &wasmtime);

    let rv32_emu = render_project(Arc::new(LpvmGraphics::from_engine(
        NativeEmuEngine::new(NativeCompileOptions::default()),
        "lpvm-native::rt_emu(rv32)",
        lpa_server::DEVICE_SHADER_FRONTEND,
    )));
    print_transcript("ORACLE-RV32", &rv32_emu);

    // Not an assertion. The two engines agreeing is the expected case and the
    // reason the primary transcript can be quoted as *the* expectation; the two
    // disagreeing is a finding about Q32 portability that this test's job is to
    // surface, not to fail on — the device comparison happens outside this
    // process, and failing here would hide the numbers a human needs to read.
    let differing = wasmtime
        .iter()
        .zip(rv32_emu.iter())
        .filter(|(a, b)| a != b)
        .count();
    println!(
        "[ORACLE-DIFF] wasmtime vs rv32-emu: {differing} differing bytes of {}",
        wasmtime.len()
    );
}

/// Load the project fresh and render [`FRAMES`] frames through `graphics`,
/// returning the ws281x bytes. A fresh server per engine keeps the engines from
/// sharing any cached artifact.
fn render_project(graphics: Arc<dyn LpGraphics>) -> Vec<u8> {
    let project_dir = repo_root().join("examples").join(PROJECT_NAME);
    assert!(
        project_dir.is_dir(),
        "missing project directory {}",
        project_dir.display()
    );

    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::with_hardware_manifest(
        default_esp32s3_hardware_manifest(),
    )));
    let mut server = LpServer::new(
        output_provider.clone(),
        Box::new(lpfs::LpFsMemory::new()),
        "projects/".as_path(),
        None,
        None,
        graphics,
    );

    copy_project_into(server.base_fs_mut(), &project_dir);
    server
        .load_project(LpPathBuf::from("/projects").join(PROJECT_NAME).as_path())
        .expect("load project");

    let mut frames: Vec<Vec<u8>> = Vec::new();
    for _ in 0..FRAMES {
        server.advance_frame(16).expect("advance frame");
        frames.push(rendered_bytes(&output_provider.borrow()));
    }

    let first = frames.first().expect("at least one frame").clone();
    assert!(
        !first.is_empty(),
        "no output channel was written — the project produced no pixels on the host, \
         so there is no oracle to compare the device against"
    );
    for (index, frame) in frames.iter().enumerate().skip(1) {
        assert_eq!(
            frame, &first,
            "frame {index} differs from frame 0; this project must be time-invariant"
        );
    }
    assert!(
        first.iter().any(|byte| *byte != 0),
        "every channel rendered black — a black frame is not evidence of a working render"
    );
    first
}

/// Print the frame in the exact shapes a `frame-dump` device build prints, so
/// the device transcript can be diffed against this one directly.
///
/// Split across two lines on purpose: the `rgb=` token is alone on the second
/// one, which is what lets `scripts/m4-hardware-walk.sh` pull it out of both
/// transcripts with the same `cut -d=` and compare the halves.
fn print_transcript(tag: &str, frame: &[u8]) {
    let leds = frame.len() / 3;
    let shown = leds.min(MAX_DUMP_LEDS);
    let hex: String = frame[..shown * 3]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    println!(
        "[{tag}] leds={leds} shown={shown} crc={:#010x} lit={}",
        fnv1a(frame),
        lit_led_count(frame)
    );
    println!("[{tag}] rgb={hex}");
}

/// The u16 engine samples a device provider would receive, reduced to the 8-bit
/// bytes the ws281x driver sees.
///
/// The reduction is `DisplayPipeline`'s, not a second implementation of it: the
/// oracle project sets `white_point = [1,1,1]`, `brightness = 1`, and LUT,
/// dithering and interpolation all off, which is exactly the configuration
/// under which the pipeline collapses to this rounding step and holds no state
/// between frames. Any of those options turned on would make the pipeline
/// temporal, and this function would silently stop describing the device.
fn rendered_bytes(provider: &MemoryOutputProvider) -> Vec<u8> {
    let mut bytes = Vec::new();
    for handle in provider.get_all_handles() {
        let Some(samples) = provider.get_data(handle) else {
            continue;
        };
        bytes.extend(
            samples
                .iter()
                .map(|sample| ((u32::from(*sample) + 0x80) >> 8).min(255) as u8),
        );
    }
    bytes
}

fn copy_project_into(fs: &mut dyn LpFs, project_dir: &Path) {
    for entry in std::fs::read_dir(project_dir).expect("read project dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().expect("file name").to_string_lossy();
        let target = LpPathBuf::from("/projects")
            .join(PROJECT_NAME)
            .join(name.as_ref());
        let bytes = std::fs::read(&path).expect("read project file");
        fs.write_file(target.as_path(), &bytes).expect("write file");
    }
}

/// `CARGO_MANIFEST_DIR` is `lp-app/lpa-server`; the repo root is two levels up.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

/// FNV-1a, matching `frame_dump::frame_checksum` in both device firmwares.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn lit_led_count(data: &[u8]) -> usize {
    data.chunks_exact(3)
        .filter(|led| led.iter().any(|channel| *channel != 0))
        .count()
}
