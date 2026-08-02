//! Live preview: [`BoardDiagram`] re-rendered on every edit, with a mode
//! switcher, pitch toggle, and a generic anatomy overlay built from the same
//! deterministic layout the renderer walks. Wired/swatch modes render sample
//! data — the editor has no real connections; the point is seeing how the
//! def behaves in those surfaces.

use dioxus::prelude::*;
use lpa_boards::geometry::{BoardLayout, DiagramOptions, RailRow};
use lpa_boards::{
    BoardDiagram, BoardDisplayFile, DiagramMargin, DiagramMode, PinSwatch, WiredConnection,
};

use crate::editor_core::editor_doc::EditorDoc;

const MODES: &[(DiagramMode, &str)] = &[
    (DiagramMode::Plain, "plain"),
    (DiagramMode::Caps, "caps"),
    (DiagramMode::Wired, "wired"),
    (DiagramMode::Swatch, "swatch"),
];

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PreviewPane(doc: Signal<EditorDoc>) -> Element {
    let mut mode = use_signal(|| DiagramMode::Caps);
    let mut u = use_signal(|| 13.0f32);
    let mut anatomy = use_signal(|| false);
    let board = doc.read().board.clone();

    let wired = matches!(mode(), DiagramMode::Wired)
        .then(|| sample_wired(&board))
        .unwrap_or_default();
    let swatches = matches!(mode(), DiagramMode::Swatch)
        .then(|| sample_swatches(&board))
        .unwrap_or_default();
    let sample_note = match mode() {
        DiagramMode::Wired if !wired.is_empty() => Some("sample connections"),
        DiagramMode::Swatch if !swatches.is_empty() => Some("sample discovery codes"),
        _ => None,
    };

    let overlay_parts = anatomy().then(|| {
        let options = DiagramOptions {
            mode: mode(),
            u: u(),
            wired: wired.clone(),
            swatches: swatches.clone(),
            ..DiagramOptions::default()
        };
        anatomy_overlay(&board, &options)
    });
    let (margin, overlay) = match overlay_parts {
        Some((margin, overlay)) => (margin, Some(overlay)),
        None => (DiagramMargin::default(), None),
    };

    rsx! {
        section { class: "lpb-ed-section lpb-ed-preview",
            div { class: "lpb-ed-section-head",
                h2 { "Preview" }
                if let Some(note) = sample_note {
                    span { class: "lpb-ed-sample-note", "{note}" }
                }
            }
            div { class: "lpb-ed-preview-controls",
                span { class: "lpb-ed-seg",
                    for (candidate, name) in MODES {
                        button {
                            key: "{name}",
                            class: "lpb-ed-seg-btn",
                            "aria-pressed": if mode() == *candidate { "true" } else { "false" },
                            onclick: move |_| mode.set(*candidate),
                            "{name}"
                        }
                    }
                }
                span { class: "lpb-ed-seg",
                    for pitch in [12.0f32, 13.0f32] {
                        button {
                            key: "{pitch}",
                            class: "lpb-ed-seg-btn",
                            "aria-pressed": if u() == pitch { "true" } else { "false" },
                            onclick: move |_| u.set(pitch),
                            "u={pitch}"
                        }
                    }
                }
                button {
                    class: "lpb-ed-btn",
                    "aria-pressed": if anatomy() { "true" } else { "false" },
                    onclick: move |_| anatomy.set(!anatomy()),
                    "anatomy"
                }
            }
            div { class: "lpb-ed-figure",
                BoardDiagram {
                    board,
                    mode: mode(),
                    u: u(),
                    scale: 1.25,
                    wired,
                    swatches,
                    margin,
                    overlay,
                }
            }
        }
    }
}

/// Sample wired connections: the first three discovery-eligible gpios.
fn sample_wired(board: &BoardDisplayFile) -> Vec<WiredConnection> {
    const TITLES: [(&str, Option<&str>); 3] = [
        ("porch strip", Some("WS2812 ×300")),
        ("eave strip", Some("WS2811 ×150")),
        ("status pixel", None),
    ];
    board
        .pins()
        .filter(|pin| pin.role.output_eligible())
        .filter_map(|pin| pin.gpio)
        .zip(TITLES)
        .map(|(gpio, (title, extra))| WiredConnection {
            gpio,
            title: title.into(),
            extra: extra.map(Into::into),
        })
        .collect()
}

