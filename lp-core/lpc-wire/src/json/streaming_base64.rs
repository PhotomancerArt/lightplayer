//! Streaming base64 helpers for JSON string fields.

use base64::Engine;

use super::json_write::JsonWrite;
use super::json_writer::{JsonValue, JsonWriter, JsonWriterError};

/// Write `bytes` as one JSON base64 string without allocating the encoded text.
pub fn write_base64_string<W>(
    writer: &mut JsonWriter<W>,
    bytes: &[u8],
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    writer.write_raw(b"\"")?;
    write_base64_contents(writer, bytes)?;
    writer.write_raw(b"\"")
}

/// Write `bytes` as base64 into an already-open JSON value slot.
pub fn write_base64_value<W>(
    value: JsonValue<'_, W>,
    bytes: &[u8],
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    write_base64_string(value.writer, bytes)
}

fn write_base64_contents<W>(
    writer: &mut JsonWriter<W>,
    bytes: &[u8],
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    let engine = base64::engine::general_purpose::STANDARD;
    let mut encoded = [0u8; 4];

    for chunk in bytes.chunks(3) {
        let len = engine
            .encode_slice(chunk, &mut encoded)
            .map_err(|_| JsonWriterError::Serialize)?;
        writer.write_raw(&encoded[..len])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use base64::Engine;

    #[test]
    fn base64_string_matches_base64_crate() {
        for bytes in [
            &[][..],
            &[0][..],
            &[0, 1][..],
            &[0, 1, 2][..],
            &[0, 1, 2, 3, 4, 5, 253, 254, 255][..],
        ] {
            let mut out = Vec::new();
            let mut writer = JsonWriter::new(&mut out);
            write_base64_string(&mut writer, bytes).unwrap();

            let expected = alloc::format!(
                "\"{}\"",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            );
            assert_eq!(core::str::from_utf8(&out).unwrap(), expected);
        }
    }
}
