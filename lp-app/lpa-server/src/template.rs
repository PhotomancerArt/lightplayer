//! Project template creation
//!
//! Provides functions to create default project templates that work with any LpFs implementation.
//!
//! Node configs are authored as static TOML matching legacy [`lpc_source::legacy::nodes`] serde
//! shape (same bytes as `toml::to_string` on the host). `toml` is not used here so `lpa-server`
//! stays compatible with `no_std` firmware builds where unified `toml` features can pull `std`.

extern crate alloc;

use crate::error::ServerError;
use alloc::format;
use lpc_model::AsLpPath;
use lpfs::LpFs;

/// TOML for a 64×64 texture node (see `TextureConfig`).
const TEXTURE_NODE_TOML: &[u8] = br#"width = 64
height = 64
"#;

/// TOML for the default shader node (see `ShaderConfig`).
const SHADER_NODE_TOML: &[u8] = br#"glsl_path = "main.glsl"
texture_spec = "/src/texture.texture"
render_order = 0

[glsl_opts]
add_sub = "saturating"
mul = "saturating"
div = "saturating"
"#;

/// TOML for GPIO strip output (see `OutputConfig::GpioStrip`).
const OUTPUT_NODE_TOML: &[u8] = br#"[GpioStrip]
pin = 4
"#;

/// TOML for the default fixture (see `FixtureConfig`).
const FIXTURE_NODE_TOML: &[u8] = br#"output_spec = "/src/output.output"
texture_spec = "/src/texture.texture"
color_order = "Rgb"
transform = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]

[mapping.PathPoints]
paths = []
sample_diameter = 2.0
"#;

/// Create a default project template
///
/// Creates the default project structure with a rainbow rotating color wheel shader.
/// The filesystem should already be chrooted to the project directory (paths like "/project.json" are relative to project root).
pub fn create_default_project_template(fs: &dyn LpFs) -> Result<(), ServerError> {
    fs.write_file(
        "/src/texture.texture/node.toml".as_path(),
        TEXTURE_NODE_TOML,
    )
    .map_err(|e| ServerError::Filesystem(format!("Failed to write texture node.toml: {e}")))?;

    fs.write_file("/src/shader.shader/node.toml".as_path(), SHADER_NODE_TOML)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write shader node.toml: {e}")))?;

    fs.write_file(
        "/src/shader.shader/main.glsl".as_path(),
        br#"// HSV to RGB conversion function
vec3 hsv_to_rgb(float h, float s, float v) {
    // h in [0, 1], s in [0, 1], v in [0, 1]
    float c = v * s;
    float x = c * (1.0 - abs(mod(h * 6.0, 2.0) - 1.0));
    float m = v - c;
    
    vec3 rgb;
    if (h < 1.0/6.0) {
        rgb = vec3(v, m + x, m);
    } else if (h < 2.0/6.0) {
        rgb = vec3(m + x, v, m);
    } else if (h < 3.0/6.0) {
        rgb = vec3(m, v, m + x);
    } else if (h < 4.0/6.0) {
        rgb = vec3(m, m + x, v);
    } else if (h < 5.0/6.0) {
        rgb = vec3(m + x, m, v);
    } else {
        rgb = vec3(v, m, m + x);
    }
    
    return rgb;
}

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 render(vec2 pos) {
    // Center of texture
    vec2 center = outputSize * 0.5;
    
    // Direction from center to fragment
    vec2 dir = pos - center;
    
    // Calculate angle (atan2 gives angle in [-PI, PI])
    float angle = atan(dir.y, dir.x);
    
    // Rotate angle with time (full rotation every 2 seconds)
    angle = (angle + time * 3.14159);
    
    // Normalize angle to [0, 1] for hue
    // atan returns [-PI, PI], map to [0, 1] by: (angle + PI) / (2 * PI)
    // Wrap hue to [0, 1] using mod to handle large time values
    float hue = mod((angle + 3.14159) / (2.0 * 3.14159), 1.0);
    
    // Distance from center (normalized to [0, 1])
    float maxDist = length(outputSize * 0.5);
    float dist = length(dir) / maxDist;
    
    // Clamp distance to prevent issues
    dist = min(dist, 1.0);
    
    // Value (brightness): highest at center, darker at edges
    float value = 1.0 - dist * 0.5;
    
    // Convert HSV to RGB
    vec3 rgb = hsv_to_rgb(hue, 1.0, value);
    
    // Clamp to [0, 1] and return
    return vec4(max(vec3(0.0), min(vec3(1.0), rgb)), 1.0);
}"#,
    )
    .map_err(|e| ServerError::Filesystem(format!("Failed to write shader main.glsl: {e}")))?;

    fs.write_file("/src/output.output/node.toml".as_path(), OUTPUT_NODE_TOML)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write output node.toml: {e}")))?;

    fs.write_file(
        "/src/fixture.fixture/node.toml".as_path(),
        FIXTURE_NODE_TOML,
    )
    .map_err(|e| ServerError::Filesystem(format!("Failed to write fixture node.toml: {e}")))?;

    Ok(())
}
