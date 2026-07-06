//! Shared detail-popover base: an icon trigger opening the standard detail card.

use dioxus::prelude::*;

use crate::base::{IconMenuButton, IconMenuTone, PopoverPlacement, StudioIconName};

/// Icon-triggered popover presenting the standard Studio "detail card": a
/// 320px-capped, sectioned surface. One base sits under every detail popup
/// (slot detail, project pending-edit popup, …) so the card chrome is never
/// copied per surface.
///
/// The base owns the trigger (via [`IconMenuButton`]), placement, and card
/// chrome; content is arbitrary, but section rows should use
/// [`detail_popover_section_class`] so dividers and status tints stay
/// consistent across consumers.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DetailPopover(
    icon: StudioIconName,
    label: String,
    #[props(default = label.clone())] title: String,
    #[props(default = IconMenuTone::Neutral)] tone: IconMenuTone,
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
    #[props(default = false)] active: bool,
    #[props(default = false)] initially_open: bool,
    children: Element,
) -> Element {
    rsx! {
        IconMenuButton {
            icon,
            label,
            title,
            tone,
            placement,
            active,
            initially_open,
            popup_class: detail_popover_card_class().to_string(),
            {children}
        }
    }
}

/// Status tint for one detail-card section row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DetailSectionTint {
    /// Plain section: divider and padding only.
    #[default]
    None,
    /// Good/bound (green) wash.
    Good,
    /// In-progress (working) wash.
    Working,
    /// Unsaved/edited (yellow) wash.
    Warning,
    /// Failed/invalid (red) wash.
    Error,
    /// Live/transient (blue) wash.
    Live,
}

/// Section styling hook: standard section padding and top divider, optionally
/// washed with a status tint gradient (the same left-edge treatment as
/// edited/live slot rows).
pub fn detail_popover_section_class(tint: DetailSectionTint) -> &'static str {
    match tint {
        DetailSectionTint::None => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Good => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-good-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Working => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-working-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Warning => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Error => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Live => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-live-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
    }
}

/// Card chrome for the detail-popover panel: capped width, sectioned grid
/// (no gap — sections carry their own padding), standard border and shadow.
fn detail_popover_card_class() -> &'static str {
    "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-0 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:text-sm tw:text-muted-foreground tw:shadow-lg"
}
