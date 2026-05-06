//! Static shape support for Rust-authored record structs.

use crate::SlotShape;

/// Static shape for a Rust-authored indexed record.
///
/// This is the non-root counterpart to [`crate::StaticSlotAccess`]. Inline
/// records can provide a shape without being independently registered in a
/// [`crate::SlotShapeRegistry`]. Root records can combine this trait with
/// `StaticSlotAccess` and register the returned shape under their root id.
pub trait SlotRecordShape {
    fn slot_record_shape() -> SlotShape;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModelType, SlotDataAccess, SlotRecordAccess, SlotValue,
        slot::shape::{field, record, value},
    };
    use alloc::vec;

    struct TestRecord {
        enabled: SlotValue<bool>,
    }

    impl SlotRecordShape for TestRecord {
        fn slot_record_shape() -> SlotShape {
            record(vec![field("enabled", value(ModelType::Bool))])
        }
    }

    impl SlotRecordAccess for TestRecord {
        fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
            match index {
                0 => Some(SlotDataAccess::Value(&self.enabled)),
                _ => None,
            }
        }
    }

    #[test]
    fn record_shape_matches_indexed_record_access() {
        let shape = TestRecord::slot_record_shape();
        let record = TestRecord {
            enabled: SlotValue::new(true),
        };

        let SlotShape::Record { fields, .. } = shape else {
            panic!("record shape");
        };
        assert_eq!(fields[0].name.as_str(), "enabled");
        assert!(matches!(record.field(0), Some(SlotDataAccess::Value(_))));
        assert!(record.field(1).is_none());
    }
}
