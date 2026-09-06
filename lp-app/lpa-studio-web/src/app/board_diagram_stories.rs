//! Stories for the board diagram renderer (`lpa-boards::BoardDiagram`).
//!
//! Coverage per the M2 milestone: every mode at u=12 and u=13, the smallest
//! and largest checked-in boards, and the annotated-anatomy figure whose
//! callout anchors are computed from the shared geometry module — the layout
//! engine is deterministic, so computed positions ARE rendered positions.
//! Scales mirror the approved spike (caps 1.45, wired 1.05, swatch 1.15) so
//! captured density is comparable to spike rev 5.

use dioxus::prelude::*;
use lpa_boards::geometry::{BoardLayout, DiagramOptions};
use lpa_boards::{
    BoardCallout, BoardDiagram, BoardDisplayFile, BoardsCatalogPage, DiagramMargin, DiagramMode,
    HostOs, PinSwatch, WiredConnection, all_boards, board_by_id,
};
use lpa_studio_web_story_macros::story;

fn board(id: &str) -> BoardDisplayFile {
    board_by_id(id)
        .unwrap_or_else(|| panic!("board {id} in the embedded catalog"))
        .clone()
}

/// The spike's hardware-pane wiring for the S3 devkit.
fn s3_wiring() -> Vec<WiredConnection> {
    vec![
        WiredConnection {
            gpio: 16,
            title: "porch strip".into(),
            extra: Some("WS2812 ×300".into()),
        },
        WiredConnection {
            gpio: 4,
            title: "eave strip".into(),
            extra: Some("WS2811 ×150".into()),
        },
        WiredConnection {
            gpio: 6,
            title: "mode button".into(),
            extra: None,
        },
        WiredConnection {
            gpio: 48,
            title: "status pixel".into(),
            extra: None,
        },
    ]
}

/// Palindromic discovery codes with K separators for every free
/// output-capable pin, mirroring the spike's code plan (3 digits × 5 colors
/// covers the C6's free-pin count on 5 pixels). The real planner is M7's;
/// this is display fixture data.
fn discovery_swatches(board: &BoardDisplayFile, reserved: &[u8]) -> Vec<PinSwatch> {
    const DIGITS: [&str; 5] = ["#ef5350", "#3fd68e", "#4f9cf0", "#2fd4c9", "#d06bd6"];
    const OFF: &str = "#15181d";
    board
        .pins()
        .filter(|pin| pin.role.output_eligible())
        .filter_map(|pin| pin.gpio)
        .filter(|gpio| !reserved.contains(gpio))
        .enumerate()
        .map(|(index, gpio)| {
            let first = DIGITS[index % DIGITS.len()];
            let middle = DIGITS[(index / DIGITS.len()) % DIGITS.len()];
            PinSwatch {
                gpio,
                colors: vec![
                    first.into(),
                    OFF.into(),
                    middle.into(),
                    OFF.into(),
                    first.into(),
                ],
                selected: false,
            }
        })
        .collect()
}

/// The spike's figure chrome: diagrams sit on the terminal surface.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardFigure(children: Element) -> Element {
    rsx! {
        div { style: "display: inline-flex; gap: 22px; align-items: flex-start; background: var(--studio-color-terminal); border: 1px solid var(--studio-color-border); border-radius: 12px; padding: 18px;",
            {children}
        }
    }
}

#[story(
    description = "Every checked-in board as a plain thumbnail — the catalog/picker rendering. One metadata source, one renderer, no hand-drawn images."
)]
pub(crate) fn catalog_thumbnails() -> Element {
    rsx! {
        div { style: "display: flex; gap: 14px; flex-wrap: wrap; align-items: flex-end;",
            for entry in all_boards() {
                div { style: "display: flex; flex-direction: column; align-items: center; gap: 6px; background: var(--studio-color-terminal); border: 1px solid var(--studio-color-border); border-radius: 10px; padding: 12px;",
                    BoardDiagram {
                        board: entry.clone(),
                        mode: DiagramMode::Plain,
                        scale: 0.55,
                        labels: false,
                    }
                    span { style: "font-size: 10px; color: var(--studio-color-text-subtle); font-family: var(--studio-font-mono);",
                        "{entry.display_name}"
                    }
                }
            }
        }
    }
}

