//! Channel accumulation from texture sampling

use super::entry::PixelMappingEntry;
use super::sampling::create_sampler;
use alloc::vec::Vec;
use lp_glsl_builtins::glsl::q32::types::q32::{Q32, ToQ32};

/// Channel accumulator result
pub struct ChannelAccumulators {
    pub r: Vec<Q32>,
    pub g: Vec<Q32>,
    pub b: Vec<Q32>,
    pub max_channel: u32,
}

/// Initialize channel accumulators
///
/// Finds the maximum channel index from mapping entries and creates Q32 vectors
/// for R, G, and B channels.
pub fn initialize_channel_accumulators(entries: &[PixelMappingEntry]) -> ChannelAccumulators {
    let max_channel = entries
        .iter()
        .filter_map(|e| {
            if !e.is_skip() {
                Some(e.channel())
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0);

    let mut ch_values_r: Vec<Q32> = Vec::with_capacity((max_channel + 1) as usize);
    let mut ch_values_g: Vec<Q32> = Vec::with_capacity((max_channel + 1) as usize);
    let mut ch_values_b: Vec<Q32> = Vec::with_capacity((max_channel + 1) as usize);
    ch_values_r.resize((max_channel + 1) as usize, Q32::ZERO);
    ch_values_g.resize((max_channel + 1) as usize, Q32::ZERO);
    ch_values_b.resize((max_channel + 1) as usize, Q32::ZERO);

    ChannelAccumulators {
        r: ch_values_r,
        g: ch_values_g,
        b: ch_values_b,
        max_channel,
    }
}

/// Accumulate values from mapping entries using format-specific texture sampling
///
/// Processes mapping entries, samples texture using the appropriate sampler,
/// and accumulates RGB values into channel accumulators.
///
/// # Arguments
/// * `entries` - Precomputed mapping entries
/// * `texture_data` - Raw texture pixel data
/// * `texture_format` - Texture format
/// * `texture_width` - Texture width in pixels
/// * `texture_height` - Texture height in pixels
///
/// # Returns
/// Channel accumulators with accumulated RGB values per channel
pub fn accumulate_from_mapping(
    entries: &[PixelMappingEntry],
    texture_data: &[u8],
    texture_format: lp_model::nodes::texture::TextureFormat,
    texture_width: u32,
    texture_height: u32,
) -> ChannelAccumulators {
    let mut accumulators = initialize_channel_accumulators(entries);

    // Create format-specific sampler
    let sampler = create_sampler(texture_format);

    // Iterate through entries and accumulate
    // Entries are ordered by pixel (x, y), with consecutive entries per pixel
    let mut pixel_index = 0u32;

    for entry in entries {
        if entry.is_skip() {
            // SKIP entry - advance to next pixel
            pixel_index += 1;
            continue;
        }

        // Get pixel coordinates
        let x = pixel_index % texture_width;
        let y = pixel_index / texture_width;

        // Sample pixel using format-specific sampler
        if let Some(pixel) = sampler.sample_pixel(texture_data, x, y, texture_width, texture_height)
        {
            let channel = entry.channel() as usize;
            if channel < accumulators.r.len() {
                let contribution_raw = entry.contribution_raw() as i32;

                let pixel_r = pixel[0];
                let pixel_g = pixel[1];
                let pixel_b = pixel[2];

                if contribution_raw == 0 {
                    // Full contribution (100%)
                    accumulators.r[channel] += pixel_r.to_q32();
                    accumulators.g[channel] += pixel_g.to_q32();
                    accumulators.b[channel] += pixel_b.to_q32();
                } else {
                    let frac = Q32(contribution_raw);

                    // Note: this is safe, because frac.0 is in range [1, 65535]
                    //       and thusly cannot overflow when multiplying by an 8-bit value
                    //       0xFF * 0xFFFF = 0xFEFF01
                    //
                    //       it also converts into Q32 because of the natural shift
                    //       from multiplying by a Q32 value, but is much faster than
                    //       doing a full Q32 multiplication

                    let accumulated_r = Q32((pixel_r as i32) * frac.0);
                    let accumulated_g = Q32((pixel_g as i32) * frac.0);
                    let accumulated_b = Q32((pixel_b as i32) * frac.0);

                    accumulators.r[channel] += accumulated_r;
                    accumulators.g[channel] += accumulated_g;
                    accumulators.b[channel] += accumulated_b;
                }
            }
        }

        // Advance pixel_index if this is the last entry for this pixel
        if !entry.has_more() {
            pixel_index += 1;
        }
    }

    accumulators
}
