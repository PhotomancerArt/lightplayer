//! [`EngineSession`] — per-frame demand resolution and engine-dispatched work.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;

use crate::dataflow::binding::{BindingEntry, BindingRef, BindingSource};
use crate::dataflow::resolver::production::{Production, ProductionSource};
use crate::dataflow::resolver::query_key::QueryKey;
use crate::dataflow::resolver::resolve_error::SessionResolveError;
use crate::dataflow::resolver::resolve_host::ResolveHost;
use crate::dataflow::resolver::resolve_trace::ResolveTrace;
use crate::dataflow::resolver::resolver::Resolver;
use crate::dataflow::resolver::resolver::materialize_literal_product;
use lpc_model::{ChannelName, NodeId, Revision, SlotData, SlotMapDyn, SlotMerge, SlotPath};

/// Active engine session for one frame (or nested test scope).
///
/// The session owns the demand-resolution cache and trace stack. Engine-owned
/// callbacks such as runtime state slot reads and product materialization still
/// pass through a host adapter so the resolver can be tested without
/// constructing a full [`crate::engine::Engine`].
pub struct EngineSession<'a> {
    revision: Revision,
    resolver: &'a mut Resolver,
    trace: ResolveTrace,
}

/// Transitional alias while resolver tests and call sites still use the older
/// name. New engine-facing code should prefer [`EngineSession`].
pub type ResolveSession<'a> = EngineSession<'a>;

