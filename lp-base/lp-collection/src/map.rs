//! Sorted-vec map with the `BTreeMap` API subset used in this workspace.

use alloc::vec::Vec;
use core::borrow::Borrow;
use core::fmt;

use crate::entry::Entry;

/// Map backed by a `Vec<(K, V)>` kept sorted by key.
pub struct VecMap<K, V> {
    pub(crate) entries: Vec<(K, V)>,
}

impl<K, V> VecMap<K, V> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        self.entries.iter().map(pair_refs)
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        self.entries.iter_mut().map(pair_refs_mut)
    }

    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut V> + ExactSizeIterator {
        self.entries.iter_mut().map(|(_, v)| v)
    }

    pub fn into_keys(self) -> impl DoubleEndedIterator<Item = K> + ExactSizeIterator {
        self.entries.into_iter().map(|(k, _)| k)
    }

    pub fn into_values(self) -> impl DoubleEndedIterator<Item = V> + ExactSizeIterator {
        self.entries.into_iter().map(|(_, v)| v)
    }

    #[must_use]
    pub fn first_key_value(&self) -> Option<(&K, &V)> {
        self.entries.first().map(pair_refs)
    }

    #[must_use]
    pub fn last_key_value(&self) -> Option<(&K, &V)> {
        self.entries.last().map(pair_refs)
    }
}

impl<K: Ord, V> VecMap<K, V> {
    pub(crate) fn search<Q>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.entries.binary_search_by(|(k, _)| k.borrow().cmp(key))
    }

    #[must_use]
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let index = self.search(key).ok()?;
        Some(&self.entries[index].1)
    }

    #[must_use]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let index = self.search(key).ok()?;
        Some(&mut self.entries[index].1)
    }

    #[must_use]
    pub fn get_key_value<Q>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let index = self.search(key).ok()?;
        Some(pair_refs(&self.entries[index]))
    }

    #[must_use]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.search(key).is_ok()
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self.search(&key) {
            Ok(index) => Some(core::mem::replace(&mut self.entries[index].1, value)),
            Err(index) => {
                self.entries.insert(index, (key, value));
                None
            }
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let index = self.search(key).ok()?;
        Some(self.entries.remove(index).1)
    }

    pub fn remove_entry<Q>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let index = self.search(key).ok()?;
        Some(self.entries.remove(index))
    }

    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        match self.search(&key) {
            Ok(index) => Entry::Occupied { map: self, index },
            Err(index) => Entry::Vacant {
                map: self,
                index,
                key,
            },
        }
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.entries.retain_mut(|(k, v)| f(k, v));
    }

    /// Move all entries from `other` into `self`; `other` keys win on clash.
    pub fn append(&mut self, other: &mut Self) {
        for (key, value) in other.entries.drain(..) {
            self.insert(key, value);
        }
    }

    /// Iterate the entries whose keys fall within `range`, in key order.
    pub fn range<Q, R>(&self, range: R) -> Iter<'_, K, V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
        R: core::ops::RangeBounds<Q>,
    {
        use core::ops::Bound;
        let start = match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(b) => self.search(b).unwrap_or_else(|i| i),
            Bound::Excluded(b) => match self.search(b) {
                Ok(i) => i + 1,
                Err(i) => i,
            },
        };
        let end = match range.end_bound() {
            Bound::Unbounded => self.entries.len(),
            Bound::Included(b) => match self.search(b) {
                Ok(i) => i + 1,
                Err(i) => i,
            },
            Bound::Excluded(b) => self.search(b).unwrap_or_else(|i| i),
        };
        self.entries[start..end.max(start)].iter().map(pair_refs)
    }
}

impl<K, V, Q> core::ops::Index<&Q> for VecMap<K, V>
where
    K: Borrow<Q> + Ord,
    Q: Ord + ?Sized,
{
    type Output = V;

    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("no entry found for key")
    }
}

fn pair_refs<K, V>(entry: &(K, V)) -> (&K, &V) {
    (&entry.0, &entry.1)
}

fn pair_refs_mut<K, V>(entry: &mut (K, V)) -> (&K, &mut V) {
    (&entry.0, &mut entry.1)
}

/// Borrowed iterator: sorted `(&K, &V)` pairs.
pub type Iter<'a, K, V> =
    core::iter::Map<core::slice::Iter<'a, (K, V)>, fn(&'a (K, V)) -> (&'a K, &'a V)>;

/// Mutable iterator: sorted `(&K, &mut V)` pairs.
pub type IterMut<'a, K, V> =
    core::iter::Map<core::slice::IterMut<'a, (K, V)>, fn(&'a mut (K, V)) -> (&'a K, &'a mut V)>;

