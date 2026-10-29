//! A render source the lab made (the desk's shaders), written as a
//! product shader node: its node file, and a small GLSL file standing in
//! for its pattern, so the saved project has a shader the product can
//! run. A shader the lab opened is never written: its own files stay as
//! they were.

use lab_mapping_model::{Pattern, RenderSource};
use serde_json::{Value, json};

/// The node file: a `Shader` reading `source` and writing its channel.
pub fn node_value(src: &RenderSource, glsl_file: &str) -> Value {
    json!({
        "kind": "Shader",
        "source": glsl_file,
        "render_order": 0,
        "bindings": {
            "output": {"target": src.writes.binding()}
        }
    })
}

/// The GLSL: the source's own text if it has some, else the pattern's
/// stand-in, written in the product's shader shape (`render_2d`).
pub fn glsl(src: &RenderSource) -> String {
    if let Some(text) = &src.glsl {
        return text.clone();
    }
    let colour = match src.pattern {
        Pattern::Rainbow => "hue(0.92 * (0.8 * uv.x + 0.2 * uv.y)) * 0.6",
        Pattern::Gradient => "hue(0.49 + 0.375 * (uv.x + uv.y) / 2.0) * 0.6",
        Pattern::Clouds => "hue(0.57 + 0.12 * sin(uv.x * 7.0) * cos(uv.y * 5.0)) * 0.55",
        Pattern::Rings => "hue(0.05 + 0.05 * cos(length(uv - 0.5) * 18.0)) * 0.5",
    };
    format!(
        "// Written by the mapping lab: a stand-in for its \"{words}\" pattern.\n\
         \n\
         layout(binding = 0) uniform vec2 outputSize;\n\
         \n\
         vec3 hue(float h) {{\n\
         \x20   vec3 k = abs(fract(vec3(h) + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);\n\
         \x20   return clamp(k - 1.0, 0.0, 1.0);\n\
         }}\n\
         \n\
         vec4 render_2d(vec2 pos) {{\n\
         \x20   vec2 uv = pos / outputSize;\n\
         \x20   return vec4({colour}, 1.0);\n\
         }}\n",
        words = src.pattern.words(),
    )
}
