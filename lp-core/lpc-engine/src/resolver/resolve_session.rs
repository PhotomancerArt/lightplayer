//! [`ResolveSession`] — per-frame demand resolution: cache, trace stack, registry, [`ResolveHost`].

use alloc::format;
use alloc::vec::Vec;

use crate::binding::{BindingEntry, BindingRegistry, BindingSource, BindingTarget};
use crate::resolver::produced_value::{ProducedValue, ProductionSource};
use crate::resolver::query_key::QueryKey;
use crate::resolver::resolve_error::SessionResolveError;
use crate::resolver::resolve_host::ResolveHost;
use crate::resolver::resolve_trace::{ResolveTrace, ResolveTraceEvent};
use crate::resolver::resolver::Resolver;
use crate::resolver::resolver::materialize_src_value_literal;
use lpc_model::ChannelName;
use lpc_model::prop::prop_path::PropPath;
use lpc_model::{FrameId, NodeId};

/// Active resolution session for one frame (or nested test scope).
pub struct ResolveSession<'a> {
    frame_id: FrameId,
    resolver: &'a mut Resolver,
    registry: &'a BindingRegistry,
    trace: ResolveTrace,
}

impl<'a> ResolveSession<'a> {
    pub fn new(
        frame_id: FrameId,
        resolver: &'a mut Resolver,
        registry: &'a BindingRegistry,
        trace: ResolveTrace,
    ) -> Self {
        Self {
            frame_id,
            resolver,
            registry,
            trace,
        }
    }

    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    pub fn trace(&self) -> &ResolveTrace {
        &self.trace
    }

    /// Demand-resolve `query` for this frame (cache + cycle stack + registry + host).
    pub fn resolve<H: ResolveHost + ?Sized>(
        &mut self,
        host: &mut H,
        query: QueryKey,
    ) -> Result<ProducedValue, SessionResolveError> {
        if let Some(pv) = self.resolver.cache().get(&query) {
            self.trace
                .record_event(ResolveTraceEvent::CacheHit(query.clone()));
            return Ok(pv.clone());
        }

        self.trace
            .try_push_active(query.clone())
            .map_err(SessionResolveError::from)?;
        let inner_result = self.resolve_uncached(host, query.clone());
        self.trace.exit(&query);
        let result = inner_result?;
        self.resolver.cache_mut().insert(query, result.clone());
        Ok(result)
    }

