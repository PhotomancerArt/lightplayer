//! Chunked hash map for allocation-sensitive contexts (e.g. embedded).
//!
//! Uses fixed 64 buckets, each backed by a ChunkedVec, to avoid large contiguous
//! hash table allocations that cause OOM on fragmented heaps. Allocates in small
//! chunks (16 entries per chunk) instead of one large SwissTable-style allocation.

use core::hash::{Hash, Hasher};
use rustc_hash::FxHasher;

use crate::chunked_vec::ChunkedVec;

/// Number of buckets. Fixed to avoid any resize allocation.
const NUM_BUCKETS: usize = 12;

/// Hash a key to a bucket index using FxHasher.
#[inline]
fn bucket_index<Q: Hash + ?Sized>(key: &Q) -> usize {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    (hasher.finish() as usize) % NUM_BUCKETS
}

/// A hash map backed by chunked allocations.
///
/// Uses 64 fixed buckets, each containing a ChunkedVec of (K, V) pairs.
/// Collisions are resolved by linear scan within the bucket. Lookup is O(n/64)
/// on average. No large contiguous allocation—each ChunkedVec grows in 16-entry
/// chunks.
#[derive(Debug)]
pub struct ChunkedHashMap<K, V> {
    buckets: [ChunkedVec<(K, V)>; NUM_BUCKETS],
    len: usize,
}

impl<K, V> Default for ChunkedHashMap<K, V> {
    fn default() -> Self {
        Self {
            buckets: core::array::from_fn(|_| ChunkedVec::new()),
            len: 0,
        }
    }
}

impl<K, V> ChunkedHashMap<K, V> {
    /// Create an empty ChunkedHashMap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with capacity hint. Currently a no-op; buckets allocate on demand.
    pub fn with_capacity(_capacity: usize) -> Self {
        Self::new()
    }

    /// Number of key-value pairs.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Inserts a key-value pair. Returns the previous value if the key existed.
    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        K: Eq + Hash,
    {
        let bi = bucket_index(&key);
        let bucket = &mut self.buckets[bi];
        for i in 0..bucket.len() {
            if bucket.get(i).map(|(k, _)| k == &key).unwrap_or(false) {
                let (_, v) = bucket.get_mut(i).unwrap();
                return Some(core::mem::replace(v, value));
            }
        }
        bucket.push((key, value));
        self.len += 1;
        None
    }

    /// Gets a reference to the value for the key.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Eq + Hash + core::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let bi = bucket_index(key);
        let bucket = &self.buckets[bi];
        for i in 0..bucket.len() {
            let (k, v) = bucket.get(i).unwrap();
            if k.borrow() == key {
                return Some(v);
            }
        }
        None
    }

    /// Gets a mutable reference to the value for the key.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Eq + Hash + core::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let bi = bucket_index(key);
        let bucket = &mut self.buckets[bi];
        let mut found = None;
        for i in 0..bucket.len() {
            let (k, _) = bucket.get(i).unwrap();
            if k.borrow() == key {
                found = Some(i);
                break;
            }
        }
        match found {
            Some(i) => {
                let (_, v) = bucket.get_mut(i).unwrap();
                Some(v)
            }
            None => None,
        }
    }

    /// Returns true if the map contains the key.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Eq + Hash + core::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(key).is_some()
    }

    /// Removes the key from the map, returning the previous value if present.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Eq + Hash + core::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let bi = bucket_index(key);
        let bucket = &mut self.buckets[bi];
        for i in 0..bucket.len() {
            let (k, _) = bucket.get(i).unwrap();
            if k.borrow() == key {
                let (_, v) = bucket.swap_remove(i).unwrap();
                self.len -= 1;
                return Some(v);
            }
        }
        None
    }

    /// Gets the entry for the given key.
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V>
    where
        K: Eq + Hash,
    {
        let bi = bucket_index(&key);
        let bucket = &mut self.buckets[bi];
        for i in 0..bucket.len() {
            let (k, _) = bucket.get(i).unwrap();
            if k == &key {
                return Entry::Occupied(OccupiedEntry {
                    map: self,
                    bucket_index: bi,
                    slot_index: i,
                });
            }
        }
        Entry::Vacant(VacantEntry {
            map: self,
            key,
            bucket_index: bi,
        })
    }

    /// Retains only the entries for which the predicate returns true.
    pub fn retain<F>(&mut self, mut f: F)
    where
        K: Eq + Hash,
        F: FnMut(&K, &mut V) -> bool,
    {
        for bucket in &mut self.buckets {
            let mut i = 0;
            while i < bucket.len() {
                let keep = {
                    let (k, v) = bucket.get_mut(i).unwrap();
                    f(k, v)
                };
                if !keep {
                    let _ = bucket.swap_remove(i);
                    self.len -= 1;
                } else {
                    i += 1;
                }
            }
        }
    }

    /// Returns an iterator over (key, value) references.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.buckets.iter().flat_map(|b| {
            (0..b.len()).map(move |i| {
                let (k, v) = b.get(i).unwrap();
                (k, v)
            })
        })
    }
}

