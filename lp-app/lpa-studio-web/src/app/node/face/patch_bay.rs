//! The two-sided patch bay, read-only (D34a).
//!
//! One row primitive, two orderings. A **cell** is one
//! `(fixture span ↔ wire span)` run, drawn as a bordered box carrying the
//! live pixels of the lamps it covers and a label naming the OTHER side:
//!
//! - the output card lays its cells along the wire, one row per port, and
//!   labels them with the fixture (`peach_body 22–43 ◀`);
//! - the fixture card lays the SAME cells along its own channel space and
//!   labels them with the wire (`→ ch34 ◀`).
//!
//! Cells are absolutely positioned as percentages of their row, so the row
//! itself is the scale: a body split across two stretches of strand reads
//! as two boxes with dark track between them, and reads unbroken on its own
//! card. Nothing is drawn for a gap — the track showing through IS the gap,
//! which is also the only honest thing to say about lamps nobody drives.
//!
//! **Colour rules.** Cell borders take fixture-neutral chrome or the error
//! token when contested; violet is never used here (it means bound/bus).
//! The pixels themselves come from the output's own published frame,
//! decoded by [`control_rgb_at_sample`] — the same decode the lamp field
//! uses, so a cell and the card above it can never disagree about a colour.
//!
//! **Twin hover.** Hovering a cell lights its twin on the other face, keyed
//! by the run's id. The hovered id lives in a shared signal provided by the
//! node workspace; a bay rendered outside one (stories, docs embeds)
//! provides its own, which degrades to per-face hover with no other change.
//!
//! Read-only by design this slice: no click target, no drag, no write.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{UiControlProductPreview, UiFixturePatch, UiPatchBay, UiPatchCell};
use wasm_bindgen::{Clamped, JsCast};

use crate::app::node::face::NodeCardSection;
use crate::app::node::lamp_view::{UNLIT_RGB, control_rgb_at_sample};

/// Monotonic cell-canvas element ids (one per mounted strip).
static NEXT_CELL_CANVAS_ID: AtomicU64 = AtomicU64::new(0);

/// The cell whose twin is currently lit, shared across cards.
///
/// A `Signal` in a context rather than per-card state: the two faces are
/// different cards, and the whole point of the twin is that they light
/// together. Provided by the node workspace; a bay without one stands up
/// its own (per-face hover, the documented fallback).
#[derive(Clone, Copy)]
pub struct HoveredPatchCell(pub Signal<Option<String>>);

/// The shared hover signal, or a local one when nothing provided it.
fn hovered_cell() -> Signal<Option<String>> {
    match try_consume_context::<HoveredPatchCell>() {
        Some(shared) => shared.0,
        None => use_context_provider(|| HoveredPatchCell(Signal::new(None))).0,
    }
}

/// The OUTPUT card's bay: one row per port, cells along the wire.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PatchBaySection(bay: UiPatchBay) -> Element {
    let hovered = hovered_cell();
    let frame = bay.frame.clone();
    let summary = bay_summary(&bay);

    rsx! {
        NodeCardSection { label: "patch",
            div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
                span { class: bay_summary_class(&bay), "{summary}" }
                for port in bay.ports.clone() {
                    div { key: "{port.key}", class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        span { class: "tw:font-mono tw:text-[0.6rem] tw:text-subtle-foreground",
                            {port_caption(&port)}
                        }
                        PatchRow {
                            start: port.start,
                            lamps: port.lamps,
                            cells: port.cells.clone(),
                            frame: frame.clone(),
                            along_wire: true,
                            hovered,
                        }
                    }
                }
            }
        }
    }
}

/// The FIXTURE card's row: the same cells, along the fixture's own space.
///
/// Not a section of its own — it rides inside the output section the
/// fixture already has, under the lamp hero. The smallest honest placement:
/// a card whose whole top is "what this fixture is showing" is exactly
/// where "and where it landed on the wire" belongs.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FixturePatchRow(patch: UiFixturePatch) -> Element {
    let hovered = hovered_cell();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5 tw:px-3 tw:pb-2",
            span { class: "tw:font-mono tw:text-[0.6rem] tw:text-subtle-foreground",
                {fixture_caption(&patch)}
            }
            PatchRow {
                start: 0,
                lamps: patch.lamps,
                cells: patch.cells.clone(),
                frame: patch.frame.clone(),
                along_wire: false,
                hovered,
            }
            if !patch.single_output {
                span { class: "tw:font-mono tw:text-[0.6rem] tw:text-status-attention-foreground",
                    "this fixture drives more than one output — the pixels below are the first wire's"
                }
            }
        }
    }
}

