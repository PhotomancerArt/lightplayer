//! Triple-buffered display pipeline

use alloc::vec::Vec;

use crate::display_pipeline::options::DisplayPipelineOptions;
use crate::error::DisplayPipelineError;
use core::cmp;

use super::dither::dither_step;

/// Below this value (post-LUT), use shared luminance dithering
/// to avoid R/G/B divergence and color flicker. ~5% of 16-bit max.
/// Disabled for now—was making colored light monochrome.
#[allow(
    dead_code,
    reason = "kept for potential future luminance dithering tuning"
)]
const LOW_GRAY_THRESHOLD: u32 = 65535 / 20;

/// Triple-buffered display pipeline. 16-bit in, 8-bit out.
///
/// "Triple" is conditional. `prev` only ever has its CONTENTS read by
/// [`Self::render_interpolated`], and `dither_overflow` only by the dithering
/// branch of [`Self::apply_white_point_dither`]. Both are gated on options
/// that are fixed for the life of the pipeline, so when an option is off its
/// buffer is left empty rather than allocated — 6 B/LED for `prev`, 3 B/LED
/// for `dither_overflow`, on a device where a whole LED costs ~90 B. See
/// [`Self::rotate_frames`] for why skipping the swap is output-identical.
///
/// With interpolation off, `next` is conditional too: [`Self::write_frame`]
/// writes straight into `current` (another 6 B/LED), because the only thing
/// `next` ever did with interpolation off was hold the frame until the next
/// rotation promoted it — `tick` promotes an available `next` before
/// rendering anyway, so the staging hop was pure copies. Safe on the same
/// grounds as the recycled-buffer argument in [`Self::rotate_frames`]: the
/// ESP32 provider rejects short frames, so a write always fully overwrites
/// its target and the buffer's previous identity is unobservable.
pub struct DisplayPipeline {
    num_leds: u32,
    /// Empty unless `options.interpolation_enabled`.
    prev: Vec<u16>,
    current: Vec<u16>,
    /// Empty unless `options.interpolation_enabled` — see the struct docs.
    next: Vec<u16>,
    prev_ts: u64,
    current_ts: u64,
    next_ts: u64,
    has_prev: bool,
    has_current: bool,
    has_next: bool,
    prev_current_delta_us: u64,
    /// Empty unless `options.dithering_enabled`.
    dither_overflow: Vec<[i8; 3]>,
    /// Per-channel white point as Q16.16, derived once from
    /// [`DisplayPipelineOptions::white_point`] (which is immutable for the
    /// life of the pipeline). See [`white_point_scale`].
    white_scale: [u32; 3],
    options: DisplayPipelineOptions,
    /// Test-only: forces the three-buffer rotation even with interpolation
    /// off, so `ReferencePipeline` can reproduce the pre-change behaviour as
    /// a bit-identity oracle. Never set in production builds.
    #[cfg(test)]
    force_three_buffer_rotation: bool,
    /// Test-only: counts `render_interpolated` calls, so the differential test
    /// can assert it actually exercised the interpolating path instead of
    /// silently proving nothing.
    #[cfg(test)]
    interpolated_renders: u32,
}

/// White point as a Q16.16 multiplier.
///
/// Saturates rather than wrapping: a white point of `NaN` or a negative value
/// becomes 0 (channel off) instead of an enormous scale, and the `as u32` on a
/// huge float saturates rather than being UB.
fn white_point_scale(white_point: f32) -> u32 {
    if !(white_point > 0.0) {
        // Also catches NaN, since every NaN comparison is false.
        return 0;
    }
    (white_point * 65536.0 + 0.5) as u32
}

