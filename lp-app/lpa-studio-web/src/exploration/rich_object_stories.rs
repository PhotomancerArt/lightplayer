//! Rich-object pattern exploration (P2 of the design spike).
//!
//! EXPLORATION ONLY — nothing here rewires production surfaces; the live
//! gallery card, node pane, and their popovers are untouched. Each story
//! answers one of the design note's open questions
//! (`Planning/lp2025/2026-07-17-rich-object-pattern/01-design-note.md`),
//! so the sheet reads as a decision matrix:
//!
//! - `device-detail-popover` — the centerpiece: circle-as-trigger + all six
//!   device sections.
//! - `indicator-trigger-treatments` — Q1, circle-as-click-target variants.
//! - `card-affordance-placement` — Q2, button on the card vs popover-only.
//! - `pane-header` — Q3, the generalized header anatomy on two consumers.
//! - `section-ordering` — Q4, fixed schema order vs worst-first.
//! - `danger-zone-treatments` — Q5, inline tinted vs collapsed summary row.
//!
//! Every action here is inert (no-op): the stories explore presentation,
//! not flows. Sections follow the design note's device table; the Health
//! section's content comes from the real [`RosterCardState`] derivation so
//! the exploration can't drift from the card vocabulary.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::{RosterCardState, UiStatus};

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::device_card::circle_props;
use crate::app::layout::{PaneChip, PaneChrome, PaneTone, StudioPane};
use crate::base::{
    DetailPopover, DetailSection, DetailSectionTint, IconMenuTone, PopoverButton, PopoverPlacement,
    StatusCircle, StatusCircleShape, StatusCircleTone, StudioIcon, StudioIconName,
    detail_popover_card_class, node_kind_icon,
};
use crate::core::{
    StatusChip, menu_item_action_class, menu_item_destructive_action_class, quiet_action_class,
};

// COMPOSITION FINDINGS (P2), both codified in P3: the detail-card chrome
// (`base::detail_popover_card_class`) and the destructive menu-row class
// (`core::menu_item_destructive_action_class`) were copied here because
// they were private; they are exported now and this spike record consumes
// the exports. (Finding #1's other half — the card chrome behind a
// non-icon trigger — dissolved at the gate: Q1 chose the icon trigger.)

#[story(
    description = "The centerpiece: the card's status circle IS the popover trigger (no More-menu), open over a Running-behind device. All six sections from the design note's device table — Health, Project, Technical, Performance, Backup, Danger zone — each with its own lines and affordance where the table says so. Advisory facts (the firmware chip) tone a chip, never the circle. All buttons are inert; exploration only."
)]
fn device_detail_popover() -> Element {
    rsx! {
        div { class: "tw:min-h-[710px] tw:p-4",
            RichObjectCard {
                treatment: IndicatorTreatment::Bare,
                affordance_button: true,
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "Q1 — indicator-as-trigger treatments on the card, side by side: the bare 8px circle (an honest look at the tiny hit target), the circle with a hover ring + chevron (its hover state rendered statically), and a circle+glyph hybrid button that always announces clickability."
)]
fn indicator_trigger_treatments() -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-4 tw:p-4",
            StoryColumn { caption: "Bare circle — today's indicator, now a click target. 8px is a hard target to hit.",
                RichObjectCard { treatment: IndicatorTreatment::Bare }
            }
            StoryColumn { caption: "Hover ring + chevron (hover state shown): rest looks like the bare circle; hovering grows a ring and a chevron.",
                RichObjectCard { treatment: IndicatorTreatment::HoverRing }
            }
            StoryColumn { caption: "Circle + glyph hybrid: a standing bordered button, always announcing the popover.",
                RichObjectCard { treatment: IndicatorTreatment::Hybrid }
            }
        }
    }
}

#[story(
    description = "Q2 — where the primary affordance lives once the popover is one click away: the same Running-behind card with the rendered affordance button (today's direction-table rule) vs the affordance living only inside the popover (open it from the circle)."
)]
fn card_affordance_placement() -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-4 tw:p-4",
            StoryColumn { caption: "Rendered affordance button (today's rule): Push v5 sits on the card.",
                RichObjectCard {
                    treatment: IndicatorTreatment::Bare,
                    affordance_button: true,
                }
            }
            StoryColumn { caption: "Popover-only: the card stays quiet; Push v5 lives in the Health section behind the circle.",
                RichObjectCard {
                    treatment: IndicatorTreatment::Bare,
                    affordance_button: false,
                }
            }
        }
    }
}

