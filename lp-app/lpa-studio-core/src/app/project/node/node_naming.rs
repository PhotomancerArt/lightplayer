//! Auto-naming for created nodes — the node-scoped sibling of
//! [`crate::app::library::package_slug`].
//!
//! Node names double as project `nodes` map keys and tree segment names, and
//! tree segment names must parse as [`lpc_model::NodeName`] (`[A-Za-z0-9_]`,
//! no leading digit — see `TreePath`). Hyphens are therefore **not** legal
//! here, unlike package slugs: dedup suffixes use `_2`/`_3`, and multi-word
//! kinds slug with underscores (`compute_shader`).

use std::collections::BTreeSet;

use lpc_model::NodeKind;

/// The name slug for a node kind: the base of an auto-generated node name
/// and file stem (`shader` → `shader.json` + `shader.glsl`).
pub fn node_kind_slug(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Project => "project",
        NodeKind::Button => "button",
        NodeKind::Clock => "clock",
        NodeKind::Texture => "texture",
        NodeKind::Shader => "shader",
        NodeKind::ComputeShader => "compute_shader",
        NodeKind::Fluid => "fluid",
        NodeKind::Playlist => "playlist",
        NodeKind::ControlRadio => "radio",
        NodeKind::Output => "output",
        NodeKind::Fixture => "fixture",
    }
}

/// Human-readable picker label for a node kind.
pub fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Project => "Project",
        NodeKind::Button => "Button",
        NodeKind::Clock => "Clock",
        NodeKind::Texture => "Texture",
        NodeKind::Shader => "Shader",
        NodeKind::ComputeShader => "Compute shader",
        NodeKind::Fluid => "Fluid",
        NodeKind::Playlist => "Playlist",
        NodeKind::ControlRadio => "Radio",
        NodeKind::Output => "Output",
        NodeKind::Fixture => "Fixture",
    }
}

/// First of `slug`, `slug_2`, `slug_3`, … not present in `taken`.
///
/// Callers populate `taken` with BOTH the effective `nodes` map keys and the
/// stems of known project files (`./shader.json` and `./shader.glsl` both
/// contribute `shader`), so a created def/asset file can never collide with
/// an existing file even when the map key itself is free.
pub fn unique_node_name(slug: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(slug) {
        return slug.to_string();
    }
    for i in 2usize.. {
        let candidate = format!("{slug}_{i}");
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded suffix search")
}

/// The stem of a project file path: the file name up to its first `.`
/// (`/nested/shader_2.json` → `shader_2`). Auto-names dedup against these.
pub fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_slug_is_a_valid_node_name() {
        for kind in [
            NodeKind::Project,
            NodeKind::Button,
            NodeKind::Clock,
            NodeKind::Texture,
            NodeKind::Shader,
            NodeKind::ComputeShader,
            NodeKind::Fluid,
            NodeKind::Playlist,
            NodeKind::ControlRadio,
            NodeKind::Output,
            NodeKind::Fixture,
        ] {
            let slug = node_kind_slug(kind);
            assert!(
                lpc_model::NodeName::parse(slug).is_ok(),
                "{kind:?} slug {slug} must parse as a tree segment name"
            );
            // The suffixed forms stay valid too.
            assert!(lpc_model::NodeName::parse(&format!("{slug}_2")).is_ok());
        }
    }

    #[test]
    fn dedups_against_node_keys() {
        let taken: BTreeSet<String> = ["shader".to_string(), "shader_2".to_string()].into();
        assert_eq!(unique_node_name("shader", &taken), "shader_3");
        assert_eq!(unique_node_name("clock", &taken), "clock");
    }

    #[test]
    fn dedups_against_file_stems() {
        // A file named `texture.json` blocks the bare name even when the
        // `nodes` key `texture` is free.
        let taken: BTreeSet<String> = [file_stem("/texture.json").to_string()].into();
        assert_eq!(unique_node_name("texture", &taken), "texture_2");
    }

    #[test]
    fn file_stem_strips_directories_and_every_extension() {
        assert_eq!(file_stem("/shader.json"), "shader");
        assert_eq!(file_stem("/nested/dir/pulse.glsl"), "pulse");
        assert_eq!(file_stem("orbit.shader.json"), "orbit");
        assert_eq!(file_stem("bare"), "bare");
    }
}
