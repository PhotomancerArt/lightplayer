use crate::error::Error;
use crate::nodes::fixture::gamma::apply_gamma;
use crate::nodes::fixture::mapping::{
    accumulate_from_mapping, compute_mapping, generate_mapping_points, MappingPoint,
    PrecomputedMapping,
};
use crate::nodes::{NodeConfig, NodeRuntime};
use crate::runtime::contexts::{NodeInitContext, OutputHandle, RenderContext, TextureHandle};
use alloc::{boxed::Box, string::String, vec::Vec};
use lp_glsl_builtins::glsl::q32::types::q32::ToQ32;
use lp_model::nodes::fixture::{ColorOrder, FixtureConfig};
use lp_model::FrameId;
use lp_shared::fs::fs_event::FsChange;

/// Fixture node runtime
pub struct FixtureRuntime {
    config: Option<FixtureConfig>,
    texture_handle: Option<TextureHandle>,
    output_handle: Option<OutputHandle>,
    color_order: ColorOrder,
    mapping: Vec<MappingPoint>,
    transform: [[f32; 4]; 4],
    texture_width: Option<u32>,
    texture_height: Option<u32>,
    /// Pre-computed pixel-to-channel mapping
    precomputed_mapping: Option<PrecomputedMapping>,
    /// Last sampled lamp colors (RGB per lamp, ordered by channel index)
    lamp_colors: Vec<u8>,
    /// Brightness level (0-255), defaults to 64
    brightness: u8,
    /// Enable gamma correction, defaults to true
    gamma_correction: bool,
}

impl FixtureRuntime {
    pub fn new() -> Self {
        Self {
            config: None,
            texture_handle: None,
            output_handle: None,
            color_order: ColorOrder::Rgb,
            mapping: Vec::new(),
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ], // Identity matrix
            texture_width: None,
            texture_height: None,
            precomputed_mapping: None,
            lamp_colors: Vec::new(),
            brightness: 64,
            gamma_correction: true,
        }
    }

    pub fn set_config(&mut self, config: FixtureConfig) {
        self.config = Some(config);
    }

    /// Get the fixture config (for state extraction)
    pub fn get_config(&self) -> Option<&FixtureConfig> {
        self.config.as_ref()
    }

    /// Get mapping points (for state extraction)
    pub fn get_mapping(&self) -> &Vec<MappingPoint> {
        &self.mapping
    }

    /// Get transform matrix (for state extraction)
    pub fn get_transform(&self) -> [[f32; 4]; 4] {
        self.transform
    }

    /// Get texture handle (for state extraction)
    pub fn get_texture_handle(&self) -> Option<TextureHandle> {
        self.texture_handle
    }

    /// Get output handle (for state extraction)
    pub fn get_output_handle(&self) -> Option<OutputHandle> {
        self.output_handle
    }

    /// Get lamp colors (for state extraction)
    /// Returns RGB values per lamp, ordered by channel index (3 bytes per lamp)
    pub fn get_lamp_colors(&self) -> &[u8] {
        &self.lamp_colors
    }

    /// Regenerate mapping when texture resolution changes or config versions change
    fn regenerate_mapping_if_needed(
        &mut self,
        texture_width: u32,
        texture_height: u32,
        our_config_ver: FrameId,
        texture_config_ver: FrameId,
    ) -> Result<(), Error> {
        let needs_regeneration = self
            .texture_width
            .map(|w| w != texture_width)
            .unwrap_or(true)
            || self
                .texture_height
                .map(|h| h != texture_height)
                .unwrap_or(true)
            || self
                .precomputed_mapping
                .as_ref()
                .map(|m| {
                    let max_config_ver = our_config_ver.max(texture_config_ver);
                    max_config_ver > m.mapping_data_ver
                })
                .unwrap_or(true);

        if needs_regeneration {
            let config = self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
                node_path: String::from("fixture"),
                reason: String::from("Config not set"),
            })?;

            // Compute new pre-computed mapping
            let max_config_ver = our_config_ver.max(texture_config_ver);
            let mapping = compute_mapping(
                &config.mapping,
                texture_width,
                texture_height,
                max_config_ver,
            );

            self.precomputed_mapping = Some(mapping);

            // Update texture dimensions
            self.texture_width = Some(texture_width);
            self.texture_height = Some(texture_height);

            // Keep existing mapping points for now (used by state extraction)
            self.mapping = generate_mapping_points(&config.mapping, texture_width, texture_height);
        }

        Ok(())
    }
}

