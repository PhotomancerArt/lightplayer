//! Narrow contexts passed into [`super::Node`] hooks.
//!
//! Phase 5 wires resolver, bus, and artifact access to [`TickContext`].

use crate::artifact::ArtifactId;
use crate::bus::Bus;
use crate::resolver::{ResolveError, ResolvedSlot, ResolverCache, ResolverContext, resolve_slot};
use lpc_model::{FrameId, NodeId, bus::ChannelName, prop::prop_path::PropPath};
use lpc_source::node::src_node_config::SrcNodeConfig;
use lps_shared::LpsValueF32;

/// Context for [`super::Node::tick`](super::Node::tick).
///
/// Provides access to resolver cache, bus, artifact state, and frame info.
/// Delegates binding resolution via the resolver cascade using a supplied
/// [`ResolverContext`] facade.
pub struct TickContext<'a> {
    node_id: NodeId,
    frame_id: FrameId,
    config: &'a SrcNodeConfig,
    resolver_cache: &'a mut ResolverCache,
    artifact_ref: ArtifactId,
    artifact_content_frame: FrameId,
    bus: &'a Bus,
    resolver: &'a dyn ResolverContext,
}

impl<'a> TickContext<'a> {
    /// Create a new tick context with all runtime dependencies.
    pub fn new(
        node_id: NodeId,
        frame_id: FrameId,
        config: &'a SrcNodeConfig,
        resolver_cache: &'a mut ResolverCache,
        artifact_ref: ArtifactId,
        artifact_content_frame: FrameId,
        bus: &'a Bus,
        resolver: &'a dyn ResolverContext,
    ) -> Self {
        Self {
            node_id,
            frame_id,
            config,
            resolver_cache,
            artifact_ref,
            artifact_content_frame,
            bus,
            resolver,
        }
    }

    /// Current node identity.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Current frame being processed.
    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    /// Resolve a property path through the binding cascade.
    ///
    /// Resolution priority:
    /// 1. `config.overrides[prop]`
    /// 2. artifact slot `bind` (via resolver context)
    /// 3. artifact slot `default` (via resolver context)
    ///
    /// Populates the resolver cache on success.
    pub fn resolve(&mut self, prop: &PropPath) -> Result<&ResolvedSlot, ResolveError> {
        resolve_slot(self.resolver_cache, self.config, prop, self.resolver)
    }

    /// Check if a property has changed since the given frame.
    ///
    /// Returns `true` if the property exists in cache and its `changed_frame`
    /// is newer than `since`.
    pub fn changed_since(&self, prop: &PropPath, since: FrameId) -> bool {
        self.resolver_cache
            .get(prop)
            .map(|slot| slot.changed_frame.0 > since.0)
            .unwrap_or(false)
    }

    /// Check if the node's artifact content has changed since the given frame.
    ///
    /// Compares `since` against the artifact's `content_frame`.
    pub fn artifact_changed_since(&self, since: FrameId) -> bool {
        self.artifact_content_frame.0 > since.0
    }

    /// Read the current value from a bus channel.
    ///
    /// Returns `None` if the channel doesn't exist or has no value.
    pub fn bus_read(&self, channel: &ChannelName) -> Option<&LpsValueF32> {
        self.bus.read(channel)
    }

    /// Get the frame at which a bus channel was last written.
    ///
    /// Returns `FrameId::new(0)` if the channel has never been written.
    pub fn bus_last_writer_frame(&self, channel: &ChannelName) -> FrameId {
        self.bus.last_writer_frame(channel)
    }

    /// Get the artifact reference for this node.
    pub fn artifact_ref(&self) -> ArtifactId {
        self.artifact_ref
    }

    /// Get the artifact content frame (when the artifact was last loaded/reloaded).
    pub fn artifact_content_frame(&self) -> FrameId {
        self.artifact_content_frame
    }
}

/// Context for [`super::Node::destroy`](super::Node::destroy).
pub struct DestroyCtx<'a> {
    node_id: NodeId,
    frame_id: FrameId,
    bus: &'a Bus,
}

