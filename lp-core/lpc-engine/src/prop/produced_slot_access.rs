//! Node-owned produced slot access.
//!
//! This is the runtime surface for data a node produces.
//!
//! Slot keys are still represented as [`ValuePath`] values in this transitional
//! resolver path. New slot-model work should not copy that choice: produced
//! and consumed endpoints should converge on [`lpc_model::SlotPath`] so the
//! runtime does not keep treating value projection paths as slot identity.

use alloc::boxed::Box;

use lpc_model::{FrameId, ValuePath};

use crate::runtime_product::RuntimeProduct;

/// One produced slot value and the frame when it last changed.
pub type ProducedSlotEntry = (ValuePath, RuntimeProduct, FrameId);

/// Access to the values produced by a runtime node.
pub trait ProducedSlotAccess {
    /// Get the current produced product at `path`, if any.
    fn get(&self, path: &ValuePath) -> Option<(RuntimeProduct, FrameId)>;

    /// Iterate produced slots whose `changed_frame > since`.
    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a>;

    /// All produced slots' current products and frames.
    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = ProducedSlotEntry> + 'a>;
}

/// Empty produced slot surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyProducedSlots;

impl ProducedSlotAccess for EmptyProducedSlots {
    fn get(&self, _path: &ValuePath) -> Option<(RuntimeProduct, FrameId)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: FrameId,
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
    use lpc_model::prop::value_path::parse_path;
    use lps_shared::LpsValueF32;

    #[derive(Default)]
    struct DummyProducedSlots {
        values: Vec<ProducedSlotEntry>,
    }

    impl ProducedSlotAccess for DummyProducedSlots {
        fn get(&self, path: &ValuePath) -> Option<(RuntimeProduct, FrameId)> {
            self.values
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, v, f)| (v.clone(), *f))
        }

        fn iter_changed_since<'a>(
            &'a self,
            since: FrameId,
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
        let path = parse_path("outputs.color").unwrap();
        slots.values.push((
            path.clone(),
            RuntimeProduct::try_value(LpsValueF32::F32(0.5)).unwrap(),
            FrameId::new(1),
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
        let path1 = parse_path("outputs.a").unwrap();
        let path2 = parse_path("outputs.b").unwrap();
        slots.values.push((
            path1,
            RuntimeProduct::try_value(LpsValueF32::F32(1.0)).unwrap(),
            FrameId::new(1),
        ));
        slots.values.push((
            path2.clone(),
            RuntimeProduct::try_value(LpsValueF32::F32(2.0)).unwrap(),
            FrameId::new(5),
        ));

        let changed: Vec<_> = slots.iter_changed_since(FrameId::new(2)).collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, path2);
    }

    #[test]
    fn snapshot_returns_all() {
        let mut slots = DummyProducedSlots::default();
        slots.values.push((
            parse_path("outputs.a").unwrap(),
            RuntimeProduct::try_value(LpsValueF32::F32(1.0)).unwrap(),
            FrameId::new(1),
        ));
        slots.values.push((
            parse_path("state.value").unwrap(),
            RuntimeProduct::try_value(LpsValueF32::I32(42)).unwrap(),
            FrameId::new(2),
        ));

        let snapshot: Vec<_> = slots.snapshot().collect();
        assert_eq!(snapshot.len(), 2);
    }
}