/// A view into a single entry in a map, which may be vacant or occupied.
pub enum Entry<'a, K, V> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, K, V>),
    /// A vacant entry.
    Vacant(VacantEntry<'a, K, V>),
}

impl<'a, K: Eq + Hash, V> Entry<'a, K, V> {
    /// Ensures a value is in the entry by inserting the default if vacant.
    pub fn or_insert(self, default: V) -> &'a mut V {
        match self {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => v.insert(default),
        }
    }

    /// Ensures a value is in the entry by inserting the result of the closure if vacant.
    pub fn or_insert_with<F>(self, default: F) -> &'a mut V
    where
        F: FnOnce() -> V,
    {
        match self {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => v.insert(default()),
        }
    }
}

/// A view into an occupied entry in a ChunkedHashMap.
pub struct OccupiedEntry<'a, K, V> {
    map: &'a mut ChunkedHashMap<K, V>,
    bucket_index: usize,
    slot_index: usize,
}

impl<'a, K, V> OccupiedEntry<'a, K, V> {
    /// Gets a reference to the value.
    pub fn get(&self) -> &V {
        let (_, v) = self.map.buckets[self.bucket_index]
            .get(self.slot_index)
            .unwrap();
        v
    }

    /// Gets a mutable reference to the value.
    pub fn get_mut(&mut self) -> &mut V {
        let (_, v) = self.map.buckets[self.bucket_index]
            .get_mut(self.slot_index)
            .unwrap();
        v
    }

    /// Consumes the entry and returns a mutable reference to the value.
    /// Used by or_insert to return a ref with the map's lifetime.
    fn into_mut(self) -> &'a mut V {
        let (_, v) = self.map.buckets[self.bucket_index]
            .get_mut(self.slot_index)
            .unwrap();
        v
    }

    /// Sets the value of the entry, returning the old value.
    pub fn insert(&mut self, value: V) -> V {
        let (_, v) = self.map.buckets[self.bucket_index]
            .get_mut(self.slot_index)
            .unwrap();
        core::mem::replace(v, value)
    }
}

/// A view into a vacant entry in a ChunkedHashMap.
pub struct VacantEntry<'a, K, V> {
    map: &'a mut ChunkedHashMap<K, V>,
    key: K,
    bucket_index: usize,
}

