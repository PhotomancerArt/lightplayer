//! A Direct fixture's lane as a work item: plain data and a sampler.

use lpc_model::nodes::fixture::ColorOrder;
use lps_q32::q32::{Q32, ToQ32};

use super::fixture_node::{DirectCoordFill, encode_fixture_channel, ordered_rgb_u16};
use super::fixture_selection::SelectionAt;
use super::power_limit::PowerPass;
#[cfg(test)]
use crate::node::NodeError;
use crate::products::control::ControlRenderTarget;

/// One Direct fixture's per-frame render, once everything that needs the
/// engine has been decided: where the sample coordinates come from, and how
/// each sampled lamp is encoded into which control samples.
///
/// It holds **plain data** — borrows of the mapping, the target and the
/// frame's power pass, and copies of the settings — and no `Rc`, no
/// `RefCell`, no resolver and no context. That is deliberate: a second core
/// will run lanes (planning `lp2025/2026-10-24-0208-engine-frame-plan`,
/// follow-up F1), and a lane it can run is one that touches nothing but its
/// own data and the sampler it is handed. Lanes write disjoint destination
/// runs, so two jobs never share anything mutable but the sampler.
///
/// Today the job runs where the render's closures ran: the shader's sample
/// stream calls [`Self::coords`]'s fill and [`Self::lamps`]' write batch by
/// batch. `run` drives the same batches against any `LaneSampler`; it is
/// test-only until a caller that owns its own sampler exists (the cross-core
/// follow-up), and its test is what proves the job needs nothing else.
pub(crate) struct DirectLaneJob<'a, 't, I> {
    /// Where the next batch's coordinates come from.
    pub(crate) coords: DirectCoordFill<'a>,
    /// How each sampled lamp lands in the target.
    pub(crate) lamps: DirectLaneLamps<'a, 't, I>,
}

/// The lamp-writing half of a [`DirectLaneJob`].
pub(crate) struct DirectLaneLamps<'a, 't, I> {
    /// The channel each sampled lamp lands on, in sample order — a cursor
    /// that resumes where the previous batch left it.
    pub(crate) channels: I,
    pub(crate) target: &'a mut ControlRenderTarget<'t>,
    /// Samples the request asked for; a lamp past them is skipped.
    pub(crate) expected_samples: usize,
    pub(crate) encoding: LampEncoding,
    pub(crate) power: &'a mut PowerPass,
    pub(crate) selection: SelectionAt<'a>,
    /// One past the last control sample written so far.
    pub(crate) written_samples: usize,
}

/// How a sampled lamp's colour becomes its control samples: gamma, then
/// brightness, then the power pass and the selection, then the wire's
/// channel order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LampEncoding {
    pub(crate) color_order: ColorOrder,
    pub(crate) gamma_correction: bool,
    pub(crate) brightness: u8,
    /// `brightness / 255` in Q32, computed once.
    pub(crate) brightness_q32: Q32,
}

impl LampEncoding {
    pub(crate) fn new(color_order: ColorOrder, gamma_correction: bool, brightness: u8) -> Self {
        Self {
            color_order,
            gamma_correction,
            brightness,
            brightness_q32: brightness.to_q32() / 255.to_q32(),
        }
    }
}

/// Samples a batch of points: the one thing a lane needs from outside.
#[cfg(test)]
pub(crate) trait LaneSampler {
    /// Sample `n` points (their coordinates in `points`, two words a point
    /// for 2D and one for 1D, as the job filled them) into `out` as RGBA16,
    /// four words a point.
    fn sample(&mut self, points: &[i32], out: &mut [u16], n: u32) -> Result<(), NodeError>;
}

#[cfg(test)]
impl<I: Iterator<Item = u32>> DirectLaneJob<'_, '_, I> {
    /// Drive every batch: fill a window of coordinates, sample it, write the
    /// lamps. `points` and `samples` are the window (`points.len() / 2`
    /// points, `samples.len() / 4` points).
    pub(crate) fn run(
        &mut self,
        sampler: &mut impl LaneSampler,
        points: &mut [i32],
        samples: &mut [u16],
    ) -> Result<(), NodeError> {
        let capacity = (points.len() / 2).min(samples.len() / 4) as u32;
        loop {
            let n = self.coords.fill(points);
            if n == 0 {
                return Ok(());
            }
            if n > capacity {
                return Err(NodeError::msg(alloc::format!(
                    "lane: fill wrote {n} points into a {capacity}-point window"
                )));
            }
            let out = &mut samples[..n as usize * 4];
            sampler.sample(points, out, n)?;
            self.lamps.write(out);
        }
    }
}

