//! Resolver — binding cascade implementation for consumed slots.
//!
//! Resolution priority:
//! 1. `SrcNodeConfig.overrides[prop]`
//! 2. artifact slot `bind`
//! 3. artifact slot `default`
//!
//! [`Resolver`] (cache owner) supports the engine demand path; slot cascade functions below are
//! unchanged.

use crate::resolver::binding_kind::BindingKind;
use crate::resolver::resolve_error::ResolveError;
use crate::resolver::resolve_source::ResolveSource;
use crate::resolver::resolved_slot::ResolvedSlot;
use crate::resolver::resolver_cache::ResolverCache;
use crate::resolver::resolver_context::ResolverContext;
use crate::resolver::slot_resolver_cache::SlotResolverCache;
use lpc_model::FrameId;
use lpc_model::Versioned;
use lpc_model::prop::prop_namespace::PropNamespace;
use lpc_model::prop::prop_path::PropPath;
use lpc_source::node::src_node_config::SrcNodeConfig;
use lpc_source::prop::src_binding::SrcBinding;
use lpc_source::prop::src_value_spec::{LoadCtx, SrcValueSpec};
use lps_shared::LpsValueF32;

/// Owns the same-frame [`ResolverCache`] for engine demand resolution.
#[derive(Clone, Debug, Default)]
pub struct Resolver {
    cache: ResolverCache,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            cache: ResolverCache::new(),
        }
    }

    pub fn cache(&self) -> &ResolverCache {
        &self.cache
    }

    pub fn cache_mut(&mut self) -> &mut ResolverCache {
        &mut self.cache
    }

    pub fn clear_frame_cache(&mut self) {
        self.cache.clear();
    }
}

pub(crate) fn materialize_src_value_literal(
    spec: &SrcValueSpec,
    frame: FrameId,
) -> Result<Versioned<LpsValueF32>, ResolveError> {
    let mut load_ctx = LoadCtx::default();
    let model_value = spec.default_model_value(&mut load_ctx);
    let lps_value = model_value_to_lps_value_f32(&model_value)?;
    Ok(Versioned::new(frame, lps_value))
}

/// Resolve a slot value using the binding cascade.
///
/// Walks the resolution priority on each call:
/// 1. Override from `config.overrides`
/// 2. Artifact binding
/// 3. Artifact default
///
/// On success, refreshes the cache and returns a reference to the cached slot.
/// If the resolved value is unchanged from the previous cache entry, the
/// previous `changed_frame` is retained.
/// Returns an error only for unrecoverable failures (not for fall-through to default).
pub fn resolve_slot<'a, C: ResolverContext + ?Sized>(
    cache: &'a mut SlotResolverCache,
    config: &SrcNodeConfig,
    prop: &PropPath,
    ctx: &C,
) -> Result<&'a ResolvedSlot, ResolveError> {
    let mut resolved = try_resolve_cascade(config, prop, ctx)?;
    if let Some(previous) = cache.get(prop) {
        if previous.value.eq(&resolved.value) {
            resolved.changed_frame = previous.changed_frame;
        }
    }
    cache.insert(prop.clone(), resolved);

    cache
        .get(prop)
        .ok_or_else(|| ResolveError::new("cache lookup failed - internal error"))
}

/// Resolve a slot value and return owned result (non-caching variant).
///
/// Use this when managing cache separately; `resolve_slot` is preferred.
pub fn resolve_slot_owned<C: ResolverContext + ?Sized>(
    config: &SrcNodeConfig,
    prop: &PropPath,
    ctx: &C,
) -> Result<ResolvedSlot, ResolveError> {
    try_resolve_cascade(config, prop, ctx)
}

/// Internal cascade: override -> artifact bind -> default.
fn try_resolve_cascade<C: ResolverContext + ?Sized>(
    config: &SrcNodeConfig,
    prop: &PropPath,
    ctx: &C,
) -> Result<ResolvedSlot, ResolveError> {
    // Priority 1: Check overrides
    for (override_path, binding) in &config.overrides {
        if override_path == prop {
            return resolve_binding(binding, prop, ctx, ResolveSource::Override);
        }
    }

    // Priority 2: Check artifact binding
    if let Some(binding) = ctx.artifact_binding(prop) {
        return resolve_binding(&binding, prop, ctx, ResolveSource::ArtifactBind);
    }

    // Priority 3: Use artifact default
    resolve_default(prop, ctx)
}

