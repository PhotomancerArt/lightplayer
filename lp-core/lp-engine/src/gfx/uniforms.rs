//! Uniform structs for [`lp_shader::LpsPxShader::render_frame`].

use alloc::string::String;
use alloc::vec;

/// Build the engine shader uniforms block (`outputSize`, `time`) as F32 values.
pub(crate) fn build_uniforms(width: u32, height: u32, time: f32) -> lps_shared::LpsValueF32 {
    lps_shared::LpsValueF32::Struct {
        name: None,
        fields: vec![
            (
                String::from("outputSize"),
                lps_shared::LpsValueF32::Vec2([width as f32, height as f32]),
            ),
            (String::from("time"), lps_shared::LpsValueF32::F32(time)),
        ],
    }
}
