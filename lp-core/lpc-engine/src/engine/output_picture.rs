//! A small picture of what the outputs show right now: lamps per published
//! output, and a few point samples of their colours as 8-bit R, G, B.
//!
//! The cloud relay's `Picture` frame (`lpc_relay::RelayPicture`) is made
//! from it, on a board between ticks, from a hook that holds only
//! `&LpServer`; so both calls here are `&self`, and they read the published
//! output buffers in place. Nothing is copied: a `U16` buffer is read
//! through [`RuntimeBuffer::samples16`], a `U8` one through its borrowed
//! bytes. (The output-frame probe, `read_project_output_frame_probe`, is
//! `&mut self` and clones every buffer: the wrong tool for this.)
//!
//! The outputs are found as that probe finds them: the tree's entries in
//! order, each alive node with a published sink buffer whose metadata says
//! `OutputChannels`, read with the sample layout the node latched.
//!
//! **What one sample's colour is** (each rule pinned by a test below; the
//! relay ADR repeats them):
//!
//! - Sample `i` of `count` is lamp `⌊i·T/count⌋` of the outputs
//!   concatenated in that order, `T` the lamps' sum (computed exactly, with
//!   no product that could overflow).
//! - A lamp is three samples, read in the order the wire carries them; an
//!   output's trailing channels that make no whole lamp are not a lamp.
//! - **The colour order is undone** by the `RgbPixels` span covering the
//!   lamp, by the rule Studio's card uses: a span covers the lamp's first
//!   sample `s` when `s ≥ start`, `s + 3 ≤ start + len` and
//!   `(s − start) % 3 == 0`. A lamp no `RgbPixels` span covers (a free
//!   stretch between placed runs, or a `Raw` span) is read in wire order.
//! - **sRGB8 display codes**: a `U16` sample `v` becomes
//!   `linear16_to_srgb8(v)`; a `U8` sample `k` is widened to `k·257` first
//!   and encoded the same way — what Studio's card draws for either, so a
//!   picture's codes mean one thing whatever the buffer holds.
//! - The samples are the published ones (post-finalize: brightness, gamma,
//!   white point, the power limit, the safe-mode clamp).
//! - A lamp past its buffer's end (the buffer changed between the two
//!   calls, which cannot happen on the main thread between ticks) reads as
//!   black: never a panic, never an out-of-bounds read.

use alloc::vec::Vec;

use lpc_model::ColorOrder;
use lpc_wire::linear16_to_srgb8;

use super::Engine;
use crate::node::NodeEntryState;
use crate::products::control::{ControlHint, ControlSpan};
use crate::resource::{RuntimeBufferMetadata, RuntimeChannelSampleFormat};
use crate::resources::buffer::RuntimeBufferData;

impl Engine {
    /// Lamps per published output (`channels / 3`), in the tree's entry
    /// order, at most `max_outputs` of them, into `lamps` (cleared first).
    pub fn output_picture_lamps(&self, max_outputs: usize, lamps: &mut Vec<u32>) {
        lamps.clear();
        lamps.extend(
            self.picture_outputs()
                .take(max_outputs)
                .map(|output| output.lamps()),
        );
    }

    /// Append `count` samples to `rgb`, three bytes each (R, G, B), for the
    /// outputs `lamps` describes (as [`Self::output_picture_lamps`] returned
    /// them). Sample `i` is lamp `⌊i·T/count⌋` of the outputs concatenated,
    /// `T` the lamps' sum. `count` is 0 exactly when `T` is 0, else
    /// `1 ≤ count ≤ T`; exactly `3·count` bytes are appended either way.
    pub fn append_output_picture(&self, lamps: &[u32], count: u32, rgb: &mut Vec<u8>) {
        append_picture_samples(self.picture_outputs(), lamps, count, rgb);
    }

    /// The published outputs, in the tree's entry order, borrowed.
    fn picture_outputs(&self) -> impl Iterator<Item = PictureOutput<'_>> {
        self.tree().entries().filter_map(|entry| {
            let NodeEntryState::Alive(node) = entry.state.value() else {
                return None;
            };
            let buffer = self
                .runtime_buffers()
                .get(node.runtime_output_sink_buffer_id()?)?
                .value();
            let RuntimeBufferMetadata::OutputChannels {
                channels,
                sample_format,
            } = buffer.metadata
            else {
                return None;
            };
            let samples = match (sample_format, &buffer.data) {
                (RuntimeChannelSampleFormat::U16, RuntimeBufferData::Samples16(samples)) => {
                    PictureSamples::U16(samples)
                }
                (RuntimeChannelSampleFormat::U8, RuntimeBufferData::Bytes(bytes)) => {
                    PictureSamples::U8(bytes)
                }
                // A format its payload does not carry: no output publishes
                // one; read nothing rather than guess.
                _ => return None,
            };
            let spans = node
                .runtime_output_sample_layout()
                .map_or(&[][..], |layout| layout.spans.as_slice());
            Some(PictureOutput {
                channels,
                samples,
                spans,
            })
        })
    }
}