/// Resolve a binding to a slot value.
fn resolve_binding<C: ResolverContext + ?Sized>(
    binding: &SrcBinding,
    prop: &PropPath,
    ctx: &C,
    source_fn: impl FnOnce(BindingKind) -> ResolveSource,
) -> Result<ResolvedSlot, ResolveError> {
    match binding {
        SrcBinding::Literal(spec) => resolve_literal(spec, ctx, source_fn),
        SrcBinding::Bus(channel) => resolve_bus(channel, prop, ctx, source_fn),
        SrcBinding::NodeProp(spec) => resolve_node_prop(spec, prop, ctx, source_fn),
    }
}

/// Resolve a literal value spec to LpsValueF32.
fn resolve_literal<C: ResolverContext + ?Sized>(
    spec: &lpc_source::prop::src_value_spec::SrcValueSpec,
    ctx: &C,
    source_fn: impl FnOnce(BindingKind) -> ResolveSource,
) -> Result<ResolvedSlot, ResolveError> {
    // For M4.3, we materialize to ModelValue then convert
    let mut load_ctx = lpc_source::prop::src_value_spec::LoadCtx::default();
    let model_value = spec.default_model_value(&mut load_ctx);

    let lps_value = model_value_to_lps_value_f32(&model_value)?;
    let frame = ctx.frame_id();

    Ok(ResolvedSlot::new(
        lps_value,
        frame,
        source_fn(BindingKind::Literal),
    ))
}

/// Resolve a bus binding.
fn resolve_bus<C: ResolverContext + ?Sized>(
    channel: &lpc_model::bus::ChannelName,
    prop: &PropPath,
    ctx: &C,
    source_fn: impl FnOnce(BindingKind) -> ResolveSource,
) -> Result<ResolvedSlot, ResolveError> {
    // Try to read from bus
    if let Some((value, frame)) = ctx.bus_value(channel) {
        return Ok(ResolvedSlot::new(
            value.clone(),
            frame,
            source_fn(BindingKind::Bus),
        ));
    }

    // Bus not available - fall through to default
    resolve_default(prop, ctx)
}

/// Resolve a NodeProp binding by dereferencing target's produced props.
fn resolve_node_prop<C: ResolverContext + ?Sized>(
    spec: &lpc_model::NodePropSpec,
    prop: &PropPath,
    ctx: &C,
    source_fn: impl FnOnce(BindingKind) -> ResolveSource,
) -> Result<ResolvedSlot, ResolveError> {
    // Validate namespace is outputs
    match spec.target_namespace() {
        Some(PropNamespace::Outputs) => {
            // Valid - continue to dereference
        }
        Some(other) => {
            return Err(ResolveError::node_prop_not_outputs(other.segment_name()));
        }
        None => {
            return Err(ResolveError::node_prop_not_outputs("unknown"));
        }
    }

    // Try to read from target
    if let Some((value, frame)) = ctx.target_prop(&spec.node, &spec.prop) {
        return Ok(ResolvedSlot::new(
            value,
            frame,
            source_fn(BindingKind::NodeProp),
        ));
    }

    // Target not available - fall through to default
    resolve_default(prop, ctx)
}

/// Resolve to artifact default.
fn resolve_default<C: ResolverContext + ?Sized>(
    prop: &PropPath,
    ctx: &C,
) -> Result<ResolvedSlot, ResolveError> {
    let frame = ctx.frame_id();

    if let Some(value) = ctx.artifact_default(prop) {
        Ok(ResolvedSlot::new(value, frame, ResolveSource::Default))
    } else {
        // No default available - use F32(0.0) as floor
        Ok(ResolvedSlot::new(
            LpsValueF32::F32(0.0),
            frame,
            ResolveSource::Failed,
        ))
    }
}

