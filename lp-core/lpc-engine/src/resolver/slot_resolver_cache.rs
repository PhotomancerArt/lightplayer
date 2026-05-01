//! Per-node cache of resolved slot values (binding cascade / [`crate::resolver::resolve_slot`]).

use crate::resolver::resolved_slot::ResolvedSlot;
use alloc::collections::BTreeMap;
use lpc_model::PropPath;

/// Cache keyed by authored property path for legacy slot resolution.
///
/// This is **not** the engine same-frame [`super::ResolverCache`]; that cache is
/// keyed by [`super::QueryKey`].
#[derive(Clone, Debug, Default)]
pub struct SlotResolverCache {
    entries: BTreeMap<PropPath, ResolvedSlot>,
}

impl SlotResolverCache {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn get(&self, path: &PropPath) -> Option<&ResolvedSlot> {
        self.entries.get(path)
    }

    pub fn insert(&mut self, path: PropPath, slot: ResolvedSlot) -> Option<ResolvedSlot> {
        self.entries.insert(path, slot)
    }

    pub fn remove(&mut self, path: &PropPath) -> Option<ResolvedSlot> {
        self.entries.remove(path)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> alloc::collections::btree_map::Iter<'_, PropPath, ResolvedSlot> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::SlotResolverCache;
    use crate::resolver::resolve_source::ResolveSource;
    use crate::resolver::resolved_slot::ResolvedSlot;
    use alloc::vec::Vec;
    use lpc_model::FrameId;
    use lpc_model::PropPath;
    use lpc_model::prop::prop_path::Segment;
    use lps_shared::LpsValueF32;

    fn make_slot(value: f32, frame: i64) -> ResolvedSlot {
        ResolvedSlot::new(
            LpsValueF32::F32(value),
            FrameId::new(frame),
            ResolveSource::Default,
        )
    }

    fn make_path(s: &str) -> PropPath {
        lpc_model::prop::prop_path::parse_path(s).unwrap()
    }

    fn first_seg_is_field(path: &PropPath, expected: &str) -> bool {
        matches!(
            path.first(),
            Some(Segment::Field(s)) if s == expected
        )
    }

    #[test]
    fn slot_resolver_cache_insert_get_round_trip() {
        let mut cache = SlotResolverCache::new();
        let path = make_path("params.speed");
        let slot = make_slot(1.5, 10);

        assert!(cache.insert(path.clone(), slot).is_none());
        let got = cache.get(&path).unwrap();
        assert_eq!(got.changed_frame.as_i64(), 10);
    }

    #[test]
    fn slot_resolver_cache_insert_returns_old() {
        let mut cache = SlotResolverCache::new();
        let path = make_path("params.value");

        let slot1 = make_slot(1.0, 1);
        let slot2 = make_slot(2.0, 2);

        assert!(cache.insert(path.clone(), slot1).is_none());
        let old = cache.insert(path.clone(), slot2).unwrap();
        assert_eq!(old.changed_frame.as_i64(), 1);

        let got = cache.get(&path).unwrap();
        assert_eq!(got.changed_frame.as_i64(), 2);
    }

    #[test]
    fn slot_resolver_cache_remove() {
        let mut cache = SlotResolverCache::new();
        let path = make_path("outputs[0]");

        cache.insert(path.clone(), make_slot(3.0, 5));
        assert_eq!(cache.len(), 1);

        let removed = cache.remove(&path).unwrap();
        assert_eq!(removed.changed_frame.as_i64(), 5);
        assert!(cache.get(&path).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn slot_resolver_cache_clear() {
        let mut cache = SlotResolverCache::new();
        cache.insert(make_path("a"), make_slot(1.0, 1));
        cache.insert(make_path("b"), make_slot(2.0, 2));
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn slot_resolver_cache_iteration_order_is_sorted() {
        let mut cache = SlotResolverCache::new();
        cache.insert(make_path("z"), make_slot(1.0, 1));
        cache.insert(make_path("a"), make_slot(2.0, 2));
        cache.insert(make_path("m"), make_slot(3.0, 3));

        let keys: Vec<_> = cache.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(keys.len(), 3);
        assert!(first_seg_is_field(&keys[0], "a"));
    }

    #[test]
    fn slot_resolver_cache_default_is_empty() {
        let cache: SlotResolverCache = Default::default();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn slot_resolver_cache_clone_preserves_entries() {
        let mut cache = SlotResolverCache::new();
        cache.insert(make_path("x"), make_slot(5.0, 7));

        let cloned = cache.clone();
        assert_eq!(cloned.len(), 1);
        let got = cloned.get(&make_path("x")).unwrap();
        assert_eq!(got.changed_frame.as_i64(), 7);
    }
}
