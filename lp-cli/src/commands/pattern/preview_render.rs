//! Run one preview project on the host engine and record its frames.
//!
//! The graphics backend is the one every device-emulating host uses:
//! `lp_gfx_lpvm::TargetLpvmGraphics` with the device's GLSL frontend
//! (`lpa_server::DEVICE_SHADER_FRONTEND`). On the host that is `lpvm-wasm`
//! under wasmtime, compiling each shader in its authored float mode — Q32
//! unless the def pins `float_mode` — so the numbers are the device's
//! fixed-point numbers, not a float approximation of them.

use std::sync::Arc;

use anyhow::{Result, bail};
use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsMemory;

/// Ticks run before recording starts, at the recording's own frame step.
/// A first frame is special (extents establish, shaders compile, a compute
/// sim seeds); five clears every catalog pattern, meteor's sim included
/// (`lpc-engine/tests/meteor_compute_animates.rs` warms up the same way).
pub const WARMUP_TICKS: u32 = 5;

/// The recorded animation: `frames × lamps × 3` bytes, 8-bit RGB.
#[derive(Debug, Clone)]
pub struct RecordedFrames {
    pub fps: u32,
    pub frame_count: usize,
    pub lamp_count: usize,
    pub rgb: Vec<u8>,
}

impl RecordedFrames {
    /// Largest channel value over the whole recording.
    pub fn peak(&self) -> u8 {
        self.rgb.iter().copied().max().unwrap_or(0)
    }

    /// How many frames differ from the frame before them.
    pub fn changing_frames(&self) -> usize {
        let stride = self.lamp_count * 3;
        if stride == 0 {
            return 0;
        }
        self.rgb
            .chunks(stride)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count()
    }
}

/// Load `fs` as a project and record `seconds` at `fps` from its output.
pub fn record(
    fs: &LpFsMemory,
    root_name: &str,
    lamp_count: usize,
    seconds: f32,
    fps: u32,
) -> Result<RecordedFrames> {
    if fps == 0 {
        bail!("--fps must be at least 1");
    }
    let root = format!("/{}.show", root_name.replace(['-', '/', '.'], "_"));
    let services = EngineServices::new(
        TreePath::parse(&root).map_err(|e| anyhow::anyhow!("root path {root}: {e:?}"))?,
    );
    let mut rt = ProjectLoader::load_from_root(fs, services)
        .map_err(|e| anyhow::anyhow!("load the preview project: {e:?}"))?;
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lpa_server::DEVICE_SHADER_FRONTEND,
        ))));

    let frame_count = ((seconds * fps as f32).round() as usize).max(1);
    let step_ms = |frame: u64| -> u32 {
        let at = |k: u64| (k * 1000 + fps as u64 / 2) / fps as u64;
        (at(frame + 1) - at(frame)) as u32
    };

    for tick in 0..WARMUP_TICKS {
        tick_checked(&mut rt, step_ms(tick as u64))?;
    }

    let mut rgb = Vec::with_capacity(frame_count * lamp_count * 3);
    for frame in 0..frame_count {
        tick_checked(&mut rt, step_ms(frame as u64))?;
        let samples = output_samples(&rt)?;
        if samples.len() != lamp_count * 3 {
            bail!(
                "the output published {} channels, expected {} ({} lamps × RGB)",
                samples.len(),
                lamp_count * 3,
                lamp_count
            );
        }
        rgb.extend(samples.iter().map(|v| unorm16_to_u8(*v)));
    }

    Ok(RecordedFrames {
        fps,
        frame_count,
        lamp_count,
        rgb,
    })
}

/// Tick, then refuse a faulted project: a faulted output paints the red
/// fault breathe, and recording that as the pattern would be a lie.
fn tick_checked(rt: &mut LoadedProjectRuntime, delta_ms: u32) -> Result<()> {
    rt.tick(delta_ms)
        .map_err(|e| anyhow::anyhow!("engine tick: {e:?}"))?;
    if let Some(fault) = rt.engine().project_fault() {
        let nodes: Vec<String> = fault
            .nodes
            .iter()
            .map(|(path, message)| format!("{path}: {message}"))
            .collect();
        bail!("project faulted: {}", nodes.join("; "));
    }
    Ok(())
}

/// The single output's published control samples (unorm16, one per
/// channel, lamp-major in wiring order).
fn output_samples(rt: &LoadedProjectRuntime) -> Result<Vec<u16>> {
    let engine = rt.engine();
    let mut found = Vec::new();
    for entry in engine.tree().entries() {
        let Some(buffer_id) = engine.runtime_output_sink_buffer_id(entry.id) else {
            continue;
        };
        let Some(buffer) = engine.runtime_buffers().get(buffer_id) else {
            continue;
        };
        found.push(buffer.value().bytes());
    }
    match found.as_slice() {
        [bytes] => Ok(bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect()),
        [] => {
            let nodes: Vec<String> = engine
                .tree()
                .entries()
                .map(|entry| format!("{} ({:?})", entry.path, entry.status.value()))
                .collect();
            bail!(
                "the preview project has no output buffer; its nodes: {}",
                nodes.join(", ")
            )
        }
        more => bail!(
            "the preview project has {} output buffers, expected one",
            more.len()
        ),
    }
}

/// unorm16 → unorm8, rounded (65535 → 255, 0 → 0).
fn unorm16_to_u8(value: u16) -> u8 {
    ((value as u32 * 255 + 32767) / 65535) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unorm16_rounds_to_the_nearest_byte() {
        assert_eq!(unorm16_to_u8(0), 0);
        assert_eq!(unorm16_to_u8(65535), 255);
        assert_eq!(unorm16_to_u8(257), 1);
        assert_eq!(unorm16_to_u8(128), 0);
        assert_eq!(unorm16_to_u8(129), 1);
    }
}
