//! Mutable output-owned target for control rendering.
//!
//! A producer renders its WHOLE product through this and never learns where
//! the samples land. Two shapes:
//!
//! - **Contiguous** ([`ControlRenderTarget::new`]): buffer sample `i` is
//!   product sample `i`. The path every unpatched project takes, byte-pinned
//!   by the output's golden oracle.
//! - **Scattered** ([`ControlRenderTarget::scattered`]): the output's whole
//!   runtime buffer plus the runs a patch cut the product into. Every write
//!   is routed to each run that claims those product samples; a product
//!   sample no run claims is dropped. This is what lets a patched product
//!   render straight into the buffer instead of into a whole-product scratch
//!   that is then copied out run by run — 6 B per lamp of every patched
//!   product, every frame, and the 35,700 B ask that halted the emulator on
//!   `examples/small-dome`. The runtime buffer is the rendered product's one
//!   home; nothing is materialized beside it
//!   (`docs/adr/2026-09-06-control-render-targets-scatter.md`).
//!
//! Placement is the target's business, not the producer's: reversal and
//! rotation within a run stay post-passes the output applies in place, exactly
//! as it did when the run was a copy.

use lpc_model::ControlExtent;

use super::ControlSampleFormat;

/// One run of a product's samples placed in the target buffer.
///
/// `Default` is the all-zero run — an empty placement — so a resident
/// `Vec<ControlTargetRun>` can be sized fallibly and filled in place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlTargetRun {
    /// First PRODUCT sample this run takes.
    pub source_offset: u32,
    /// First BUFFER sample it lands on.
    pub offset: u32,
    /// Length, in samples.
    pub len: u32,
}

impl ControlTargetRun {
    /// One past the last product sample this run takes.
    const fn source_end(&self) -> u32 {
        self.source_offset.saturating_add(self.len)
    }
}

/// Why a scattered target could not be built.
///
/// A run outside the product or the buffer is a caller bug — the output
/// filters its fragments before it asks — never a frame that renders wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlTargetError {
    /// A run takes product samples past the extent.
    RunOutsideProduct,
    /// A run lands past the end of the buffer.
    RunOutsideBuffer,
}

impl core::fmt::Display for ControlTargetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::RunOutsideProduct => "control target run outside the product extent",
            Self::RunOutsideBuffer => "control target run outside the buffer",
        })
    }
}

/// Output-owned mutable target for a control materialization request.
pub struct ControlRenderTarget<'a> {
    pub extent: ControlExtent,
    pub sample_format: ControlSampleFormat,
    samples: &'a mut [u16],
    /// `None` = contiguous. `Some` = scattered through these runs — possibly
    /// none at all, when every run the output planned was filtered out: the
    /// product still renders (its layout is still wanted) and nothing lands.
    placement: Option<&'a [ControlTargetRun]>,
    /// The runs are sorted by `source_offset` and never overlap in the
    /// product, so a write can binary-search its first run. A patch that
    /// lists the same lamps twice makes this false and every write scans
    /// every run — slower, still correct.
    source_disjoint: bool,
}

impl<'a> ControlRenderTarget<'a> {
    /// A contiguous target: `samples[i]` is product sample `i`.
    #[must_use]
    pub fn new(
        extent: ControlExtent,
        sample_format: ControlSampleFormat,
        samples: &'a mut [u16],
    ) -> Self {
        Self {
            extent,
            sample_format,
            samples,
            placement: None,
            source_disjoint: true,
        }
    }

    /// A scattered target: `samples` is the whole buffer, and each run maps
    /// `len` product samples from `source_offset` onto the buffer at
    /// `offset`.
    ///
    /// Every run is checked against the extent and the buffer here, once,
    /// so the per-write routing below never has to. Runs are used in the
    /// order given — sorting would need an allocation on the tick path, and
    /// the output already hands them over in fragment order.
    pub fn scattered(
        extent: ControlExtent,
        sample_format: ControlSampleFormat,
        samples: &'a mut [u16],
        runs: &'a [ControlTargetRun],
    ) -> Result<Self, ControlTargetError> {
        let product_len = extent.sample_count() as usize;
        for run in runs {
            let source_end = run.source_offset as usize + run.len as usize;
            if source_end > product_len {
                return Err(ControlTargetError::RunOutsideProduct);
            }
            let end = run.offset as usize + run.len as usize;
            if end > samples.len() {
                return Err(ControlTargetError::RunOutsideBuffer);
            }
        }
        let source_disjoint = runs
            .windows(2)
            .all(|pair| pair[0].source_end() <= pair[1].source_offset);
        Ok(Self {
            extent,
            sample_format,
            samples,
            placement: Some(runs),
            source_disjoint,
        })
    }

