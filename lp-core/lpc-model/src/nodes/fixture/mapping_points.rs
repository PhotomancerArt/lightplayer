//! Mapping point generation from configuration
//!
//! Generates LED sample points (texture-space centers + radii) from either
//! fixture mapping representation — the slot-modelled [`MappingConfig`] or
//! the document-resolved [`ResolvedMappingCompact`], both viewed through
//! [`MappingRef`]. Lives in the model crate so both the engine and
//! Studio-side tooling can share the generator.

use alloc::vec::Vec;

use crate::nodes::fixture::{MappingConfig, MappingRef, PathSpec, ResolvedMappingCompact};

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
        MappingRef::Slots(MappingConfig::Unset) => 0,
        MappingRef::Slots(MappingConfig::Map2d { .. }) => 0,
        MappingRef::Slots(MappingConfig::PathPoints { paths, .. }) => paths
            .entries
            .values()
            .map(|path_spec| match path_spec.value() {
                PathSpec::PointList { points, .. } => points.entries.len(),
            })
            .sum(),
        MappingRef::Compact(compact) => compact.lamp_count(),
    }
}

/// Visit every mapping point in channel-assignment order without
/// materializing the point list.
///
/// `f(visit_index, point)` receives the running 0-based position of the
/// point in the visit order (NOT its channel — channels come from the
/// point-list entry keys and are not required to be contiguous or
/// ordered). The visit order is: paths in `paths.entries.values()` order,
/// points within a path in point-list entry order. That order is the
/// contract every consumer's buffer layout depends on; see
/// `visitor_matches_generate_mapping_points_across_configs`.
///
/// `Map2d` is an authored source reference: the loader resolves it into a
/// [`MappingRef::Compact`] carrier before this runs, so it yields no sample
/// points here.
///
/// The `Compact` arm walks the span-concatenated point list with a running
/// cursor, taking each point's channel from its span's `first_channel` plus
/// its offset within the span — the same `first_channel + offset` rule the
/// slot arm applies to point-list entry keys, and the same visit order
/// (spans in channel-assignment order, points in span order). Radius is
/// computed identically from the sample diameter.
pub fn for_each_mapping_point<'a>(
    mapping: impl Into<MappingRef<'a>>,
    texture_width: u32,
    texture_height: u32,
    f: impl FnMut(usize, MappingPoint),
) {
    match mapping.into() {
        MappingRef::Slots(config) => {
            for_each_slot_mapping_point(config, texture_width, texture_height, f)
        }
        MappingRef::Compact(compact) => {
            for_each_compact_mapping_point(compact, texture_width, texture_height, f)
        }
    }
}

fn for_each_slot_mapping_point(
    config: &MappingConfig,
    texture_width: u32,
    texture_height: u32,
    mut f: impl FnMut(usize, MappingPoint),
) {
    let MappingConfig::PathPoints {
        paths,
        sample_diameter,
        ..
    } = config
    else {
        return;
    };

    let normalized_radius =
        normalized_sample_radius(sample_diameter.value().0, texture_width, texture_height);
    let mut visit_index = 0usize;

    for path_spec in paths.entries.values() {
        match path_spec.value() {
            PathSpec::PointList {
                first_channel,
                points,
                ..
            } => {
                let first_channel = *first_channel.value();
                for (index, point) in points.entries.iter() {
                    let [x, y] = point.value().0;
                    f(
                        visit_index,
                        MappingPoint {
                            channel: first_channel.saturating_add(*index),
                            center: [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)],
                            radius: normalized_radius,
                        },
                    );
                    visit_index += 1;
                }
            }
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
                    // both arms answer identically for any input.
                    center: [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)],
                    radius: normalized_radius,
                },
            );
        }
        visit_index = end;
    }
}

