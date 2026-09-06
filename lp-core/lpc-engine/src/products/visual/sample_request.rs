//! Visual sampling request types.

use alloc::vec::Vec;

use lp_gfx::{LpGraphics, SampleOutHandle, SamplePointsHandle};

use super::{ConsumerPolicy, VisualSpace};
use crate::node::{NodeError, err_ctx};

/// Texture UV sample point encoded as Q16.16.
///
/// This is for sampling a materialized texture product, not for direct shader execution.
/// Direct shader sampling uses [`lp_gfx::SamplePointsHandle`], whose points are
/// shader pixel-space Q16.16 coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureUvSamplePoint {
    pub u_q16: i32,
    pub v_q16: i32,
}

/// Texture sampling request for a materialized visual product.
#[derive(Debug, Clone)]
pub struct TextureSampleBatch {
    pub points: Vec<TextureUvSamplePoint>,
    pub time_seconds: f32,
}

/// A visual product sampled in bounded batches.
///
/// The consumer owns a **window** — `points` and `samples` of one capacity —
/// and two closures. The PRODUCER drives the loop (see [`Self::drive`]): it
/// hands `fill` the window's coordinate words (borrowed in place through
/// [`LpGraphics::sample_points_data_mut`] — no scratch, no upload), samples
/// the first `n` points with the uniforms it bound once for the whole
/// stream, and hands `n × 4` RGBA16 words to `consume`. Nothing O(product)
/// is materialized on either side: the coordinates' home is the consumer's
/// mapping, the samples' home is wherever `consume` writes them, and the
/// window between is bounded by [`LpGraphics::sample_batch_capacity`]
/// (`docs/adr/2026-09-06-direct-sampling-bounded-batches.md`).
///
/// `fill` writes at most `capacity` points in the packing `space` declares —
/// `[x, y]` pairs for [`VisualSpace::TwoD`], single `[t]` words for
/// [`VisualSpace::OneD`] (the tail of the window is slack nothing reads) —
/// and returns the point count; `0` ends the stream. `output_width` and
/// `output_height` define the shader `outputSize` uniform for those points
/// — `(N, 1)` for a 1D request.
pub struct VisualSampleStream<'a> {
    pub points: &'a mut SamplePointsHandle,
    pub samples: &'a mut SampleOutHandle,
    pub fill: &'a mut dyn FnMut(&mut [i32]) -> u32,
    pub consume: &'a mut dyn FnMut(&[u16]) -> Result<(), NodeError>,
    pub output_width: u32,
    pub output_height: u32,
    pub time_seconds: f32,
    /// Space the consumer is asking in, and the lane packing of the points.
    pub space: VisualSpace,
    /// The consumer's projection policy, honored by the producer when the
    /// spaces disagree.
    pub policy: ConsumerPolicy,
    /// This stream is a later batch of a product the consumer is walking
    /// through repeated calls this frame (the playlist crossfade drives its
    /// own loop and hands each batch to both entries). A producer made its
    /// frame's decisions on the first call — compile now, keep the last good
    /// program, or render black — and must hold them: a compile that was
    /// deferred on batch 0 stays deferred on batch 1, exactly as it would
    /// have across the one call the whole product used to be.
    pub continuation: bool,
}

impl VisualSampleStream<'_> {
    /// Points per batch: the window's size.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.points.count()
    }

    /// Check the window's two halves agree before a producer relies on them.
    pub fn validate(&self) -> Result<u32, NodeError> {
        let capacity = self.capacity();
        if self.samples.count() != capacity {
            return Err(NodeError::msg(alloc::format!(
                "sample stream: {capacity}-point window with a {}-point sample-out",
                self.samples.count()
            )));
        }
        Ok(capacity)
    }

    /// Run the loop for a producer: fill → `sample_batch(points, samples, n)`
    /// → consume, until `fill` yields `0`.
    ///
    /// `fill` writes into the point handle's own words; `sample_batch` gets
    /// the filled handle and the sample-out to write (a projecting producer
    /// reads the coordinates back through
    /// [`LpGraphics::sample_points_data_mut`]); after it returns, the first
    /// `n × 4` words of the sample-out are handed to `consume` through
    /// [`LpGraphics::sample_out_data`] — a borrow, never a copy.
    pub fn drive(
        &mut self,
        graphics: &dyn LpGraphics,
        mut sample_batch: impl FnMut(
            &mut SamplePointsHandle,
            &mut SampleOutHandle,
            u32,
        ) -> Result<(), NodeError>,
    ) -> Result<(), NodeError> {
        let capacity = self.validate()?;
        loop {
            let words = graphics
                .sample_points_data_mut(self.points)
                .map_err(err_ctx("sample stream points"))?;
            let n = (self.fill)(words);
            if n == 0 {
                return Ok(());
            }
            if n > capacity {
                return Err(NodeError::msg(alloc::format!(
                    "sample stream: fill wrote {n} points into a {capacity}-point window"
                )));
            }
            sample_batch(self.points, self.samples, n)?;
            let data = graphics
                .sample_out_data(self.samples)
                .map_err(err_ctx("sample stream read"))?;
            (self.consume)(&data[..n as usize * 4])?;
        }
    }

    /// Drive the stream with the sample-out already holding every batch's
    /// answer — the producers' "black" path: clear the window once, then
    /// let every batch consume zeros.
    pub fn drive_cleared(&mut self, graphics: &dyn LpGraphics) -> Result<(), NodeError> {
        graphics
            .clear_sample_out(self.samples)
            .map_err(err_ctx("sample stream clear"))?;
        self.drive(graphics, |_, _, _| Ok(()))
    }
}
