//! Temporary **scalar** reflection for bridge/sync: [`lps_shared::LpsValueF32`] fields only.
//!
//! Payloads here are **shader- and wire-compatible** [`lps_shared::LpsValueF32`];
//! sync paths use [`lpc_model::ModelValue`] via [`crate::wire_bridge`].
//!
//! This is legacy-shaped node “props” introspection, not the authoritative
//! output model. Non-scalar node products (e.g. [`crate::runtime_product::RuntimeProduct::Render`])
//! are exposed through [`crate::prop::RuntimeOutputAccess`] on [`crate::node::Node`].
//! Demand-driven resolution still flows through [`crate::resolver::production::Production`]
//! / [`crate::node::contexts::TickContext::resolve`].

use alloc::boxed::Box;

use lpc_model::project::FrameId;
use lpc_model::prop::PropPath;
use lps_shared::LpsValueF32;

/// Scalar / legacy reflection over fields exposed for wire and slot resolvers.
///
/// Implemented by runtime `*Props` structs; consumed by sync, the slot
/// resolver cascade ([`crate::resolver::resolver_context::ResolverContext`]), and
/// tooling before values cross the wire as [`lpc_model::ModelValue`].
///
/// See [`crate::prop::RuntimeOutputAccess`] for node-owned non-scalar products.
pub trait RuntimePropAccess {
    /// Get the current value at `path`, if any.
    fn get(&self, path: &PropPath) -> Option<(LpsValueF32, FrameId)>;

    /// Iterate produced fields whose `changed_frame > since`.
    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a>;

    /// All produced fields' current values and frames.
    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use lpc_model::prop::prop_path::parse_path;

    #[derive(Default)]
    struct DummyProps {
        values: Vec<(PropPath, LpsValueF32, FrameId)>,
    }

    impl RuntimePropAccess for DummyProps {
        fn get(&self, path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
            self.values
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, v, f)| (v.clone(), *f))
        }

        fn iter_changed_since<'a>(
            &'a self,
            since: FrameId,
        ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
            Box::new(
                self.values
                    .iter()
                    .filter(move |(_, _, frame)| frame.as_i64() > since.as_i64())
                    .map(|(p, v, f)| (p.clone(), v.clone(), *f)),
            )
        }

        fn snapshot<'a>(
            &'a self,
        ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
            Box::new(
                self.values
                    .iter()
                    .map(|(p, v, f)| (p.clone(), v.clone(), *f)),
            )
        }
    }

    impl Clone for DummyProps {
        fn clone(&self) -> Self {
            Self {
                values: self.values.clone(),
            }
        }
    }

    #[test]
    fn runtime_prop_access_is_object_safe() {
        let _: Box<dyn RuntimePropAccess> = Box::new(DummyProps::default());
    }

    #[test]
    fn get_finds_existing_path() {
        let mut props = DummyProps::default();
        let path = parse_path("outputs.color").unwrap();
        props
            .values
            .push((path.clone(), LpsValueF32::F32(0.5), FrameId::new(1)));

        let result = props.get(&path);
        assert!(matches!(result, Some((LpsValueF32::F32(0.5), _))));
    }

    #[test]
    fn get_returns_none_for_missing_path() {
        let props = DummyProps::default();
        let path = parse_path("outputs.missing").unwrap();
        assert!(props.get(&path).is_none());
    }

    #[test]
    fn iter_changed_since_filters_by_frame() {
        let mut props = DummyProps::default();
        let path1 = parse_path("outputs.a").unwrap();
        let path2 = parse_path("outputs.b").unwrap();
        props
            .values
            .push((path1.clone(), LpsValueF32::F32(1.0), FrameId::new(1)));
        props
            .values
            .push((path2.clone(), LpsValueF32::F32(2.0), FrameId::new(5)));

        let since = FrameId::new(2);
        let changed: Vec<_> = props.iter_changed_since(since).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, path2);
    }

    #[test]
    fn snapshot_returns_all() {
        let mut props = DummyProps::default();
        let path1 = parse_path("outputs.a").unwrap();
        let path2 = parse_path("state.value").unwrap();
        props
            .values
            .push((path1.clone(), LpsValueF32::F32(1.0), FrameId::new(1)));
        props
            .values
            .push((path2.clone(), LpsValueF32::I32(42), FrameId::new(2)));

        let snapshot: Vec<_> = props.snapshot().collect();
        assert_eq!(snapshot.len(), 2);
    }
}
