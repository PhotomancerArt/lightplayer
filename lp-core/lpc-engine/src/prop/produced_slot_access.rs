//! Node-owned produced slot access.
//!
//! This is the runtime surface for data a node produces.
//!
//! Produced slot identity uses [`lpc_model::SlotPath`]. Projection inside a
//! produced value belongs to [`lpc_model::ValuePath`] at a higher layer.

use alloc::boxed::Box;

use lpc_model::{Revision, SlotPath};

use crate::runtime_product::RuntimeProduct;

/// One produced slot value and the frame when it last changed.
pub type ProducedSlotEntry = (SlotPath, RuntimeProduct, Revision);

/// Access to the values produced by a runtime node.
pub trait ProducedSlotAccess {
    /// Get the current produced product at `path`, if any.
    fn get(&self, path: &SlotPath) -> Option<(RuntimeProduct, Revision)>;

    /// Iterate produced slots whose `changed_frame > since`.
    fn iter_changed_since<'a>(
        &'a self,
        since: Revision,
    ) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a>;

    /// All produced slots' current products and frames.
    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a>;
}

/// Empty produced slot surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyProducedSlots;

impl ProducedSlotAccess for EmptyProducedSlots {
    fn get(&self, _path: &SlotPath) -> Option<(RuntimeProduct, Revision)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: Revision,
    ) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a> {
        Box::new(core::iter::empty())
    }
}

pub const EMPTY_PRODUCED_SLOTS: EmptyProducedSlots = EmptyProducedSlots;

/// Reserved for opaque runtime state snapshots (sync/debug tooling).
pub trait RuntimeStateAccess {}

/// No runtime state surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyRuntimeState;

impl RuntimeStateAccess for EmptyRuntimeState {}

pub const EMPTY_RUNTIME_STATE: EmptyRuntimeState = EmptyRuntimeState;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use lpc_model::SlotPath;
    use lps_shared::LpsValueF32;

    #[derive(Default)]
    struct DummyProducedSlots {
        values: Vec<ProducedSlotEntry>,
    }

    impl ProducedSlotAccess for DummyProducedSlots {
        fn get(&self, path: &SlotPath) -> Option<(RuntimeProduct, Revision)> {
            self.values
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, v, f)| (v.clone(), *f))
        }

        fn iter_changed_since<'a>(
            &'a self,
            since: Revision,
        ) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a> {
            Box::new(
                self.values
                    .iter()
                    .filter(move |(_, _, frame)| frame.as_i64() > since.as_i64())
                    .map(|(p, v, f)| (p.clone(), v.clone(), *f)),
            )
        }

        fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a> {
            Box::new(
                self.values
                    .iter()
                    .map(|(p, v, f)| (p.clone(), v.clone(), *f)),
            )
        }
    }

    #[test]
    fn produced_slot_access_is_object_safe() {
        let _: Box<dyn ProducedSlotAccess> = Box::new(DummyProducedSlots::default());
    }

    #[test]
    fn get_finds_existing_path() {
        let mut slots = DummyProducedSlots::default();
        let path = SlotPath::parse("outputs.color").unwrap();
        slots.values.push((
            path.clone(),
            RuntimeProduct::try_value(LpsValueF32::F32(0.5)).unwrap(),
            Revision::new(1),
        ));

        let result = slots.get(&path);
        assert!(matches!(
            result,
            Some((RuntimeProduct::Value(LpsValueF32::F32(0.5)), _))
        ));
    }

    #[test]
    fn iter_changed_since_filters_by_frame() {
        let mut slots = DummyProducedSlots::default();
        let path1 = SlotPath::parse("outputs.a").unwrap();
        let path2 = SlotPath::parse("outputs.b").unwrap();
        slots.values.push((
            path1,
            RuntimeProduct::try_value(LpsValueF32::F32(1.0)).unwrap(),
            Revision::new(1),
        ));
        slots.values.push((
            path2.clone(),
            RuntimeProduct::try_value(LpsValueF32::F32(2.0)).unwrap(),
            Revision::new(5),
        ));

        let changed: Vec<_> = slots.iter_changed_since(Revision::new(2)).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, path2);
    }

    #[test]
    fn snapshot_returns_all() {
        let mut slots = DummyProducedSlots::default();
        slots.values.push((
            SlotPath::parse("outputs.a").unwrap(),
            RuntimeProduct::try_value(LpsValueF32::F32(1.0)).unwrap(),
            Revision::new(1),
        ));
        slots.values.push((
            SlotPath::parse("state.value").unwrap(),
            RuntimeProduct::try_value(LpsValueF32::I32(42)).unwrap(),
            Revision::new(2),
        ));

        let snapshot: Vec<_> = slots.snapshot().collect();
        assert_eq!(snapshot.len(), 2);
    }
}
