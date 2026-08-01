//! `BoardDiagram`: the row-engine SVG renderer as a Dioxus component.
//!
//! All geometry comes from [`crate::geometry::BoardLayout`]; this module only
//! walks the computed layout and emits SVG. Colors ride `lpb-*` CSS classes
//! (studio palette; the class block lives in the consuming app's stylesheet —
//! see `lpa-studio-web/src/style.css`), so the same component serves the
//! catalog, provisioning picker, hardware pane, and discovery UI.

use dioxus::prelude::*;

use crate::display_manifest::BoardDisplayFile;
use crate::geometry::{
    BoardLayout, CellBody, CellLayout, DiagramMode, DiagramOptions, PinSwatch, RowLabel,
    WiredConnection, pad_css_suffix,
};

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
            for usb in hw.usb.iter() {
                rect {
                    class: "lpb-usb",
                    x: "{usb.x - 11.0}",
                    y: "{board_h - 9.0}",
                    width: "22",
                    height: "13",
                    rx: "3",
                }
                if labels {
                    text {
                        class: "lpb-silk",
                        x: "{usb.x}",
                        y: "{board_h + 0.9 * layout.u}",
                        text_anchor: "middle",
                        "{usb.label}"
                    }
                }
            }
            for button in hw.buttons.iter() {
                {
                    let by = if button.y < 0.0 { board_h + button.y } else { button.y };
                    let inner = button.x < board_w / 2.0;
                    let caption_x = if inner { button.x + 10.0 } else { button.x - 10.0 };
                    let caption_anchor = if inner { "start" } else { "end" };
                    rsx! {
                        rect {
                            class: "lpb-button",
                            x: "{button.x - 7.0}",
                            y: "{by - 5.0}",
                            width: "14",
                            height: "10",
                            rx: "2",
                        }
                        circle { class: "lpb-button-cap", cx: "{button.x}", cy: "{by}", r: "2.6" }
                        if labels {
                            text {
                                class: "lpb-silk",
                                x: "{caption_x}",
                                y: "{by + 2.5}",
                                text_anchor: caption_anchor,
                                "{button.label}"
                            }
                        }
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
