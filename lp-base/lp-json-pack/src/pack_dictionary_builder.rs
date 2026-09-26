//! Building a [`Dictionary`] on a host (feature `alloc`).
//!
//! Generators and tests use this to lay out the tables and their hash lookups
//! exactly as [`Dictionary`] reads them. Nothing here runs on a device: a
//! device links the generated statics, not the builder.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::pack_dictionary::{Dictionary, HASH_EMPTY, PackStrings, fnv1a};

/// A dictionary's tables, owned. [`OwnedDictionary::leak`] turns it into the
/// `&'static` shape the encoder and decoder take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedDictionary {
    /// Key entries, in code order.
    pub keys: Vec<String>,
    /// Value entries, in code order.
    pub values: Vec<String>,
}

/// One table laid out for [`PackStrings`] plus its hash lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLayout {
    /// Every entry, concatenated.
    pub text: String,
    /// `len + 1` offsets into `text`.
    pub offsets: Vec<u16>,
    /// The open-addressed lookup table.
    pub hash: Vec<u16>,
}

impl OwnedDictionary {
    /// A dictionary with these entries, in this order.
    pub fn from_entries<K: AsRef<str>, V: AsRef<str>>(keys: &[K], values: &[V]) -> Self {
        Self {
            keys: keys.iter().map(|k| String::from(k.as_ref())).collect(),
            values: values.iter().map(|v| String::from(v.as_ref())).collect(),
        }
    }

    /// Lay out the key table.
    pub fn key_layout(&self) -> TableLayout {
        layout_table(&self.keys)
    }

    /// Lay out the value table.
    pub fn value_layout(&self) -> TableLayout {
        layout_table(&self.values)
    }

    /// Leak the tables into a `&'static Dictionary` (tests and host tools).
    pub fn leak(&self) -> &'static Dictionary {
        let k = self.key_layout();
        let v = self.value_layout();
        Box::leak(Box::new(Dictionary {
            keys: PackStrings {
                text: Box::leak(k.text.into_boxed_str()),
                offsets: Box::leak(k.offsets.into_boxed_slice()),
            },
            values: PackStrings {
                text: Box::leak(v.text.into_boxed_str()),
                offsets: Box::leak(v.offsets.into_boxed_slice()),
            },
            key_hash: Box::leak(k.hash.into_boxed_slice()),
            value_hash: Box::leak(v.hash.into_boxed_slice()),
        }))
    }
}

/// Lay out `entries` as concatenated text, offsets, and a hash table about
/// half full. Entries must be distinct and total under 64 KiB of text.
pub fn layout_table<S: AsRef<str>>(entries: &[S]) -> TableLayout {
    let mut text = String::new();
    let mut offsets = vec![0u16];
    for e in entries {
        text.push_str(e.as_ref());
        offsets.push(u16::try_from(text.len()).expect("dictionary text over 64 KiB"));
    }
    let hash = if entries.is_empty() {
        Vec::new()
    } else {
        let size = (entries.len() * 2).next_power_of_two().max(2);
        let mut hash = vec![HASH_EMPTY; size];
        let mask = size - 1;
        for (i, e) in entries.iter().enumerate() {
            let mut slot = fnv1a(e.as_ref().as_bytes()) as usize & mask;
            while hash[slot] != HASH_EMPTY {
                slot = (slot + 1) & mask;
            }
            hash[slot] = u16::try_from(i).expect("over 65535 dictionary entries");
        }
        hash
    };
    TableLayout {
        text,
        offsets,
        hash,
    }
}

/// Harvests a frequency-ranked dictionary from sample JSON text.
///
/// Every object key and every string value is counted. Strings holding an
/// escape are skipped (a dictionary entry is matched against unescaped text,
/// and such strings are rare on the wire).
#[derive(Debug, Default, Clone)]
pub struct DictionaryBuilder {
    keys: BTreeMap<String, u64>,
    values: BTreeMap<String, u64>,
}

impl DictionaryBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Count the keys and string values in one JSON text. Text that is not
    /// JSON is counted as far as it looks like strings; it cannot fail.
    pub fn observe_json(&mut self, json: &[u8]) {
        let mut i = 0;
        while i < json.len() {
            if json[i] != b'"' {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            let mut escaped = false;
            while j < json.len() && json[j] != b'"' {
                if json[j] == b'\\' {
                    escaped = true;
                    j += 1;
                }
                j += 1;
            }
            let end = j.min(json.len());
            i = end + 1;
            if escaped {
                continue;
            }
            let Ok(s) = core::str::from_utf8(&json[start..end]) else {
                continue;
            };
            let is_key = json.get(i) == Some(&b':');
            let map = if is_key {
                &mut self.keys
            } else {
                &mut self.values
            };
            *map.entry(String::from(s)).or_insert(0) += 1;
        }
    }

    /// Rank what was observed: every key, and every value string seen at
    /// least `min_value_count` times, commonest first (ties by name).
    pub fn build(&self, min_value_count: u64) -> OwnedDictionary {
        fn rank(map: &BTreeMap<String, u64>, min: u64) -> Vec<String> {
            let mut v: Vec<(&String, u64)> = map
                .iter()
                .filter(|&(_, &n)| n >= min)
                .map(|(s, &n)| (s, n))
                .collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            v.into_iter().map(|(s, _)| s.clone()).collect()
        }
        OwnedDictionary {
            keys: rank(&self.keys, 1),
            values: rank(&self.values, min_value_count.max(1)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaked_dictionary_checks_and_finds_every_entry() {
        let keys: Vec<String> = (0..700).map(|i| alloc::format!("key{i}")).collect();
        let owned = OwnedDictionary::from_entries(&keys, &["a", "b", "ünï"]);
        let d = owned.leak();
        assert_eq!(d.check(), Ok(()));
        for (i, k) in keys.iter().enumerate() {
            assert_eq!(d.find_key(k.as_bytes()), Some(i));
        }
        assert_eq!(d.find_value("ünï".as_bytes()), Some(2));
    }

    #[test]
    fn harvests_by_frequency() {
        let mut b = DictionaryBuilder::new();
        b.observe_json(br#"{"kind":"rgb","value":"rgb","x":"a\"b"}"#);
        b.observe_json(br#"{"kind":"rgba"}"#);
        let d = b.build(2);
        assert_eq!(d.keys, ["kind", "value", "x"]);
        assert_eq!(d.values, ["rgb"]);
    }
}
