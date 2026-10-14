//! One library project as a row, for the home page's list view.
//!
//! ```text
//!   [sw] 2026-07-02-0930-porch-sign   Edited 2 h ago   On Desk C6   Open  ⋯
//! ```
//!
//! A poster swatch, the slug as the title, when it was edited, which boards
//! play it, and an **Open** link (the project's share address, a plain
//! `<a>`: opening is navigation). The ⋯ is the card's own menu
//! (`PackageCardMenu`), so a row offers exactly what a card does. A package
//! this Studio cannot open shows its headline in the attention tone and no
//! Open link. The row builds no action.

use dioxus::prelude::*;
use lpa_studio_core::core::time_ago::time_ago;
use lpa_studio_core::{UiAction, UiPackageCard};
use lpc_cloud_api::share_link::slugify;

use crate::app::home::boards_line::boards_line;
use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::package_card::{PackageCardMenu, platform_now_secs};
use crate::core::quiet_action_class;
use crate::router::canonical_share_path;

/// One project in the list.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectRow(
    card: UiPackageCard,
    /// This project's open is in flight: the Open link holds navigation.
    #[props(default)]
    opening: bool,
    /// A fixed clock for stories; `None` reads the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let facts = row_facts(&card, now_secs.unwrap_or_else(platform_now_secs));
    let swatch = thumb_swatch_style(&card.uid, facts.blocked.is_some());

    rsx! {
        div { class: ROW_CLASS,
            div {
                class: "tw:flex-none tw:rounded-sm",
                style: "width: 36px; height: 36px; {swatch}",
                aria_hidden: "true",
            }
            div { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-0.5",
                span {
                    class: "tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                    title: "{card.slug}",
                    "{card.slug}"
                }
                if let Some(headline) = facts.blocked.clone() {
                    span { class: "tw:truncate tw:text-xs tw:font-semibold tw:text-status-attention-foreground",
                        "{headline}"
                    }
                } else if let Some(edited) = facts.edited.clone() {
                    span { class: "tw:truncate tw:text-xs tw:text-muted-foreground", "Edited {edited}" }
                }
            }
            if let Some(boards) = facts.boards.clone() {
                span {
                    class: "tw:max-w-[40%] tw:flex-none tw:truncate tw:text-xs tw:text-muted-foreground",
                    title: "{boards}",
                    "{boards}"
                }
            }
            if let Some(href) = facts.open_href.clone() {
                a {
                    class: quiet_action_class(),
                    href: "{href}",
                    title: "Open this project.",
                    onclick: move |event: MouseEvent| {
                        // Only this project's own open in flight holds the
                        // navigation; any other supersedes it (D4).
                        if opening {
                            event.prevent_default();
                        }
                    },
                    "Open"
                }
            }
            PackageCardMenu {
                card: card.clone(),
                edited_line: facts.edited,
                open_href: facts.open_href,
                opening,
                on_action,
            }
        }
    }
}

/// The row: one line (the list draws the frame and the hairlines).
const ROW_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-3 tw:px-3 tw:py-2";

/// What a row says about its project.
#[derive(Debug, PartialEq)]
struct ProjectRowFacts {
    /// "2 h ago", when the project was ever saved.
    edited: Option<String>,
    /// "On Desk C6", when a board plays it.
    boards: Option<String>,
    /// The package's blocked headline, when this Studio cannot open it.
    blocked: Option<String>,
    /// The project's share address, when it can be opened.
    open_href: Option<String>,
}

fn row_facts(card: &UiPackageCard, now: f64) -> ProjectRowFacts {
    let blocked = card
        .health
        .blocked()
        .map(|(headline, _remedy)| headline.to_string());
    ProjectRowFacts {
        edited: card.last_saved_at.map(|at| time_ago(now, at)),
        boards: boards_line(&card.on_boards),
        open_href: blocked
            .is_none()
            .then(|| canonical_share_path(&slugify(&card.slug), &card.uid)),
        blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::app::library::PackageHealth;

    #[test]
    fn the_boards_column_appears_only_with_boards() {
        let alone = row_facts(&card(), 0.0);
        assert_eq!(alone.boards, None);

        let played = row_facts(
            &UiPackageCard {
                on_boards: vec!["Desk C6".to_string()],
                ..card()
            },
            0.0,
        );
        assert_eq!(played.boards.as_deref(), Some("On Desk C6"));
    }

    #[test]
    fn an_openable_project_links_to_its_share_address() {
        let facts = row_facts(&card(), 0.0);
        assert_eq!(facts.blocked, None);
        let href = facts.open_href.expect("an openable project opens");
        assert!(href.starts_with("/p/"), "{href}");
        assert!(href.ends_with("prj3fKq8Zr21bTxYw0AhVmDpe"), "{href}");
    }

    #[test]
    fn a_blocked_package_shows_its_headline_and_no_open_link() {
        let facts = row_facts(
            &UiPackageCard {
                health: PackageHealth::Blocked {
                    headline: "Format 3 — too old for this Studio".to_string(),
                    remedy: "Export a copy or delete it.".to_string(),
                },
                ..card()
            },
            0.0,
        );
        assert_eq!(
            facts.blocked.as_deref(),
            Some("Format 3 — too old for this Studio")
        );
        assert_eq!(facts.open_href, None);
    }

    fn card() -> UiPackageCard {
        UiPackageCard {
            uid: "prj3fKq8Zr21bTxYw0AhVmDpe".to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: "2026-07-02-0930-porch-sign".to_string(),
            last_saved_at: None,
            provenance: None,
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: None,
            health: PackageHealth::Ready,
        }
    }
}
