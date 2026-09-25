//! The injected dictionary: key names and common value strings both ends know.
//!
//! This is Ion's *shared symbol table* without Ion's per-stream declaration:
//! both ends compile the same tables in, and something outside the frame (on
//! the wire, `WIRE_PROTO_VERSION` plus the runtime [`Dictionary::fingerprint`]
//! check) says which tables those are. Tables are frequency-ordered by whoever
//! generates them, so the commonest entries get the one-byte codes.
//!
//! The shape is plain `&'static` data so a generated file is nothing but
//! statics:
//!
//! ```
//! use lp_json_pack::{Dictionary, PackStrings};
//!
//! static KEYS: PackStrings = PackStrings { text: "kindvalue", offsets: &[0, 4, 9] };
//! static VALUES: PackStrings = PackStrings { text: "rgb", offsets: &[0, 3] };
//! pub static MY_DICTIONARY: Dictionary = Dictionary {
//!     keys: KEYS,
//!     values: VALUES,
//!     // FNV-1a of each entry, linear probing over a power-of-two table of
//!     // entry indices, 0xFFFF = empty (see `DictionaryBuilder` for the maker).
//!     key_hash: &[0xFFFF, 0xFFFF, 1, 0],
//!     value_hash: &[0, 0xFFFF],
//! };
//! assert_eq!(MY_DICTIONARY.check(), Ok(()));
//! assert_eq!(MY_DICTIONARY.find_key(b"value"), Some(1));
//! const FINGERPRINT: u32 = MY_DICTIONARY.fingerprint();
//! # let _ = FINGERPRINT;
//! ```

use crate::pack_tags;

/// The value of an empty slot in a hash table.
pub const HASH_EMPTY: u16 = 0xFFFF;

/// The JSON Pack format revision. It is mixed into every
/// [`Dictionary::fingerprint`], so two ends that disagree on the tag table
/// disagree on the fingerprint too.
pub const PACK_FORMAT_VERSION: u8 = 1;

/// An ordered string table: entry `i` is `text[offsets[i]..offsets[i + 1]]`.
#[derive(Debug, Clone, Copy)]
pub struct PackStrings {
    /// Every entry, concatenated.
    pub text: &'static str,
    /// `len() + 1` byte offsets into `text`, starting at 0, non-decreasing.
    pub offsets: &'static [u16],
}

impl PackStrings {
    /// No entries.
    pub const EMPTY: PackStrings = PackStrings {
        text: "",
        offsets: &[0],
    };

    /// Entry count.
    pub const fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Whether the table has no entries.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entry `i`, or `None` past the end (or on a malformed table).
    pub fn get(&self, i: usize) -> Option<&'static [u8]> {
        let a = usize::from(*self.offsets.get(i)?);
        let b = usize::from(*self.offsets.get(i + 1)?);
        self.text.as_bytes().get(a..b)
    }
}

/// A key table and a value table, each with its hashed lookup.
#[derive(Debug, Clone, Copy)]
pub struct Dictionary {
    /// Object keys, in code order.
    pub keys: PackStrings,
    /// Value strings, in code order.
    pub values: PackStrings,
    /// Open-addressed lookup into `keys`: a power-of-two table of entry indices.
    pub key_hash: &'static [u16],
    /// Open-addressed lookup into `values`.
    pub value_hash: &'static [u16],
}

/// Why a dictionary's tables are unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryError {
    /// Offsets are empty, do not start at 0, decrease, or run past the text.
    BadOffsets,
    /// An entry does not start or end on a UTF-8 character boundary.
    NotUtf8Boundary,
    /// A hash table's length is not a power of two, or has no empty slot.
    BadHashTable,
    /// An entry is missing from its hash table, or is not found at its index.
    HashMismatch,
    /// The same text appears twice in one table.
    Duplicate,
    /// More keys than key codes ([`pack_tags::KEY_DICT_MAX`]).
    TooManyKeys,
}

