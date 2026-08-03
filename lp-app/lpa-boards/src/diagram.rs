//! `BoardDiagram`: the row-engine SVG renderer as a Dioxus component.
//!
//! All geometry comes from [`crate::geometry::BoardLayout`]; this module only
//! walks the computed layout and emits SVG. Colors ride `lpb-*` CSS classes
//! (studio palette; the class block lives in the consuming app's stylesheet —
//! see `lpa-studio-web/src/style.css`), so the same component serves the
//! catalog, provisioning picker, hardware pane, and discovery UI.

use dioxus::prelude::*;

use crate::callout::BoardCallout;
use crate::display_manifest::BoardDisplayFile;
use crate::geometry::{
    BoardLayout, CellBody, CellLayout, DiagramMode, DiagramOptions, PinSwatch, RowLabel,
    WiredConnection, pad_css_suffix,
};

/// Callout label size, matching `.lpb-anat-label`'s `font-size`. The
/// engine is deliberately DOM-free, so text width is ESTIMATED from the
/// character count with the same per-char factor the row engine uses for
/// cells — a fixed budget clipped long instructions, which is exactly the
/// failure a derived margin exists to prevent.
const CALLOUT_FONT: f32 = 9.0;
/// Arrowhead length and half-width, in drawing units.
const ARROW_LEN: f32 = 7.0;
const ARROW_HALF_W: f32 = 3.2;
const CALLOUT_CHAR_W: f32 = 0.72;

/// Estimated on-screen width of a callout's RENDERED label, in drawing
/// units — the step lead-in included, because it is drawn too. Measuring
/// only `text` clipped every numbered callout.
///
/// The factor over-estimates on purpose: the label is semibold, and the
/// costs are asymmetric — a generous guess adds whitespace, a tight one
/// cuts words off.
fn callout_label_width(callout: &crate::callout::CalloutPlacement) -> f32 {
    let step_chars = callout
        .step
        .map(|step| format!("Step {step}. ").chars().count())
        .unwrap_or(0);
    (step_chars + callout.text.chars().count()) as f32 * CALLOUT_FONT * CALLOUT_CHAR_W + 8.0
}