#[story(
    description = "The fit contract at picker size (P10). Top row: every board TURNED a quarter turn and fitted to the device picker's 132×56 tile band — one box, so eight boards that differ three to one in drawing height line their names up on one row, which a single `scale` cannot do; and on their side, because a devkit standing upright in that band is a 20px sliver in a field of air while the same board lying down fills it. Middle row: the same boards upright at the same fit, which is what the turn is worth. Bottom row: the turned boards in the trigger's 20×14 swatch, where all that survives is the silhouette — a long devkit against a stub of a XIAO. Labels are off at every size: pin names here would be a grey smear, and turned they would be a sideways one."
)]
pub(crate) fn picker_thumbnails() -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 18px; align-items: flex-start;",
            div { style: "display: flex; gap: 10px; flex-wrap: wrap; align-items: flex-start;",
                for entry in all_boards() {
                    div { style: "display: flex; flex-direction: column; gap: 4px; width: 150px; border: 1px solid var(--studio-color-border-subtle); border-radius: 6px; padding: 8px;",
                        span { style: "display: grid; place-items: center; height: 56px; overflow: hidden; border-radius: 3px; background: var(--studio-color-terminal);",
                            BoardDiagram {
                                board: entry.clone(),
                                mode: DiagramMode::Plain,
                                labels: false,
                                landscape: true,
                                fit: (132.0, 56.0),
                            }
                        }
                        span { style: "font-size: 11.5px; font-weight: 600; color: var(--studio-color-text-strong);",
                            "{entry.display_name}"
                        }
                        span { style: "font-size: 10.5px; color: var(--studio-color-text-dim);",
                            "{entry.manufacturer} \u{b7} {entry.flash} flash"
                        }
                    }
                }
            }
            div { style: "display: flex; gap: 10px; flex-wrap: wrap; align-items: flex-start;",
                for entry in all_boards() {
                    span { style: "display: grid; place-items: center; width: 132px; height: 56px; overflow: hidden; border-radius: 3px; background: var(--studio-color-terminal);",
                        BoardDiagram {
                            board: entry.clone(),
                            mode: DiagramMode::Plain,
                            labels: false,
                            fit: (132.0, 56.0),
                        }
                    }
                }
            }
            div { style: "display: flex; gap: 10px; align-items: center;",
                for entry in all_boards() {
                    span { style: "display: grid; place-items: center; width: 22px; height: 16px; overflow: hidden; border-radius: 3px; border: 1px solid var(--studio-color-border-strong); background: var(--studio-color-surface-raised-strong);",
                        BoardDiagram {
                            board: entry.clone(),
                            mode: DiagramMode::Plain,
                            labels: false,
                            landscape: true,
                            fit: (20.0, 14.0),
                        }
                    }
                }
            }
        }
    }
}

#[story(description = "Plain mode at u=12: the board at rest, labels on (C6 devkit).")]
pub(crate) fn plain_u12() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram { board: board("espressif/esp32-c6-devkitc-1"), u: 12.0, scale: 1.15 }
        }
    }
}

#[story(description = "Plain mode at u=13 (C6 devkit).")]
pub(crate) fn plain_u13() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram { board: board("espressif/esp32-c6-devkitc-1"), u: 13.0, scale: 1.15 }
        }
    }
}

#[story(description = "Caps mode at u=12: every pin's capability cells in their rows (C6 devkit).")]
pub(crate) fn caps_u12() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("espressif/esp32-c6-devkitc-1"),
                mode: DiagramMode::Caps,
                u: 12.0,
                scale: 1.45,
            }
        }
    }
}

#[story(
    description = "Caps mode at u=13 — the boards-page pinout density the spike settled on (C6 devkit)."
)]
pub(crate) fn caps_u13() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("espressif/esp32-c6-devkitc-1"),
                mode: DiagramMode::Caps,
                u: 13.0,
                scale: 1.45,
            }
        }
    }
}