impl Dictionary {
    /// No entries: every key and string goes inline.
    pub const EMPTY: Dictionary = Dictionary {
        keys: PackStrings::EMPTY,
        values: PackStrings::EMPTY,
        key_hash: &[],
        value_hash: &[],
    };

    /// Key `i`.
    pub fn key(&self, i: usize) -> Option<&'static [u8]> {
        self.keys.get(i)
    }

    /// Value string `i`.
    pub fn value(&self, i: usize) -> Option<&'static [u8]> {
        self.values.get(i)
    }

    /// The code of key `text`, if the dictionary has it.
    pub fn find_key(&self, text: &[u8]) -> Option<usize> {
        find(&self.keys, self.key_hash, text)
    }

    /// The code of value string `text`, if the dictionary has it.
    pub fn find_value(&self, text: &[u8]) -> Option<usize> {
        find(&self.values, self.value_hash, text)
    }

    /// A stable 32-bit hash of what the tables *mean*: the format revision
    /// and every entry of both tables, in order. The hash tables are left out
    /// (they are derived). Two dictionaries with the same fingerprint encode
    /// and decode identically.
    ///
    /// Algorithm: FNV-1a (32-bit) over `b"JSONPACK"`, [`PACK_FORMAT_VERSION`],
    /// then for the key table and then the value table: the entry count as a
    /// u32 little-endian, and each entry as its length (u32 LE) and its bytes.
    pub const fn fingerprint(&self) -> u32 {
        let mut h = FNV_OFFSET;
        h = fnv_bytes(h, b"JSONPACK");
        h = fnv_byte(h, PACK_FORMAT_VERSION);
        h = fingerprint_table(h, &self.keys);
        fingerprint_table(h, &self.values)
    }

    /// Check the tables are well-formed and the hash tables find every entry.
    /// Generated dictionaries should assert this in a test.
    pub fn check(&self) -> Result<(), DictionaryError> {
        if self.keys.len() > pack_tags::KEY_DICT_MAX {
            return Err(DictionaryError::TooManyKeys);
        }
        check_table(&self.keys, self.key_hash)?;
        check_table(&self.values, self.value_hash)
    }
}

/// FNV-1a over `bytes`: the hash the lookup tables are built with.
pub const fn fnv1a(bytes: &[u8]) -> u32 {
    fnv_bytes(FNV_OFFSET, bytes)
}

const FNV_OFFSET: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

const fn fnv_byte(h: u32, b: u8) -> u32 {
    (h ^ b as u32).wrapping_mul(FNV_PRIME)
}

const fn fnv_bytes(mut h: u32, bytes: &[u8]) -> u32 {
    let mut i = 0;
    while i < bytes.len() {
        h = fnv_byte(h, bytes[i]);
        i += 1;
    }
    h
}

const fn fnv_u32(h: u32, v: u32) -> u32 {
    fnv_bytes(h, &v.to_le_bytes())
}

const fn fingerprint_table(mut h: u32, table: &PackStrings) -> u32 {
    let n = table.len();
    h = fnv_u32(h, n as u32);
    let text = table.text.as_bytes();
    let mut i = 0;
    while i < n {
        let a = table.offsets[i] as usize;
        let b = table.offsets[i + 1] as usize;
        h = fnv_u32(h, b.saturating_sub(a) as u32);
        let mut j = a;
        while j < b && j < text.len() {
            h = fnv_byte(h, text[j]);
            j += 1;
        }
        i += 1;
    }
    h
}

/// Linear probing from the entry's FNV-1a slot until a match or an empty slot.
fn find(table: &PackStrings, hash: &[u16], needle: &[u8]) -> Option<usize> {
    if hash.is_empty() {
        return None;
    }
    let mask = hash.len() - 1;
    let mut slot = fnv1a(needle) as usize & mask;
    // Bounded, so a table without an empty slot cannot loop forever.
    for _ in 0..hash.len() {
        let i = hash[slot];
        if i == HASH_EMPTY {
            return None;
        }
        if table.get(usize::from(i))? == needle {
            return Some(usize::from(i));
        }
        slot = (slot + 1) & mask;
    }
    None
}

