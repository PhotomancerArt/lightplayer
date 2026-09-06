//! The Direct sampling path's per-lamp channel list, encoded away when it
//! is the identity.
//!
//! Since PR #303 the channel list has been the fixture's ONLY resident
//! per-lamp state (coordinates regenerate transiently when the sample-point
//! buffer's key changes). For every document-resolved mapping the list is
//! `0..n` by construction — the resolver's spans are a running cursor and the
//! carrier copies `first_channel = span.start` — so the 4 B/lamp it cost was
//! 6,000 B on zook and ~25 KB on small-dome for a fact that fits in one
//! `u32`. Only a hand-authored `PathPoints` mapping with sparse or offset
//! point-list keys needs the explicit list, and it keeps it.

use alloc::vec::Vec;

use lpc_model::nodes::fixture::{MappingRef, for_each_mapping_point, mapping_point_count};

/// The channel each lamp of the Direct path writes, in mapping visit order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DirectChannels {
    /// `channel == lamp index` for every lamp. Every document-resolved
    /// mapping, and any dense `PathPoints` starting at channel 0.
    Identity(u32),
    /// Anything else — sparse or out-of-order point-list keys, a
    /// `first_channel` offset. 4 B/lamp, exactly what the `Vec` always was.
    Explicit(Vec<u32>),
}

impl DirectChannels {
    /// Derive the channel list from a mapping.
    ///
    /// Streamed, never a `Vec<MappingPoint>`: the compact carrier answers
    /// from its spans in O(spans); the slot form is walked once with no
    /// allocation, and only a mismatch walks it again into an exact-capacity
    /// list. `Unset` and an unresolved `Map2d` reference have no lamps.
    pub(crate) fn from_mapping(mapping: MappingRef<'_>) -> Self {
        let count = mapping_point_count(mapping);
        if is_identity(mapping) {
            return Self::Identity(count as u32);
        }
        let mut channels = Vec::with_capacity(count);
        for_each_mapping_point(mapping, 1, 1, |_, point| channels.push(point.channel));
        Self::Explicit(channels)
    }

    /// Lamp count — the sample-point/sample-out size and the 1D strip width.
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Identity(count) => *count as usize,
            Self::Explicit(channels) => channels.len(),
        }
    }

    /// The highest channel any lamp writes; `None` for no lamps.
    pub(crate) fn max_channel(&self) -> Option<u32> {
        match self {
            Self::Identity(count) => count.checked_sub(1),
            Self::Explicit(channels) => channels.iter().copied().max(),
        }
    }

    /// Visit every channel in lamp order. One `match` per call; each arm's
    /// loop is monomorphized on its own iterator, so nothing branches per
    /// lamp. The frame paths (`write_direct_lamps`,
    /// `accumulate_strip_lamps` in `fixture_node.rs`) need to zip with
    /// another stream, so they match on the enum themselves and hand either
    /// iterator to one generic body — the same shape as this.
    #[cfg(test)]
    pub(crate) fn for_each(&self, f: impl FnMut(u32)) {
        match self {
            Self::Identity(count) => (0..*count).for_each(f),
            Self::Explicit(channels) => channels.iter().copied().for_each(f),
        }
    }

    /// The materialized list — what the `Vec` used to hold. Test seam for
    /// the representation differentials; never on a frame path.
    #[cfg(test)]
    pub(crate) fn to_vec(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.len());
        self.for_each(|channel| out.push(channel));
        out
    }
}

