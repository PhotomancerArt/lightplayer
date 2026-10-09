//! The "Link" section of the connection bar's details
//! ([`UiDetailPanel::LinkCounters`]): the board's link counters off its
//! heartbeat as label/value rows, words, units and the caption from core
//! ([`link_counter_rows`], [`LINK_COUNTERS_CAPTION`]). A count the link had
//! to recover from wears the warning tone; a clean link reads as zeros.
//! Moved out of today's card's ⋯ menu unchanged.
//!
//! [`UiDetailPanel::LinkCounters`]: lpa_studio_core::UiDetailPanel::LinkCounters

use dioxus::prelude::*;
use lpa_studio_core::{DeviceLinkCounters, LINK_COUNTERS_CAPTION, link_counter_rows};

use crate::base::DetailSection;

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn LinkCountersSection(counters: DeviceLinkCounters) -> Element {
    let rows = link_counter_rows(&counters);
    rsx! {
        DetailSection { title: Some("Link".to_string()),
            dl { class: "tw:m-0 tw:grid tw:grid-cols-[auto_1fr] tw:gap-x-4 tw:gap-y-1 tw:text-xs tw:leading-snug",
                for row in rows {
                    // A `div` per pair is valid inside `dl`, and gives the
                    // pair one key; `contents` keeps the grid flat.
                    div { key: "{row.label}", class: "tw:contents",
                        dt { class: "tw:m-0 tw:text-muted-foreground", "{row.label}" }
                        dd { class: value_class(row.notable), "{row.value}" }
                    }
                }
            }
            p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                "{LINK_COUNTERS_CAPTION}"
            }
        }
    }
}

/// A counter's value: tabular, selectable, the card's strong ink — or the
/// warning tone for one the link had to recover from.
fn value_class(notable: bool) -> &'static str {
    match notable {
        true => "tw:m-0 tw:font-mono tw:tabular-nums tw:text-status-warning-foreground",
        false => "tw:m-0 tw:font-mono tw:tabular-nums tw:text-strong-foreground",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notable_count_wears_the_warning_tone() {
        assert!(value_class(true).contains("tw:text-status-warning-foreground"));
        assert!(!value_class(false).contains("status"));
    }
}
