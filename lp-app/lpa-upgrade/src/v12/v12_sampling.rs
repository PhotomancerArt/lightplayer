//! What a format-12 shader sees at each lamp: the coordinate words and the
//! uniforms one sample stream binds, frozen (`lpc-engine`
//! `nodes/fixture/fixture_node.rs` `render_direct_fixture_control`,
//! `nodes/shader/shader_node.rs` `sample_visual_into` at `f90a0388a`).

use crate::v12::v12_coordinates::{
    normalized_f32_to_pixel_q16, normalized_f32_to_q16, normalized_q16_to_pixel_q16,
    pixel_q16_to_normalized_f32, strip_point_coord,
};
use crate::v12::v12_lamp_fit::fit_lamps;
use crate::v12::v12_pattern_frame::{V12PatternFrame, V12ScopeGeometry};
use crate::v12::v12_projection::{V12Projection, centre_scanline};
use crate::v12::v12_request_space::{V12ConsumerPolicy, V12Space, select_request_space};

/// How a fixture samples (`FixtureSamplingConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V12Sampling {
    /// One sample per lamp, at its coordinate.
    Direct,
    /// A texture of the whole render, averaged per lamp: no per-lamp
    /// coordinate reaches the shader, so this copy does not answer it.
    TextureArea,
}

/// The coordinate frame a shader asked for (`ShaderCoords`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V12Coords {
    /// Request pixels, y down (`"coords"` absent or `"pixels"`).
    Pixels,
    /// Pattern space: the lamp box, long side −1…1, y up (`"pattern"`).
    Pattern,
}

/// The space a shader renders in (`ShaderSpace`), with a 1D shader's answer
/// for a 2D request (`in_2d`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V12SourceSpace {
    TwoD,
    OneD { in_2d: V12Projection },
}

/// One format-12 fixture, as plain numbers.
#[derive(Debug, Clone, Copy)]
pub struct V12Fixture<'a> {
    /// Each lamp's document position, in the order its map2d renders them.
    pub positions: &'a [[f32; 2]],
    /// How many lamps each rendered segment holds, in order: a segment is a
    /// strand for the pitch (`for_each_mapping_center_in_strands`).
    pub segment_counts: &'a [u32],
    /// The map2d's `view_box`.
    pub view_box: [f32; 4],
    /// `render_size`, width and height.
    pub render_size: [u32; 2],
    pub sampling: V12Sampling,
    pub strip_order_meaningful: bool,
    pub wire_reversed: bool,
    pub policy: V12ConsumerPolicy,
}

/// One shader, as plain fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V12Source {
    pub coords: V12Coords,
    pub space: V12SourceSpace,
}

/// The uniforms a sample stream binds that say where the lamps are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct V12Uniforms {
    /// `outputSize`.
    pub output_size: [f32; 2],
    /// `patternExtent`, `patternPitch`, `lampCount`, for a pattern-space
    /// shader.
    pub pattern: Option<V12PatternUniforms>,
}

/// Pattern space's three scope intrinsics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct V12PatternUniforms {
    pub extent: [f32; 2],
    pub pitch: f32,
    pub lamp_count: f32,
}

/// What the shader sees, lamp by lamp.
#[derive(Debug, Clone, PartialEq)]
pub struct V12Sampled {
    /// The space the fixture asked in.
    pub request: V12Space,
    /// The shader's own space: how `words` are packed (`[x, y]` pairs in 2D,
    /// one word in 1D).
    pub shader_space: V12Space,
    /// Every lamp's coordinate as the shader receives it, Q16.16, in lamp
    /// order.
    pub words: Vec<i32>,
    pub uniforms: V12Uniforms,
}

impl V12Sampled {
    /// Words per lamp.
    #[must_use]
    pub fn lanes(&self) -> usize {
        match self.shader_space {
            V12Space::OneD => 1,
            V12Space::TwoD => 2,
        }
    }
}