/// One track with its cells positioned across it.
///
/// `start`/`lamps` are the row's own span — a port's stretch of wire, or a
/// fixture's whole channel space — and every cell's position is a
/// percentage of it, which is what makes the two faces the same primitive.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchRow(
    start: u32,
    lamps: u32,
    cells: Vec<UiPatchCell>,
    frame: Option<UiControlProductPreview>,
    /// The row runs along the WIRE (output side) rather than along the
    /// producer's own channels (fixture side). It decides both the label's
    /// context and which end of a reversed cell is drawn first.
    along_wire: bool,
    hovered: Signal<Option<String>>,
) -> Element {
    rsx! {
        div { class: "tw:relative tw:h-8 tw:w-full tw:overflow-hidden tw:rounded-xs tw:bg-track",
            for cell in cells {
                PatchCellBox {
                    key: "{cell.id}-{cell.wire_start}",
                    cell: cell.clone(),
                    row_start: start,
                    row_lamps: lamps,
                    frame: frame.clone(),
                    along_wire,
                    hovered,
                }
            }
        }
    }
}

/// One cell: a bordered box of live pixels over a label.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchCellBox(
    cell: UiPatchCell,
    row_start: u32,
    row_lamps: u32,
    frame: Option<UiControlProductPreview>,
    along_wire: bool,
    hovered: Signal<Option<String>>,
) -> Element {
    let offset = if along_wire {
        cell.wire_start.saturating_sub(row_start)
    } else {
        cell.source_start
    };
    let (left, width) = span_percent(offset, cell.lamps, row_lamps);
    let lit = hovered.read().as_deref() == Some(cell.id.as_str());
    let label = if along_wire {
        wire_side_label(&cell)
    } else {
        fixture_side_label(&cell)
    };
    let title = cell_title(&cell);
    let enter_id = cell.id.clone();

    rsx! {
        div {
            class: cell_class(cell.contested, lit),
            style: "left: {left:.4}%; width: {width:.4}%;",
            title: "{title}",
            onmouseenter: move |_| {
                let mut hovered = hovered;
                hovered.set(Some(enter_id.clone()));
            },
            onmouseleave: move |_| {
                let mut hovered = hovered;
                hovered.set(None);
            },
            PatchCellStrip { cell: cell.clone(), frame, along_wire }
            span {
                class: if cell.contested {
                    "tw:block tw:truncate tw:px-1 tw:font-mono tw:text-[0.6rem] tw:leading-[1.25] tw:text-status-error-foreground"
                } else {
                    "tw:block tw:truncate tw:px-1 tw:font-mono tw:text-[0.6rem] tw:leading-[1.25] tw:text-dim-foreground"
                },
                "{label}"
            }
        }
    }
}

/// The cell's live pixels: one canvas texel per lamp, CSS-scaled.
///
/// One texel per lamp and `image-rendering: pixelated` — a cell is a
/// picture of DISCRETE lamps, and smoothing them would draw a gradient the
/// strip is not showing. Reuses `ux-produced-product-pixel-canvas` so the
/// story-capture ready-wait (which polls that class for
/// `data-preview-painted`) covers these strips with no script change.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchCellStrip(
    cell: UiPatchCell,
    frame: Option<UiControlProductPreview>,
    along_wire: bool,
) -> Element {
    let canvas_id = use_hook(|| {
        let id = NEXT_CELL_CANVAS_ID.fetch_add(1, Ordering::Relaxed);
        format!("patch-cell-canvas-{id}")
    });
    let painted = use_hook(|| Rc::new(RefCell::new(None::<CellPaintKey>)));
    let texels = cell.lamps.max(1);
    let key = CellPaintKey {
        revision: frame.as_ref().map_or(-1, |frame| frame.revision),
        bytes: frame
            .as_ref()
            .map_or(0, |frame| frame.bytes.as_ptr() as usize),
        wire_start: cell.wire_start,
        lamps: cell.lamps,
        reversed: cell.reversed,
        contested: cell.contested,
        along_wire,
    };

    if *painted.borrow() != Some(key) {
        let canvas_id = canvas_id.clone();
        let painted = painted.clone();
        let cell = cell.clone();
        let frame = frame.clone();
        spawn(async move {
            match paint_cell_strip(&canvas_id, &cell, frame.as_ref(), along_wire) {
                Ok(()) => *painted.borrow_mut() = Some(key),
                Err(error) => log::debug!("patch cell paint skipped: {error}"),
            }
        });
    }

    let mount_id = canvas_id.clone();
    let mount_painted = painted.clone();
    let mount_cell = cell.clone();
    let mount_frame = frame.clone();
    rsx! {
        // `ux-produced-product-pixel-canvas` positions absolutely and fills
        // its container (that is what makes it a product preview's
        // full-bleed frame), so the strip supplies the sized, relative box
        // it fills — the same arrangement `GradientStripCanvas` uses.
        div { class: "tw:relative tw:h-3.5 tw:w-full",
            canvas {
                id: "{canvas_id}",
                class: "ux-produced-product-pixel-canvas",
                width: "{texels}",
                height: "1",
                onmounted: move |_| {
                    let canvas_id = mount_id.clone();
                    let painted = mount_painted.clone();
                    let cell = mount_cell.clone();
                    let frame = mount_frame.clone();
                    spawn(async move {
                        match paint_cell_strip(&canvas_id, &cell, frame.as_ref(), along_wire) {
                            Ok(()) => *painted.borrow_mut() = Some(key),
                            Err(error) => log::debug!("patch cell mount paint failed: {error}"),
                        }
                    });
                },
            }
        }
    }
}