    /// How many product samples this target can take: the slice length when
    /// contiguous, the extent when scattered (every run was checked against
    /// it at construction).
    #[must_use]
    pub fn product_len(&self) -> usize {
        match self.placement {
            None => self.samples.len(),
            Some(_) => self.extent.sample_count() as usize,
        }
    }

    /// Zero everything this target maps — what `fill(0)` on the slice meant
    /// before placement existed. A scattered target zeroes exactly the run
    /// regions a copy used to overwrite, and nothing beside them.
    pub fn clear(&mut self) {
        match self.placement {
            None => self.samples.fill(0),
            Some(runs) => {
                for run in runs {
                    let start = run.offset as usize;
                    self.samples[start..start + run.len as usize].fill(0);
                }
            }
        }
    }

    /// Place `values` at product sample `product_offset`.
    ///
    /// Contiguous: one slice copy. Scattered: the intersection with every
    /// run that claims any of those samples, each at
    /// `run.offset + (sample - run.source_offset)` — the address the copy put
    /// it at — and samples no run claims are dropped.
    ///
    /// A contiguous write past the slice is clipped rather than a panic: the
    /// writers guard their own bounds before calling, so this cannot happen
    /// today, and a diagnostic must never kill output if it ever does.
    pub fn write(&mut self, product_offset: usize, values: &[u16]) {
        match self.placement {
            None => {
                let end = product_offset
                    .saturating_add(values.len())
                    .min(self.samples.len());
                let start = product_offset.min(end);
                debug_assert_eq!(
                    end - start,
                    values.len(),
                    "control write past a contiguous target"
                );
                self.samples[start..end].copy_from_slice(&values[..end - start]);
            }
            Some(runs) => {
                let product_end = product_offset.saturating_add(values.len());
                // With disjoint sorted runs, `source_end` is monotone too, so
                // the runs that end at or before this write are a prefix.
                let first = if self.source_disjoint {
                    runs.partition_point(|run| run.source_end() as usize <= product_offset)
                } else {
                    0
                };
                for run in &runs[first..] {
                    let run_start = run.source_offset as usize;
                    let run_end = run.source_end() as usize;
                    if self.source_disjoint && run_start >= product_end {
                        break;
                    }
                    let a = product_offset.max(run_start);
                    let b = product_end.min(run_end);
                    if a >= b {
                        continue;
                    }
                    let dest = run.offset as usize + (a - run_start);
                    self.samples[dest..dest + (b - a)]
                        .copy_from_slice(&values[a - product_offset..b - product_offset]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTENT: ControlExtent = ControlExtent::new(1, 12);

    fn run(source_offset: u32, offset: u32, len: u32) -> ControlTargetRun {
        ControlTargetRun {
            source_offset,
            offset,
            len,
        }
    }

    #[test]
    fn a_contiguous_target_is_the_slice() {
        let mut samples = [7u16; 12];
        let mut target =
            ControlRenderTarget::new(EXTENT, ControlSampleFormat::Unorm16, &mut samples);
        assert_eq!(target.product_len(), 12);
        target.clear();
        assert_eq!(samples, [0u16; 12]);

        let mut target =
            ControlRenderTarget::new(EXTENT, ControlSampleFormat::Unorm16, &mut samples);
        target.write(3, &[1, 2, 3]);
        assert_eq!(samples[3..6], [1, 2, 3]);
        assert!(
            samples[..3]
                .iter()
                .chain(samples[6..].iter())
                .all(|s| *s == 0)
        );
    }

    /// Two runs of a 12-sample product land at two places in a 30-sample
    /// buffer; product samples 6.. are on no run.
    #[test]
    fn a_scattered_write_lands_at_the_runs_address() {
        let mut buffer = [9u16; 30];
        let runs = [run(0, 20, 3), run(3, 10, 3)];
        let mut target = ControlRenderTarget::scattered(
            EXTENT,
            ControlSampleFormat::Unorm16,
            &mut buffer,
            &runs,
        )
        .expect("valid runs");
        assert_eq!(target.product_len(), 12);

        target.clear();
        // Only the run regions are zeroed.
        assert_eq!(buffer[20..23], [0, 0, 0]);
        assert_eq!(buffer[10..13], [0, 0, 0]);
        assert!(buffer[..10].iter().all(|s| *s == 9));
        assert!(buffer[13..20].iter().all(|s| *s == 9));
        assert!(buffer[23..].iter().all(|s| *s == 9));

        let mut target = ControlRenderTarget::scattered(
            EXTENT,
            ControlSampleFormat::Unorm16,
            &mut buffer,
            &runs,
        )
        .expect("valid runs");
        target.write(0, &[1, 2, 3]);
        target.write(3, &[4, 5, 6]);
        // Product samples no run claims are dropped.
        target.write(6, &[7, 8, 9]);
        assert_eq!(buffer[20..23], [1, 2, 3]);
        assert_eq!(buffer[10..13], [4, 5, 6]);
        assert!(buffer[13..20].iter().all(|s| *s == 9));
    }

    #[test]
    fn a_write_straddling_two_runs_is_split() {
        let mut buffer = [0u16; 12];
        // Product 0..4 → buffer 8..12, product 4..8 → buffer 0..4.
        let runs = [run(0, 8, 4), run(4, 0, 4)];
        let mut target = ControlRenderTarget::scattered(
            EXTENT,
            ControlSampleFormat::Unorm16,
            &mut buffer,
            &runs,
        )
        .expect("valid runs");
        target.write(3, &[1, 2, 3]);
        assert_eq!(buffer, [2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    }

    /// The same product lamps on two wire places (a patch that lists them
    /// twice): a write lands in BOTH.
    #[test]
    fn source_overlapping_runs_both_receive_the_write() {
        let mut buffer = [0u16; 12];
        let runs = [run(0, 0, 3), run(0, 6, 3)];
        let mut target = ControlRenderTarget::scattered(
            EXTENT,
            ControlSampleFormat::Unorm16,
            &mut buffer,
            &runs,
        )
        .expect("valid runs");
        assert!(!target.source_disjoint);
        target.write(0, &[1, 2, 3]);
        assert_eq!(buffer[0..3], [1, 2, 3]);
        assert_eq!(buffer[6..9], [1, 2, 3]);
    }

    #[test]
    fn unsorted_runs_still_route() {
        let mut buffer = [0u16; 12];
        let runs = [run(6, 0, 3), run(0, 6, 3)];
        let mut target = ControlRenderTarget::scattered(
            EXTENT,
            ControlSampleFormat::Unorm16,
            &mut buffer,
            &runs,
        )
        .expect("valid runs");
        assert!(!target.source_disjoint);
        target.write(0, &[1, 2, 3]);
        target.write(6, &[4, 5, 6]);
        assert_eq!(buffer, [4, 5, 6, 0, 0, 0, 1, 2, 3, 0, 0, 0]);
    }

    #[test]
    fn a_target_with_no_runs_renders_nowhere() {
        let mut buffer = [5u16; 6];
        let runs: [ControlTargetRun; 0] = [];
        let mut target = ControlRenderTarget::scattered(
            EXTENT,
            ControlSampleFormat::Unorm16,
            &mut buffer,
            &runs,
        )
        .expect("no runs is valid");
        assert_eq!(target.product_len(), 12, "the product is still the extent");
        target.clear();
        target.write(0, &[1, 2, 3]);
        assert_eq!(buffer, [5u16; 6]);
    }

    #[test]
    fn runs_outside_the_product_or_buffer_are_refused() {
        let mut buffer = [0u16; 6];
        let past_product = [run(10, 0, 3)];
        assert_eq!(
            ControlRenderTarget::scattered(
                EXTENT,
                ControlSampleFormat::Unorm16,
                &mut buffer,
                &past_product
            )
            .err(),
            Some(ControlTargetError::RunOutsideProduct)
        );
        let past_buffer = [run(0, 4, 3)];
        assert_eq!(
            ControlRenderTarget::scattered(
                EXTENT,
                ControlSampleFormat::Unorm16,
                &mut buffer,
                &past_buffer
            )
            .err(),
            Some(ControlTargetError::RunOutsideBuffer)
        );
    }
}
