use super::SlotRef;
use crate::ValuePath;

/// Reference to a nested value inside a slot.
///
/// [`SlotRef`] answers "which slot"; [`ValuePath`] answers "where inside that
/// slot's structured value".
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ValueRef {
    pub slot: SlotRef,
    pub path: ValuePath,
}

impl ValueRef {
    pub fn new(slot: SlotRef, path: ValuePath) -> Self {
        Self { slot, path }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prop::value_path::parse_path;
    use crate::{NodeId, SlotName, SlotOwner};

    #[test]
    fn value_ref_combines_slot_and_value_path() {
        let slot = SlotRef::new(
            SlotOwner::Node(NodeId::new(2)),
            SlotName::parse("output").unwrap(),
        );
        let reference = ValueRef::new(slot, parse_path("image.width").unwrap());
        assert_eq!(reference.path.len(), 2);
    }
}
