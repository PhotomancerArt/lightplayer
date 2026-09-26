//! The JSON one preview writes: `<out>/<pattern>__<swatch>.json`.
//!
//! ```text
//! {
//!   "schema": "lp-pattern-preview/1",
//!   "pattern": { slug, name, description, family, idea, provenance, knobs[] },
//!   "swatch":  { name, lamp_count, size: [w, h], pitch, positions: [[x, y]…] },
//!   "render":  { backend, float_mode, render_size, sampling, fps, seconds,
//!                frame_count, warmup_ticks, set: {knob: value} },
//!   "frames":  base64 of frame_count × lamp_count × 3 bytes (8-bit RGB),
//!   "stats":   { peak, changing_frames },
//!   "error":   null | "why this cell did not render"
//! }
//! ```
//!
//! Swatch positions are in drawing units: the lamp box's long side runs
//! 0 → 1, y down (canvas convention). `pitch` is the median
//! nearest-neighbour spacing in the same units.

use serde_json::{Value, json};

use super::base64;
use super::knob_override::KnobOverride;
use super::preview_render::{RecordedFrames, WARMUP_TICKS};
use super::swatch::Swatch;
use super::swatch_rig::RigFixture;

/// Identifies the format for the page generator.
pub const SCHEMA: &str = "lp-pattern-preview/1";

/// The backend `preview_render` runs on this host.
pub const BACKEND: &str = "lpvm-wasm (wasmtime), LpsGlsl frontend";

/// File name for one (pattern, swatch).
pub fn file_name(pattern_slug: &str, swatch_name: &str) -> String {
    format!("{pattern_slug}__{swatch_name}.json")
}

/// The swatch half, shared by success and failure records.
pub fn swatch_block(swatch: &Swatch) -> Value {
    let (positions, size) = swatch.drawing_positions();
    let pitch = Swatch::drawing_pitch(&positions);
    let rounded: Vec<[f32; 2]> = positions
        .iter()
        .map(|[x, y]| [round4(*x), round4(*y)])
        .collect();
    json!({
        "name": swatch.name,
        "lamp_count": swatch.lamp_count(),
        "size": [round4(size[0]), round4(size[1])],
        "pitch": round4(pitch),
        "positions": rounded,
    })
}

/// A rendered preview.
pub fn success(
    pattern: Value,
    float_mode: &str,
    swatch: &Swatch,
    rig: &RigFixture,
    overrides: &[KnobOverride],
    seconds: f32,
    frames: &RecordedFrames,
) -> Value {
    json!({
        "schema": SCHEMA,
        "pattern": pattern,
        "swatch": swatch_block(swatch),
        "render": render_block(
            float_mode,
            Some(rig),
            overrides,
            seconds,
            frames.fps,
            Some(frames),
        ),
        "frames": base64::encode(&frames.rgb),
        "stats": {
            "peak": frames.peak(),
            "changing_frames": frames.changing_frames(),
        },
        "error": Value::Null,
    })
}

/// A preview that could not render; the page shows `error` in its cell.
pub fn failure(
    pattern: Value,
    float_mode: &str,
    swatch: &Swatch,
    overrides: &[KnobOverride],
    seconds: f32,
    fps: u32,
    error: &str,
) -> Value {
    json!({
        "schema": SCHEMA,
        "pattern": pattern,
        "swatch": swatch_block(swatch),
        "render": render_block(float_mode, None, overrides, seconds, fps, None),
        "frames": "",
        "stats": Value::Null,
        "error": error,
    })
}

fn render_block(
    float_mode: &str,
    rig: Option<&RigFixture>,
    overrides: &[KnobOverride],
    seconds: f32,
    fps: u32,
    frames: Option<&RecordedFrames>,
) -> Value {
    let set: serde_json::Map<String, Value> = overrides
        .iter()
        .map(|knob| (knob.name.clone(), json!(knob.value)))
        .collect();
    json!({
        "backend": BACKEND,
        "float_mode": float_mode,
        "render_size": rig.map(|rig| rig.render_size),
        "sampling": rig.map(|rig| rig.sampling.clone()),
        "fps": fps,
        "seconds": seconds,
        "frame_count": frames.map(|frames| frames.frame_count),
        "warmup_ticks": WARMUP_TICKS,
        "set": Value::Object(set),
    })
}

fn round4(value: f32) -> f32 {
    (value * 10_000.0).round() / 10_000.0
}
