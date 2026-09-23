//! The shared dictionary: key names and common value strings, known to both ends.
//!
//! This is Ion's *shared symbol table* idea without Ion's symbol-table
//! declaration in every stream: both ends compile the same table in, and the
//! wire's existing `ServerHello` version (`WIRE_PROTO_VERSION`) is what says
//! which table that is. Tables are frequency-ordered so the commonest entries
//! get one-byte codes; lookup is a binary search over a sorted index.

#[allow(clippy::all)]
mod data {
    include!("wire_dictionary_data.rs");
}

pub use data::BLOB_KEYS;

fn entry<'a>(text: &'a [u8], offs: &[u16], i: usize) -> Option<&'a [u8]> {
    let a = *offs.get(i)? as usize;
    let b = *offs.get(i + 1)? as usize;
    text.get(a..b)
}

fn find(text: &[u8], offs: &[u16], sorted: &[u16], needle: &[u8]) -> Option<usize> {
    let (mut lo, mut hi) = (0usize, sorted.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let i = sorted[mid] as usize;
        match entry(text, offs, i)?.cmp(needle) {
            core::cmp::Ordering::Less => lo = mid + 1,
            core::cmp::Ordering::Greater => hi = mid,
            core::cmp::Ordering::Equal => return Some(i),
        }
    }
    None
}

pub fn key(i: usize) -> Option<&'static [u8]> {
    entry(data::KEY_TEXT, data::KEY_OFFS, i)
}

pub fn find_key(text: &[u8]) -> Option<usize> {
    find(data::KEY_TEXT, data::KEY_OFFS, data::KEY_SORTED, text)
}

pub fn value(i: usize) -> Option<&'static [u8]> {
    entry(data::VAL_TEXT, data::VAL_OFFS, i)
}

pub fn find_value(text: &[u8]) -> Option<usize> {
    find(data::VAL_TEXT, data::VAL_OFFS, data::VAL_SORTED, text)
}

pub fn is_blob_key(text: &[u8]) -> bool {
    BLOB_KEYS.contains(&text)
}

pub fn key_count() -> usize {
    data::KEY_SORTED.len()
}

pub fn value_count() -> usize {
    data::VAL_SORTED.len()
}
