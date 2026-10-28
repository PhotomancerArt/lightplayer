//! One project on the board, as the stage found it: where it lives and what
//! its `project.json` says against the engine's format.

use alloc::string::{String, ToString};

use lpa_upgrade::{PROJECT_MANIFEST, ProjectFiles, UpgradeError, stored_format};

/// A project the board lists (`ListAvailableProjects`), with its verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredProject {
    /// The project's path as the board listed it (`projects/<name>` or
    /// `/projects/<name>`); every effect about it carries it back as given.
    pub path: String,
    /// What its manifest says against the engine's format.
    pub disposition: StoredDisposition,
}

/// What one stored project needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredDisposition {
    /// At the engine's format: left alone.
    Current,
    /// Older than the engine's format: read, upgraded, written back.
    Older { found: u32 },
    /// Not upgradable as it stands (no or an unreadable manifest, or newer
    /// than the engine reads): named with its sentence and left alone.
    Refused { why: String },
}

impl StoredProject {
    /// Judge a project by its manifest's bytes (`None`: the board had no
    /// `project.json` to read) against `engine_format`.
    pub fn judge(path: String, manifest: Option<&[u8]>, engine_format: u32) -> Self {
        let mut files = ProjectFiles::new();
        if let Some(bytes) = manifest {
            files.insert(PROJECT_MANIFEST, bytes.to_vec());
        }
        let disposition = match stored_format(&files) {
            Err(class) => StoredDisposition::Refused {
                why: class.describe(),
            },
            Ok(found) if found == engine_format => StoredDisposition::Current,
            Ok(found) if found > engine_format => StoredDisposition::Refused {
                why: UpgradeError::NewerThanTarget {
                    found,
                    target: engine_format,
                }
                .to_string(),
            },
            Ok(found) => StoredDisposition::Older { found },
        };
        Self { path, disposition }
    }

    /// The project's directory name (the last path segment), which is what
    /// the board's startup choice names.
    pub fn name(&self) -> &str {
        project_name(&self.path)
    }
}

/// The last segment of a project path, without a trailing `/`.
pub(crate) fn project_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_is_judged_against_the_engines_format() {
        let judge = |manifest: Option<&[u8]>| {
            StoredProject::judge(String::from("projects/a"), manifest, 12).disposition
        };
        assert_eq!(
            judge(Some(br#"{"format": 12}"#)),
            StoredDisposition::Current
        );
        assert_eq!(
            judge(Some(br#"{"format": 11}"#)),
            StoredDisposition::Older { found: 11 }
        );
        let StoredDisposition::Refused { why } = judge(Some(br#"{"format": 13}"#)) else {
            panic!("newer is refused");
        };
        assert!(why.contains("saved by a newer LightPlayer"), "{why}");
        assert!(matches!(judge(None), StoredDisposition::Refused { .. }));
        assert!(matches!(
            judge(Some(b"not json")),
            StoredDisposition::Refused { .. }
        ));
    }

    #[test]
    fn a_project_is_named_by_its_last_segment() {
        assert_eq!(project_name("projects/eaves"), "eaves");
        assert_eq!(project_name("/projects/eaves/"), "eaves");
        assert_eq!(project_name("eaves"), "eaves");
    }
}
