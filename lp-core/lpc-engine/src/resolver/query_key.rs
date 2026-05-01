//! Cache and cycle-detection key for engine resolution.

use lpc_model::{ChannelName, NodeId, PropPath};

/// Demand/cache key for one resolved value in the engine resolver.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryKey {
    Bus(ChannelName),
    NodeOutput { node: NodeId, output: PropPath },
    NodeInput { node: NodeId, input: PropPath },
}

#[cfg(test)]
mod tests {
    use super::QueryKey;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::ChannelName;
    use lpc_model::NodeId;
    use lpc_model::prop::prop_path::parse_path;

    #[test]
    fn query_key_works_as_btree_map_key() {
        let mut m = BTreeMap::new();
        let k1 = QueryKey::Bus(ChannelName(String::from("a")));
        let k2 = QueryKey::Bus(ChannelName(String::from("b")));
        m.insert(k1.clone(), 1u32);
        m.insert(
            QueryKey::NodeOutput {
                node: NodeId::new(0),
                output: parse_path("out").unwrap(),
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
