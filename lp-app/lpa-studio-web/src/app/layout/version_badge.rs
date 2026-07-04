//! Header build-info badge.
//!
//! [`VersionBadge`] is a small header indicator that fetches the deployed
//! `version.json` (and `changelog.json`) at runtime and surfaces which build is
//! actually live. Both files are written into the site root by
//! `scripts/pages/prepare-pages-artifact.mjs` on every Pages deploy; local dev
//! builds (`dx serve`, `just studio-web-build`) do not emit them, so the fetch
//! 404s and the badge degrades to a "dev build" state.
//!
//! The presentational [`VersionDetails`] component is pure — it takes its data
//! via props so stories can render fixtures without a network fetch. The
//! [`VersionBadge`] wrapper owns the fetches and drives that component.

use dioxus::prelude::*;

use crate::base::{IconPopoverButton, PopoverPlacement, StudioIconName};

/// Subset of the deploy `version.json` schema (v1) that the UI renders.
///
/// Every field is optional so schema drift never panics the badge.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct VersionInfo {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub source: VersionSource,
    #[serde(default)]
    pub build: VersionBuild,
}

/// `source` block of `version.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct VersionSource {
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default)]
    pub dirty: Option<bool>,
    #[serde(default)]
    pub r#ref: Option<String>,
}

/// `build` block of `version.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct VersionBuild {
    #[serde(default, rename = "generatedAt")]
    pub generated_at: Option<String>,
}

/// Subset of the deploy `changelog.json` schema (v1) that the UI renders.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Changelog {
    #[serde(default)]
    pub entries: Vec<ChangelogEntry>,
}

/// One "Recent updates" row: a single version tag, best-effort summarized.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct ChangelogEntry {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub pr: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum VersionState {
    Loading,
    Loaded(VersionInfo),
    Unavailable,
}

/// Header indicator: fetches deploy metadata and renders it in a detail popover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VersionBadge() -> Element {
    let mut state = use_signal(|| VersionState::Loading);
    let mut changelog = use_signal(Vec::<ChangelogEntry>::new);

    // version.json and changelog.json are fetched independently so a missing
    // changelog never blanks the current-build details, and vice versa.
    use_future(move || async move {
        match fetch_json::<VersionInfo>("version.json").await {
            Some(info) => state.set(VersionState::Loaded(info)),
            None => state.set(VersionState::Unavailable),
        }
    });
    use_future(move || async move {
        if let Some(log) = fetch_json::<Changelog>("changelog.json").await {
            changelog.set(log.entries);
        }
    });

    let current = state();
    let info = match &current {
        VersionState::Loaded(info) => Some(info.clone()),
        VersionState::Loading | VersionState::Unavailable => None,
    };
    let trigger_label = trigger_label(&current);

    rsx! {
        div { class: "tw:ml-auto tw:flex tw:min-w-0 tw:items-center tw:gap-2",
            span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:font-mono tw:text-[0.7rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                "{trigger_label}"
            }
            IconPopoverButton {
                class: TRIGGER_CLASS.to_string(),
                open_class: TRIGGER_OPEN_CLASS.to_string(),
                icon: StudioIconName::Info,
                icon_size: 15,
                label: "Build info".to_string(),
                title: "Build info".to_string(),
                popup_class: POPUP_CLASS.to_string(),
                chrome_class: "ux-popover-chrome-neutral".to_string(),
                placement: PopoverPlacement::BottomEnd,
                VersionDetails { info, changelog: changelog() }
            }
        }
    }
}