/// The coordinates and uniforms `source` sees at every lamp of `fixture`
/// today. `Err` names why there is no per-lamp answer: a texture-area
/// fixture, or a fit the engine refuses.
pub fn sample_coordinates(
    fixture: &V12Fixture<'_>,
    source: &V12Source,
) -> Result<V12Sampled, String> {
    if fixture.sampling == V12Sampling::TextureArea {
        return Err(
            "it samples a texture of its whole render (texture_area), so no lamp has \
                    a coordinate of its own"
                .to_string(),
        );
    }
    let [width, height] = fixture.render_size;
    let fitted = fit_lamps(fixture.positions, fixture.view_box, width, height)?;
    let count = fitted.len() as u32;
    let declared = match source.space {
        V12SourceSpace::TwoD => V12Space::TwoD,
        V12SourceSpace::OneD { .. } => V12Space::OneD,
    };
    // Every format-12 fixture with a map2d carries 2D coordinates.
    let request = select_request_space(declared, fixture.strip_order_meaningful, true);
    let (output_width, output_height) = match request {
        V12Space::OneD => (count, 1),
        V12Space::TwoD => (width, height),
    };

    // The fixture's stream, in the request's space.
    let mut scope = None;
    let stream: Vec<i32> = match request {
        V12Space::TwoD => {
            let pixels: Vec<[i32; 2]> = fitted
                .iter()
                .map(|&[x, y]| {
                    [
                        normalized_q16_to_pixel_q16(normalized_f32_to_q16(x), width),
                        normalized_q16_to_pixel_q16(normalized_f32_to_q16(y), height),
                    ]
                })
                .collect();
            scope = V12ScopeGeometry::from_pixels(&pixels, &strand_starts(fixture, pixels.len()));
            pixels.into_iter().flatten().collect()
        }
        V12Space::OneD => (0..count)
            .map(|index| strip_point_coord(index, count, fixture.wire_reversed))
            .collect(),
    };

    let pattern = (source.coords == V12Coords::Pattern).then(|| match (declared, request) {
        (V12Space::TwoD, V12Space::TwoD) => V12PatternFrame::two_d(
            &scope.unwrap_or_else(|| V12ScopeGeometry::texel_centres(output_width, output_height)),
        ),
        (V12Space::TwoD, V12Space::OneD) => V12PatternFrame::two_d(
            &V12ScopeGeometry::texel_centres(output_width, output_height),
        ),
        (V12Space::OneD, V12Space::OneD) => V12PatternFrame::one_d_strip(output_width),
        (V12Space::OneD, V12Space::TwoD) => V12PatternFrame::one_d_projected(
            output_width,
            scope.map_or(output_width.saturating_mul(output_height), |s| s.lamp_count),
        ),
    });

    // Onto the shader's own space.
    let mut words: Vec<i32> = match (declared, request) {
        (V12Space::OneD, V12Space::TwoD) => {
            let V12SourceSpace::OneD { in_2d } = source.space else {
                unreachable!("declared 1D")
            };
            let cell = fixture.policy.resolve(Some(in_2d));
            stream
                .chunks_exact(2)
                .map(|pair| {
                    let u = pixel_q16_to_normalized_f32(pair[0], output_width);
                    let v = pixel_q16_to_normalized_f32(pair[1], output_height);
                    normalized_f32_to_pixel_q16(cell.project(u, v), output_width)
                })
                .collect()
        }
        (V12Space::TwoD, V12Space::OneD) => stream
            .iter()
            .flat_map(|&t| {
                let (u, v) = centre_scanline(pixel_q16_to_normalized_f32(t, output_width));
                [
                    normalized_f32_to_pixel_q16(u, output_width),
                    normalized_f32_to_pixel_q16(v, output_height),
                ]
            })
            .collect(),
        _ => stream,
    };
    if let Some(frame) = &pattern {
        frame.map_words(&mut words);
    }

    Ok(V12Sampled {
        request,
        shader_space: declared,
        words,
        uniforms: V12Uniforms {
            output_size: [output_width as f32, output_height as f32],
            pattern: pattern.map(|frame| V12PatternUniforms {
                extent: frame.extent,
                pitch: frame.pitch,
                lamp_count: frame.lamp_count as f32,
            }),
        },
    })
}

/// For each lamp, whether it opens a strand: the first lamp of each segment.
fn strand_starts(fixture: &V12Fixture<'_>, lamps: usize) -> Vec<bool> {
    let mut starts = vec![false; lamps];
    let mut at = 0usize;
    for &count in fixture.segment_counts {
        if count > 0 && at < lamps {
            starts[at] = true;
        }
        at = at.saturating_add(count as usize);
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v12::v12_coordinates::Q16_ONE;

    /// A 1D pattern shader on a strip fixture runs 0 → 1 along the wire.
    #[test]
    fn a_one_d_pattern_shader_runs_zero_to_one_along_the_wire() {
        let positions: Vec<[f32; 2]> = (0..5).map(|k| [k as f32, 0.0]).collect();
        let mut fixture = strip_fixture(&positions, &[5]);
        fixture.view_box = [0.0, -1.0, 4.0, 2.0];
        let sampled = sample_coordinates(
            &fixture,
            &V12Source {
                coords: V12Coords::Pattern,
                space: V12SourceSpace::OneD {
                    in_2d: V12Projection::default(),
                },
            },
        )
        .unwrap();
        assert_eq!(sampled.request, V12Space::OneD);
        assert_eq!(
            sampled.words,
            vec![0, Q16_ONE / 4, Q16_ONE / 2, 3 * Q16_ONE / 4, Q16_ONE]
        );
        assert_eq!(sampled.uniforms.output_size, [5.0, 1.0]);
    }

    /// A pixel shader on a mapped fixture sees request pixels.
    #[test]
    fn a_pixel_shader_sees_request_pixels() {
        let positions = [[0.0, 0.0], [10.0, 10.0]];
        let fixture = strip_fixture(&positions, &[2]);
        let sampled = sample_coordinates(
            &fixture,
            &V12Source {
                coords: V12Coords::Pixels,
                space: V12SourceSpace::TwoD,
            },
        )
        .unwrap();
        assert_eq!(sampled.words, vec![0, 0, 10 * Q16_ONE, 10 * Q16_ONE]);
        assert_eq!(sampled.uniforms.pattern, None);
    }

    fn strip_fixture<'a>(positions: &'a [[f32; 2]], counts: &'a [u32]) -> V12Fixture<'a> {
        V12Fixture {
            positions,
            segment_counts: counts,
            view_box: [0.0; 4],
            render_size: [10, 10],
            sampling: V12Sampling::Direct,
            strip_order_meaningful: true,
            wire_reversed: false,
            policy: V12ConsumerPolicy::default(),
        }
    }
}