/// One published output as the picture reads it: borrowed, never copied.
#[derive(Clone, Copy)]
pub(crate) struct PictureOutput<'a> {
    /// The buffer's channel count (three per lamp).
    pub(crate) channels: u32,
    /// The published samples.
    pub(crate) samples: PictureSamples<'a>,
    /// The latched sample layout's spans (empty when none is latched).
    pub(crate) spans: &'a [ControlSpan],
}

/// An output's published samples, in the element type the buffer stores.
#[derive(Clone, Copy)]
pub(crate) enum PictureSamples<'a> {
    /// Linear unorm16.
    U16(&'a [u16]),
    /// Linear unorm8.
    U8(&'a [u8]),
}

impl PictureOutput<'_> {
    /// Whole lamps: `channels / 3` (trailing channels are not a lamp).
    fn lamps(&self) -> u32 {
        self.channels / 3
    }

    /// Lamp `lamp`'s colour as sRGB8 display codes, R, G, B. Black past the
    /// buffer's end.
    fn lamp_rgb(&self, lamp: u32) -> [u8; 3] {
        if lamp >= self.lamps() {
            return [0; 3];
        }
        let start = lamp as usize * 3;
        let wire = match self.samples {
            PictureSamples::U16(samples) => match samples.get(start..start + 3) {
                Some(&[a, b, c]) => [a, b, c],
                _ => return [0; 3],
            },
            PictureSamples::U8(bytes) => match bytes.get(start..start + 3) {
                Some(&[a, b, c]) => [a, b, c].map(|k| u16::from(k) * 257),
                _ => return [0; 3],
            },
        };
        let codes = wire.map(linear16_to_srgb8);
        match color_order_at(self.spans, lamp * 3) {
            Some(order) => unswizzle(order, codes),
            None => codes,
        }
    }
}

/// Append `count` samples of `outputs` (the ones `lamps` describes, in the
/// same order) to `rgb`: the rules in the module doc. Pure, so it is tested
/// without an engine.
pub(crate) fn append_picture_samples<'a>(
    outputs: impl Iterator<Item = PictureOutput<'a>>,
    lamps: &[u32],
    count: u32,
    rgb: &mut Vec<u8>,
) {
    if count == 0 {
        return;
    }
    let total: u64 = lamps.iter().map(|&lamps| u64::from(lamps)).sum();
    let count64 = u64::from(count);
    // Sample i's lamp is ⌊i·T/count⌋ = i·q + ⌊i·r/count⌋ with T = q·count + r:
    // stepped exactly, with no i·T product to overflow.
    let (step, step_rem) = (total / count64, total % count64);
    let (mut lamp, mut rem) = (0u64, 0u64);
    let mut outputs = outputs.take(lamps.len());
    let mut current = outputs.next();
    let mut index = 0usize;
    let mut base = 0u64;
    rgb.reserve(count as usize * 3);
    for _ in 0..count {
        while index < lamps.len() && lamp >= base + u64::from(lamps[index]) {
            base += u64::from(lamps[index]);
            index += 1;
            current = outputs.next();
        }
        let colour = match current {
            Some(output) if index < lamps.len() => output.lamp_rgb((lamp - base) as u32),
            _ => [0; 3],
        };
        rgb.extend_from_slice(&colour);
        lamp += step;
        rem += step_rem;
        if rem >= count64 {
            rem -= count64;
            lamp += 1;
        }
    }
}

/// The colour order of the `RgbPixels` span covering the lamp whose first
/// sample is `sample_start`, by Studio's rule
/// (`lamp_view.rs`, `control_color_order_at_sample`); `None` when no
/// `RgbPixels` span covers it.
fn color_order_at(spans: &[ControlSpan], sample_start: u32) -> Option<ColorOrder> {
    spans.iter().find_map(|span| match span.encoding {
        ControlHint::RgbPixels { color_order, .. }
            if sample_start >= span.start
                && sample_start.saturating_add(3) <= span.start.saturating_add(span.len)
                && (sample_start - span.start) % 3 == 0 =>
        {
            Some(color_order)
        }
        _ => None,
    })
}

