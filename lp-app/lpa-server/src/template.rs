//! Project template creation
//!
//! Provides functions to create default project templates that work with any LpFs implementation.
//!
//! Node definitions are authored as canonical slot-codec JSON (the same form
//! `NodeDef::write_json` emits), so `lpa-server` needs no parser here and
//! stays compatible with `no_std` firmware builds.

extern crate alloc;

use crate::error::ServerError;
use alloc::format;
use lpc_model::AsLpPath;
use lpfs::LpFs;

const PROJECT_JSON: &[u8] = br#"{
  "kind": "Project",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "fixture": {
      "ref": "./fixture.json"
    },
    "output": {
      "ref": "./output.json"
    },
    "shader": {
      "ref": "./shader.json"
    },
    "texture": {
      "ref": "./texture.json"
    }
  }
}
"#;

/// JSON for the default clock node.
const CLOCK_NODE_JSON: &[u8] = br#"{
  "kind": "Clock",
  "controls": {
    "running": true,
    "rate": 1,
    "scrub_offset_seconds": 0
  }
}
"#;

/// JSON for a 64×64 texture node.
const TEXTURE_NODE_JSON: &[u8] = br#"{
  "kind": "Texture",
  "size": {
    "width": 64,
    "height": 64
  }
}
"#;

/// JSON for the default shader node.
const SHADER_NODE_JSON: &[u8] = br#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "render_order": 0,
  "glsl_opts": {
    "add_sub": "saturating",
    "mul": "saturating",
    "div": "saturating"
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0,
      "label": "Time",
      "description": "Project clock time in seconds"
    }
  }
}
"#;

/// JSON for GPIO strip output.
const OUTPUT_NODE_JSON: &[u8] = br#"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10"
}
"#;

/// JSON for the default fixture.
const FIXTURE_NODE_JSON: &[u8] = br#"{
  "kind": "Fixture",
  "render_size": {
    "width": 16,
    "height": 16
  },
  "bindings": {
    "input": {
      "source": "bus#visual.out"
    },
    "output": {
      "target": "bus#control.out"
    }
  },
  "sampling": "direct",
  "diagnostic_mode": "off",
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2
  },
  "color_order": "rgb",
  "transform": [
    [
      1,
      0,
      0
    ],
    [
      0,
      1,
      0
    ],
    [
      0,
      0,
      1
    ]
  ],
  "brightness": 64,
  "gamma_correction": true
}
"#;

/// Create a default project template
///
/// Creates the default project structure with a rainbow rotating color wheel shader.
/// The filesystem should already be chrooted to the project directory.
pub fn create_default_project_template(fs: &dyn LpFs) -> Result<(), ServerError> {
    fs.write_file("/project.json".as_path(), PROJECT_JSON)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write project.json: {e}")))?;

    fs.write_file("/clock.json".as_path(), CLOCK_NODE_JSON)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write clock.json: {e}")))?;

    fs.write_file("/texture.json".as_path(), TEXTURE_NODE_JSON)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write texture.json: {e}")))?;

    fs.write_file("/shader.json".as_path(), SHADER_NODE_JSON)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write shader.json: {e}")))?;

    fs.write_file(
        "/shader.glsl".as_path(),
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
    .map_err(|e| ServerError::Filesystem(format!("Failed to write shader.glsl: {e}")))?;

    fs.write_file("/output.json".as_path(), OUTPUT_NODE_JSON)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write output.json: {e}")))?;

    fs.write_file("/fixture.json".as_path(), FIXTURE_NODE_JSON)
        .map_err(|e| ServerError::Filesystem(format!("Failed to write fixture.json: {e}")))?;

    Ok(())
}
