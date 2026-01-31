//! Pre-computed mapping computation

use alloc::vec::Vec;
use libm;
use lp_glsl_builtins::glsl::q32::types::q32::Q32;
use lp_model::FrameId;
use lp_model::nodes::fixture::mapping::MappingConfig;

use super::entry::PixelMappingEntry;
use super::overlap::circle::circle_pixel_overlap;
use super::points::generate_mapping_points;
use super::structure::PrecomputedMapping;

/// Compute pre-computed mapping from configuration
///
/// # Arguments
/// * `config` - Mapping configuration
/// * `texture_width` - Texture width in pixels
/// * `texture_height` - Texture height in pixels
/// * `mapping_data_ver` - FrameId for version tracking
///
/// # Returns
/// PrecomputedMapping with entries ordered by pixel (x, y)
pub fn compute_mapping(
    config: &MappingConfig,
    texture_width: u32,
    texture_height: u32,
    mapping_data_ver: FrameId,
) -> PrecomputedMapping {
    let mut mapping = PrecomputedMapping::new(texture_width, texture_height, mapping_data_ver);

    match config {
        MappingConfig::PathPoints {
            paths: _,
            sample_diameter: _,
        } => {
            // First pass: collect all mapping points (circles)
            let mapping_points = generate_mapping_points(config, texture_width, texture_height);

            // Second pass: for each pixel, compute contributions from all circles
            let mut pixel_contributions: Vec<Vec<(u32, f32)>> =
                Vec::with_capacity((texture_width * texture_height) as usize);
            pixel_contributions.resize((texture_width * texture_height) as usize, Vec::new());

            // Track total weight per channel for normalization
            let mut channel_totals: Vec<f32> = Vec::new();
            let max_channel = mapping_points.iter().map(|p| p.channel).max().unwrap_or(0);
            channel_totals.resize((max_channel + 1) as usize, 0.0);

            for mapping_point in &mapping_points {
                // Convert normalized coordinates to pixel coordinates
                let center_x = mapping_point.center[0] * texture_width as f32;
                let center_y = mapping_point.center[1] * texture_height as f32;
                // Convert normalized radius to pixel radius
                let radius = mapping_point.radius * texture_width.max(texture_height) as f32;

                // Find pixels that might overlap with this circle
                let min_x = (libm::floorf(center_x - radius) as i32).max(0) as u32;
                let max_x =
                    (libm::ceilf(center_x + radius) as i32).min(texture_width as i32 - 1) as u32;
                let min_y = (libm::floorf(center_y - radius) as i32).max(0) as u32;
                let max_y =
                    (libm::ceilf(center_y + radius) as i32).min(texture_height as i32 - 1) as u32;

                for y in min_y..=max_y {
                    for x in min_x..=max_x {
                        let weight = circle_pixel_overlap(center_x, center_y, radius, x, y);
                        if weight > 0.0 {
                            let pixel_idx = (y * texture_width + x) as usize;
                            pixel_contributions[pixel_idx].push((mapping_point.channel, weight));
                            // Accumulate total weight per channel
                            channel_totals[mapping_point.channel as usize] += weight;
                        }
                    }
                }
            }

            // Third pass: normalize weights per-channel and build entries
            // Each channel's total contribution from all pixels should sum to 1.0
            for y in 0..texture_height {
                for x in 0..texture_width {
                    let pixel_idx = (y * texture_width + x) as usize;
                    let contributions = &pixel_contributions[pixel_idx];

                    if contributions.is_empty() {
                        // No contributions - add SKIP entry
                        mapping.entries.push(PixelMappingEntry::skip());
                    } else {
                        // Normalize weights per-channel: divide by channel total
                        // This ensures each channel's total contribution from all pixels = 1.0
                        let normalized: Vec<(u32, f32)> = contributions
                            .iter()
                            .map(|(ch, w)| {
                                let channel_total = channel_totals[*ch as usize];
                                if channel_total > 0.0 {
                                    (*ch, *w / channel_total)
                                } else {
                                    (*ch, 0.0)
                                }
                            })
                            .collect();

                        // Add entries (last one has has_more = false)
                        for (idx, (channel, weight)) in normalized.iter().enumerate() {
                            let has_more = idx < normalized.len() - 1;
                            let contribution_q32 = Q32::from_f32(*weight);

                            mapping.entries.push(PixelMappingEntry::new(
                                *channel,
                                contribution_q32,
                                has_more,
                            ));
                        }
                    }
                }
            }
        }
    }

    mapping
}