#[story(
    description = "The smallest checked-in board (XIAO ESP32-C6) in caps mode: named D-pins, no IO prefix."
)]
pub(crate) fn caps_smallest_xiao() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("seeed/xiao-esp32-c6"),
                mode: DiagramMode::Caps,
                u: 13.0,
                scale: 1.45,
            }
        }
    }
}

#[story(
    description = "The largest checked-in board (ESP32 DevKitC v4, 38 pins) in caps mode: the density stress case — input-only warns, DACs, touch, SPI/I2C/UART, strap and reserved flash pins."
)]
pub(crate) fn caps_largest_devkitc_v4() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("espressif/esp32-devkitc-v4"),
                mode: DiagramMode::Caps,
                u: 13.0,
                scale: 1.45,
            }
        }
    }
}

#[story(
    description = "Wired mode at u=12: bound connections as violet cells beside the pin names (S3 devkit, device-card density)."
)]
pub(crate) fn wired_u12() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("espressif/esp32-s3-devkitc-1"),
                mode: DiagramMode::Wired,
                u: 12.0,
                scale: 1.05,
                wired: s3_wiring(),
            }
        }
    }
}

#[story(description = "Wired mode at u=13 (S3 devkit).")]
pub(crate) fn wired_u13() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("espressif/esp32-s3-devkitc-1"),
                mode: DiagramMode::Wired,
                u: 13.0,
                scale: 1.05,
                wired: s3_wiring(),
            }
        }
    }
}

#[story(
    description = "Wired mode with top-edge screw terminals (QuinLED-Dig-Uno): terminal rows live in a band above the board, tied to their pads by leader lines."
)]
pub(crate) fn wired_band_quinled() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("quinled/dig-uno"),
                mode: DiagramMode::Wired,
                u: 12.0,
                scale: 1.05,
                wired: vec![
                    WiredConnection {
                        gpio: 16,
                        title: "garden run".into(),
                        extra: Some("WS2815 ×600".into()),
                    },
                    WiredConnection {
                        gpio: 3,
                        title: "path lights".into(),
                        extra: Some("WS2811 ×100".into()),
                    },
                ],
            }
        }
    }
}

#[story(
    description = "The DOMRAEM DOM-Z-102 (Ethernet WLED-class controller, screw terminals both sides) wired on all four fused data channels. Its stacked OPT/RST buttons render side by side — the 2D compromise pending the callout milestone."
)]
pub(crate) fn wired_domraem_4ch() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("domraem/dom-z-102"),
                mode: DiagramMode::Wired,
                u: 12.0,
                scale: 1.05,
                wired: vec![
                    WiredConnection {
                        gpio: 18,
                        title: "roofline".into(),
                        extra: Some("WS2815 ×800".into()),
                    },
                    WiredConnection {
                        gpio: 16,
                        title: "porch".into(),
                        extra: Some("WS2812 ×300".into()),
                    },
                    WiredConnection {
                        gpio: 14,
                        title: "garden".into(),
                        extra: Some("WS2811 ×150".into()),
                    },
                    WiredConnection {
                        gpio: 2,
                        title: "path".into(),
                        extra: None,
                    },
                ],
            }
        }
    }
}

#[story(
    description = "Swatch mode at u=12: palindromic K-separated discovery codes on every free output-capable pin (C6 devkit; IO8 reserved for the onboard pixel, USB-JTAG pins skipped by role)."
)]
pub(crate) fn swatch_u12() -> Element {
    let c6 = board("espressif/esp32-c6-devkitc-1");
    let swatches = discovery_swatches(&c6, &[8]);
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: c6,
                mode: DiagramMode::Swatch,
                u: 12.0,
                scale: 1.15,
                swatches,
            }
        }
    }
}

#[story(description = "Swatch mode at u=13 (C6 devkit).")]
pub(crate) fn swatch_u13() -> Element {
    let c6 = board("espressif/esp32-c6-devkitc-1");
    let swatches = discovery_swatches(&c6, &[8]);
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: c6,
                mode: DiagramMode::Swatch,
                u: 13.0,
                scale: 1.15,
                swatches,
            }
        }
    }
}

