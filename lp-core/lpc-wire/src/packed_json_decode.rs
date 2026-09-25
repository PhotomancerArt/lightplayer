//! Hosts: a packed wire frame back to its JSON text.

use alloc::string::String;
use alloc::vec::Vec;

use lp_json_pack::{DecodeError, decode};

use crate::wire_dictionary::WIRE_DICTIONARY;

/// Why a packed frame did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackedDecodeError {
    /// The frame is not a well-formed JSON Pack value against
    /// [`WIRE_DICTIONARY`].
    Decode(DecodeError),
    /// The decoded text is not UTF-8 (a frame that carried broken text).
    NotUtf8,
}

impl core::fmt::Display for PackedDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "packed frame did not decode: {e:?}"),
            Self::NotUtf8 => f.write_str("packed frame decoded to text that is not UTF-8"),
        }
    }
}

/// Decode one packed frame's payload (COBS already undone) into exactly the
/// JSON text the board would have written as `M!{json}`.
pub fn decode_packed_to_json(packed: &[u8]) -> Result<String, PackedDecodeError> {
    // Packed frames run 3-4x smaller than their JSON; start there.
    let mut out = Vec::with_capacity(packed.len() * 4);
    decode(&WIRE_DICTIONARY, packed, &mut out).map_err(PackedDecodeError::Decode)?;
    String::from_utf8(out).map_err(|_| PackedDecodeError::NotUtf8)
}