/// Sample palindromic K-separated discovery codes on every eligible pin
/// (display fixture data — the real planner is M7's).
fn sample_swatches(board: &BoardDisplayFile) -> Vec<PinSwatch> {
    const DIGITS: [&str; 5] = ["#ef5350", "#3fd68e", "#4f9cf0", "#2fd4c9", "#d06bd6"];
    const OFF: &str = "#15181d";
    board
        .pins()
        .filter(|pin| pin.role.output_eligible())
        .filter_map(|pin| pin.gpio)
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

// ---- anatomy overlay -----------------------------------------------------
//
// A generic subset of the anatomy story's callouts, computed from the shared
// layout so it works on whatever board is being authored: u-pitch ticks, a
// rail bracket, the row/cell outline on the first pin that has cells, a pad
// note, and the band bracket when terminals exist. Every anchor is
// defensive — a missing feature just drops its callout.

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

fn anatomy_overlay(board: &BoardDisplayFile, options: &DiagramOptions) -> (DiagramMargin, Element) {
    let layout = BoardLayout::compute(board, options);
    let u = layout.u;

    // The rail the brackets hang on: right when populated, else left.
    let rail: &[RailRow] = if layout.right.is_empty() {
        &layout.left
    } else {
        &layout.right
    };
    let rail_is_right = !layout.right.is_empty();

    let rail_bracket = (rail.len() >= 2).then(|| {
        let x0 = rail
            .iter()
            .map(|row| if rail_is_right { row.pad.right() } else { row.pad.x })
            .fold(if rail_is_right { f32::MIN } else { f32::MAX }, |acc, x| {
                if rail_is_right { acc.max(x) } else { acc.min(x) }
            });
        let x = if rail_is_right { x0 + 40.0 } else { x0 - 40.0 };
        let y0 = rail.first().unwrap().pad.y - 2.0;
        let y1 = rail.last().unwrap().pad.bottom() + 2.0;
        let mid = (y0 + y1) / 2.0 + 3.0;
        let dir = if rail_is_right { -1.0 } else { 1.0 };
        rsx! {
            {bracket(x, y0, y1, dir)}
            {note(x + 16.0 * -dir, mid, x + 1.0 * -dir, mid, "Rail", if rail_is_right { "start" } else { "end" })}
        }
    });

    let pitch_ticks = (layout.left.len() >= 2 || layout.right.len() >= 2).then(|| {
        let side: &[RailRow] = if layout.left.len() >= 2 {
            &layout.left
        } else {
            &layout.right
        };
        let (p0x, p0y) = side[0].pad.center();
        let (_, p1y) = side[1].pad.center();
        let ux = p0x - 12.0;
        rsx! {
            path {
                class: "lpb-anat-shape lpb-anat-solid",
                d: "M {ux - 3.0} {p0y} L {ux + 3.0} {p0y} M {ux} {p0y} L {ux} {p1y} M {ux - 3.0} {p1y} L {ux + 3.0} {p1y}",
            }
            {note(ux - 10.0, p0y + u / 2.0 + 3.0, ux, (p0y + p1y) / 2.0, "u = pitch", "end")}
        }
    });

    let row_and_cell = layout
        .rail_rows()
        .find(|row| !row.cells.is_empty())
        .map(|row| {
            let cells_x0 = row
                .cells
                .iter()
                .map(|cell| cell.rect.x)
                .fold(f32::MAX, f32::min);
            let cells_x1 = row
                .cells
                .iter()
                .map(|cell| cell.rect.right())
                .fold(f32::MIN, f32::max);
            let x0 = cells_x0.min(row.pad.x) - 2.0;
            let x1 = cells_x1.max(row.pad.right()) + 2.0;
            let y0 = row.pad.center().1 - u / 2.0;
            let (cell_cx, cell_cy) = row.cells.first().unwrap().rect.center();
            rsx! {
                rect {
                    class: "lpb-anat-shape",
                    x: "{x0}",
                    y: "{y0}",
                    width: "{x1 - x0}",
                    height: "{u}",
                    rx: "4",
                }
                {note(x1 + 20.0, row.pad.y - 12.0, x1 + 1.0, y0 + 2.0, "Row (1u)", "start")}
                {note(cell_cx + 10.0, cell_cy + 24.0, cell_cx, cell_cy + 4.0, "Cell", "start")}
            }
        });

    let pad_note = rail.last().map(|row| {
        let (pad_cx, pad_cy) = row.pad.center();
        rsx! {
            {note(pad_cx + 24.0, pad_cy + 14.0, pad_cx + 3.0, pad_cy + 2.0, "Pad", "start")}
        }
    });

    let band_bracket = (!layout.band.is_empty()).then(|| {
        let x = layout
            .band
            .iter()
            .filter_map(|row| row.cells.last())
            .map(|cell| cell.rect.right())
            .fold(f32::MIN, f32::max)
            + 6.0;
        let y0 = layout
            .band
            .iter()
            .filter_map(|row| row.cells.first())
            .map(|cell| cell.rect.y)
            .fold(f32::MAX, f32::min)
            - 2.0;
        let y1 = layout
            .band
            .iter()
            .filter_map(|row| row.cells.first())
            .map(|cell| cell.rect.bottom())
            .fold(f32::MIN, f32::max)
            + 2.0;
        let mid = (y0 + y1) / 2.0 + 3.0;
        rsx! {
            {bracket(x, y0, y1, -1.0)}
            {note(x + 16.0, mid, x + 1.0, mid, "Band", "start")}
        }
    });

    let margin = DiagramMargin {
        top: 8.0,
        right: 92.0,
        bottom: 12.0,
        left: 44.0,
    };
    let overlay = rsx! {
        {pitch_ticks}
        {rail_bracket}
        {row_and_cell}
        {pad_note}
        {band_bracket}
    };
    (margin, overlay)
}
