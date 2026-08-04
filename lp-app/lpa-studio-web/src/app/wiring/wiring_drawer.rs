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
//! The value box is a [`SlotPane`], so the detail popup
//! (`UiBusChannelView::visible_aspects`) stays one click away and every
//! site chip remains a focus affordance (D7: the UI feels linked).
//!
//! Geometry is deterministic: chips are fixed-height and ellipsize (they
//! never wrap), rows top-align, and the wires anchor at the value box's
//! header midline — so the connector SVGs render from site counts alone,
//! with no DOM measurement.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiBusChannelView, UiBusSiteOrigin, UiBusSiteView, UiBusView};

use crate::app::node::value_display::fixed_decimal_display;
use crate::app::node::{ProductPreview, SlotPane, SlotPaneTreatment};
use crate::base::StudioIconName;

/// Chip box height — shared by the chip class and the wire geometry.
const CHIP_H: f64 = 22.0;
/// Vertical gap inside a chip stack (`tw:gap-1.5`).
const CHIP_GAP: f64 = 6.0;
/// Connector gutter width.
const GUTTER_W: f64 = 36.0;
/// Wires plug into the value box at its header midline (header is
/// `tw:py-1` + one 12px text line ≈ 26px tall).
const HUB_Y: f64 = 13.0;
/// Arrowhead length.
const TIP: f64 = 6.0;

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
        div { class: "tw:grid tw:min-w-0",
            for (index , channel) in view.channels.into_iter().enumerate() {
                FlowChannelRow { channel, on_action, first: index == 0 }
            }
        }
    }
}

/// One channel's flow row: writer chips → wires → value box → wires →
/// reader chips.
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
    let writer_shadows: Vec<bool> = channel.writers.iter().map(|site| site.shadowed).collect();
    let reader_dotted: Vec<bool> = channel
        .readers
        .iter()
        .map(|site| site.child_scope.is_some())
        .collect();

    rsx! {
        div {
            class: "tw:grid tw:min-w-0 tw:items-start tw:gap-y-1 tw:py-2.5{divider} tw:grid-cols-[minmax(0,1fr)_36px_minmax(150px,200px)_36px_minmax(0,1fr)]",
            div { class: "tw:flex tw:min-w-0 tw:flex-col tw:items-end tw:gap-1.5 tw:pt-px",
                if channel.writers.is_empty() {
                    span { class: "tw:pt-1 tw:text-right tw:text-[10.5px] tw:italic tw:leading-snug tw:text-dim-foreground",
                        if channel.value_error.is_some() {
                            "no writer"
                        } else {
                            "no writer — authored default (R6)"
                        }
                    }
                } else {
                    for site in channel.writers.clone() {
                        BusSiteChip { site, on_action }
                    }
                }
            }
            WriterWires { shadows: writer_shadows, contended: channel.contended }
            SlotPane {
                title: channel.name.clone(),
                aspects,
                initially_open,
                treatment,
                title_icon: StudioIconName::Bus,
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
            ReaderWires { dotted: reader_dotted }
            div { class: "tw:flex tw:min-w-0 tw:flex-col tw:items-start tw:gap-1.5 tw:pt-px",
                if channel.readers.is_empty() {
                    span { class: "tw:pt-1 tw:text-[10.5px] tw:italic tw:leading-snug tw:text-dim-foreground",
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
fn BusSiteChip(site: UiBusSiteView, on_action: EventHandler<UiAction>) -> Element {
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
        "tw:inline-flex tw:h-[22px] tw:max-w-full tw:min-w-0 tw:flex-none tw:items-baseline \
         tw:gap-1 tw:rounded-xs tw:border tw:px-1.5 tw:leading-[20px]",
    );
    class.push_str(if panel_writer {
        " tw:border-status-attention-border tw:bg-status-attention-bg tw:text-status-attention-foreground"
    } else if publish {
        " tw:border-status-bound-border tw:bg-card-subtle tw:text-status-bound-foreground"
    } else {
        " tw:border-border-strong tw:bg-card-subtle tw:text-muted-foreground"
    });
    if origin == UiBusSiteOrigin::Default {
        class.push_str(" tw:border-dashed");
    }
    if child_scope.is_some() {
        class.push_str(" tw:border-dotted");
    }
    if shadowed {
        class.push_str(" tw:opacity-50");
    }
    let clickable = focus.is_some();
    if clickable {
        class.push_str(" tw:cursor-pointer tw:hover:border-selection-border tw:hover:text-foreground");
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

/// The writer-side connector gutter: one wire per chip converging on the
/// value box's header midline, plus a single arrowhead at the box edge.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WriterWires(shadows: Vec<bool>, contended: bool) -> Element {
    let count = shadows.len();
    if count == 0 {
        return rsx! {
            div {}
        };
    }
    let height = wire_gutter_height(count);
    let tip_x = GUTTER_W;
    let base_x = GUTTER_W - TIP;
    let arrow_color = if contended {
        "var(--studio-status-attention-border)"
    } else {
        "var(--studio-status-bound-border)"
    };
    rsx! {
        svg {
            class: "tw:flex-none",
            width: "{GUTTER_W}",
            height: "{height}",
            view_box: "0 0 {GUTTER_W} {height}",
            for (index , shadowed) in shadows.into_iter().enumerate() {
                path {
                    d: wire_path(0.0, chip_mid_y(index), base_x, HUB_Y),
                    fill: "none",
                    stroke: if contended { "var(--studio-status-attention-border)" } else if shadowed { "var(--studio-color-border-strong)" } else { "var(--studio-status-bound-border)" },
                    stroke_width: "1.5",
                    stroke_dasharray: if shadowed && !contended { "4 3" } else { "" },
                }
            }
            polygon {
                points: "{tip_x},{HUB_Y} {base_x - 1.0},{HUB_Y - 3.4} {base_x - 1.0},{HUB_Y + 3.4}",
                fill: "{arrow_color}",
            }
        }
    }
}

/// The reader-side connector gutter: wires fanning out from the box's
/// header midline, one arrowhead per reader chip. Child-scope readers
/// (dotted chips) get dashed wires.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ReaderWires(dotted: Vec<bool>) -> Element {
    let count = dotted.len();
    if count == 0 {
        return rsx! {
            div {}
        };
    }
    let height = wire_gutter_height(count);
    rsx! {
        svg {
            class: "tw:flex-none",
            width: "{GUTTER_W}",
            height: "{height}",
            view_box: "0 0 {GUTTER_W} {height}",
            for (index , dashed) in dotted.into_iter().enumerate() {
                path {
                    d: wire_path(0.0, HUB_Y, GUTTER_W - TIP, chip_mid_y(index)),
                    fill: "none",
                    stroke: "var(--studio-color-border-strong)",
                    stroke_width: "1.5",
                    stroke_dasharray: if dashed { "2 3" } else { "" },
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

/// Gutter height covering `count` chips (and never clipping the hub).
fn wire_gutter_height(count: usize) -> f64 {
    let stack = count as f64 * (CHIP_H + CHIP_GAP) - CHIP_GAP;
    stack.max(HUB_Y + 6.0)
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
