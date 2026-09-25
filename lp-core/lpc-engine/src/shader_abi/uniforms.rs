//! Uniform structs for visual shader execution.

use alloc::string::String;
use alloc::vec::Vec;
use lps_shared::LpsValueF32;

use crate::products::visual::PatternFrame;

/// One prepared uniform value consumed by visual shader GLSL.
pub(crate) type VisualUniform = (String, LpsValueF32);

/// Uniform name of the pattern-space lamp-box half-size intrinsic.
pub(crate) const PATTERN_EXTENT_UNIFORM: &str = "patternExtent";
/// Uniform name of the pattern-space pitch intrinsic.
pub(crate) const PATTERN_PITCH_UNIFORM: &str = "patternPitch";
/// Uniform name of the pattern-space lamp-count intrinsic.
pub(crate) const LAMP_COUNT_UNIFORM: &str = "lampCount";

/// Build the engine shader uniforms block.
///
/// `outputSize` is a render-request intrinsic. All other fields come from
/// resolved visual shader consumed slots cached on the shader node during
/// tick — plus, for a shader that opted into pattern space, the three scope
/// intrinsics `patternExtent` (vec2), `patternPitch` (float) and
/// `lampCount` (float).
///
/// A pixel-space shader passes `None` and gets exactly the block it always
/// got. A pattern-space shader keeps `outputSize` too: it still means the
/// render request's size.
pub(crate) fn build_uniforms(
    width: u32,
    height: u32,
    pattern: Option<&PatternFrame>,
    consumed: &[VisualUniform],
) -> LpsValueF32 {
    let mut fields = Vec::with_capacity(consumed.len() + 1 + if pattern.is_some() { 3 } else { 0 });
    fields.push((
        String::from("outputSize"),
        LpsValueF32::Vec2([width as f32, height as f32]),
    ));
    if let Some(frame) = pattern {
        fields.push((
            String::from(PATTERN_EXTENT_UNIFORM),
            LpsValueF32::Vec2(frame.extent),
        ));
        fields.push((
            String::from(PATTERN_PITCH_UNIFORM),
            LpsValueF32::F32(frame.pitch),
        ));
        fields.push((
            String::from(LAMP_COUNT_UNIFORM),
            LpsValueF32::F32(frame.lamp_count as f32),
        ));
    }
    fields.extend(consumed.iter().cloned());
    LpsValueF32::Struct { name: None, fields }
}
