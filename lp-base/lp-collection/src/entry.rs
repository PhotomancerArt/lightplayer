//! Entry API for [`VecMap`].

use crate::map::VecMap;

/// A view into a single map slot, occupied or vacant.
pub enum Entry<'a, K: Ord, V> {
    Occupied {
        map: &'a mut VecMap<K, V>,
        index: usize,
    },
    Vacant {
        map: &'a mut VecMap<K, V>,
        index: usize,
        key: K,
    },
}

impl<'a, K: Ord, V> Entry<'a, K, V> {
    pub fn or_insert(self, default: V) -> &'a mut V {
        self.or_insert_with(|| default)
    }

    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Occupied { map, index } => &mut map.entries[index].1,
            Entry::Vacant { map, index, key } => {
                map.entries.insert(index, (key, default()));
                &mut map.entries[index].1
            }
        }
    }

    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }

    #[must_use]
    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied { map, index } => &map.entries[*index].0,
            Entry::Vacant { key, .. } => key,
        }
    }

    #[must_use]
    pub fn and_modify<F: FnOnce(&mut V)>(self, f: F) -> Self {
        match self {
            Entry::Occupied { map, index } => {
                f(&mut map.entries[index].1);
                Entry::Occupied { map, index }
            }
            vacant => vacant,
        }
    }
}