/// Extra viewBox space around the board, in drawing units, for overlays that
/// draw outside the layout (the anatomy story's callouts).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DiagramMargin {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// One board, drawn by the row layout engine.
///
/// `u` is the pin pitch — the single scaling unit of the design language.
/// `scale` multiplies the rendered size only; geometry never changes with it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardDiagram(
    board: BoardDisplayFile,
    #[props(default)] mode: DiagramMode,
    #[props(default = 13.0)] u: f32,
    #[props(default = 1.0)] scale: f32,
    #[props(default = true)] labels: bool,
    #[props(default)] wired: Vec<WiredConnection>,
    #[props(default)] swatches: Vec<PinSwatch>,
    #[props(default)] margin: DiagramMargin,
    /// "Press THIS button" instructions (M2b). Anchored to the layout, so
    /// they land on the feature the renderer drew; the margin they need is
    /// derived, not hand-tuned by the caller.
    #[props(default)]
    callouts: Vec<BoardCallout>,
    /// Extra SVG content rendered above the board (annotation layers).
    #[props(default)]
    overlay: Option<Element>,
) -> Element {
    let options = DiagramOptions {
        mode,
        u,
        labels,
        wired,
        swatches,
    };
    let layout = BoardLayout::compute(&board, &options);
    let hw = &board.hw;

    // Callouts extend past the board, so the viewBox has to grow for them.
    // Deriving it here is the difference between an API and a trap: a caller
    // who forgot to widen `margin` would get labels clipped at the edge.
    let placed = layout.place_callouts(&callouts);
    let (view_left, view_right) = (layout.view_box[0], layout.view_box[0] + layout.view_box[2]);
    let margin = placed.iter().fold(margin, |margin, callout| {
        // The text box runs OUTWARD from the label point, whichever way the
        // label is anchored; the margin is how far that box escapes the
        // current viewBox on each side.
        let width = callout_label_width(callout);
        let (text_left, text_right) = if callout.start_anchored {
            (callout.label.0, callout.label.0 + width)
        } else {
            (callout.label.0 - width, callout.label.0)
        };
        DiagramMargin {
            left: margin.left.max((view_left - text_left).max(0.0)),
            right: margin.right.max((text_right - view_right).max(0.0)),
            ..margin
        }
    });

    let [vx, vy, vw, vh] = layout.view_box;
    let (vx, vy) = (vx - margin.left, vy - margin.top);
    let (vw, vh) = (
        vw + margin.left + margin.right,
        vh + margin.top + margin.bottom,
    );
    let width = (vw * scale).round();
    let height = (vh * scale).round();

    let board_w = layout.board_w;
    let board_h = layout.board_h;

    // Module body: the antenna keep-out strip occupies the top 14 units.
    let module = &hw.module;
    let module_y = if module.antenna {
        module.y + 14.0
    } else {
        module.y
    };
    let module_h = module.h - if module.antenna { 14.0 } else { 0.0 };
    let module_font = if module.w > 60.0 { 9.0 } else { 7.5 };
    let antenna_zigzag = module.antenna.then(|| {
        let mut path = format!("M {} {}", module.x + 6.0, module.y + 6.0);
        for step in 1..=8 {
            let x = module.x + 6.0 + step as f32 * (module.w - 12.0) / 8.0;
            let y = module.y + if step % 2 == 1 { 2.5 } else { 9.5 };
            path.push_str(&format!(" L {x} {y}"));
        }
        path
    });

    rsx! {
        svg {
            class: "lpb-diagram",
            view_box: "{vx} {vy} {vw} {vh}",
            width: "{width}",
            height: "{height}",
            xmlns: "http://www.w3.org/2000/svg",

            // ---- pcb ----------------------------------------------------
            rect {
                class: "lpb-pcb",
                x: "0",
                y: "0",
                width: "{board_w}",
                height: "{board_h}",
                rx: "7",
            }
            for (hx, hy) in [
                (8.0, 8.0),
                (board_w - 8.0, 8.0),
                (8.0, board_h - 8.0),
                (board_w - 8.0, board_h - 8.0),
            ] {
                circle { class: "lpb-hole", cx: "{hx}", cy: "{hy}", r: "2.6" }
            }

            // ---- module -------------------------------------------------
            if module.antenna {
                rect {
                    class: "lpb-antenna",
                    x: "{module.x}",
                    y: "{module.y}",
                    width: "{module.w}",
                    height: "12",
                }
            }
            if let Some(zigzag) = antenna_zigzag {
                path { class: "lpb-antenna-zigzag", d: "{zigzag}" }
            }
            rect {
                class: "lpb-module",
                x: "{module.x}",
                y: "{module_y}",
                width: "{module.w}",
                height: "{module_h}",
                rx: "2",
            }
            text {
                class: "lpb-module-label",
                x: "{module.x + module.w / 2.0}",
                y: "{module_y + module_h / 2.0 + 3.0}",
                text_anchor: "middle",
                style: "font-size: {module_font}px",
                "{module.label}"
            }

            // ---- fixed hardware -----------------------------------------
            for usb in layout.usb.iter() {
                rect {
                    class: "lpb-usb",
                    x: "{usb.rect.x}",
                    y: "{usb.rect.y}",
                    width: "{usb.rect.w}",
                    height: "{usb.rect.h}",
                    rx: "3",
                }
                if let Some(caption) = &usb.caption {
                    text {
                        class: "lpb-silk",
                        x: "{caption.x}",
                        y: "{caption.y}",
                        text_anchor: "middle",
                        "{caption.text}"
                    }
                }
            }
            // Buttons come from the LAYOUT (M2b): callouts anchor to the
            // same coordinates, which only holds if one place computes them.
            for button in layout.buttons.iter() {
                rect {
                    class: "lpb-button",
                    x: "{button.rect.x}",
                    y: "{button.rect.y}",
                    width: "{button.rect.w}",
                    height: "{button.rect.h}",
                    rx: "2",
                }
                circle {
                    class: "lpb-button-cap",
                    cx: "{button.center.0}",
                    cy: "{button.center.1}",
                    r: "{button.cap_radius}",
                }
                if let Some(caption) = &button.caption {
                    text {
                        class: "lpb-silk",
                        x: "{caption.x}",
                        y: "{caption.y}",
                        text_anchor: if caption.start_anchored { "start" } else { "end" },
                        "{caption.text}"
                    }
                }
            }
            if let Some(rgb) = &hw.rgb {
                rect {
                    class: "lpb-rgb",
                    x: "{rgb.x - 4.0}",
                    y: "{rgb.y - 4.0}",
                    width: "8",
                    height: "8",
                    rx: "1.5",
                }
                circle { class: "lpb-rgb-die", cx: "{rgb.x}", cy: "{rgb.y}", r: "2" }
            }
            for terminal in layout.terminals.iter() {
                rect {
                    class: "lpb-terminal",
                    x: "{terminal.rect.x}",
                    y: "{terminal.rect.y}",
                    width: "{terminal.rect.w}",
                    height: "{terminal.rect.h}",
                    rx: "2",
                }
                circle {
                    class: "lpb-screw",
                    cx: "{terminal.screw_center.0}",
                    cy: "{terminal.screw_center.1}",
                    r: "{terminal.screw_radius}",
                }
                if let Some(label) = &terminal.label {
                    text {
                        class: "lpb-silk",
                        x: "{label.x}",
                        y: "{label.y}",
                        text_anchor: "middle",
                        "{label.text}"
                    }
                }
            }

            // ---- rails --------------------------------------------------
            for row in layout.rail_rows() {
                rect {
                    class: "lpb-pad lpb-pad--{pad_css_suffix(row.role)}",
                    x: "{row.pad.x}",
                    y: "{row.pad.y}",
                    width: "{row.pad.w}",
                    height: "{row.pad.h}",
                    rx: "1.4",
                }
                // Screw-terminal pins carry the same screw head the top-edge
                // terminal blocks use, so "this is a screw terminal" reads
                // identically everywhere.
                if row.pad_style == crate::display_manifest::PadStyle::Screw {
                    circle {
                        class: "lpb-screw",
                        cx: "{row.pad.center().0}",
                        cy: "{row.pad.center().1}",
                        r: "{row.pad.w * 0.34}",
                    }
                }
                if let Some(label) = &row.label {
                    PinLabel { label: label.clone() }
                }
                CellRow { cells: row.cells.clone(), font: layout.font, gpio: row.gpio }
            }

            // ---- band + leaders -----------------------------------------
            for row in layout.band.iter() {
                if !row.cells.is_empty() {
                    path {
                        class: "lpb-leader",
                        d: "M {row.leader[0].0} {row.leader[0].1} L {row.leader[1].0} {row.leader[1].1} L {row.leader[2].0} {row.leader[2].1}",
                    }
                    CellRow { cells: row.cells.clone(), font: layout.font, gpio: row.gpio }
                }
            }

            // Callouts sit under any story overlay, above the board. Bolder
            // than the anatomy annotation they grew out of: an instruction
            // someone follows at the desk has to win against the pin rows
            // behind it, so the leader is solid and ends in an arrowhead
            // rather than fading into a dot.
            for callout in placed.iter() {
                {
                    // The head points along the lead, toward the feature.
                    let (ax, ay) = callout.anchor;
                    let dir = if callout.start_anchored { 1.0 } else { -1.0 };
                    let base = ax + dir * ARROW_LEN;
                    let head = format!(
                        "M {ax} {ay} L {base} {} L {base} {} Z",
                        ay - ARROW_HALF_W,
                        ay + ARROW_HALF_W
                    );
                    rsx! {
                        path {
                            class: "lpb-callout-line",
                            d: "M {callout.label.0} {callout.label.1 - 2.5} L {base} {ay}",
                        }
                        path { class: "lpb-callout-head", d: "{head}" }
                        text {
                            class: "lpb-callout-label",
                            x: "{callout.label.0}",
                            y: "{callout.label.1}",
                            text_anchor: if callout.start_anchored { "start" } else { "end" },
                            if let Some(step) = callout.step {
                                tspan { class: "lpb-callout-step", "Step {step}. " }
                            }
                            "{callout.text}"
                        }
                    }
                }
            }
            if let Some(overlay) = overlay {
                {overlay}
            }
        }
    }
}