#[story(
    description = "Swatch mode after the user clicks their code: only the confirmed pin keeps its swatch, framed in bound violet."
)]
pub(crate) fn swatch_confirmed() -> Element {
    let c6 = board("espressif/esp32-c6-devkitc-1");
    let confirmed: Vec<PinSwatch> = discovery_swatches(&c6, &[8])
        .into_iter()
        .filter(|swatch| swatch.gpio == 21)
        .map(|swatch| PinSwatch {
            selected: true,
            ..swatch
        })
        .collect();
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: c6,
                mode: DiagramMode::Swatch,
                u: 12.0,
                scale: 1.15,
                swatches: confirmed,
            }
        }
    }
}

#[story(
    description = "The /boards catalog page: tier legend, sort + SoC filter controls, cards with renderer thumbnails — and the D5 driver warning on the CH340K board (rendered as on macOS)."
)]
pub(crate) fn catalog_page_macos() -> Element {
    rsx! {
        div { style: "max-width: 1080px;",
            BoardsCatalogPage { os: HostOs::MacOs }
        }
    }
}

#[story(
    description = "The board detail view (/boards/domraem/dom-z-102 deep link): full caps pinout, driver section with the verified macOS CH340K steps, and the os-tagged note system."
)]
pub(crate) fn detail_page_dom_z_102() -> Element {
    rsx! {
        div { style: "max-width: 1080px;",
            BoardsCatalogPage {
                os: HostOs::MacOs,
                initial_board: Some("domraem/dom-z-102".to_string()),
            }
        }
    }
}

// ---- computed board↔firmware join (roadmap M5) ---------------------------

#[story(
    description = "Board detail, exact flash fit: the XIAO ESP32-C6 (4 MB) against the 4 MB C6 build. The Firmware block is computed — chip identity ∧ flash fit over the checked-in build defs — and its feature lines are read from the manifest compiled into that image, never authored here."
)]
pub(crate) fn firmware_detail_exact_fit() -> Element {
    rsx! {
        div { style: "max-width: 1080px;",
            BoardsCatalogPage {
                os: HostOs::MacOs,
                initial_board: Some("seeed/xiao-esp32-c6".to_string()),
            }
        }
    }
}

#[story(
    description = "Board detail with headroom and honest gaps: the XIAO ESP32-S3 Plus (16 MB) runs the 8 MB S3 build with 8 MB spare, gains f32 shader math, and lacks the ESP-NOW radio the C6 image carries. Gaps use the driver-warning attention tone — a caveat, not a fault."
)]
pub(crate) fn firmware_detail_headroom_and_gaps() -> Element {
    rsx! {
        div { style: "max-width: 1080px;",
            BoardsCatalogPage {
                os: HostOs::MacOs,
                initial_board: Some("seeed/xiao-esp32-s3-plus".to_string()),
            }
        }
    }
}

#[story(
    description = "Board detail with no firmware: the QuinLED-Dig-Uno is a classic ESP32, and no checked-in build def targets that chip yet. The empty state states the computed reason instead of going silent."
)]
pub(crate) fn firmware_detail_no_build() -> Element {
    rsx! {
        div { style: "max-width: 1080px;",
            BoardsCatalogPage {
                os: HostOs::MacOs,
                initial_board: Some("quinled/dig-uno".to_string()),
            }
        }
    }
}

// ---- callouts (M2b) ------------------------------------------------------

#[story(
    description = "The recovery ritual on the C6 devkit, as numbered steps with bold arrows: hold BOOT, then plug in USB. Gold, not attention-orange — a callout explains rather than alarms (M2b gate). BOOT and RST sit side by side, which is exactly why prose fails and an arrow does not."
)]
pub(crate) fn callout_boot_button() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("espressif/esp32-c6-devkitc-1"),
                u: 13.0,
                scale: 1.25,
                callouts: vec![
                    BoardCallout::step(1, lpa_boards::CalloutTarget::Button("BOOT".into()), "Hold BOOT"),
                    BoardCallout::step(2, lpa_boards::CalloutTarget::Usb("USB".into()), "Plug in USB"),
                ],
            }
        }
    }
}