#[story(
    description = "Q3 — the generalized pane header on two consumers, composing the shared StudioPane layout (node_pane/project_pane untouched): device content (circle indicator as the primary slot, USB kind, warning wash, 'Running v3 — behind' chip) and node content (kind-glyph select control, pencil detail trigger, unsaved wash)."
)]
fn pane_header() -> Element {
    rsx! {
        div { class: "tw:grid tw:max-w-xl tw:content-start tw:gap-4 tw:p-4",
            StoryColumn { caption: "Device consumer: the status circle rides the primary slot as the detail trigger; the rollup tone (Warning) washes the header.",
                StudioPane {
                    primary: rsx! {
                        CircleDetailTrigger {
                            treatment: IndicatorTreatment::Hybrid,
                            shape: StatusCircleShape::Solid,
                            tone: StatusCircleTone::Warning,
                            label: "Device details",
                            {device_identity_section()}
                            {rich_section(health_running_behind())}
                        }
                    },
                    title: "Luna's porch sign",
                    kind: "USB".to_string(),
                    chrome: PaneChrome {
                        tone: PaneTone::Warning,
                        accent: false,
                        chips: vec![PaneChip {
                            tone: PaneTone::Warning,
                            text: "Running v3 — behind".to_string(),
                            title: "Running v3 while your copy is at v5".to_string(),
                        }],
                    },
                }
            }
            StoryColumn { caption: "Node consumer: same anatomy, node chrome — the kind glyph in the primary slot, the pencil affordance as the detail trigger, the unsaved wash.",
                StudioPane {
                    primary: rsx! {
                        // The node select control, copied from NodeSelectButton
                        // (unfocused) so node_pane stays untouched.
                        button {
                            class: "tw:inline-flex tw:h-8 tw:w-8 tw:shrink-0 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-border-subtle tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:border-accent-border tw:hover:text-accent",
                            r#type: "button",
                            aria_label: "Select this node so probes follow it",
                            title: "Select this node so probes follow it",
                            StudioIcon { name: node_kind_icon("Shader"), size: 15 }
                        }
                    },
                    title: "blast",
                    kind: "Shader".to_string(),
                    chrome: PaneChrome {
                        tone: PaneTone::Warning,
                        accent: false,
                        chips: vec![PaneChip {
                            tone: PaneTone::Warning,
                            text: "2 unsaved".to_string(),
                            title: "Pending persisted edits".to_string(),
                        }],
                    },
                    detail: rsx! {
                        DetailPopover {
                            icon: StudioIconName::Edited,
                            label: "blast details",
                            tone: IconMenuTone::Warning,
                            active: true,
                            DetailSection {
                                title: "Unsaved (persisted)".to_string(),
                                meta: "2".to_string(),
                                tint: DetailSectionTint::Warning,
                                p { class: "tw:m-0 tw:py-1 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                                    "Shader body · Render order"
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[story(
    description = "Q4 — section ordering inside the popover: fixed schema order vs worst-first, on a device whose Health is Neutral while Project carries a Warning, so the orders differ visibly (worst-first floats Project to the top). The danger zone stays pinned last in both — Danger weight never sorts up or colors the rollup."
)]
fn section_ordering() -> Element {
    let fixed = ordering_demo_sections();
    let mut worst_first = ordering_demo_sections();
    worst_first.sort_by(|a, b| tint_severity(b.tint).cmp(&tint_severity(a.tint)));

    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-4 tw:p-4",
            StoryColumn { caption: "Fixed schema order: Health, Project, Technical, Performance, Backup — a stable map; the Warning sits mid-card.",
                StaticDetailCard {
                    {identity_section("Luna's porch sign", UiStatus::neutral("Connected"))}
                    for section in fixed {
                        {rich_section(section)}
                    }
                    {danger_zone(DangerTreatment::InlineTinted)}
                }
            }
            StoryColumn { caption: "Worst-first: the Warning Project section floats to the top; Neutral sections keep their schema order below it.",
                StaticDetailCard {
                    {identity_section("Luna's porch sign", UiStatus::neutral("Connected"))}
                    for section in worst_first {
                        {rich_section(section)}
                    }
                    {danger_zone(DangerTreatment::InlineTinted)}
                }
            }
        }
    }
}

#[story(
    description = "Q5 — the danger zone inside the popover, two ways: an inline red-tinted section behind a hard red separator (always visible, never shouting into the rollup) vs a collapsed summary row that keeps the destructive verbs one more click away."
)]
fn danger_zone_treatments() -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-4 tw:p-4",
            StoryColumn { caption: "Inline tinted: a hard red separator, the Error tint on the title, destructive rows in place.",
                StaticDetailCard {
                    {rich_section(backup_section())}
                    {danger_zone(DangerTreatment::InlineTinted)}
                }
            }
            StoryColumn { caption: "Collapsed summary row: one quiet row names the zone; the destructive verbs live behind it.",
                StaticDetailCard {
                    {rich_section(backup_section())}
                    {danger_zone(DangerTreatment::SummaryRow)}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Prototype surfaces (story-local; production components stay untouched)
// ---------------------------------------------------------------------------

/// Which circle-as-trigger treatment a card wears (Q1's variants).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndicatorTreatment {
    /// The 8px circle alone is the button.
    Bare,
    /// Circle in a pill with a chevron — rendered in its HOVER state so the
    /// capture shows the affordance (at rest it looks like `Bare`).
    HoverRing,
    /// A standing bordered button: circle + chevron glyph, always visible.
    Hybrid,
}

impl IndicatorTreatment {
    fn trigger_class(self) -> &'static str {
        match self {
            Self::Bare => {
                "tw:inline-flex tw:cursor-pointer tw:items-center tw:border-0 tw:bg-transparent tw:p-0"
            }
            Self::HoverRing => {
                "tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1 tw:rounded-pill tw:border tw:border-border-strong tw:bg-card-subtle tw:px-1.5 tw:py-1 tw:text-subtle-foreground"
            }
            Self::Hybrid => {
                "tw:inline-flex tw:h-6 tw:cursor-pointer tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-terminal tw:px-1.5 tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
            }
        }
    }

    fn open_class(self) -> &'static str {
        match self {
            Self::Bare => {
                "tw:inline-flex tw:cursor-pointer tw:items-center tw:border-0 tw:bg-transparent tw:p-0"
            }
            Self::HoverRing => {
                "tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1 tw:rounded-pill tw:border tw:border-border-strong tw:bg-card-subtle tw:px-1.5 tw:py-1 tw:text-strong-foreground"
            }
            Self::Hybrid => {
                "tw:inline-flex tw:h-6 tw:cursor-pointer tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:border-border-strong tw:bg-terminal tw:px-1.5 tw:text-strong-foreground"
            }
        }
    }

    fn shows_chevron(self) -> bool {
        matches!(self, Self::HoverRing | Self::Hybrid)
    }
}

/// The status circle as the detail-popover trigger — the pattern's
/// `RichIndicator` prototype. [`PopoverButton`] directly (not
/// [`DetailPopover`]) because the trigger is a circle, not an icon.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CircleDetailTrigger(
    treatment: IndicatorTreatment,
    shape: StatusCircleShape,
    tone: StatusCircleTone,
    label: String,
    #[props(default = false)] initially_open: bool,
    children: Element,
) -> Element {
    rsx! {
        PopoverButton {
            class: treatment.trigger_class().to_string(),
            open_class: treatment.open_class().to_string(),
            trigger: rsx! {
                StatusCircle { shape, tone }
                if treatment.shows_chevron() {
                    StudioIcon { name: StudioIconName::Expanded, size: 10 }
                }
            },
            label: label.clone(),
            title: label,
            popup_class: detail_popover_card_class().to_string(),
            // The rollup tone's popover chrome (the class name is
            // `icon_menu_chrome_class`'s private mapping).
            chrome_class: "ux-popover-chrome-warning".to_string(),
            placement: PopoverPlacement::BottomStart,
            initially_open,
            {children}
        }
    }
}

/// The rich-object device card prototype: the roster card's anatomy (classes
/// copied from `device_card.rs` so the production card stays untouched) with
/// the status circle replaced by the popover trigger and no More-menu — the
/// six-section popover replaces it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RichObjectCard(
    treatment: IndicatorTreatment,
    #[props(default = true)] affordance_button: bool,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let state = RosterCardState::RunningBehind {
        observed_version: Some(3),
        head_version: Some(5),
    };
    let (shape, tone) = circle_props(state.circle());
    let status_line = state.status_line(0.0);
    let swatch = thumb_swatch_style("prj_3fKq8Zr21bTxYw0A", false);

    rsx! {
        article { class: "tw:w-64 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            header { class: "tw:flex tw:items-center tw:gap-2 tw:border-b tw:border-border tw:bg-terminal tw:px-3 tw:py-2",
                CircleDetailTrigger {
                    treatment,
                    shape,
                    tone,
                    label: "Device details",
                    initially_open,
                    {device_identity_section()}
                    for section in running_behind_sections() {
                        {rich_section(section)}
                    }
                    {danger_zone(DangerTreatment::InlineTinted)}
                }
                span { class: "tw:inline-flex tw:items-center tw:text-muted-foreground",
                    StudioIcon { name: StudioIconName::Usb, size: 14 }
                }
                span { class: "tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-muted-foreground",
                    "USB"
                }
                span { class: "tw:ml-auto tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1.5",
                    span {
                        class: "tw:inline-block tw:h-3 tw:w-3 tw:flex-none tw:rounded-[3px]",
                        style: "{swatch}",
                    }
                    span { class: "tw:truncate tw:text-[11px] tw:text-muted-foreground", "porch-sign" }
                }
            }
            div { class: "tw:grid tw:gap-0.5 tw:p-3",
                p { class: "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground",
                    "Luna's porch sign"
                }
                p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-dim-foreground", "{status_line}" }
                if affordance_button {
                    div { class: "tw:mt-1",
                        InertQuietButton { icon: StudioIconName::Upload, label: "Push v5" }
                    }
                }
            }
        }
    }
}

