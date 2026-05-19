//! Minimal shader-compatible graph control message.

use crate::SlotValue;
use serde::{Deserialize, Serialize};

/// Native shape name used by authored shader slot definitions.
pub const CONTROL_MESSAGE_SHAPE_NAME: &str = "lp::control::Message";

/// One trigger/control event.
///
/// Collections of messages should be represented as `MapSlot<u32, ControlMessage>` with `id` as
/// the sentinel map key field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, SlotValue)]
#[slot_value(shape_id = "lp::control::Message")]
pub struct ControlMessage {
    pub id: u32,
    pub seq: u32,
}

impl ControlMessage {
    pub const fn new(id: u32, seq: u32) -> Self {
        Self { id, seq }
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub const fn seq(self) -> u32 {
        self.seq
    }
}

/// First semantic use of the control message envelope: a no-payload trigger.
pub type TriggerEvent = ControlMessage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromLpValue, LpValue, SlotShapeRegistry, StaticSlotShape, ToLpValue};

    #[test]
    fn control_message_round_trips_through_lp_value() {
        let message = ControlMessage::new(7, 42);

        assert_eq!(
            ControlMessage::from_lp_value(&message.to_lp_value()).unwrap(),
            message
        );
    }

    #[test]
    fn control_message_value_shape_has_minimal_fields() {
        let value = ControlMessage::new(7, 42).to_lp_value();

        let LpValue::Struct { name, fields } = value else {
            panic!("expected struct");
        };
        assert_eq!(name.as_deref(), Some("ControlMessage"));
        assert_eq!(fields.len(), 2);
        assert_eq!(
            fields[0],
            (alloc::string::String::from("id"), LpValue::U32(7))
        );
        assert_eq!(
            fields[1],
            (alloc::string::String::from("seq"), LpValue::U32(42))
        );
    }

    #[test]
    fn trigger_events_with_different_seq_are_distinct() {
        let first: TriggerEvent = ControlMessage::new(7, 1);
        let second: TriggerEvent = ControlMessage::new(7, 2);

        assert_ne!(first, second);
    }

    #[test]
    fn control_message_registers_native_shape_name() {
        let mut registry = SlotShapeRegistry::default();

        ControlMessage::ensure_registered(&mut registry).expect("registered");

        assert_eq!(
            registry
                .entry(&<ControlMessage as SlotValue>::SHAPE_ID)
                .and_then(|entry| entry.name()),
            Some(CONTROL_MESSAGE_SHAPE_NAME)
        );
    }
}