impl<'a, K, V> VacantEntry<'a, K, V> {
    /// Sets the value of the entry with the vacant entry's key.
    pub fn insert(self, value: V) -> &'a mut V {
        self.map.buckets[self.bucket_index].push((self.key, value));
        self.map.len += 1;
        let (_, v) = self.map.buckets[self.bucket_index]
            .get_mut(self.map.buckets[self.bucket_index].len() - 1)
            .unwrap();
        v
    }

    /// Sets the value of the entry with the vacant entry's key, or inserts with default.
    pub fn or_insert(self, default: V) -> &'a mut V
    where
        K: Eq + Hash,
    {
        self.insert(default)
    }

    /// Sets the value of the entry with the vacant entry's key, or inserts with the result of the closure.
    pub fn or_insert_with<F>(self, default: F) -> &'a mut V
    where
        K: Eq + Hash,
        F: FnOnce() -> V,
    {
        self.insert(default())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    fn chunked_hashmap<K: Hash + Eq, V>() -> ChunkedHashMap<K, V> {
        ChunkedHashMap::new()
    }

    #[test]
    fn empty_len() {
        let m: ChunkedHashMap<u32, i32> = chunked_hashmap();
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn default_empty() {
        let m: ChunkedHashMap<u32, i32> = ChunkedHashMap::default();
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn insert_and_get() {
        let mut m = chunked_hashmap();
        assert_eq!(m.get(&1), None);
        m.insert(1, 10);
        assert_eq!(m.get(&1), Some(&10));
        assert_eq!(m.get(&2), None);
    }

    #[test]
    fn insert_overwrite() {
        let mut m = chunked_hashmap();
        assert_eq!(m.insert(1, 10), None);
        assert_eq!(m.insert(1, 20), Some(10));
        assert_eq!(m.get(&1), Some(&20));
    }

    #[test]
    fn contains_key() {
        let mut m = chunked_hashmap();
        assert!(!m.contains_key(&1));
        m.insert(1, 10);
        assert!(m.contains_key(&1));
        assert!(!m.contains_key(&2));
    }

    #[test]
    fn entry_vacant_insert() {
        let mut m = chunked_hashmap();
        m.entry(1).or_insert(10);
        assert_eq!(m.get(&1), Some(&10));
    }

    #[test]
    fn entry_vacant_or_insert_with() {
        let mut m = chunked_hashmap();
        m.entry(1).or_insert_with(|| 42);
        assert_eq!(m.get(&1), Some(&42));
    }

    #[test]
    fn entry_occupied_get() {
        let mut m = chunked_hashmap();
        m.insert(1, 10);
        match m.entry(1) {
            Entry::Occupied(o) => assert_eq!(*o.get(), 10),
            Entry::Vacant(_) => panic!("expected occupied"),
        }
    }

    #[test]
    fn entry_occupied_insert() {
        let mut m = chunked_hashmap();
        m.insert(1, 10);
        match m.entry(1) {
            Entry::Occupied(mut o) => {
                assert_eq!(o.insert(20), 10);
            }
            Entry::Vacant(_) => panic!("expected occupied"),
        }
        assert_eq!(m.get(&1), Some(&20));
    }

    #[test]
    fn iter_yields_all_pairs() {
        let mut m = chunked_hashmap();
        m.insert(1, 10);
        m.insert(2, 20);
        m.insert(3, 30);
        let mut pairs: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
        pairs.sort_by_key(|(k, _)| *k);
        assert_eq!(pairs, [(1, 10), (2, 20), (3, 30)]);
    }

    #[test]
    fn retain_removes_matching() {
        let mut m = chunked_hashmap();
        m.insert(1, 10);
        m.insert(2, 20);
        m.insert(3, 30);
        m.retain(|k, _| *k != 2);
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(&1), Some(&10));
        assert_eq!(m.get(&2), None);
        assert_eq!(m.get(&3), Some(&30));
    }

    #[test]
    fn many_insertions_no_rehash() {
        let mut m = chunked_hashmap();
        for i in 0..600 {
            m.insert(i, i * 2);
        }
        assert_eq!(m.len(), 600);
        for i in 0..600 {
            assert_eq!(m.get(&i), Some(&(i * 2)));
        }
    }

    #[test]
    fn collisions_same_bucket() {
        let mut m: ChunkedHashMap<u32, u32> = chunked_hashmap();
        for i in 0..200 {
            let k = i * 64;
            m.insert(k, i);
        }
        assert_eq!(m.len(), 200);
        for i in 0..200 {
            assert_eq!(m.get(&(i * 64)), Some(&i));
        }
    }

    #[test]
    fn string_keys() {
        let mut m: ChunkedHashMap<alloc::string::String, i32> = chunked_hashmap();
        m.insert(alloc::string::String::from("foo"), 1);
        m.insert(alloc::string::String::from("bar"), 2);
        assert_eq!(m.get("foo"), Some(&1));
        assert_eq!(m.get("bar"), Some(&2));
    }

    #[test]
    fn tuple_keys() {
        let mut m: ChunkedHashMap<(i32, i32), i32> = chunked_hashmap();
        m.insert((1, 2), 10);
        m.insert((3, 4), 20);
        assert_eq!(m.get(&(1, 2)), Some(&10));
        assert_eq!(m.get(&(3, 4)), Some(&20));
    }

    #[test]
    fn drop_cleans_up() {
        let mut m: ChunkedHashMap<u32, Vec<u8>> = chunked_hashmap();
        for i in 0..300 {
            m.insert(i, (0..100).collect());
        }
        drop(m);
    }
}

// --- ChunkedHashSet ---

/// Hash set backed by ChunkedHashMap; no large contiguous allocations.
#[derive(Debug)]
pub struct ChunkedHashSet<K> {
    map: ChunkedHashMap<K, ()>,
}

impl<K> Default for ChunkedHashSet<K> {
    fn default() -> Self {
        Self {
            map: ChunkedHashMap::default(),
        }
    }
}

impl<K> ChunkedHashSet<K> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: K) -> bool
    where
        K: Eq + Hash,
    {
        self.map.insert(key, ()).is_none()
    }

    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Eq + Hash + core::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.map.iter().map(|(k, _)| k)
    }
}

impl<K, const N: usize> From<[K; N]> for ChunkedHashSet<K>
where
    K: Eq + Hash,
{
    fn from(arr: [K; N]) -> Self {
        let mut set = ChunkedHashSet::new();
        for k in arr {
            set.insert(k);
        }
        set
    }
}

impl<K, I> core::iter::FromIterator<I> for ChunkedHashSet<K>
where
    K: Eq + Hash,
    I: Into<K>,
{
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        let mut set = ChunkedHashSet::new();
        for k in iter {
            set.insert(k.into());
        }
        set
    }
}

#[cfg(test)]
mod set_tests {
    use super::*;

    #[test]
    fn set_insert_contains() {
        let mut s: ChunkedHashSet<u32> = ChunkedHashSet::new();
        assert!(!s.contains(&1));
        s.insert(1);
        assert!(s.contains(&1));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn set_from_iter() {
        let s: ChunkedHashSet<i32> = [1, 2, 3, 2, 1].into_iter().collect();
        assert_eq!(s.len(), 3);
        assert!(s.contains(&1));
        assert!(s.contains(&2));
        assert!(s.contains(&3));
    }
}
