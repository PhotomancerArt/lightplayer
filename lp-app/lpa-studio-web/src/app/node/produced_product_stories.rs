//! Stories for produced product views.

use dioxus::prelude::*;
use lpa_studio_core::{
    ColorOrder, ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d,
    ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, Revision,
    UiControlProductPreview, UiControlSampleFormat, UiProducedProduct, UiProductPreview,
    UiProductTrackingState,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::lamp_view::LampView;
use crate::app::node::node_story_fixtures::{
    control_preview_product, control_unsupported_product, produced_product_variants_fixture,
    visual_error_product, visual_preview_product,
};
use crate::app::node::{ProducedProductView, ProducedProducts};

#[story(description = "Produced product variants shown as a node pane section would render them.")]
pub(crate) fn gallery() -> Element {
    rsx! {
        ProducedProducts { products: produced_product_variants_fixture() }
    }
}

#[story(description = "An output slot that has not resolved to a product yet.")]
pub(crate) fn empty_product() -> Element {
    rsx! {
        ProducedProductView { product: UiProducedProduct::empty("output").with_detail("not resolved") }
    }
}

#[story(description = "A visual product that exists but is not being tracked.")]
pub(crate) fn visual_untracked() -> Element {
    rsx! {
        ProducedProductView { product: UiProducedProduct::visual("output").with_detail("32 x 32 preview") }
    }
}

#[story(description = "A visual product waiting for its first tracked preview.")]
pub(crate) fn visual_pending() -> Element {
    rsx! {
        ProducedProductView {
            product: UiProducedProduct::visual("output")
                .with_detail("32 x 32 preview")
                .with_preview(UiProductPreview::Pending)
                .with_tracking(UiProductTrackingState::Tracking)
        }
    }
}

/// P7 item 1. A time product wears the same product chip as the other two
/// families, and it is metadata-only BY DESIGN rather than by omission:
/// there is nothing to draw behind the handle, and the way to look at it is
/// the clock face's phasor listing. The copy must not report a gap.
#[story(
    description = "The time product a clock publishes on bus:time. Same chip as visual/control, no preview — the handle has no picture, and its detail line says what it is instead of 'Studio does not render this yet'."
)]
pub(crate) fn time_product() -> Element {
    rsx! {
        ProducedProductView {
            product: UiProducedProduct::time("product")
                .with_detail("node 2 output 0")
                .with_tracking(UiProductTrackingState::Tracking)
        }
    }
}

#[story(description = "A visual product with loaded RGB preview bytes.")]
pub(crate) fn visual_loaded() -> Element {
    rsx! {
        ProducedProductView { product: visual_preview_product("output") }
    }
}

#[story(description = "A visual product with cached preview bytes that is not being tracked now.")]
pub(crate) fn visual_paused() -> Element {
    rsx! {
        ProducedProductView {
            product: visual_preview_product("output")
                .with_tracking(UiProductTrackingState::Paused)
        }
    }
}

#[story(description = "A visual product whose preview probe failed.")]
pub(crate) fn visual_error() -> Element {
    rsx! {
        ProducedProductView { product: visual_error_product("output") }
    }
}

#[story(description = "An open produced product detail popup.")]
pub(crate) fn detail_popup() -> Element {
    let product = produced_product_variants_fixture().remove(3);

    rsx! {
        div { class: "tw:min-h-56",
            ProducedProductView {
                product,
                initially_open: true,
            }
        }
    }
}

/// The design-language pin for the lamp painter: the voronoi cells must
/// survive both authoring-scale extremes with no absolute clamps — the
/// two regressions the canvas round's G1 found live here as a picture.
#[story(
    description = "The lamp field's voronoi cells at both scale extremes. Left: a tight, jittered two-row field (fyeah-like — footprints wider than the pitch), whose cells stay one uniform mosaic instead of a size salad. Middle: a coarse, sparse arc (peach-like), whose cells still meet at the bisector instead of shrinking into separated balls under an absolute cap. Right: the same coarse field unfed — the neutral cell colour that reads as geometry. All lengths derive from each layout's own pitch and footprint; a % or px clamp would break one extreme or the other."
)]
pub(crate) fn lamp_cells_scale_extremes() -> Element {
    let tight = lamp_story_preview(
        200,
        60,
        // Two jittered rows, pitch ~8 of 200 wide — deterministic zigzag
        // jitter stands in for freehand authoring.
        &(0..48)
            .map(|i| {
                let (row, col) = (i / 24, i % 24);
                let wobble = match i % 4 {
                    0 => -1.4,
                    1 => 0.9,
                    2 => 1.3,
                    _ => -0.7,
                };
                [
                    (6.0 + col as f32 * 8.0 + wobble) / 200.0,
                    (16.0 + row as f32 * 26.0 + wobble * 0.8) / 60.0,
                ]
            })
            .collect::<Vec<_>>(),
        // Footprint diameter 12 in hint units vs pitch 8: the fyeah
        // regime — bulbs wider than their spacing.
        6.0 / 200.0,
    );
    let coarse_positions: Vec<[f32; 2]> = (0..9)
        .map(|i| {
            let sweep = (0.15 + i as f32 / 8.0 * 0.7) * core::f32::consts::PI;
            [0.5 - sweep.cos() * 0.42, 0.88 - sweep.sin() * 0.78]
        })
        .collect();
    // Pitch ~ 30 of a 200 x 100 hint space, footprint 13 — the peach
    // regime: the median pitch, not the footprint, sizes the cells.
    let coarse = lamp_story_preview(200, 100, &coarse_positions, 6.5 / 200.0);
    let unfed = coarse.clone();
    rsx! {
        div { class: "tw:flex tw:gap-4 tw:items-end",
            div { class: "tw:relative tw:w-72 tw:h-24",
                LampView { preview: tight }
            }
            div { class: "tw:relative tw:w-56 tw:h-28",
                LampView { preview: coarse }
            }
            div { class: "tw:relative tw:w-56 tw:h-28",
                LampView { preview: unfed, live: false }
            }
        }
    }
}

