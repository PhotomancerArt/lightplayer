//! Engine-level same-frame resolver cache keyed by [`super::QueryKey`].

use crate::resolver::produced_value::ProducedValue;
use crate::resolver::query_key::QueryKey;
use alloc::collections::BTreeMap;

/// Per-frame cache of [`ProducedValue`] entries addressed by [`QueryKey`].
#[derive(Clone, Debug, Default)]
pub struct ResolverCache {
    entries: BTreeMap<QueryKey, ProducedValue>,
}

impl ResolverCache {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &QueryKey) -> Option<&ProducedValue> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: QueryKey, value: ProducedValue) -> Option<ProducedValue> {
        self.entries.insert(key, value)
    }

    pub fn remove(&mut self, key: &QueryKey) -> Option<ProducedValue> {
        self.entries.remove(key)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn iter(&self) -> alloc::collections::btree_map::Iter<'_, QueryKey, ProducedValue> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ProducedValue, QueryKey, ResolverCache};
    use crate::binding::BindingId;
    use crate::resolver::produced_value::ProductionSource;
    use crate::resolver::{ResolveLogLevel, ResolveTrace, ResolveTraceEvent};
    use lpc_model::ChannelName;
    use lpc_model::FrameId;
    use lpc_model::NodeId;
    use lpc_model::Versioned;
    use lpc_model::prop::prop_path::parse_path;
    use lps_shared::LpsValueF32;

    fn sample_bus_key(name: &str) -> QueryKey {
        QueryKey::Bus(ChannelName(alloc::string::String::from(name)))
    }

    fn make_produced(frame: i64, source: ProductionSource) -> ProducedValue {
        ProducedValue::new(
            Versioned::new(FrameId::new(frame), LpsValueF32::F32(1.0)),
            source,
        )
    }

    #[test]
    fn resolver_cache_insert_get_and_cache_hit_trace() {
        let mut cache = ResolverCache::new();
        let key = sample_bus_key("video");
        let pv = make_produced(
            1,
            ProductionSource::BusBinding {
                binding: BindingId::new(0),
            },
        );

        assert!(cache.insert(key.clone(), pv.clone()).is_none());
        let got = cache.get(&key).unwrap();
        assert!(got.value.get().eq(&LpsValueF32::F32(1.0)));
        assert_eq!(
            got.source,
            ProductionSource::BusBinding {
                binding: BindingId::new(0),
            }
        );

        let trace = ResolveTrace::new(ResolveLogLevel::Basic);
        {
            let _g = trace.enter(key.clone()).unwrap();
            let _hit = cache.get(&key);
            assert!(_hit.is_some());
            trace.record_event(ResolveTraceEvent::CacheHit(key.clone()));
        }

        assert!(trace.events().iter().any(|e| matches!(
            e,
            ResolveTraceEvent::CacheHit(k) if k == &sample_bus_key("video")
        )));
    }

    #[test]
    fn resolver_cache_remove_clear_len() {
        let mut cache = ResolverCache::new();
        let k = QueryKey::NodeOutput {
            node: NodeId::new(2),
            output: parse_path("color").unwrap(),
        };
        cache.insert(k.clone(), make_produced(3, ProductionSource::Literal));
        assert_eq!(cache.len(), 1);

        cache.remove(&k);
        assert!(cache.is_empty());

        cache.insert(k.clone(), make_produced(4, ProductionSource::Default));
        cache.clear();
        assert_eq!(cache.len(), 0);
    }
}
