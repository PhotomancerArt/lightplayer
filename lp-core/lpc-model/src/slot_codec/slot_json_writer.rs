use alloc::vec::Vec;
use core::convert::Infallible;
use core::fmt;

use base64::Engine;

/// Byte sink used by the slot JSON writer.
///
/// This mirrors only the operation the slot codec needs, so embedded callers
/// can adapt bounded/chunked sinks without depending on `std::io::Write`.
pub trait SlotJsonWrite {
    type Error;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

impl<T> SlotJsonWrite for &mut T
where
    T: SlotJsonWrite + ?Sized,
{
    type Error = T::Error;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        (**self).write_all(bytes)
    }
}

impl SlotJsonWrite for Vec<u8> {
    type Error = Infallible;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

#[derive(Debug)]
pub enum SlotJsonWriterError<E> {
    Write(E),
    Serialize,
}

impl<E: fmt::Display> fmt::Display for SlotJsonWriterError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write(error) => write!(f, "{error}"),
            Self::Serialize => f.write_str("slot JSON serialization failed"),
        }
    }
}

/// Slot-native JSON writer facade.
pub struct SlotJsonWriter<W>
where
    W: SlotJsonWrite,
{
    out: W,
}

impl<W> SlotJsonWriter<W>
where
    W: SlotJsonWrite,
{
    pub fn new(out: W) -> Self {
        Self { out }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    pub fn object(&mut self) -> Result<SlotJsonObject<'_, W>, SlotJsonWriterError<W::Error>> {
        self.write_raw(b"{")?;
        Ok(SlotJsonObject {
            writer: self,
            first: true,
        })
    }

    fn array(&mut self) -> Result<SlotJsonArray<'_, W>, SlotJsonWriterError<W::Error>> {
        self.write_raw(b"[")?;
        Ok(SlotJsonArray {
            writer: self,
            first: true,
        })
    }

    fn write_raw(&mut self, bytes: &[u8]) -> Result<(), SlotJsonWriterError<W::Error>> {
        self.out
            .write_all(bytes)
            .map_err(SlotJsonWriterError::Write)
    }

    fn write_json_string(&mut self, value: &str) -> Result<(), SlotJsonWriterError<W::Error>> {
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

    fn write_display<T>(&mut self, value: T) -> Result<(), SlotJsonWriterError<W::Error>>
    where
        T: fmt::Display,
    {
        struct Adapter<'a, W: SlotJsonWrite>(&'a mut SlotJsonWriter<W>);

        impl<W: SlotJsonWrite> fmt::Write for Adapter<'_, W> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.write_raw(s.as_bytes()).map_err(|_| fmt::Error)
            }
        }

        fmt::write(&mut Adapter(self), format_args!("{value}"))
            .map_err(|_| SlotJsonWriterError::Serialize)
    }

    fn write_hex_nibble(&mut self, nibble: u8) -> Result<(), SlotJsonWriterError<W::Error>> {
        let byte = match nibble {
            0..=9 => b'0' + nibble,
            _ => b'a' + (nibble - 10),
        };
        self.write_raw(&[byte])
    }
}

pub struct SlotJsonObject<'a, W>
where
    W: SlotJsonWrite,
{
    writer: &'a mut SlotJsonWriter<W>,
    first: bool,
}

impl<'a, W> SlotJsonObject<'a, W>
where
    W: SlotJsonWrite,
{
    pub fn prop(
        &mut self,
        name: &str,
    ) -> Result<SlotJsonValue<'_, W>, SlotJsonWriterError<W::Error>> {
        self.before_entry()?;
        self.writer.write_json_string(name)?;
        self.writer.write_raw(b":")?;
        Ok(SlotJsonValue {
            writer: self.writer,
        })
    }

    pub fn finish(self) -> Result<(), SlotJsonWriterError<W::Error>> {
        self.writer.write_raw(b"}")
    }

    fn before_entry(&mut self) -> Result<(), SlotJsonWriterError<W::Error>> {
        if self.first {
            self.first = false;
        } else {
            self.writer.write_raw(b",")?;
        }
        Ok(())
    }
}

pub struct SlotJsonArray<'a, W>
where
    W: SlotJsonWrite,
{
    writer: &'a mut SlotJsonWriter<W>,
    first: bool,
}

impl<'a, W> SlotJsonArray<'a, W>
where
    W: SlotJsonWrite,
{
    pub fn item(&mut self) -> Result<SlotJsonValue<'_, W>, SlotJsonWriterError<W::Error>> {
        self.before_entry()?;
        Ok(SlotJsonValue {
            writer: self.writer,
        })
    }

    pub fn finish(self) -> Result<(), SlotJsonWriterError<W::Error>> {
        self.writer.write_raw(b"]")
    }

    fn before_entry(&mut self) -> Result<(), SlotJsonWriterError<W::Error>> {
        if self.first {
            self.first = false;
        } else {
            self.writer.write_raw(b",")?;
        }
        Ok(())
    }
}

pub struct SlotJsonValue<'a, W>
where
    W: SlotJsonWrite,
{
    writer: &'a mut SlotJsonWriter<W>,
}

impl<'a, W> SlotJsonValue<'a, W>
where
    W: SlotJsonWrite,
{
    pub fn object(self) -> Result<SlotJsonObject<'a, W>, SlotJsonWriterError<W::Error>> {
        self.writer.object()
    }

    pub fn array(self) -> Result<SlotJsonArray<'a, W>, SlotJsonWriterError<W::Error>> {
        self.writer.array()
    }

    pub fn f32(self, value: f32) -> Result<(), SlotJsonWriterError<W::Error>> {
        if !value.is_finite() {
            return Err(SlotJsonWriterError::Serialize);
        }
        self.writer.write_display(value)
    }

    pub fn u32(self, value: u32) -> Result<(), SlotJsonWriterError<W::Error>> {
        self.writer.write_display(value)
    }

    pub fn bool(self, value: bool) -> Result<(), SlotJsonWriterError<W::Error>> {
        self.writer
            .write_raw(if value { b"true" } else { b"false" })
    }

    pub fn string(self, value: &str) -> Result<(), SlotJsonWriterError<W::Error>> {
        self.writer.write_json_string(value)
    }

    pub fn binary_base64_tuple(self, bytes: &[u8]) -> Result<(), SlotJsonWriterError<W::Error>> {
        let mut array = self.array()?;
        array.item()?.u32(bytes.len() as u32)?;
        array.item()?.base64_string(bytes)?;
        array.finish()
    }

    fn base64_string(self, bytes: &[u8]) -> Result<(), SlotJsonWriterError<W::Error>> {
        self.writer.write_raw(b"\"")?;
        let engine = base64::engine::general_purpose::STANDARD;
        let mut encoded = [0u8; 4];

        for chunk in bytes.chunks(3) {
            let len = engine
                .encode_slice(chunk, &mut encoded)
                .map_err(|_| SlotJsonWriterError::Serialize)?;
            self.writer.write_raw(&encoded[..len])?;
        }

        self.writer.write_raw(b"\"")
    }
}