/// Everything a cell's paint depends on. The frame is keyed by revision AND
/// buffer pointer, the same pair the lamp canvas uses: two products can
/// share a revision, and a fresh `Rc` is what a genuinely new frame carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CellPaintKey {
    revision: i64,
    bytes: usize,
    wire_start: u32,
    lamps: u32,
    reversed: bool,
    contested: bool,
    along_wire: bool,
}

// -- painting ------------------------------------------------------------------

fn paint_cell_strip(
    canvas_id: &str,
    cell: &UiPatchCell,
    frame: Option<&UiControlProductPreview>,
    along_wire: bool,
) -> Result<(), String> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| format!("canvas #{canvas_id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element #{canvas_id} is not a canvas"))?;
    let texels = cell.lamps.max(1);
    if canvas.width() != texels || canvas.height() != 1 {
        canvas.set_width(texels);
        canvas.set_height(1);
    }
    let context = canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "canvas has no 2d context".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has an unexpected type".to_string())?;

    let rgba = cell_strip_rgba(cell, frame, along_wire);
    let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), texels, 1)
        .map_err(|error| format!("build ImageData: {error:?}"))?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|error| format!("putImageData: {error:?}"))?;
    canvas
        .set_attribute("data-preview-painted", "1")
        .map_err(|error| format!("mark painted: {error:?}"))?;
    Ok(())
}

/// One RGBA quad per lamp of the cell, in the ROW's reading order.
///
/// The output row reads along the wire, so texel `j` is wire lamp
/// `wire_start + j`. The fixture row reads along the producer's own
/// channels, so texel `j` is the fixture's channel `source_start + j` —
/// whose wire lamp runs BACKWARDS when the run is reversed. That mapping is
/// the whole reason a reversed strand looks continuous on its own card and
/// end-first on the wire.
fn cell_strip_rgba(
    cell: &UiPatchCell,
    frame: Option<&UiControlProductPreview>,
    along_wire: bool,
) -> Vec<u8> {
    let texels = cell.lamps.max(1) as usize;
    let mut rgba = Vec::with_capacity(texels * 4);
    for texel in 0..texels {
        let index = texel as u32;
        let wire_lamp = if along_wire || !cell.reversed {
            cell.wire_start.saturating_add(index)
        } else {
            cell.wire_start
                .saturating_add(cell.lamps.saturating_sub(1).saturating_sub(index))
        };
        let rgb = frame
            .and_then(|frame| control_rgb_at_sample(frame, wire_lamp.saturating_mul(3)))
            .unwrap_or(UNLIT_RGB);
        rgba.extend_from_slice(&rgb);
        rgba.push(255);
    }
    rgba
}

// -- derivations ---------------------------------------------------------------

/// A span's `(left, width)` as percentages of a row of `total` lamps.
///
/// A zero-lamp row cannot place anything, and a zero-lamp cell would
/// collapse to an invisible sliver — both degrade to a hairline rather than
/// to a division by zero.
fn span_percent(offset: u32, lamps: u32, total: u32) -> (f32, f32) {
    if total == 0 {
        return (0.0, 100.0);
    }
    let total = total as f32;
    let left = (offset as f32 / total * 100.0).clamp(0.0, 100.0);
    let width = (lamps as f32 / total * 100.0).clamp(0.35, 100.0 - left);
    (left, width)
}

/// The output side's label: which fixture, and which of ITS lamps.
fn wire_side_label(cell: &UiPatchCell) -> String {
    let span = match cell.source_end() {
        Some(end) if end > cell.source_start => format!("{}–{end}", cell.source_start),
        Some(end) => format!("{end}"),
        None => "empty".to_string(),
    };
    format!("{} {span}{}", cell.producer, direction_mark(cell))
}

