//! Wiring drawer body: the **flow view** — one row per channel reading
//! `[writers] → [value box] → [readers]`.
//!
//! The 2026-08-03 wiring-UI spike (`spikes/wiring-ui/index.html`, gates
//! G1–G3) replaced the relocated bus-pane value-hero rows: bus-as-
//! writers/readers is now drawn, not hidden in a popup. Writer chips
//! enter from the left on arrowed wires, the channel's value box is the
//! junction (name in the violet header, *what's on the channel* as the
//! body — a picture for visual products, fixed decimals + a position bar
//! for unit floats), and reader chips fan out right. Wire weight carries
//! resolution: the winning writer solid violet, shadowed writers dim and
//! dashed (R5/R11), E3 contention turns the writer side attention-orange
//! with a "2× fallback" badge (display only — the pick gesture is future
//! work, modules.md §5). Child-scope readers list as dotted chips with
//! their scope path (R5 inheritance; spike gate 3).
//!
//! **Narrow containers stack vertically** (`@container` query on the
//! drawer, below the `@md` width): writers become a tree — a trunk with
//! elbow branches to each chip and an arrowhead dropping into the value
//! box — and readers branch off a matching trunk below it. Same DOM,
//! two layouts; the side gutters and the tree rails hide each other.
//!
//! The value box is a [`SlotPane`], so the detail popup
//! (`UiBusChannelView::visible_aspects`) stays one click away and every
//! site chip remains a focus affordance (D7: the UI feels linked).
//!
//! Geometry is deterministic: chips are fixed-height and ellipsize (they
//! never wrap), and in the wide layout every row cell centers on the
//! row's vertical axis — the connector SVG is exactly as tall as its
//! chip stack, so chip midlines and the box's side midpoint (= the SVG's
//! own vertical center) are knowable from site counts alone, with no DOM
//! measurement. The tree rails derive from the same constants.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiBusChannelView, UiBusSiteOrigin, UiBusSiteView, UiBusView};

use crate::app::node::value_display::fixed_decimal_display;
use crate::app::node::{
    GradientDisplayDensity, GradientValueDisplay, ProductPreview, SlotPane, SlotPaneTreatment,
};

/// A writer wire's tone — always matching its chip, so the drawing needs
/// no legend: orange = engaged panel writer, violet = a node write that
/// is driving the channel, grey = out-ranked (and every reader).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriterTone {
    Panel,
    Active,
    Shadowed,
}

impl WriterTone {
    fn of(site: &UiBusSiteView) -> Self {
        if site.origin == UiBusSiteOrigin::Panel {
            Self::Panel
        } else if site.shadowed {
            Self::Shadowed
        } else {
            Self::Active
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::Panel => "var(--studio-status-attention-border)",
            Self::Active => "var(--studio-status-bound-border)",
            Self::Shadowed => "var(--studio-color-border-strong)",
        }
    }
}

/// Chip box height — shared by the chip class and the wire geometry.
const CHIP_H: f64 = 22.0;
/// Vertical gap inside a chip stack (`tw:gap-1.5`).
const CHIP_GAP: f64 = 6.0;
/// Connector gutter width (wide layout).
const GUTTER_W: f64 = 36.0;
/// Arrowhead length (wide layout).
const TIP: f64 = 6.0;
/// Tree rail width (stacked layout): trunk at x=6, elbows reach the chips.
const RAIL_W: f64 = 18.0;
/// Trunk x position inside a tree rail.
const RAIL_X: f64 = 6.0;
/// Reader tree lead-in: the trunk drops this far below the value box
/// before the first branch, so the box and the consumer chips breathe.
const READER_LEAD: f64 = 10.0;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn WiringDrawerBody(view: UiBusView, on_action: EventHandler<UiAction>) -> Element {
    if view.channels.is_empty() {
        return rsx! {
            div { class: "tw:grid tw:gap-1 tw:text-sm tw:text-muted-foreground",
                p { class: "tw:m-0", "No bus channels yet." }
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-subtle-foreground",
                    "The bus is the project's patch bay: nodes publish and consume "
                    "values on named channels. Bind a slot to "
                    code { class: "tw:font-mono", "bus:…" }
                    " and the channel appears here."
                }
            }
        };
    }

    rsx! {
        // `@container`: rows morph on the DRAWER's own width, not the
        // viewport — a narrow card stacks vertically wherever it lives.
        div { class: "tw:@container tw:grid tw:min-w-0",
            for (index , channel) in view.channels.into_iter().enumerate() {
                FlowChannelRow { channel, on_action, first: index == 0 }
            }
            WireKey {}
        }
    }
}

