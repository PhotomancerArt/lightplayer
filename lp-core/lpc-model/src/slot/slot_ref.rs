use super::{SlotName, SlotOwner};

/// Reference to one slot owned by a node or bus.
///
/// This type does not include direction. Produced versus consumed is determined
/// by the operation being performed against the slot.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotRef {
    pub owner: SlotOwner,
    pub slot: SlotName,
}

impl SlotRef {
    pub fn new(owner: SlotOwner, slot: SlotName) -> Self {
        Self { owner, slot }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    #[test]
    fn slot_ref_contains_owner_and_slot_only() {
        let slot = SlotRef::new(
            SlotOwner::Node(NodeId::new(7)),
            SlotName::parse("output").unwrap(),
        );
        assert_eq!(slot.slot.as_str(), "output");
    }
}
