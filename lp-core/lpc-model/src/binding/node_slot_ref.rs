use crate::{RelativeNodeRef, RelativeNodeRefError, SlotPath, SlotPathError};
use alloc::string::{String, ToString};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Parsed reference to a slot on another node.
///
/// URI-style authored form: the `node:` scheme, a filesystem-style relative
/// node path, then `#` addressing the slot within that node (a dotted field
/// path, as slots read everywhere else):
///
/// ```text
/// node:../shader#output
/// node:..#entry_time
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeSlotRef {
    node: RelativeNodeRef,
    slot: SlotPath,
}

impl NodeSlotRef {
    pub const PREFIX: &'static str = "node:";

    pub fn new(node: RelativeNodeRef, slot: SlotPath) -> Self {
        Self { node, slot }
    }

    pub fn parse(input: &str) -> Result<Self, NodeSlotRefError> {
        let Some(rest) = input.strip_prefix(Self::PREFIX) else {
            return Err(NodeSlotRefError::MissingPrefix);
        };
        let Some((node, slot)) = rest.split_once('#') else {
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
        write!(f, "{}{}#{}", Self::PREFIX, self.node, self.slot)
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
    MissingPrefix,
    MissingSeparator,
    MissingSlot,
    InvalidNode(RelativeNodeRefError),
    InvalidSlot(SlotPathError),
}

impl fmt::Display for NodeSlotRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => f.write_str("node slot ref must start with `node:`"),
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
        let parsed = NodeSlotRef::parse("node:../shader#output").unwrap();
        assert_eq!(parsed.node().to_string(), "../shader");
        assert_eq!(parsed.slot().to_string(), "output");
        assert_eq!(parsed.to_string(), "node:../shader#output");

        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, r#""node:../shader#output""#);
        let back: NodeSlotRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parsed);
    }

    #[test]
    fn parses_parent_slot_ref() {
        let parsed = NodeSlotRef::parse("node:..#entry_time").unwrap();
        assert_eq!(parsed.node().to_string(), "..");
        assert_eq!(parsed.slot().to_string(), "entry_time");
        assert_eq!(parsed.to_string(), "node:..#entry_time");
    }

    #[test]
    fn rejects_refs_without_prefix_or_slots() {
        assert_eq!(
            NodeSlotRef::parse("../shader#output"),
            Err(NodeSlotRefError::MissingPrefix)
        );
        assert_eq!(
            NodeSlotRef::parse("..shader#output"),
            Err(NodeSlotRefError::MissingPrefix)
        );
        assert_eq!(
            NodeSlotRef::parse("node:../shader"),
            Err(NodeSlotRefError::MissingSeparator)
        );
        assert_eq!(
            NodeSlotRef::parse("node:../shader#"),
            Err(NodeSlotRefError::MissingSlot)
        );
    }
}