/// The standard detail card as a static (always-open, non-floating) panel,
/// for side-by-side treatment comparisons.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StaticDetailCard(children: Element) -> Element {
    rsx! {
        div { class: detail_popover_card_class(), {children} }
    }
}

/// One story variant with its caption below.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryColumn(caption: String, children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-2",
            {children}
            p { class: "tw:m-0 tw:max-w-80 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                "{caption}"
            }
        }
    }
}

/// An inert quiet-chip button (exploration: presentation only, no action).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn InertQuietButton(icon: Option<StudioIconName>, label: String) -> Element {
    rsx! {
        button { class: quiet_action_class(), r#type: "button",
            if let Some(icon) = icon {
                span {
                    class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center",
                    aria_hidden: "true",
                    StudioIcon { name: icon, size: 14 }
                }
            }
            span { "{label}" }
        }
    }
}

// ---------------------------------------------------------------------------
// Section rendering (the RichSection prototype)
// ---------------------------------------------------------------------------

/// One section of the exploration's rich-object model: the design note's
/// `RichSection` reduced to what the stories need. `tint` carries the
/// section tone; the Danger weight is modeled separately ([`danger_zone`])
/// because it renders differently, never sorts, and never colors rollup.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RichSectionSpec {
    title: &'static str,
    tint: DetailSectionTint,
    rows: Vec<(&'static str, &'static str)>,
    /// A standing advisory chip: tones a chip, never the object indicator.
    chip: Option<&'static str>,
    /// The section's ≤1 affordance (inert here).
    affordance: Option<&'static str>,
}

