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
use dioxus_icons::lucide::{GitBranch, GitPullRequest, LibraryBig};

use crate::base::{IconPopoverButton, PopoverPlacement, StudioIconName};

/// GitHub slug used when no deploy `version.json` is present (local dev builds),
/// so the repo link and copyright still resolve.
const DEFAULT_REPO: &str = "light-player/lightplayer";
/// Studio is authored by Yona Appletree / photomancer.art.
const AUTHOR: &str = "Yona Appletree";
const AUTHOR_URL: &str = "https://photomancer.art";
const COPYRIGHT_YEAR: &str = "2026";

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
    /// GitHub `owner/name` slug, used to build repo/commit/PR links.
    #[serde(default)]
    pub repository: Option<String>,
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
    let repo = repo_slug(info.as_ref());
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
                        VersionDetailRow {
                            label: "commit",
                            value: commit_display(&info.source),
                            href: commit_url(&repo, &info.source),
                        }
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
                RecentUpdates { changelog, repo: repo.clone() }
            }
            VersionFooter { repo }
        }
    }
}

/// Repo link + copyright. Always rendered, so even a local dev build (no
/// `version.json`) surfaces the source and attribution.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn VersionFooter(repo: String) -> Element {
    rsx! {
        footer { class: "tw:grid tw:min-w-0 tw:gap-1.5 tw:border-t tw:border-border-subtle tw:pt-2.5",
            // Shown only when the story book is compiled into this build; the
            // link switches the app into the full-screen design library.
            if cfg!(feature = "stories") {
                a {
                    class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:hover:text-accent",
                    href: "#/stories",
                    onclick: move |event| {
                        // App() only checks the story-book hash at mount, so
                        // set the hash and reload to enter it.
                        event.prevent_default();
                        if let Some(window) = web_sys::window() {
                            let location = window.location();
                            let _ = location.set_hash("/stories");
                            let _ = location.reload();
                        }
                    },
                    LibraryBig { size: 13 }
                    span { "Design library" }
                }
            }
            a {
                class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:hover:text-accent",
                href: "{repo_url(&repo)}",
                target: "_blank",
                rel: "noopener noreferrer",
                GitBranch { size: 13 }
                span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:font-mono", "{repo}" }
            }
            span { class: "tw:text-[0.68rem] tw:text-subtle-foreground",
                "© {COPYRIGHT_YEAR} {AUTHOR} · "
                a {
                    class: "tw:text-subtle-foreground tw:underline tw:decoration-dotted tw:underline-offset-2 tw:hover:text-accent",
                    href: "{AUTHOR_URL}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "photomancer.art"
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RecentUpdates(changelog: Vec<ChangelogEntry>, repo: String) -> Element {
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
                                a {
                                    class: "tw:ml-auto tw:inline-flex tw:shrink-0 tw:items-center tw:gap-1 tw:self-center tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground tw:hover:text-accent",
                                    href: "{pr_url(&repo, pr)}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    GitPullRequest { size: 12 }
                                    "#{pr}"
                                }
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
fn VersionDetailRow(label: &'static str, value: String, href: Option<String>) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[64px_minmax(0,1fr)] tw:gap-2",
            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words",
                match href {
                    Some(href) => rsx! {
                        a {
                            class: "tw:text-muted-foreground tw:underline tw:decoration-dotted tw:underline-offset-2 tw:hover:text-accent",
                            href: "{href}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "{value}"
                        }
                    },
                    None => rsx! { "{value}" },
                }
            }
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

/// The GitHub `owner/name` slug for this build, falling back to [`DEFAULT_REPO`]
/// so links resolve even when no `version.json` was fetched.
fn repo_slug(info: Option<&VersionInfo>) -> String {
    info.and_then(|info| info.source.repository.as_deref())
        .filter(|repo| !repo.is_empty())
        .unwrap_or(DEFAULT_REPO)
        .to_string()
}

fn repo_url(repo: &str) -> String {
    format!("https://github.com/{repo}")
}

fn pr_url(repo: &str, pr: u64) -> String {
    format!("https://github.com/{repo}/pull/{pr}")
}

/// Link to the exact commit on GitHub, or `None` when no sha is known (so the
/// commit row renders as plain text rather than a dead link).
fn commit_url(repo: &str, source: &VersionSource) -> Option<String> {
    source
        .sha
        .as_deref()
        .filter(|sha| !sha.is_empty())
        .map(|sha| format!("https://github.com/{repo}/commit/{sha}"))
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
            repository: None,
        };
        assert_eq!(commit_display(&source), "01234567 (dirty)");
    }

    #[test]
    fn commit_display_clean_has_no_marker() {
        let source = VersionSource {
            sha: Some("0123456789abcdef".to_string()),
            dirty: Some(false),
            r#ref: None,
            repository: None,
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

    #[test]
    fn repo_slug_falls_back_to_default_when_absent() {
        assert_eq!(repo_slug(None), DEFAULT_REPO);
        let info = VersionInfo::default();
        assert_eq!(repo_slug(Some(&info)), DEFAULT_REPO);
    }

    #[test]
    fn repo_slug_uses_deploy_repository_when_present() {
        let info = VersionInfo {
            source: VersionSource {
                repository: Some("acme/widgets".to_string()),
                ..VersionSource::default()
            },
            ..VersionInfo::default()
        };
        assert_eq!(repo_slug(Some(&info)), "acme/widgets");
    }

    #[test]
    fn link_helpers_build_github_urls() {
        assert_eq!(repo_url("acme/widgets"), "https://github.com/acme/widgets");
        assert_eq!(
            pr_url("acme/widgets", 42),
            "https://github.com/acme/widgets/pull/42"
        );
        let source = VersionSource {
            sha: Some("0123456789abcdef".to_string()),
            ..VersionSource::default()
        };
        assert_eq!(
            commit_url("acme/widgets", &source),
            Some("https://github.com/acme/widgets/commit/0123456789abcdef".to_string())
        );
    }

    #[test]
    fn commit_url_is_none_without_sha() {
        assert_eq!(commit_url("acme/widgets", &VersionSource::default()), None);
    }
}
