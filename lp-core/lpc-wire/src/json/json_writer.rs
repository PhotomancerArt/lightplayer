//! Semantic JSON writer with automatic comma handling.

use core::fmt;

use serde::Serialize;

use super::json_write::JsonWrite;

/// Error returned by the semantic JSON writer.
#[derive(Debug)]
pub enum JsonWriterError<E> {
    Write(E),
    Serialize,
}

impl<E: fmt::Display> fmt::Display for JsonWriterError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write(error) => write!(f, "{error}"),
            Self::Serialize => f.write_str("JSON serialization failed"),
        }
    }
}

/// Semantic JSON writer over a bounded byte sink.
pub struct JsonWriter<W> {
    out: W,
}

impl<W> JsonWriter<W>
where
    W: JsonWrite,
{
    #[must_use]
    pub fn new(out: W) -> Self {
        Self { out }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    pub fn object(&mut self) -> Result<JsonObject<'_, W>, JsonWriterError<W::Error>> {
        self.write_raw(b"{")?;
        Ok(JsonObject::new(self))
    }

    pub fn array(&mut self) -> Result<JsonArray<'_, W>, JsonWriterError<W::Error>> {
        self.write_raw(b"[")?;
        Ok(JsonArray::new(self))
    }

    pub fn null(&mut self) -> Result<(), JsonWriterError<W::Error>> {
        self.write_raw(b"null")
    }

    pub fn bool(&mut self, value: bool) -> Result<(), JsonWriterError<W::Error>> {
        self.write_raw(if value { b"true" } else { b"false" })
    }

    pub fn i64(&mut self, value: i64) -> Result<(), JsonWriterError<W::Error>> {
        self.write_display(value)
    }

    pub fn u64(&mut self, value: u64) -> Result<(), JsonWriterError<W::Error>> {
        self.write_display(value)
    }

    pub fn string(&mut self, value: &str) -> Result<(), JsonWriterError<W::Error>> {
        self.write_json_string(value)
    }

    pub fn serde<T>(&mut self, value: &T) -> Result<(), JsonWriterError<W::Error>>
    where
        T: Serialize,
    {
        write_serde(self, value)
    }

    /// Write already-formed JSON bytes.
    ///
    /// This is intentionally low level. Prefer [`JsonObject`], [`JsonArray`],
    /// and typed value methods for normal JSON construction; direct writers use
    /// this when they need to preserve an existing wire envelope without first
    /// allocating an intermediate object.
    pub fn write_raw(&mut self, bytes: &[u8]) -> Result<(), JsonWriterError<W::Error>> {
        self.out.write_all(bytes).map_err(JsonWriterError::Write)
    }

    pub(crate) fn write_json_string(
        &mut self,
        value: &str,
    ) -> Result<(), JsonWriterError<W::Error>> {
        self.write_raw(b"\"")?;
        for ch in value.chars() {
            match ch {
                '"' => self.write_raw(br#"\""#)?,
                '\\' => self.write_raw(br#"\\"#)?,
                '\n' => self.write_raw(br#"\n"#)?,
                '\r' => self.write_raw(br#"\r"#)?,
                '\t' => self.write_raw(br#"\t"#)?,
                '\u{08}' => self.write_raw(br#"\b"#)?,
                '\u{0c}' => self.write_raw(br#"\f"#)?,
                ch if ch <= '\u{1f}' => {
                    self.write_raw(br#"\u00"#)?;
                    let n = ch as u8;
                    self.write_hex_nibble(n >> 4)?;
                    self.write_hex_nibble(n & 0x0f)?;
                }
                ch => {
                    let mut buf = [0u8; 4];
                    self.write_raw(ch.encode_utf8(&mut buf).as_bytes())?;
                }
            }
        }
        self.write_raw(b"\"")
    }

    fn write_display<T>(&mut self, value: T) -> Result<(), JsonWriterError<W::Error>>
    where
        T: fmt::Display,
    {
        struct Adapter<'a, W: JsonWrite>(&'a mut JsonWriter<W>);

        impl<W: JsonWrite> fmt::Write for Adapter<'_, W> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.write_raw(s.as_bytes()).map_err(|_| fmt::Error)
            }
        }

        fmt::write(&mut Adapter(self), format_args!("{value}"))
            .map_err(|_| JsonWriterError::Serialize)
    }

    fn write_hex_nibble(&mut self, nibble: u8) -> Result<(), JsonWriterError<W::Error>> {
        let byte = match nibble {
            0..=9 => b'0' + nibble,
            _ => b'a' + (nibble - 10),
        };
        self.write_raw(&[byte])
    }
}

/// Object scope for [`JsonWriter`].
pub struct JsonObject<'a, W>
where
    W: JsonWrite,
{
    writer: &'a mut JsonWriter<W>,
    first: bool,
    finished: bool,
}

impl<'a, W> JsonObject<'a, W>
where
    W: JsonWrite,
{
    fn new(writer: &'a mut JsonWriter<W>) -> Self {
        Self {
            writer,
            first: true,
            finished: false,
        }
    }

    pub fn prop(&mut self, name: &str) -> Result<JsonValue<'_, W>, JsonWriterError<W::Error>> {
        self.before_entry()?;
        self.writer.write_json_string(name)?;
        self.writer.write_raw(b":")?;
        Ok(JsonValue {
            writer: self.writer,
        })
    }

    pub fn finish(mut self) -> Result<(), JsonWriterError<W::Error>> {
        self.finished = true;
        self.writer.write_raw(b"}")
    }

    fn before_entry(&mut self) -> Result<(), JsonWriterError<W::Error>> {
        if self.first {
            self.first = false;
        } else {
            self.writer.write_raw(b",")?;
        }
        Ok(())
    }
}

/// Array scope for [`JsonWriter`].
pub struct JsonArray<'a, W>
where
    W: JsonWrite,
{
    writer: &'a mut JsonWriter<W>,
    first: bool,
    finished: bool,
}

impl<'a, W> JsonArray<'a, W>
where
    W: JsonWrite,
{
    fn new(writer: &'a mut JsonWriter<W>) -> Self {
        Self {
            writer,
            first: true,
            finished: false,
        }
    }

    pub fn item(&mut self) -> Result<JsonValue<'_, W>, JsonWriterError<W::Error>> {
        self.before_entry()?;
        Ok(JsonValue {
            writer: self.writer,
        })
    }

    pub fn finish(mut self) -> Result<(), JsonWriterError<W::Error>> {
        self.finished = true;
        self.writer.write_raw(b"]")
    }

    fn before_entry(&mut self) -> Result<(), JsonWriterError<W::Error>> {
        if self.first {
            self.first = false;
        } else {
            self.writer.write_raw(b",")?;
        }
        Ok(())
    }
}

/// A single property or array item value slot.
pub struct JsonValue<'a, W>
where
    W: JsonWrite,
{
    pub(crate) writer: &'a mut JsonWriter<W>,
}

impl<'a, W> JsonValue<'a, W>
where
    W: JsonWrite,
{
    pub fn object(self) -> Result<JsonObject<'a, W>, JsonWriterError<W::Error>> {
        self.writer.object()
    }

    pub fn array(self) -> Result<JsonArray<'a, W>, JsonWriterError<W::Error>> {
        self.writer.array()
    }

    pub fn null(self) -> Result<(), JsonWriterError<W::Error>> {
        self.writer.null()
    }

    pub fn bool(self, value: bool) -> Result<(), JsonWriterError<W::Error>> {
        self.writer.bool(value)
    }

    pub fn i64(self, value: i64) -> Result<(), JsonWriterError<W::Error>> {
        self.writer.i64(value)
    }

    pub fn u64(self, value: u64) -> Result<(), JsonWriterError<W::Error>> {
        self.writer.u64(value)
    }

    pub fn string(self, value: &str) -> Result<(), JsonWriterError<W::Error>> {
        self.writer.string(value)
    }

    pub fn serde<T>(self, value: &T) -> Result<(), JsonWriterError<W::Error>>
    where
        T: Serialize,
    {
        self.writer.serde(value)
    }

    pub fn raw_json(self, bytes: &[u8]) -> Result<(), JsonWriterError<W::Error>> {
        self.writer.write_raw(bytes)
    }
}

