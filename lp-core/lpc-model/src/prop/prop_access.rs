//! Object-safe reflection over a node's *produced* fields
//! (outputs and state).
//!
//! Returned by `Node::props()` (the `Node` trait lands in M4.3).
//! Used by the sync layer to walk produced values and emit
//! per-prop deltas (M4.4).
//!
//! `*Props` structs implement this trait; the M4.3 derive macro
//! will emit standard impls. M4.2 callers can implement it by
//! hand for tests.

use crate::LpsValue;
use crate::project::FrameId;
use crate::prop::prop_path::PropPath;
use alloc::boxed::Box;

/// Object-safe reflection over a node's *produced* fields
/// (outputs and state).
///
/// Returned by `Node::props()` (the `Node` trait lands in M4.3).
/// Used by the sync layer to walk produced values and emit
/// per-prop deltas (M4.4).
///
/// `*Props` structs implement this trait; the M4.3 derive macro
/// will emit standard impls. M4.2 callers can implement it by
/// hand for tests.
pub trait PropAccess {
    /// Get the current value at `path`, if any.
    /// `LpsValue`-typed (structural); the impl's typed fields are
    /// an internal optimisation invisible at this layer.
    fn get(&self, path: &PropPath) -> Option<LpsValue>;

    /// Iterate produced fields whose `changed_frame > since`.
    /// The diff source for sync.
    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + 'a>;

    /// All produced fields' current values + frames. The cold-start
    /// path on first connect or detail-request.
    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + 'a>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Hand-rolled impl to prove object-safety.
    #[derive(Default)]
    struct DummyProps {
        values: Vec<(PropPath, LpsValue, FrameId)>,
    }

    impl PropAccess for DummyProps {
        fn get(&self, path: &PropPath) -> Option<LpsValue> {
            self.values
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, v, _)| v.clone())
        }

        fn iter_changed_since<'a>(
            &'a self,
            since: FrameId,
        ) -> Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + 'a> {
            Box::new(
                self.values
                    .iter()
                    .filter(move |(_, _, frame)| frame.as_i64() > since.as_i64())
                    .cloned(),
            )
        }

        fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + 'a> {
            Box::new(self.values.iter().cloned())
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
    fn prop_access_is_object_safe() {
        let _: Box<dyn PropAccess> = Box::new(DummyProps::default());
    }

    #[test]
    fn prop_access_get_finds_existing_path() {
        use crate::prop::prop_path::parse_path;

        let mut props = DummyProps::default();
        let path = parse_path("outputs.color").unwrap();
        props
            .values
            .push((path.clone(), LpsValue::F32(0.5), FrameId::new(1)));

        let result = props.get(&path);
        assert!(matches!(result, Some(LpsValue::F32(0.5))));
    }

    #[test]
    fn prop_access_get_returns_none_for_missing_path() {
        use crate::prop::prop_path::parse_path;

        let props = DummyProps::default();
        let path = parse_path("outputs.missing").unwrap();

        assert!(props.get(&path).is_none());
    }

    #[test]
    fn prop_access_iter_changed_since_filters_by_frame() {
        use crate::prop::prop_path::parse_path;

        let mut props = DummyProps::default();
        let path1 = parse_path("outputs.a").unwrap();
        let path2 = parse_path("outputs.b").unwrap();
        props
            .values
            .push((path1.clone(), LpsValue::F32(1.0), FrameId::new(1)));
        props
            .values
            .push((path2.clone(), LpsValue::F32(2.0), FrameId::new(5)));

        let since = FrameId::new(2);
        let changed: Vec<_> = props.iter_changed_since(since).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, path2);
    }

    #[test]
    fn prop_access_snapshot_returns_all() {
        use crate::prop::prop_path::parse_path;

        let mut props = DummyProps::default();
        let path1 = parse_path("outputs.a").unwrap();
        let path2 = parse_path("state.value").unwrap();
        props
            .values
            .push((path1.clone(), LpsValue::F32(1.0), FrameId::new(1)));
        props
            .values
            .push((path2.clone(), LpsValue::I32(42), FrameId::new(2)));

        let snapshot: Vec<_> = props.snapshot().collect();
        assert_eq!(snapshot.len(), 2);
    }
}
