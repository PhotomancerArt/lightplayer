//! A projects section's items: core's uid list, resolved against the
//! library's cards and drawn as cards (`PackageCard`) or rows
//! ([`ProjectRow`]).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPackageCard};

use super::home_view_mode::HomeViewMode;
use super::project_row::ProjectRow;
use crate::app::home::card_grid_class;
use crate::app::home::package_card::PackageCard;

/// A section's projects, in the section's order.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectItems(
    /// The section's `prj…` uids, in core's order.
    uids: Vec<String>,
    /// The prefix every key in this list carries, so a project shown in two
    /// sections never shares a key.
    key_prefix: &'static str,
    /// The library's cards (`UiHomeView::projects`).
    projects: Vec<UiPackageCard>,
    /// The card key (uid or slug) whose open is in flight.
    opening: Option<String>,
    mode: HomeViewMode,
    /// A fixed clock for stories ("edited 3 days ago").
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let items = resolve_items(&uids, &projects);
    let busy = opening.is_some();
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        match mode {
            HomeViewMode::Cards => rsx! {
                div { class: card_grid_class(),
                    for card in items {
                        PackageCard {
                            key: "{key_prefix}-{card.uid}",
                            opening: is_opening(opening.as_deref(), &card),
                            busy,
                            card,
                            now_secs,
                            on_action,
                        }
                    }
                }
            },
            HomeViewMode::List => rsx! {
                div { class: LIST_CLASS,
                    for card in items {
                        ProjectRow {
                            key: "{key_prefix}-{card.uid}",
                            opening: is_opening(opening.as_deref(), &card),
                            card,
                            now_secs,
                            on_action,
                        }
                    }
                }
            },
        }
    }
}

/// The rows' frame: one bordered list, a hairline between rows.
const LIST_CLASS: &str = "tw:grid tw:divide-y tw:divide-border-muted tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card";

/// The cards `uids` names, in the uids' order. A uid with no card (a
/// library change between two emissions) is skipped, and a uid named
/// twice is drawn once: a repeated key in a keyed list takes the whole app
/// down.
fn resolve_items(uids: &[String], projects: &[UiPackageCard]) -> Vec<UiPackageCard> {
    let mut seen = std::collections::BTreeSet::new();
    uids.iter()
        .filter(|uid| seen.insert(uid.as_str()))
        .filter_map(|uid| projects.iter().find(|card| card.uid == *uid))
        .cloned()
        .collect()
}

/// Whether `card`'s open is the one in flight. Opens arrive keyed by uid
/// (menu paths) or by slug (href navigation), so either matches.
fn is_opening(opening: Option<&str>, card: &UiPackageCard) -> bool {
    opening.is_some_and(|key| key == card.uid || key == card.slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::app::library::PackageHealth;

    #[test]
    fn the_sections_order_is_kept() {
        let projects = vec![
            card("prja", "alpha"),
            card("prjb", "beta"),
            card("prjc", "gamma"),
        ];
        let items = resolve_items(&uids(&["prjc", "prja", "prjb"]), &projects);
        let slugs: Vec<&str> = items.iter().map(|card| card.slug.as_str()).collect();
        assert_eq!(slugs, ["gamma", "alpha", "beta"]);
    }

    #[test]
    fn a_uid_with_no_card_is_skipped() {
        let projects = vec![card("prja", "alpha")];
        let items = resolve_items(&uids(&["prjgone", "prja"]), &projects);
        let slugs: Vec<&str> = items.iter().map(|card| card.slug.as_str()).collect();
        assert_eq!(slugs, ["alpha"]);
    }

    #[test]
    fn a_uid_named_twice_is_drawn_once() {
        let projects = vec![card("prja", "alpha")];
        assert_eq!(resolve_items(&uids(&["prja", "prja"]), &projects).len(), 1);
    }

    #[test]
    fn opening_matches_the_uid_or_the_slug() {
        let card = card("prja", "alpha");
        assert!(is_opening(Some("prja"), &card));
        assert!(is_opening(Some("alpha"), &card));
        assert!(!is_opening(Some("prjb"), &card));
        assert!(!is_opening(None, &card));
    }

    fn uids(uids: &[&str]) -> Vec<String> {
        uids.iter().map(|uid| uid.to_string()).collect()
    }

    fn card(uid: &str, slug: &str) -> UiPackageCard {
        UiPackageCard {
            uid: uid.to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: slug.to_string(),
            last_saved_at: None,
            provenance: None,
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: None,
            health: PackageHealth::Ready,
        }
    }
}
