use crate::{ChannelName, NodeId};

/// Entity that owns a slot namespace.
///
/// Nodes and buses both expose named slots, but a bus is not a node: it has
/// routing semantics rather than node lifecycle or tick behavior.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum SlotOwner {
    Node(NodeId),
    Bus(ChannelName),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn slot_owner_distinguishes_node_and_bus() {
        assert_ne!(
            SlotOwner::Node(NodeId::new(1)),
            SlotOwner::Bus(ChannelName(String::from("main"))),
        );
    }
}
