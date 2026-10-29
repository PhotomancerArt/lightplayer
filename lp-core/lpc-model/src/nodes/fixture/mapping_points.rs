//! Mapping point generation from configuration
//!
//! Generates LED sample points (texture-space centers + radii) from a
//! fixture's mapping, viewed through [`MappingRef`]: the document-resolved
//! [`ResolvedMappingCompact`] holds the lamps; the slot form
//! ([`MappingConfig`]: unset, or a map2d source not yet read) holds none.
//! Lives in the model crate so both the engine and Studio-side tooling can
//! share the generator.

use alloc::vec::Vec;

use crate::nodes::fixture::{MappingRef, ResolvedMappingCompact};

/// Mapping point representing a single LED sampling location
#[derive(Debug, Clone)]
pub struct MappingPoint {
    pub channel: u32,
    pub center: [f32; 2], // Texture space coordinates [0, 1]
    pub radius: f32,
}

/// Number of mapping points [`for_each_mapping_point`] will visit for
/// `mapping`, without generating any of them.
///
/// Lets consumers size their output buffers exactly — the whole point of
/// the streaming API is that nothing on these paths grows by doubling.
pub fn mapping_point_count<'a>(mapping: impl Into<MappingRef<'a>>) -> usize {
    match mapping.into() {
        MappingRef::Slots(_) => 0,
        MappingRef::Compact(compact) => compact.lamp_count(),
    }
}

/// Visit every mapping point in channel-assignment order without
/// materializing the point list.
///
/// `f(visit_index, point)` receives the running 0-based position of the
/// point in the visit order (NOT its channel — a point's channel is its
/// span's `first_channel` plus its offset within the span, and spans need
/// not be contiguous or ordered). The visit order is spans in
/// channel-assignment order, points in span order. That order is the
/// contract every consumer's buffer layout depends on; see
/// `visitor_matches_generate_mapping_points_across_configs`.
///
/// The slot form holds no lamps: `Unset`, and `Map2d`, an authored source
/// reference the loader reads into a [`MappingRef::Compact`] carrier before
/// this runs.
pub fn for_each_mapping_point<'a>(
    mapping: impl Into<MappingRef<'a>>,
    texture_width: u32,
    texture_height: u32,
    f: impl FnMut(usize, MappingPoint),
) {
    match mapping.into() {
        MappingRef::Slots(_) => {}
        MappingRef::Compact(compact) => {
            for_each_compact_mapping_point(compact, texture_width, texture_height, f)
        }
    }
}

fn for_each_compact_mapping_point(
    compact: &ResolvedMappingCompact,
    texture_width: u32,
    texture_height: u32,
    mut f: impl FnMut(usize, MappingPoint),
) {
    let normalized_radius =
        normalized_sample_radius(compact.sample_diameter, texture_width, texture_height);
    let mut visit_index = 0usize;

    for span in &compact.spans {
        // Clamp against the point list rather than indexing on faith: the
        // carrier's invariant says these agree, and a carrier that broke it
        // should truncate, not panic. `mapping_point_count` clamps the same
        // way, so the count and the visit can never disagree.
        let end = visit_index
            .saturating_add(span.count as usize)
            .min(compact.points.len());
        for (offset, [x, y]) in compact.points[visit_index..end].iter().copied().enumerate() {
            f(
                visit_index + offset,
                MappingPoint {
                    channel: span.first_channel.saturating_add(offset as u32),
                    // Already clamped by the document fit; clamped again so
                    // the answer holds for any carrier.
                    center: [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)],
                    radius: normalized_radius,
                },
            );
        }
        visit_index = end;
    }
}

