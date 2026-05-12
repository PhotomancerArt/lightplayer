//! Cache and cycle-detection key for engine resolution.
//!
//! Produced and consumed endpoints use [`lpc_model::SlotPath`] because they
//! address slot identity, not projection inside a leaf value.

use lpc_model::{ChannelName, NodeId, SlotAccessor, SlotPath};

/// Demand/cache key for one resolved value in the engine resolver.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryKey {
    Bus(ChannelName),
    ProducedSlot {
        node: NodeId,
        slot: SlotPath,
    },
    ConsumedSlot {
        node: NodeId,
        slot: SlotPath,
    },
    ConsumedSlotAccessor {
        node: NodeId,
        accessor: SlotAccessor,
    },
}

impl QueryKey {
    pub fn consumed_slot_path(&self) -> Option<&SlotPath> {
        match self {
            Self::ConsumedSlot { slot, .. } => Some(slot),
            Self::ConsumedSlotAccessor { accessor, .. } => Some(accessor.path()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QueryKey;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::ChannelName;
    use lpc_model::NodeId;
    use lpc_model::SlotPath;

    #[test]
    fn query_key_works_as_btree_map_key() {
        let mut m = BTreeMap::new();
        let k1 = QueryKey::Bus(ChannelName(String::from("a")));
        let k2 = QueryKey::Bus(ChannelName(String::from("b")));
        m.insert(k1.clone(), 1u32);
        m.insert(
            QueryKey::ProducedSlot {
                node: NodeId::new(0),
                slot: SlotPath::parse("out").unwrap(),
            },
            2,
        );
        m.insert(
            QueryKey::ConsumedSlot {
                node: NodeId::new(1),
                slot: SlotPath::parse("in").unwrap(),
            },
            4,
        );
        m.insert(k2.clone(), 3);

        let keys: Vec<_> = m.keys().cloned().collect();
        assert_eq!(keys.len(), 4);
        assert_eq!(m.get(&k1), Some(&1));
        assert_eq!(m.get(&k2), Some(&3));
    }
}