    fn resolve_uncached<H: ResolveHost + ?Sized>(
        &mut self,
        host: &mut H,
        query: QueryKey,
    ) -> Result<ProducedValue, SessionResolveError> {
        match &query {
            QueryKey::Bus(channel) => self.resolve_bus(host, channel, &query),
            QueryKey::NodeInput { node, input } => {
                self.resolve_node_input(host, *node, input.clone(), &query)
            }
            QueryKey::NodeOutput { .. } => {
                self.trace
                    .record_event(ResolveTraceEvent::ProduceStart(query.clone()));
                let r = host.produce(&query, self);
                match &r {
                    Ok(_) => self
                        .trace
                        .record_event(ResolveTraceEvent::ProduceEnd(query.clone())),
                    Err(_) => self.trace.record_event(ResolveTraceEvent::ResolveError {
                        query: query.clone(),
                    }),
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
    ) -> Result<ProducedValue, SessionResolveError> {
        let candidates: Vec<&BindingEntry> = self.registry.providers_for_bus(channel).collect();
        let entry = select_highest_priority_bus_provider(channel, &candidates)?;
        self.trace.record_event(ResolveTraceEvent::SelectBinding {
            query: query.clone(),
            binding: entry.id,
        });
        self.resolve_binding_source(host, entry.id, &entry.source)
    }

    fn resolve_node_input(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        node: NodeId,
        input: PropPath,
        query: &QueryKey,
    ) -> Result<ProducedValue, SessionResolveError> {
        if let Some(entry) = find_binding_for_node_input(self.registry, node, &input) {
            self.trace.record_event(ResolveTraceEvent::SelectBinding {
                query: query.clone(),
                binding: entry.id,
            });
            return self.resolve_binding_source(host, entry.id, &entry.source);
        }
        self.trace
            .record_event(ResolveTraceEvent::ProduceStart(query.clone()));
        let r = host.produce(query, self);
        match &r {
            Ok(_) => self
                .trace
                .record_event(ResolveTraceEvent::ProduceEnd(query.clone())),
            Err(_) => self.trace.record_event(ResolveTraceEvent::ResolveError {
                query: query.clone(),
            }),
        }
        r
    }

    fn resolve_binding_source(
        &mut self,
        host: &mut (impl ResolveHost + ?Sized),
        binding_id: crate::binding::BindingId,
        source: &BindingSource,
    ) -> Result<ProducedValue, SessionResolveError> {
        match source {
            BindingSource::Literal(spec) => {
                let versioned =
                    materialize_src_value_literal(spec, self.frame_id).map_err(|e| {
                        SessionResolveError::other(format!(
                            "literal materialization: {}",
                            e.message
                        ))
                    })?;
                Ok(ProducedValue::new(versioned, ProductionSource::Literal))
            }
            BindingSource::NodeOutput { node, output } => {
                let key = QueryKey::NodeOutput {
                    node: *node,
                    output: output.clone(),
                };
                let mut pv = self.resolve(host, key)?;
                pv.source = ProductionSource::BusBinding {
                    binding: binding_id,
                };
                Ok(pv)
            }
            BindingSource::BusChannel(other) => {
                let key = QueryKey::Bus(other.clone());
                let mut pv = self.resolve(host, key)?;
                pv.source = ProductionSource::BusBinding {
                    binding: binding_id,
                };
                Ok(pv)
            }
        }
    }
}

fn find_binding_for_node_input<'a>(
    registry: &'a BindingRegistry,
    node: NodeId,
    input: &PropPath,
) -> Option<&'a BindingEntry> {
    registry.iter().find(|e| {
        matches!(
            &e.target,
            BindingTarget::NodeInput { node: n, input: p } if *n == node && p == input
        )
    })
}

fn select_highest_priority_bus_provider<'a>(
    channel: &ChannelName,
    candidates: &[&'a BindingEntry],
) -> Result<&'a BindingEntry, SessionResolveError> {
    if candidates.is_empty() {
        return Err(SessionResolveError::NoBusProvider {
            channel: channel.clone(),
        });
    }
    let Some(max_p) = candidates.iter().map(|e| e.priority).max() else {
        return Err(SessionResolveError::NoBusProvider {
            channel: channel.clone(),
        });
    };
    let at_max: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|e| e.priority == max_p)
        .collect();
    if at_max.len() != 1 {
        return Err(SessionResolveError::AmbiguousBusBinding {
            channel: channel.clone(),
        });
    }
    Ok(at_max[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::BindingDraft;
    use crate::binding::BindingId;
    use crate::binding::BindingPriority;
    use crate::resolver::resolve_trace::ResolveLogLevel;
    use alloc::string::String;
    use lpc_model::Kind;
    use lpc_model::prop::prop_path::parse_path;
    use lpc_model::{ChannelName, Versioned};
    use lpc_source::SrcValueSpec;
    use lps_shared::LpsValueF32;

    fn ch(s: &str) -> ChannelName {
        ChannelName(String::from(s))
    }

    fn path(s: &str) -> PropPath {
        parse_path(s).expect("path")
    }

    struct CountingHost {
        produce_calls: u32,
        node: NodeId,
        out_path: PropPath,
    }

    impl CountingHost {
        fn new(node: NodeId, out_path: PropPath) -> Self {
            Self {
                produce_calls: 0,
                node,
                out_path,
            }
        }
    }

    impl ResolveHost for CountingHost {
        fn produce(
            &mut self,
            query: &QueryKey,
            session: &mut ResolveSession<'_>,
        ) -> Result<ProducedValue, SessionResolveError> {
            self.produce_calls += 1;
            match query {
                QueryKey::NodeOutput { node, output }
                    if *node == self.node && *output == self.out_path =>
                {
                    Ok(ProducedValue::new(
                        Versioned::new(session.frame_id(), LpsValueF32::F32(42.0)),
                        ProductionSource::NodeOutput {
                            node: *node,
                            output: output.clone(),
                        },
                    ))
                }
                _ => Err(SessionResolveError::other("unexpected produce query")),
            }
        }
    }

    #[test]
    fn same_node_output_twice_calls_host_once() {
        let mut resolver = Resolver::new();
        let registry = BindingRegistry::new();
        let frame = FrameId::new(1);
        let node = NodeId::new(7);
        let out = path("color");
        let key = QueryKey::NodeOutput {
            node,
            output: out.clone(),
        };
        let mut host = CountingHost::new(node, out.clone());
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            &registry,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let a = session.resolve(&mut host, key.clone()).unwrap();
        let b = session.resolve(&mut host, key).unwrap();
        assert!(a.value.get().eq(&LpsValueF32::F32(42.0)));
        assert!(b.value.get().eq(&LpsValueF32::F32(42.0)));
        assert_eq!(host.produce_calls, 1);
    }

    #[test]
    fn bus_channel_selects_highest_priority_binding() {
        let mut resolver = Resolver::new();
        let mut registry = BindingRegistry::new();
        let frame = FrameId::new(2);
        let c = ch("video");
        let low_node = NodeId::new(1);
        let high_node = NodeId::new(2);
        registry
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::F32(1.0),
                    )),
                    target: BindingTarget::BusChannel(c.clone()),
                    priority: BindingPriority::new(1),
                    kind: Kind::Amplitude,
                    owner: low_node,
                },
                frame,
            )
            .unwrap();
        registry
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::F32(9.0),
                    )),
                    target: BindingTarget::BusChannel(c.clone()),
                    priority: BindingPriority::new(10),
                    kind: Kind::Amplitude,
                    owner: high_node,
                },
                frame,
            )
            .unwrap();

        let mut host = CountingHost::new(low_node, path("x"));
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            &registry,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let pv = session
            .resolve(&mut host, QueryKey::Bus(c))
            .expect("resolve bus");
        assert!(pv.value.get().eq(&LpsValueF32::F32(9.0)));
        assert_eq!(host.produce_calls, 0);
    }

    #[test]
    fn equal_priority_bus_providers_return_ambiguous_error() {
        let e1 = BindingEntry {
            id: BindingId::new(1),
            source: BindingSource::Literal(SrcValueSpec::Literal(lpc_model::ModelValue::F32(1.0))),
            target: BindingTarget::BusChannel(ch("z")),
            priority: BindingPriority::new(5),
            kind: Kind::Amplitude,
            version: FrameId::new(0),
            owner: NodeId::new(0),
        };
        let e2 = BindingEntry {
            id: BindingId::new(2),
            source: BindingSource::Literal(SrcValueSpec::Literal(lpc_model::ModelValue::F32(2.0))),
            target: BindingTarget::BusChannel(ch("z")),
            priority: BindingPriority::new(5),
            kind: Kind::Amplitude,
            version: FrameId::new(0),
            owner: NodeId::new(1),
        };
        let c = ch("z");
        let err = select_highest_priority_bus_provider(&c, &[&e1, &e2]).unwrap_err();
        assert!(matches!(
            err,
            SessionResolveError::AmbiguousBusBinding { .. }
        ));
    }

    #[test]
    fn bus_to_bus_recursion_resolves_through_both_labels() {
        let mut resolver = Resolver::new();
        let mut registry = BindingRegistry::new();
        let frame = FrameId::new(3);
        let outer = ch("a");
        let inner = ch("b");
        registry
            .register(
                BindingDraft {
                    source: BindingSource::BusChannel(inner.clone()),
                    target: BindingTarget::BusChannel(outer.clone()),
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: NodeId::new(0),
                },
                frame,
            )
            .unwrap();
        registry
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::F32(3.25),
                    )),
                    target: BindingTarget::BusChannel(inner.clone()),
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: NodeId::new(1),
                },
                frame,
            )
            .unwrap();

        let mut host = CountingHost::new(NodeId::new(99), path("noop"));
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            &registry,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let pv = session
            .resolve(&mut host, QueryKey::Bus(outer))
            .expect("bus chain");
        assert!(pv.value.get().eq(&LpsValueF32::F32(3.25)));
    }

    struct NoProduceHost;

    impl ResolveHost for NoProduceHost {
        fn produce(
            &mut self,
            _query: &QueryKey,
            _session: &mut ResolveSession<'_>,
        ) -> Result<ProducedValue, SessionResolveError> {
            Err(SessionResolveError::other(
                "produce should not run in bus-only cycle test",
            ))
        }
    }

    #[test]
    fn bus_recursion_cycle_is_detected() {
        let mut resolver = Resolver::new();
        let mut registry = BindingRegistry::new();
        let frame = FrameId::new(4);
        let a = ch("loop_a");
        let b = ch("loop_b");
        registry
            .register(
                BindingDraft {
                    source: BindingSource::BusChannel(b.clone()),
                    target: BindingTarget::BusChannel(a.clone()),
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: NodeId::new(0),
                },
                frame,
            )
            .unwrap();
        registry
            .register(
                BindingDraft {
                    source: BindingSource::BusChannel(a.clone()),
                    target: BindingTarget::BusChannel(b.clone()),
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: NodeId::new(1),
                },
                frame,
            )
            .unwrap();

        let mut host = NoProduceHost;
        let mut session = ResolveSession::new(
            frame,
            &mut resolver,
            &registry,
            ResolveTrace::new(ResolveLogLevel::Off),
        );
        let err = session
            .resolve(&mut host, QueryKey::Bus(a))
            .expect_err("cycle");
        assert!(matches!(err, SessionResolveError::Cycle { .. }));
    }

    struct TraceHost {
        node: NodeId,
    }

    impl ResolveHost for TraceHost {
        fn produce(
            &mut self,
            query: &QueryKey,
            session: &mut ResolveSession<'_>,
        ) -> Result<ProducedValue, SessionResolveError> {
            match query {
                QueryKey::NodeOutput { node, output } if *node == self.node => {
                    Ok(ProducedValue::new(
                        Versioned::new(session.frame_id(), LpsValueF32::F32(0.5)),
                        ProductionSource::NodeOutput {
                            node: *node,
                            output: output.clone(),
                        },
                    ))
                }
                _ => Err(SessionResolveError::other("trace host")),
            }
        }
    }

    #[test]
    fn trace_events_when_logging_basic() {
        let mut resolver = Resolver::new();
        let mut registry = BindingRegistry::new();
        let frame = FrameId::new(5);
        let bus = ch("out");
        let node = NodeId::new(3);
        let out = path("rgb");
        registry
            .register(
                BindingDraft {
                    source: BindingSource::NodeOutput {
                        node,
                        output: out.clone(),
                    },
                    target: BindingTarget::BusChannel(bus.clone()),
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: node,
                },
                frame,
            )
            .unwrap();

        let trace = ResolveTrace::new(ResolveLogLevel::Basic);
        let mut host = TraceHost { node };
        let mut session = ResolveSession::new(frame, &mut resolver, &registry, trace);
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
            ResolveTraceEvent::ProduceStart(QueryKey::NodeOutput { .. })
        )));
        assert!(evs.iter().any(|e| matches!(
            e,
            ResolveTraceEvent::ProduceEnd(QueryKey::NodeOutput { .. })
        )));
        assert!(evs.iter().any(|e| matches!(
            e,
            ResolveTraceEvent::CacheHit(QueryKey::Bus(b)) if b.0 == "out"
        )));
    }
}