fn check_table(table: &PackStrings, hash: &[u16]) -> Result<(), DictionaryError> {
    let offs = table.offsets;
    if offs.first() != Some(&0) || usize::from(offs[offs.len() - 1]) != table.text.len() {
        return Err(DictionaryError::BadOffsets);
    }
    if offs.windows(2).any(|w| w[0] > w[1]) {
        return Err(DictionaryError::BadOffsets);
    }
    if offs
        .iter()
        .any(|&o| !table.text.is_char_boundary(usize::from(o)))
    {
        return Err(DictionaryError::NotUtf8Boundary);
    }
    if table.is_empty() {
        return Ok(());
    }
    if !hash.len().is_power_of_two() || !hash.contains(&HASH_EMPTY) {
        return Err(DictionaryError::BadHashTable);
    }
    for i in 0..table.len() {
        let text = table.get(i).ok_or(DictionaryError::BadOffsets)?;
        match find(table, hash, text) {
            Some(j) if j == i => {}
            Some(_) => return Err(DictionaryError::Duplicate),
            None => return Err(DictionaryError::HashMismatch),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinned (checked by hand against the documented algorithm): if this
    /// moves, every dictionary's fingerprint moved.
    const PINNED_TWO_FINGERPRINT: u32 = 0x0f5e_fe26;

    static TWO: Dictionary = Dictionary {
        keys: PackStrings {
            text: "kindvalue",
            offsets: &[0, 4, 9],
        },
        values: PackStrings {
            text: "rgb",
            offsets: &[0, 3],
        },
        key_hash: &[HASH_EMPTY, HASH_EMPTY, 1, 0],
        value_hash: &[0, HASH_EMPTY],
    };

    #[test]
    fn looks_up_both_tables() {
        assert_eq!(TWO.check(), Ok(()));
        assert_eq!(TWO.find_key(b"kind"), Some(0));
        assert_eq!(TWO.find_key(b"value"), Some(1));
        assert_eq!(TWO.find_key(b"rgb"), None);
        assert_eq!(TWO.find_value(b"rgb"), Some(0));
        assert_eq!(TWO.key(1), Some(&b"value"[..]));
        assert_eq!(TWO.key(2), None);
    }

    #[test]
    fn empty_dictionary_finds_nothing() {
        assert_eq!(Dictionary::EMPTY.check(), Ok(()));
        assert_eq!(Dictionary::EMPTY.find_key(b"x"), None);
        assert_eq!(Dictionary::EMPTY.find_value(b""), None);
    }

    #[test]
    fn fingerprint_is_stable_and_const() {
        const FP: u32 = TWO.fingerprint();
        // Pinned: a change here is a format change for every dictionary.
        assert_eq!(FP, TWO.fingerprint());
        assert_ne!(FP, Dictionary::EMPTY.fingerprint());
        assert_eq!(FP, PINNED_TWO_FINGERPRINT);
    }

    #[test]
    fn fingerprint_sees_entry_boundaries() {
        static SPLIT: Dictionary = Dictionary {
            keys: PackStrings {
                text: "kindvalue",
                offsets: &[0, 5, 9],
            },
            ..TWO
        };
        assert_ne!(SPLIT.fingerprint(), TWO.fingerprint());
    }

    #[test]
    fn check_catches_bad_tables() {
        static NO_EMPTY: Dictionary = Dictionary {
            key_hash: &[1, 0, 1, 0],
            ..TWO
        };
        assert_eq!(NO_EMPTY.check(), Err(DictionaryError::BadHashTable));
        static MISSING: Dictionary = Dictionary {
            key_hash: &[HASH_EMPTY, HASH_EMPTY, HASH_EMPTY, 0],
            ..TWO
        };
        assert_eq!(MISSING.check(), Err(DictionaryError::HashMismatch));
        static BAD_OFFS: Dictionary = Dictionary {
            keys: PackStrings {
                text: "kindvalue",
                offsets: &[0, 4, 8],
            },
            ..TWO
        };
        assert_eq!(BAD_OFFS.check(), Err(DictionaryError::BadOffsets));
    }
}
