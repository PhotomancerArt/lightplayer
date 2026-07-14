//! The compiled-in example packages (offline/first-run fallback).
//!
//! Until the examples place lands (M6, D17), the gallery's *Examples*
//! section lists these. The id doubles as the seed-once provenance source
//! (`SeededFrom { source }`), so a package seeded by the pre-M4 demo flow
//! and one opened from the gallery are the same package.

use crate::app::project::demo_project::{DEMO_PROJECT_ID, demo_project_files};

/// One compiled-in example.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedExample {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
}

impl EmbeddedExample {
    /// The example's package files as (relative path, bytes).
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        // every embedded example currently ships the demo file set; a second
        // example would grow a match here
        demo_project_files()
            .iter()
            .map(|file| (file.relative_path.to_string(), file.bytes.to_vec()))
            .collect()
    }
}

/// All embedded examples, gallery order.
pub fn embedded_examples() -> &'static [EmbeddedExample] {
    &[EmbeddedExample {
        id: DEMO_PROJECT_ID,
        name: "Basic",
        kind: "Project",
    }]
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
    fn basic_example_is_embedded_with_files() {
        let example = embedded_example("examples/basic").expect("basic is embedded");
        assert_eq!(example.name, "Basic");
        assert_eq!(example.kind, "Project");
        let files = example.files();
        assert!(
            files
                .iter()
                .any(|(path, _)| path == "project.json" && !files.is_empty())
        );
    }

    #[test]
    fn unknown_example_is_none() {
        assert!(embedded_example("examples/unknown").is_none());
    }
}
