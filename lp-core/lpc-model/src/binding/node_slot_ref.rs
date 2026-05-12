use crate::{RelativeNodeRef, RelativeNodeRefError, SlotPath, SlotPathError};
use alloc::string::{String, ToString};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Parsed reference to a slot on another node.
///
/// The authored form combines a relative node ref with a slot path:
///
/// ```text
/// ..shader#output
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeSlotRef {
    node: RelativeNodeRef,
    slot: SlotPath,
}

impl NodeSlotRef {
    pub fn new(node: RelativeNodeRef, slot: SlotPath) -> Self {
        Self { node, slot }
    }

    pub fn parse(input: &str) -> Result<Self, NodeSlotRefError> {
        let Some((node, slot)) = input.split_once('#') else {
            return Err(NodeSlotRefError::MissingSeparator);
        };
        if slot.is_empty() {
            return Err(NodeSlotRefError::MissingSlot);
        }
        Ok(Self {
            node: RelativeNodeRef::parse(node).map_err(NodeSlotRefError::InvalidNode)?,
            slot: SlotPath::parse(slot).map_err(NodeSlotRefError::InvalidSlot)?,
        })
    }

    pub fn node(&self) -> &RelativeNodeRef {
        &self.node
    }

    pub fn slot(&self) -> &SlotPath {
        &self.slot
    }
}

impl fmt::Display for NodeSlotRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.node, self.slot)
    }
}

impl Serialize for NodeSlotRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NodeSlotRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a [`NodeSlotRef`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeSlotRefError {
    MissingSeparator,
    MissingSlot,
    InvalidNode(RelativeNodeRefError),
    InvalidSlot(SlotPathError),
}

impl fmt::Display for NodeSlotRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator => f.write_str("node slot ref is missing `#`"),
            Self::MissingSlot => f.write_str("node slot ref is missing a slot path"),
            Self::InvalidNode(err) => write!(f, "invalid node ref: {err}"),
            Self::InvalidSlot(err) => write!(f, "invalid node slot path: {err}"),
        }
    }
}

impl core::error::Error for NodeSlotRefError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn parses_and_round_trips_node_slot_refs() {
        let parsed = NodeSlotRef::parse("..shader#output").unwrap();
        assert_eq!(parsed.node().to_string(), "..shader");
        assert_eq!(parsed.slot().to_string(), "output");
        assert_eq!(parsed.to_string(), "..shader#output");

        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, r#""..shader#output""#);
        let back: NodeSlotRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parsed);
    }

    #[test]
    fn rejects_refs_without_slots() {
        assert_eq!(
            NodeSlotRef::parse("..shader"),
            Err(NodeSlotRefError::MissingSeparator)
        );
        assert_eq!(
            NodeSlotRef::parse("..shader#"),
            Err(NodeSlotRefError::MissingSlot)
        );
    }
}
