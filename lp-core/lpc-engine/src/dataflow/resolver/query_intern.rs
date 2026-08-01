//! Interning of [`QueryKey`]s to dense [`QueryId`]s.
//!
//! Resolution addresses slots by name: a `QueryKey` owns a `SlotPath` (a `Vec`
//! of heap `String` segments) or a `ChannelName`. Comparing those on every
//! cache probe means string compares, and storing them in every cache entry,
//! trace frame, and route means cloning heap data per frame.
//!
//! Interning pays that cost once per distinct query per structural epoch.
//! Afterwards a query is a `u32`: cache lookups index an array, the cycle
//! stack is `Copy`, and nothing allocates.
//!
//! Ids are meaningful only within one epoch. [`QueryInternTable::clear`] runs
//! when the graph changes shape, and every id handed out before it is void.

use alloc::vec::Vec;
use core::hash::{Hash, Hasher};
use lpc_model::SlotPath;

use crate::dataflow::resolver::query_key::QueryKey;

/// A [`QueryKey`] reduced to an index, valid for one structural epoch.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueryId(u32);

impl QueryId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// FNV-1a. Small, no dependency, and good enough for a lookup filter — the
/// table verifies every candidate with `==`, so a collision costs a compare,
/// never a wrong answer.
#[derive(Default)]
struct FnvHasher(u64);

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut hash = if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        };
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        self.0 = hash;
    }
}

/// Hash the parts of a key that are cheap and highly discriminating.
///
/// Deliberately *not* the whole key: a `SlotAccessor` carries compiled steps
/// that add nothing two accessors with the same node and path would not
/// already differ on in practice. Equality still decides.
fn hash_query(query: &QueryKey) -> u64 {
    let mut hasher = FnvHasher::default();
    match query {
        QueryKey::Bus(channel) => {
            hasher.write_u8(0);
            channel.hash(&mut hasher);
        }
        QueryKey::ProducedSlot { node, slot } => {
            hasher.write_u8(1);
            node.hash(&mut hasher);
            hash_slot_path(slot, &mut hasher);
        }
        QueryKey::ConsumedSlot { node, slot } => {
            hasher.write_u8(2);
            node.hash(&mut hasher);
            hash_slot_path(slot, &mut hasher);
        }
        QueryKey::ConsumedSlotAccessor { node, accessor } => {
            hasher.write_u8(3);
            node.hash(&mut hasher);
            hash_slot_path(accessor.path(), &mut hasher);
        }
    }
    hasher.finish()
}

fn hash_slot_path(path: &SlotPath, hasher: &mut impl Hasher) {
    path.hash(hasher);
}

/// `QueryKey` ↔ [`QueryId`], scoped to a structural epoch.
#[derive(Clone, Debug, Default)]
pub struct QueryInternTable {
    /// Indexed by [`QueryId`]: the key it stands for.
    keys: Vec<QueryKey>,
    /// `(hash, id)` sorted by hash, so lookup is a binary search plus one
    /// equality check in the common case.
    by_hash: Vec<(u64, QueryId)>,
}

impl QueryInternTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Id for `query`, assigning one if this is the first time it is seen.
    pub fn intern(&mut self, query: &QueryKey) -> QueryId {
        let hash = hash_query(query);
        let start = self.by_hash.partition_point(|(h, _)| *h < hash);
        for (candidate_hash, id) in &self.by_hash[start..] {
            if *candidate_hash != hash {
                break;
            }
            if &self.keys[id.index()] == query {
                return *id;
            }
        }

        let id = QueryId(self.keys.len() as u32);
        self.keys.push(query.clone());
        self.by_hash.insert(start, (hash, id));
        id
    }

    /// Id for `query` if it has been interned, without assigning one.
    pub fn lookup(&self, query: &QueryKey) -> Option<QueryId> {
        let hash = hash_query(query);
        let start = self.by_hash.partition_point(|(h, _)| *h < hash);
        self.by_hash[start..]
            .iter()
            .take_while(|(candidate_hash, _)| *candidate_hash == hash)
            .find(|(_, id)| &self.keys[id.index()] == query)
            .map(|(_, id)| *id)
    }

    /// The key an id stands for. Cold path: error formatting and tracing.
    pub fn key(&self, id: QueryId) -> Option<&QueryKey> {
        self.keys.get(id.index())
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn clear(&mut self) {
        self.keys.clear();
        self.by_hash.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lpc_model::{ChannelName, NodeId};

    fn bus(name: &str) -> QueryKey {
        QueryKey::Bus(ChannelName(String::from(name)))
    }

    fn produced(node: u32, slot: &str) -> QueryKey {
        QueryKey::ProducedSlot {
            node: NodeId::new(node),
            slot: SlotPath::parse(slot).expect("slot path"),
        }
    }

    #[test]
    fn interning_is_stable_and_distinct() {
        let mut table = QueryInternTable::new();
        let a = table.intern(&bus("video"));
        let b = table.intern(&bus("audio"));
        let a_again = table.intern(&bus("video"));

        assert_eq!(a, a_again, "the same key must intern to the same id");
        assert_ne!(a, b);
        assert_eq!(table.len(), 2, "re-interning must not grow the table");
        assert_eq!(table.key(a), Some(&bus("video")));
    }

    #[test]
    fn keys_differing_only_by_variant_do_not_collide() {
        let mut table = QueryInternTable::new();
        let produced_id = table.intern(&produced(3, "out"));
        let consumed_id = table.intern(&QueryKey::ConsumedSlot {
            node: NodeId::new(3),
            slot: SlotPath::parse("out").expect("slot path"),
        });
        assert_ne!(produced_id, consumed_id);
    }

    #[test]
    fn lookup_finds_interned_and_misses_unknown() {
        let mut table = QueryInternTable::new();
        let id = table.intern(&produced(1, "color"));
        assert_eq!(table.lookup(&produced(1, "color")), Some(id));
        assert_eq!(table.lookup(&produced(2, "color")), None);
        assert_eq!(table.lookup(&produced(1, "other")), None);
    }

    #[test]
    fn clear_voids_every_id() {
        let mut table = QueryInternTable::new();
        table.intern(&bus("a"));
        table.intern(&bus("b"));
        table.clear();
        assert!(table.is_empty());
        assert_eq!(table.lookup(&bus("a")), None);
        // Ids restart, which is why they may not outlive an epoch.
        assert_eq!(table.intern(&bus("z")), QueryId(0));
    }

    #[test]
    fn many_keys_round_trip_through_the_hash_index() {
        let mut table = QueryInternTable::new();
        let mut ids = Vec::new();
        for node in 0..64u32 {
            ids.push(table.intern(&produced(node, "out")));
        }
        for (node, id) in ids.iter().enumerate() {
            assert_eq!(
                table.lookup(&produced(node as u32, "out")),
                Some(*id),
                "every interned key must be findable regardless of insertion order"
            );
        }
        assert_eq!(table.len(), 64);
    }
}
