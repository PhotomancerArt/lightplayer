//! Cache and cycle-detection key for engine resolution.
//!
//! Produced and consumed slots still use [`ValuePath`] in this transitional
//! resolver layer. The slot data model uses [`lpc_model::SlotPath`] for slot
//! identity, so this type should be converted before real runtime slot trees
//! become the primary node surface.

use lpc_model::{ChannelName, NodeId, ValuePath};

/// Demand/cache key for one resolved value in the engine resolver.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryKey {
    Bus(ChannelName),
    ProducedSlot { node: NodeId, slot: ValuePath },
    ConsumedSlot { node: NodeId, slot: ValuePath },
}

#[cfg(test)]
mod tests {
    use super::QueryKey;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::ChannelName;
    use lpc_model::NodeId;
    use lpc_model::prop::value_path::parse_path;

    #[test]
    fn query_key_works_as_btree_map_key() {
        let mut m = BTreeMap::new();
        let k1 = QueryKey::Bus(ChannelName(String::from("a")));
        let k2 = QueryKey::Bus(ChannelName(String::from("b")));
        m.insert(k1.clone(), 1u32);
        m.insert(
            QueryKey::ProducedSlot {
                node: NodeId::new(0),
                slot: parse_path("out").unwrap(),
            },
            2,
        );
        m.insert(k2.clone(), 3);

        let keys: Vec<_> = m.keys().cloned().collect();
        assert_eq!(keys.len(), 3);
        assert_eq!(m.get(&k1), Some(&1));
        assert_eq!(m.get(&k2), Some(&3));
    }
}
