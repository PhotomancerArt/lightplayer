//! Ids for a format-11 map2d's objects as format-6 parts (lean Q58).
//!
//! - An object with an id keeps it.
//! - One without takes its name, slugged (`F-lower` → `f-lower`,
//!   `rim a` → `rim-a`: lowercase, every character the id rule refuses
//!   becomes `-`, runs of `-` fold into one, trimmed at both ends and to the
//!   id rule's 24 bytes), when that is a valid id and **no other object of
//!   the fixture** has it as its id or its slug (logo-sign's `L` and `l`
//!   both give `l`, so neither takes it).
//! - Else its kind and a number (`path3`, `circle1`, `filled2`), the first
//!   number no id in the fixture has.
//! - A repeat's held part takes the object's id; each repeat round it gets
//!   `repeat<n>`, outermost first. Copy k of `rim-a` is then `rim-a@k`, as
//!   today's patch rows read `/rim-a/k`; nested copies are written innermost
//!   first, trailing zeros dropped (F5): today's instances `[outer, inner]`
//!   = `[1, 0]` are `x@0@1`, and `[0, 3]` is `x@3`.
//!
//! The `next` counters exceed every number minted, and every id that reads
//! as minted (a kind's word and digits, `path7`), whatever minted it — what
//! the format-6 writer writes, so the document reads back to itself.

use std::collections::{BTreeMap, BTreeSet};

use super::v11_segments::check_object_id;

/// The words a part's id is minted from, one per kind of part (frozen from
/// `lpc_mapping::part::PART_KIND_WORDS`, in its order: the first word an id
/// reads as minted from is the one its number counts against).
const PART_KIND_WORDS: [&str; 8] = [
    "path", "polygon", "circle", "grid", "filled", "points", "group", "repeat",
];

/// The longest id, in bytes (`MAP2D_OBJECT_ID_MAX_LEN`).
const ID_MAX_LEN: usize = 24;

/// One map2d object, as the id minting needs it.
pub(crate) struct ObjectToName<'a> {
    pub(crate) id: Option<&'a str>,
    pub(crate) name: &'a str,
    /// How many repeats wrap its shape.
    pub(crate) repeats: usize,
    /// Its innermost shape's part kind word (`path`, `circle`, `filled`…).
    pub(crate) word: &'static str,
}

/// The ids every object's parts take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartIds {
    /// Per object: the repeats' ids, outermost first, then the held part's.
    pub(crate) objects: Vec<ObjectPartIds>,
    /// The next number per kind word, for every word that minted or reads
    /// in an id.
    pub(crate) next: BTreeMap<&'static str, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectPartIds {
    pub(crate) repeats: Vec<String>,
    pub(crate) held: String,
}

impl PartIds {
    /// The node id of one segment: the held part's id, with its copies
    /// (today's instances, outermost first) written innermost first,
    /// trailing zeros dropped.
    pub(crate) fn segment_id(&self, object: usize, instances: &[u32]) -> String {
        let mut copies: Vec<u32> = instances.iter().rev().copied().collect();
        while copies.last() == Some(&0) {
            copies.pop();
        }
        let mut id = self.objects[object].held.clone();
        for copy in copies {
            id.push('@');
            id.push_str(&copy.to_string());
        }
        id
    }
}

/// Mint every object's part ids (rules in the module doc).
pub(crate) fn mint_part_ids(objects: &[ObjectToName<'_>]) -> PartIds {
    let mut used: BTreeSet<String> = objects
        .iter()
        .filter_map(|object| object.id.map(String::from))
        .collect();

    // Slugs, taken only where no other object of the fixture claims the
    // same text.
    let slugs: Vec<Option<String>> = objects
        .iter()
        .map(|object| match object.id {
            Some(_) => None,
            None => slug(object.name),
        })
        .collect();
    let mut held: Vec<Option<String>> = objects
        .iter()
        .map(|object| object.id.map(String::from))
        .collect();
    for (index, candidate) in slugs.iter().enumerate() {
        let Some(candidate) = candidate else { continue };
        let others = slugs
            .iter()
            .enumerate()
            .filter(|(other, slug)| *other != index && slug.as_deref() == Some(candidate))
            .count();
        if others == 0 && !used.contains(candidate) {
            used.insert(candidate.clone());
            held[index] = Some(candidate.clone());
        }
    }

    // Kind and number for the rest, and for every repeat, in object order.
    let mut counters: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(objects.len());
    for (object, held) in objects.iter().zip(held) {
        let repeats = (0..object.repeats)
            .map(|_| mint(&mut counters, &mut used, "repeat"))
            .collect();
        let held = held.unwrap_or_else(|| mint(&mut counters, &mut used, object.word));
        out.push(ObjectPartIds { repeats, held });
    }

    // Every id that reads as minted keeps its word's counter above it.
    for id in &used {
        if let Some((word, n)) = minted_part_number(id) {
            let next = counters.entry(word).or_insert(1);
            *next = (*next).max(n.saturating_add(1));
        }
    }
    PartIds {
        objects: out,
        next: counters,
    }
}

/// A name as an id, or `None` when it makes no valid one.
pub(crate) fn slug(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
            c
        } else {
            '-'
        };
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(c);
    }
    out.truncate(ID_MAX_LEN);
    while out.ends_with('-') {
        out.pop();
    }
    check_object_id(&out).is_ok().then_some(out)
}

