//! What format a package on disk is at — sniffed leniently, never through
//! the strict manifest parser.
//!
//! [`super::package_manifest::read_manifest`] goes through
//! [`lpc_model::ProjectManifest::read_json`], which hard-errors on unknown
//! top-level keys. That strictness is deliberate (it is what makes library
//! patches lossless), but it means a pre-mitosis `project.json` — `kind`,
//! `nodes`, … — dies there *before* anything gets to look at `format`. The
//! package then had no healthy summary, and the gallery silently dropped it:
//! a project the user could see yesterday was simply gone, with a
//! `log::warn!` nobody reads as its only trace.
//!
//! So classification comes first and parsing second, exactly as
//! `lpa_upgrade::classify` intends. Everything here reads raw bytes.

use lpa_upgrade::{FormatClass, PROJECT_MANIFEST, ProjectFiles, classify};
use lpc_model::AsLpPath;
use lpfs::LpFs;

use super::package_manifest::MANIFEST_PATH;

/// What the library found when it looked at a package, and what the user can
/// do about it.
///
/// Every arm is showable: a package that cannot be opened still gets a card
/// naming what was found and the remedy. A package never vanishes for being
/// unreadable — that was the bug this type exists to end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageHealth {
    /// Current format, manifest readable: opens normally.
    Ready,
    /// An older but supported format. Opening it migrates it in place
    /// (`lpa_upgrade`), so the card stays a normal card.
    UpgradesOnOpen { found: u32 },
    /// This Studio cannot open it as it stands.
    Blocked {
        /// Card-sized statement of what was found ("Format 3 — too old").
        headline: String,
        /// What to do about it — a full sentence, from the classifier where
        /// the classifier has one.
        remedy: String,
    },
}

impl PackageHealth {
    /// Whether the package can be opened at all (migrating if it must).
    pub fn is_openable(&self) -> bool {
        !matches!(self, Self::Blocked { .. })
    }

    /// The blocked arm's two lines, for a caller building an issue pane.
    pub fn blocked(&self) -> Option<(&str, &str)> {
        match self {
            Self::Blocked { headline, remedy } => Some((headline, remedy)),
            _ => None,
        }
    }
}

/// Read `/project.json` and classify it. A package with no manifest is
/// [`FormatClass::NotAProject`]; unreadable bytes are
/// [`FormatClass::Unreadable`]. Never fails: an unreadable package is a
/// classification, not an error.
pub fn classify_package(fs: &dyn LpFs) -> FormatClass {
    match fs.read_file(MANIFEST_PATH.as_path()) {
        Ok(bytes) => {
            let files: ProjectFiles = [(PROJECT_MANIFEST, bytes)].into_iter().collect();
            classify(&files)
        }
        Err(lpfs::FsError::NotFound(_)) => FormatClass::NotAProject,
        Err(error) => FormatClass::Unreadable {
            detail: error.to_string(),
        },
    }
}

/// Join a format classification with whatever the strict manifest reader
/// said about the same file.
///
/// `manifest_defect` is `Some` when the strict parse failed. It only decides
/// the verdict for a CURRENT-format project: below the floor, above the
/// current version, or unreadable, the format is the honest headline and the
/// parser's complaint is noise. An upgradable project is openable even if
/// today's strict parser rejects its manifest — migrating it is precisely
/// what makes it parse.
pub fn health_for(class: &FormatClass, manifest_defect: Option<&str>) -> PackageHealth {
    match class {
        FormatClass::Current => match manifest_defect {
            None => PackageHealth::Ready,
            Some(detail) => PackageHealth::Blocked {
                headline: String::from("Damaged project file"),
                remedy: format!(
                    "project.json is at the current format but could not be read ({detail}). \
                     Export a copy to repair it by hand, or delete the project."
                ),
            },
        },
        FormatClass::Upgradable { found } => PackageHealth::UpgradesOnOpen { found: *found },
        other => PackageHealth::Blocked {
            headline: headline_for(other),
            remedy: other.describe(),
        },
    }
}

/// The card-sized half of a blocked verdict. The long form is the
/// classifier's own [`FormatClass::describe`], which already names what was
/// found, what was expected, and a remedy.
fn headline_for(class: &FormatClass) -> String {
    match class {
        FormatClass::BelowFloor { found: Some(found) } => {
            format!("Format {found} — too old for this Studio")
        }
        FormatClass::BelowFloor { found: None } => {
            String::from("Format not stated — too old for this Studio")
        }
        FormatClass::FutureFormat { found } => {
            format!("Format {found} — made by a newer LightPlayer")
        }
        FormatClass::NotAProject => String::from("No project.json — not a project"),
        FormatClass::Unreadable { .. } => String::from("project.json could not be read"),
        // Not blocked; kept total rather than unreachable! so a future
        // FormatClass arm cannot panic the gallery.
        FormatClass::Current | FormatClass::Upgradable { .. } => class.describe(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::PROJECT_FORMAT_VERSION;
    use lpfs::LpFsMemory;

    fn fs_with(manifest: &[u8]) -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), manifest).unwrap();
        fs
    }

    #[test]
    fn a_pre_mitosis_manifest_classifies_instead_of_erroring() {
        // The strict reader dies on `kind`/`nodes`; classification must not.
        let fs = fs_with(br#"{"kind": "Project", "nodes": {}}"#);
        assert_eq!(
            classify_package(&fs),
            FormatClass::BelowFloor { found: None }
        );
    }

    #[test]
    fn a_missing_manifest_is_not_a_project() {
        assert_eq!(
            classify_package(&LpFsMemory::new()),
            FormatClass::NotAProject
        );
    }

    #[test]
    fn garbage_is_unreadable_not_absent() {
        assert!(matches!(
            classify_package(&fs_with(b"{ not json")),
            FormatClass::Unreadable { .. }
        ));
    }

    #[test]
    fn current_and_upgradable_are_openable_and_the_rest_are_not() {
        let current = fs_with(format!(r#"{{"format": {PROJECT_FORMAT_VERSION}}}"#).as_bytes());
        assert_eq!(
            health_for(&classify_package(&current), None),
            PackageHealth::Ready
        );

        let upgradable = fs_with(br#"{"format": 4}"#);
        assert_eq!(
            health_for(&classify_package(&upgradable), None),
            PackageHealth::UpgradesOnOpen { found: 4 }
        );

        for manifest in [
            br#"{"format": 3}"#.to_vec(),
            br#"{"format": 99}"#.to_vec(),
            b"{ not json".to_vec(),
        ] {
            let health = health_for(&classify_package(&fs_with(&manifest)), None);
            let (headline, remedy) = health.blocked().expect("blocked");
            assert!(!headline.is_empty());
            assert!(remedy.ends_with('.'), "{remedy}");
        }
    }

    #[test]
    fn an_upgradable_project_is_openable_even_if_the_strict_parser_refuses_it() {
        // Migrating is what makes an old manifest parse; refusing to open it
        // because today's parser rejects it would be the swallow bug again.
        let health = health_for(&FormatClass::Upgradable { found: 4 }, Some("unknown field"));
        assert_eq!(health, PackageHealth::UpgradesOnOpen { found: 4 });
    }

    #[test]
    fn a_current_format_project_that_will_not_parse_is_blocked_honestly() {
        let health = health_for(&FormatClass::Current, Some("unknown field `nodes`"));
        let (headline, remedy) = health.blocked().expect("blocked");
        assert_eq!(headline, "Damaged project file");
        assert!(remedy.contains("unknown field `nodes`"), "{remedy}");
    }
}
