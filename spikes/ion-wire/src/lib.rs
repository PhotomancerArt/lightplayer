//! **Spike** — LPBJ, an Ion-inspired compact binary JSON for the `M!` wire.
//!
//! Throwaway prototype for the `ion-wire-spike` investigation
//! (`~/.photomancer/planning/lp2025/2026-09-23-1528-ion-wire-spike/`). Not wired
//! into any default build and not a wire format anyone speaks.
//!
//! # Shape
//!
//! - [`Encoder`] is a streaming *sink*: feed it the JSON bytes exactly as
//!   `ser_write_json` writes them, and it emits LPBJ into a caller-owned buffer.
//!   Because it sits behind the one erased JSON serializer, no `Serialize` impl
//!   is instantiated twice.
//! - [`decode_to_json`] inflates LPBJ back to the **byte-identical** JSON text,
//!   so every `Deserialize` and every text tool stays as it is.
//! - Both ends share [`wire_dictionary`], a static key and value-string table
//!   that a real version would generate from the wire types and version with
//!   `WIRE_PROTO_VERSION`.
//!
//! # Encoding (one frame = one JSON value)
//!
//! Value position:
//!
//! | byte | meaning |
//! |---|---|
//! | `00..=3F` | unsigned int 0..=63 |
//! | `40..=7F` | value-dictionary string 0..=63 |
//! | `80..=9F` | inline string, length 0..=31, UTF-8 follows |
//! | `A0` | object: key codes follow, then `FF` |
//! | `A1` / `A2` | array start / end |
//! | `A3` `A4` `A5` | null, false, true |
//! | `A6` / `A7` | unsigned / negative int, LEB128 magnitude |
//! | `A8` / `A9` | decimal, +/−: zigzag LEB128 exponent, LEB128 coefficient |
//! | `AA` | string: LEB128 length + UTF-8 |
//! | `AB` | blob (a base64 string under a blob key): LEB128 length + raw bytes |
//! | `AC` | value-dictionary string 64+: LEB128 (index − 64) |
//! | `AD` | back-reference: LEB128 n, the n-th inline text of this frame |
//! | `AE` | number text escape hatch: LEB128 length + ASCII |
//!
//! Key position: `00..=EF` key-dictionary 0..=239; `F0..=FB` + one byte =
//! key-dictionary 240 + ((b − F0) << 8 | next); `FC` inline key (LEB128 length
//! + UTF-8); `FD` back-reference (LEB128 n); `FF` end of object.
//!
//! Back-references: every inline text (value `80..=9F`/`AA`, key `FC`) of two or
//! more bytes is numbered in order, up to [`MAX_BACKREFS`] per frame.

#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub mod base64_blob;
pub mod cobs_frame;
pub mod decimal_text;
pub mod format_tags;
pub mod json_to_lpbj;
pub mod tokens_to_lpbj;
pub mod lpbj_to_json;
pub mod varint;
pub mod wire_dictionary;

pub use json_to_lpbj::{EncodeError, Encoder};
pub use tokens_to_lpbj::TokenEncoder;
pub use lpbj_to_json::{DecodeError, decode_to_json};

/// Back-reference table size per frame, shared by encoder and decoder.
pub const MAX_BACKREFS: usize = 64;

#[cfg(test)]
mod tests {
    #[test]
    fn cobs_in_place_matches_copying_encoder() {
        let payload: [u8; 700] = core::array::from_fn(|i| if i % 97 == 0 { 0 } else { (i * 7) as u8 | 1 });
        let mut a = [0u8; 800];
        let na = crate::cobs_frame::encode(&payload, &mut a).unwrap();
        let mut b = [0u8; 800];
        let at = crate::cobs_frame::in_place_headroom(payload.len());
        b[at..at + payload.len()].copy_from_slice(&payload);
        let nb = crate::cobs_frame::encode_in_place(&mut b, at, payload.len()).unwrap();
        assert_eq!(&a[..na], &b[..nb]);
        assert!(na <= crate::cobs_frame::max_framed_len(payload.len()));
        let mut back = [0u8; 800];
        let n = crate::cobs_frame::decode(&a[2..na - 1], &mut back).unwrap();
        assert_eq!(&back[..n], &payload[..]);
    }
}