/// Generate mapping points from either mapping representation.
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
    use lp_collection::VecMap;

    use crate::nodes::fixture::ResolvedSpan;
    use crate::{EnumSlot, MapSlot, ValueSlot, Xy, XySlot};

    fn config(paths: alloc::vec::Vec<PathSpec>) -> MappingConfig {
        MappingConfig::path_points_vec(paths, 2.0)
    }

    #[test]
    fn point_lists_generate_points_with_sequential_channels() {
        let config = config(vec![
            PathSpec::point_list(0, [[0.1, 0.2], [0.3, 0.4]]),
            PathSpec::point_list(2, [[0.5, 0.6]]),
        ]);
        let points = generate_mapping_points(&config, 100, 100);
        assert_eq!(points.len(), 3);
        for (index, point) in points.iter().enumerate() {
            assert_eq!(point.channel, index as u32);
        }
        assert_eq!(points[0].center, [0.1, 0.2]);
        assert_eq!(points[2].center, [0.5, 0.6]);
    }

    #[test]
    fn sample_diameter_normalizes_against_max_dimension() {
        let config = config(vec![PathSpec::point_list(0, [[0.5, 0.5]])]);
        let points = generate_mapping_points(&config, 200, 100);
        // diameter 2.0 px on a 200-wide texture -> radius 1/200.
        assert!((points[0].radius - 0.005).abs() < 1e-6);
    }

    #[test]
    fn coordinates_clamp_to_unit_range() {
        let config = config(vec![PathSpec::point_list(0, [[-0.5, 1.5]])]);
        let points = generate_mapping_points(&config, 100, 100);
        assert_eq!(points[0].center, [0.0, 1.0]);
    }

    #[test]
    fn unset_and_map2d_yield_no_points() {
        assert!(generate_mapping_points(&MappingConfig::Unset, 10, 10).is_empty());
        assert!(generate_mapping_points(&MappingConfig::map2d("a.map2d.json"), 10, 10).is_empty());
    }

    /// A path whose point-list keys are explicit, so the test can build the
    /// sparse/out-of-order key sets `PathSpec::point_list` cannot express.
    /// Channels come from the ENTRY KEY (`first_channel + key`), never from
    /// a running offset — the gap cases below are what pins that.
    fn path_with_keys(first_channel: u32, points: &[(u32, [f32; 2])]) -> PathSpec {
        let mut entries = VecMap::new();
        for (key, xy) in points {
            entries.insert(*key, XySlot::new(Xy(*xy)));
        }
        PathSpec::PointList {
            first_channel: ValueSlot::new(first_channel),
            points: MapSlot::new(entries),
        }
    }

    /// Paths keyed explicitly and inserted out of key order, so the test
    /// exercises `paths.entries.values()` order rather than insertion order.
    fn config_with_path_keys(paths: &[(u32, PathSpec)]) -> MappingConfig {
        config_with_diameter(paths, 2.0)
    }

    fn config_with_diameter(paths: &[(u32, PathSpec)], sample_diameter: f32) -> MappingConfig {
        let mut entries = VecMap::new();
        for (key, spec) in paths {
            entries.insert(*key, EnumSlot::new(spec.clone()));
        }
        MappingConfig::path_points(MapSlot::new(entries), sample_diameter)
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

    /// The ordering contract, pinned two ways:
    ///
    /// 1. against a hand-written golden `(channel, center)` sequence, which
    ///    is what actually catches a reordering — the wrapper delegates to
    ///    the visitor now, so a visitor-vs-wrapper diff alone would sabotage
    ///    both sides identically and stay green;
    /// 2. against the wrapper, which pins the wrapper as a faithful
    ///    materialization of the visit order (the thing P3's `Compact` arm
    ///    must keep true).
    ///
    /// Every streaming consumer's buffer layout rides on this order.
    #[test]
    fn visitor_matches_generate_mapping_points_across_configs() {
        // (name, config, golden (channel, center) sequence in visit order)
        let cases: alloc::vec::Vec<(&str, MappingConfig, &[(u32, [f32; 2])])> = vec![
            ("unset", MappingConfig::Unset, &[]),
            ("map2d", MappingConfig::map2d("a.map2d.json"), &[]),
            ("empty path set", config(vec![]), &[]),
            (
                "single point",
                config(vec![PathSpec::point_list(9, [[0.25, 0.75]])]),
                &[(9, [0.25, 0.75])],
            ),
            (
                "degenerate: empty path between populated ones",
                config(vec![
                    PathSpec::point_list(0, [[0.1, 0.2]]),
                    PathSpec::point_list(5, []),
                    PathSpec::point_list(7, [[0.3, 0.4], [0.5, 0.6]]),
                ]),
                &[(0, [0.1, 0.2]), (7, [0.3, 0.4]), (8, [0.5, 0.6])],
            ),
            (
                "multi-path, sequential",
                config(vec![
                    PathSpec::point_list(0, [[0.1, 0.2], [0.3, 0.4]]),
                    PathSpec::point_list(2, [[0.5, 0.6]]),
                    PathSpec::point_list(3, [[0.7, 0.8], [0.9, 1.0], [-0.1, 0.5]]),
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
            (
                "multi-path, sparse point keys and overlapping channel ranges",
                config_with_path_keys(&[
                    (4, path_with_keys(100, &[(0, [0.1, 0.1]), (7, [0.2, 0.2])])),
                    (
                        1,
                        path_with_keys(10, &[(3, [0.3, 0.3]), (2, [0.4, 0.4]), (9, [0.5, 0.5])]),
                    ),
                    (9, path_with_keys(0, &[])),
                    (2, path_with_keys(u32::MAX - 1, &[(5, [0.6, 0.6])])),
                ]),
                // Paths in ascending path-key order (1, 2, 4, 9), points in
                // ascending point-key order, channel = first_channel + key
                // (saturating).
                &[
                    (12, [0.4, 0.4]),
                    (13, [0.3, 0.3]),
                    (19, [0.5, 0.5]),
                    (u32::MAX, [0.6, 0.6]),
                    (100, [0.1, 0.1]),
                    (107, [0.2, 0.2]),
                ],
            ),
        ];

        for (name, config, golden) in cases {
            for (w, h) in [(1u32, 1u32), (100, 100), (200, 100)] {
                let wrapped = generate_mapping_points(&config, w, h);
                let actual = visited(&config, w, h);

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
                    mapping_point_count(&config),
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

    #[test]
    fn channels_come_from_entry_keys_not_a_running_offset() {
        // Sparse keys: 2 points, keys 0 and 7, first_channel 100 -> 100, 107.
        let config = config_with_path_keys(&[
            (0, path_with_keys(100, &[(0, [0.0, 0.0]), (7, [0.0, 0.0])])),
            (1, path_with_keys(200, &[(3, [0.0, 0.0])])),
        ]);
        let channels: alloc::vec::Vec<u32> = visited(&config, 1, 1)
            .into_iter()
            .map(|(_, point)| point.channel)
            .collect();
        assert_eq!(channels, vec![100, 107, 203]);
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

    /// A compact carrier resolves to exactly what the equivalent slot form
    /// resolves to — same channels, centers, and radii — which is the
    /// property the engine's document differential test rides on.
    #[test]
    fn compact_matches_the_equivalent_slot_form() {
        let points = [[0.1, 0.2], [0.3, 0.4], [0.5, 0.6], [0.7, 0.8]];
        let carrier = compact(&[(0, 0, 3), (1, 3, 1)], &points, 3.0);
        let slots = config_with_diameter(
            &[
                (
                    0,
                    path_with_keys(0, &[(0, points[0]), (1, points[1]), (2, points[2])]),
                ),
                (1, path_with_keys(3, &[(0, points[3])])),
            ],
            3.0,
        );

        for (w, h) in [(1u32, 1u32), (100, 100), (200, 100), (64, 128)] {
            let from_slots = generate_mapping_points(&slots, w, h);
            let from_compact = generate_mapping_points(&carrier, w, h);
            assert_eq!(from_slots.len(), from_compact.len(), "count @ {w}x{h}");
            for (index, (a, b)) in from_slots.iter().zip(from_compact.iter()).enumerate() {
                assert_eq!(a.channel, b.channel, "channel {index} @ {w}x{h}");
                assert_eq!(a.center, b.center, "center {index} @ {w}x{h}");
                assert_eq!(a.radius, b.radius, "radius {index} @ {w}x{h}");
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
        let config = config(vec![
            PathSpec::point_list(0, [[0.1, 0.2], [0.3, 0.4]]),
            PathSpec::point_list(2, [[0.5, 0.6]]),
        ]);
        let points = generate_mapping_points(&config, 100, 100);
        assert_eq!(points.len(), 3);
        assert_eq!(points.capacity(), 3);
    }
}