impl<'a> EngineSession<'a> {
    pub fn new(frame_id: Revision, resolver: &'a mut Resolver, trace: ResolveTrace) -> Self {
        Self {
            revision: frame_id,
            resolver,
            trace,
        }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn trace(&self) -> &ResolveTrace {
        &self.trace
    }

    pub fn publish(&mut self, query: QueryKey, production: Production) {
        self.resolver.cache_mut().insert(query, production);
    }

    pub fn publish_produced_slot(&mut self, node: NodeId, slot: SlotPath, production: Production) {
        self.publish(QueryKey::ProducedSlot { node, slot }, production);
    }

    /// Demand-resolve `query` for this frame (cache + cycle stack + host-owned bindings).
    pub fn resolve<H: ResolveHost + ?Sized>(
        &mut self,
        host: &mut H,
        query: QueryKey,
    ) -> Result<Production, SessionResolveError> {
        if let Some(pv) = self.resolver.cache().get(&query) {
            self.trace.record_cache_hit(&query);
            return Ok(pv.clone());
        }

        self.trace
            .try_push_active(&query)
            .map_err(SessionResolveError::from)?;
        match self.resolve_uncached(host, &query) {
            Ok(result) => {
                self.trace.exit(&query);
                self.resolver.cache_mut().insert(query, result.clone());
                Ok(result)
            }
            Err(err) => {
                self.trace.exit(&query);
                Err(err)
            }
        }
    }

    fn resolve_uncached<H: ResolveHost + ?Sized>(
        &mut self,
        host: &mut H,
        query: &QueryKey,
    ) -> Result<Production, SessionResolveError> {
        match query {
            QueryKey::Bus(channel) => self.resolve_bus(host, channel, query),
            QueryKey::ConsumedSlot { node, slot } => {
                self.resolve_consumed_slot(host, *node, slot, query)
            }
            QueryKey::ConsumedSlotAccessor { node, accessor } => {
                self.resolve_consumed_slot(host, *node, accessor.path(), query)
            }
            QueryKey::ProducedSlot { .. } => {
                self.trace.record_produce_start(query);
                let r = host.produce(query, self);
                match &r {
                    Ok(_) => self.trace.record_produce_end(query),
                    Err(_) => self.trace.record_resolve_error(query),
                }
                r
            }
        }
    }

    fn resolve_bus(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        channel: &ChannelName,
        query: &QueryKey,
    ) -> Result<Production, SessionResolveError> {
        let candidates = host.providers_for_bus(channel);
        let entry = select_highest_priority_bus_provider(channel, &candidates)?;
        self.trace.record_select_binding(query, entry.0);
        self.resolve_binding_source(host, entry.0, &entry.1.source)
    }

    fn resolve_consumed_slot(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        node: NodeId,
        slot: &SlotPath,
        query: &QueryKey,
    ) -> Result<Production, SessionResolveError> {
        let policy = host.merge_policy_for_consumed_slot(node, slot);
        self.trace.record_select_merge_policy(query, policy);
        match policy {
            SlotMerge::Latest => self.resolve_latest_consumed_slot(host, node, slot, query),
            SlotMerge::Error => self.resolve_error_merge_consumed_slot(host, node, slot, query),
            SlotMerge::ByKey => self.resolve_by_key_consumed_slot(host, node, slot, query),
        }
    }

    fn resolve_latest_consumed_slot(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        node: NodeId,
        slot: &SlotPath,
        query: &QueryKey,
    ) -> Result<Production, SessionResolveError> {
        if let Some(entry) = host.binding_for_consumed_slot(node, slot) {
            self.trace.record_select_binding(query, entry.0);
            return self.resolve_binding_source(host, entry.0, &entry.1.source);
        }
        self.trace.record_produce_start(query);
        let r = host.produce(query, self);
        match &r {
            Ok(_) => self.trace.record_produce_end(query),
            Err(_) => self.trace.record_resolve_error(query),
        }
        r
    }

    fn resolve_error_merge_consumed_slot(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        node: NodeId,
        slot: &SlotPath,
        query: &QueryKey,
    ) -> Result<Production, SessionResolveError> {
        let entries = host.bindings_for_consumed_slot(node, slot);
        if entries.len() > 1 {
            return Err(SessionResolveError::other(format!(
                "multiple bindings for non-mergeable consumed slot node={node:?} slot={slot}"
            )));
        }
        self.resolve_latest_consumed_slot(host, node, slot, query)
    }

    fn resolve_by_key_consumed_slot(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        node: NodeId,
        slot: &SlotPath,
        query: &QueryKey,
    ) -> Result<Production, SessionResolveError> {
        let entries = host.bindings_for_consumed_slot(node, slot);
        if entries.is_empty() {
            return self.resolve_latest_consumed_slot(host, node, slot, query);
        }

        let mut inputs = Vec::new();
        for (binding_ref, entry) in entries.iter() {
            let binding_ref = *binding_ref;
            self.trace.record_merge_input(query, binding_ref);
            self.resolve_binding_source_for_merge(
                host,
                query,
                binding_ref,
                &entry.source,
                &mut inputs,
            )?;
        }

        merge_maps_by_key(inputs, query, &self.trace)
    }

    fn resolve_binding_source(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        binding_ref: BindingRef,
        source: &BindingSource,
    ) -> Result<Production, SessionResolveError> {
        match source {
            BindingSource::Literal(spec) => {
                let product = materialize_literal_product(spec, self.revision);
                Ok(Production::leaf(product, ProductionSource::Literal))
            }
            BindingSource::ProducedSlot { node, slot } => {
                let key = QueryKey::ProducedSlot {
                    node: *node,
                    slot: slot.clone(),
                };
                let mut pv = self.resolve(host, key)?;
                pv.source = ProductionSource::BusBinding {
                    binding: binding_ref,
                };
                Ok(pv)
            }
            BindingSource::BusChannel(other) => {
                let key = QueryKey::Bus(other.clone());
                let mut pv = self.resolve(host, key)?;
                pv.source = ProductionSource::BusBinding {
                    binding: binding_ref,
                };
                Ok(pv)
            }
        }
    }

    fn resolve_binding_source_for_merge(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        query: &QueryKey,
        binding_ref: BindingRef,
        source: &BindingSource,
        out: &mut Vec<Production>,
    ) -> Result<(), SessionResolveError> {
        match source {
            BindingSource::BusChannel(channel) => {
                let bus_query = QueryKey::Bus(channel.clone());
                self.trace
                    .try_push_active(&bus_query)
                    .map_err(SessionResolveError::from)?;
                let mut providers = host.providers_for_bus(channel);
                providers.sort_by_key(|(provider_ref, entry)| (entry.priority, *provider_ref));
                for (provider_ref, provider) in providers.iter() {
                    let provider_ref = *provider_ref;
                    self.trace.record_merge_input(query, provider_ref);
                    match self.resolve_binding_source_for_merge(
                        host,
                        query,
                        provider_ref,
                        &provider.source,
                        out,
                    ) {
                        Ok(()) => {}
                        Err(err) => {
                            self.trace.exit(&bus_query);
                            return Err(err);
                        }
                    }
                }
                self.trace.exit(&bus_query);
                Ok(())
            }
            _ => {
                let mut production = self.resolve_binding_source(host, binding_ref, source)?;
                production.source = ProductionSource::BusBinding {
                    binding: binding_ref,
                };
                out.push(production);
                Ok(())
            }
        }
    }
}

fn merge_maps_by_key(
    inputs: Vec<Production>,
    query: &QueryKey,
    trace: &ResolveTrace,
) -> Result<Production, SessionResolveError> {
    let mut keys_revision = Revision::default();
    let mut entries = BTreeMap::new();
    for input in inputs {
        let SlotData::Map(map) = input.data().clone() else {
            return Err(SessionResolveError::other(format!(
                "merge by key expected map input for {query:?}"
            )));
        };
        keys_revision = core::cmp::max(keys_revision, map.keys_revision);
        for (key, data) in map.entries {
            if entries.insert(key.clone(), data).is_some() {
                trace.record_merge_replace_key(query, key);
            }
        }
    }
    Ok(Production::new(
        SlotData::Map(SlotMapDyn::with_revision(keys_revision, entries)),
        ProductionSource::Merged,
    ))
}

fn select_highest_priority_bus_provider(
    channel: &ChannelName,
    candidates: &[(BindingRef, BindingEntry)],
) -> Result<(BindingRef, BindingEntry), SessionResolveError> {
    if candidates.is_empty() {
        return Err(SessionResolveError::NoBusProvider {
            channel: channel.clone(),
        });
    }
    let Some(max_p) = candidates.iter().map(|(_, e)| e.priority).max() else {
        return Err(SessionResolveError::NoBusProvider {
            channel: channel.clone(),
        });
    };
    let at_max: Vec<_> = candidates
        .iter()
        .filter(|(_, e)| e.priority == max_p)
        .collect();
    if at_max.len() != 1 {
        return Err(SessionResolveError::AmbiguousBusBinding {
            channel: channel.clone(),
        });
    }
    Ok(at_max[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataflow::binding::BindingDraft;
    use crate::dataflow::binding::BindingPriority;
    use crate::dataflow::binding::BindingTarget;
    use crate::dataflow::resolver::resolve_trace::ResolveLogLevel;
    use crate::dataflow::resolver::resolve_trace::ResolveTraceEvent;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use lpc_model::Kind;
    use lpc_model::{ChannelName, LpValue, SlotMapKey, WithRevision};
    use lps_shared::LpsValueF32;

    fn ch(s: &str) -> ChannelName {
        ChannelName(String::from(s))
    }

    fn path(s: &str) -> SlotPath {
        SlotPath::parse(s).expect("path")
    }

    struct CountingHost {
        produce_calls: u32,
        node: NodeId,
        out_path: SlotPath,
        bindings: TestBindings,
    }

    impl CountingHost {
        fn new(node: NodeId, out_path: SlotPath) -> Self {
            Self {
                produce_calls: 0,
                node,
                out_path,
                bindings: TestBindings::default(),
            }
        }

        fn with_bindings(mut self, bindings: TestBindings) -> Self {
            self.bindings = bindings;
            self
        }
    }

    #[derive(Default)]
    struct TestBindings {
        entries: Vec<(BindingRef, BindingEntry)>,
    }

    impl TestBindings {
        fn add(&mut self, draft: BindingDraft, revision: Revision) {
            let binding_ref = BindingRef::new(draft.owner, self.entries.len());
            self.entries.push((
                binding_ref,
                BindingEntry {
                    source: draft.source,
                    target: draft.target,
                    priority: draft.priority,
                    kind: draft.kind,
                    version: revision,
                    owner: draft.owner,
                },
            ));
        }

        fn binding_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Option<(BindingRef, BindingEntry)> {
            self.entries.iter().find_map(|(binding_ref, entry)| {
                matches!(
                    &entry.target,
                    BindingTarget::ConsumedSlot { node: n, slot: p } if *n == node && p == slot
                )
                .then(|| (*binding_ref, entry.clone()))
            })
        }

        fn bindings_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Vec<(BindingRef, BindingEntry)> {
            self.entries
                .iter()
                .filter_map(|(binding_ref, entry)| {
                    matches!(
                        &entry.target,
                        BindingTarget::ConsumedSlot { node: n, slot: p } if *n == node && p == slot
                    )
                    .then(|| (*binding_ref, entry.clone()))
                })
                .collect()
        }

        fn providers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, BindingEntry)> {
            self.entries
                .iter()
                .filter_map(|(binding_ref, entry)| {
                    matches!(&entry.target, BindingTarget::BusChannel(c) if c == channel)
                        .then(|| (*binding_ref, entry.clone()))
                })
                .collect()
        }
    }

    impl ResolveHost for CountingHost {
        fn produce(
            &mut self,
            query: &QueryKey,
            session: &mut ResolveSession<'_>,
        ) -> Result<Production, SessionResolveError> {
            self.produce_calls += 1;
            match query {
                QueryKey::ProducedSlot { node, slot }
                    if *node == self.node && *slot == self.out_path =>
                {
                    Ok(Production::value(
                        WithRevision::new(session.revision(), LpsValueF32::F32(42.0)),
                        ProductionSource::ProducedSlot {
                            node: *node,
                            slot: slot.clone(),
                        },
                    )?)
                }
                _ => Err(SessionResolveError::other("unexpected produce query")),
            }
        }

        fn binding_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Option<(BindingRef, BindingEntry)> {
            self.bindings.binding_for_consumed_slot(node, slot)
        }

        fn bindings_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.bindings_for_consumed_slot(node, slot)
        }

        fn providers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.providers_for_bus(channel)
        }
    }

