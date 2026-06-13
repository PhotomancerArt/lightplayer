//! Sorted-vec set with the `BTreeSet` API subset used in this workspace.

use alloc::vec::Vec;
use core::borrow::Borrow;
use core::fmt;

/// Set backed by a sorted `Vec<T>`.
pub struct VecSet<T> {
    items: Vec<T>,
}

impl<T> VecSet<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.items.iter()
    }

    #[must_use]
    pub fn first(&self) -> Option<&T> {
        self.items.first()
    }

    #[must_use]
    pub fn last(&self) -> Option<&T> {
        self.items.last()
    }
}

impl<T: Ord> VecSet<T> {
    fn search<Q>(&self, item: &Q) -> Result<usize, usize>
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.items
            .binary_search_by(|probe| probe.borrow().cmp(item))
    }

    #[must_use]
    pub fn contains<Q>(&self, item: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.search(item).is_ok()
    }

    #[must_use]
    pub fn get<Q>(&self, item: &Q) -> Option<&T>
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let index = self.search(item).ok()?;
        Some(&self.items[index])
    }

    pub fn insert(&mut self, item: T) -> bool {
        match self.search(&item) {
            Ok(_) => false,
            Err(index) => {
                self.items.insert(index, item);
                true
            }
        }
    }

    pub fn remove<Q>(&mut self, item: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        match self.search(item) {
            Ok(index) => {
                self.items.remove(index);
                true
            }
            Err(_) => false,
        }
    }

    pub fn take<Q>(&mut self, item: &Q) -> Option<T>
    where
        T: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let index = self.search(item).ok()?;
        Some(self.items.remove(index))
    }

    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.items.retain(f);
    }
}

impl<T> Default for VecSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Clone for VecSet<T> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for VecSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<T: PartialEq> PartialEq for VecSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl<T: Eq> Eq for VecSet<T> {}

impl<T: PartialOrd> PartialOrd for VecSet<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.items.partial_cmp(&other.items)
    }
}

impl<T: Ord> Ord for VecSet<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.items.cmp(&other.items)
    }
}

impl<T: core::hash::Hash> core::hash::Hash for VecSet<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.items.hash(state);
    }
}

impl<T: Ord> FromIterator<T> for VecSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut set = Self::new();
        set.extend(iter);
        set
    }
}

impl<T: Ord> Extend<T> for VecSet<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.insert(item);
        }
    }
}

impl<T: Ord, const N: usize> From<[T; N]> for VecSet<T> {
    fn from(items: [T; N]) -> Self {
        items.into_iter().collect()
    }
}

impl<T> IntoIterator for VecSet<T> {
    type Item = T;
    type IntoIter = alloc::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a VecSet<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for VecSet<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

#[cfg(feature = "serde")]
impl<'de, T> serde::Deserialize<'de> for VecSet<T>
where
    T: serde::Deserialize<'de> + Ord,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SetVisitor<T> {
            marker: core::marker::PhantomData<T>,
        }

        impl<'de, T> serde::de::Visitor<'de> for SetVisitor<T>
        where
            T: serde::Deserialize<'de> + Ord,
        {
            type Value = VecSet<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<Self::Value, A::Error> {
                let mut set = VecSet::new();
                while let Some(item) = access.next_element()? {
                    set.insert(item);
                }
                Ok(set)
            }
        }

        deserializer.deserialize_seq(SetVisitor {
            marker: core::marker::PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_contains_remove_keep_sorted_unique() {
        let mut set = VecSet::new();
        assert!(set.insert(3));
        assert!(set.insert(1));
        assert!(!set.insert(3));

        assert!(set.contains(&1));
        assert!(!set.contains(&2));
        let items: alloc::vec::Vec<u32> = set.iter().copied().collect();
        assert_eq!(items, [1, 3]);

        assert!(set.remove(&1));
        assert!(!set.remove(&1));
    }
}