/// A deterministic control preview over `positions` (normalized), with a
/// fixed rainbow riding LINEAR unorm16 bytes — what the wire carries and
/// what `LampView` decodes (the roster ▶ stories' rule).
fn lamp_story_preview(
    width_hint: u32,
    height_hint: u32,
    positions: &[[f32; 2]],
    radius: f32,
) -> UiControlProductPreview {
    let count = positions.len() as u32;
    let mut lamps = Vec::with_capacity(positions.len());
    let mut bytes = Vec::with_capacity(positions.len() * 6);
    for (index, center) in positions.iter().enumerate() {
        lamps.push(ControlLamp2d {
            lamp_index: index as u32,
            sample_start: index as u32 * 3,
            center: *center,
            radius,
        });
        let phase = index as f32 / positions.len() as f32;
        for channel in 0..3_u32 {
            let turn = (phase + channel as f32 / 3.0) * core::f32::consts::TAU;
            let level = (turn.sin() * 0.5 + 0.5).powi(2);
            bytes.extend_from_slice(&((level * f32::from(u16::MAX)) as u16).to_le_bytes());
        }
    }
    UiControlProductPreview {
        revision: 7,
        extent: ControlExtent::new(1, count * 3),
        sample_format: UiControlSampleFormat::U16,
        sample_layout: ControlSampleLayout {
            spans: vec![ControlSampleSpan {
                row: 0,
                start: 0,
                len: count * 3,
                encoding: ControlSampleEncoding::RgbPixels {
                    count,
                    color_order: ColorOrder::Rgb,
                },
            }],
        },
        display_layout: Some(std::rc::Rc::new(ControlDisplayLayout::Layout2d(
            ControlLayout2d::new(Revision::new(7), width_hint, height_hint, lamps),
        ))),
        bytes: bytes.into(),
    }
}

#[story(description = "A control product that exists but is not being tracked.")]
pub(crate) fn control_untracked() -> Element {
    rsx! {
        ProducedProductView { product: UiProducedProduct::control("dmx").with_detail("16 RGB lamps") }
    }
}

#[story(description = "A control product waiting for its first tracked preview.")]
pub(crate) fn control_pending() -> Element {
    rsx! {
        ProducedProductView {
            product: UiProducedProduct::control("dmx")
                .with_detail("16 RGB lamps")
                .with_preview(UiProductPreview::Pending)
                .with_tracking(UiProductTrackingState::Tracking)
        }
    }
}

#[story(description = "A control product with native samples and a 2D display layout.")]
pub(crate) fn control_loaded() -> Element {
    rsx! {
        ProducedProductView { product: control_preview_product("dmx") }
    }
}

#[story(description = "A control product with cached preview bytes that is not being tracked now.")]
pub(crate) fn control_paused() -> Element {
    rsx! {
        ProducedProductView {
            product: control_preview_product("dmx")
                .with_tracking(UiProductTrackingState::Paused)
        }
    }
}

#[story(description = "A control product whose native samples cannot be shown as a 2D layout.")]
pub(crate) fn control_unsupported() -> Element {
    rsx! {
        ProducedProductView { product: control_unsupported_product("dmx") }
    }
}

#[story(description = "A control product whose preview probe failed.")]
pub(crate) fn control_error() -> Element {
    rsx! {
        ProducedProductView {
            product: UiProducedProduct::control("dmx")
                .with_detail("16 RGB lamps")
                .with_tracking(UiProductTrackingState::Tracking)
                .with_preview(UiProductPreview::Error {
                    message: "control probe failed".to_string(),
                })
        }
    }
}
