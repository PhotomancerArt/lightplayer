//! ResolverContext — narrow facade for resolver access to runtime state.
//!
//! This trait provides the minimal access the **slot** resolver needs to:
//! - Read bus values and their frames
//! - Read target node produced props as [`lps_shared::LpsValueF32`] (same shape as
//!   [`crate::prop::RuntimePropAccess`], not [`crate::resolver::production::Production`])
//! - Look up artifact slot bindings and defaults
//! - Know the current frame

use lpc_model::bus::ChannelName;
use lpc_model::project::FrameId;
use lpc_model::prop::prop_path::PropPath;
use lpc_model::tree::tree_path::TreePath;
use lpc_source::prop::src_binding::SrcBinding;
use lps_shared::LpsValueF32;

/// Narrow context facade for the resolver.
///
/// Implementations provide read-only access to runtime state needed for
/// binding resolution. This trait is intentionally minimal to avoid
/// borrow-heavy APIs that would complicate TickContext construction.
pub trait ResolverContext {
    /// Current frame ID for the tick being processed.
    fn frame_id(&self) -> FrameId;

    /// Read the current value and last-writer frame for a bus channel.
    ///
    /// Returns `None` if the channel doesn't exist or has no value.
    fn bus_value(&self, channel: &ChannelName) -> Option<(&LpsValueF32, FrameId)>;

    /// Read a target node's produced property (outputs or state namespace).
    ///
    /// Returns `None` if the node doesn't exist or the property isn't found.
    ///
    /// Values are shader-runtime [`LpsValueF32`] for this cascade; render-product
    /// handles use the `ResolveSession` / [`crate::resolver::production::Production`] path instead.
    fn target_prop(&self, node: &TreePath, prop: &PropPath) -> Option<(LpsValueF32, FrameId)>;

    /// Get the artifact's binding for a property path, if any.
    ///
    /// This comes from the artifact's slot `bind` field.
    fn artifact_binding(&self, prop: &PropPath) -> Option<SrcBinding>;

    /// Get the artifact's default value for a property path, if any.
    ///
    /// This comes from the artifact's slot `default` or derived shape.
    fn artifact_default(&self, prop: &PropPath) -> Option<LpsValueF32>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::bus::ChannelName;
    use lpc_model::prop::prop_path::parse_path;

    /// A dummy context for testing the resolver in isolation.
    struct DummyContext {
        frame: FrameId,
        bus: BTreeMap<ChannelName, (LpsValueF32, FrameId)>,
        targets: BTreeMap<TreePath, Vec<(PropPath, LpsValueF32, FrameId)>>,
        bindings: BTreeMap<PropPath, SrcBinding>,
        defaults: BTreeMap<PropPath, LpsValueF32>,
    }

    impl DummyContext {
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
            node_path: &str,
            prop: &str,
            value: LpsValueF32,
            frame: FrameId,
        ) -> Self {
            let path = TreePath::parse(node_path).unwrap();
            let prop_path = parse_path(prop).unwrap();
            self.targets
                .entry(path)
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

    impl ResolverContext for DummyContext {
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

    #[test]
    fn dummy_context_bus_round_trip() {
        let ctx = DummyContext::new(FrameId::new(10)).with_bus(
            "speed",
            LpsValueF32::F32(3.5),
            FrameId::new(5),
        );

        let (val, frame) = ctx.bus_value(&ChannelName(String::from("speed"))).unwrap();
        assert!(matches!(val, LpsValueF32::F32(3.5)));
        assert_eq!(frame.as_i64(), 5);
    }

    #[test]
    fn dummy_context_missing_bus_returns_none() {
        let ctx = DummyContext::new(FrameId::new(10));
        assert!(
            ctx.bus_value(&ChannelName(String::from("missing")))
                .is_none()
        );
    }

    #[test]
    fn dummy_context_target_prop_round_trip() {
        let ctx = DummyContext::new(FrameId::new(10)).with_target(
            "/show.test/node1.thing",
            "outputs[0]",
            LpsValueF32::F32(1.5),
            FrameId::new(7),
        );

        let path = TreePath::parse("/show.test/node1.thing").unwrap();
        let prop = parse_path("outputs[0]").unwrap();
        let (val, frame) = ctx.target_prop(&path, &prop).unwrap();
        assert!(matches!(val, LpsValueF32::F32(1.5)));
        assert_eq!(frame.as_i64(), 7);
    }

    #[test]
    fn dummy_context_missing_target_returns_none() {
        let ctx = DummyContext::new(FrameId::new(10));
        let path = TreePath::parse("/show.test/node1.thing").unwrap();
        let prop = parse_path("outputs[0]").unwrap();
        assert!(ctx.target_prop(&path, &prop).is_none());
    }

    #[test]
    fn dummy_context_binding_round_trip() {
        let binding = SrcBinding::Bus(ChannelName(String::from("test")));
        let ctx = DummyContext::new(FrameId::new(10)).with_binding("params.speed", binding.clone());

        let prop = parse_path("params.speed").unwrap();
        assert_eq!(ctx.artifact_binding(&prop), Some(binding));
    }

    #[test]
    fn dummy_context_default_round_trip() {
        let ctx =
            DummyContext::new(FrameId::new(10)).with_default("params.scale", LpsValueF32::F32(2.0));

        let prop = parse_path("params.scale").unwrap();
        let val = ctx.artifact_default(&prop).unwrap();
        assert!(matches!(val, LpsValueF32::F32(2.0)));
    }

    #[test]
    fn dummy_context_frame_id() {
        let ctx = DummyContext::new(FrameId::new(42));
        assert_eq!(ctx.frame_id().as_i64(), 42);
    }
}
