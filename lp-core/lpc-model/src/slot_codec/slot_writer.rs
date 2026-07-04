use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::convert::Infallible;
use core::fmt;
use lp_collection::VecMap;

use base64::Engine;

/// Byte sink used by the slot JSON writer.
///
/// This mirrors only the operation the slot codec needs, so embedded callers
/// can adapt bounded/chunked sinks without depending on `std::io::Write`.
pub trait SlotWrite {
    type Error;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

impl<T> SlotWrite for &mut T
where
    T: SlotWrite + ?Sized,
{
    type Error = T::Error;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        (**self).write_all(bytes)
    }
}

impl SlotWrite for Vec<u8> {
    type Error = Infallible;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

#[derive(Debug)]
pub enum SlotWriteError<E> {
    Write(E),
    InvalidSlotData(String),
    Serialize,
}

impl<E: fmt::Display> fmt::Display for SlotWriteError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write(error) => write!(f, "{error}"),
            Self::InvalidSlotData(error) => f.write_str(error),
            Self::Serialize => f.write_str("slot JSON serialization failed"),
        }
    }
}

/// Slot-native JSON writer facade.
///
/// Compact by default (wire format). [`SlotWriter::new_pretty`] enables the
/// authored-file style: 2-space indentation, one entry per line, `": "` after
/// prop names. Output is deterministic either way — entries follow slot-shape
/// declaration order upstream, so identical models serialize byte-identically.
pub struct SlotWriter<W>
where
    W: SlotWrite,
{
    out: W,
    pretty: bool,
    depth: usize,
}

impl<W> SlotWriter<W>
where
    W: SlotWrite,
{
    pub fn new(out: W) -> Self {
        Self {
            out,
            pretty: false,
            depth: 0,
        }
    }

    pub fn new_pretty(out: W) -> Self {
        Self {
            out,
            pretty: true,
            depth: 0,
        }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    pub fn value(&mut self) -> SlotValueWriter<'_, W> {
        SlotValueWriter { writer: self }
    }

    pub fn object(&mut self) -> Result<SlotObjectWriter<'_, W>, SlotWriteError<W::Error>> {
        self.write_raw(b"{")?;
        self.depth += 1;
        Ok(SlotObjectWriter {
            writer: self,
            first: true,
        })
    }

    fn array(&mut self) -> Result<SlotArrayWriter<'_, W>, SlotWriteError<W::Error>> {
        self.write_raw(b"[")?;
        self.depth += 1;
        Ok(SlotArrayWriter {
            writer: self,
            first: true,
        })
    }

    fn entry_break(&mut self) -> Result<(), SlotWriteError<W::Error>> {
        if !self.pretty {
            return Ok(());
        }
        self.write_raw(b"\n")?;
        for _ in 0..self.depth {
            self.write_raw(b"  ")?;
        }
        Ok(())
    }

    fn close_container(
        &mut self,
        close: &[u8],
        non_empty: bool,
    ) -> Result<(), SlotWriteError<W::Error>> {
        self.depth = self.depth.saturating_sub(1);
        if non_empty {
            self.entry_break()?;
        }
        self.write_raw(close)
    }

    fn write_raw(&mut self, bytes: &[u8]) -> Result<(), SlotWriteError<W::Error>> {
        self.out.write_all(bytes).map_err(SlotWriteError::Write)
    }

    fn write_json_string(&mut self, value: &str) -> Result<(), SlotWriteError<W::Error>> {
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

    fn write_display<T>(&mut self, value: T) -> Result<(), SlotWriteError<W::Error>>
    where
        T: fmt::Display,
    {
        struct Adapter<'a, W: SlotWrite>(&'a mut SlotWriter<W>);

        impl<W: SlotWrite> fmt::Write for Adapter<'_, W> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.write_raw(s.as_bytes()).map_err(|_| fmt::Error)
            }
        }

        fmt::write(&mut Adapter(self), format_args!("{value}"))
            .map_err(|_| SlotWriteError::Serialize)
    }

    fn write_hex_nibble(&mut self, nibble: u8) -> Result<(), SlotWriteError<W::Error>> {
        let byte = match nibble {
            0..=9 => b'0' + nibble,
            _ => b'a' + (nibble - 10),
        };
        self.write_raw(&[byte])
    }
}

pub struct SlotObjectWriter<'a, W>
where
    W: SlotWrite,
{
    writer: &'a mut SlotWriter<W>,
    first: bool,
}

