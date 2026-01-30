//! Project template creation
//!
//! Provides functions to create default project templates that work with any LpFs implementation.

extern crate alloc;

use crate::error::ServerError;
use alloc::format;
use lp_model::AsLpPath;
use lp_shared::fs::LpFs;

/// Create a default project template
///
/// Creates the default project structure with a rainbow rotating color wheel shader.
/// The filesystem should already be chrooted to the project directory (paths like "/project.json" are relative to project root).
pub fn create_default_project_template(fs: &dyn LpFs) -> Result<(), ServerError> {
    // Create texture node
    fs.write_file(
        "/src/texture.texture/node.json".as_path(),
        br#"{"$type":"Memory","size":[64,64],"format":"RGB8"}"#,
    )
    .map_err(|e| ServerError::Filesystem(format!("Failed to write texture node.json: {e}")))?;

    // Create shader node
    fs.write_file(
        "/src/shader.shader/node.json".as_path(),
        br#"{"$type":"Single","texture_id":"/src/texture.texture"}"#,
    )
    .map_err(|e| ServerError::Filesystem(format!("Failed to write shader node.json: {e}")))?;

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

vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    // Center of texture
    vec2 center = outputSize * 0.5;
    
    // Direction from center to fragment
    vec2 dir = fragCoord - center;
    
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

    // Create output node
    fs.write_file(
        "/src/output.output/node.json".as_path(),
        br#"{"$type":"gpio_strip","chip":"ws2812","gpio_pin":4,"count":128}"#,
    )
    .map_err(|e| ServerError::Filesystem(format!("Failed to write output node.json: {e}")))?;

    // Create fixture node
    fs.write_file(
        "/src/fixture.fixture/node.json".as_path(),
        br#"{"$type":"circle-list","output_id":"/src/output.output","texture_id":"/src/texture.texture","channel_order":"rgb","mapping":[{"channel":0,"center":[0.03125,0.0625],"radius":0.05},{"channel":1,"center":[0.09375,0.0625],"radius":0.05},{"channel":2,"center":[0.15625,0.0625],"radius":0.05},{"channel":3,"center":[0.21875,0.0625],"radius":0.05},{"channel":4,"center":[0.28125,0.0625],"radius":0.05},{"channel":5,"center":[0.34375,0.0625],"radius":0.05},{"channel":6,"center":[0.40625,0.0625],"radius":0.05},{"channel":7,"center":[0.46875,0.0625],"radius":0.05},{"channel":8,"center":[0.53125,0.0625],"radius":0.05},{"channel":9,"center":[0.59375,0.0625],"radius":0.05},{"channel":10,"center":[0.65625,0.0625],"radius":0.05},{"channel":11,"center":[0.71875,0.0625],"radius":0.05}]}"#,
    )
    .map_err(|e| ServerError::Filesystem(format!("Failed to write fixture node.json: {e}")))?;

    Ok(())
}