/// A pin's name, inside the board edge. Plain mode uses the silkscreen color;
/// annotated modes color it by the pin's name-cell family.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PinLabel(label: RowLabel) -> Element {
    let class = match label.kind {
        None => "lpb-silk".to_string(),
        Some(kind) => format!("lpb-pinlabel lpb-fg--{}", kind.css_suffix()),
    };
    let anchor = if label.start_anchored { "start" } else { "end" };
    rsx! {
        text {
            class: "{class}",
            x: "{label.x}",
            y: "{label.y}",
            text_anchor: anchor,
            style: "font-size: {label.font_size}px",
            "{label.text}"
        }
    }
}

/// One row's cells (already positioned by the layout).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CellRow(cells: Vec<CellLayout>, font: f32, gpio: Option<u8>) -> Element {
    let gpio_attr = gpio.map(|gpio| gpio.to_string()).unwrap_or_default();
    rsx! {
        g { class: "lpb-row", "data-gpio": "{gpio_attr}",
            for cell in cells.iter() {
                match &cell.body {
                    CellBody::Text { text, kind } => rsx! {
                        g { class: "lpb-cell lpb-cell--{kind.css_suffix()}",
                            rect {
                                class: "lpb-cell-rect",
                                x: "{cell.rect.x}",
                                y: "{cell.rect.y}",
                                width: "{cell.rect.w}",
                                height: "{cell.rect.h}",
                                rx: "3",
                            }
                            text {
                                class: "lpb-cell-text",
                                x: "{cell.rect.x + cell.rect.w / 2.0}",
                                y: "{cell.rect.y + cell.rect.h / 2.0 + font * 0.36}",
                                text_anchor: "middle",
                                style: "font-size: {font}px",
                                "{text}"
                            }
                        }
                    },
                    CellBody::Swatch { colors, selected } => rsx! {
                        g {
                            class: if *selected {
                                "lpb-cell lpb-cell--swatch lpb-cell--swatch-selected"
                            } else {
                                "lpb-cell lpb-cell--swatch"
                            },
                            rect {
                                class: "lpb-cell-rect",
                                x: "{cell.rect.x}",
                                y: "{cell.rect.y}",
                                width: "{cell.rect.w}",
                                height: "{cell.rect.h}",
                                rx: "3",
                            }
                            for (index, color) in colors.iter().enumerate() {
                                rect {
                                    class: "lpb-swatch-px",
                                    x: "{cell.rect.x + 4.0 + index as f32 * cell.rect.h * 0.74}",
                                    y: "{cell.rect.y + cell.rect.h / 2.0 - cell.rect.h * 0.29}",
                                    width: "{cell.rect.h * 0.58}",
                                    height: "{cell.rect.h * 0.58}",
                                    rx: "1.5",
                                    fill: "{color}",
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