/// The lamp centres of `mapping` in visit order, unclamped, one at a time —
/// the streaming twin of [`for_each_mapping_point`] for consumers that only
/// need coordinates and cannot afford the per-point clamp (the fixture's
/// per-render batch fill on a part with no FPU; its Q16 conversion clamps on
/// the bit pattern instead, to the same answer). Same visit order — spans in
/// channel-assignment order, points in span order, clamped to the point
/// list — pinned by `mapping_centers_visits_what_for_each_mapping_point_visits`.
///
/// Resumable, unlike the visitor: a batched consumer takes `capacity` points
/// at a time across as many calls as the product needs.
pub fn mapping_centers<'a>(mapping: impl Into<MappingRef<'a>>) -> MappingCenters<'a> {
    let points = match mapping.into() {
        MappingRef::Slots(_) => [].iter(),
        MappingRef::Compact(compact) => compact.points[..compact.lamp_count()].iter(),
    };
    MappingCenters { points }
}

/// See [`mapping_centers`].
pub struct MappingCenters<'a> {
    points: core::slice::Iter<'a, [f32; 2]>,
}

impl Iterator for MappingCenters<'_> {
    type Item = [f32; 2];

    fn next(&mut self) -> Option<[f32; 2]> {
        self.points.next().copied()
    }
}

/// Visit every lamp centre of `mapping` in visit order, unclamped, telling
/// the visitor whether it opens a new strand: `f(starts_strand, center)`.
///
/// A strand is one physical run of wire — one span of a document-resolved
/// mapping (a segment: a repeat contributes one per copy). Same visit order as [`mapping_centers`],
/// which is what lets scope geometry (the pattern-space lamp box and
/// consecutive-lamp pitch) be computed in one O(n), O(1)-memory pass over
/// exactly the coordinates the sampler will stream.
pub fn for_each_mapping_center_in_strands<'a>(
    mapping: impl Into<MappingRef<'a>>,
    mut f: impl FnMut(bool, [f32; 2]),
) {
    match mapping.into() {
        MappingRef::Slots(_) => {}
        MappingRef::Compact(compact) => {
            let mut visit_index = 0usize;
            for span in &compact.spans {
                let end = visit_index
                    .saturating_add(span.count as usize)
                    .min(compact.points.len());
                for (offset, center) in compact.points[visit_index..end].iter().enumerate() {
                    f(offset == 0, *center);
                }
                visit_index = end;
            }
        }
    }
}

/// Generate a mapping's points.
///
/// A thin exact-capacity wrapper over [`for_each_mapping_point`] — kept for
/// the consumers that genuinely need random access over the whole list
/// (the texture-area precompute and the display-layout probe). Streaming
/// consumers should call the visitor directly.
pub fn generate_mapping_points<'a>(
    mapping: impl Into<MappingRef<'a>>,
    texture_width: u32,
    texture_height: u32,
) -> Vec<MappingPoint> {
    let mapping = mapping.into();
    let mut all_points = Vec::with_capacity(mapping_point_count(mapping));
    for_each_mapping_point(mapping, texture_width, texture_height, |_, point| {
        all_points.push(point)
    });
    all_points
}

