//! Stories for the Projects page's archive drawer.
//!
//! The live section reads the `CloudSession` and one `ListMyProjects`,
//! which stories never provide — so these mount the pure list with
//! fixtures, open (capture cannot click a disclosure).

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;
use lpc_history::{PrefixedUid, UidPrefix};

use crate::app::share::archived_projects::{ArchivedProject, ArchivedProjectsList};

#[story(
    label = "Archived projects",
    description = "The library's last section, collapsed by default and shown open here: archiving is the removal verb (D8), so a row is dimmed rather than red, leads with the readable half of its link over the canonical path, and offers exactly one loud action — Restore. There is deliberately no Delete forever."
)]
pub(crate) fn archived_projects_open() -> Element {
    rsx! {
        div { class: "tw:max-w-[560px]",
            ArchivedProjectsList {
                projects: vec![
                    ArchivedProject {
                        uid: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
                        slug: "ember-field-v1".to_string(),
                    },
                    ArchivedProject {
                        uid: PrefixedUid::mint(UidPrefix::Project, &[9u8; 16]),
                        slug: "first-try-gradient".to_string(),
                    },
                ],
                initially_open: true,
            }
        }
    }
}

#[story(
    label = "Archived projects, collapsed",
    description = "The same drawer as the library actually renders it: closed, one quiet line at the page's bottom with the count, so an archive nobody is looking for costs nothing but a row."
)]
pub(crate) fn archived_projects_collapsed() -> Element {
    rsx! {
        div { class: "tw:max-w-[560px]",
            ArchivedProjectsList {
                projects: vec![
                    ArchivedProject {
                        uid: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
                        slug: "ember-field-v1".to_string(),
                    },
                    ArchivedProject {
                        uid: PrefixedUid::mint(UidPrefix::Project, &[9u8; 16]),
                        slug: "first-try-gradient".to_string(),
                    },
                ],
            }
        }
    }
}
