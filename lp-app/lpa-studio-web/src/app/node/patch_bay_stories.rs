//! Stories for the two-sided patch bay (D34a).
//!
//! The bay is the first Studio surface whose whole point is a DISAGREEMENT
//! between two views of one thing, so the coverage is deliberately paired:
//! the peach's wire from the output card and the same peach's body from its
//! fixture card, side by side in the story list. Everything else is the
//! states that must not be silently pretty — an overlap, an unpatched
//! fixture, and the 2D-product variant that proves patching is
//! dimension-agnostic (D19v: the cells do not change).
//!
//! The fixtures here are hand-built DTOs, like every other face story. The
//! real derivation (`patch_bay_derivation` in `lpa-studio-core`) is covered
//! by its own unit tests and by an end-to-end walk over the running peach;
//! what these capture is the LOOK.

use dioxus::prelude::*;
use lpa_studio_core::{
    ColorOrder, ControlExtent, ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan,
    UiControlProductPreview, UiControlSampleFormat, UiFixturePatch, UiPatchBay, UiPatchCell,
    UiPatchPort,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{
    fixture_face, fyeah_presentable_doc, map2d_fixture_face, output_channel, output_face,
    output_node_view,
};
use crate::app::node::{FixtureFace, NodePane};

/// The peach's wire: 44 body lamps and 12 leaf lamps on one 56-lamp strand.
const WIRE_LAMPS: u32 = 56;
const BODY_LAMPS: u32 = 44;
const LEAF_LAMPS: u32 = 12;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

/// One run, as both faces read it.
fn cell(
    id: &str,
    producer: &str,
    source_start: u32,
    lamps: u32,
    wire_start: u32,
    reversed: bool,
) -> UiPatchCell {
    UiPatchCell {
        id: id.to_string(),
        producer: producer.to_string(),
        producer_node: Some(format!("/peach_1d.show/{producer}.fixture")),
        source_start,
        lamps,
        wire_start,
        reversed,
        contested: false,
        port_key: Some(0),
        port_label: "LED1".to_string(),
        output_label: "Strips".to_string(),
    }
}

/// The peach's shipped patch: the body's first half, the leaf anchored
/// between the halves, the body's second half laid back down the strand.
fn peach_cells() -> Vec<UiPatchCell> {
    vec![
        cell("2:0:0:0", "peach_body", 0, 22, 0, false),
        cell("3:0:0:22", "peach_leaf", 0, 12, 22, false),
        cell("2:0:22:34", "peach_body", 22, 22, 34, true),
    ]
}

/// The same peach with the leaf slid four lamps early: it now shares
/// channels 30–33 with the body's first stretch, and the strand's last two
/// channels reach nothing.
fn overlapped_cells() -> Vec<UiPatchCell> {
    let mut cells = vec![
        cell("2:0:0:0", "peach_body", 0, 34, 0, false),
        cell("3:0:0:30", "peach_leaf", 0, 12, 30, false),
        cell("2:0:34:44", "peach_body", 34, 10, 44, true),
    ];
    cells[0].contested = true;
    cells[1].contested = true;
    cells
}

/// The output card's bay for a set of cells on the peach's one port.
fn peach_bay(cells: Vec<UiPatchCell>, contested_lamps: u32, gap_lamps: u32) -> UiPatchBay {
    UiPatchBay {
        ports: vec![UiPatchPort {
            key: 0,
            pin_label: "LED1".to_string(),
            start: 0,
            lamps: WIRE_LAMPS,
            cells: cells.clone(),
        }],
        frame: Some(wire_frame(&cells)),
        contested_lamps,
        gap_lamps,
    }
}

/// The body's own row: two runs, unbroken in its own numbering.
fn body_patch() -> UiFixturePatch {
    UiFixturePatch {
        lamps: BODY_LAMPS,
        cells: peach_cells()
            .into_iter()
            .filter(|cell| cell.producer == "peach_body")
            .collect(),
        frame: Some(wire_frame(&peach_cells())),
        single_output: true,
    }
}

/// The leaf's own row: one run, anchored at channel 22.
fn leaf_patch() -> UiFixturePatch {
    UiFixturePatch {
        lamps: LEAF_LAMPS,
        cells: peach_cells()
            .into_iter()
            .filter(|cell| cell.producer == "peach_leaf")
            .collect(),
        frame: Some(wire_frame(&peach_cells())),
        single_output: true,
    }
}

/// A 241-lamp strip with no patch: one run over the whole wire.
fn auto_flow_cell() -> UiPatchCell {
    let mut cell = cell("2:0:0:0", "halo", 0, 241, 0, false);
    cell.producer_node = Some("/fyeah_sign.show/halo.fixture".to_string());
    cell
}

/// The peach's wire as the output published it: every lamp wearing the
/// colour its own fixture rendered, at the channel the patch put it on.
///
/// Built from the SAME cells the boxes describe, so the pixels and the
/// boxes cannot disagree — a leaf drawn outside the channels its cell
/// claims would be a bug in the story, not a prettier picture. Contested
/// channels go dark, because that is what the engine actually publishes:
/// two strands claiming one lamp means nobody's colour is right, so it
/// gets none.
fn wire_frame(cells: &[UiPatchCell]) -> UiControlProductPreview {
    let mut claims: Vec<u8> = vec![0; WIRE_LAMPS as usize];
    for cell in cells {
        for lamp in cell.wire_start..cell.wire_start + cell.lamps {
            if let Some(claim) = claims.get_mut(lamp as usize) {
                *claim += 1;
            }
        }
    }
    let mut samples: Vec<u16> = vec![0; (WIRE_LAMPS * 3) as usize];
    for cell in cells {
        let leaf = cell.producer == "peach_leaf";
        let span = if leaf { LEAF_LAMPS } else { BODY_LAMPS };
        for index in 0..cell.lamps {
            let wire = if cell.reversed {
                cell.wire_start + cell.lamps - 1 - index
            } else {
                cell.wire_start + index
            };
            if claims.get(wire as usize).copied().unwrap_or(0) > 1 {
                continue;
            }
            let channel = cell.source_start + index;
            let t = channel as f32 / (span.max(2) - 1) as f32;
            let rgb = if leaf { leaf_rgb(t) } else { body_rgb(t) };
            let base = (wire * 3) as usize;
            samples[base..base + 3].copy_from_slice(&rgb);
        }
    }
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in &samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    UiControlProductPreview {
        revision: 104,
        extent: ControlExtent::new(1, WIRE_LAMPS * 3),
        sample_format: UiControlSampleFormat::U16,
        sample_layout: ControlSampleLayout {
            spans: vec![ControlSampleSpan {
                row: 0,
                start: 0,
                len: WIRE_LAMPS * 3,
                encoding: ControlSampleEncoding::RgbPixels {
                    count: WIRE_LAMPS,
                    color_order: ColorOrder::Rgb,
                },
            }],
        },
        display_layout: None,
        bytes: bytes.into(),
    }
}

/// Peach flesh: warm pink deepening along the fruit.
fn body_rgb(t: f32) -> [u16; 3] {
    linear([0.95 - 0.25 * t, 0.34 + 0.12 * t, 0.30 + 0.22 * t])
}

/// Leaf: green, brightening toward the tip.
fn leaf_rgb(t: f32) -> [u16; 3] {
    linear([0.10 + 0.10 * t, 0.55 + 0.35 * t, 0.20 + 0.15 * t])
}

fn linear(rgb: [f32; 3]) -> [u16; 3] {
    rgb.map(|channel| (channel.clamp(0.0, 1.0) * 65535.0) as u16)
}

#[story(
    description = "The output card's patch bay: one row per port, cells laid along the wire, each labelled with the FIXTURE it came from. The peach's strand reads body 0–21, then the leaf anchored between the halves, then the body's second half laid back down the strand (◀). Each cell carries the live lamps of the channels it covers, so a strand plugged in at the wrong end is visible as a picture, not only as a number. Gaps would show as dark track; nothing is drawn for lamps nobody drives."
)]
fn output_face_patched() -> Element {
    let mut face = output_face(
        Some("quinled/dig-uno"),
        vec![output_channel(0, "LED1", Some(WIRE_LAMPS))],
        Some(WIRE_LAMPS),
        Vec::new(),
    );
    face.patch = Some(peach_bay(peach_cells(), 0, 0));
    rsx! {
        PatchCardCanvas {
            NodePane {
                view: output_node_view(face, "1 wire · 56 lamps"),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The SAME cells from the fixture's end: the body's two stretches of strand, laid along the body's own channel space, where they read as one unbroken 0–43. Labels carry the wire instead of the fixture ('→ D10 ch34 ◀'). That difference between the two rows IS the patch — and it is why both faces exist rather than one."
)]
fn fixture_face_patched_body() -> Element {
    let mut face = fixture_face();
    face.patch = Some(body_patch());
    rsx! {
        PatchCardCanvas {
            FixtureFace { face, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "The leaf's end of the same patch: one run, whole — anchored at channel 22, between the body's halves. From this side an anchor and plain auto-flow look identical, honestly so: what the fixture can say is that its own space arrives in one piece; WHERE it landed is the label's business."
)]
fn fixture_face_patched_leaf() -> Element {
    let mut face = fixture_face();
    face.patch = Some(leaf_patch());
    rsx! {
        PatchCardCanvas {
            FixtureFace { face, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "Overlap: the leaf slid four channels early and now shares 30–33 with the body. Both contesting cells take the error token — the engine darkens contested samples and names the range on the output's status — and the summary counts them beside the two channels at the far end that nothing reaches. Contested is an error (something is wrong); unclaimed is only attention (a project being built)."
)]
fn output_face_overlap() -> Element {
    let mut face = output_face(
        Some("quinled/dig-uno"),
        vec![output_channel(0, "LED1", Some(WIRE_LAMPS))],
        Some(WIRE_LAMPS),
        Vec::new(),
    );
    face.patch = Some(peach_bay(overlapped_cells(), 4, 2));
    rsx! {
        PatchCardCanvas {
            NodePane {
                view: output_node_view(face, "1 wire · 56 lamps"),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The ordinary project, for scale: one fixture, no patch document, one run taking the whole 241-lamp wire. The bay is still worth drawing — it is where 'this fixture is the whole strand' stops being an assumption — but it says one thing and says it quietly."
)]
fn output_face_auto_flow() -> Element {
    let mut face = output_face(
        Some("quinled/dig-uno"),
        vec![output_channel(0, "LED1", None)],
        Some(241),
        Vec::new(),
    );
    face.patch = Some(UiPatchBay {
        ports: vec![UiPatchPort {
            key: 0,
            pin_label: "LED1".to_string(),
            start: 0,
            lamps: 241,
            cells: vec![auto_flow_cell()],
        }],
        // No frame: an output nothing has published yet still draws its
        // geometry, in the unlit neutral, rather than a black lie.
        frame: None,
        contested_lamps: 0,
        gap_lamps: 0,
    });
    rsx! {
        PatchCardCanvas {
            NodePane {
                view: output_node_view(face, "1 wire · 241 lamps"),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The unpatched fixture's own row: one auto-flow cell taking the whole wire, from channel 0. Nothing here says 'unpatched' — from the fixture's end, an anchor naming channel 0 and auto-flow landing on it are the same run, and claiming otherwise would be a guess. What the row states is what it knows: 241 lamps, one run, starting at ch0."
)]
fn fixture_face_auto_flow() -> Element {
    let mut face = fixture_face();
    face.patch = Some(UiFixturePatch {
        lamps: 241,
        cells: vec![auto_flow_cell()],
        frame: None,
        single_output: true,
    });
    rsx! {
        PatchCardCanvas {
            FixtureFace { face, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "The 2D-product variant (D19v): the same fixture rendering a two-dimensional product — a mapped shape rather than a strip — carrying the SAME patch row, unchanged. Patching is dimension-agnostic: it is about which lamps land where on a wire, and a fixture's sampling dimension has nothing to say about that."
)]
fn fixture_face_two_d_product() -> Element {
    let doc = fyeah_presentable_doc();
    let mut face = map2d_fixture_face(&doc);
    face.patch = Some(body_patch());
    rsx! {
        PatchCardCanvas {
            FixtureFace { face, on_action: move |_| {} }
        }
    }
}