impl NodeRuntime for FixtureRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // Get config
        let config = self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
            node_path: String::from("fixture"),
            reason: String::from("Config not set"),
        })?;

        // Resolve texture handle
        let texture_handle = ctx.resolve_texture(&config.texture_spec)?;
        self.texture_handle = Some(texture_handle);

        // Resolve output handle
        let output_handle = ctx.resolve_output(&config.output_spec)?;
        self.output_handle = Some(output_handle);

        // Store config values
        self.color_order = config.color_order;
        self.transform = config.transform;
        self.brightness = config.brightness.unwrap_or(64);
        self.gamma_correction = config.gamma_correction.unwrap_or(true);

        // Mapping will be generated in render() when texture is available
        // Texture dimensions are not available in init() (texture is lazy-loaded)
        self.mapping = Vec::new();

        Ok(())
    }

    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
        // Get texture handle
        let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
            message: String::from("Texture handle not resolved"),
        })?;

        // Get texture (triggers lazy rendering if needed)
        let texture = ctx.get_texture(texture_handle)?;

        let texture_width = texture.width();
        let texture_height = texture.height();

        // Regenerate mapping if texture resolution changed
        // TODO: Get proper config versions from context
        let our_config_ver = FrameId::new(0);
        let texture_config_ver = FrameId::new(0);
        self.regenerate_mapping_if_needed(
            texture_width,
            texture_height,
            our_config_ver,
            texture_config_ver,
        )?;

        // Get pre-computed mapping
        let mapping = self
            .precomputed_mapping
            .as_ref()
            .ok_or_else(|| Error::Other {
                message: String::from("Precomputed mapping not available"),
            })?;

        // Accumulate channel values using format-specific sampling
        let texture_data = texture.data();
        let texture_format = texture.format();
        let accumulators = accumulate_from_mapping(
            &mapping.entries,
            texture_data,
            texture_format,
            texture_width,
            texture_height,
        );

        let max_channel = accumulators.max_channel;
        let ch_values_r = &accumulators.r;
        let ch_values_g = &accumulators.g;
        let ch_values_b = &accumulators.b;

        // Get output handle
        let output_handle = self.output_handle.ok_or_else(|| Error::Other {
            message: String::from("Output handle not resolved"),
        })?;

        // Store lamp colors for state extraction
        // Create dense array: each channel uses 3 bytes (RGB)
        self.lamp_colors.clear();
        self.lamp_colors.resize((max_channel as usize + 1) * 3, 0);

        let brightness = self.brightness.to_q32() / 255.to_q32();

        // Write sampled values to output buffer
        // For now, use universe 0 and channel_offset 0 (sequential writing)
        // TODO: Add universe and channel_offset fields to FixtureConfig when needed
        let universe = 0u32;
        let channel_offset = 0u32;
        for channel in 0..=max_channel as usize {
            let mut r = (ch_values_r[channel] * brightness).to_u8_clamped();
            let mut g = (ch_values_g[channel] * brightness).to_u8_clamped();
            let mut b = (ch_values_b[channel] * brightness).to_u8_clamped();

            let idx = channel * 3;
            self.lamp_colors[idx] = r;
            self.lamp_colors[idx + 1] = g;
            self.lamp_colors[idx + 2] = b;

            // Apply gamma correction if enabled, _after_ writing to lamp_colors, which should
            // not be gamma corrected
            if self.gamma_correction {
                r = apply_gamma(r);
                g = apply_gamma(g);
                b = apply_gamma(b);
            }

            let start_ch = channel_offset + (channel as u32) * 3; // 3 bytes per RGB
            let buffer = ctx.get_output(output_handle, universe, start_ch, 3)?;
            self.color_order.write_rgb(buffer, 0, r, g, b);
        }

        Ok(())
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }

    fn update_config(
        &mut self,
        new_config: Box<dyn NodeConfig>,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        // Downcast to FixtureConfig
        let fixture_config = new_config
            .as_any()
            .downcast_ref::<FixtureConfig>()
            .ok_or_else(|| Error::InvalidConfig {
                node_path: String::from("fixture"),
                reason: String::from("Config is not a FixtureConfig"),
            })?;

        let old_config = self.config.as_ref();
        let texture_changed = old_config
            .map(|old| old.texture_spec != fixture_config.texture_spec)
            .unwrap_or(true);
        let output_changed = old_config
            .map(|old| old.output_spec != fixture_config.output_spec)
            .unwrap_or(true);

        self.config = Some(fixture_config.clone());
        self.color_order = fixture_config.color_order;
        self.transform = fixture_config.transform;

        // Re-resolve handles if they changed
        if texture_changed {
            let texture_handle = ctx.resolve_texture(&fixture_config.texture_spec)?;
            self.texture_handle = Some(texture_handle);
        }

        if output_changed {
            let output_handle = ctx.resolve_output(&fixture_config.output_spec)?;
            self.output_handle = Some(output_handle);
        }

        // Regenerate mapping if we have texture dimensions
        // If texture dimensions not available, mapping will be regenerated in render()
        if let (Some(width), Some(height)) = (self.texture_width, self.texture_height) {
            self.mapping = generate_mapping_points(&fixture_config.mapping, width, height);
        } else {
            // Texture dimensions not available, clear mapping - will be regenerated in render()
            self.mapping = Vec::new();
        }

        Ok(())
    }

    fn handle_fs_change(
        &mut self,
        _change: &FsChange,
        _ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        // Fixtures don't currently support external mapping files
        // This is a no-op for now
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lp_model::nodes::fixture::mapping::{MappingConfig, PathSpec, RingOrder};

    #[test]
    fn test_fixture_runtime_creation() {
        let runtime = FixtureRuntime::new();
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }

    #[test]
    fn test_pixel_index_advancement() {
        // Test that pixel_index advances correctly
        // Simulate: pixel 0 has 2 entries (channels 0 and 1), pixel 1 has 1 entry (channel 0)
        use crate::nodes::fixture::mapping::{PixelMappingEntry, PrecomputedMapping};
        use lp_glsl_builtins::glsl::q32::types::q32::Q32;
        use lp_model::FrameId;

        let mut mapping = PrecomputedMapping::new(2, 1, FrameId::new(1));
        // Pixel 0: channel 0 (has_more = true)
        mapping
            .entries
            .push(PixelMappingEntry::new(0, Q32::from_f32(0.5), true));
        // Pixel 0: channel 1 (has_more = false) - last entry for pixel 0
        mapping
            .entries
            .push(PixelMappingEntry::new(1, Q32::from_f32(0.5), false));
        // Pixel 1: channel 0 (has_more = false) - only entry for pixel 1
        mapping
            .entries
            .push(PixelMappingEntry::new(0, Q32::from_f32(1.0), false));

        let mut pixel_index = 0u32;
        let texture_width = 2u32;
        let mut processed_pixels = Vec::new();

        for entry in &mapping.entries {
            if entry.is_skip() {
                pixel_index += 1;
                continue;
            }

            let x = pixel_index % texture_width;
            processed_pixels.push((x, entry.channel()));

            if !entry.has_more() {
                pixel_index += 1;
            }
        }

        // Should process: pixel 0 (channel 0), pixel 0 (channel 1), pixel 1 (channel 0)
        assert_eq!(processed_pixels.len(), 3);
        assert_eq!(
            processed_pixels[0],
            (0, 0),
            "First entry should be pixel 0, channel 0"
        );
        assert_eq!(
            processed_pixels[1],
            (0, 1),
            "Second entry should be pixel 0, channel 1"
        );
        assert_eq!(
            processed_pixels[2],
            (1, 0),
            "Third entry should be pixel 1, channel 0"
        );
    }

    #[test]
    fn test_channel_contribution_sum() {
        // Test that all pixel contributions to a channel sum correctly
        // Create a simple mapping: one circle (one channel) that covers some pixels
        use crate::nodes::fixture::mapping::compute_mapping;
        use lp_model::FrameId;
        use lp_model::nodes::fixture::mapping::{MappingConfig, PathSpec, RingOrder};

        // Create a simple config: one ring with 1 lamp at center
        let config = MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.5, 0.5),
                diameter: 0.2, // Small diameter
                start_ring_inclusive: 0,
                end_ring_exclusive: 1,
                ring_lamp_counts: vec![1],
                offset_angle: 0.0,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 4.0, // Sample diameter in pixels
        };

        // Build mapping for a small texture
        let texture_width = 32u32;
        let texture_height = 32u32;
        let mapping = compute_mapping(&config, texture_width, texture_height, FrameId::new(1));

        // Sum up all contributions to channel 0 from all pixels
        // Decode contributions the same way the runtime does
        let mut total_contribution_ch0 = 0.0f64;

        for entry in &mapping.entries {
            if entry.is_skip() {
                continue;
            }

            if entry.channel() == 0 {
                // Decode the same way runtime.rs does (line 370-381)
                let contribution_raw = entry.contribution_raw() as i32;
                let contribution_float = if contribution_raw == 0 {
                    1.0 // Full contribution
                } else {
                    // Use raw value directly as Q32 fraction
                    contribution_raw as f64 / 65536.0
                };
                total_contribution_ch0 += contribution_float;
            }
        }

        // After fixing normalization to be per-channel instead of per-pixel,
        // the total contribution to each channel should sum to approximately 1.0
        assert!(
            (total_contribution_ch0 - 1.0).abs() < 0.1,
            "Total contribution to channel 0 should be ~1.0 (normalized per-channel), got {}",
            total_contribution_ch0
        );
    }

    // Test helper: create RingArray path spec
    fn create_ring_array_path(
        center: (f32, f32),
        diameter: f32,
        start_ring: u32,
        end_ring: u32,
        ring_lamp_counts: Vec<u32>,
        offset_angle: f32,
        order: RingOrder,
    ) -> PathSpec {
        PathSpec::RingArray {
            center,
            diameter,
            start_ring_inclusive: start_ring,
            end_ring_exclusive: end_ring,
            ring_lamp_counts,
            offset_angle,
            order,
        }
    }

    #[test]
    fn test_single_ring_center() {
        // Single ring at center (ring_index = 0) with 8 lamps
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![8], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify 8 points generated
        assert_eq!(points.len(), 8);

        // Verify all points at center position (radius = 0 for single ring)
        for point in &points {
            assert!((point.center[0] - 0.5).abs() < 0.001);
            assert!((point.center[1] - 0.5).abs() < 0.001);
        }

        // Verify channels 0-7 assigned sequentially
        for (i, point) in points.iter().enumerate() {
            assert_eq!(point.channel, i as u32);
        }

        // Verify angles evenly spaced (0, π/4, π/2, ...)
        // Since all points are at center, angles don't matter, but verify structure
        assert_eq!(points[0].channel, 0);
        assert_eq!(points[7].channel, 7);
    }

    #[test]
    fn test_multiple_rings() {
        // Multiple rings with different lamp counts
        // Ring 0: 1 lamp (center)
        // Ring 1: 8 lamps
        // Ring 2: 16 lamps
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 8, 16],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify correct number of points (1 + 8 + 16 = 25)
        assert_eq!(points.len(), 25);

        // Verify channels assigned sequentially (0-24)
        for (i, point) in points.iter().enumerate() {
            assert_eq!(point.channel, i as u32);
        }

        // Verify ring 0 (center) has 1 point at center
        assert_eq!(points[0].channel, 0);
        assert!((points[0].center[0] - 0.5).abs() < 0.001);
        assert!((points[0].center[1] - 0.5).abs() < 0.001);

        // Verify ring 1 has 8 points (channels 1-8)
        // Verify ring 2 has 16 points (channels 9-24)
        assert_eq!(points[1].channel, 1);
        assert_eq!(points[8].channel, 8);
        assert_eq!(points[9].channel, 9);
        assert_eq!(points[24].channel, 24);
    }

    #[test]
    fn test_inner_first_ordering() {
        // Multiple rings with different lamp counts
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 4, 8],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify channels assigned inner→outer
        // Ring 0: channels 0-0 (1 lamp)
        // Ring 1: channels 1-4 (4 lamps)
        // Ring 2: channels 5-12 (8 lamps)
        assert_eq!(points[0].channel, 0); // Ring 0, first lamp
        assert_eq!(points[1].channel, 1); // Ring 1, first lamp
        assert_eq!(points[5].channel, 5); // Ring 2, first lamp
        assert_eq!(points[12].channel, 12); // Ring 2, last lamp
    }

    #[test]
    fn test_outer_first_ordering() {
        // Multiple rings with different lamp counts
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 4, 8],
            0.0,
            RingOrder::OuterFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify channels assigned outer→inner
        // Ring 2: channels 0-7 (8 lamps, outer)
        // Ring 1: channels 8-11 (4 lamps)
        // Ring 0: channel 12 (1 lamp, inner)
        assert_eq!(points[0].channel, 0); // Ring 2, first lamp (outer)
        assert_eq!(points[7].channel, 7); // Ring 2, last lamp
        assert_eq!(points[8].channel, 8); // Ring 1, first lamp
        assert_eq!(points[11].channel, 11); // Ring 1, last lamp
        assert_eq!(points[12].channel, 12); // Ring 0, only lamp (inner)
    }

    #[test]
    fn test_offset_angle() {
        // Single ring with offset angle
        let path = create_ring_array_path(
            (0.5, 0.5),
            0.5,
            0,
            1,
            vec![4],
            core::f32::consts::PI / 4.0, // π/4 offset
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify 4 points generated
        assert_eq!(points.len(), 4);

        // Verify first lamp at angle π/4 (not 0)
        // For ring at radius 0 (center), all points are at center, so angles don't affect position
        // But verify structure is correct
        assert_eq!(points[0].channel, 0);
        assert_eq!(points[3].channel, 3);
    }

    #[test]
    fn test_coordinate_correctness() {
        // Test coordinates are in [0, 1] range
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            2,
            vec![1, 8],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        for point in &points {
            // Verify coordinates in [0, 1] range
            assert!(point.center[0] >= 0.0 && point.center[0] <= 1.0);
            assert!(point.center[1] >= 0.0 && point.center[1] <= 1.0);
            assert!(point.radius >= 0.0 && point.radius <= 1.0);
        }
    }

    #[test]
    fn test_coordinate_edge_cases() {
        // Test edge cases: center at (0, 0), (1, 1), (0.5, 0.5)
        for center in [(0.0, 0.0), (1.0, 1.0), (0.5, 0.5)] {
            let path =
                create_ring_array_path(center, 0.5, 0, 1, vec![4], 0.0, RingOrder::InnerFirst);
            let config = MappingConfig::PathPoints {
                paths: vec![path],
                sample_diameter: 2.0,
            };

            let points = generate_mapping_points(&config, 100, 100);

            for point in &points {
                // Verify coordinates clamped to [0, 1]
                assert!(point.center[0] >= 0.0 && point.center[0] <= 1.0);
                assert!(point.center[1] >= 0.0 && point.center[1] <= 1.0);
            }
        }
    }

    #[test]
    fn test_sample_diameter_conversion() {
        // Test sample diameter to normalized radius conversion
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![1], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        // Test with square texture (100x100)
        let points_square = generate_mapping_points(&config, 100, 100);
        assert_eq!(points_square.len(), 1);
        // sample_diameter = 2.0, max_dimension = 100, normalized_radius = (2.0 / 2.0) / 100 = 0.01
        assert!((points_square[0].radius - 0.01).abs() < 0.0001);

        // Test with wide texture (200x100)
        let points_wide = generate_mapping_points(&config, 200, 100);
        assert_eq!(points_wide.len(), 1);
        // sample_diameter = 2.0, max_dimension = 200, normalized_radius = (2.0 / 2.0) / 200 = 0.005
        assert!((points_wide[0].radius - 0.005).abs() < 0.0001);

        // Test with tall texture (100x200)
        let points_tall = generate_mapping_points(&config, 100, 200);
        assert_eq!(points_tall.len(), 1);
        // sample_diameter = 2.0, max_dimension = 200, normalized_radius = (2.0 / 2.0) / 200 = 0.005
        assert!((points_tall[0].radius - 0.005).abs() < 0.0001);
    }

    #[test]
    fn test_channel_assignment_multiple_paths() {
        // Multiple paths with different LED counts
        let path1 =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![5], 0.0, RingOrder::InnerFirst);
        let path2 =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![3], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path1, path2],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Verify channels sequential with no gaps
        // Path 1: channels 0-4 (5 LEDs)
        // Path 2: channels 5-7 (3 LEDs)
        assert_eq!(points.len(), 8);
        assert_eq!(points[0].channel, 0);
        assert_eq!(points[4].channel, 4);
        assert_eq!(points[5].channel, 5);
        assert_eq!(points[7].channel, 7);
    }

    // Note: test_channel_offset removed - channel_offset is now handled internally
    // by generate_mapping_points which accumulates offsets across paths

    #[test]
    fn test_edge_cases_empty_ring() {
        // Test with zero lamp count for a ring
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            2,
            vec![1, 0],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Should only generate 1 point (ring 0), ring 1 has 0 lamps
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].channel, 0);
    }

    #[test]
    fn test_edge_cases_invalid_ring_indices() {
        // Test with start_ring >= end_ring
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 2, 2, vec![], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Should generate no points (empty range)
        assert_eq!(points.len(), 0);
    }

    #[test]
    fn test_edge_cases_single_lamp() {
        // Test with single lamp in a ring
        let path =
            create_ring_array_path((0.5, 0.5), 1.0, 0, 1, vec![1], 0.0, RingOrder::InnerFirst);
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].channel, 0);
        assert!((points[0].center[0] - 0.5).abs() < 0.001);
        assert!((points[0].center[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_ring_radius_calculation() {
        // Test that ring radii increase correctly
        let path = create_ring_array_path(
            (0.5, 0.5),
            1.0,
            0,
            3,
            vec![1, 8, 16],
            0.0,
            RingOrder::InnerFirst,
        );
        let config = MappingConfig::PathPoints {
            paths: vec![path],
            sample_diameter: 2.0,
        };

        let points = generate_mapping_points(&config, 100, 100);

        // Ring 0 (center): radius should be 0
        assert!((points[0].center[0] - 0.5).abs() < 0.001);
        assert!((points[0].center[1] - 0.5).abs() < 0.001);

        // Ring 1: should have non-zero radius
        let ring1_radius =
            ((points[1].center[0] - 0.5).powi(2) + (points[1].center[1] - 0.5).powi(2)).sqrt();
        assert!(ring1_radius > 0.0);

        // Ring 2: should have larger radius than ring 1
        let ring2_radius =
            ((points[9].center[0] - 0.5).powi(2) + (points[9].center[1] - 0.5).powi(2)).sqrt();
        assert!(ring2_radius > ring1_radius);
    }
}