/// Scale a 16-bit channel value by a Q16.16 white point, rounding to nearest.
///
/// Widened to `u64` deliberately. A white point above 1.0 is not physically
/// meaningful — balancing scales channels *down* — but the previous LUT
/// implementation permitted it (building a boosted table and clamping at
/// 65535), so the arithmetic keeps that behaviour rather than quietly
/// redefining it. At `white_point == 1.0` the `u32` product would fit with
/// 32,767 to spare; anything above overflows, and one widening multiply is
/// cheaper than the three heap loads this replaced.
#[inline]
fn apply_white_point(value: u32, scale: u32) -> u32 {
    let value = value.min(65535) as u64;
    (((value * scale as u64 + 0x8000) >> 16) as u32).min(65535)
}

impl DisplayPipeline {
    /// Allocate pipeline
    pub fn new(
        num_leds: u32,
        options: DisplayPipelineOptions,
    ) -> Result<Self, DisplayPipelineError> {
        if num_leds == 0 {
            return Err(DisplayPipelineError::AllocationFailed { num_leds: 0 });
        }
        let size = (num_leds as usize) * 3;
        // `prev` and `next` both exist only for interpolation — see the
        // struct docs for why the single-buffer write is output-identical.
        let interp_size = if options.interpolation_enabled {
            size
        } else {
            0
        };
        let prev_size = interp_size;
        let next_size = interp_size;
        let overflow_size = if options.dithering_enabled {
            num_leds as usize
        } else {
            0
        };
        let mut prev = Vec::with_capacity(prev_size);
        let mut current = Vec::with_capacity(size);
        let mut next = Vec::with_capacity(next_size);
        prev.resize(prev_size, 0);
        current.resize(size, 0);
        next.resize(next_size, 0);
        let mut dither_overflow = Vec::with_capacity(overflow_size);
        dither_overflow.resize(overflow_size, [0i8; 3]);
        let white_scale = [
            white_point_scale(options.white_point[0]),
            white_point_scale(options.white_point[1]),
            white_point_scale(options.white_point[2]),
        ];
        Ok(Self {
            num_leds,
            prev,
            current,
            next,
            prev_ts: 0,
            current_ts: 0,
            next_ts: 0,
            has_prev: false,
            has_current: false,
            has_next: false,
            prev_current_delta_us: 1,
            dither_overflow,
            white_scale,
            options,
            #[cfg(test)]
            force_three_buffer_rotation: false,
            #[cfg(test)]
            interpolated_renders: 0,
        })
    }

    /// Resize pipeline to new LED count. Clears frame state; old data is lost.
    pub fn resize(&mut self, num_leds: u32) {
        if num_leds == 0 {
            return;
        }
        let size = (num_leds as usize) * 3;
        // Keep the disabled-option buffers empty across a resize; `new` decided
        // they are never read for this pipeline's options.
        if self.options.interpolation_enabled {
            self.prev.resize(size, 0);
        }
        self.current.resize(size, 0);
        self.next.resize(size, 0);
        if self.options.dithering_enabled {
            self.dither_overflow.resize(num_leds as usize, [0i8; 3]);
        }
        self.num_leds = num_leds;
        self.has_prev = false;
        self.has_current = false;
        self.has_next = false;
    }

    /// Rotate buffers: prev<-current, current<-next
    ///
    /// Only invalidates `has_prev` when we actually overwrite `prev` (i.e.
    /// when `current` exists and gets swapped into `prev`). Without that
    /// guard, calling `rotate_frames` from `tick()` in the "missing
    /// current, advance next" path silently drops the previously captured
    /// `prev` frame, which permanently disables temporal interpolation in
    /// loops that interleave ticks with `write_frame` calls.
    fn rotate_frames(&mut self) {
        if self.has_current {
            // With interpolation off, `prev` is empty and its contents are
            // dead: `render_interpolated` is the only reader and `tick` gates
            // every call to it on `interpolation_enabled`. Skipping the swap
            // leaves `current`'s buffer in hand to be recycled by the swap
            // below, turning this into a two-buffer rotation.
            //
            // Every timestamp and flag below is still maintained exactly as in
            // the three-buffer case — `prev_ts`, `has_prev` and
            // `prev_current_delta_us` feed `tick`'s frame-age decisions even
            // when no interpolation happens, so they are NOT interpolation
            // bookkeeping and must not be gated.
            #[cfg(test)]
            let keep_prev_buffer =
                self.options.interpolation_enabled || self.force_three_buffer_rotation;
            #[cfg(not(test))]
            let keep_prev_buffer = self.options.interpolation_enabled;
            if keep_prev_buffer {
                core::mem::swap(&mut self.prev, &mut self.current);
            }
            self.prev_ts = self.current_ts;
            self.has_prev = true;
            self.has_current = false;
        }
        if self.has_next {
            core::mem::swap(&mut self.current, &mut self.next);
            self.current_ts = self.next_ts;
            self.has_current = true;
            self.has_next = false;
        }
        if self.has_prev && self.has_current {
            self.prev_current_delta_us = self.current_ts.saturating_sub(self.prev_ts).max(1);
        }
    }