impl<I: Iterator<Item = u32>> DirectLaneLamps<'_, '_, I> {
    /// Encode one batch of sampled lamps into their channels' three control
    /// samples, skipping lamps whose channel falls past the requested extent.
    pub(crate) fn write(&mut self, sampled: &[u16]) {
        let encoding = self.encoding;
        // The samples drive the zip. `Zip` pulls its LEFT side first, so with
        // the channel cursor on the left the lamp after a batch's last sample
        // would be pulled and dropped at every batch boundary.
        for (rgba, channel) in sampled.chunks_exact(4).zip(&mut self.channels) {
            let base = (channel as usize).saturating_mul(3);
            if base + 3 > self.expected_samples {
                continue;
            }
            let encode = |value| {
                encode_fixture_channel(
                    value,
                    encoding.gamma_correction,
                    encoding.brightness,
                    encoding.brightness_q32,
                )
            };
            let (r, g, b) = (encode(rgba[0]), encode(rgba[1]), encode(rgba[2]));
            // After gamma, never before. See `power_limit`.
            let r = self.power.channel(r);
            let g = self.power.channel(g);
            let b = self.power.channel(b);
            let [r, g, b] = self.selection.apply(channel, [r, g, b]);
            let ordered = ordered_rgb_u16(encoding.color_order, r, g, b);
            self.target.write(base, &ordered);
            self.written_samples = self.written_samples.max(base + 3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::products::control::ControlSampleFormat;
    use lpc_model::ControlExtent;

    /// Answers every point with the same colour, and counts the batches.
    struct SolidSampler {
        rgba: [u16; 4],
        batches: u32,
    }

    impl LaneSampler for SolidSampler {
        fn sample(&mut self, _points: &[i32], out: &mut [u16], n: u32) -> Result<(), NodeError> {
            for lamp in out.chunks_exact_mut(4).take(n as usize) {
                lamp.copy_from_slice(&self.rgba);
            }
            self.batches += 1;
            Ok(())
        }
    }

    #[test]
    fn a_hand_built_lane_runs_from_plain_data_without_allocating() {
        const LAMPS: u32 = 5;
        let mut samples = [0u16; LAMPS as usize * 3];
        let mut target = ControlRenderTarget::new(
            ControlExtent::new(1, LAMPS * 3),
            ControlSampleFormat::Unorm16,
            &mut samples,
        );
        let mut power = PowerPass::unlimited();
        // A two-point window, so the lane takes three batches and the channel
        // cursor has to resume across them.
        let mut points = [0i32; 4];
        let mut window = [0u16; 8];
        let mut sampler = SolidSampler {
            rgba: [1000, 2000, 3000, u16::MAX],
            batches: 0,
        };
        let (result, churn) = crate::test_alloc_counter::measure(|| {
            let mut job = DirectLaneJob {
                coords: DirectCoordFill::OneD {
                    next: 0,
                    count: LAMPS,
                    wire_reversed: false,
                },
                lamps: DirectLaneLamps {
                    channels: 0..LAMPS,
                    target: &mut target,
                    expected_samples: LAMPS as usize * 3,
                    encoding: LampEncoding::new(ColorOrder::Grb, false, u8::MAX),
                    power: &mut power,
                    selection: SelectionAt::NONE,
                    written_samples: 0,
                },
            };
            let ran = job.run(&mut sampler, &mut points, &mut window);
            (ran, job.lamps.written_samples)
        });
        let (ran, written) = result;
        ran.expect("the lane runs");
        assert_eq!(churn.allocs, 0, "a lane allocates nothing: {churn:?}");
        assert_eq!(sampler.batches, 3);
        assert_eq!(written, LAMPS as usize * 3);
        // Full brightness, no gamma, no budget: the sampled colour, in GRB.
        assert_eq!(
            samples,
            [2000, 1000, 3000].repeat(LAMPS as usize).as_slice()
        );
    }
}
