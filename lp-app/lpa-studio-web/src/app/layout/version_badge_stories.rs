//! Stories for the header build-info badge details.
//!
//! `VersionDetails` is pure, so these fixtures exercise the loaded, dev-build
//! fallback, and "Recent updates" states without a network fetch.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::layout::version_badge::{
    ChangelogEntry, VersionBuild, VersionDetails, VersionInfo, VersionSource,
};

#[story(description = "Deployed build metadata with a recent-updates list.")]
pub(crate) fn loaded() -> Element {
    panel(rsx! {
        VersionDetails {
            info: Some(loaded_info()),
            changelog: changelog_entries(),
        }
    })
}

#[story(description = "Local dev build with no version.json present.")]
pub(crate) fn dev_fallback() -> Element {
    panel(rsx! {
        VersionDetails { info: None, changelog: Vec::new() }
    })
}

fn loaded_info() -> VersionInfo {
    VersionInfo {
        version: Some("2026.07.04-1".to_string()),
        channel: Some("production".to_string()),
        source: VersionSource {
            sha: Some("1de1f392c0a1b2c3".to_string()),
            dirty: Some(false),
            r#ref: Some("main".to_string()),
            repository: Some("light-player/lightplayer".to_string()),
        },
        build: VersionBuild {
            generated_at: Some("2026-07-04T13:47:00Z".to_string()),
        },
    }
}

fn changelog_entries() -> Vec<ChangelogEntry> {
    vec![
        ChangelogEntry {
            version: Some("2026.07.04-1".to_string()),
            date: Some("2026-07-04".to_string()),
            summary: Some("Gate merges into main with the pre-merge workflow".to_string()),
            pr: Some(51),
        },
        ChangelogEntry {
            version: Some("2026.06.26-3".to_string()),
            date: Some("2026-06-26".to_string()),
            summary: Some("Fix espflash deploy install".to_string()),
            pr: Some(47),
        },
        ChangelogEntry {
            version: Some("2026.05.20-1".to_string()),
            date: Some("2026-05-20".to_string()),
            summary: Some("Add fixture diagnostics and button reuse".to_string()),
            pr: None,
        },
    ]
}

fn panel(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-[min(320px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card tw:text-sm tw:text-muted-foreground tw:shadow-lg",
            {children}
        }
    }
}
