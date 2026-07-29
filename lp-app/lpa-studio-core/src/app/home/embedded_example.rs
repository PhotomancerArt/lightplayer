//! The compiled-in example packages (offline/first-run fallback).
//!
//! Until the examples place lands (M6, D17), the gallery's *Examples*
//! section lists these. The id doubles as the seed-once provenance source
//! (`SeededFrom { source }`), so a package seeded by the pre-M4 demo flow
//! and one opened from the gallery are the same package.
//!
//! Effect examples (kind `Effect`) are workbench projects: the openable
//! unit is the whole rig (clock + preview fixture + output), and the
//! vendorable unit is the effect subfolder its root references
//! (effects-are-projects ADR). All shipped example content is CC0 unless
//! otherwise noted.

use crate::app::project::demo_project::{DEMO_PROJECT_ID, demo_project_files};

/// One compiled-in example.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedExample {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
}

/// One compiled-in example file, path relative to the example root.
pub struct EmbeddedExampleFile {
    pub relative_path: &'static str,
    pub bytes: &'static [u8],
}

/// Compiled-in files for one `examples/<...>` directory.
macro_rules! example_files {
    ($base:literal, [$($file:literal),+ $(,)?]) => {
        &[$(EmbeddedExampleFile {
            relative_path: $file,
            bytes: include_bytes!(concat!("../../../../../examples/", $base, "/", $file)),
        }),+]
    };
}

pub const PLASMA_EXAMPLE_ID: &str = "examples/effects/plasma";
pub const METEOR_EXAMPLE_ID: &str = "examples/effects/meteor";

static PLASMA_FILES: &[EmbeddedExampleFile] = example_files!(
    "effects/plasma",
    [
        "project.json",
        "clock.json",
        "fixture.json",
        "fixture.map2d.json",
        "output.json",
        "plasma/project.json",
        "plasma/shader.json",
        "plasma/main.glsl",
    ]
);

static METEOR_FILES: &[EmbeddedExampleFile] = example_files!(
    "effects/meteor",
    [
        "project.json",
        "clock.json",
        "fixture.json",
        "output.json",
        "meteor/project.json",
        "meteor/sim.json",
        "meteor/sim.glsl",
        "meteor/render.json",
        "meteor/render.glsl",
    ]
);

impl EmbeddedExample {
    /// The example's package files as (relative path, bytes).
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        let files: &[EmbeddedExampleFile] = match self.id {
            PLASMA_EXAMPLE_ID => PLASMA_FILES,
            METEOR_EXAMPLE_ID => METEOR_FILES,
            _ => {
                return demo_project_files()
                    .iter()
                    .map(|file| (file.relative_path.to_string(), file.bytes.to_vec()))
                    .collect();
            }
        };
        files
            .iter()
            .map(|file| (file.relative_path.to_string(), file.bytes.to_vec()))
            .collect()
    }
}

/// All embedded examples, gallery order.
pub fn embedded_examples() -> &'static [EmbeddedExample] {
    &[
        EmbeddedExample {
            id: DEMO_PROJECT_ID,
            name: "Fyeah Sign",
            kind: "Project",
        },
        EmbeddedExample {
            id: PLASMA_EXAMPLE_ID,
            name: "Plasma",
            kind: "Effect",
        },
        EmbeddedExample {
            id: METEOR_EXAMPLE_ID,
            name: "Meteor",
            kind: "Effect",
        },
    ]
}

/// Look up an embedded example by id.
pub fn embedded_example(id: &str) -> Option<EmbeddedExample> {
    embedded_examples()
        .iter()
        .copied()
        .find(|example| example.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_example_is_embedded_with_files() {
        let example = embedded_example(DEMO_PROJECT_ID).expect("demo example is embedded");
        assert_eq!(example.name, "Fyeah Sign");
        assert_eq!(example.kind, "Project");
        let files = example.files();
        assert!(
            files
                .iter()
                .any(|(path, _)| path == "project.json" && !files.is_empty())
        );
    }

    #[test]
    fn effect_examples_are_embedded_workbenches() {
        for (id, name, effect_def) in [
            (PLASMA_EXAMPLE_ID, "Plasma", "plasma/project.json"),
            (METEOR_EXAMPLE_ID, "Meteor", "meteor/project.json"),
        ] {
            let example = embedded_example(id).expect("effect example is embedded");
            assert_eq!(example.name, name);
            assert_eq!(example.kind, "Effect");
            let files = example.files();
            assert!(
                files.iter().any(|(path, _)| path == "project.json"),
                "{id}: workbench root present"
            );
            assert!(
                files.iter().any(|(path, _)| path == effect_def),
                "{id}: nested effect def present"
            );
            // Every asset the workbench's fixture references must travel
            // with the copy — a missing mapping document would open as a
            // fixture with no lamps.
            for (path, bytes) in &files {
                assert!(!bytes.is_empty(), "{id}: {path} is empty");
            }
        }
    }

    /// Every asset an embedded example's node defs reference must be in
    /// its file list: a gallery-opened copy has only these bytes, so a
    /// missing mapping document or shader source opens broken.
    #[test]
    fn effect_example_files_carry_every_referenced_asset() {
        for id in [PLASMA_EXAMPLE_ID, METEOR_EXAMPLE_ID] {
            let files = embedded_example(id).expect("example").files();
            let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
            for (path, bytes) in &files {
                if !path.ends_with(".json") {
                    continue;
                }
                let text = core::str::from_utf8(bytes).expect("utf-8");
                let dir = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
                // Asset refs are file-relative bare strings on `source`
                // fields (shader/compute source, Map2d mapping document).
                for needle in ["\"source\": \""] {
                    let mut rest = text;
                    while let Some(at) = rest.find(needle) {
                        rest = &rest[at + needle.len()..];
                        let Some(end) = rest.find('"') else { break };
                        let asset = &rest[..end];
                        // Binding endpoints share the `source` key name but
                        // are refs, not files.
                        if asset.is_empty()
                            || asset.contains('{')
                            || asset.starts_with("node:")
                            || asset.starts_with("bus:")
                        {
                            continue;
                        }
                        let stripped = asset.strip_prefix("./").unwrap_or(asset);
                        let expected = if dir.is_empty() {
                            stripped.to_string()
                        } else {
                            format!("{dir}/{stripped}")
                        };
                        assert!(
                            paths.contains(&expected.as_str()),
                            "{id}: {path} references {asset}, which is not in the package \
                             (paths: {paths:?})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unknown_example_is_none() {
        assert!(embedded_example("examples/unknown").is_none());
    }
}