/// The next `word<n>` no id has, and the counter past it.
fn mint(
    counters: &mut BTreeMap<&'static str, u32>,
    used: &mut BTreeSet<String>,
    word: &'static str,
) -> String {
    let mut n = counters.get(word).copied().unwrap_or(1);
    let id = loop {
        let id = format!("{word}{n}");
        n = n.saturating_add(1);
        if !used.contains(&id) {
            break id;
        }
    };
    counters.insert(word, n);
    used.insert(id.clone());
    id
}

/// The kind word and number of an id that reads as minted (`path3` →
/// `("path", 3)`), the first word in [`PART_KIND_WORDS`] it reads as.
fn minted_part_number(id: &str) -> Option<(&'static str, u32)> {
    PART_KIND_WORDS.iter().find_map(|word| {
        let digits = id.strip_prefix(word)?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((*word, digits.parse().ok()?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_slugs_to_an_id() {
        assert_eq!(slug("F-lower").as_deref(), Some("f-lower"));
        assert_eq!(slug("rim a").as_deref(), Some("rim-a"));
        assert_eq!(slug("  Left  Arm! ").as_deref(), Some("left-arm"));
        assert_eq!(slug("snake_case").as_deref(), Some("snake_case"));
        assert_eq!(slug("2nd row"), None, "an id starts with a letter");
        assert_eq!(slug(""), None);
        assert_eq!(slug("!!!"), None);
        assert_eq!(
            slug("a very long name for one strip of lamps").as_deref(),
            Some("a-very-long-name-for-one")
        );
    }

    /// An object without an id takes its name's slug; two that slug alike
    /// (logo-sign's `L` and `l`) both take kind and number.
    #[test]
    fn colliding_slugs_take_kind_and_number() {
        let ids = mint_part_ids(&[
            object(None, "L", 0, "path"),
            object(None, "l", 0, "path"),
            object(None, "Rim A", 0, "circle"),
            object(Some("keep"), "whatever", 0, "grid"),
        ]);
        assert_eq!(held(&ids), vec!["path1", "path2", "rim-a", "keep"]);
        assert_eq!(ids.next.get("path"), Some(&3));
        assert_eq!(ids.next.get("circle"), None);
    }

    /// A slug that collides with an authored id is not taken, and minting
    /// skips ids that are already there.
    #[test]
    fn minting_skips_ids_already_there() {
        let ids = mint_part_ids(&[
            object(Some("path1"), "a", 0, "path"),
            object(None, "path1", 0, "path"),
            object(None, "", 0, "path"),
        ]);
        assert_eq!(held(&ids), vec!["path1", "path2", "path3"]);
        assert_eq!(ids.next.get("path"), Some(&4));
    }

    /// An id that reads as minted keeps its word's counter above it, even
    /// when nothing of that kind was minted.
    #[test]
    fn a_minted_looking_id_raises_its_counter() {
        let ids = mint_part_ids(&[object(Some("grid7"), "", 0, "grid")]);
        assert_eq!(ids.next.get("grid"), Some(&8));
    }

    /// A repeat's held part takes the object's id; the repeats round it are
    /// `repeat<n>`, outermost first.
    #[test]
    fn a_repeat_gets_its_own_id_and_its_part_keeps_the_objects() {
        let ids = mint_part_ids(&[
            object(Some("rim-a"), "", 1, "polygon"),
            object(None, "", 2, "path"),
        ]);
        assert_eq!(ids.objects[0].repeats, vec!["repeat1"]);
        assert_eq!(ids.objects[0].held, "rim-a");
        assert_eq!(ids.objects[1].repeats, vec!["repeat2", "repeat3"]);
        assert_eq!(ids.objects[1].held, "path1");
        assert_eq!(ids.next.get("repeat"), Some(&4));
    }

    /// Copy k of `rim-a` is `rim-a@k`, as today's row `/rim-a/k`; nested
    /// copies are innermost first, trailing zeros dropped (F5).
    #[test]
    fn copies_are_written_innermost_first() {
        let ids = mint_part_ids(&[object(Some("x"), "", 2, "path")]);
        assert_eq!(ids.segment_id(0, &[]), "x");
        assert_eq!(ids.segment_id(0, &[0, 0]), "x");
        assert_eq!(ids.segment_id(0, &[0, 3]), "x@3");
        assert_eq!(ids.segment_id(0, &[1, 0]), "x@0@1");
        assert_eq!(ids.segment_id(0, &[2, 3]), "x@3@2");
    }

    fn object<'a>(
        id: Option<&'a str>,
        name: &'a str,
        repeats: usize,
        word: &'static str,
    ) -> ObjectToName<'a> {
        ObjectToName {
            id,
            name,
            repeats,
            word,
        }
    }

    fn held(ids: &PartIds) -> Vec<&str> {
        ids.objects.iter().map(|o| o.held.as_str()).collect()
    }
}