/// Is `channel == visit index` for every lamp of `mapping`?
fn is_identity(mapping: MappingRef<'_>) -> bool {
    match mapping {
        MappingRef::Compact(compact) => {
            // The visitor walks spans in order, `first_channel + offset` per
            // point, and truncates against the point list; every visited
            // lamp is the identity iff every span starts where the cursor
            // stands. Spans past the point list contribute no lamps, so
            // they cannot break it.
            let mut cursor = 0u32;
            compact.spans.iter().all(|span| {
                let starts_at_cursor = span.first_channel == cursor;
                cursor = cursor.saturating_add(span.count);
                starts_at_cursor
            })
        }
        MappingRef::Slots(_) => {
            let mut identity = true;
            for_each_mapping_point(mapping, 1, 1, |visit_index, point| {
                identity &= point.channel as usize == visit_index;
            });
            identity
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lp_collection::VecMap;
    use lpc_model::nodes::fixture::{
        MappingConfig, PathSpec, ResolvedMappingCompact, ResolvedSpan,
    };
    use lpc_model::{EnumSlot, MapSlot, ValueSlot, Xy, XySlot};

    #[test]
    fn a_dense_point_list_from_channel_zero_is_the_identity() {
        let mapping = MappingConfig::path_points_vec(
            vec![
                PathSpec::point_list(0, [[0.0, 0.0], [0.5, 0.5]]),
                PathSpec::point_list(2, [[1.0, 1.0]]),
            ],
            1.0,
        );
        let channels = DirectChannels::from_mapping(MappingRef::Slots(&mapping));
        assert_eq!(channels, DirectChannels::Identity(3));
        assert_eq!(channels.to_vec(), vec![0, 1, 2]);
        assert_eq!(channels.max_channel(), Some(2));
    }

    #[test]
    fn a_first_channel_offset_is_explicit() {
        let mapping =
            MappingConfig::path_points_vec(vec![PathSpec::point_list(5, [[0.0, 0.0]; 3])], 1.0);
        let channels = DirectChannels::from_mapping(MappingRef::Slots(&mapping));
        assert_eq!(channels, DirectChannels::Explicit(vec![5, 6, 7]));
        assert_eq!(channels.max_channel(), Some(7));
    }

    /// Channels come from the ENTRY KEY (`first_channel + key`), never from
    /// a running offset — sparse keys are exactly the case the identity
    /// encoding must refuse.
    #[test]
    fn sparse_point_list_keys_are_explicit_channels() {
        let mapping = config_with_paths(&[
            (0, path_with_keys(100, &[(0, [0.0, 0.0]), (7, [0.0, 0.0])])),
            (1, path_with_keys(200, &[(3, [0.0, 0.0])])),
        ]);
        let channels = DirectChannels::from_mapping(MappingRef::Slots(&mapping));
        assert_eq!(channels, DirectChannels::Explicit(vec![100, 107, 203]));

        // Dense keys but starting at 0 with a gap later: still explicit.
        let mapping = config_with_paths(&[(
            0,
            path_with_keys(0, &[(0, [0.0, 0.0]), (1, [0.0, 0.0]), (3, [0.0, 0.0])]),
        )]);
        assert_eq!(
            DirectChannels::from_mapping(MappingRef::Slots(&mapping)),
            DirectChannels::Explicit(vec![0, 1, 3])
        );
    }

    #[test]
    fn cursor_ordered_spans_are_the_identity_and_a_moved_span_is_not() {
        let carrier = compact(&[(0, 0, 3), (1, 3, 2), (1, 5, 4)], 9);
        let channels = DirectChannels::from_mapping(MappingRef::Compact(&carrier));
        assert_eq!(channels, DirectChannels::Identity(9));
        assert_eq!(channels.to_vec(), (0..9).collect::<Vec<_>>());

        let moved = compact(&[(0, 0, 3), (1, 4, 2), (1, 6, 4)], 9);
        let channels = DirectChannels::from_mapping(MappingRef::Compact(&moved));
        assert_eq!(
            channels,
            DirectChannels::Explicit(vec![0, 1, 2, 4, 5, 6, 7, 8, 9])
        );
    }

    /// A malformed carrier (spans past the point list) truncates in the
    /// visitor; the identity answer must agree with what is visited.
    #[test]
    fn a_truncated_carrier_is_the_identity_over_the_visited_lamps() {
        let compact = compact(&[(0, 0, 3), (1, 3, 5)], 5);
        let channels = DirectChannels::from_mapping(MappingRef::Compact(&compact));
        assert_eq!(channels, DirectChannels::Identity(5));
    }

    #[test]
    fn unset_and_unresolved_mappings_have_no_lamps() {
        for config in [MappingConfig::Unset, MappingConfig::map2d("a.map2d.json")] {
            let channels = DirectChannels::from_mapping(MappingRef::Slots(&config));
            assert_eq!(channels, DirectChannels::Identity(0));
            assert_eq!(channels.len(), 0);
            assert_eq!(channels.max_channel(), None);
        }
    }

    #[test]
    fn for_each_visits_both_forms_in_lamp_order() {
        let mut seen = Vec::new();
        DirectChannels::Identity(3).for_each(|c| seen.push(c));
        DirectChannels::Explicit(vec![9, 4]).for_each(|c| seen.push(c));
        assert_eq!(seen, vec![0, 1, 2, 9, 4]);
    }

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

    fn config_with_paths(paths: &[(u32, PathSpec)]) -> MappingConfig {
        let mut entries = VecMap::new();
        for (key, spec) in paths {
            entries.insert(*key, EnumSlot::new(spec.clone()));
        }
        MappingConfig::path_points(MapSlot::new(entries), 1.0)
    }

    fn compact(spans: &[(u32, u32, u32)], points: usize) -> ResolvedMappingCompact {
        ResolvedMappingCompact {
            spans: spans
                .iter()
                .map(|(object, first_channel, count)| ResolvedSpan {
                    object: *object,
                    first_channel: *first_channel,
                    count: *count,
                })
                .collect(),
            points: vec![[0.0, 0.0]; points],
            sample_diameter: 1.0,
        }
    }
}