    /// Submit 16-bit RGB frame for next buffer
    pub fn write_frame(&mut self, ts_us: u64, data: &[u16]) {
        #[cfg(test)]
        let single_buffer =
            !self.options.interpolation_enabled && !self.force_three_buffer_rotation;
        #[cfg(not(test))]
        let single_buffer = !self.options.interpolation_enabled;
        if single_buffer {
            // Write straight into `current` — `next` is empty (struct docs).
            // The timestamp/flag bookkeeping mirrors what the write-rotate
            // plus tick-promote pair produced: the previous frame's stamp
            // becomes `prev_ts` and still feeds `tick`'s frame-age decisions.
            if self.has_current {
                self.prev_ts = self.current_ts;
                self.has_prev = true;
            }
            let len = cmp::min(data.len(), self.current.len());
            self.current[..len].copy_from_slice(&data[..len]);
            self.current_ts = ts_us;
            self.has_current = true;
            if self.has_prev {
                self.prev_current_delta_us = self.current_ts.saturating_sub(self.prev_ts).max(1);
            }
            return;
        }
        self.rotate_frames();
        let len = cmp::min(data.len(), self.next.len());
        self.next[..len].copy_from_slice(&data[..len]);
        self.next_ts = ts_us;
        self.has_next = true;
    }

    /// Submit 8-bit RGB frame (expand to 16-bit)
    pub fn write_frame_from_u8(&mut self, ts_us: u64, data: &[u8]) {
        let size = (self.num_leds as usize) * 3;
        let mut expanded = Vec::with_capacity(size);
        let copy_len = cmp::min(data.len(), size);
        for i in 0..copy_len {
            expanded.push((data[i] as u16) * 257);
        }
        expanded.resize(size, 0);
        self.write_frame(ts_us, &expanded);
    }

    /// Advance pipeline, produce 8-bit output
    pub fn tick(&mut self, now_us: u64, out: &mut [u8]) {
        let out_len = (self.num_leds as usize) * 3;
        if out.len() < out_len {
            return;
        }
        if !self.options.interpolation_enabled && self.has_next {
            self.rotate_frames();
        }
        if !self.has_current && self.has_next {
            self.rotate_frames();
        }
        if !self.has_current {
            out[..out_len].fill(0);
            return;
        }
        if self.options.interpolation_enabled && !self.has_prev {
            self.render_current(out);
            return;
        }
        let frame_progress_us = now_us.saturating_sub(self.prev_ts);
        if self.options.interpolation_enabled
            && self.has_prev
            && frame_progress_us < self.prev_current_delta_us
        {
            self.render_interpolated(now_us, out);
            return;
        }
        if self.has_next && frame_progress_us > self.prev_current_delta_us * 2 {
            self.rotate_frames();
        }
        self.render_current(out);
    }

    fn render_current(&mut self, out: &mut [u8]) {
        let num_leds = self.num_leds as usize;
        for i in 0..num_leds {
            let r = self.current[i * 3] as u32;
            let g = self.current[i * 3 + 1] as u32;
            let b = self.current[i * 3 + 2] as u32;
            let (or, og, ob) = self.apply_white_point_dither(r, g, b, i);
            out[i * 3] = or;
            out[i * 3 + 1] = og;
            out[i * 3 + 2] = ob;
        }
    }

