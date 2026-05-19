//! Uniform structs for visual shader execution.

use alloc::string::String;
use alloc::vec::Vec;
use lps_shared::LpsValueF32;

/// One prepared uniform value consumed by visual shader GLSL.
pub(crate) type VisualUniform = (String, LpsValueF32);

/// Build the engine shader uniforms block.
///
/// `outputSize` is a render-request intrinsic. All other fields come from
/// resolved visual shader consumed slots cached on the shader node during tick.
pub(crate) fn build_uniforms(width: u32, height: u32, consumed: &[VisualUniform]) -> LpsValueF32 {
    let mut fields = Vec::with_capacity(consumed.len() + 1);
    fields.push((
        String::from("outputSize"),
        LpsValueF32::Vec2([width as f32, height as f32]),
    ));
    fields.extend(consumed.iter().cloned());
    LpsValueF32::Struct { name: None, fields }
}
