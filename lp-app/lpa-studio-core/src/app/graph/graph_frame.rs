//! What the board shows, as the Graph draws it: the lab's `BoardFrame`,
//! built from the frames the board itself published.
//!
//! Studio reads each output node's published frame on every project read
//! (`ProjectSync::output_frame`): the output's whole wire, one RGB triple
//! a lamp, after the board's own colour order, gamma and brightness — and
//! after its `highlight`, so a selection breathing on the board breathes
//! here too. A port is a window of that wire (the lab's flat wire: each
//! port starts where the one before it ends). The bytes ride in each
//! fixture's colour order, which the frame's layout says per run; a pixel
//! no run names (a free one, which only the highlight lights) is read as
//! RGB.

use std::collections::BTreeMap;

use lab_mapping_model::{BoardFrame, NodeId, Project, Rgb};

use crate::{ColorOrder, ControlSampleEncoding, UiControlProductPreview};

/// The board's frame for `project`, from the published frame of each of
/// its outputs (`outputs`: the lab's output node → Studio's node).
pub(crate) fn board_frame<'a>(
    project: &Project,
    outputs: &BTreeMap<NodeId, lpc_model::NodeId>,
    published: impl Fn(lpc_model::NodeId) -> Option<&'a UiControlProductPreview>,
) -> BoardFrame {
    project.board_frame_from_ports(|output, key| {
        let preview = published(*outputs.get(output)?)?;
        let start = project.output(output)?.port_start(key)?;
        let n = project.port_pixels(output, key);
        Some(
            (start..start + n)
                .map(|wire_lamp| wire_rgb(preview, wire_lamp).unwrap_or(Rgb::BLACK))
                .collect(),
        )
    })
}

/// One wire lamp as it looks, or `None` past what the board sent.
pub(crate) fn wire_rgb(preview: &UiControlProductPreview, wire_lamp: u32) -> Option<Rgb> {
    let at = wire_lamp.checked_mul(3)?;
    let sample = |offset: u32| -> Option<u8> {
        let index = at.checked_add(offset)? as usize;
        Some(lpc_wire::linear16_to_srgb8(preview.unorm16_sample(index)?))
    };
    let wire = [sample(0)?, sample(1)?, sample(2)?];
    let [r, g, b] = from_wire(order_at(preview, at), wire);
    Some(Rgb::new(r, g, b))
}

/// The colour order the frame's layout gives the run holding `sample`,
/// or RGB where no run does.
fn order_at(preview: &UiControlProductPreview, sample: u32) -> ColorOrder {
    preview
        .sample_layout
        .spans
        .iter()
        .find_map(|span| match span.encoding {
            ControlSampleEncoding::RgbPixels { color_order, .. }
                if sample >= span.start
                    && sample.saturating_add(3) <= span.start.saturating_add(span.len)
                    && (sample - span.start).is_multiple_of(3) =>
            {
                Some(color_order)
            }
            _ => None,
        })
        .unwrap_or(ColorOrder::Rgb)
}

/// Bytes in a strip's order, back to red, green, blue.
fn from_wire(order: ColorOrder, [a, b, c]: [u8; 3]) -> [u8; 3] {
    match order {
        ColorOrder::Rgb => [a, b, c],
        ColorOrder::Grb => [b, a, c],
        ColorOrder::Rbg => [a, c, b],
        ColorOrder::Gbr => [c, a, b],
        ColorOrder::Brg => [b, c, a],
        ColorOrder::Bgr => [c, b, a],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every order, written the product's way, reads back as the colour.
    #[test]
    fn every_colour_order_reads_back_as_the_colour() {
        for order in [
            ColorOrder::Rgb,
            ColorOrder::Grb,
            ColorOrder::Rbg,
            ColorOrder::Gbr,
            ColorOrder::Brg,
            ColorOrder::Bgr,
        ] {
            let mut wire = [0u8; 3];
            order.write_rgb(&mut wire, 0, 10, 20, 30);
            assert_eq!(from_wire(order, wire), [10, 20, 30], "{order:?}");
        }
    }
}
