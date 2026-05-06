//! Project creation logic
//!
//! Functions for creating new projects with sensible defaults.

use anyhow::{Context, Result};
use std::path::Path;

use lpc_model::{AsLpPath, AsLpPathBuf, RelativeNodeRef};
use lpc_source::node::{
    fixture::{ColorOrder, FixtureDef, MappingConfig},
    output::OutputDef,
    shader::ShaderDef,
    texture::TextureDef,
};
use lpfs::LpFs;

use crate::messages;

/// Derive project name from directory path
///
/// Extracts the directory name and sanitizes it if needed.
pub fn derive_project_name(dir: &Path) -> String {
    dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string()
}

/// Generate a UID from project name
///
/// Format: `YYYY.MM.DD-HH.MM.SS-<name>`
/// Example: `2025.01.15-12.15.02-my-project`
pub fn generate_uid(name: &str) -> String {
    // Get current time
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Convert to UTC time components
    // Using a simple approach - calculate from seconds since epoch
    let days_since_epoch = now / 86400;
    let seconds_today = now % 86400;

    // Calculate date (approximate, doesn't account for leap years perfectly)
    // Epoch: 1970-01-01
    let mut year = 1970;
    let mut days_remaining = days_since_epoch;

    // Account for leap years
    while days_remaining >= 365 {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if days_remaining >= days_in_year {
            days_remaining -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    // Calculate month and day (simplified)
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mut month = 1;
    let mut day = days_remaining as u32 + 1;

    for &days_in_month in &month_days {
        let days = if month == 2 && is_leap {
            29
        } else {
            days_in_month
        };
        if day > days {
            day -= days;
            month += 1;
        } else {
            break;
        }
    }

    // Calculate time components
    let hour = (seconds_today / 3600) as u32;
    let minute = ((seconds_today % 3600) / 60) as u32;
    let second = (seconds_today % 60) as u32;

    // Format: YYYY.MM.DD-HH.MM.SS-<name>
    format!("{year:04}.{month:02}.{day:02}-{hour:02}.{minute:02}.{second:02}-{name}")
}

/// Create project directory structure
///
/// Creates the project directory, project.toml, and default node artifacts.
pub fn create_project_structure(dir: &Path, name: Option<&str>, uid: Option<&str>) -> Result<()> {
    // Create directory if doesn't exist
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;

    // Derive name from directory if not provided
    let project_name = if let Some(name) = name {
        name.to_string()
    } else {
        derive_project_name(dir)
    };

    // Generate uid if not provided
    let project_uid = if let Some(uid) = uid {
        uid.to_string()
    } else {
        generate_uid(&project_name)
    };

    // Create filesystem view for project directory
    let fs = lpfs::LpFsStd::new(dir.to_path_buf());

    // Create default template
    create_default_template(&fs)?;
    write_project_toml(&fs, &project_uid, &project_name)?;

    Ok(())
}

/// Create default project template
///
/// Creates the default project structure with a rainbow rotating color wheel shader.
/// The filesystem should already be chrooted to the project directory.
pub fn create_default_template(fs: &dyn LpFs) -> Result<()> {
    // Create texture node
    let texture_config = TextureDef {
        width: 64,
        height: 64,
    };
    let texture_toml = with_kind(
        "texture",
        toml::to_string(&texture_config).context("Failed to serialize texture def to TOML")?,
    );
    fs.write_file("/texture.toml".as_path(), texture_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write texture.toml: {e}"))?;

    // Create shader node
    let shader_config = ShaderDef {
        glsl_path: "shader.glsl".as_path_buf(),
        texture_loc: RelativeNodeRef::parse("..texture").expect("valid node ref"),
        render_order: 0,
        glsl_opts: lpc_source::legacy::glsl_opts::GlslOpts::default(),
    };
    let shader_toml = with_kind(
        "shader",
        toml::to_string(&shader_config).context("Failed to serialize shader def to TOML")?,
    );
    fs.write_file("/shader.toml".as_path(), shader_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write shader.toml: {e}"))?;

    // Create shader GLSL
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
    .map_err(|e| anyhow::anyhow!("Failed to write shader.glsl: {e}"))?;

    // Create output node
    let output_config = OutputDef::GpioStrip {
        pin: 4,
        options: None,
    };
    let output_toml = with_kind(
        "output",
        toml::to_string(&output_config).context("Failed to serialize output def to TOML")?,
    );
    fs.write_file("/output.toml".as_path(), output_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write output.toml: {e}"))?;

    // Create fixture node
    let fixture_config = FixtureDef {
        output_loc: RelativeNodeRef::parse("..output").expect("valid node ref"),
        texture_loc: RelativeNodeRef::parse("..texture").expect("valid node ref"),
        mapping: MappingConfig::PathPoints {
            paths: vec![],
            sample_diameter: 2.0,
        },
        color_order: ColorOrder::Rgb,
        transform: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        brightness: None,
        gamma_correction: None,
    };
    let fixture_toml = with_kind(
        "fixture",
        toml::to_string(&fixture_config).context("Failed to serialize fixture def to TOML")?,
    );
    fs.write_file("/fixture.toml".as_path(), fixture_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write fixture.toml: {e}"))?;

    Ok(())
}

fn write_project_toml(fs: &dyn LpFs, uid: &str, name: &str) -> Result<()> {
    let project_toml = format!(
        r#"kind = "project"
uid = "{uid}"
name = "{name}"

[nodes.output]
artifact = "./output.toml"

[nodes.texture]
artifact = "./texture.toml"

[nodes.shader]
artifact = "./shader.toml"

[nodes.fixture]
artifact = "./fixture.toml"
"#
    );
    fs.write_file("/project.toml".as_path(), project_toml.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write project.toml: {e}"))?;
    Ok(())
}

fn with_kind(kind: &str, body: String) -> String {
    format!("kind = \"{kind}\"\n{body}")
}

/// Print success message with next steps
pub fn print_success_message(dir: &Path, name: &str) {
    let uid = if let Ok(config) = std::fs::read_to_string(dir.join("project.toml")) {
        match toml::from_str::<toml::Value>(&config) {
            Ok(project_config) => project_config
                .get("uid")
                .and_then(toml::Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            Err(_) => "unknown".to_string(),
        }
    } else {
        "unknown".to_string()
    };

    let next_step_cmd =
        messages::format_command(&format!("cd {name} && lp-cli dev ws://localhost:2812/"));

    messages::print_success(
        &format!("Project created successfully: {name} (uid: {uid})"),
        &[&next_step_cmd],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;
    use tempfile::TempDir;

    #[test]
    fn test_derive_project_name() {
        assert_eq!(
            derive_project_name(Path::new("/path/to/my-project")),
            "my-project"
        );
        // "." has no file_name, so it defaults to "project"
        assert_eq!(derive_project_name(Path::new("../../../..")), "project");
    }

    #[test]
    fn test_generate_uid_format() {
        let uid = generate_uid("test-project");
        // Should match format: YYYY.MM.DD-HH.MM.SS-<name>
        assert!(uid.starts_with("20")); // Year should start with 20
        assert!(uid.contains(".")); // Should have dots
        assert!(uid.contains("-")); // Should have dashes
        assert!(uid.ends_with("-test-project")); // Should end with name
        // Format: YYYY.MM.DD-HH.MM.SS-<name> = 4 dots (YYYY.MM.DD-HH.MM.SS)
        assert_eq!(uid.matches(".").count(), 4);
        assert_eq!(uid.matches("-").count(), 3); // 3 dashes (date-time-name)
    }

    #[test]
    fn test_create_project_structure_with_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("my-project");

        create_project_structure(&project_dir, None, None).unwrap();

        assert!(project_dir.join("project.toml").exists());
        let project_toml = std::fs::read_to_string(project_dir.join("project.toml")).unwrap();
        let project_value: toml::Value = toml::from_str(&project_toml).unwrap();
        assert_eq!(project_value["name"].as_str(), Some("my-project"));
        assert!(!project_value["uid"].as_str().unwrap().is_empty());
        assert!(project_dir.join("shader.glsl").exists());
    }

    #[test]
    fn test_create_project_structure_with_custom_name_uid() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("custom");

        create_project_structure(&project_dir, Some("Custom Name"), Some("custom-uid")).unwrap();

        let project_toml = std::fs::read_to_string(project_dir.join("project.toml")).unwrap();
        let project_value: toml::Value = toml::from_str(&project_toml).unwrap();
        assert_eq!(project_value["name"].as_str(), Some("Custom Name"));
        assert_eq!(project_value["uid"].as_str(), Some("custom-uid"));
    }

    #[test]
    fn test_create_default_template() {
        let mut fs = LpFsMemory::new();

        // For memory filesystem, we need to use write_file_mut
        // In production, LpFsStd works with write_file
        create_default_template_mut(&mut fs).unwrap();

        assert!(fs.file_exists("/texture.toml".as_path()).unwrap());
        assert!(fs.file_exists("/shader.toml".as_path()).unwrap());
        assert!(fs.file_exists("/shader.glsl".as_path()).unwrap());
        assert!(fs.file_exists("/output.toml".as_path()).unwrap());
        assert!(fs.file_exists("/fixture.toml".as_path()).unwrap());
    }

    #[test]
    fn test_create_default_template_with_memory_fs() {
        let mut fs = LpFsMemory::new();

        create_default_template_mut(&mut fs).unwrap();

        // Verify texture node content
        let texture_toml = fs.read_file("/texture.toml".as_path()).unwrap();
        let texture_config: TextureDef =
            toml::from_str(std::str::from_utf8(&texture_toml).expect("UTF-8"))
                .expect("texture node TOML");
        assert_eq!(texture_config.width, 64);
        assert_eq!(texture_config.height, 64);

        // Verify shader node content
        let shader_toml = fs.read_file("/shader.toml".as_path()).unwrap();
        let shader_config: ShaderDef =
            toml::from_str(std::str::from_utf8(&shader_toml).expect("UTF-8"))
                .expect("shader node TOML");
        assert_eq!(shader_config.glsl_path, "shader.glsl".as_path_buf());
        assert_eq!(
            shader_config.texture_loc,
            RelativeNodeRef::parse("..texture").expect("valid node ref")
        );

        // Verify GLSL exists
        let glsl = fs.read_file("/shader.glsl".as_path()).unwrap();
        let glsl_str = std::str::from_utf8(&glsl).unwrap();
        assert!(glsl_str.contains("hsv_to_rgb"));
        assert!(glsl_str.contains("vec4 render"));
    }

    // Helper function for tests that use mutable filesystem
    fn create_default_template_mut(fs: &mut LpFsMemory) -> Result<()> {
        create_default_template(fs)
    }
}
