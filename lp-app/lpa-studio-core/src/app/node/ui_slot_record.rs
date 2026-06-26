//! Record-shaped config slot data.

use crate::UiConfigSlot;

/// A structured slot value rendered as nested config rows.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSlotRecord {
    /// Ordered child fields projected from a structured slot.
    pub fields: Vec<UiConfigSlot>,
}

impl UiSlotRecord {
    /// Create a record from ordered child fields.
    pub fn new(fields: Vec<UiConfigSlot>) -> Self {
        Self { fields }
    }

    /// Returns true when the record has no visible fields.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

impl From<Vec<UiConfigSlot>> for UiSlotRecord {
    fn from(fields: Vec<UiConfigSlot>) -> Self {
        Self::new(fields)
    }
}

#[cfg(test)]
mod tests {
    use crate::{UiConfigSlot, UiSlotRecord, UiSlotValue};

    #[test]
    fn reports_empty_records() {
        assert!(UiSlotRecord::new(Vec::new()).is_empty());
    }

    #[test]
    fn keeps_field_order() {
        let record = UiSlotRecord::new(vec![
            UiConfigSlot::value("time", "Time", UiSlotValue::f32(1.0)),
            UiConfigSlot::value("trigger", "Trigger", UiSlotValue::bool(false)),
        ]);

        assert_eq!(record.fields[1].key, "trigger");
    }
}