fn normalized_sample_radius(sample_diameter: f32, texture_width: u32, texture_height: u32) -> f32 {
    let max_dimension = texture_width.max(texture_height) as f32;
    (sample_diameter / 2.0) / max_dimension
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    use crate::nodes::fixture::{MappingConfig, ResolvedSpan};

    /// One span per path, `(first_channel, points)`, diameter 2.0.
    fn paths(paths: alloc::vec::Vec<(u32, alloc::vec::Vec<[f32; 2]>)>) -> ResolvedMappingCompact {
        let mut spans = alloc::vec::Vec::new();
        let mut points = alloc::vec::Vec::new();
        for (object, (first_channel, path)) in paths.into_iter().enumerate() {
            spans.push(ResolvedSpan {
                object: object as u32,
                first_channel,
                count: path.len() as u32,
            });
            points.extend(path);
        }
        ResolvedMappingCompact {
            spans,
            points,
            sample_diameter: 2.0,
        }
    }

    #[test]
    fn point_lists_generate_points_with_sequential_channels() {
        let mapping = paths(vec![
            (0, vec![[0.1, 0.2], [0.3, 0.4]]),
            (2, vec![[0.5, 0.6]]),
        ]);
        let points = generate_mapping_points(&mapping, 100, 100);
        assert_eq!(points.len(), 3);
        for (index, point) in points.iter().enumerate() {
            assert_eq!(point.channel, index as u32);
        }
        assert_eq!(points[0].center, [0.1, 0.2]);
        assert_eq!(points[2].center, [0.5, 0.6]);
    }

    #[test]
    fn sample_diameter_normalizes_against_max_dimension() {
        let mapping = paths(vec![(0, vec![[0.5, 0.5]])]);
        let points = generate_mapping_points(&mapping, 200, 100);
        // diameter 2.0 px on a 200-wide texture -> radius 1/200.
        assert!((points[0].radius - 0.005).abs() < 1e-6);
    }

    #[test]
    fn coordinates_clamp_to_unit_range() {
        let mapping = paths(vec![(0, vec![[-0.5, 1.5]])]);
        let points = generate_mapping_points(&mapping, 100, 100);
        assert_eq!(points[0].center, [0.0, 1.0]);
    }

    #[test]
    fn unset_and_map2d_yield_no_points() {
        assert!(generate_mapping_points(&MappingConfig::Unset, 10, 10).is_empty());
        assert!(generate_mapping_points(&MappingConfig::map2d("a.map2d.json"), 10, 10).is_empty());
    }

    fn visited<'a>(
        mapping: impl Into<MappingRef<'a>>,
        w: u32,
        h: u32,
    ) -> alloc::vec::Vec<(usize, MappingPoint)> {
        let mut out = alloc::vec::Vec::new();
        for_each_mapping_point(mapping, w, h, |index, point| out.push((index, point)));
        out
    }

    /// The streaming twin yields the same centres in the same order as the
    /// visitor — before the visitor's clamp, which is the one difference by
    /// design — for an unset mapping and for a compact carrier whose span
    /// total overshoots its point list.
    #[test]
    fn mapping_centers_visits_what_for_each_mapping_point_visits() {
        let compact = ResolvedMappingCompact {
            spans: vec![
                ResolvedSpan {
                    object: 0,
                    first_channel: 0,
                    count: 2,
                },
                ResolvedSpan {
                    object: 1,
                    first_channel: 2,
                    count: 5,
                },
            ],
            points: vec![[0.1, 0.9], [0.2, 0.8], [0.3, 0.7], [-0.5, 1.5]],
            sample_diameter: 2.0,
        };
        let cases: alloc::vec::Vec<(&str, MappingRef<'_>)> = vec![
            ("unset", MappingRef::Slots(&MappingConfig::Unset)),
            ("compact, spans overshoot", MappingRef::Compact(&compact)),
        ];
        for (name, mapping) in cases {
            let streamed: alloc::vec::Vec<[f32; 2]> = mapping_centers(mapping)
                .map(|[x, y]| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
                .collect();
            let visited: alloc::vec::Vec<[f32; 2]> = visited(mapping, 10, 10)
                .into_iter()
                .map(|(_, point)| point.center)
                .collect();
            assert_eq!(streamed, visited, "{name}");
            assert_eq!(
                streamed.len(),
                mapping_point_count(mapping),
                "{name}: count"
            );
            // The strand walker visits the same centres in the same order,
            // and opens a strand exactly where a span begins.
            let mut stranded = alloc::vec::Vec::new();
            let mut starts = 0usize;
            for_each_mapping_center_in_strands(mapping, |starts_strand, center| {
                starts += usize::from(starts_strand);
                stranded.push(center);
            });
            assert_eq!(
                stranded,
                mapping_centers(mapping).collect::<alloc::vec::Vec<_>>(),
                "{name}: strand walk"
            );
            let expected_starts = match name {
                "unset" => 0,
                // Two spans; the second is clamped to the two points left.
                _ => 2,
            };
            assert_eq!(starts, expected_starts, "{name}: strand starts");
        }
    }

    /// The ordering contract, pinned two ways:
    ///
    /// 1. against a hand-written golden `(channel, center)` sequence, which
    ///    is what actually catches a reordering — the wrapper delegates to
    ///    the visitor, so a visitor-vs-wrapper diff alone would sabotage
    ///    both sides identically and stay green;
    /// 2. against the wrapper, which pins the wrapper as a faithful
    ///    materialization of the visit order.
    ///
    /// Every streaming consumer's buffer layout rides on this order.
    #[test]
    fn visitor_matches_generate_mapping_points_across_configs() {
        // (name, mapping, golden (channel, center) sequence in visit order)
        let cases: alloc::vec::Vec<(&str, ResolvedMappingCompact, &[(u32, [f32; 2])])> = vec![
            ("empty path set", paths(vec![]), &[]),
            (
                "single point",
                paths(vec![(9, vec![[0.25, 0.75]])]),
                &[(9, [0.25, 0.75])],
            ),
            (
                "degenerate: empty path between populated ones",
                paths(vec![
                    (0, vec![[0.1, 0.2]]),
                    (5, vec![]),
                    (7, vec![[0.3, 0.4], [0.5, 0.6]]),
                ]),
                &[(0, [0.1, 0.2]), (7, [0.3, 0.4]), (8, [0.5, 0.6])],
            ),
            (
                "multi-path, sequential",
                paths(vec![
                    (0, vec![[0.1, 0.2], [0.3, 0.4]]),
                    (2, vec![[0.5, 0.6]]),
                    (3, vec![[0.7, 0.8], [0.9, 1.0], [-0.1, 0.5]]),
                ]),
                &[
                    (0, [0.1, 0.2]),
                    (1, [0.3, 0.4]),
                    (2, [0.5, 0.6]),
                    (3, [0.7, 0.8]),
                    (4, [0.9, 1.0]),
                    // x clamps into range; the point still keeps its slot.
                    (5, [0.0, 0.5]),
                ],
            ),
        ];

        for (name, mapping, golden) in cases {
            for (w, h) in [(1u32, 1u32), (100, 100), (200, 100)] {
                let wrapped = generate_mapping_points(&mapping, w, h);
                let actual = visited(&mapping, w, h);

                assert_eq!(
                    actual.len(),
                    golden.len(),
                    "{name} @ {w}x{h}: visited count"
                );
                assert_eq!(
                    wrapped.len(),
                    golden.len(),
                    "{name} @ {w}x{h}: wrapper count"
                );
                assert_eq!(
                    mapping_point_count(&mapping),
                    golden.len(),
                    "{name} @ {w}x{h}: mapping_point_count"
                );

                for (position, ((visit_index, got), (want_channel, want_center))) in
                    actual.iter().zip(golden.iter()).enumerate()
                {
                    assert_eq!(*visit_index, position, "{name} @ {w}x{h}: visit index");
                    assert_eq!(
                        got.channel, *want_channel,
                        "{name} @ {w}x{h}: channel at {position}"
                    );
                    assert_eq!(
                        got.center, *want_center,
                        "{name} @ {w}x{h}: center at {position}"
                    );
                }

                for (position, ((_, got), want)) in actual.iter().zip(wrapped.iter()).enumerate() {
                    assert_eq!(
                        got.channel, want.channel,
                        "{name} @ {w}x{h}: wrapper channel at {position}"
                    );
                    assert_eq!(
                        got.center, want.center,
                        "{name} @ {w}x{h}: wrapper center at {position}"
                    );
                    assert_eq!(
                        got.radius, want.radius,
                        "{name} @ {w}x{h}: wrapper radius at {position}"
                    );
                }
            }
        }
    }

    fn compact(
        spans: &[(u32, u32, u32)],
        points: &[[f32; 2]],
        sample_diameter: f32,
    ) -> ResolvedMappingCompact {
        ResolvedMappingCompact {
            spans: spans
                .iter()
                .map(|(object, first_channel, count)| ResolvedSpan {
                    object: *object,
                    first_channel: *first_channel,
                    count: *count,
                })
                .collect(),
            points: points.to_vec(),
            sample_diameter,
        }
    }

    /// The compact arm's own ordering contract, pinned against hand-written
    /// goldens — the same shape of pin the slot arm gets above.
    #[test]
    fn compact_visitor_matches_golden_sequences() {
        let cases: alloc::vec::Vec<(&str, ResolvedMappingCompact, &[(u32, [f32; 2])])> = vec![
            ("empty", compact(&[], &[], 2.0), &[]),
            (
                "single span",
                compact(&[(0, 9, 1)], &[[0.25, 0.75]], 2.0),
                &[(9, [0.25, 0.75])],
            ),
            (
                "zero-count span between populated ones",
                compact(
                    &[(0, 0, 1), (1, 1, 0), (2, 1, 2)],
                    &[[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]],
                    2.0,
                ),
                &[(0, [0.1, 0.2]), (1, [0.3, 0.4]), (2, [0.5, 0.6])],
            ),
            (
                "sparse spans: channel ranges with gaps between objects",
                compact(
                    &[(0, 100, 2), (1, 10, 3), (2, u32::MAX - 1, 1)],
                    &[
                        [0.1, 0.1],
                        [0.2, 0.2],
                        [0.3, 0.3],
                        [0.4, 0.4],
                        [0.5, 0.5],
                        [-0.5, 1.5],
                    ],
                    2.0,
                ),
                &[
                    (100, [0.1, 0.1]),
                    (101, [0.2, 0.2]),
                    (10, [0.3, 0.3]),
                    (11, [0.4, 0.4]),
                    (12, [0.5, 0.5]),
                    // Out-of-range coordinates clamp, exactly as the slot arm.
                    (u32::MAX - 1, [0.0, 1.0]),
                ],
            ),
        ];

        for (name, carrier, golden) in cases {
            for (w, h) in [(1u32, 1u32), (100, 100), (200, 100)] {
                let visited = visited(&carrier, w, h);
                let wrapped = generate_mapping_points(&carrier, w, h);
                assert_eq!(
                    visited.len(),
                    golden.len(),
                    "{name} @ {w}x{h}: visited count"
                );
                assert_eq!(
                    mapping_point_count(&carrier),
                    golden.len(),
                    "{name} @ {w}x{h}: mapping_point_count"
                );
                assert_eq!(
                    wrapped.len(),
                    golden.len(),
                    "{name} @ {w}x{h}: wrapper count"
                );
                for (position, ((visit_index, got), (want_channel, want_center))) in
                    visited.iter().zip(golden.iter()).enumerate()
                {
                    assert_eq!(*visit_index, position, "{name} @ {w}x{h}: visit index");
                    assert_eq!(
                        got.channel, *want_channel,
                        "{name} @ {w}x{h}: channel at {position}"
                    );
                    assert_eq!(
                        got.center, *want_center,
                        "{name} @ {w}x{h}: center at {position}"
                    );
                }
            }
        }
    }

    /// A carrier whose spans disagree with its point list truncates rather
    /// than panicking — and `mapping_point_count` reports what the visitor
    /// will actually visit, in both directions of the disagreement.
    #[test]
    fn broken_carrier_invariant_truncates_and_stays_countable() {
        // Spans claim more lamps than there are points.
        let over = compact(
            &[(0, 0, 2), (1, 2, 5)],
            &[[0.0, 0.0], [0.1, 0.1], [0.2, 0.2]],
            2.0,
        );
        assert_eq!(mapping_point_count(&over), 3);
        assert_eq!(visited(&over, 10, 10).len(), 3);
        assert_eq!(
            visited(&over, 10, 10)
                .into_iter()
                .map(|(_, point)| point.channel)
                .collect::<alloc::vec::Vec<_>>(),
            vec![0, 1, 2]
        );

        // Spans claim fewer lamps than there are points.
        let under = compact(&[(0, 0, 1)], &[[0.0, 0.0], [0.1, 0.1]], 2.0);
        assert_eq!(mapping_point_count(&under), 1);
        assert_eq!(visited(&under, 10, 10).len(), 1);
    }

    #[test]
    fn generate_mapping_points_allocates_exactly_once() {
        let mapping = paths(vec![
            (0, vec![[0.1, 0.2], [0.3, 0.4]]),
            (2, vec![[0.5, 0.6]]),
        ]);
        let points = generate_mapping_points(&mapping, 100, 100);
        assert_eq!(points.len(), 3);
        assert_eq!(points.capacity(), 3);
    }
}
