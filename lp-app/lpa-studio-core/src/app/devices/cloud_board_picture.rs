//! A board's last picture from lightplayer.app's relay, drawn as a lamp
//! preview: the card's picture of an account board no link here reaches.
//!
//! The relay keeps a board's picture as samples, not geometry: sample `i`
//! of `colors` is lamp `⌊i·T/count⌋` of the outputs concatenated in order
//! (`T` their lamps' sum, `count` the samples; `lpc_relay::RelayPicture`).
//! The card has no layout for a board it never opened, so each output is
//! one row of lamps, in the board's order, its samples spaced evenly
//! across it: the colours are the board's own, and where they sit is only
//! which output they belong to.

use std::rc::Rc;

use lpc_model::{
    ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d, ControlSampleLayout,
    Revision,
};

use crate::{UiControlProductPreview, UiControlSampleFormat};

/// One board's picture, as the relay last sent it.
#[derive(Clone, Debug, PartialEq)]
pub struct CloudBoardPicture {
    /// Moves on every picture the relay accepts from the board.
    pub seq: u64,
    /// When the picture arrived at the relay, epoch seconds.
    pub at: f64,
    /// Lamps per output, in the project's tree order.
    pub outputs: Vec<u32>,
    /// R, G, B per sample, sRGB codes.
    pub colors: Vec<u8>,
}

/// The picture as a lamp preview, one row per output; `None` when it holds
/// no whole sample or no lamps to place them on.
pub fn cloud_picture_preview(picture: &CloudBoardPicture) -> Option<UiControlProductPreview> {
    let count = picture.colors.len() / 3;
    let lamps: u64 = picture.outputs.iter().map(|&lamps| u64::from(lamps)).sum();
    if count == 0 || lamps == 0 {
        return None;
    }
    // Each sample's output: the one its lamp falls in.
    let output_of = |sample: usize| {
        let lamp = sample as u64 * lamps / count as u64;
        let mut end = 0u64;
        picture
            .outputs
            .iter()
            .position(|&output| {
                end += u64::from(output);
                lamp < end
            })
            .unwrap_or(picture.outputs.len() - 1)
    };
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); picture.outputs.len()];
    for sample in 0..count {
        rows[output_of(sample)].push(sample);
    }
    rows.retain(|row| !row.is_empty());
    let widest = rows.iter().map(Vec::len).max().unwrap_or(1);
    let cells = widest.max(rows.len()) as f32;
    let mut placed = Vec::with_capacity(count);
    for (y, row) in rows.iter().enumerate() {
        for (x, sample) in row.iter().enumerate() {
            placed.push(ControlLamp2d {
                lamp_index: *sample as u32,
                sample_start: (*sample * 3) as u32,
                center: [
                    (x as f32 + 0.5) / row.len() as f32,
                    (y as f32 + 0.5) / rows.len() as f32,
                ],
                radius: 0.45 / cells,
            });
        }
    }
    let revision = i64::try_from(picture.seq).unwrap_or(i64::MAX);
    let layout = ControlLayout2d::new(
        Revision::new(revision),
        widest as u32,
        rows.len() as u32,
        placed,
    );
    Some(UiControlProductPreview {
        revision,
        extent: ControlExtent::new(1, (count * 3) as u32),
        sample_format: UiControlSampleFormat::Srgb8,
        sample_layout: ControlSampleLayout::default(),
        display_layout: Some(Rc::new(ControlDisplayLayout::Layout2d(layout))),
        bytes: Rc::from(&picture.colors[..count * 3]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_output_is_a_row_of_its_own_samples() {
        // Two outputs, 4 and 2 lamps, sampled 6 times: one sample a lamp.
        let preview = cloud_picture_preview(&picture(vec![4, 2], 6)).unwrap();
        let ControlDisplayLayout::Layout2d(layout) = preview.display_layout.as_deref().unwrap();
        let rows: Vec<f32> = layout.lamps.iter().map(|lamp| lamp.center[1]).collect();
        assert_eq!(rows, [0.25, 0.25, 0.25, 0.25, 0.75, 0.75]);
        assert_eq!(layout.lamps[5].sample_start, 15);
        assert_eq!(preview.sample_format, UiControlSampleFormat::Srgb8);
        assert_eq!(preview.unorm16_sample(15), Some(u16::MAX));
        assert_eq!(preview.revision, 9);
    }

    #[test]
    fn fewer_samples_than_lamps_fall_in_the_outputs_they_sample() {
        // 100 lamps on one output and 10 on another, sampled 11 times: the
        // tenth sample is lamp 90, the eleventh lamp 100 (the second output).
        let preview = cloud_picture_preview(&picture(vec![100, 10], 11)).unwrap();
        let ControlDisplayLayout::Layout2d(layout) = preview.display_layout.as_deref().unwrap();
        assert_eq!(layout.height_hint, 2);
        assert_eq!(layout.width_hint, 10);
    }

    #[test]
    fn a_picture_with_nothing_to_place_draws_nothing() {
        assert!(cloud_picture_preview(&picture(vec![4], 0)).is_none());
        assert!(cloud_picture_preview(&picture(Vec::new(), 3)).is_none());
        assert!(cloud_picture_preview(&picture(vec![0], 3)).is_none());
    }

    fn picture(outputs: Vec<u32>, samples: usize) -> CloudBoardPicture {
        CloudBoardPicture {
            seq: 9,
            at: 1.0,
            outputs,
            colors: (0..samples).flat_map(|_| [255, 0, 0]).collect(),
        }
    }
}
