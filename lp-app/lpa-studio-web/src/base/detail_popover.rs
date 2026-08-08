//! Shared detail-popover base: an icon trigger opening the standard detail card.

use dioxus::prelude::*;

use crate::base::{IconMenuButton, IconMenuTone, PopoverButton, PopoverPlacement, StudioIconName};

use super::icon_menu::icon_menu_chrome_class;

/// Popover presenting the standard Studio "detail card": a 320px-capped,
/// sectioned surface. One base sits under every detail popup (slot detail,
/// project pending-edit popup, …) so the card chrome is never copied per
/// surface.
///
/// The base owns the trigger, placement, and card chrome; content is
/// arbitrary, but section rows should use [`DetailSection`] (or
/// [`detail_popover_section_class`] for bespoke section markup) so dividers,
/// titles, and status tints stay consistent across consumers.
///
/// The trigger is the toned `icon` (via [`IconMenuButton`]) by default; a
/// custom `trigger` element (panel-control labels) replaces it while keeping
/// the identical toned card chrome, so both trigger shapes open the same
/// popup.
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
    /// Custom trigger content replacing the icon trigger (the panel
    /// control's label-as-trigger). Presentational only — the popover base
    /// still owns the button.
    #[props(default = None)]
    trigger: Option<Element>,
    /// Button class for the custom trigger at rest.
    #[props(default = String::new())]
    trigger_class: String,
    /// Button class for the custom trigger while open.
    #[props(default = String::new())]
    trigger_open_class: String,
    /// Anchored mode pass-through (see `PopoverButton`): the detail card
    /// merges its outline with an external anchor element — "diving into
    /// the control" (node-card P2c item 3).
    #[props(default = None)]
    anchor_id: Option<String>,
    /// Anchored mode pass-through (see `PopoverButton`).
    #[props(default = None)]
    anchor_visual: Option<Element>,
    /// Custom-trigger pass-through (see `PopoverButton`): the trigger holds
    /// icon plus label, so its top-layer copy keeps the trigger's own box.
    #[props(default = false)]
    layer_keeps_layout: bool,
    children: Element,
) -> Element {
    if let Some(trigger) = trigger {
        return rsx! {
            PopoverButton {
                class: trigger_class,
                open_class: trigger_open_class,
                trigger,
                label,
                title,
                popup_class: detail_popover_card_class().to_string(),
                chrome_class: icon_menu_chrome_class(tone).to_string(),
                placement,
                initially_open,
                anchor_id,
                anchor_visual,
                layer_keeps_layout,
                {children}
            }
        };
    }

    rsx! {
        IconMenuButton {
            icon,
            label,
            title,
            tone,
            placement,
            active,
            initially_open,
            anchor_id,
            anchor_visual,
            popup_class: detail_popover_card_class().to_string(),
            {children}
        }
    }
}

/// One section of the detail card: standard padding, top divider, optional
/// title row, optional status tint.
///
/// The tint convention (user, editing-polish item 2): **a section carrying an
/// affordance wears its color on the TITLE** — the title text takes the
/// tint's foreground token while an untinted section keeps the standard
/// heading color. The section surface keeps the same left-edge status wash
/// as edited/live slot rows ([`detail_popover_section_class`]).
///
/// `meta` is an optional right-aligned annotation on the title row (counts,
/// mostly), rendered in the same mono/muted style as detail value cells.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DetailSection(
    #[props(default)] title: Option<String>,
    #[props(default)] meta: Option<String>,
    #[props(default)] tint: DetailSectionTint,
    children: Element,
) -> Element {
    rsx! {
        section { class: detail_popover_section_class(tint),
            if title.is_some() || meta.is_some() {
                div { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                    if let Some(title) = title {
                        h3 { class: detail_section_title_class(tint), "{title}" }
                    }
                    if let Some(meta) = meta {
                        span { class: "tw:font-mono tw:text-xs tw:leading-snug tw:text-muted-foreground",
                            "{meta}"
                        }
                    }
                }
            }
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
    /// Good/valid (green) wash.
    Good,
    /// In-progress (working) wash.
    Working,
    /// Unsaved/edited (yellow) wash.
    Warning,
    /// Health-attention (orange) wash — device/roster surfaces.
    Attention,
    /// Failed/invalid (red) wash.
    Error,
    /// Live/transient (blue) wash.
    Live,
    /// **Debug** (D9): attention-orange + hazard stripes, the section wash
    /// for transient-by-nature diagnostics territory.
    Debug,
    /// Bound/bus-linked (violet) wash.
    Bound,
    /// **Export** (sage): the module-authoring family — "this ships from
    /// here" (module authoring unit D11). Never violet or good-green: an
    /// export is a structural property of the project, not a binding and
    /// not a health verdict.
    Export,
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
        DetailSectionTint::Attention => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-attention-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Error => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Live => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-live-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Debug => "lp-debug-detail-section",
        DetailSectionTint::Bound => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-bound-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
        DetailSectionTint::Export => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-export-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5 tw:first:border-t-0"
        }
    }
}

/// Title styling for a [`DetailSection`]: the standard section heading, with
/// the text color carrying the section's affordance tint (the "color on the
/// TITLE" convention) — untinted sections keep the heading token.
fn detail_section_title_class(tint: DetailSectionTint) -> &'static str {
    match tint {
        DetailSectionTint::None => "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading",
        DetailSectionTint::Good => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-good-foreground"
        }
        DetailSectionTint::Working => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-working-foreground"
        }
        DetailSectionTint::Warning => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-warning-foreground"
        }
        DetailSectionTint::Attention => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-attention-foreground"
        }
        DetailSectionTint::Error => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-error-foreground"
        }
        DetailSectionTint::Live => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-live-foreground"
        }
        DetailSectionTint::Debug => "tw:m-0 tw:text-xs tw:font-bold tw:uppercase lp-debug-title",
        DetailSectionTint::Bound => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-bound-foreground"
        }
        DetailSectionTint::Export => {
            "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-status-export-foreground"
        }
    }
}

/// Card chrome for the detail-popover panel: capped width, sectioned grid
/// (no gap — sections carry their own padding), standard border and shadow.
///
/// Public (P3 rich-object codification) so a surface that needs the
/// standard detail card behind a bespoke trigger or as a static panel
/// (story comparison sheets) never copies the class.
pub fn detail_popover_card_class() -> &'static str {
    "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-0 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:text-sm tw:text-muted-foreground tw:shadow-lg"
}
