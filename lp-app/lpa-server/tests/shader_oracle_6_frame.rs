//! The shader oracle's format-6 twin renders the oracle's bytes.
//!
//! `projects/test/shader-oracle-6` is `projects/test/shader-oracle` with its
//! fixture's map2d written in format 6 (mapping rebuild M2): the same grid as
//! one part, placed by its own start at lamp 0 of port 0 of the named output
//! `main`, through the engine's patcher instead of today's auto-flow. Every
//! lamp lands where the oracle's does, so the bytes a device's ws281x driver
//! receives are the oracle's, byte for byte, through both host engines.
//!
//! That is what lets the emulated C6 walk
//! (`PROJECT=projects/test/shader-oracle-6 just walk-esp32c6-emu`) compare
//! the twin against the one unchanged oracle, `shader_oracle_frame`'s, with
//! no oracle of its own.
//!
//! ```bash
//! cargo test -p lpa-server --test shader_oracle_6_frame
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

/// The oracle and its twin.
const ORACLE: &str = "shader-oracle";
const TWIN: &str = "shader-oracle-6";

/// Frames rendered and compared, after the compile-window frame.
const FRAMES: u32 = 4;

/// The format-6 twin's frame is the oracle's, byte for byte, through
/// wasmtime.
#[test]
fn the_twins_frame_is_the_oracles_through_wasmtime() {
    let graphics = || -> Arc<dyn LpGraphics> {
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
    };
    assert_same_frame(
        render_project(ORACLE, graphics()),
        render_project(TWIN, graphics()),
    );
}

/// The format-6 twin's frame is the oracle's, byte for byte, through the
/// rv32 emulation of the code generator a device JITs.
#[test]
fn the_twins_frame_is_the_oracles_through_rv32_emulation() {
    let graphics = || -> Arc<dyn LpGraphics> {
        Arc::new(LpvmGraphics::from_engine(
            NativeEmuEngine::new(NativeCompileOptions::default()),
            "lpvm-native::rt_emu(rv32)",
            lpa_server::DEVICE_SHADER_FRONTEND,
        ))
    };
    assert_same_frame(
        render_project(ORACLE, graphics()),
        render_project(TWIN, graphics()),
    );
}

fn assert_same_frame(oracle: Vec<Vec<u8>>, twin: Vec<Vec<u8>>) {
    assert_eq!(oracle.len(), FRAMES as usize);
    assert!(
        oracle[0].len() == 64 * 3 && oracle[0].iter().any(|byte| *byte != 0),
        "the oracle renders its 64 lamps, not black"
    );
    for (frame, (oracle, twin)) in oracle.iter().zip(&twin).enumerate() {
        assert_eq!(
            twin, oracle,
            "frame {frame}: {TWIN} renders {ORACLE}'s bytes"
        );
    }
}

/// Load `project` fresh and render [`FRAMES`] frames through `graphics`,
/// returning each frame's ws281x bytes (the reduction
/// `shader_oracle_frame.rs` describes: the neutral pipeline's rounding).
fn render_project(project: &str, graphics: Arc<dyn LpGraphics>) -> Vec<Vec<u8>> {
    let project_dir = repo_root().join("projects/test").join(project);
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
    copy_project_into(server.base_fs_mut(), project, &project_dir);
    server
        .load_project(LpPathBuf::from("/projects").join(project).as_path())
        .expect("load project");
    // The compile-window frame renders the black fallback.
    server
        .advance_frame(16)
        .expect("advance compile-window frame");
    (0..FRAMES)
        .map(|_| {
            server.advance_frame(16).expect("advance frame");
            rendered_bytes(&output_provider.borrow())
        })
        .collect()
}

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

fn copy_project_into(fs: &mut dyn LpFs, project: &str, project_dir: &Path) {
    for entry in std::fs::read_dir(project_dir).expect("read project dir") {
        let path = entry.expect("dir entry").path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().expect("file name").to_string_lossy();
        let target = LpPathBuf::from("/projects")
            .join(project)
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
