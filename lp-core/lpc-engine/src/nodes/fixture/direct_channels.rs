//! The Direct sampling path's per-lamp channel list: the identity, held as
//! one count.
//!
//! Since PR #303 the channel list has been the fixture's ONLY resident
//! per-lamp state (coordinates regenerate transiently when the sample-point
//! buffer's key changes). A fixture's carrier is rendered from its parts
//! (`parts_carrier.rs`), whose spans are a running cursor over the tree's
//! segments — so the list is `0..n` by construction, and it is held as `n`.
//! The explicit list (4 B/lamp) went with the hand-placed `PathPoints`
//! lamps, the only mapping whose channels could be sparse or offset.

use lpc_model::nodes::fixture::{MappingRef, mapping_point_count};

/// The channel each lamp of the Direct path writes, in mapping visit order:
/// `channel == lamp index` for every lamp, so only the lamp count is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirectChannels(pub(crate) u32);

impl DirectChannels {
    /// The channel list of a mapping: its lamp count. `Unset` and an
    /// unresolved `Map2d` reference have no lamps.
    pub(crate) fn from_mapping(mapping: MappingRef<'_>) -> Self {
        debug_assert!(
            is_identity(mapping),
            "a fixture's carrier writes its lamps in order from channel 0"
        );
        Self(mapping_point_count(mapping) as u32)
    }

    /// Lamp count — the sample-point/sample-out size and the 1D strip width.
    pub(crate) fn len(&self) -> usize {
        self.0 as usize
    }

    /// The highest channel any lamp writes; `None` for no lamps.
    pub(crate) fn max_channel(&self) -> Option<u32> {
        self.0.checked_sub(1)
    }
}

/// Is `channel == visit index` for every lamp of `mapping`? The visitor walks
/// spans in order, `first_channel + offset` per point, and truncates against
/// the point list; every visited lamp is the identity iff every span starts
/// where the cursor stands.
fn is_identity(mapping: MappingRef<'_>) -> bool {
    match mapping {
        MappingRef::Compact(compact) => {
            let mut cursor = 0u32;
            compact.spans.iter().all(|span| {
                let starts_at_cursor = span.first_channel == cursor;
                cursor = cursor.saturating_add(span.count);
                starts_at_cursor
            })
        }
        MappingRef::Slots(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::nodes::fixture::{MappingConfig, ResolvedMappingCompact, ResolvedSpan};

    #[test]
    fn cursor_ordered_spans_are_the_identity() {
        let carrier = compact(&[(0, 0, 3), (1, 3, 2), (1, 5, 4)], 9);
        let channels = DirectChannels::from_mapping(MappingRef::Compact(&carrier));
        assert_eq!(channels, DirectChannels(9));
        assert_eq!(channels.max_channel(), Some(8));
    }

    /// A malformed carrier (spans past the point list) truncates in the
    /// visitor; the count must agree with what is visited.
    #[test]
    fn a_truncated_carrier_is_the_identity_over_the_visited_lamps() {
        let compact = compact(&[(0, 0, 3), (1, 3, 5)], 5);
        let channels = DirectChannels::from_mapping(MappingRef::Compact(&compact));
        assert_eq!(channels, DirectChannels(5));
    }

    #[test]
    fn unset_and_unresolved_mappings_have_no_lamps() {
        for config in [MappingConfig::Unset, MappingConfig::map2d("a.map2d.json")] {
            let channels = DirectChannels::from_mapping(MappingRef::Slots(&config));
            assert_eq!(channels, DirectChannels(0));
            assert_eq!(channels.len(), 0);
            assert_eq!(channels.max_channel(), None);
        }
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
