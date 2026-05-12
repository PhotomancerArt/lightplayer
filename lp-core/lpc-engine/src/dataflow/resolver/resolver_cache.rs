//! Engine-level same-frame resolver cache keyed by [`super::QueryKey`].

use crate::dataflow::resolver::production::Production;
use crate::dataflow::resolver::query_key::QueryKey;
use alloc::vec::Vec;

/// Per-frame cache of [`Production`] entries addressed by [`QueryKey`].
///
/// Resolver caches are small in normal scenes, and they are rebuilt every frame.
/// A linear vec avoids per-entry tree allocation and pointer chasing on embedded
/// targets.
#[derive(Clone, Debug, Default)]
pub struct ResolverCache {
    entries: Vec<(QueryKey, Production)>,
}

impl ResolverCache {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn get(&self, key: &QueryKey) -> Option<&Production> {
        self.entries
            .iter()
            .find_map(|(entry_key, value)| (entry_key == key).then_some(value))
    }

    pub fn insert(&mut self, key: QueryKey, value: Production) -> Option<Production> {
        if let Some((_, current)) = self
            .entries
            .iter_mut()
            .find(|(entry_key, _)| entry_key == &key)
        {
            return Some(core::mem::replace(current, value));
        }
        self.entries.push((key, value));
        None
    }

    pub fn remove(&mut self, key: &QueryKey) -> Option<Production> {
        let index = self
            .entries
            .iter()
            .position(|(entry_key, _)| entry_key == key)?;
        Some(self.entries.swap_remove(index).1)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn iter(&self) -> core::slice::Iter<'_, (QueryKey, Production)> {
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
    use super::{Production, QueryKey, ResolverCache};
    use crate::dataflow::binding::BindingRef;
    use crate::dataflow::resolver::production::ProductionSource;
    use crate::dataflow::resolver::{ResolveLogLevel, ResolveTrace, ResolveTraceEvent};
    use lpc_model::ChannelName;
    use lpc_model::NodeId;
    use lpc_model::Revision;
    use lpc_model::SlotPath;
    use lpc_model::WithRevision;
    use lps_shared::LpsValueF32;

    fn sample_bus_key(name: &str) -> QueryKey {
        QueryKey::Bus(ChannelName(alloc::string::String::from(name)))
    }

    fn make_produced(frame: i64, source: ProductionSource) -> Production {
        Production::value(
            WithRevision::new(Revision::new(frame), LpsValueF32::F32(1.0)),
            source,
        )
        .expect("scalar production")
    }

    #[test]
    fn resolver_cache_insert_get_and_cache_hit_trace() {
        let mut cache = ResolverCache::new();
        let key = sample_bus_key("video");
        let pv = make_produced(
            1,
            ProductionSource::BusBinding {
                binding: BindingRef::new(NodeId::new(0), 0),
            },
        );

        assert!(cache.insert(key.clone(), pv.clone()).is_none());
        let got = cache.get(&key).unwrap();
        assert!(got.as_value().expect("value").eq(&LpsValueF32::F32(1.0)));
        assert_eq!(
            got.source,
            ProductionSource::BusBinding {
                binding: BindingRef::new(NodeId::new(0), 0),
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
        let k = QueryKey::ProducedSlot {
            node: NodeId::new(2),
            slot: SlotPath::parse("color").unwrap(),
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