/// Worst-first rank for a section tone (Q4). Danger is not ranked — it is
/// pinned last by construction.
fn tint_severity(tint: DetailSectionTint) -> u8 {
    match tint {
        DetailSectionTint::Error => 4,
        DetailSectionTint::Warning => 3,
        DetailSectionTint::Working | DetailSectionTint::Live | DetailSectionTint::Bound => 2,
        DetailSectionTint::Good => 1,
        DetailSectionTint::None => 0,
    }
}

fn rich_section(section: RichSectionSpec) -> Element {
    rsx! {
        DetailSection { title: section.title.to_string(), tint: section.tint,
            if !section.rows.is_empty() {
                dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-1.5 tw:py-1 tw:text-xs",
                    for (label , value) in section.rows {
                        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[88px_minmax(0,1fr)] tw:gap-2",
                            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                                "{label}"
                            }
                            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words",
                                "{value}"
                            }
                        }
                    }
                }
            }
            if let Some(chip) = section.chip {
                div { class: "tw:py-1",
                    StatusChip { status: UiStatus::warning(chip) }
                }
            }
            if let Some(affordance) = section.affordance {
                div { class: "tw:py-1",
                    InertQuietButton { icon: None, label: affordance }
                }
            }
        }
    }
}

/// The popover's identity header section (mirrors the node popover's).
fn identity_section(name: &'static str, status: UiStatus) -> Element {
    rsx! {
        DetailSection {
            div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4 tw:py-1",
                div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                    strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words",
                        "{name}"
                    }
                    span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "Device" }
                }
                StatusChip { status }
            }
        }
    }
}

fn device_identity_section() -> Element {
    identity_section("Luna's porch sign", UiStatus::warning("Running behind"))
}

/// Q5's two danger-zone presentations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DangerTreatment {
    /// Red-tinted section behind a hard red separator, rows in place.
    InlineTinted,
    /// One quiet summary row; the destructive verbs live behind it.
    SummaryRow,
}