/// The fixture side's label: where on the wire this run landed.
fn fixture_side_label(cell: &UiPatchCell) -> String {
    if cell.port_label.is_empty() {
        format!("→ ch{}{}", cell.wire_start, direction_mark(cell))
    } else {
        format!(
            "→ {} ch{}{}",
            cell.port_label,
            cell.wire_start,
            direction_mark(cell)
        )
    }
}

/// Which way the run is laid down. The forward mark is deliberately drawn
/// too: an arrow that only appears when something is unusual reads as a
/// warning, and a reversed strand is an ordinary, authored fact.
fn direction_mark(cell: &UiPatchCell) -> &'static str {
    if cell.reversed { " ◀" } else { " ▶" }
}

/// The hover title — the whole tuple in words, both sides at once, since
/// each face's label can only show one of them.
fn cell_title(cell: &UiPatchCell) -> String {
    let wire_end = cell.wire_start.saturating_add(cell.lamps.max(1)) - 1;
    let source_end = cell.source_end().unwrap_or(cell.source_start);
    let mut title = format!(
        "{} lamps {}–{} → {} channels {}–{}",
        cell.producer, cell.source_start, source_end, cell.output_label, cell.wire_start, wire_end
    );
    if let Some(port) = cell.port_key {
        title.push_str(&format!(" (port ch{port})"));
    }
    if cell.reversed {
        title.push_str("\nlaid onto the wire end-first");
    }
    if cell.contested {
        title.push_str(
            "\nsome of these channels are claimed by another strand too — the engine darkens them",
        );
    }
    title
}

/// The port's caption: its name, its pin, and the stretch of wire it drives.
fn port_caption(port: &lpa_studio_core::UiPatchPort) -> String {
    let pin = if port.pin_label.is_empty() {
        String::new()
    } else {
        format!(" · {}", port.pin_label)
    };
    if port.lamps == 0 {
        return format!("ch{}{pin} · no lamps", port.key);
    }
    format!(
        "ch{}{pin} · channels {}–{}",
        port.key,
        port.start,
        port.start + port.lamps - 1
    )
}

/// The fixture row's caption: how its own channel space arrives.
fn fixture_caption(patch: &UiFixturePatch) -> String {
    let runs = patch.cells.len();
    if patch.is_single_run() {
        return format!("{} lamps · one run", patch.lamps);
    }
    format!("{} lamps · {runs} runs", patch.lamps)
}

/// The bay's own summary: what the wire adds up to, and what it does not.
fn bay_summary(bay: &UiPatchBay) -> String {
    let cells: usize = bay.ports.iter().map(|port| port.cells.len()).sum();
    let noun = if cells == 1 { "cell" } else { "cells" };
    let mut summary = format!("{cells} {noun}");
    if bay.contested_lamps > 0 {
        summary.push_str(&format!(" · {} contested", bay.contested_lamps));
    }
    if bay.gap_lamps > 0 {
        summary.push_str(&format!(" · {} unclaimed", bay.gap_lamps));
    }
    summary
}

/// Contested lamps are an ERROR (the engine darkens them and says so);
/// unclaimed ones are only attention — a wire nobody drives is a normal
/// state of a project being built.
fn bay_summary_class(bay: &UiPatchBay) -> &'static str {
    if bay.contested_lamps > 0 {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:text-status-error-foreground"
    } else if bay.gap_lamps > 0 {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:text-status-attention-foreground"
    } else {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground"
    }
}

