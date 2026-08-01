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

/// Coerce arbitrary text into a usable node name.
///
/// Auto-created nodes get a kind slug, which is already safe. A **pasted**
/// node brings its own label from wherever it was copied, and that becomes
/// both a `nodes` map key and a tree segment name — so the result must
/// satisfy [`lpc_model::NodeName`]: `[A-Za-z0-9_]` with **no leading
/// digit** (see this module's header).
///
/// Anything else folds to `_`, runs collapse, a leading digit gets an `n`
/// prefix, and an empty or all-punctuation result falls back to `node`.
pub fn sanitize_node_name(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut last_was_underscore = false;
    for ch in label.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if last_was_underscore || out.is_empty() {
                continue;
            }
            last_was_underscore = true;
        } else {
            last_was_underscore = false;
        }
        out.push(mapped);
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        return "node".to_string();
    }
    // Tree segment names may not lead with a digit.
    if out.starts_with(|ch: char| ch.is_ascii_digit()) {
        out.insert(0, 'n');
    }
    out
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
    fn pasted_labels_become_safe_file_stems() {
        // A pasted node's label comes from wherever it was copied and
        // becomes a filename, so it cannot be trusted the way a kind slug
        // can.
        assert_eq!(sanitize_node_name("Orbit"), "orbit");
        assert_eq!(sanitize_node_name("My Cool Shader"), "my_cool_shader");
        assert_eq!(sanitize_node_name("a/../../etc/passwd"), "a_etc_passwd");
        assert_eq!(sanitize_node_name("dots.and.dots"), "dots_and_dots");
        assert_eq!(sanitize_node_name("  spaced  out  "), "spaced_out");
        assert_eq!(sanitize_node_name("kebab-case"), "kebab_case");
    }

    #[test]
    fn unusable_labels_fall_back_rather_than_producing_an_empty_name() {
        for label in ["", "   ", "!!!", "///", "___"] {
            assert_eq!(sanitize_node_name(label), "node", "{label:?}");
        }
    }

    #[test]
    fn every_sanitized_name_parses_as_a_tree_segment_name() {
        // The property that matters: whatever a copied label was, the
        // pasted node's name must be legal — including the no-leading-digit
        // rule this module's header calls out.
        for label in [
            "Orbit",
            "2 Cool 4 School",
            "99",
            "🌈 rainbow",
            "a/../../etc/passwd",
            "",
            "!!!",
            "-leading-dash",
        ] {
            let name = sanitize_node_name(label);
            assert!(
                lpc_model::NodeName::parse(&name).is_ok(),
                "{label:?} sanitized to {name:?}, which is not a valid node name"
            );
        }
    }

    #[test]
    fn sanitized_names_still_dedup_through_unique_node_name() {
        let taken: BTreeSet<String> = ["orbit".to_string(), "orbit_2".to_string()]
            .into_iter()
            .collect();
        assert_eq!(
            unique_node_name(&sanitize_node_name("Orbit"), &taken),
            "orbit_3"
        );
    }

    #[test]
    fn file_stem_strips_directories_and_every_extension() {
        assert_eq!(file_stem("/shader.json"), "shader");
        assert_eq!(file_stem("/nested/dir/pulse.glsl"), "pulse");
        assert_eq!(file_stem("orbit.shader.json"), "orbit");
        assert_eq!(file_stem("bare"), "bare");
    }
}