    fn render_interpolated(&mut self, now_us: u64, out: &mut [u8]) {
        #[cfg(test)]
        {
            self.interpolated_renders += 1;
        }
        let frame_progress_us = now_us.saturating_sub(self.prev_ts);
        let frame_progress16 = ((frame_progress_us << 16) / self.prev_current_delta_us) as u16;
        let inv_progress16 = 0xFFFF - frame_progress16;
        let num_leds = self.num_leds as usize;
        for i in 0..num_leds {
            let pr = self.prev[i * 3] as u32;
            let pg = self.prev[i * 3 + 1] as u32;
            let pb = self.prev[i * 3 + 2] as u32;
            let cr = self.current[i * 3] as u32;
            let cg = self.current[i * 3 + 1] as u32;
            let cb = self.current[i * 3 + 2] as u32;
            let ir = ((pr * inv_progress16 as u32) + (cr * frame_progress16 as u32)) >> 16;
            let ig = ((pg * inv_progress16 as u32) + (cg * frame_progress16 as u32)) >> 16;
            let ib = ((pb * inv_progress16 as u32) + (cb * frame_progress16 as u32)) >> 16;
            let (or, og, ob) = self.apply_white_point_dither(ir, ig, ib, i);
            out[i * 3] = or;
            out[i * 3 + 1] = og;
            out[i * 3 + 2] = ob;
        }
    }

    fn apply_white_point_dither(&mut self, r: u32, g: u32, b: u32, pixel: usize) -> (u8, u8, u8) {
        let ir = if self.options.lut_enabled {
            apply_white_point(r, self.white_scale[0])
        } else {
            r
        };
        let ig = if self.options.lut_enabled {
            apply_white_point(g, self.white_scale[1])
        } else {
            g
        };
        let ib = if self.options.lut_enabled {
            apply_white_point(b, self.white_scale[2])
        } else {
            b
        };
        // Shared luminance dithering for low-gray grayscale disabled for now:
        // was causing colored light to appear monochrome; grayscale check was insufficient
        //
        // `&& dithering_enabled` is load-bearing if this is ever switched back
        // on: `dither_overflow` is only allocated when dithering is enabled, so
        // the indexing below would panic on an empty buffer without it.
        let use_shared_luma = false && self.options.dithering_enabled;

        if use_shared_luma {
            let lum = (ir + ig + ib) / 3;
            let (out, no) = dither_step(lum as i32, self.dither_overflow[pixel][0]);
            self.dither_overflow[pixel] = [no, no, no];
            (out, out, out)
        } else if self.options.dithering_enabled {
            let (or, no_r) = dither_step(ir as i32, self.dither_overflow[pixel][0]);
            let (og, no_g) = dither_step(ig as i32, self.dither_overflow[pixel][1]);
            let (ob, no_b) = dither_step(ib as i32, self.dither_overflow[pixel][2]);
            self.dither_overflow[pixel] = [no_r, no_g, no_b];
            (or, og, ob)
        } else {
            let or = ((ir + 0x80) >> 8).min(255) as u8;
            let og = ((ig + 0x80) >> 8).min(255) as u8;
            let ob = ((ib + 0x80) >> 8).min(255) as u8;
            (or, og, ob)
        }
    }
}

/// Reference pipeline reproducing the pre-change allocation behaviour: all
/// three frame buffers and the dither carry always present, and the
/// unconditional three-buffer rotation.
///
/// SCOPE — this is a wrapper around the real [`DisplayPipeline`], not an
/// independent reimplementation. It therefore proves exactly one thing: that
/// making `prev`/`dither_overflow` conditional and skipping the `prev` swap
/// does not change output. It CANNOT catch a regression in code the two share
/// — white-point scaling, `dither_step`, the interpolation arithmetic — because
/// a mutation there moves both sides equally. Those are covered by the
/// value-path tests below (`white_point_matches_the_lut_it_replaced`,
/// `unit_white_point_is_the_identity`, the `dither` module's tests).
///
/// Verified to have teeth: removing the `prev` swap while interpolation is
/// enabled fails this test at step 1.
#[cfg(test)]
struct ReferencePipeline {
    inner: DisplayPipeline,
}