/// The one-line key: wire/chip color says who drives the channel.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WireKey() -> Element {
    rsx! {
        div { class: "tw:mt-1 tw:flex tw:flex-wrap tw:items-center tw:gap-x-3.5 tw:gap-y-1 tw:border-t tw:border-border-muted tw:pt-2",
            KeyItem { color: "var(--studio-status-bound-border)", label: "driving write" }
            KeyItem { color: "var(--studio-status-attention-border)", label: "engaged panel" }
            KeyItem { color: "var(--studio-color-border-strong)", label: "reader / out-ranked" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn KeyItem(color: &'static str, label: &'static str) -> Element {
    rsx! {
        span { class: "tw:inline-flex tw:items-center tw:gap-1.5",
            svg {
                width: "18",
                height: "8",
                view_box: "0 0 18 8",
                path {
                    d: "M 0 4 L 12 4",
                    fill: "none",
                    stroke: "{color}",
                    stroke_width: "1.5",
                }
                polygon { points: "18,4 12,1 12,7", fill: "{color}" }
            }
            span { class: "tw:text-[10px] tw:text-dim-foreground", "{label}" }
        }
    }
}

/// One channel's flow row: writer chips → wires → value box → wires →
/// reader chips (wide), or the writer tree above the box and the reader
/// tree below it (stacked).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn FlowChannelRow(
    channel: UiBusChannelView,
    on_action: EventHandler<UiAction>,
    /// Open the value box's detail popup on first render (stories).
    #[props(default = false)]
    initially_open: bool,
    /// Suppress the divider above the first row.
    #[props(default = false)]
    first: bool,
) -> Element {
    let aspects = channel.visible_aspects();
    let treatment = if channel.contended {
        SlotPaneTreatment::Attention
    } else {
        SlotPaneTreatment::Bound
    };
    let divider = if first {
        ""
    } else {
        " tw:border-t tw:border-border-muted"
    };
    let writer_tones: Vec<WriterTone> = channel.writers.iter().map(WriterTone::of).collect();
    let reader_count = channel.readers.len();

    rsx! {
        div {
            class: "tw:flex tw:min-w-0 tw:flex-col tw:gap-1 tw:py-2.5{divider} tw:@md:grid tw:@md:items-center tw:@md:gap-y-1 tw:@md:grid-cols-[minmax(0,1fr)_36px_minmax(150px,224px)_36px_minmax(0,1fr)]",
            // Writer cell: tree rail beside a left-aligned chip column when
            // stacked; right-aligned column beside the wire gutter at @md.
            div { class: "tw:flex tw:min-w-0 tw:items-stretch tw:-mb-1 tw:@md:mb-0 tw:@md:w-full tw:@md:justify-end",
                WriterTreeRail { tones: writer_tones.clone() }
                div { class: "tw:flex tw:min-w-0 tw:flex-col tw:items-start tw:gap-1.5 tw:@md:items-end",
                    if channel.writers.is_empty() {
                        span { class: "tw:text-[10.5px] tw:italic tw:leading-snug tw:text-dim-foreground tw:@md:text-right",
                            if channel.value_error.is_some() {
                                "no writer"
                            } else {
                                "no writer — authored default (R6)"
                            }
                        }
                    } else {
                        for site in channel.writers.clone() {
                            BusSiteChip {
                                active_writer: WriterTone::of(&site) == WriterTone::Active,
                                site,
                                on_action,
                            }
                        }
                    }
                }
            }
            WriterWires { tones: writer_tones }
            div { class: "tw:w-full tw:min-w-0 tw:@md:w-auto",
                SlotPane {
                    title: channel.name.clone(),
                    aspects,
                    initially_open,
                    treatment,
                    on_action,
                    badges: rsx! {
                        if channel.primary_visual {
                            span {
                                class: "tw:flex-none tw:rounded-xs tw:bg-status-bound-bg tw:px-1 tw:text-[9px] tw:font-bold tw:uppercase tw:leading-snug tw:text-status-bound-foreground",
                                title: "The project's primary visual output",
                                "primary"
                            }
                        }
                        if let Some(kind) = channel.kind.clone() {
                            span { class: "tw:flex-none tw:text-[10px] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{kind}" }
                        }
                    },
                    ChannelValueBody { channel: channel.clone() }
                }
            }
            ReaderWires { count: reader_count }
            // Reader cell: mirrored tree below the box when stacked.
            div { class: "tw:flex tw:min-w-0 tw:items-stretch tw:-mt-1 tw:@md:mt-0 tw:@md:w-full",
                ReaderTreeRail { count: reader_count }
                div { class: "tw:flex tw:min-w-0 tw:flex-col tw:items-start tw:gap-1.5 tw:pt-2.5 tw:@md:pt-0",
                    if channel.readers.is_empty() {
                        span { class: "tw:text-[10.5px] tw:italic tw:leading-snug tw:text-dim-foreground",
                            "no readers"
                        }
                    } else {
                        for site in channel.readers.clone() {
                            BusSiteChip { site, on_action }
                        }
                    }
                }
            }
        }
    }
}

/// The value box body: *what's on the channel*.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChannelValueBody(channel: UiBusChannelView) -> Element {
    rsx! {
        if let Some(preview) = channel.preview.clone() {
            div { class: "tw:grid tw:w-full tw:min-w-0 tw:justify-items-center tw:gap-1",
                // Capped so a square probe frame stays a thumbnail, not a
                // hero — the value box is small by decision (spike G2).
                div { class: "tw:w-full tw:max-w-[120px]",
                    ProductPreview {
                        kind: preview.kind,
                        preview: preview.preview,
                        tracking: preview.tracking,
                        frame: preview.frame,
                        focus_action: None,
                    }
                }
                if let Some(value) = channel.value.clone() {
                    code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[10px] tw:text-subtle-foreground",
                        "{value}"
                    }
                }
                ContentionBadge { contended: channel.contended }
            }
        } else if let Some(config) = channel.gradient.clone() {
            // A palette on the channel is shown, not spelled — the same
            // rule the product preview above follows.
            div { class: "tw:grid tw:w-full tw:min-w-0 tw:justify-items-center tw:gap-1",
                div { class: "tw:w-full tw:max-w-[120px]",
                    GradientValueDisplay {
                        config,
                        density: GradientDisplayDensity::Compact,
                    }
                }
                ContentionBadge { contended: channel.contended }
            }
        } else if let Some(value) = channel.value.clone() {
            div { class: "tw:grid tw:min-w-0 tw:justify-items-center tw:gap-1",
                code { class: "tw:min-w-0 tw:break-all tw:text-center tw:font-mono tw:text-sm tw:font-bold tw:text-strong-foreground",
                    {fixed_decimal_display(&value, None)}
                }
                // A position bar for unit-interval floats only — never a
                // fake range for other values.
                if let Some(fraction) = unit_fraction(&channel) {
                    div { class: "tw:relative tw:h-[3px] tw:w-14 tw:overflow-hidden tw:rounded-full tw:border tw:border-border-muted tw:bg-track",
                        div {
                            class: "tw:absolute tw:inset-y-0 tw:left-0 tw:bg-status-live-foreground tw:opacity-75",
                            style: "width: {(fraction * 100.0).round()}%",
                        }
                    }
                }
                ContentionBadge { contended: channel.contended }
            }
        } else if let Some(error) = channel.value_error.clone() {
            span {
                class: "tw:min-w-0 tw:truncate tw:text-xs tw:text-status-error-foreground",
                title: "{error}",
                "unresolved"
            }
        } else if channel.writers.is_empty() {
            // R6 invitation: nothing wrote the channel; consumers fall back
            // to their authored defaults, and the channel still lists.
            span { class: "tw:text-[11px] tw:italic tw:text-dim-foreground", "authored defaults" }
        } else {
            span { class: "tw:text-xs tw:text-subtle-foreground", "\u{2014}" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ContentionBadge(contended: bool) -> Element {
    rsx! {
        if contended {
            span {
                class: "tw:rounded-xs tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-1 tw:text-[9px] tw:font-bold tw:uppercase tw:leading-snug tw:text-status-attention-foreground",
                title: "Two writers tie at fallback priority — ambiguous until the author picks (E3). The pick gesture is future work.",
                "2\u{d7} fallback"
            }
        }
    }
}

/// One writer/reader site as a clickable chip. Flavor is worn on the
/// border: dashed = default-origin, dotted = child-scope reader (R5),
/// attention = engaged panel writer (R10), violet = module publish (R7),
/// half-faded = shadowed (R11).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BusSiteChip(
    site: UiBusSiteView,
    on_action: EventHandler<UiAction>,
    /// Writer that is actually driving the channel — worn violet so the
    /// chip matches its wire.
    #[props(default = false)]
    active_writer: bool,
) -> Element {
    let UiBusSiteView {
        node_label,
        slot,
        origin,
        publish,
        shadowed,
        child_scope,
        focus,
    } = site;
    let panel_writer = origin == UiBusSiteOrigin::Panel;

    // NOTE: font-size classes live on the inner spans, not the button —
    // style.css's unlayered `button { font: inherit }` beats layered
    // Tailwind utilities on the element itself.
    let mut class = String::from(
        "tw:inline-flex tw:h-[22px] tw:max-w-full tw:min-w-0 tw:flex-none tw:items-center \
         tw:gap-1 tw:rounded-xs tw:border tw:px-1.5",
    );
    class.push_str(if panel_writer {
        " tw:border-status-attention-border tw:bg-status-attention-bg tw:text-status-attention-foreground"
    } else if publish || active_writer {
        " tw:border-status-bound-border tw:bg-card-subtle tw:text-status-bound-foreground"
    } else {
        " tw:border-border-strong tw:bg-card-subtle tw:text-muted-foreground"
    });
    if shadowed {
        class.push_str(" tw:opacity-50");
    }
    let clickable = focus.is_some();
    if clickable {
        class.push_str(
            " tw:cursor-pointer tw:hover:border-selection-border tw:hover:text-foreground",
        );
    }

    // One flavor mark at most (plus "shadowed" when it applies) — the
    // node label owns the space; the popup spells out the rest.
    let flavor = if publish {
        Some("publish")
    } else if child_scope.is_some() {
        Some("child scope")
    } else if origin == UiBusSiteOrigin::Default {
        Some("default")
    } else {
        None
    };
    let mut marks: Vec<&str> = flavor.into_iter().collect();
    if shadowed {
        marks.push("shadowed");
    }
    let marks = (!marks.is_empty()).then(|| format!("({})", marks.join(", ")));
    let title = if panel_writer {
        "Engaged panel writer — unauthored runtime state (R10)"
    } else if child_scope.is_some() {
        "Lives in a child module's scope; its input has no writer there, so it resolves to this channel (R5)"
    } else if publish {
        "Module publish (R7)"
    } else if origin == UiBusSiteOrigin::Default {
        "Default binding"
    } else if clickable {
        "Jump to this node"
    } else {
        ""
    };
    let label = if panel_writer {
        "panel \u{b7} engaged".to_string()
    } else {
        node_label
    };

    rsx! {
        button {
            class,
            title: "{title}",
            disabled: !clickable,
            onclick: move |_| {
                if let Some(focus) = focus.clone() {
                    on_action.call(focus);
                }
            },
            if let Some(scope) = child_scope {
                span { class: "tw:flex-none tw:text-[10px] tw:text-dim-foreground", "{scope} \u{b7}" }
            }
            span { class: "tw:min-w-0 tw:truncate tw:text-[11px]", "{label}" }
            if let Some(slot) = slot.filter(|_| !panel_writer) {
                span { class: "tw:flex-none tw:font-mono tw:text-[10px] tw:text-dim-foreground", ".{slot}" }
            }
            if let Some(marks) = marks {
                span { class: "tw:flex-none tw:text-[9.5px] tw:text-dim-foreground", "{marks}" }
            }
        }
    }
}

/// Stacked layout: the writer tree — a trunk with an elbow branch to
/// each chip and an arrowhead dropping toward the value box below:
///
/// ```text
///  |- writer-1
///  |- writer-2
///  v
/// ```
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WriterTreeRail(tones: Vec<WriterTone>) -> Element {
    let count = tones.len();
    if count == 0 {
        return rsx! {
            div { class: "tw:hidden" }
        };
    }
    let stack = stack_height(count);
    let height = stack + 12.0;
    let first_mid = CHIP_H / 2.0;
    let trunk_end = height - 5.0;
    // The trunk and arrow wear the top-priority writer's tone (providers
    // arrive winner-first), so what enters the box matches who drives it.
    let trunk = tones[0].color();
    rsx! {
        svg {
            class: "tw:flex-none tw:@md:hidden",
            width: "{RAIL_W}",
            height: "{height}",
            view_box: "0 0 {RAIL_W} {height}",
            path {
                d: "M {RAIL_X} {first_mid} L {RAIL_X} {trunk_end}",
                fill: "none",
                stroke: "{trunk}",
                stroke_width: "1.5",
            }
            for (index , tone) in tones.into_iter().enumerate() {
                path {
                    d: "M {RAIL_X} {chip_mid_y(index)} L {RAIL_W} {chip_mid_y(index)}",
                    fill: "none",
                    stroke: "{tone.color()}",
                    stroke_width: "1.5",
                }
            }
            polygon {
                points: "{RAIL_X},{height} {RAIL_X - 3.4},{height - 6.0} {RAIL_X + 3.4},{height - 6.0}",
                fill: "{trunk}",
            }
        }
    }
}

/// Stacked layout: the reader tree below the value box — a trunk the
/// consumers branch off:
///
/// ```text
///  +- reader-1
///  +- reader-2
/// ```
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ReaderTreeRail(count: usize) -> Element {
    if count == 0 {
        return rsx! {
            div { class: "tw:hidden" }
        };
    }
    let height = stack_height(count) + READER_LEAD;
    let last_mid = READER_LEAD + chip_mid_y(count - 1);
    rsx! {
        svg {
            class: "tw:flex-none tw:@md:hidden",
            width: "{RAIL_W}",
            height: "{height}",
            view_box: "0 0 {RAIL_W} {height}",
            path {
                d: "M {RAIL_X} 0 L {RAIL_X} {last_mid}",
                fill: "none",
                stroke: "var(--studio-color-border-strong)",
                stroke_width: "1.5",
            }
            for index in 0..count {
                path {
                    d: "M {RAIL_X} {READER_LEAD + chip_mid_y(index)} L {RAIL_W - 5.0} {READER_LEAD + chip_mid_y(index)}",
                    fill: "none",
                    stroke: "var(--studio-color-border-strong)",
                    stroke_width: "1.5",
                }
                polygon {
                    points: "{RAIL_W},{READER_LEAD + chip_mid_y(index)} {RAIL_W - 5.5},{READER_LEAD + chip_mid_y(index) - 3.2} {RAIL_W - 5.5},{READER_LEAD + chip_mid_y(index) + 3.2}",
                    fill: "var(--studio-color-border-strong)",
                }
            }
        }
    }
}

/// Wide layout: the writer-side connector gutter — one wire per chip
/// converging on the value box's side midpoint, plus a single arrowhead
/// at the box edge.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WriterWires(tones: Vec<WriterTone>) -> Element {
    let count = tones.len();
    if count == 0 {
        return rsx! {
            div { class: "tw:hidden tw:@md:block" }
        };
    }
    let height = wire_gutter_height(count);
    // Both the stack and this SVG are centered on the row axis, so the
    // box's side midpoint is simply the SVG's own vertical center.
    let hub_y = height / 2.0;
    let tip_x = GUTTER_W;
    let base_x = GUTTER_W - TIP;
    // The arrowhead wears the top-priority writer's tone, matching the
    // wire that actually drives the channel.
    let arrow_color = tones[0].color();
    rsx! {
        svg {
            class: "tw:hidden tw:flex-none tw:self-center tw:@md:block",
            width: "{GUTTER_W}",
            height: "{height}",
            view_box: "0 0 {GUTTER_W} {height}",
            for (index , tone) in tones.into_iter().enumerate() {
                path {
                    d: wire_path(0.0, chip_mid_y(index), base_x, hub_y),
                    fill: "none",
                    stroke: "{tone.color()}",
                    stroke_width: "1.5",
                }
            }
            polygon {
                points: "{tip_x},{hub_y} {base_x - 1.0},{hub_y - 3.4} {base_x - 1.0},{hub_y + 3.4}",
                fill: "{arrow_color}",
            }
        }
    }
}

/// Wide layout: the reader-side connector gutter — wires fanning out
/// from the box's side midpoint, one arrowhead per reader chip.
/// Child-scope readers (dotted chips) get dashed wires.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ReaderWires(count: usize) -> Element {
    if count == 0 {
        return rsx! {
            div { class: "tw:hidden tw:@md:block" }
        };
    }
    let height = wire_gutter_height(count);
    let hub_y = height / 2.0;
    rsx! {
        svg {
            class: "tw:hidden tw:flex-none tw:self-center tw:@md:block",
            width: "{GUTTER_W}",
            height: "{height}",
            view_box: "0 0 {GUTTER_W} {height}",
            for index in 0..count {
                path {
                    d: wire_path(0.0, hub_y, GUTTER_W - TIP, chip_mid_y(index)),
                    fill: "none",
                    stroke: "var(--studio-color-border-strong)",
                    stroke_width: "1.5",
                }
                polygon {
                    points: "{GUTTER_W},{chip_mid_y(index)} {GUTTER_W - TIP - 1.0},{chip_mid_y(index) - 3.4} {GUTTER_W - TIP - 1.0},{chip_mid_y(index) + 3.4}",
                    fill: "var(--studio-color-border-strong)",
                }
            }
        }
    }
}

/// Center line of the `index`-th chip in a top-aligned stack.
fn chip_mid_y(index: usize) -> f64 {
    index as f64 * (CHIP_H + CHIP_GAP) + CHIP_H / 2.0
}

/// Height of a `count`-chip stack.
fn stack_height(count: usize) -> f64 {
    (count as f64 * (CHIP_H + CHIP_GAP) - CHIP_GAP).max(CHIP_H)
}

/// Gutter height: exactly the chip stack's height, so centering the SVG
/// and the stack on the same row axis keeps their coordinates aligned.
fn wire_gutter_height(count: usize) -> f64 {
    stack_height(count)
}

/// A horizontal-tangent cubic between two points — the wire look.
fn wire_path(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    let dx = ((x1 - x0) * 0.5).max(10.0);
    format!(
        "M {x0} {y0} C {cx0} {y0}, {cx1} {y1}, {x1} {y1}",
        cx0 = x0 + dx,
        cx1 = x1 - dx,
    )
}

/// `Some(value)` when the channel's display value is a unit-interval
/// float (the only case a position bar is honest). Instants tick past 1
/// and get no bar; so do counts, colors, and structured values.
fn unit_fraction(channel: &UiBusChannelView) -> Option<f64> {
    let value: f64 = channel.value.as_deref()?.parse().ok()?;
    (0.0..=1.0).contains(&value).then_some(value)
}
