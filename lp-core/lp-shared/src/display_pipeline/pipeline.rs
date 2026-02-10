//! Triple-buffered display pipeline

use alloc::vec::Vec;

use crate::display_pipeline::lut::{LUT_LEN, build_lut, lut_interpolate};
use crate::display_pipeline::options::DisplayPipelineOptions;
use crate::error::DisplayPipelineError;
use core::cmp;

use super::dither::dither_step;

/// Below this value (post-LUT, post-brightness), use shared luminance dithering
/// to avoid R/G/B divergence and color flicker. ~5% of 16-bit max.
/// Disabled for now—was making colored light monochrome.
#[allow(dead_code)]
const LOW_GRAY_THRESHOLD: u32 = 65535 / 20;

/// Triple-buffered display pipeline. 16-bit in, 8-bit out.
pub struct DisplayPipeline {
    num_leds: u32,
    prev: Vec<u16>,
    current: Vec<u16>,
    next: Vec<u16>,
    prev_ts: u64,
    current_ts: u64,
    next_ts: u64,
    has_prev: bool,
    has_current: bool,
    has_next: bool,
    prev_current_delta_us: u64,
    dither_overflow: Vec<[i8; 3]>,
    lut: [[u32; LUT_LEN]; 3],
    options: DisplayPipelineOptions,
    brightness_u8: u8,
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
        let mut prev = Vec::with_capacity(size);
        let mut current = Vec::with_capacity(size);
        let mut next = Vec::with_capacity(size);
        prev.resize(size, 0);
        current.resize(size, 0);
        next.resize(size, 0);
        let mut dither_overflow = Vec::with_capacity(num_leds as usize);
        dither_overflow.resize(num_leds as usize, [0i8; 3]);
        let mut lut = [[0u32; LUT_LEN]; 3];
        build_lut(&mut lut[0], options.white_point[0], options.lum_power);
        build_lut(&mut lut[1], options.white_point[1], options.lum_power);
        build_lut(&mut lut[2], options.white_point[2], options.lum_power);
        let brightness_u8 = (options.brightness.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
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
            lut,
            options,
            brightness_u8,
        })
    }

    /// Rotate buffers: prev<-current, current<-next
    fn rotate_frames(&mut self) {
        self.has_prev = false;
        if self.has_current {
            core::mem::swap(&mut self.prev, &mut self.current);
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
            let (or, og, ob) = self.apply_lut_dither(r, g, b, i);
            out[i * 3] = or;
            out[i * 3 + 1] = og;
            out[i * 3 + 2] = ob;
        }
    }

    fn render_interpolated(&mut self, now_us: u64, out: &mut [u8]) {
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
            let (or, og, ob) = self.apply_lut_dither(ir, ig, ib, i);
            out[i * 3] = or;
            out[i * 3 + 1] = og;
            out[i * 3 + 2] = ob;
        }
    }

    fn apply_lut_dither(&mut self, r: u32, g: u32, b: u32, pixel: usize) -> (u8, u8, u8) {
        let mut ir = if self.options.lut_enabled {
            lut_interpolate(r, &self.lut[0])
        } else {
            r
        };
        let mut ig = if self.options.lut_enabled {
            lut_interpolate(g, &self.lut[1])
        } else {
            g
        };
        let mut ib = if self.options.lut_enabled {
            lut_interpolate(b, &self.lut[2])
        } else {
            b
        };
        let brightness = self.brightness_u8;
        if brightness < 255 {
            ir = (ir * brightness as u32) >> 8;
            ig = (ig * brightness as u32) >> 8;
            ib = (ib * brightness as u32) >> 8;
        }

        // Shared luminance dithering for low-gray grayscale disabled for now:
        // was causing colored light to appear monochrome; grayscale check was insufficient
        let use_shared_luma = false;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