#[story(
    description = "The stacked-button case that motivated M2b (DOM-Z-102: BOOT sits physically ON TOP of RST). The 2D drawing can only place them adjacent, so the step TEXT carries the disambiguation the picture cannot."
)]
pub(crate) fn callout_stacked_buttons() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("domraem/dom-z-102"),
                u: 12.0,
                scale: 1.15,
                callouts: vec![
                    BoardCallout::step(
                        1,
                        lpa_boards::CalloutTarget::Button("BOOT".into()),
                        "Hold BOOT (the TOP button)",
                    ),
                    BoardCallout::step(
                        2,
                        lpa_boards::CalloutTarget::Button("RST".into()),
                        "Tap RST (the BOTTOM button)",
                    ),
                ],
            }
        }
    }
}

#[story(
    description = "A callout addressing a PIN rather than a button (DOM-Z-102 IO18): pin discovery and wiring help point at pads the same way. Anchors resolve against the same layout the renderer walks, so the dot lands on the drawn pad."
)]
pub(crate) fn callout_pin() -> Element {
    rsx! {
        BoardFigure {
            BoardDiagram {
                board: board("domraem/dom-z-102"),
                mode: DiagramMode::Caps,
                u: 12.0,
                scale: 1.15,
                callouts: vec![BoardCallout::gpio(18, "Wire your first strip here")],
            }
        }
    }
}

// ---- annotated anatomy ---------------------------------------------------

/// Estimated bbox of an end-anchored pin label (the layout stores anchor +
/// baseline; width comes from the same char-count heuristic cells use).
fn label_center(x: f32, baseline_y: f32, text_len: usize, font_size: f32) -> (f32, f32) {
    let width = text_len as f32 * font_size * 0.62;
    (x - width / 2.0, baseline_y - 2.0)
}

fn note(lx: f32, ly: f32, px: f32, py: f32, text: &str, anchor: &str) -> Element {
    let ty = if ly < py { ly - 3.0 } else { ly + 8.0 };
    rsx! {
        path { class: "lpb-anat-line", d: "M {lx} {ly} L {px} {py}" }
        circle { class: "lpb-anat-dot", cx: "{px}", cy: "{py}", r: "1.6" }
        text { class: "lpb-anat-label", x: "{lx}", y: "{ty}", text_anchor: "{anchor}", "{text}" }
    }
}

fn bracket(x: f32, y1: f32, y2: f32, dir: f32) -> Element {
    let tip = x + 4.0 * dir;
    rsx! {
        path {
            class: "lpb-anat-shape lpb-anat-solid",
            d: "M {tip} {y1} L {x} {y1} L {x} {y2} L {tip} {y2}",
        }
    }
}

