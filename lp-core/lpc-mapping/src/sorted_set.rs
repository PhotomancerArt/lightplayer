//! Sets and maps kept as sorted `Vec`s, for the few small ones a load
//! builds (part ids, a circle's ring keys, the id counters): what
//! `BTreeSet::insert` and `BTreeMap::insert` answered, in a fraction of
//! their code.
//!
//! The device links every collection the load uses, once per element type:
//! a `BTreeSet<&str>` and a `BTreeSet<String>` were ~2.5 KB of the C6
//! engine's flash for sets of a few dozen ids (mapping rebuild M3 P13). A
//! sorted `Vec` answers the same questions (is it there; every member in
//! order) with a binary search and a shift; a `BTreeMap<String, u32>` was
//! another ~2 KB.

use alloc::string::String;
use alloc::vec::Vec;

/// A small map from words to numbers, sorted by word: a kind's next id
/// number ([`crate::part::IdCounters`], a document's `next`).
pub(crate) type WordNumbers = Vec<(String, u32)>;

/// Where `word` is in `map`, or where it would go.
pub(crate) fn find_word(map: &WordNumbers, word: &str) -> Result<usize, usize> {
    map.binary_search_by(|(held, _)| held.as_str().cmp(word))
}

/// Set `word`'s number, the word added in order when new (a word given
/// twice keeps its last number, as `BTreeMap::insert` keeps it).
pub(crate) fn set_word(map: &mut WordNumbers, word: &str, n: u32) {
    match find_word(map, word) {
        Ok(at) => map[at].1 = n,
        Err(at) => map.insert(at, (String::from(word), n)),
    }
}

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

    /// Every set answers as a `BTreeMap`'s insert does: the last number
    /// kept, the words in its order.
    #[test]
    fn words_answer_as_a_btree_map_does() {
        use alloc::collections::BTreeMap;
        use alloc::string::ToString;
        let sets = [
            ("path", 4),
            ("circle", 2),
            ("path", 1),
            ("grid", 9),
            ("circle", 3),
        ];
        let mut map = WordNumbers::new();
        let mut tree = BTreeMap::new();
        for (word, n) in sets {
            set_word(&mut map, word, n);
            tree.insert(word.to_string(), n);
        }
        assert_eq!(map, tree.into_iter().collect::<Vec<_>>());
        assert_eq!(find_word(&map, "grid"), Ok(1));
        assert_eq!(find_word(&map, "points"), Err(3));
    }
}
