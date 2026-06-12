//! Effective project graph node identity.

use alloc::vec::Vec;

use crate::SlotPath;

/// Deterministic identity for one effective project node instance.
///
/// A project node key is authored-topology identity, not runtime identity. The
/// root key has no segments; child keys append the authored invocation slot path
/// at each parent step.
#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ProjectNodeLocation {
    pub segments: Vec<LocationSeg>,
}

impl ProjectNodeLocation {
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn child(&self, slot: SlotPath) -> Self {
        let mut segments = self.segments.clone();
        segments.push(LocationSeg { slot });
        Self { segments }
    }

    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }
}

/// One authored invocation step in a [`ProjectNodeLocation`].
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct LocationSeg {
    pub slot: SlotPath,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn root_key_is_empty() {
        let key = ProjectNodeLocation::root();

        assert!(key.is_root());
        assert!(key.segments.is_empty());
    }

    #[test]
    fn child_key_appends_slot_ancestry() {
        let root = ProjectNodeLocation::root();
        let first = root.child(SlotPath::parse("nodes[playlist]").unwrap());
        let second = first.child(SlotPath::parse("entries[1].node").unwrap());

        assert_eq!(first.segments.len(), 1);
        assert_eq!(second.segments.len(), 2);
        assert_eq!(second.segments[0].slot.to_string(), "nodes[playlist]");
        assert_eq!(second.segments[1].slot.to_string(), "entries[1].node");
    }

    #[test]
    fn key_serializes_as_slot_path_segments() {
        let key = ProjectNodeLocation::root().child(SlotPath::parse("nodes[shader]").unwrap());

        let json = serde_json::to_string(&key).unwrap();
        let round_trip: ProjectNodeLocation = serde_json::from_str(&json).unwrap();

        assert_eq!(round_trip, key);
    }
}