impl<'a, W> SlotObjectWriter<'a, W>
where
    W: SlotWrite,
{
    pub fn prop(&mut self, name: &str) -> Result<SlotValueWriter<'_, W>, SlotWriteError<W::Error>> {
        self.before_entry()?;
        self.writer.write_json_string(name)?;
        self.writer.write_raw(b":")?;
        if self.writer.pretty {
            self.writer.write_raw(b" ")?;
        }
        Ok(SlotValueWriter {
            writer: self.writer,
        })
    }

    pub fn finish(self) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.close_container(b"}", !self.first)
    }

    fn before_entry(&mut self) -> Result<(), SlotWriteError<W::Error>> {
        if self.first {
            self.first = false;
        } else {
            self.writer.write_raw(b",")?;
        }
        self.writer.entry_break()
    }
}

pub struct SlotArrayWriter<'a, W>
where
    W: SlotWrite,
{
    writer: &'a mut SlotWriter<W>,
    first: bool,
}

impl<'a, W> SlotArrayWriter<'a, W>
where
    W: SlotWrite,
{
    pub fn item(&mut self) -> Result<SlotValueWriter<'_, W>, SlotWriteError<W::Error>> {
        self.before_entry()?;
        Ok(SlotValueWriter {
            writer: self.writer,
        })
    }

    pub fn finish(self) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.close_container(b"]", !self.first)
    }

    fn before_entry(&mut self) -> Result<(), SlotWriteError<W::Error>> {
        if self.first {
            self.first = false;
        } else {
            self.writer.write_raw(b",")?;
        }
        self.writer.entry_break()
    }
}

pub struct SlotValueWriter<'a, W>
where
    W: SlotWrite,
{
    writer: &'a mut SlotWriter<W>,
}

impl<'a, W> SlotValueWriter<'a, W>
where
    W: SlotWrite,
{
    pub fn object(self) -> Result<SlotObjectWriter<'a, W>, SlotWriteError<W::Error>> {
        self.writer.object()
    }

    pub fn array(self) -> Result<SlotArrayWriter<'a, W>, SlotWriteError<W::Error>> {
        self.writer.array()
    }

    pub fn f32(self, value: f32) -> Result<(), SlotWriteError<W::Error>> {
        if !value.is_finite() {
            return Err(SlotWriteError::Serialize);
        }
        self.writer.write_display(value)
    }

    pub fn u32(self, value: u32) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.write_display(value)
    }

    pub fn i32(self, value: i32) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.write_display(value)
    }

    pub fn i64(self, value: i64) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.write_display(value)
    }

    pub fn bool(self, value: bool) -> Result<(), SlotWriteError<W::Error>> {
        self.writer
            .write_raw(if value { b"true" } else { b"false" })
    }

    pub fn null(self) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.write_raw(b"null")
    }

    pub fn string(self, value: &str) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.write_json_string(value)
    }

    pub fn binary_base64_tuple(self, bytes: &[u8]) -> Result<(), SlotWriteError<W::Error>> {
        let mut array = self.array()?;
        array.item()?.u32(bytes.len() as u32)?;
        array.item()?.base64_string(bytes)?;
        array.finish()
    }

    pub fn string_key_map<T>(
        self,
        map: &VecMap<String, T>,
        mut write_value: impl FnMut(SlotValueWriter<'_, W>, &T) -> Result<(), SlotWriteError<W::Error>>,
    ) -> Result<(), SlotWriteError<W::Error>> {
        let mut object = self.object()?;
        for (key, entry) in map {
            write_value(object.prop(key)?, entry)?;
        }
        object.finish()
    }

    pub fn u32_key_map<T>(
        self,
        map: &VecMap<u32, T>,
        mut write_value: impl FnMut(SlotValueWriter<'_, W>, &T) -> Result<(), SlotWriteError<W::Error>>,
    ) -> Result<(), SlotWriteError<W::Error>> {
        let mut object = self.object()?;
        for (key, entry) in map {
            write_value(object.prop(&key.to_string())?, entry)?;
        }
        object.finish()
    }

    pub fn f32_array<const N: usize>(
        self,
        values: &[f32; N],
    ) -> Result<(), SlotWriteError<W::Error>> {
        let mut array = self.array()?;
        for value in values {
            array.item()?.f32(*value)?;
        }
        array.finish()
    }

    fn base64_string(self, bytes: &[u8]) -> Result<(), SlotWriteError<W::Error>> {
        self.writer.write_raw(b"\"")?;
        let engine = base64::engine::general_purpose::STANDARD;
        let mut encoded = [0u8; 4];

        for chunk in bytes.chunks(3) {
            let len = engine
                .encode_slice(chunk, &mut encoded)
                .map_err(|_| SlotWriteError::Serialize)?;
            self.writer.write_raw(&encoded[..len])?;
        }

        self.writer.write_raw(b"\"")
    }
}