/// Undo `order`: the three channels as the wire carries them back to R, G,
/// B (the inverse of `ColorOrder::write_rgb`).
fn unswizzle(order: ColorOrder, [a, b, c]: [u8; 3]) -> [u8; 3] {
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
    use alloc::vec;

    use lpc_model::ControlSampleEncoding;

    #[test]
    fn a_u16_lamp_in_a_grb_span_comes_back_rgb_and_srgb_encoded() {
        // Lamp 0: R = 0x8000, G = 0x1000, B = 0xffff, written GRB.
        let samples = [0x1000, 0x8000, 0xffff];
        let spans = [rgb_span(0, 3, ColorOrder::Grb)];
        let rgb = picture(&[output_u16(&samples, &spans)], 1);
        assert_eq!(
            rgb,
            vec![
                linear16_to_srgb8(0x8000),
                linear16_to_srgb8(0x1000),
                linear16_to_srgb8(0xffff),
            ]
        );
        assert_eq!(rgb[2], 255);
    }

    #[test]
    fn a_lamp_outside_every_rgb_span_reads_in_wire_order() {
        // Two lamps; only lamp 1 is placed (BGR); lamp 0 is a free stretch.
        let samples = [0xffff, 0, 0, 0, 0, 0xffff];
        let spans = [rgb_span(3, 3, ColorOrder::Bgr)];
        let rgb = picture(&[output_u16(&samples, &spans)], 2);
        assert_eq!(rgb, vec![255, 0, 0, 255, 0, 0]);
        // A Raw span is no colour order either.
        let raw = [ControlSpan {
            row: 0,
            start: 0,
            len: 6,
            encoding: ControlSampleEncoding::Raw,
        }];
        let rgb = picture(&[output_u16(&[0, 0, 0xffff, 0, 0, 0], &raw)], 2);
        assert_eq!(rgb, vec![0, 0, 255, 0, 0, 0]);
    }

    #[test]
    fn a_span_covers_only_whole_lamps_on_its_own_grid() {
        // A span starting at sample 1 covers no lamp of this output.
        let samples = [0, 0xffff, 0, 0, 0, 0];
        let spans = [rgb_span(1, 5, ColorOrder::Grb)];
        let rgb = picture(&[output_u16(&samples, &spans)], 2);
        assert_eq!(rgb[..3], [0, 255, 0], "wire order, not GRB");
    }

    #[test]
    fn a_u8_output_is_widened_by_257_then_encoded() {
        let bytes = [128, 1, 255];
        let rgb = picture(&[output_u8(&bytes, &[])], 1);
        assert_eq!(
            rgb,
            vec![
                linear16_to_srgb8(128 * 257),
                linear16_to_srgb8(257),
                linear16_to_srgb8(255 * 257),
            ]
        );
        assert_eq!(rgb[2], 255);
        assert_ne!(rgb[0], 128, "a display code, not the linear one");
    }

    #[test]
    fn two_outputs_of_five_and_three_lamps_sampled_to_four_are_lamps_0_2_4_6() {
        // Each lamp's red channel is its index across both outputs.
        let a: Vec<u16> = (0..5).flat_map(|i| [i * 4096, 0, 0]).collect();
        let b: Vec<u16> = (5..8).flat_map(|i| [i * 4096, 0, 0]).collect();
        let outputs = [output_u16(&a, &[]), output_u16(&b, &[])];
        let mut lamps = Vec::new();
        lamps.extend(outputs.iter().map(PictureOutput::lamps));
        assert_eq!(lamps, [5, 3]);
        let mut rgb = Vec::new();
        append_picture_samples(outputs.iter().copied(), &lamps, 4, &mut rgb);
        let reds: Vec<u8> = rgb.chunks(3).map(|c| c[0]).collect();
        let want: Vec<u8> = [0u16, 2, 4, 6]
            .iter()
            .map(|&i| linear16_to_srgb8(i * 4096))
            .collect();
        assert_eq!(reds, want);
    }

    #[test]
    fn a_thousand_lamps_sampled_to_256_are_lamp_i_times_1000_over_256() {
        // Each lamp spells its index in display codes: red = index / 256,
        // green = index % 256 (`srgb8_to_linear16` round-trips every code).
        let samples: Vec<u16> = (0..1000u32)
            .flat_map(|i| [code(i / 256), code(i % 256), 0])
            .collect();
        let rgb = picture(&[output_u16(&samples, &[])], 256);
        let shown: Vec<u32> = rgb
            .chunks(3)
            .map(|c| u32::from(c[0]) * 256 + u32::from(c[1]))
            .collect();
        let want: Vec<u32> = (0..256u32).map(|i| i * 1000 / 256).collect();
        assert_eq!(shown, want);
        assert_eq!(shown[255], 996, "the last sample is lamp 996, never 1000");
    }

    #[test]
    fn the_stepped_lamp_index_is_exact_for_big_totals() {
        // Against the plain formula, in u128, for awkward totals.
        for &(total, count) in &[
            (u64::from(u32::MAX), 660u32),
            (7, 3),
            (1, 1),
            (65_535, 65_535),
        ] {
            let (step, step_rem) = (total / u64::from(count), total % u64::from(count));
            let (mut lamp, mut rem) = (0u64, 0u64);
            for i in 0..u64::from(count) {
                assert_eq!(
                    u128::from(lamp),
                    u128::from(i) * u128::from(total) / u128::from(count),
                    "{total}/{count} at {i}"
                );
                lamp += step;
                rem += step_rem;
                if rem >= u64::from(count) {
                    rem -= u64::from(count);
                    lamp += 1;
                }
            }
        }
    }

    #[test]
    fn only_the_outputs_lamps_lists_are_read() {
        // 17 outputs of one lamp each; the caller asked for 16 of them.
        let samples: Vec<[u16; 3]> = (0..17u16).map(|i| [i * 1000, 0, 0]).collect();
        let outputs: Vec<PictureOutput<'_>> = samples.iter().map(|s| output_u16(s, &[])).collect();
        let lamps: Vec<u32> = outputs.iter().take(16).map(PictureOutput::lamps).collect();
        assert_eq!(lamps.len(), 16);
        let mut rgb = Vec::new();
        append_picture_samples(outputs.iter().copied(), &lamps, 16, &mut rgb);
        assert_eq!(rgb.len(), 48);
        assert_eq!(rgb[45], linear16_to_srgb8(15 * 1000), "the sixteenth, last");
    }

    #[test]
    fn no_outputs_means_no_lamps_and_nothing_appended() {
        let mut rgb = vec![9];
        append_picture_samples(core::iter::empty(), &[], 0, &mut rgb);
        assert_eq!(rgb, vec![9]);
    }

    #[test]
    fn trailing_channels_that_make_no_lamp_are_ignored() {
        let samples = [0xffff, 0xffff, 0xffff, 0xffff, 0xffff];
        let output = output_u16(&samples, &[]);
        assert_eq!(output.lamps(), 1);
        assert_eq!(output.lamp_rgb(1), [0; 3], "channels 3–4 are no lamp");
    }

    #[test]
    fn a_lamp_past_its_buffer_reads_black() {
        // Lamps said 3; the buffer now holds one lamp.
        let samples = [0xffff, 0xffff, 0xffff];
        let output = PictureOutput {
            channels: 9,
            samples: PictureSamples::U16(&samples),
            spans: &[],
        };
        let mut rgb = Vec::new();
        append_picture_samples([output].into_iter(), &[3], 3, &mut rgb);
        assert_eq!(rgb, vec![255, 255, 255, 0, 0, 0, 0, 0, 0]);
        // An output gone altogether: every sample of it black, still 3·count.
        let mut rgb = Vec::new();
        append_picture_samples(core::iter::empty(), &[2], 2, &mut rgb);
        assert_eq!(rgb, vec![0; 6]);
    }

    #[test]
    fn unswizzle_inverts_write_rgb_for_every_order() {
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
            assert_eq!(unswizzle(order, wire), [10, 20, 30], "{order:?}");
        }
    }

    fn picture(outputs: &[PictureOutput<'_>], count: u32) -> Vec<u8> {
        let lamps: Vec<u32> = outputs.iter().map(PictureOutput::lamps).collect();
        let mut rgb = Vec::new();
        append_picture_samples(outputs.iter().copied(), &lamps, count, &mut rgb);
        assert_eq!(rgb.len(), count as usize * 3, "always 3·count bytes");
        rgb
    }

    fn output_u16<'a>(samples: &'a [u16], spans: &'a [ControlSpan]) -> PictureOutput<'a> {
        PictureOutput {
            channels: samples.len() as u32,
            samples: PictureSamples::U16(samples),
            spans,
        }
    }

    fn output_u8<'a>(bytes: &'a [u8], spans: &'a [ControlSpan]) -> PictureOutput<'a> {
        PictureOutput {
            channels: bytes.len() as u32,
            samples: PictureSamples::U8(bytes),
            spans,
        }
    }

    fn rgb_span(start: u32, len: u32, color_order: ColorOrder) -> ControlSpan {
        ControlSpan {
            row: 0,
            start,
            len,
            encoding: ControlSampleEncoding::RgbPixels {
                count: len / 3,
                color_order,
            },
        }
    }

    /// The linear value whose display code is `k`.
    fn code(k: u32) -> u16 {
        lpc_wire::srgb8_to_linear16(k as u8)
    }
}