    #[test]
    fn same_produced_slot_twice_calls_host_once() {
        let mut resolver = Resolver::new();
        let frame = Revision::new(1);
        let node = NodeId::new(7);
        let out = path("color");
        let key = QueryKey::ProducedSlot {
            node,
            slot: out.clone(),
        };
        let mut host = CountingHost::new(node, out.clone());
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let a = session.resolve(&mut host, key.clone()).unwrap();
        let b = session.resolve(&mut host, key).unwrap();
        assert!(a.as_value().expect("value").eq(&LpsValueF32::F32(42.0)));
        assert!(b.as_value().expect("value").eq(&LpsValueF32::F32(42.0)));
        assert!(
            a.as_value()
                .expect("value")
                .eq(&b.as_value().expect("value"))
        );
        assert_eq!(
            a.value_leaf().expect("value").changed_at(),
            b.value_leaf().expect("value").changed_at()
        );
        assert_eq!(host.produce_calls, 1);
    }

    #[test]
    fn bus_channel_selects_highest_priority_binding() {
        let mut resolver = Resolver::new();
        let mut bindings = TestBindings::default();
        let frame = Revision::new(2);
        let c = ch("video");
        let low_node = NodeId::new(1);
        let high_node = NodeId::new(2);
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(LpValue::F32(1.0)),
                target: BindingTarget::BusChannel(c.clone()),
                priority: BindingPriority::new(1),
                kind: Kind::Amplitude,
                owner: low_node,
            },
            frame,
        );
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(LpValue::F32(9.0)),
                target: BindingTarget::BusChannel(c.clone()),
                priority: BindingPriority::new(10),
                kind: Kind::Amplitude,
                owner: high_node,
            },
            frame,
        );

        let mut host = CountingHost::new(low_node, path("x")).with_bindings(bindings);
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let pv = session
            .resolve(&mut host, QueryKey::Bus(c))
            .expect("resolve bus");
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(9.0)));
        assert_eq!(host.produce_calls, 0);
    }

    #[test]
    fn equal_priority_bus_providers_return_ambiguous_error() {
        let e1 = BindingEntry {
            source: BindingSource::Literal(LpValue::F32(1.0)),
            target: BindingTarget::BusChannel(ch("z")),
            priority: BindingPriority::new(5),
            kind: Kind::Amplitude,
            version: Revision::new(0),
            owner: NodeId::new(0),
        };
        let e2 = BindingEntry {
            source: BindingSource::Literal(LpValue::F32(2.0)),
            target: BindingTarget::BusChannel(ch("z")),
            priority: BindingPriority::new(5),
            kind: Kind::Amplitude,
            version: Revision::new(0),
            owner: NodeId::new(1),
        };
        let c = ch("z");
        let err = select_highest_priority_bus_provider(
            &c,
            &[
                (BindingRef::new(NodeId::new(0), 0), e1),
                (BindingRef::new(NodeId::new(1), 0), e2),
            ],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SessionResolveError::AmbiguousBusBinding { .. }
        ));
    }

    #[test]
    fn bus_to_bus_recursion_resolves_through_both_labels() {
        let mut resolver = Resolver::new();
        let mut bindings = TestBindings::default();
        let frame = Revision::new(3);
        let outer = ch("a");
        let inner = ch("b");
        bindings.add(
            BindingDraft {
                source: BindingSource::BusChannel(inner.clone()),
                target: BindingTarget::BusChannel(outer.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: NodeId::new(0),
            },
            frame,
        );
        bindings.add(
            BindingDraft {
                source: BindingSource::Literal(LpValue::F32(3.25)),
                target: BindingTarget::BusChannel(inner.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: NodeId::new(1),
            },
            frame,
        );

        let mut host = CountingHost::new(NodeId::new(99), path("noop")).with_bindings(bindings);
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let pv = session
            .resolve(&mut host, QueryKey::Bus(outer))
            .expect("bus chain");
        assert!(pv.as_value().expect("value").eq(&LpsValueF32::F32(3.25)));
    }

    struct NoProduceHost {
        bindings: TestBindings,
    }

    impl ResolveHost for NoProduceHost {
        fn produce(
            &mut self,
            _query: &QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<Production, SessionResolveError> {
            Err(SessionResolveError::other(
                "produce should not run in bus-only cycle test",
            ))
        }

        fn providers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.providers_for_bus(channel)
        }
    }

    #[test]
    fn bus_recursion_cycle_is_detected() {
        let mut resolver = Resolver::new();
        let mut bindings = TestBindings::default();
        let frame = Revision::new(4);
        let a = ch("loop_a");
        let b = ch("loop_b");
        bindings.add(
            BindingDraft {
                source: BindingSource::BusChannel(b.clone()),
                target: BindingTarget::BusChannel(a.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: NodeId::new(0),
            },
            frame,
        );
        bindings.add(
            BindingDraft {
                source: BindingSource::BusChannel(a.clone()),
                target: BindingTarget::BusChannel(b.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: NodeId::new(1),
            },
            frame,
        );

        let mut host = NoProduceHost { bindings };
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let err = session
            .resolve(&mut host, QueryKey::Bus(a))
            .expect_err("cycle");
        assert!(matches!(err, SessionResolveError::Cycle { .. }));
    }

    struct MapMergeHost {
        bindings: TestBindings,
        receiver: NodeId,
        receiver_slot: SlotPath,
    }

    impl MapMergeHost {
        fn new(bindings: TestBindings, receiver: NodeId, receiver_slot: SlotPath) -> Self {
            Self {
                bindings,
                receiver,
                receiver_slot,
            }
        }
    }

    impl ResolveHost for MapMergeHost {
        fn produce(
            &mut self,
            query: &QueryKey,
            session: &mut ResolveSession<'_>,
        ) -> Result<Production, SessionResolveError> {
            let QueryKey::ProducedSlot { node, .. } = query else {
                return Err(SessionResolveError::other("unexpected map merge query"));
            };
            let entries = match node.0 {
                1 => [(1, 10), (2, 20)].into_iter().collect(),
                2 => [(2, 200), (3, 300)].into_iter().collect(),
                _ => return Err(SessionResolveError::other("unknown map producer")),
            };
            Ok(Production::new(
                map_data(session.revision(), entries),
                ProductionSource::ProducedSlot {
                    node: *node,
                    slot: path("emitters"),
                },
            ))
        }

        fn bindings_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.bindings_for_consumed_slot(node, slot)
        }

        fn binding_for_consumed_slot(
            &self,
            node: NodeId,
            slot: &SlotPath,
        ) -> Option<(BindingRef, BindingEntry)> {
            self.bindings.binding_for_consumed_slot(node, slot)
        }

        fn providers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.providers_for_bus(channel)
        }

        fn merge_policy_for_consumed_slot(&self, node: NodeId, slot: &SlotPath) -> SlotMerge {
            if node == self.receiver && slot == &self.receiver_slot {
                SlotMerge::ByKey
            } else {
                SlotMerge::Latest
            }
        }
    }

    fn map_data(revision: Revision, pairs: BTreeMap<u32, u32>) -> SlotData {
        SlotData::Map(SlotMapDyn::with_revision(
            revision,
            pairs
                .into_iter()
                .map(|(key, value)| {
                    (
                        SlotMapKey::U32(key),
                        SlotData::Value(WithRevision::new(revision, LpValue::U32(value))),
                    )
                })
                .collect(),
        ))
    }

    fn map_u32(map: &SlotData, key: u32) -> u32 {
        let SlotData::Map(map) = map else {
            panic!("map");
        };
        let Some(SlotData::Value(value)) = map.entries.get(&SlotMapKey::U32(key)) else {
            panic!("key {key}");
        };
        let LpValue::U32(value) = value.value() else {
            panic!("u32");
        };
        *value
    }

    #[test]
    fn consumed_slot_merge_by_key_combines_direct_map_bindings() {
        let mut resolver = Resolver::new();
        let mut bindings = TestBindings::default();
        let frame = Revision::new(6);
        let receiver = NodeId::new(99);
        let receiver_slot = path("emitters");
        for producer in [NodeId::new(1), NodeId::new(2)] {
            bindings.add(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: producer,
                        slot: path("emitters"),
                    },
                    target: BindingTarget::ConsumedSlot {
                        node: receiver,
                        slot: receiver_slot.clone(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: receiver,
                },
                frame,
            );
        }

        let mut host = MapMergeHost::new(bindings, receiver, receiver_slot.clone());
        let trace = ResolveTrace::new(ResolveLogLevel::Basic);
        let mut session = ResolveSession::new(frame, &mut resolver, trace);
        let production = session
            .resolve(
                &mut host,
                QueryKey::ConsumedSlot {
                    node: receiver,
                    slot: receiver_slot,
                },
            )
            .expect("merged map");

        assert_eq!(map_u32(production.data(), 1), 10);
        assert_eq!(map_u32(production.data(), 2), 200);
        assert_eq!(map_u32(production.data(), 3), 300);
        assert_eq!(production.source, ProductionSource::Merged);
        assert!(session.trace().events().iter().any(|event| matches!(
            event,
            ResolveTraceEvent::SelectMergePolicy {
                policy: SlotMerge::ByKey,
                ..
            }
        )));
        assert!(session.trace().events().iter().any(|event| matches!(
            event,
            ResolveTraceEvent::MergeReplaceKey {
                key: SlotMapKey::U32(2),
                ..
            }
        )));
    }

    #[test]
    fn consumed_slot_merge_by_key_expands_bus_providers() {
        let mut resolver = Resolver::new();
        let mut bindings = TestBindings::default();
        let frame = Revision::new(7);
        let receiver = NodeId::new(99);
        let receiver_slot = path("emitters");
        let bus = ch("fluid.emitters");
        bindings.add(
            BindingDraft {
                source: BindingSource::BusChannel(bus.clone()),
                target: BindingTarget::ConsumedSlot {
                    node: receiver,
                    slot: receiver_slot.clone(),
                },
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: receiver,
            },
            frame,
        );
        for (producer, priority) in [(NodeId::new(1), 1), (NodeId::new(2), 10)] {
            bindings.add(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: producer,
                        slot: path("emitters"),
                    },
                    target: BindingTarget::BusChannel(bus.clone()),
                    priority: BindingPriority::new(priority),
                    kind: Kind::Amplitude,
                    owner: producer,
                },
                frame,
            );
        }

        let mut host = MapMergeHost::new(bindings, receiver, receiver_slot.clone());
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let production = session
            .resolve(
                &mut host,
                QueryKey::ConsumedSlot {
                    node: receiver,
                    slot: receiver_slot,
                },
            )
            .expect("merged bus map");

        assert_eq!(map_u32(production.data(), 1), 10);
        assert_eq!(map_u32(production.data(), 2), 200);
        assert_eq!(map_u32(production.data(), 3), 300);
    }

    struct TraceHost {
        node: NodeId,
        bindings: TestBindings,
    }

    impl ResolveHost for TraceHost {
        fn produce(
            &mut self,
            query: &QueryKey,
            session: &mut ResolveSession<'_>,
        ) -> Result<Production, SessionResolveError> {
            match query {
                QueryKey::ProducedSlot { node, slot } if *node == self.node => {
                    Ok(Production::value(
                        WithRevision::new(session.revision(), LpsValueF32::F32(0.5)),
                        ProductionSource::ProducedSlot {
                            node: *node,
                            slot: slot.clone(),
                        },
                    )?)
                }
                _ => Err(SessionResolveError::other("trace host")),
            }
        }

        fn providers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, BindingEntry)> {
            self.bindings.providers_for_bus(channel)
        }
    }

    #[test]
    fn trace_events_when_logging_basic() {
        let mut resolver = Resolver::new();
        let mut bindings = TestBindings::default();
        let frame = Revision::new(5);
        let bus = ch("out");
        let node = NodeId::new(3);
        let out = path("rgb");
        bindings.add(
            BindingDraft {
                source: BindingSource::ProducedSlot {
                    node,
                    slot: out.clone(),
                },
                target: BindingTarget::BusChannel(bus.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Color,
                owner: node,
            },
            frame,
        );

        let trace = ResolveTrace::new(ResolveLogLevel::Basic);
        let mut host = TraceHost { node, bindings };
        let mut session = ResolveSession::new(frame, &mut resolver, trace);
        session
            .resolve(&mut host, QueryKey::Bus(bus.clone()))
            .unwrap();
        // Second resolve — cache hit on bus
        session.resolve(&mut host, QueryKey::Bus(bus)).unwrap();

        let evs = session.trace().events();
        assert!(evs.iter().any(|e| {
            matches!(e, ResolveTraceEvent::BeginQuery(QueryKey::Bus(b)) if b.0 == "out")
        }));
        assert!(
            evs.iter()
                .any(|e| matches!(e, ResolveTraceEvent::SelectBinding { .. }))
        );
        assert!(evs.iter().any(|e| matches!(
            e,
            ResolveTraceEvent::ProduceStart(QueryKey::ProducedSlot { .. })
        )));
        assert!(evs.iter().any(|e| matches!(
            e,
            ResolveTraceEvent::ProduceEnd(QueryKey::ProducedSlot { .. })
        )));
        assert!(evs.iter().any(|e| matches!(
            e,
            ResolveTraceEvent::CacheHit(QueryKey::Bus(b)) if b.0 == "out"
        )));
    }
}
