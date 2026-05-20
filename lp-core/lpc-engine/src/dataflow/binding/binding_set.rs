//! Node-owned runtime bindings.

use alloc::vec::Vec;

use super::BindingEntry;

/// Ordered bindings owned by a single node entry.
#[derive(Clone, Debug, Default)]
pub struct BindingSet {
    entries: Vec<BindingEntry>,
}

impl BindingSet {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: BindingEntry) -> usize {
        let index = self.entries.len();
        self.entries.push(entry);
        index
    }

    pub fn get(&self, index: usize) -> Option<&BindingEntry> {
        self.entries.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &BindingEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
