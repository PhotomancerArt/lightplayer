//! The shared dictionary: key names and common value strings, known to both ends.
//!
//! This is Ion's *shared symbol table* idea without Ion's symbol-table
//! declaration in every stream: both ends compile the same table in, and the
//! wire's existing `ServerHello` version (`WIRE_PROTO_VERSION`) is what says
//! which table that is. Tables are frequency-ordered so the commonest entries
//! get one-byte codes; lookup hashes the text into an open-addressed table of entry indices.

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

/// FNV-1a, 32-bit — the generator's hash; linear probing over a power-of-two
/// table of entry indices (`0xFFFF` = empty), about half full.
fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    h
}

fn find(text: &[u8], offs: &[u16], hash: &[u16], needle: &[u8]) -> Option<usize> {
    let mask = hash.len() - 1;
    let mut h = fnv1a(needle) as usize & mask;
    loop {
        let i = hash[h];
        if i == 0xFFFF {
            return None;
        }
        if entry(text, offs, usize::from(i))? == needle {
            return Some(usize::from(i));
        }
        h = (h + 1) & mask;
    }
}

pub fn key(i: usize) -> Option<&'static [u8]> {
    entry(data::KEY_TEXT, data::KEY_OFFS, i)
}

pub fn find_key(text: &[u8]) -> Option<usize> {
    find(data::KEY_TEXT, data::KEY_OFFS, data::KEY_HASH, text)
}

pub fn value(i: usize) -> Option<&'static [u8]> {
    entry(data::VAL_TEXT, data::VAL_OFFS, i)
}

pub fn find_value(text: &[u8]) -> Option<usize> {
    find(data::VAL_TEXT, data::VAL_OFFS, data::VAL_HASH, text)
}

pub fn is_blob_key(text: &[u8]) -> bool {
    BLOB_KEYS.contains(&text)
}

pub fn key_count() -> usize {
    data::KEY_OFFS.len() - 1
}

pub fn value_count() -> usize {
    data::VAL_OFFS.len() - 1
}