fn danger_zone(treatment: DangerTreatment) -> Element {
    match treatment {
        DangerTreatment::InlineTinted => rsx! {
            // The wrapper's red border is the hard separator; the section's
            // own divider drops via its `first:` rule inside the wrapper.
            div { class: "tw:border-t tw:border-status-error-border",
                DetailSection { title: "Danger zone".to_string(), tint: DetailSectionTint::Error,
                    div { class: "tw:grid tw:py-1",
                        button { class: menu_item_destructive_action_class(), r#type: "button",
                            StudioIcon { name: StudioIconName::Apply, size: 14 }
                            span { "Flash firmware…" }
                        }
                        button { class: menu_item_destructive_action_class(), r#type: "button",
                            StudioIcon { name: StudioIconName::Remove, size: 14 }
                            span { "Erase device…" }
                        }
                    }
                }
            }
        },
        DangerTreatment::SummaryRow => rsx! {
            DetailSection {
                button { class: menu_item_action_class(), r#type: "button",
                    span { class: "tw:flex-1 tw:text-left", "Danger zone" }
                    span { class: "tw:text-xs tw:text-dim-foreground", "Flash firmware · Erase" }
                    StudioIcon { name: StudioIconName::Collapsed, size: 12 }
                }
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Fake device data (realistic, fixed — never live)
// ---------------------------------------------------------------------------

/// The Health section for the centerpiece's Running-behind device. Its
/// status line comes from the REAL card derivation ([`RosterCardState`]) —
/// the Health section IS today's card state, per the design note.
fn health_running_behind() -> RichSectionSpec {
    RichSectionSpec {
        title: "Health",
        tint: DetailSectionTint::Warning,
        rows: vec![
            ("status", "Running v3 — behind your copy"),
            ("link", "replied 12 ms ago · retry ladder idle"),
        ],
        chip: None,
        affordance: Some("Push v5"),
    }
}

/// All six-minus-danger sections for the Running-behind centerpiece, in the
/// design note's schema order.
fn running_behind_sections() -> Vec<RichSectionSpec> {
    vec![
        health_running_behind(),
        RichSectionSpec {
            title: "Project",
            tint: DetailSectionTint::Warning,
            rows: vec![
                ("running", "porch-sign · v3"),
                ("your copy", "v5 · 2 versions ahead"),
                ("banked", "device copy saved to history"),
            ],
            chip: None,
            affordance: Some("Open project"),
        },
        RichSectionSpec {
            title: "Technical",
            tint: DetailSectionTint::None,
            rows: vec![
                ("board", "ESP32-C6"),
                ("uid", "dev_7pQr5St89uVwXy2C"),
                ("transport", "USB · Web Serial"),
                ("firmware", "fw-esp32 @ 9f31c2a · release-esp32"),
            ],
            chip: Some("Firmware update available"),
            affordance: None,
        },
        RichSectionSpec {
            title: "Performance",
            tint: DetailSectionTint::None,
            rows: vec![
                ("frame rate", "62 fps"),
                ("frame Δ", "3.1 ms"),
                ("memory", "118 KB free heap"),
            ],
            chip: None,
            affordance: None,
        },
        backup_section(),
    ]
}

fn backup_section() -> RichSectionSpec {
    RichSectionSpec {
        title: "Backup",
        tint: DetailSectionTint::None,
        rows: vec![
            ("last banked", "v3 + device edits"),
            ("when", "2 days ago · at connect"),
        ],
        chip: None,
        affordance: Some("Download copy"),
    }
}

/// Q4's dataset: Neutral Health, Warning in Project — the one arrangement
/// where fixed and worst-first orders visibly differ.
fn ordering_demo_sections() -> Vec<RichSectionSpec> {
    vec![
        RichSectionSpec {
            title: "Health",
            tint: DetailSectionTint::None,
            rows: vec![("status", "Connected"), ("link", "replied 9 ms ago")],
            chip: None,
            affordance: None,
        },
        RichSectionSpec {
            title: "Project",
            tint: DetailSectionTint::Warning,
            rows: vec![
                ("holds", "porch-sign · v3"),
                ("your copy", "v5 · 2 versions ahead"),
            ],
            chip: None,
            affordance: Some("Push v5"),
        },
        RichSectionSpec {
            title: "Technical",
            tint: DetailSectionTint::None,
            rows: vec![("board", "ESP32-C6"), ("transport", "USB · Web Serial")],
            chip: None,
            affordance: None,
        },
        RichSectionSpec {
            title: "Performance",
            tint: DetailSectionTint::None,
            rows: vec![("frame rate", "62 fps"), ("memory", "118 KB free heap")],
            chip: None,
            affordance: None,
        },
        backup_section(),
    ]
}
