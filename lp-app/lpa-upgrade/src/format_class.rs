//! Sniffing a project's authored format version, before any parse that
//! could fail for an unrelated reason.
//!
//! This deliberately does **not** go through `ProjectManifest::read_json`.
//! That parser is strict — unknown top-level keys hard-error — so a
//! pre-mitosis manifest (`kind`, `nodes`, …) dies there before anything gets
//! to look at `format`, and the user sees a syntax complaint instead of
//! "this project is too old". Classify first, parse second; the same shape
//! `peek_header_lenient` uses for share envelopes.

use crate::json::JsonNode;
use crate::project_files::ProjectFiles;
use lpc_model::PROJECT_FORMAT_VERSION;

/// The oldest format this crate can migrate. Older projects are refused with
/// a message, never guessed at: v1→v3 predate project/module mitosis, and
/// their types are long deleted (see `schemas/history/`).
pub const UPGRADE_FLOOR: u32 = 4;

/// What a project's `project.json` says about its format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatClass {
    /// Already at [`PROJECT_FORMAT_VERSION`]; nothing to do.
    Current,
    /// At or above [`UPGRADE_FLOOR`] and below current: the migrator runs.
    Upgradable { found: u32 },
    /// Older than [`UPGRADE_FLOOR`]. `found` is `None` for a pre-mitosis
    /// root, which is diagnosable by its `kind` key rather than a version.
    BelowFloor { found: Option<u32> },
    /// Written by a newer LightPlayer than this build.
    FutureFormat { found: u32 },
    /// No `project.json` at all.
    NotAProject,
    /// `project.json` is not readable JSON.
    Unreadable { detail: String },
}

impl FormatClass {
    /// The version found in the manifest, when there was one.
    pub fn found(&self) -> Option<u32> {
        match self {
            Self::Current => Some(PROJECT_FORMAT_VERSION),
            Self::Upgradable { found } | Self::FutureFormat { found } => Some(*found),
            Self::BelowFloor { found } => *found,
            Self::NotAProject | Self::Unreadable { .. } => None,
        }
    }

    pub fn is_current(&self) -> bool {
        matches!(self, Self::Current)
    }

    pub fn is_upgradable(&self) -> bool {
        matches!(self, Self::Upgradable { .. })
    }

    /// A user-facing sentence: what was found, what this build expects, and
    /// what to do about it. Every arm names a remedy — a classification the
    /// user cannot act on is the silent-failure problem in a new costume.
    pub fn describe(&self) -> String {
        match self {
            Self::Current => format!(
                "Project format {PROJECT_FORMAT_VERSION} — already current; no upgrade needed."
            ),
            Self::Upgradable { found } => format!(
                "Project format {found}, expected {PROJECT_FORMAT_VERSION}; \
                 upgrade it to open it in this version."
            ),
            Self::BelowFloor { found: Some(found) } => format!(
                "Project format {found}, expected {PROJECT_FORMAT_VERSION}; \
                 formats below {UPGRADE_FLOOR} are too old to upgrade automatically. \
                 Open it in a LightPlayer that still reads format {found} and re-save it, \
                 or rebuild the project."
            ),
            Self::BelowFloor { found: None } => format!(
                "Project format not stated (a pre-mitosis project, format 2 or older), \
                 expected {PROJECT_FORMAT_VERSION}; formats below {UPGRADE_FLOOR} are too old \
                 to upgrade automatically. Rebuild the project in this version."
            ),
            Self::FutureFormat { found } => format!(
                "Project format {found}, expected {PROJECT_FORMAT_VERSION}; \
                 it was written by a newer LightPlayer. Update LightPlayer to open it."
            ),
            Self::NotAProject => String::from(
                "No project.json — this is not a LightPlayer project. \
                 Pick a folder that contains project.json.",
            ),
            Self::Unreadable { detail } => format!(
                "project.json could not be read as a project manifest ({detail}); \
                 expected a JSON object stating format {PROJECT_FORMAT_VERSION}. \
                 Fix or restore the file before opening the project."
            ),
        }
    }
}

