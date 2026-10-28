//! A set kept as a sorted `Vec`, for the few small sets a load builds (part
//! ids, a circle's ring keys): what `BTreeSet::insert` answered, in a
//! fraction of its code.
//!
//! The device links every collection the load uses, once per element type:
//! a `BTreeSet<&str>` and a `BTreeSet<String>` were ~2.5 KB of the C6
//! engine's flash for sets of a few dozen ids (mapping rebuild M3 P13). A
//! sorted `Vec` answers the same questions (is it there; every member in
//! order) with a binary search and a shift.

use alloc::vec::Vec;

/// Insert `value` into `set`, kept sorted; `false` when it was already
/// there (the set unchanged), as `BTreeSet::insert` answers.
pub(crate) fn insert_sorted<T: Ord>(set: &mut Vec<T>, value: T) -> bool {
    match set.binary_search(&value) {
        Ok(_) => false,
        Err(at) => {
            set.insert(at, value);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::vec;

    /// Every insert answers as a `BTreeSet`'s does, and the members end in
    /// its order.
    #[test]
    fn answers_as_a_btree_set_does() {
        let words = ["path2", "circle1", "path2", "a", "zz", "circle1", "b", "a"];
        let mut set = Vec::new();
        let mut tree = BTreeSet::new();
        for word in words {
            assert_eq!(insert_sorted(&mut set, word), tree.insert(word), "{word}");
        }
        assert_eq!(set, tree.into_iter().collect::<Vec<_>>());
        assert_eq!(set, vec!["a", "b", "circle1", "path2", "zz"]);
    }
}