#[story(
    label = "Anatomy",
    description = "The design language drawn on a real board (QuinLED-Dig-Uno): unit, row, cell, rail, band, leader, pad, label — every callout anchor computed from the shared geometry module, not hand-positioned. Doc figure for docs/design/board-diagrams.md."
)]
pub(crate) fn anatomy() -> Element {
    let quinled = board("quinled/dig-uno");
    let options = DiagramOptions {
        mode: DiagramMode::Caps,
        u: 13.0,
        ..DiagramOptions::default()
    };
    let layout = BoardLayout::compute(&quinled, &options);
    let u = layout.u;

    // Pad — right rail, last pin (free space beyond: no cells on that row).
    let pad_target = layout.right.last().expect("quinled right rail").pad;
    let (pad_cx, pad_cy) = pad_target.center();

    // Label — the "IO15" name inside the board (right rail, second pin).
    let io15 = &layout.right[1];
    let label = io15.label.as_ref().expect("labels are on");
    let (lbl_x, lbl_y) = label_center(
        label.x,
        label.y,
        label.text.chars().count(),
        label.font_size,
    );

    // Row + Cell — IO15's row: outline pad → cells, then point at the cell.
    let cells_x0 = io15
        .cells
        .iter()
        .map(|cell| cell.rect.x)
        .fold(f32::MAX, f32::min);
    let cells_x1 = io15
        .cells
        .iter()
        .map(|cell| cell.rect.right())
        .fold(f32::MIN, f32::max);
    let row_x0 = cells_x0.min(io15.pad.x) - 2.0;
    let row_x1 = cells_x1.max(io15.pad.right()) + 2.0;
    let row_y0 = io15.pad.center().1 - u / 2.0;
    let first_cell = io15.cells.first().expect("IO15 has an ADC cell");
    let (cell_cx, cell_cy) = first_cell.rect.center();

    // Rail — bracket spanning the right rail's pads.
    let rail_x = row_x1 + 44.0;
    let rail_y0 = layout.right.first().unwrap().pad.y - 2.0;
    let rail_y1 = layout.right.last().unwrap().pad.bottom() + 2.0;
    let rail_mid =
        (layout.right.first().unwrap().pad.y + layout.right.last().unwrap().pad.y) / 2.0 + 4.0;

    // Band — bracket spanning the terminal band rows above the board.
    let band_x = layout
        .band
        .iter()
        .filter_map(|row| row.cells.last())
        .map(|cell| cell.rect.right())
        .fold(f32::MIN, f32::max)
        + 6.0;
    let band_y0 = layout
        .band
        .iter()
        .flat_map(|row| row.cells.first())
        .map(|cell| cell.rect.y)
        .fold(f32::MAX, f32::min)
        - 2.0;
    let band_y1 = layout
        .band
        .iter()
        .flat_map(|row| row.cells.first())
        .map(|cell| cell.rect.bottom())
        .fold(f32::MIN, f32::max)
        + 2.0;
    let band_mid = (band_y0 + band_y1) / 2.0 + 3.0;

    // Leader — the elbow line from the first (deepest) terminal pad.
    let leader = &layout.band.first().expect("quinled has terminals").leader;
    let (leader_x, leader_bottom) = (leader[0].0, leader[0].1);

    // Unit u — dimension ticks between the first two left pads.
    let (p0x, p0y) = layout.left[0].pad.center();
    let (_p1x, p1y) = layout.left[1].pad.center();
    let ux = p0x - 12.0;

    let overlay = rsx! {
        {note(pad_cx + 26.0, pad_cy + 14.0, pad_cx + 3.0, pad_cy + 2.0, "Pad", "start")}
        {note(lbl_x - 4.0, lbl_y + 30.0, lbl_x, lbl_y + 4.0, "Label", "middle")}
        rect {
            class: "lpb-anat-shape",
            x: "{row_x0}",
            y: "{row_y0}",
            width: "{row_x1 - row_x0}",
            height: "{u}",
            rx: "4",
        }
        {note(row_x1 + 26.0, io15.pad.y - 12.0, row_x1 + 1.0, row_y0 + 2.0, "Row (1u tall)", "start")}
        {note(cell_cx + 10.0, cell_cy + 26.0, cell_cx, cell_cy + 4.0, "Cell", "start")}
        {bracket(rail_x, rail_y0, rail_y1, -1.0)}
        {note(rail_x + 20.0, rail_mid, rail_x + 1.0, rail_mid, "Rail", "start")}
        {bracket(band_x, band_y0, band_y1, -1.0)}
        {note(band_x + 18.0, band_mid, band_x + 1.0, band_mid, "Band", "start")}
        {note(leader_x - 28.0, leader_bottom + 16.0, leader_x + 1.0, leader_bottom - 4.0, "Leader", "start")}
        path {
            class: "lpb-anat-shape lpb-anat-solid",
            d: "M {ux - 3.0} {p0y} L {ux + 3.0} {p0y} M {ux} {p0y} L {ux} {p1y} M {ux - 3.0} {p1y} L {ux + 3.0} {p1y}",
        }
        {note(ux - 26.0, p0y + u / 2.0 + 3.0, ux, (p0y + p1y) / 2.0, "u = pitch", "end")}
    };

    rsx! {
        BoardFigure {
            BoardDiagram {
                board: quinled,
                mode: DiagramMode::Caps,
                u: 13.0,
                scale: 1.35,
                margin: DiagramMargin {
                    top: 8.0,
                    right: 92.0,
                    bottom: 12.0,
                    left: 44.0,
                },
                overlay,
            }
        }
    }
}
