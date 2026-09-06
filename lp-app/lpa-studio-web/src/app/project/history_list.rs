//! The document's **banked timeline**: history rows, newest first,
//! read-only — `vN · KIND · what · when` (relationship-control spike §4).
//!
//! Born as the project popover's History tab (D10); re-homed into the
//! header control's CHANGES popup by D14 — changes and history are one
//! temporal axis (the receipt "Save banks v13" and the timeline's "v12
//! saved" are the same ledger read from opposite ends), so the banked rows
//! render under the pending block rather than behind a separate tab. This
//! module is the rows alone; the popup owns the pending block above them.
//!
//! **Local history only.** The rows are whatever the handle backing this
//! session holds: for a visit that is the shared history the open
//! prefetched verbatim, for a library project the full log. Nothing is
//! fetched here, so a member's tab may show fewer rows than the service
//! knows — and it says nothing about the rest rather than claiming
//! completeness.
//!
//! **No restore.** Checkout is vision D6, parked: every row is text.

use dioxus::prelude::*;
use lpa_studio_core::UiProjectHistory;
use lpa_studio_core::core::time_ago::time_ago;

use crate::app::home::package_card::platform_now_secs;
use crate::app::share::relationship::ProjectRelationship;

/// The banked rows, or the honest empty line. No synthetic "editing" row —
/// in the merged changes panel the pending block directly above IS the
/// in-flight statement, so repeating it here would count the same work
/// twice.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HistoryList(
    relationship: ProjectRelationship,
    history: UiProjectHistory,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
) -> Element {
    if let Some(line) = history_empty_line(relationship, &history) {
        return rsx! {
            p { class: "tw:m-0 tw:px-0.5 tw:text-[10.5px] tw:leading-snug tw:text-dim-foreground",
                "{line}"
            }
        };
    }
    let now = now_secs.unwrap_or_else(platform_now_secs);

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            for (index , entry) in history.entries.iter().enumerate() {
                div { key: "{index}", class: "{HISTORY_ROW_CLASS} tw:border-0 tw:border-b tw:border-dashed tw:border-border-muted tw:last:border-b-0",
                    span { class: "{HISTORY_VERSION_CLASS} tw:text-muted-foreground",
                        if let Some(version) = entry.version {
                            "v{version}"
                        }
                    }
                    span { class: "{HISTORY_KIND_CLASS} tw:text-dim-foreground", "{entry.kind.word()}" }
                    span { class: "{HISTORY_WHAT_CLASS} tw:text-subtle-foreground", "{entry.label}" }
                    span { class: "tw:flex-none tw:text-[9.5px] tw:text-dim-foreground",
                        "{time_ago(now, entry.at)}"
                    }
                }
            }
        }
        p { class: "tw:m-0 tw:px-0.5 tw:pt-1 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
            "Read-only for now \u{2014} restore lands with the history effort."
        }
    }
}

/// The line the timeline shows INSTEAD of rows, when there are none to
/// show honestly.
///
/// An [`Example`](ProjectRelationship::Example) session always lands here, rows or
/// not: its history is the seed the transient open wrote (a provenance
/// origin plus an initial `Saved` of the bytes it opened), which is
/// bookkeeping, not something the person did. Saying "no history yet" is
/// the true statement, and it is the same sentence the timeline keeps after
/// they save a copy and the real rows begin.
pub fn history_empty_line(
    relationship: ProjectRelationship,
    history: &UiProjectHistory,
) -> Option<&'static str> {
    (relationship == ProjectRelationship::Example || history.entries.is_empty())
        .then_some("No history yet \u{2014} history begins at your first save.")
}

/// One history row's geometry, before its separator: four columns on a
/// shared baseline (spike §4 — `.histrow`).
const HISTORY_ROW_CLASS: &str =
    "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2 tw:px-0.5 tw:py-1.5 tw:text-[10.5px]";
/// The three column geometries; the event rows add the neutral family.
const HISTORY_VERSION_CLASS: &str = "tw:w-[30px] tw:flex-none tw:font-mono tw:font-bold";
const HISTORY_KIND_CLASS: &str =
    "tw:w-[54px] tw:flex-none tw:text-[8.5px] tw:font-bold tw:uppercase tw:tracking-wide";
const HISTORY_WHAT_CLASS: &str = "tw:min-w-0 tw:flex-1 tw:truncate";

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{UiHistoryKind, UiProjectHistoryEntry};

    fn one_save() -> UiProjectHistory {
        UiProjectHistory {
            entries: vec![UiProjectHistoryEntry {
                version: Some(1),
                kind: UiHistoryKind::Saved,
                label: String::new(),
                at: 1_000.0,
            }],
            next_version: Some(2),
        }
    }

    /// An example's rows are the transient open's own seed, not the
    /// person's history — so the timeline says so even when the projection
    /// handed it entries.
    #[test]
    fn an_example_keeps_the_empty_state_even_with_rows() {
        assert!(history_empty_line(ProjectRelationship::Example, &one_save()).is_some());
        assert!(
            history_empty_line(ProjectRelationship::Example, &UiProjectHistory::default())
                .is_some()
        );
    }

    /// Every other state renders whatever the local handle holds, and
    /// falls back to the same honest sentence when it holds nothing.
    #[test]
    fn the_other_states_render_the_rows_they_have() {
        for relationship in [
            ProjectRelationship::MineLocal,
            ProjectRelationship::MinePublished,
            ProjectRelationship::MemberOfSomeoneElses,
            ProjectRelationship::ViewingSomeoneElses,
        ] {
            assert!(
                history_empty_line(relationship, &one_save()).is_none(),
                "{relationship:?} hid rows it has"
            );
            assert!(
                history_empty_line(relationship, &UiProjectHistory::default()).is_some(),
                "{relationship:?} claimed rows it does not have"
            );
        }
    }
}