impl<'a> DestroyCtx<'a> {
    /// Create a new destroy context.
    pub fn new(node_id: NodeId, frame_id: FrameId, bus: &'a Bus) -> Self {
        Self {
            node_id,
            frame_id,
            bus,
        }
    }

    /// Node being destroyed.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Frame at which destruction is occurring.
    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    /// Read the current value from a bus channel.
    pub fn bus_read(&self, channel: &ChannelName) -> Option<&LpsValueF32> {
        self.bus.read(channel)
    }
}

/// Context for [`super::Node::handle_memory_pressure`](super::Node::handle_memory_pressure).
pub struct MemPressureCtx<'a> {
    node_id: NodeId,
    frame_id: FrameId,
    bus: &'a Bus,
}

impl<'a> MemPressureCtx<'a> {
    /// Create a new memory pressure context.
    pub fn new(node_id: NodeId, frame_id: FrameId, bus: &'a Bus) -> Self {
        Self {
            node_id,
            frame_id,
            bus,
        }
    }

    /// Node under pressure.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Current frame.
    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    /// Read the current value from a bus channel.
    pub fn bus_read(&self, channel: &ChannelName) -> Option<&LpsValueF32> {
        self.bus.read(channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::Bus;
    use crate::node::node::Node;
    use crate::resolver::{BindingKind, ResolveSource, ResolverCache};
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use lpc_model::bus::ChannelName;
    use lpc_model::prop::prop_path::parse_path;
    use lpc_model::tree::tree_path::TreePath;
    use lpc_source::artifact::src_artifact_spec::SrcArtifactSpec;
    use lpc_source::prop::src_binding::SrcBinding;

    /// Test resolver context that provides full resolver capabilities.
    struct TestResolverContext {
        frame: FrameId,
        bus: BTreeMap<ChannelName, (LpsValueF32, FrameId)>,
        bindings: BTreeMap<PropPath, SrcBinding>,
        defaults: BTreeMap<PropPath, LpsValueF32>,
    }

    impl TestResolverContext {
        fn new(frame: FrameId) -> Self {
            Self {
                frame,
                bus: BTreeMap::new(),
                bindings: BTreeMap::new(),
                defaults: BTreeMap::new(),
            }
        }

        fn with_bus(mut self, name: &str, value: LpsValueF32, frame: FrameId) -> Self {
            self.bus
                .insert(ChannelName(String::from(name)), (value, frame));
            self
        }

        fn with_binding(mut self, prop: &str, binding: SrcBinding) -> Self {
            self.bindings.insert(parse_path(prop).unwrap(), binding);
            self
        }

        fn with_default(mut self, prop: &str, value: LpsValueF32) -> Self {
            self.defaults.insert(parse_path(prop).unwrap(), value);
            self
        }
    }

    impl ResolverContext for TestResolverContext {
        fn frame_id(&self) -> FrameId {
            self.frame
        }

        fn bus_value(&self, channel: &ChannelName) -> Option<(&LpsValueF32, FrameId)> {
            self.bus.get(channel).map(|(v, f)| (v, *f))
        }

        fn target_prop(
            &self,
            _node: &TreePath,
            _prop: &PropPath,
        ) -> Option<(LpsValueF32, FrameId)> {
            // NodeProp support deferred to integration phase - would need target node access
            None
        }

        fn artifact_binding(&self, prop: &PropPath) -> Option<SrcBinding> {
            self.bindings.get(prop).cloned()
        }

        fn artifact_default(&self, prop: &PropPath) -> Option<LpsValueF32> {
            self.defaults.get(prop).cloned()
        }
    }

    #[test]
    fn tick_context_accessors() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();
        let artifact_ref = ArtifactId::from_raw(1);
        let resolver = TestResolverContext::new(FrameId::new(10));

        let ctx = TickContext::new(
            NodeId::new(7),
            FrameId::new(3),
            &config,
            &mut cache,
            artifact_ref,
            FrameId::new(5),
            &bus,
            &resolver,
        );

        assert_eq!(ctx.node_id(), NodeId::new(7));
        assert_eq!(ctx.frame_id(), FrameId::new(3));
        assert_eq!(ctx.artifact_ref(), artifact_ref);
        assert_eq!(ctx.artifact_content_frame(), FrameId::new(5));
    }

    #[test]
    fn tick_context_resolve_delegates_to_supplied_resolver() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();

        // Set up resolver with artifact binding
        let resolver = TestResolverContext::new(FrameId::new(10)).with_binding(
            "params.speed",
            SrcBinding::Literal(lpc_source::prop::src_value_spec::SrcValueSpec::Literal(
                lpc_model::ModelValue::F32(5.5),
            )),
        );

        let mut ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        let prop = parse_path("params.speed").unwrap();
        let result = ctx.resolve(&prop).unwrap();

        // Verify resolution came from artifact binding
        assert!(matches!(result.value, LpsValueF32::F32(5.5)));
        assert!(matches!(
            result.source,
            ResolveSource::ArtifactBind(BindingKind::Literal)
        ));

        // Verify cache was populated
        assert!(cache.get(&prop).is_some());
    }

