use super::{SlotOwner, SlotPath};

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
    pub path: SlotPath,
}

impl SlotRef {
    pub fn new(owner: SlotOwner, path: SlotPath) -> Self {
        Self { owner, path }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;
    use alloc::string::ToString;

    #[test]
    fn slot_ref_contains_owner_and_path_only() {
        let slot = SlotRef::new(
            SlotOwner::Node(NodeId::new(7)),
            SlotPath::parse("state.output").unwrap(),
        );
        assert_eq!(slot.path.to_string(), "state.output");
    }
}
