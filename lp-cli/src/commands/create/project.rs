//! Project creation logic
//!
//! Creates new projects from the shared starter composition in `lpc-model`
//! (`starter_project_files`): clock + texture + shader + GLSL + output +
//! fixture, wired over the bus, with a rainbow rotating color wheel shader.

use anyhow::{Context, Result};
use std::path::Path;

use lpc_model::{AsLpPath, SlotShapeRegistry, starter_project_files};
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

/// Create project directory structure
///
/// Creates the project directory and writes the starter project file set.
pub fn create_project_structure(dir: &Path, name: Option<&str>) -> Result<()> {
    // Create directory if doesn't exist
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;

    // Derive name from directory if not provided
    let project_name = if let Some(name) = name {
        name.to_string()
    } else {
        derive_project_name(dir)
    };

    // Create filesystem view for project directory
    let fs = lpfs::LpFsStd::new(dir.to_path_buf());

    write_starter_project(&fs, &project_name)
}

/// Write the shared starter project composition into `fs` (chrooted at the
/// project root).
pub fn write_starter_project(fs: &dyn LpFs, name: &str) -> Result<()> {
    let registry = SlotShapeRegistry::default();
    let files = starter_project_files(name, &registry)
        .map_err(|e| anyhow::anyhow!("Failed to serialize starter project: {e}"))?;
    for (relative, bytes) in files {
        let path = format!("/{relative}");
        fs.write_file(path.as_str().as_path(), &bytes)
            .map_err(|e| anyhow::anyhow!("Failed to write {relative}: {e}"))?;
    }
    Ok(())
}

/// Print success message with next steps
pub fn print_success_message(_dir: &Path, name: &str) {
    let next_step_cmd =
        messages::format_command(&format!("cd {name} && lp-cli dev ws://localhost:2812/"));

    messages::print_success(
        &format!("Project created successfully: {name}"),
        &[&next_step_cmd],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{BindingRef, NodeDef};
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
    fn test_create_project_structure_with_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("my-project");

        create_project_structure(&project_dir, None).unwrap();

        assert!(project_dir.join("project.json").exists());
        let project_json = std::fs::read_to_string(project_dir.join("project.json")).unwrap();
        let def = NodeDef::from_json_str(&project_json).unwrap();
        let NodeDef::Module(project) = def else {
            panic!("expected project def");
        };
        assert_eq!(project.name(), Some("my-project"));
        assert!(!project_json.contains("\"uid\""));
        assert!(project_dir.join("shader.glsl").exists());
    }

    #[test]
    fn test_create_project_structure_with_custom_name() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("custom");

        create_project_structure(&project_dir, Some("Custom Name")).unwrap();

        let project_json = std::fs::read_to_string(project_dir.join("project.json")).unwrap();
        let def = NodeDef::from_json_str(&project_json).unwrap();
        let NodeDef::Module(project) = def else {
            panic!("expected project def");
        };
        assert_eq!(project.name(), Some("Custom Name"));
        assert!(!project_json.contains("\"uid\""));
    }

    #[test]
    fn created_file_set_matches_shared_composition() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join("compo");

        create_project_structure(&project_dir, Some("compo")).unwrap();

        let registry = SlotShapeRegistry::default();
        let expected = starter_project_files("compo", &registry).unwrap();
        let mut on_disk: Vec<String> = std::fs::read_dir(&project_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut expected_names: Vec<String> =
            expected.iter().map(|(name, _)| name.clone()).collect();
        expected_names.sort();
        assert_eq!(on_disk, expected_names, "file set matches the composition");

        for (name, bytes) in &expected {
            let written = std::fs::read(project_dir.join(name)).unwrap();
            assert_eq!(&written, bytes, "{name} bytes match the composition");
        }
    }

    #[test]
    fn test_write_starter_project_with_memory_fs() {
        let fs = LpFsMemory::new();

        write_starter_project(&fs, "demo").unwrap();

        assert!(fs.file_exists("/project.json".as_path()).unwrap());
        assert!(fs.file_exists("/clock.json".as_path()).unwrap());
        assert!(fs.file_exists("/texture.json".as_path()).unwrap());
        assert!(fs.file_exists("/shader.json".as_path()).unwrap());
        assert!(fs.file_exists("/shader.glsl".as_path()).unwrap());
        assert!(fs.file_exists("/output.json".as_path()).unwrap());
        assert!(fs.file_exists("/fixture.json".as_path()).unwrap());

        // Verify texture node content
        let texture_json = fs.read_file("/texture.json".as_path()).unwrap();
        let texture_config =
            NodeDef::from_json_str(std::str::from_utf8(&texture_json).expect("UTF-8"))
                .expect("texture node JSON");
        let NodeDef::Texture(texture_config) = texture_config else {
            panic!("expected texture node JSON");
        };
        assert_eq!(texture_config.width(), 64);
        assert_eq!(texture_config.height(), 64);
        assert!(matches!(
            texture_config.bindings.entries()["input"].source_ref(),
            Some(BindingRef::Bus(_))
        ));

        // Verify shader node content
        let shader_json = fs.read_file("/shader.json".as_path()).unwrap();
        let shader_config =
            NodeDef::from_json_str(std::str::from_utf8(&shader_json).expect("UTF-8"))
                .expect("shader node JSON");
        let NodeDef::Shader(shader_config) = shader_config else {
            panic!("expected shader node JSON");
        };
        assert_eq!(
            shader_config
                .shader_source()
                .artifact_value()
                .unwrap()
                .to_string(),
            "shader.glsl"
        );
        assert!(matches!(
            shader_config.bindings.entries()["output"].target_ref(),
            Some(BindingRef::Bus(_))
        ));

        // Verify GLSL exists
        let glsl = fs.read_file("/shader.glsl".as_path()).unwrap();
        let glsl_str = std::str::from_utf8(&glsl).unwrap();
        assert!(glsl_str.contains("hsv_to_rgb"));
        assert!(glsl_str.contains("vec4 render"));
    }
}