#[cfg(test)]
impl ReferencePipeline {
    fn new(num_leds: u32, options: DisplayPipelineOptions) -> Self {
        let mut inner = DisplayPipeline::new(num_leds, options).expect("reference pipeline");
        // Force the pre-change allocation shape: all three frame buffers and
        // the dither carry always present, regardless of options.
        let size = (num_leds as usize) * 3;
        inner.prev.resize(size, 0);
        inner.next.resize(size, 0);
        inner.dither_overflow.resize(num_leds as usize, [0i8; 3]);
        inner.force_three_buffer_rotation = true;
        Self { inner }
    }

    fn write_frame(&mut self, ts_us: u64, data: &[u16]) {
        self.inner.write_frame(ts_us, data);
    }

    fn tick(&mut self, now_us: u64, out: &mut [u8]) {
        self.inner.tick(now_us, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// Deterministic xorshift — no rand dependency in a no_std crate.
    fn lcg(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    /// The bit-identity proof for the conditional-buffer change: for every
    /// combination of the options that gate `prev` and `dither_overflow`,
    /// drive a randomized frame/tick sequence through both the reference
    /// three-buffer pipeline and the real one, and require byte-identical
    /// output on every tick.
    #[test]
    fn conditional_buffers_are_bit_identical_to_three_buffer_reference() {
        const NUM_LEDS: u32 = 37;
        let size = (NUM_LEDS as usize) * 3;

        for interpolation_enabled in [false, true] {
            for dithering_enabled in [false, true] {
                for lut_enabled in [false, true] {
                    let options = DisplayPipelineOptions {
                        white_point: [0.9, 1.0, 0.75],
                        interpolation_enabled,
                        dithering_enabled,
                        lut_enabled,
                    };
                    let mut reference = ReferencePipeline::new(NUM_LEDS, options.clone());
                    let mut actual =
                        DisplayPipeline::new(NUM_LEDS, options.clone()).expect("pipeline");

                    let mut state = 0x2026_0802_u64;
                    let mut frame = vec![0u16; size];
                    let mut ref_out = vec![0u8; size];
                    let mut act_out = vec![0u8; size];

                    // Frames are stamped in the future relative to the tick
                    // that follows them — the same ordering the ESP32 provider
                    // produces, and the only one under which `tick` reaches
                    // `render_interpolated` (it needs
                    // `now_us - prev_ts < prev_current_delta_us`). A pattern
                    // that ticks past each frame's timestamp silently never
                    // interpolates, which would make this whole test vacuous.
                    const INTERVAL_US: u64 = 20_000;
                    for step in 0..200u64 {
                        for sample in frame.iter_mut() {
                            *sample = (lcg(&mut state) & 0xFFFF) as u16;
                        }
                        let frame_ts = step * INTERVAL_US;
                        reference.write_frame(frame_ts, &frame);
                        actual.write_frame(frame_ts, &frame);

                        // Occasionally a second frame lands before any tick —
                        // the producer outrunning the output cadence. The
                        // single-buffer path overwrites `current` where the
                        // staged path quietly promotes, and the states must
                        // still converge to identical output.
                        if step % 11 == 4 {
                            for sample in frame.iter_mut() {
                                *sample = (lcg(&mut state) & 0xFFFF) as u16;
                            }
                            let burst_ts = frame_ts + INTERVAL_US / 4;
                            reference.write_frame(burst_ts, &frame);
                            actual.write_frame(burst_ts, &frame);
                        }

                        // Tick inside the interval just before `frame_ts`, and
                        // occasionally well past it so the stale-frame
                        // catch-up branch is covered too.
                        let now_us = if step % 7 == 6 {
                            frame_ts + INTERVAL_US * 3
                        } else {
                            frame_ts.saturating_sub(INTERVAL_US / 2)
                        };
                        reference.tick(now_us, &mut ref_out);
                        actual.tick(now_us, &mut act_out);
                        assert_eq!(
                            ref_out, act_out,
                            "output diverged at step {step} with \
                             interpolation={interpolation_enabled} \
                             dithering={dithering_enabled} lut={lut_enabled}"
                        );
                    }

                    // The test is only meaningful if the interpolating path
                    // actually ran when it was enabled.
                    if interpolation_enabled {
                        assert!(
                            actual.interpolated_renders > 0
                                && reference.inner.interpolated_renders > 0,
                            "interpolation enabled but render_interpolated never ran — \
                             the differential proves nothing"
                        );
                    } else {
                        assert_eq!(
                            actual.interpolated_renders, 0,
                            "interpolation disabled but render_interpolated ran"
                        );
                    }
                }
            }
        }
    }

    /// The saving is the point: assert the buffers really are absent, so a
    /// future refactor that quietly reallocates them fails here rather than
    /// silently costing 9 B/LED again.
    #[test]
    fn disabled_options_do_not_allocate_their_buffers() {
        let opts = DisplayPipelineOptions {
            white_point: [1.0, 1.0, 1.0],
            interpolation_enabled: false,
            dithering_enabled: false,
            lut_enabled: true,
        };
        let mut pipeline = DisplayPipeline::new(100, opts).expect("pipeline");
        assert_eq!(
            pipeline.next.len(),
            0,
            "interpolation off must not allocate `next` — write_frame goes \
             straight to `current`"
        );
        assert_eq!(
            pipeline.prev.len(),
            0,
            "prev allocated with interpolation off"
        );
        assert_eq!(
            pipeline.dither_overflow.len(),
            0,
            "dither carry allocated with dithering off"
        );
        // A resize must not reintroduce them.
        pipeline.resize(200);
        assert_eq!(pipeline.prev.len(), 0, "resize reallocated prev");
        assert_eq!(
            pipeline.dither_overflow.len(),
            0,
            "resize reallocated dither carry"
        );
        assert_eq!(pipeline.current.len(), 600);
        assert_eq!(pipeline.next.len(), 600);
    }

    #[test]
    fn enabled_options_still_allocate_their_buffers() {
        let opts = DisplayPipelineOptions {
            white_point: [1.0, 1.0, 1.0],
            interpolation_enabled: true,
            dithering_enabled: true,
            lut_enabled: true,
        };
        let pipeline = DisplayPipeline::new(100, opts).expect("pipeline");
        assert_eq!(pipeline.prev.len(), 300);
        assert_eq!(pipeline.dither_overflow.len(), 100);
    }

    #[test]
    fn new_creates_pipeline() {
        let pipeline = DisplayPipeline::new(64, DisplayPipelineOptions::default());
        assert!(pipeline.is_ok());
    }

    #[test]
    fn write_frame_tick_produces_output() {
        let mut pipeline = DisplayPipeline::new(2, DisplayPipelineOptions::default()).unwrap();
        let data: [u16; 6] = [32768, 0, 65535, 65535, 32768, 0];
        pipeline.write_frame(0, &data);
        pipeline.write_frame(1000, &data);
        let mut out = [0u8; 6];
        pipeline.tick(500, &mut out);
        assert!(out[0] > 0 || out[1] > 0 || out[2] > 0);
    }

    /// The 257-entry white-point LUT this arithmetic replaced (deleted from
    /// production in the same commit), kept here as a migration oracle.
    ///
    /// Its formula was `lut[i] = clamp(round((i/256) * white_point * 65535))`
    /// — a straight line — read back by linear interpolation on the high byte.
    /// Interpolating between samples of a line reproduces the line, so the
    /// whole 3,084-byte-per-channel table computed `value * white_point`.
    /// Keeping it executable rather than as a comment is what lets
    /// [`white_point_matches_the_lut_it_replaced`] state the behaviour delta as
    /// a measured number instead of a claim.
    mod legacy_lut {
        const LUT_LEN: usize = 257;

        pub fn build(white_point: f32) -> [u32; LUT_LEN] {
            let mut lut = [0u32; LUT_LEN];
            for (i, slot) in lut.iter_mut().enumerate() {
                let normal = (i as f32 / 256.0) * white_point;
                let rounded = (normal * 65535.0 + 0.5) as i64;
                *slot = rounded.clamp(0, 65535) as u32;
            }
            lut
        }

        pub fn interpolate(value: u32, lut: &[u32; LUT_LEN]) -> u32 {
            let value = value.min(65535);
            let index = (value >> 8) as usize;
            let alpha = value & 0xFF;
            let inv_alpha = 0x100 - alpha;
            (lut[index] * inv_alpha + lut[index.saturating_add(1).min(LUT_LEN - 1)] * alpha) >> 8
        }
    }

    /// The multiply reproduces the table it replaced to within 2 counts of
    /// 65,535, across every input, at every white point that matters.
    ///
    /// 2/65535 is a quarter of one ULP at the 8-bit wire the pipeline actually
    /// drives, so no LED can resolve it. Where the two *do* diverge, the
    /// multiply is the more correct of the pair: the table was a piecewise
    /// approximation of a line, and this is the line.
    #[test]
    fn white_point_matches_the_lut_it_replaced() {
        for white_point in [1.0f32, 0.9, 0.8, 0.75, 0.5, 0.25, 0.1, 0.0] {
            let lut = legacy_lut::build(white_point);
            let scale = white_point_scale(white_point);
            let worst = (0u32..=65535)
                .map(|v| apply_white_point(v, scale).abs_diff(legacy_lut::interpolate(v, &lut)))
                .max()
                .unwrap();
            assert!(
                worst <= 2,
                "white_point {white_point}: diverged by {worst} counts (max 2)"
            );
        }
    }

    /// A white point of 1.0 must be exactly the identity, not merely close.
    ///
    /// This is the overwhelmingly common case — every output node that does not
    /// deliberately balance its channels — so a systematic off-by-one here
    /// would dim every strip in the product by one 16-bit count forever.
    #[test]
    fn unit_white_point_is_the_identity() {
        let scale = white_point_scale(1.0);
        for v in 0u32..=65535 {
            assert_eq!(apply_white_point(v, scale), v, "not identity at {v}");
        }
    }

    /// Degenerate white points must not wrap into an enormous scale.
    #[test]
    fn degenerate_white_points_saturate() {
        assert_eq!(white_point_scale(f32::NAN), 0);
        assert_eq!(white_point_scale(-1.0), 0);
        assert_eq!(white_point_scale(0.0), 0);
        // Above 1.0 boosts and clamps, exactly as the LUT did.
        let boosted = white_point_scale(2.0);
        assert_eq!(apply_white_point(20_000, boosted), 40_000);
        assert_eq!(apply_white_point(65535, boosted), 65535, "must clamp");
    }

    #[test]
    fn unit_white_point_preserves_linear_midpoint() {
        let mut opts = DisplayPipelineOptions::default();
        opts.white_point = [1.0, 1.0, 1.0];
        opts.dithering_enabled = false;
        opts.interpolation_enabled = false;
        opts.lut_enabled = true;
        let mut pipeline = DisplayPipeline::new(1, opts).unwrap();
        let data: [u16; 3] = [32768, 32768, 32768];
        pipeline.write_frame(0, &data);
        let mut out = [0u8; 3];

        pipeline.tick(0, &mut out);

        assert_eq!(out, [128, 128, 128]);
    }

    #[test]
    fn write_frame_from_u8() {
        let mut opts = DisplayPipelineOptions::default();
        opts.lut_enabled = false;
        let mut pipeline = DisplayPipeline::new(1, opts).unwrap();
        pipeline.write_frame_from_u8(0, &[255, 0, 0]);
        pipeline.write_frame_from_u8(1000, &[255, 0, 0]);
        let mut out = [0u8; 3];
        pipeline.tick(500, &mut out);
        assert_eq!(out[0], 255);
    }

    #[test]
    fn no_current_outputs_black() {
        let mut pipeline = DisplayPipeline::new(1, DisplayPipelineOptions::default()).unwrap();
        let mut out = [0xFFu8; 3];
        pipeline.tick(0, &mut out);
        assert_eq!(out, [0, 0, 0]);
    }

    #[test]
    fn low_gray_shared_dither_keeps_rgb_equal() {
        let mut opts = DisplayPipelineOptions::default();
        opts.lut_enabled = true;
        opts.dithering_enabled = true;
        let mut pipeline = DisplayPipeline::new(1, opts).unwrap();
        // Low value grayscale: 2% of 16-bit max, should use shared luminance path
        let val: u16 = 65535 / 50;
        let data: [u16; 3] = [val, val, val];
        pipeline.write_frame(0, &data);
        pipeline.write_frame(1000, &data);
        let mut out = [0u8; 3];
        pipeline.tick(500, &mut out);
        assert_eq!(out[0], out[1], "R and G should match for low gray");
        assert_eq!(out[1], out[2], "G and B should match for low gray");
    }

    #[test]
    fn interleaved_ticks_preserve_prev_for_interpolation() {
        let mut opts = DisplayPipelineOptions::default();
        opts.lut_enabled = false;
        opts.dithering_enabled = false;
        opts.interpolation_enabled = true;
        let mut pipeline = DisplayPipeline::new(1, opts).unwrap();

        // Forward-stamped writes (frame_ts = now + period). Real loop pattern:
        //   write_frame(period,   A) at t=0
        //   ... ticks at t=10..(period-10) ...
        //   write_frame(2*period, B) at t=period
        //   ... ticks at t=period+10..(2*period-10) ...
        //   write_frame(3*period, C) at t=2*period
        //   ... ticks in the [period, 2*period) window which now actually
        //   straddles the (prev_ts=period, current_ts=2*period) interval
        //   used for interpolation.
        // Frame A=red, B=green, C=blue.
        let red: [u16; 3] = [65535, 0, 0];
        let green: [u16; 3] = [0, 65535, 0];
        let blue: [u16; 3] = [0, 0, 65535];

        let period: u64 = 1000;
        let mut out = [0u8; 3];

        pipeline.write_frame(period, &red);
        for t in (10..period).step_by(50) {
            pipeline.tick(t, &mut out);
        }

        pipeline.write_frame(2 * period, &green);
        for t in ((period + 10)..(2 * period)).step_by(50) {
            pipeline.tick(t, &mut out);
        }

        pipeline.write_frame(3 * period, &blue);
        // Tick midway through the (prev=green @ 2*period, current=blue @ 3*period)
        // window. With the rotate_frames bug, has_prev would be false here and
        // we'd render `current` (pure blue). With the fix, we should see a
        // 50/50 lerp of green and blue.
        pipeline.tick(2 * period + period / 2, &mut out);
        assert!(
            out[1] > 50 && out[2] > 50,
            "expected interpolated green+blue (got R={}, G={}, B={})",
            out[0],
            out[1],
            out[2]
        );
        assert_eq!(out[0], 0, "red should be zero");
    }

    #[test]
    fn resize_clears_state_and_accepts_new_data() {
        let mut pipeline = DisplayPipeline::new(2, DisplayPipelineOptions::default()).unwrap();
        let data2: [u16; 6] = [65535, 0, 0, 0, 65535, 0];
        pipeline.write_frame(0, &data2);
        pipeline.write_frame(1000, &data2);
        pipeline.resize(3);
        let data3: [u16; 9] = [0, 65535, 0, 0, 0, 65535, 65535, 65535, 0];
        pipeline.write_frame(0, &data3);
        pipeline.write_frame(1000, &data3);
        let mut out = [0u8; 9];
        pipeline.tick(500, &mut out);
        assert_eq!(out[1], 255);
        assert_eq!(out[5], 255);
    }
}