/// Convert ModelValue to LpsValueF32.
///
/// This is the inverse of the wire_bridge conversion. For M4.3, we support
/// the common scalar/vector variants used by tests.
fn model_value_to_lps_value_f32(
    value: &lpc_model::ModelValue,
) -> Result<LpsValueF32, ResolveError> {
    use lpc_model::ModelValue;

    match value {
        ModelValue::I32(v) => Ok(LpsValueF32::I32(*v)),
        ModelValue::U32(v) => Ok(LpsValueF32::U32(*v)),
        ModelValue::F32(v) => Ok(LpsValueF32::F32(*v)),
        ModelValue::Bool(v) => Ok(LpsValueF32::Bool(*v)),
        ModelValue::Vec2(v) => Ok(LpsValueF32::Vec2(*v)),
        ModelValue::Vec3(v) => Ok(LpsValueF32::Vec3(*v)),
        ModelValue::Vec4(v) => Ok(LpsValueF32::Vec4(*v)),
        ModelValue::IVec2(v) => Ok(LpsValueF32::IVec2(*v)),
        ModelValue::IVec3(v) => Ok(LpsValueF32::IVec3(*v)),
        ModelValue::IVec4(v) => Ok(LpsValueF32::IVec4(*v)),
        ModelValue::UVec2(v) => Ok(LpsValueF32::UVec2(*v)),
        ModelValue::UVec3(v) => Ok(LpsValueF32::UVec3(*v)),
        ModelValue::UVec4(v) => Ok(LpsValueF32::UVec4(*v)),
        ModelValue::BVec2(v) => Ok(LpsValueF32::BVec2(*v)),
        ModelValue::BVec3(v) => Ok(LpsValueF32::BVec3(*v)),
        ModelValue::BVec4(v) => Ok(LpsValueF32::BVec4(*v)),
        ModelValue::Mat2x2(v) => Ok(LpsValueF32::Mat2x2(*v)),
        ModelValue::Mat3x3(v) => Ok(LpsValueF32::Mat3x3(*v)),
        ModelValue::Mat4x4(v) => Ok(LpsValueF32::Mat4x4(*v)),
        ModelValue::Array(items) => {
            let mut result = alloc::vec::Vec::with_capacity(items.len());
            for item in items.iter() {
                result.push(model_value_to_lps_value_f32(item)?);
            }
            Ok(LpsValueF32::Array(result.into_boxed_slice()))
        }
        ModelValue::Struct { name, fields } => {
            let mut result_fields = alloc::vec::Vec::with_capacity(fields.len());
            for (k, v) in fields.iter() {
                result_fields.push((k.clone(), model_value_to_lps_value_f32(v)?));
            }
            Ok(LpsValueF32::Struct {
                name: name.clone(),
                fields: result_fields,
            })
        }
        ModelValue::Texture2D {
            ptr,
            width,
            height,
            row_stride,
        } => {
            use lps_shared::{LpsTexture2DDescriptor, LpsTexture2DValue};
            Ok(LpsValueF32::Texture2D(
                LpsTexture2DValue::from_guest_descriptor(LpsTexture2DDescriptor {
                    ptr: *ptr,
                    width: *width,
                    height: *height,
                    row_stride: *row_stride,
                }),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::FrameId;
    use lpc_model::ModelValue;
    use lpc_model::NodePropSpec;
    use lpc_model::bus::ChannelName;
    use lpc_model::prop::prop_path::parse_path;
    use lpc_model::tree::tree_path::TreePath;
    use lpc_source::artifact::src_artifact_spec::SrcArtifactSpec;
    use lpc_source::prop::src_value_spec::SrcValueSpec;

    /// Test resolver context with programmable responses.
    struct TestContext {
        frame: FrameId,
        bus: BTreeMap<ChannelName, (LpsValueF32, FrameId)>,
        targets: BTreeMap<TreePath, Vec<(PropPath, LpsValueF32, FrameId)>>,
        bindings: BTreeMap<PropPath, SrcBinding>,
        defaults: BTreeMap<PropPath, LpsValueF32>,
    }

    impl TestContext {
        fn new(frame: FrameId) -> Self {
            Self {
                frame,
                bus: BTreeMap::new(),
                targets: BTreeMap::new(),
                bindings: BTreeMap::new(),
                defaults: BTreeMap::new(),
            }
        }

        fn with_bus(mut self, name: &str, value: LpsValueF32, frame: FrameId) -> Self {
            self.bus
                .insert(ChannelName(String::from(name)), (value, frame));
            self
        }

        fn with_target(
            mut self,
            node: &str,
            prop: &str,
            value: LpsValueF32,
            frame: FrameId,
        ) -> Self {
            let node_path = TreePath::parse(node).unwrap();
            let prop_path = parse_path(prop).unwrap();
            self.targets
                .entry(node_path)
                .or_default()
                .push((prop_path, value, frame));
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

    impl ResolverContext for TestContext {
        fn frame_id(&self) -> FrameId {
            self.frame
        }

        fn bus_value(&self, channel: &ChannelName) -> Option<(&LpsValueF32, FrameId)> {
            self.bus.get(channel).map(|(v, f)| (v, *f))
        }

        fn target_prop(&self, node: &TreePath, prop: &PropPath) -> Option<(LpsValueF32, FrameId)> {
            self.targets.get(node).and_then(|entries| {
                entries
                    .iter()
                    .find(|(p, _, _)| p == prop)
                    .map(|(_, v, f)| (v.clone(), *f))
            })
        }

        fn artifact_binding(&self, prop: &PropPath) -> Option<SrcBinding> {
            self.bindings.get(prop).cloned()
        }

        fn artifact_default(&self, prop: &PropPath) -> Option<LpsValueF32> {
            self.defaults.get(prop).cloned()
        }
    }

    fn make_config() -> SrcNodeConfig {
        SrcNodeConfig::new(SrcArtifactSpec::path("./test.lp"))
    }

    fn make_config_with_override(prop: &str, binding: SrcBinding) -> SrcNodeConfig {
        let mut config = make_config();
        config.overrides.push((parse_path(prop).unwrap(), binding));
        config
    }

    #[test]
    fn override_literal_beats_artifact_binding() {
        let mut cache = SlotResolverCache::new();
        let config = make_config_with_override(
            "params.speed",
            SrcBinding::Literal(SrcValueSpec::Literal(ModelValue::F32(5.5))),
        );

        let ctx = TestContext::new(FrameId::new(10))
            .with_binding(
                "params.speed",
                SrcBinding::Bus(ChannelName(String::from("bus1"))),
            )
            .with_default("params.speed", LpsValueF32::F32(1.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(5.5)));
        assert!(matches!(
            result.source,
            ResolveSource::Override(BindingKind::Literal)
        ));
    }

    #[test]
    fn artifact_binding_beats_default() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let ctx = TestContext::new(FrameId::new(10))
            .with_binding(
                "params.speed",
                SrcBinding::Literal(SrcValueSpec::Literal(ModelValue::F32(3.5))),
            )
            .with_default("params.speed", LpsValueF32::F32(1.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(3.5)));
        assert!(matches!(
            result.source,
            ResolveSource::ArtifactBind(BindingKind::Literal)
        ));
    }

    #[test]
    fn missing_bus_falls_through_to_default() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let ctx = TestContext::new(FrameId::new(10))
            .with_binding(
                "params.speed",
                SrcBinding::Bus(ChannelName(String::from("missing"))),
            )
            .with_default("params.speed", LpsValueF32::F32(2.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(2.0)));
        assert!(matches!(result.source, ResolveSource::Default));
    }

    #[test]
    fn bus_read_uses_bus_value_and_frame() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let ctx = TestContext::new(FrameId::new(10))
            .with_binding(
                "params.speed",
                SrcBinding::Bus(ChannelName(String::from("bus1"))),
            )
            .with_bus("bus1", LpsValueF32::F32(7.5), FrameId::new(5))
            .with_default("params.speed", LpsValueF32::F32(1.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(7.5)));
        assert_eq!(result.changed_frame.as_i64(), 5);
        assert!(matches!(
            result.source,
            ResolveSource::ArtifactBind(BindingKind::Bus)
        ));
    }

    #[test]
    fn node_prop_reads_target_runtime_prop_access() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let spec = NodePropSpec::parse("/show.source/node1.thing#outputs[0]").unwrap();
        let ctx = TestContext::new(FrameId::new(10))
            .with_binding("params.speed", SrcBinding::NodeProp(spec))
            .with_target(
                "/show.source/node1.thing",
                "outputs[0]",
                LpsValueF32::F32(4.5),
                FrameId::new(8),
            )
            .with_default("params.speed", LpsValueF32::F32(1.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(4.5)));
        assert_eq!(result.changed_frame.as_i64(), 8);
        assert!(matches!(
            result.source,
            ResolveSource::ArtifactBind(BindingKind::NodeProp)
        ));
    }

    #[test]
    fn node_prop_rejects_non_outputs_namespace() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        // params is not outputs namespace
        let spec = NodePropSpec::parse("/show.source/node1.thing#params.value").unwrap();
        let ctx = TestContext::new(FrameId::new(10))
            .with_binding("params.speed", SrcBinding::NodeProp(spec))
            .with_default("params.speed", LpsValueF32::F32(1.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("NodeProp binding must target outputs"));
    }

    #[test]
    fn node_prop_rejects_state_namespace() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        // state is also not outputs namespace
        let spec = NodePropSpec::parse("/show.source/node1.thing#state.counter").unwrap();
        let ctx = TestContext::new(FrameId::new(10))
            .with_binding("params.speed", SrcBinding::NodeProp(spec))
            .with_default("params.speed", LpsValueF32::F32(1.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx);

        assert!(result.is_err());
    }

    #[test]
    fn node_prop_missing_target_falls_to_default() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        // Target doesn't exist, should fall through to default
        let spec = NodePropSpec::parse("/show.source/missing.node#outputs[0]").unwrap();
        let ctx = TestContext::new(FrameId::new(10))
            .with_binding("params.speed", SrcBinding::NodeProp(spec))
            .with_default("params.speed", LpsValueF32::F32(3.0));

        let prop = parse_path("params.speed").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(3.0)));
        assert!(matches!(result.source, ResolveSource::Default));
    }

    #[test]
    fn cache_is_populated_with_expected_value_source_frame() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let ctx =
            TestContext::new(FrameId::new(10)).with_default("params.scale", LpsValueF32::F32(2.5));

        let prop = parse_path("params.scale").unwrap();
        let _result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        // Verify cache was populated
        let cached = cache.get(&prop).unwrap();
        assert!(matches!(cached.value, LpsValueF32::F32(2.5)));
        assert!(matches!(cached.source, ResolveSource::Default));
        assert_eq!(cached.changed_frame.as_i64(), 10);
    }

    #[test]
    fn cache_recomputes_but_preserves_frame_when_value_unchanged() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let mut ctx =
            TestContext::new(FrameId::new(10)).with_default("params.value", LpsValueF32::F32(1.0));

        let prop = parse_path("params.value").unwrap();

        let r1 = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();
        assert!(matches!(r1.value, LpsValueF32::F32(1.0)));
        assert_eq!(r1.changed_frame, FrameId::new(10));

        ctx.frame = FrameId::new(20);
        let r2 = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();
        assert!(matches!(r2.value, LpsValueF32::F32(1.0)));
        assert_eq!(r2.changed_frame, FrameId::new(10));
    }

    #[test]
    fn cache_recomputes_and_updates_frame_when_value_changes() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let mut ctx =
            TestContext::new(FrameId::new(10)).with_default("params.value", LpsValueF32::F32(1.0));
        let prop = parse_path("params.value").unwrap();
        resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        ctx.frame = FrameId::new(20);
        ctx.defaults.clear();
        ctx.defaults.insert(prop.clone(), LpsValueF32::F32(2.0));
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();
        assert!(matches!(result.value, LpsValueF32::F32(2.0)));
        assert_eq!(result.changed_frame, FrameId::new(20));
    }

    #[test]
    fn default_materialization_to_lps_value_f32() {
        let mut cache = SlotResolverCache::new();
        let config = make_config();

        let ctx = TestContext::new(FrameId::new(10));
        // No binding, no default - should use failed floor

        let prop = parse_path("params.missing").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(0.0)));
        assert!(matches!(result.source, ResolveSource::Failed));
    }

    #[test]
    fn override_beats_all() {
        let mut cache = SlotResolverCache::new();
        // Override with bus
        let config = make_config_with_override(
            "inputs.level",
            SrcBinding::Bus(ChannelName(String::from("override_bus"))),
        );

        let ctx = TestContext::new(FrameId::new(10))
            .with_binding(
                "inputs.level",
                SrcBinding::Literal(SrcValueSpec::Literal(ModelValue::F32(1.0))),
            )
            .with_bus("override_bus", LpsValueF32::F32(9.9), FrameId::new(3))
            .with_default("inputs.level", LpsValueF32::F32(0.5));

        let prop = parse_path("inputs.level").unwrap();
        let result = resolve_slot(&mut cache, &config, &prop, &ctx).unwrap();

        assert!(matches!(result.value, LpsValueF32::F32(9.9)));
        assert!(matches!(
            result.source,
            ResolveSource::Override(BindingKind::Bus)
        ));
    }

    #[test]
    fn model_value_conversion_f32() {
        let val = ModelValue::F32(3.14);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        assert!(matches!(lps, LpsValueF32::F32(3.14)));
    }

    #[test]
    fn model_value_conversion_vec3() {
        let val = ModelValue::Vec3([1.0, 2.0, 3.0]);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        assert!(matches!(lps, LpsValueF32::Vec3([1.0, 2.0, 3.0])));
    }

    #[test]
    fn model_value_conversion_array() {
        let val = ModelValue::Array(alloc::vec![ModelValue::F32(1.0), ModelValue::F32(2.0),]);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        match lps {
            LpsValueF32::Array(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(items[0], LpsValueF32::F32(1.0)));
                assert!(matches!(items[1], LpsValueF32::F32(2.0)));
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn model_value_conversion_struct() {
        let val = ModelValue::Struct {
            name: Some(String::from("Test")),
            fields: alloc::vec![
                (String::from("x"), ModelValue::F32(1.0)),
                (String::from("y"), ModelValue::F32(2.0)),
            ],
        };
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        match lps {
            LpsValueF32::Struct { name, fields } => {
                assert_eq!(name.as_deref(), Some("Test"));
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }
}