/// Sniff `files`' format without parsing any artifact.
pub fn classify(files: &ProjectFiles) -> FormatClass {
    let Some(bytes) = files.manifest() else {
        return FormatClass::NotAProject;
    };
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => {
            return FormatClass::Unreadable {
                detail: e.to_string(),
            };
        }
    };
    let manifest = match JsonNode::parse(text) {
        Ok(manifest) => manifest,
        Err(e) => return FormatClass::Unreadable { detail: e.detail },
    };
    if manifest.object().is_none() {
        return FormatClass::Unreadable {
            detail: String::from("project manifest root must be an object"),
        };
    }

    let Some(format) = manifest.get("format") else {
        // No version at all. A `kind` key means a pre-mitosis root, back
        // when project.json WAS the root node artifact
        // (`lpc-model/src/project/manifest.rs`, `read_json` tests).
        return if manifest.get("kind").is_some() {
            FormatClass::BelowFloor { found: None }
        } else {
            FormatClass::Unreadable {
                detail: String::from("project.json has no `format` version"),
            }
        };
    };
    let Some(found) = format.as_u32() else {
        return FormatClass::Unreadable {
            detail: String::from("field `format` must be an unsigned integer"),
        };
    };

    match found {
        found if found == PROJECT_FORMAT_VERSION => FormatClass::Current,
        found if found > PROJECT_FORMAT_VERSION => FormatClass::FutureFormat { found },
        found if found >= UPGRADE_FLOOR => FormatClass::Upgradable { found },
        found => FormatClass::BelowFloor { found: Some(found) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_format_is_recognized() {
        let manifest = format!(r#"{{"format": {PROJECT_FORMAT_VERSION}}}"#);
        assert_eq!(classify(&manifest_files(&manifest)), FormatClass::Current);
    }

    #[test]
    fn the_floor_is_upgradable_and_below_it_is_not() {
        assert_eq!(
            classify(&manifest_files(r#"{"format": 4}"#)),
            FormatClass::Upgradable { found: 4 }
        );
        for found in 1..UPGRADE_FLOOR {
            assert_eq!(
                classify(&manifest_files(&format!(r#"{{"format": {found}}}"#))),
                FormatClass::BelowFloor { found: Some(found) }
            );
        }
    }

    #[test]
    fn a_pre_mitosis_root_is_diagnosed_by_its_kind_key() {
        assert_eq!(
            classify(&manifest_files(r#"{"kind": "Project", "nodes": {}}"#)),
            FormatClass::BelowFloor { found: None }
        );
    }

    #[test]
    fn a_newer_format_is_not_guessed_at() {
        assert_eq!(
            classify(&manifest_files(r#"{"format": 999}"#)),
            FormatClass::FutureFormat { found: 999 }
        );
    }

    #[test]
    fn missing_and_broken_manifests_are_distinguished() {
        assert_eq!(classify(&ProjectFiles::new()), FormatClass::NotAProject);
        assert!(matches!(
            classify(&manifest_files("{ not json")),
            FormatClass::Unreadable { .. }
        ));
        assert!(matches!(
            classify(&manifest_files("[1, 2]")),
            FormatClass::Unreadable { .. }
        ));
        assert!(matches!(
            classify(&manifest_files(r#"{"format": "4"}"#)),
            FormatClass::Unreadable { .. }
        ));
        assert!(matches!(
            classify(&manifest_files(r#"{"name": "no version"}"#)),
            FormatClass::Unreadable { .. }
        ));
    }

    #[test]
    fn every_description_names_the_expectation_and_a_remedy() {
        let classes = [
            FormatClass::Current,
            FormatClass::Upgradable { found: 4 },
            FormatClass::BelowFloor { found: Some(2) },
            FormatClass::BelowFloor { found: None },
            FormatClass::FutureFormat { found: 999 },
            FormatClass::NotAProject,
            FormatClass::Unreadable {
                detail: String::from("boom"),
            },
        ];
        for class in classes {
            let message = class.describe();
            assert!(message.ends_with('.'), "{message}");
            if let Some(found) = class.found() {
                assert!(message.contains(&found.to_string()), "{message}");
            }
        }
    }

    fn manifest_files(manifest: &str) -> ProjectFiles {
        [("project.json", manifest.as_bytes().to_vec())]
            .into_iter()
            .collect()
    }
}