    #[test]
    fn tick_context_resolve_uses_artifact_default_via_resolver() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();

        // Set up resolver with artifact default (no binding)
        let resolver = TestResolverContext::new(FrameId::new(10))
            .with_default("params.scale", LpsValueF32::F32(2.5));

        let mut ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        let prop = parse_path("params.scale").unwrap();
        let result = ctx.resolve(&prop).unwrap();

        // Verify resolution came from artifact default
        assert!(matches!(result.value, LpsValueF32::F32(2.5)));
        assert!(matches!(result.source, ResolveSource::Default));
    }

    #[test]
    fn tick_context_resolve_bus_via_resolver() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();

        // Set up resolver with bus value
        let resolver = TestResolverContext::new(FrameId::new(10))
            .with_binding(
                "params.level",
                SrcBinding::Bus(ChannelName(String::from("level_bus"))),
            )
            .with_bus("level_bus", LpsValueF32::F32(7.8), FrameId::new(5));

        let mut ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        let prop = parse_path("params.level").unwrap();
        let result = ctx.resolve(&prop).unwrap();

        // Verify resolution came from bus via resolver
        assert!(matches!(result.value, LpsValueF32::F32(7.8)));
        assert_eq!(result.changed_frame.as_i64(), 5);
        assert!(matches!(
            result.source,
            ResolveSource::ArtifactBind(BindingKind::Bus)
        ));
    }

    #[test]
    fn tick_context_resolve_override_beats_artifact_binding() {
        let bus = Bus::new();
        let mut config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        // Add override
        config.overrides.push((
            parse_path("params.priority").unwrap(),
            SrcBinding::Literal(lpc_source::prop::src_value_spec::SrcValueSpec::Literal(
                lpc_model::ModelValue::F32(9.9),
            )),
        ));

        let mut cache = ResolverCache::new();

        // Set up resolver with artifact binding (should be overridden)
        let resolver = TestResolverContext::new(FrameId::new(10)).with_binding(
            "params.priority",
            SrcBinding::Literal(lpc_source::prop::src_value_spec::SrcValueSpec::Literal(
                lpc_model::ModelValue::F32(1.0),
            )),
        );

        let mut ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        let prop = parse_path("params.priority").unwrap();
        let result = ctx.resolve(&prop).unwrap();

        // Override value wins
        assert!(matches!(result.value, LpsValueF32::F32(9.9)));
        assert!(matches!(
            result.source,
            ResolveSource::Override(BindingKind::Literal)
        ));
    }

    #[test]
    fn tick_context_changed_since_reads_cache_frames() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();
        let resolver = TestResolverContext::new(FrameId::new(10));

        // Pre-populate cache with a value at frame 5
        let prop = parse_path("params.value").unwrap();
        cache.insert(
            prop.clone(),
            ResolvedSlot::new(
                LpsValueF32::F32(1.0),
                FrameId::new(5),
                ResolveSource::Default,
            ),
        );

        let mut cache_ref = ResolverCache::new();
        cache_ref.insert(
            prop.clone(),
            ResolvedSlot::new(
                LpsValueF32::F32(1.0),
                FrameId::new(5),
                ResolveSource::Default,
            ),
        );

        let ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache_ref,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        // Value changed at frame 5, so it changed since frame 4
        assert!(ctx.changed_since(&prop, FrameId::new(4)));
        // Value did not change since frame 5
        assert!(!ctx.changed_since(&prop, FrameId::new(5)));
        // Value did not change since frame 6
        assert!(!ctx.changed_since(&prop, FrameId::new(6)));
    }

    #[test]
    fn tick_context_changed_since_returns_false_for_unknown_prop() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();
        let resolver = TestResolverContext::new(FrameId::new(10));

        let ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        let unknown_prop = parse_path("params.unknown").unwrap();
        assert!(!ctx.changed_since(&unknown_prop, FrameId::new(0)));
    }

    #[test]
    fn tick_context_artifact_changed_since_compares_content_frame() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();
        let resolver = TestResolverContext::new(FrameId::new(10));

        let ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(5), // artifact content frame
            &bus,
            &resolver,
        );

        // Artifact loaded at frame 5, so it changed since frame 4
        assert!(ctx.artifact_changed_since(FrameId::new(4)));
        // Artifact did not change since frame 5
        assert!(!ctx.artifact_changed_since(FrameId::new(5)));
        // Artifact did not change since frame 6
        assert!(!ctx.artifact_changed_since(FrameId::new(6)));
    }

    #[test]
    fn tick_context_bus_read_returns_value() {
        let mut bus = Bus::new();
        let channel = ChannelName(String::from("test"));

        bus.claim_writer(
            &channel,
            NodeId::new(1),
            parse_path("outputs[0]").unwrap(),
            lpc_model::Kind::Amplitude,
        )
        .unwrap();
        bus.publish(&channel, LpsValueF32::F32(7.5), FrameId::new(3));

        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();
        let resolver = TestResolverContext::new(FrameId::new(10));

        let ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        let val = ctx.bus_read(&channel).unwrap();
        assert!(matches!(val, LpsValueF32::F32(7.5)));
    }

    #[test]
    fn tick_context_bus_read_missing_returns_none() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();
        let resolver = TestResolverContext::new(FrameId::new(10));

        let ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        let missing = ChannelName(String::from("missing"));
        assert!(ctx.bus_read(&missing).is_none());
    }

    #[test]
    fn tick_context_bus_last_writer_frame() {
        let mut bus = Bus::new();
        let channel = ChannelName(String::from("test"));

        bus.claim_writer(
            &channel,
            NodeId::new(1),
            parse_path("outputs[0]").unwrap(),
            lpc_model::Kind::Amplitude,
        )
        .unwrap();
        bus.publish(&channel, LpsValueF32::F32(1.0), FrameId::new(7));

        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();
        let resolver = TestResolverContext::new(FrameId::new(10));

        let ctx = TickContext::new(
            NodeId::new(1),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        assert_eq!(ctx.bus_last_writer_frame(&channel).as_i64(), 7);
    }

    #[test]
    fn destroy_ctx_accessors() {
        let bus = Bus::new();
        let ctx = DestroyCtx::new(NodeId::new(1), FrameId::new(99), &bus);
        assert_eq!(ctx.node_id(), NodeId::new(1));
        assert_eq!(ctx.frame_id(), FrameId::new(99));
    }

    #[test]
    fn destroy_ctx_bus_read() {
        let mut bus = Bus::new();
        let channel = ChannelName(String::from("test"));

        bus.claim_writer(
            &channel,
            NodeId::new(1),
            parse_path("outputs[0]").unwrap(),
            lpc_model::Kind::Amplitude,
        )
        .unwrap();
        bus.publish(&channel, LpsValueF32::F32(2.5), FrameId::new(5));

        let ctx = DestroyCtx::new(NodeId::new(1), FrameId::new(99), &bus);
        let val = ctx.bus_read(&channel).unwrap();
        assert!(matches!(val, LpsValueF32::F32(2.5)));
    }

    #[test]
    fn mem_pressure_ctx_accessors() {
        let bus = Bus::new();
        let ctx = MemPressureCtx::new(NodeId::new(2), FrameId::new(100), &bus);
        assert_eq!(ctx.node_id(), NodeId::new(2));
        assert_eq!(ctx.frame_id(), FrameId::new(100));
    }

    #[test]
    fn mem_pressure_ctx_bus_read() {
        let mut bus = Bus::new();
        let channel = ChannelName(String::from("pressure"));

        bus.claim_writer(
            &channel,
            NodeId::new(1),
            parse_path("outputs[0]").unwrap(),
            lpc_model::Kind::Amplitude,
        )
        .unwrap();
        bus.publish(&channel, LpsValueF32::F32(0.8), FrameId::new(2));

        let ctx = MemPressureCtx::new(NodeId::new(2), FrameId::new(100), &bus);
        let val = ctx.bus_read(&channel).unwrap();
        assert!(matches!(val, LpsValueF32::F32(0.8)));
    }

    // Dummy node that uses ctx.resolve() from tick() - proves the context works end-to-end
    struct ResolvingNode {
        target_prop: PropPath,
        resolved_value: Option<f32>,
    }

    impl super::super::Node for ResolvingNode {
        fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), crate::node::NodeError> {
            let slot = ctx.resolve(&self.target_prop).map_err(|e| {
                crate::node::NodeError::msg(alloc::format!("resolve failed: {}", e.message))
            })?;
            if let LpsValueF32::F32(v) = slot.value {
                self.resolved_value = Some(v);
            }
            Ok(())
        }

        fn destroy(
            &mut self,
            _ctx: &mut super::DestroyCtx<'_>,
        ) -> Result<(), crate::node::NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: super::super::PressureLevel,
            _ctx: &mut super::MemPressureCtx<'_>,
        ) -> Result<(), crate::node::NodeError> {
            Ok(())
        }

        fn props(&self) -> &dyn crate::prop::RuntimePropAccess {
            // Dummy implementation for testing
            struct EmptyProps;
            impl crate::prop::RuntimePropAccess for EmptyProps {
                fn get(&self, _path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
                    None
                }
                fn iter_changed_since<'a>(
                    &'a self,
                    _since: FrameId,
                ) -> alloc::boxed::Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a>
                {
                    alloc::boxed::Box::new(alloc::vec::Vec::new().into_iter())
                }
                fn snapshot<'a>(
                    &'a self,
                ) -> alloc::boxed::Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a>
                {
                    alloc::boxed::Box::new(alloc::vec::Vec::new().into_iter())
                }
            }
            static EMPTY_PROPS: EmptyProps = EmptyProps;
            &EMPTY_PROPS
        }
    }

    #[test]
    fn dummy_node_can_resolve_artifact_default_from_tick() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();

        // Set up resolver with artifact default
        let resolver = TestResolverContext::new(FrameId::new(10))
            .with_default("params.value", LpsValueF32::F32(4.2));

        let mut node = ResolvingNode {
            target_prop: parse_path("params.value").unwrap(),
            resolved_value: None,
        };

        let mut ctx = TickContext::new(
            NodeId::new(2),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        node.tick(&mut ctx).expect("tick should succeed");
        assert_eq!(node.resolved_value, Some(4.2));
    }

    #[test]
    fn dummy_node_can_resolve_bus_binding_from_tick() {
        let bus = Bus::new();
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let mut cache = ResolverCache::new();

        // Set up resolver with bus binding and value
        let resolver = TestResolverContext::new(FrameId::new(10))
            .with_binding(
                "params.input",
                SrcBinding::Bus(ChannelName(String::from("in"))),
            )
            .with_bus("in", LpsValueF32::F32(8.8), FrameId::new(5));

        let mut node = ResolvingNode {
            target_prop: parse_path("params.input").unwrap(),
            resolved_value: None,
        };

        let mut ctx = TickContext::new(
            NodeId::new(2),
            FrameId::new(10),
            &config,
            &mut cache,
            ArtifactId::from_raw(1),
            FrameId::new(1),
            &bus,
            &resolver,
        );

        node.tick(&mut ctx).expect("tick should succeed");
        assert_eq!(node.resolved_value, Some(8.8));
    }
}