impl<K, V> Default for VecMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Clone, V: Clone> Clone for VecMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for VecMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: PartialEq, V: PartialEq> PartialEq for VecMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl<K: Eq, V: Eq> Eq for VecMap<K, V> {}

impl<K: PartialOrd, V: PartialOrd> PartialOrd for VecMap<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.entries.partial_cmp(&other.entries)
    }
}

impl<K: Ord, V: Ord> Ord for VecMap<K, V> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.entries.cmp(&other.entries)
    }
}

impl<K: core::hash::Hash, V: core::hash::Hash> core::hash::Hash for VecMap<K, V> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.entries.hash(state);
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for VecMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = Self::new();
        map.extend(iter);
        map
    }
}

impl<K: Ord, V> Extend<(K, V)> for VecMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<K: Ord, V, const N: usize> From<[(K, V); N]> for VecMap<K, V> {
    fn from(entries: [(K, V); N]) -> Self {
        entries.into_iter().collect()
    }
}

impl<K, V> IntoIterator for VecMap<K, V> {
    type Item = (K, V);
    type IntoIter = alloc::vec::IntoIter<(K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a, K, V> IntoIterator for &'a VecMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut VecMap<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(feature = "serde")]
impl<K: serde::Serialize, V: serde::Serialize> serde::Serialize for VecMap<K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter())
    }
}

#[cfg(feature = "serde")]
impl<'de, K, V> serde::Deserialize<'de> for VecMap<K, V>
where
    K: serde::Deserialize<'de> + Ord,
    V: serde::Deserialize<'de>,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor<K, V> {
            marker: core::marker::PhantomData<(K, V)>,
        }

        impl<'de, K, V> serde::de::Visitor<'de> for MapVisitor<K, V>
        where
            K: serde::Deserialize<'de> + Ord,
            V: serde::Deserialize<'de>,
        {
            type Value = VecMap<K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<Self::Value, A::Error> {
                let mut map = VecMap::with_capacity(access.size_hint().unwrap_or(0));
                while let Some((key, value)) = access.next_entry()? {
                    map.insert(key, value);
                }
                Ok(map)
            }
        }

        deserializer.deserialize_map(MapVisitor {
            marker: core::marker::PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use alloc::string::{String, ToString};

    #[test]
    fn insert_get_remove_keep_sorted_unique_keys() {
        let mut map = VecMap::new();
        assert_eq!(map.insert("b".to_string(), 2), None);
        assert_eq!(map.insert("a".to_string(), 1), None);
        assert_eq!(map.insert("c".to_string(), 3), None);
        assert_eq!(map.insert("b".to_string(), 20), Some(2));

        assert_eq!(map.len(), 3);
        assert_eq!(map.get("b"), Some(&20));
        assert_eq!(map.get("missing"), None);
        let keys: alloc::vec::Vec<&String> = map.keys().collect();
        assert_eq!(keys, ["a", "b", "c"]);

        assert_eq!(map.remove("a"), Some(1));
        assert_eq!(map.remove("a"), None);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn entry_api_matches_btreemap_semantics() {
        let mut map: VecMap<u32, alloc::vec::Vec<u32>> = VecMap::new();
        map.entry(5).or_default().push(1);
        map.entry(5).or_default().push(2);
        map.entry(7).or_insert_with(alloc::vec::Vec::new).push(9);
        map.entry(5).and_modify(|v| v.push(3));
        map.entry(11).and_modify(|v| v.push(0)).or_default();

        assert_eq!(map.get(&5).unwrap().as_slice(), &[1, 2, 3]);
        assert_eq!(map.get(&7).unwrap().as_slice(), &[9]);
        assert!(map.get(&11).unwrap().is_empty());
    }

    #[test]
    fn iteration_is_key_ordered() {
        let map: VecMap<u32, u32> = [(3, 30), (1, 10), (2, 20)].into();
        let pairs: alloc::vec::Vec<(u32, u32)> = map.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(pairs, [(1, 10), (2, 20), (3, 30)]);
    }

    #[test]
    fn retain_and_append() {
        let mut map: VecMap<u32, u32> = (0..6).map(|n| (n, n * 10)).collect();
        map.retain(|k, _| k % 2 == 0);
        let mut other: VecMap<u32, u32> = [(2, 999), (8, 80)].into();
        map.append(&mut other);

        assert!(other.is_empty());
        let pairs: alloc::vec::Vec<(u32, u32)> = map.into_iter().collect();
        assert_eq!(pairs, [(0, 0), (2, 999), (4, 40), (8, 80)]);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_round_trips_as_json_object() {
        let map: VecMap<String, u32> = [("b".to_string(), 2), ("a".to_string(), 1)].into();
        let json = serde_json::to_string(&map).unwrap();
        assert_eq!(json, r#"{"a":1,"b":2}"#);
        let back: VecMap<String, u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, map);
    }
}