/// The cell's chrome. Fixture-neutral at rest (violet is bound/bus and
/// means something else here), the error token when contested, and a
/// strong-border lift when this cell — or its twin on the other face — is
/// hovered.
fn cell_class(contested: bool, lit: bool) -> &'static str {
    match (contested, lit) {
        (true, true) => {
            "tw:absolute tw:top-0.5 tw:bottom-0.5 tw:overflow-hidden tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:ring-1 tw:ring-border-strong"
        }
        (true, false) => {
            "tw:absolute tw:top-0.5 tw:bottom-0.5 tw:overflow-hidden tw:rounded-xs tw:border tw:border-status-error-border tw:bg-card-muted"
        }
        (false, true) => {
            "tw:absolute tw:top-0.5 tw:bottom-0.5 tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-raised tw:ring-1 tw:ring-border-strong"
        }
        (false, false) => {
            "tw:absolute tw:top-0.5 tw:bottom-0.5 tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-muted"
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiPatchBay, UiPatchCell, UiPatchPort};

    use super::{
        UiFixturePatch, bay_summary, cell_class, fixture_caption, fixture_side_label, port_caption,
        span_percent, wire_side_label,
    };

    fn cell(
        producer: &str,
        source_start: u32,
        lamps: u32,
        wire_start: u32,
        reversed: bool,
    ) -> UiPatchCell {
        UiPatchCell {
            id: format!("2:0:{source_start}:{wire_start}"),
            producer: producer.to_string(),
            source_start,
            lamps,
            wire_start,
            reversed,
            output_label: "Output".to_string(),
            ..UiPatchCell::default()
        }
    }

    #[test]
    fn each_face_labels_the_other_side() {
        let mut run = cell("peach_body", 22, 22, 34, true);
        assert_eq!(wire_side_label(&run), "peach_body 22–43 ◀");
        assert_eq!(fixture_side_label(&run), "→ ch34 ◀");
        run.port_label = "D10".to_string();
        assert_eq!(
            fixture_side_label(&run),
            "→ D10 ch34 ◀",
            "the pin joins the label once a project has more than one port to confuse"
        );
        let forward = cell("peach_leaf", 0, 12, 22, false);
        assert_eq!(wire_side_label(&forward), "peach_leaf 0–11 ▶");
    }

    /// A one-lamp run must not read `7–7`, and an empty one must not read
    /// `7–6`: both are shapes the engine can plan.
    #[test]
    fn degenerate_runs_get_honest_labels() {
        assert_eq!(wire_side_label(&cell("strip", 7, 1, 0, false)), "strip 7 ▶");
        assert_eq!(
            wire_side_label(&cell("strip", 7, 0, 0, false)),
            "strip empty ▶"
        );
    }

    #[test]
    fn cells_are_positioned_as_percentages_of_their_row() {
        assert_eq!(span_percent(0, 22, 56), (0.0, 22.0 / 56.0 * 100.0));
        assert_eq!(
            span_percent(34, 22, 56),
            (34.0 / 56.0 * 100.0, 22.0 / 56.0 * 100.0)
        );
        // A zero-lamp cell keeps a hairline rather than vanishing, and an
        // empty row places its cell over the whole track.
        assert!(span_percent(0, 0, 56).1 > 0.0);
        assert_eq!(span_percent(0, 0, 0), (0.0, 100.0));
    }

    #[test]
    fn a_cell_never_overflows_its_row() {
        let (left, width) = span_percent(50, 20, 56);
        assert!(left + width <= 100.0001, "{left} + {width}");
    }

    #[test]
    fn the_summary_names_contested_and_unclaimed_lamps() {
        let mut bay = UiPatchBay {
            ports: vec![UiPatchPort {
                cells: vec![cell("body", 0, 22, 0, false)],
                ..UiPatchPort::default()
            }],
            ..UiPatchBay::default()
        };
        assert_eq!(bay_summary(&bay), "1 cell");
        assert!(!bay_summary_class_is_error(&bay));
        bay.gap_lamps = 2;
        assert_eq!(bay_summary(&bay), "1 cell · 2 unclaimed");
        bay.contested_lamps = 4;
        assert_eq!(bay_summary(&bay), "1 cell · 4 contested · 2 unclaimed");
        assert!(bay_summary_class_is_error(&bay));
    }

    fn bay_summary_class_is_error(bay: &UiPatchBay) -> bool {
        super::bay_summary_class(bay).contains("status-error")
    }

    /// Violet is bound/bus everywhere else in Studio; a cell must never
    /// borrow it, hovered or not.
    #[test]
    fn no_cell_state_wears_the_bound_violet() {
        for contested in [false, true] {
            for lit in [false, true] {
                let class = cell_class(contested, lit);
                assert!(!class.contains("status-bound"), "{class}");
            }
        }
    }

    #[test]
    fn the_captions_read_the_two_spans() {
        assert_eq!(
            port_caption(&UiPatchPort {
                key: 0,
                pin_label: "D10".to_string(),
                start: 0,
                lamps: 56,
                cells: Vec::new(),
            }),
            "ch0 · D10 · channels 0–55"
        );
        let single = UiFixturePatch {
            lamps: 12,
            cells: vec![cell("leaf", 0, 12, 22, false)],
            frame: None,
            single_output: true,
        };
        assert_eq!(fixture_caption(&single), "12 lamps · one run");
        let split = UiFixturePatch {
            lamps: 44,
            cells: vec![
                cell("body", 0, 22, 0, false),
                cell("body", 22, 22, 34, true),
            ],
            ..single
        };
        assert_eq!(fixture_caption(&split), "44 lamps · 2 runs");
    }
}
