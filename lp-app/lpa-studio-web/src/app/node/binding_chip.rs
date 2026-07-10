//! Compact inline binding chip for slot and produced rows.

use dioxus::prelude::*;
use lpa_studio_core::UiBindingEndpoint;

use crate::base::{StudioIcon, StudioIconName};

/// Which way the row participates in the binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingChipDirection {
    /// The row's value is supplied by the endpoint.
    Consumes,
    /// The row's value is published to the endpoint.
    Publishes,
}

/// Small accent chip naming the bound endpoint inline on a row.
///
/// The full endpoint (with the `bus:` prefix) lives in the tooltip and the
/// detail popover; the chip shows the compact channel name so bound rows
/// read at a glance without dominating the row (M1 review gate may adjust).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BindingChip(endpoint: UiBindingEndpoint, direction: BindingChipDirection) -> Element {
    let compact = endpoint
        .label
        .strip_prefix("bus:")
        .unwrap_or(&endpoint.label)
        .to_string();
    let verb = match direction {
        BindingChipDirection::Consumes => "Bound from",
        BindingChipDirection::Publishes => "Publishes to",
    };
    let mut title = format!("{verb} {}", endpoint.label);
    if endpoint.default_origin {
        title.push_str(
            " — default binding (declared by the slot; authoring a binding overrides it)",
        );
    }

    rsx! {
        span {
            class: "tw:inline-flex tw:min-w-0 tw:max-w-40 tw:shrink tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:leading-none tw:text-status-bound-foreground",
            title,
            StudioIcon {
                name: StudioIconName::BoundValue,
                size: 11,
            }
            if direction == BindingChipDirection::Publishes {
                span { class: "tw:flex-none tw:text-[10px] tw:font-bold", "\u{2192}" }
            }
            code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[11px]", "{compact}" }
            if endpoint.default_origin {
                span {
                    class: "tw:flex-none tw:text-[9px] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                    "def"
                }
            }
        }
    }
}
