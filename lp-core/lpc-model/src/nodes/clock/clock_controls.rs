use serde::{Deserialize, Serialize};

use alloc::vec;

use crate::{
    FieldSlot, LpType, OrderedF32, Revision, SlotDataAccess, SlotMeta, SlotPolicy,
    SlotRecordAccess, SlotShape, SlotShapeId, SlotValueShape, ValueEditorHint, ValueSlot,
};

/// Transient user controls for the project clock.
///
/// Clock controls live in authored node-def slot data so the UI can mutate them
/// through the same path as ordinary config. Their slot policy marks them as
/// writable and transient: they are runtime controls, not durable defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockControls {
    #[serde(default = "default_running")]
    pub running: ValueSlot<bool>,
    #[serde(default = "default_rate")]
    pub rate: ValueSlot<f32>,
    #[serde(default)]
    pub scrub_offset_seconds: ValueSlot<f32>,
}

impl Default for ClockControls {
    fn default() -> Self {
        Self {
            running: default_running(),
            rate: default_rate(),
            scrub_offset_seconds: ValueSlot::new(0.0),
        }
    }
}

impl FieldSlot for ClockControls {
    fn slot_field_shape() -> SlotShape {
        SlotShape::Record {
            meta: SlotMeta::empty(),
            fields: vec![
                crate::slot::shape::field_with_policy(
                    "running",
                    ValueSlot::<bool>::slot_field_shape(),
                    SlotPolicy::writable_transient(),
                ),
                crate::slot::shape::field_with_policy(
                    "rate",
                    SlotShape::leaf(clock_rate_shape()),
                    SlotPolicy::writable_transient(),
                ),
                crate::slot::shape::field_with_policy(
                    "scrub_offset_seconds",
                    SlotShape::leaf(clock_scrub_offset_shape()),
                    SlotPolicy::writable_transient(),
                ),
            ],
        }
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotRecordAccess for ClockControls {
    fn fields_revision(&self) -> Revision {
        Revision::default()
    }

    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(self.running.slot_field_data()),
            1 => Some(self.rate.slot_field_data()),
            2 => Some(self.scrub_offset_seconds.slot_field_data()),
            _ => None,
        }
    }
}

fn default_running() -> ValueSlot<bool> {
    ValueSlot::new(true)
}

fn default_rate() -> ValueSlot<f32> {
    ValueSlot::new(1.0)
}

fn clock_rate_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("lp::clock::Rate"),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Slider {
            min: OrderedF32(0.0),
            max: OrderedF32(4.0),
            step: Some(OrderedF32(0.05)),
        },
    }
}

fn clock_scrub_offset_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("lp::clock::ScrubOffsetSeconds"),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Slider {
            min: OrderedF32(-10.0),
            max: OrderedF32(10.0),
            step: Some(OrderedF32(0.05)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SlotPersistence;

    #[test]
    fn clock_controls_fields_are_writable_transient() {
        let SlotShape::Record { fields, .. } = ClockControls::slot_field_shape() else {
            panic!("record shape");
        };
        assert_eq!(fields.len(), 3);
        for field in fields {
            assert!(field.policy.writable);
            assert_eq!(field.policy.persistence, SlotPersistence::Transient);
        }
    }
}
