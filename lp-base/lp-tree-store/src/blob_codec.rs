//! Coding one chunk's payload: stored, raw deflate, or raw deflate against
//! the store dictionary. Encoding is host-only (feature `encode`, flate2 on
//! zlib-rs, which takes a preset dictionary for a raw stream); decoding is
//! always `lp_deflate::inflate`, the device's own decoder, with the
//! dictionary as `buf[..start]`.

use alloc::vec;
use alloc::vec::Vec;

use crate::object_id::ObjectId;
use crate::record_kind::ChunkCodec;

/// The most logical bytes one chunk decodes to (bounds the decode buffer to
/// dictionary + this).
pub const MAX_LOGICAL_CHUNK: usize = 4096;

/// The payload prefix before the deflate stream.
pub fn prefix_len(codec: ChunkCodec) -> usize {
    match codec {
        ChunkCodec::Stored => 0,
        ChunkCodec::Deflate => 2,
        ChunkCodec::DeflateDict => 10,
    }
}

/// The dictionary a payload names, if any.
pub fn chunk_dict_ref(codec: ChunkCodec, payload: &[u8]) -> Option<ObjectId> {
    if codec != ChunkCodec::DeflateDict || payload.len() < 10 {
        return None;
    }
    let mut x = [0u8; 8];
    x.copy_from_slice(&payload[2..10]);
    Some(ObjectId(u64::from_le_bytes(x)))
}

/// Decode a chunk payload; `dict` must be the bytes of
/// [`chunk_dict_ref`]'s dictionary for `DeflateDict`. `None` = undecodable.
pub fn decode_chunk(codec: ChunkCodec, payload: &[u8], dict: Option<&[u8]>) -> Option<Vec<u8>> {
    if codec == ChunkCodec::Stored {
        return Some(payload.to_vec());
    }
    if payload.len() < prefix_len(codec) {
        return None;
    }
    let len = usize::from(u16::from_le_bytes([payload[0], payload[1]]));
    if len > MAX_LOGICAL_CHUNK {
        return None;
    }
    let dict: &[u8] = match codec {
        ChunkCodec::DeflateDict => dict?,
        _ => &[],
    };
    let mut buf = vec![0u8; dict.len() + len];
    buf[..dict.len()].copy_from_slice(dict);
    match lp_deflate::inflate(&payload[prefix_len(codec)..], &mut buf, dict.len()) {
        Ok(n) if n == len => Some(buf.split_off(dict.len())),
        _ => None,
    }
}

/// Try to code `bytes` with deflate (against `dict` when given). Returns the
/// payload, round-tripped through `lp_deflate`, or `None` when the encoder is
/// not built in or failed.
pub fn deflate_payload(
    bytes: &[u8],
    dict: Option<(ObjectId, &[u8])>,
) -> Option<(ChunkCodec, Vec<u8>)> {
    if bytes.len() > MAX_LOGICAL_CHUNK {
        return None;
    }
    let (codec, dict_bytes) = match dict {
        Some((_, d)) => (ChunkCodec::DeflateDict, d),
        None => (ChunkCodec::Deflate, &[][..]),
    };
    let z = raw_deflate(dict_bytes, bytes)?;
    let mut payload = Vec::with_capacity(prefix_len(codec) + z.len());
    payload.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    if let Some((id, _)) = dict {
        payload.extend_from_slice(&id.0.to_le_bytes());
    }
    payload.extend_from_slice(&z);
    let back = decode_chunk(codec, &payload, Some(dict_bytes))?;
    (back == bytes).then_some((codec, payload))
}

#[cfg(feature = "encode")]
fn raw_deflate(dict: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    use flate2::{Compress, Compression, FlushCompress, Status};
    let mut c = Compress::new(Compression::best(), false);
    if !dict.is_empty() {
        c.set_dictionary(dict).ok()?;
    }
    let mut z = Vec::with_capacity(data.len() + 64);
    match c.compress_vec(data, &mut z, FlushCompress::Finish) {
        Ok(Status::StreamEnd) => Some(z),
        _ => None,
    }
}

#[cfg(not(feature = "encode"))]
fn raw_deflate(_dict: &[u8], _data: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(all(test, feature = "encode"))]
mod tests {
    use super::*;

    #[test]
    fn deflate_round_trips_with_and_without_dict() {
        let data = b"{\"kind\": \"shader\", \"params\": {\"speed\": 1.0}}\n".repeat(20);
        let (c, p) = deflate_payload(&data, None).unwrap();
        assert_eq!(c, ChunkCodec::Deflate);
        assert!(p.len() < data.len());
        assert_eq!(decode_chunk(c, &p, None).unwrap(), data);

        let dict = b"{\"kind\": \"shader\", \"params\": {\"speed\": ".to_vec();
        let small = b"{\"kind\": \"shader\", \"params\": {\"speed\": 2.0}}";
        let (c, p) = deflate_payload(small, Some((ObjectId(9), &dict))).unwrap();
        assert_eq!(c, ChunkCodec::DeflateDict);
        assert_eq!(chunk_dict_ref(c, &p), Some(ObjectId(9)));
        assert_eq!(decode_chunk(c, &p, Some(&dict)).unwrap(), small);
        assert_eq!(decode_chunk(c, &p, None), None);
    }

    #[test]
    fn garbage_never_panics() {
        for n in 0..40u8 {
            let junk: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            let _ = decode_chunk(ChunkCodec::Deflate, &junk, None);
            let _ = decode_chunk(ChunkCodec::DeflateDict, &junk, Some(b"abc"));
        }
    }
}