/// Pure presentation of the build details + recent-updates list.
///
/// `info == None` renders the local dev-build fallback. A non-empty `changelog`
/// renders a secondary "Recent updates" section; an empty one omits it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VersionDetails(info: Option<VersionInfo>, changelog: Vec<ChangelogEntry>) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:p-3",
            div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                strong { class: "tw:text-sm tw:text-strong-foreground", "Build info" }
                span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground",
                    "Sourced from the deployed artifact"
                }
            }
            match info {
                Some(info) => rsx! {
                    dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-2 tw:text-xs",
                        VersionDetailRow { label: "version", value: display_or(info.version.as_deref(), "unknown") }
                        VersionDetailRow { label: "channel", value: display_or(info.channel.as_deref(), "—") }
                        VersionDetailRow { label: "commit", value: commit_display(&info.source) }
                        VersionDetailRow { label: "built", value: display_or(info.build.generated_at.as_deref(), "—") }
                    }
                },
                None => rsx! {
                    p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground",
                        "Dev build — version metadata is only present in deployed builds."
                    }
                },
            }
            if !changelog.is_empty() {
                RecentUpdates { changelog }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RecentUpdates(changelog: Vec<ChangelogEntry>) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-1.5 tw:border-t tw:border-border-subtle tw:pt-2.5",
            span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "Recent updates" }
            ul { class: "tw:m-0 tw:grid tw:min-w-0 tw:list-none tw:gap-1.5 tw:p-0",
                for entry in changelog {
                    li { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2",
                            span { class: "tw:font-mono tw:text-xs tw:font-bold tw:text-strong-foreground", "{display_or(entry.version.as_deref(), \"unknown\")}" }
                            if let Some(date) = entry.date.as_deref() {
                                span { class: "tw:text-[0.68rem] tw:text-subtle-foreground", "{date}" }
                            }
                            if let Some(pr) = entry.pr {
                                span { class: "tw:ml-auto tw:shrink-0 tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground", "#{pr}" }
                            }
                        }
                        if let Some(summary) = entry.summary.as_deref() {
                            span { class: "tw:min-w-0 tw:text-xs tw:text-muted-foreground tw:break-words", "{summary}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn VersionDetailRow(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[64px_minmax(0,1fr)] tw:gap-2",
            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words", "{value}" }
        }
    }
}

const TRIGGER_CLASS: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:p-0 tw:text-status-neutral-foreground";
const TRIGGER_OPEN_CLASS: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-card-raised tw:p-0 tw:text-status-neutral-foreground";
const POPUP_CLASS: &str = "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card tw:bg-[linear-gradient(90deg,var(--studio-status-neutral-bg),transparent_74%)] tw:text-sm tw:text-muted-foreground tw:shadow-lg";

/// Fetch and deserialize same-origin static JSON. Any failure (404 in dev,
/// network error, parse error) resolves to `None` — the caller degrades.
async fn fetch_json<T: serde::de::DeserializeOwned>(path: &str) -> Option<T> {
    let response = gloo_net::http::Request::get(path).send().await.ok()?;
    if !response.ok() {
        return None;
    }
    response.json::<T>().await.ok()
}

/// At-a-glance header text for the current fetch state.
fn trigger_label(state: &VersionState) -> String {
    match state {
        VersionState::Loading => "…".to_string(),
        VersionState::Unavailable => "dev build".to_string(),
        VersionState::Loaded(info) => info
            .version
            .clone()
            .filter(|version| !version.is_empty())
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

fn display_or(value: Option<&str>, fallback: &str) -> String {
    match value {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => fallback.to_string(),
    }
}

/// Short commit sha with a `(dirty)` marker when the deploy was built from a
/// dirty tree.
fn commit_display(source: &VersionSource) -> String {
    let sha = match source.sha.as_deref() {
        Some(sha) if !sha.is_empty() => sha,
        _ => return "—".to_string(),
    };
    let short: String = sha.chars().take(8).collect();
    if source.dirty == Some(true) {
        format!("{short} (dirty)")
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_display_shortens_and_marks_dirty() {
        let source = VersionSource {
            sha: Some("0123456789abcdef".to_string()),
            dirty: Some(true),
            r#ref: None,
        };
        assert_eq!(commit_display(&source), "01234567 (dirty)");
    }

    #[test]
    fn commit_display_clean_has_no_marker() {
        let source = VersionSource {
            sha: Some("0123456789abcdef".to_string()),
            dirty: Some(false),
            r#ref: None,
        };
        assert_eq!(commit_display(&source), "01234567");
    }

    #[test]
    fn commit_display_missing_sha_is_dash() {
        assert_eq!(commit_display(&VersionSource::default()), "—");
    }

    #[test]
    fn trigger_label_uses_version_when_loaded() {
        let info = VersionInfo {
            version: Some("2026.07.04-1".to_string()),
            ..VersionInfo::default()
        };
        assert_eq!(trigger_label(&VersionState::Loaded(info)), "2026.07.04-1");
    }

    #[test]
    fn trigger_label_unavailable_is_dev_build() {
        assert_eq!(trigger_label(&VersionState::Unavailable), "dev build");
    }
}