#[cfg(feature = "ser-write-json")]
fn write_serde<W, T>(writer: &mut JsonWriter<W>, value: &T) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
    T: Serialize,
{
    struct Adapter<'a, W: JsonWrite> {
        writer: &'a mut JsonWriter<W>,
    }

    impl<W: JsonWrite> ser_write_json::SerWrite for Adapter<'_, W> {
        type Error = fmt::Error;

        fn write(&mut self, buf: &[u8]) -> Result<(), fmt::Error> {
            if self.writer.out.write_all(buf).is_err() {
                return Err(fmt::Error);
            }
            Ok(())
        }
    }

    let mut adapter = Adapter { writer };
    ser_write_json::ser::to_writer(&mut adapter, value).map_err(|_| JsonWriterError::Serialize)
}

#[cfg(not(feature = "ser-write-json"))]
fn write_serde<W, T>(writer: &mut JsonWriter<W>, value: &T) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
    T: Serialize,
{
    let json = serde_json::to_string(value).map_err(|_| JsonWriterError::Serialize)?;
    writer.write_raw(json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::json_write::ChunkCountingWrite;
    use alloc::vec::Vec;
    use serde::Deserialize;

    #[test]
    fn json_writer_builds_nested_values_with_commas() {
        let mut bytes = Vec::new();
        let mut writer = JsonWriter::new(&mut bytes);
        let mut object = writer.object().unwrap();
        object.prop("name").unwrap().string("shader").unwrap();
        object.prop("revision").unwrap().i64(15).unwrap();
        let mut values = object.prop("values").unwrap().array().unwrap();
        values.item().unwrap().bool(true).unwrap();
        values.item().unwrap().null().unwrap();
        values.finish().unwrap();
        object.finish().unwrap();

        assert_eq!(
            core::str::from_utf8(&bytes).unwrap(),
            r#"{"name":"shader","revision":15,"values":[true,null]}"#
        );
    }

    #[test]
    fn json_writer_escapes_strings() {
        let mut bytes = Vec::new();
        let mut writer = JsonWriter::new(&mut bytes);
        writer.string("a\"b\\c\n\t\u{1f}").unwrap();

        assert_eq!(
            core::str::from_utf8(&bytes).unwrap(),
            r#""a\"b\\c\n\t\u001f""#
        );
    }

    #[test]
    fn json_writer_serde_bridge_emits_valid_json() {
        #[derive(Debug, Deserialize, PartialEq, serde::Serialize)]
        struct Sample {
            a: u32,
            b: bool,
        }

        let sample = Sample { a: 7, b: true };
        let mut bytes = Vec::new();
        JsonWriter::new(&mut bytes).serde(&sample).unwrap();
        let decoded: Sample = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(decoded, sample);
    }

    #[test]
    fn chunk_counting_writer_records_bounded_writes() {
        let mut out = ChunkCountingWrite::new(4);
        let mut writer = JsonWriter::new(&mut out);
        writer.string("abcdefghijkl").unwrap();

        assert_eq!(
            core::str::from_utf8(out.bytes()).unwrap(),
            r#""abcdefghijkl""#
        );
        assert!(out.chunk_count() > 1);
    }
}
