//! Per-render uniform inputs for [`crate::engine::FxInstance::render`].

use crate::input::FxValue;

/// All inputs needed to render one frame.
///
/// Built per-call by the caller; not stored on the instance. Mirrors
/// `LpsPxShader::render_frame`'s shape (uniforms-per-call, no
/// per-instance uniform cache).
///
/// `time` is a typed field — the frame clock is mandatory. User-defined
/// manifest inputs go in `inputs` as `(&str, FxValue)` pairs; the
/// implementation looks each up by name and applies it to the
/// shader's `input_<name>` uniform.
pub struct FxRenderInputs<'a> {
    pub time: f32,
    pub inputs: &'a [(&'a str, FxValue)],
}
