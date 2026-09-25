//! JSON compact serde serializer for `ser-write`
use core::fmt;
use core::marker::PhantomData;
use core::mem::MaybeUninit;

#[cfg(feature = "std")]
use std::{
    string::{String, ToString},
    vec::Vec,
};

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::SerWrite;
use ser_write::Token;
use serde::{ser, Serialize};

/// JSON serializer serializing bytes to an array of numbers
pub type SerializerByteArray<W> = Serializer<W, ArrayByteEncoder>;
/// JSON serializer serializing bytes to a HEX-encoded string
pub type SerializerByteHexStr<W> = Serializer<W, HexStrByteEncoder>;
/// JSON serializer serializing bytes to a Base-64 string
pub type SerializerByteBase64<W> = Serializer<W, Base64ByteEncoder>;
/// JSON serializer passing bytes through
pub type SerializerBytePass<W> = Serializer<W, PassThroughByteEncoder>;

/// Serde JSON serializer.
///
/// `W` - should implement [`SerWrite`] and `B` - [`ByteEncoder`].
///
/// `ByteEncoder` determines [`ser::Serializer::serialize_bytes`] implementation.
pub struct Serializer<W, B> {
    output: W,
    format: PhantomData<B>,
}

const SERDE_JSON_RAW_VALUE_TOKEN: &str = "$serde_json::private::RawValue";

/// LP fork: the newtype-struct name that marks raw bytes whose text form is a
/// base64 string (the same trick as serde_json's `RawValue`).
///
/// For `serialize_newtype_struct(BLOB_MARKER, &inner)` the serializer offers
/// the sink [`Token::Blob`] with `inner`'s bytes, which it reads by
/// serializing `inner` into a serializer that is **not** human-readable:
/// there, `inner` must call `serialize_bytes`. If the sink declines, `inner`
/// is serialized as usual, which for a human-readable serializer must be the
/// base64 string. serde_json knows nothing of the marker and is transparent
/// to newtypes, so the text is the same on both serializers.
pub const BLOB_MARKER: &str = "$lp::blob";

/// Serialization error
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Error<E> {
    /// Underlying writer error
    Writer(E),
    /// Invalid type for a JSON object key
    InvalidKeyType,
    #[cfg(any(feature = "std", feature = "alloc"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
    /// Error encoding UTF-8 string with pass-through bytes encoder
    Utf8Encode,
    /// Error formatting a collected string
    FormatError,
    #[cfg(any(feature = "std", feature = "alloc"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
    /// An error passed down from a [`serde::ser::Serialize`] implementation
    SerializeError(String),
    #[cfg(not(any(feature = "std", feature = "alloc")))]
    SerializeError,
}

/// Serialization result
pub type Result<T, E> = core::result::Result<T, Error<E>>;

impl<E: fmt::Display + fmt::Debug> serde::de::StdError for Error<E> {}

impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Writer(err) => err.fmt(f),
            Error::InvalidKeyType => f.write_str("invalid JSON object key data type"),
            #[cfg(any(feature = "std", feature = "alloc"))]
            Error::Utf8Encode => f.write_str("error encoding JSON as UTF-8 string"),
            Error::FormatError => f.write_str("error while collecting a string"),
            #[cfg(any(feature = "std", feature = "alloc"))]
            Error::SerializeError(s) => write!(f, "{} while serializing JSON", s),
            #[cfg(not(any(feature = "std", feature = "alloc")))]
            Error::SerializeError => f.write_str("error while serializing JSON"),
        }
    }
}

#[cfg(any(feature = "std", feature = "alloc"))]
impl<E: fmt::Display + fmt::Debug> serde::ser::Error for Error<E> {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Error::SerializeError(msg.to_string())
    }
}

#[cfg(not(any(feature = "std", feature = "alloc")))]
impl<E: fmt::Display + fmt::Debug> serde::ser::Error for Error<E> {
    fn custom<T>(_msg: T) -> Self
    where
        T: fmt::Display,
    {
        Error::SerializeError
    }
}

impl<E> From<E> for Error<E> {
    fn from(err: E) -> Self {
        Error::Writer(err)
    }
}

/// Determine how raw byte data types are serialized
pub trait ByteEncoder: Sized {
    fn serialize_bytes<'a, W: SerWrite>(
        ser: &'a mut Serializer<W, Self>,
        v: &[u8],
    ) -> Result<(), W::Error>
    where
        &'a mut Serializer<W, Self>: serde::ser::Serializer<Ok = (), Error = Error<W::Error>>;
}

/// Implements [`ByteEncoder::serialize_bytes`] serializing to an array of numbers
pub struct ArrayByteEncoder;
/// Implements [`ByteEncoder::serialize_bytes`] serializing to a HEX string
pub struct HexStrByteEncoder;
/// Implements [`ByteEncoder::serialize_bytes`] serializing to a Base-64 string
pub struct Base64ByteEncoder;
/// Implements [`ByteEncoder::serialize_bytes`] passing bytes through
pub struct PassThroughByteEncoder;

impl ByteEncoder for ArrayByteEncoder {
    fn serialize_bytes<'a, W: SerWrite>(
        ser: &'a mut Serializer<W, Self>,
        v: &[u8],
    ) -> Result<(), W::Error>
    where
        &'a mut Serializer<W, Self>: serde::ser::Serializer<Ok = (), Error = Error<W::Error>>,
    {
        use serde::ser::{SerializeSeq, Serializer};
        let mut seq = ser.serialize_seq(Some(v.len()))?;
        for byte in v {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }
}

impl ByteEncoder for HexStrByteEncoder {
    fn serialize_bytes<'a, W: SerWrite>(
        ser: &'a mut Serializer<W, Self>,
        v: &[u8],
    ) -> Result<(), W::Error>
    where
        &'a mut Serializer<W, Self>: serde::ser::Serializer<Ok = (), Error = Error<W::Error>>,
    {
        ser.writer().write_byte(b'"')?;
        ser.serialize_bytes_as_hex_str(v)?;
        Ok(ser.writer().write_byte(b'"')?)
    }
}

impl ByteEncoder for Base64ByteEncoder {
    fn serialize_bytes<'a, W: SerWrite>(
        ser: &'a mut Serializer<W, Self>,
        v: &[u8],
    ) -> Result<(), W::Error>
    where
        &'a mut Serializer<W, Self>: serde::ser::Serializer<Ok = (), Error = Error<W::Error>>,
    {
        ser.writer().write_byte(b'"')?;
        crate::base64::encode(ser.writer(), v)?;
        Ok(ser.writer().write_byte(b'"')?)
    }
}

impl ByteEncoder for PassThroughByteEncoder {
    fn serialize_bytes<'a, W: SerWrite>(
        ser: &'a mut Serializer<W, Self>,
        v: &[u8],
    ) -> Result<(), W::Error>
    where
        &'a mut Serializer<W, Self>: serde::ser::Serializer<Ok = (), Error = Error<W::Error>>,
    {
        Ok(ser.writer().write(v)?)
    }
}

#[cfg(any(feature = "std", feature = "alloc"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
pub fn to_string<T>(value: &T) -> Result<String, ser_write::SerError>
where
    T: Serialize + ?Sized,
{
    let mut vec = Vec::new();
    to_writer(&mut vec, value)?;
    // SAFETY: SerializerByteArray produce a valid UTF-8 output
    Ok(unsafe { String::from_utf8_unchecked(vec) })
}

#[cfg(any(feature = "std", feature = "alloc"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
pub fn to_string_hex_bytes<T>(value: &T) -> Result<String, ser_write::SerError>
where
    T: Serialize + ?Sized,
{
    let mut vec = Vec::new();
    to_writer_hex_bytes(&mut vec, value)?;
    // SAFETY: SerializerByteHexStr produce a valid UTF-8 output
    Ok(unsafe { String::from_utf8_unchecked(vec) })
}

#[cfg(any(feature = "std", feature = "alloc"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
pub fn to_string_base64_bytes<T>(value: &T) -> Result<String, ser_write::SerError>
where
    T: Serialize + ?Sized,
{
    let mut vec = Vec::new();
    to_writer_base64_bytes(&mut vec, value)?;
    // SAFETY: SerializerByteBase64 produce a valid UTF-8 output
    Ok(unsafe { String::from_utf8_unchecked(vec) })
}

#[cfg(any(feature = "std", feature = "alloc"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
pub fn to_string_pass_bytes<T>(value: &T) -> Result<String, ser_write::SerError>
where
    T: Serialize + ?Sized,
{
    let mut vec = Vec::new();
    to_writer_pass_bytes(&mut vec, value)?;
    String::from_utf8(vec).map_err(|_| Error::Utf8Encode)
}

/// Serialize `value` as JSON to a [`SerWrite`] implementation using a provided [`ByteEncoder`].
pub fn to_writer_with_encoder<B, W, T>(writer: W, value: &T) -> Result<(), W::Error>
where
    B: ByteEncoder,
    W: SerWrite,
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
    T: Serialize + ?Sized,
{
    let mut serializer = Serializer::<_, B>::new(writer);
    value.serialize(&mut serializer)
}

/// Serialize `value` as JSON to a [`SerWrite`] implementation.
///
/// Serialize bytes as arrays of numbers.
pub fn to_writer<W, T>(writer: W, value: &T) -> Result<(), W::Error>
where
    W: SerWrite,
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
    T: Serialize + ?Sized,
{
    to_writer_with_encoder::<ArrayByteEncoder, _, _>(writer, value)
}

/// Serialize `value` as JSON to a [`SerWrite`] implementation.
///
/// Serialize bytes as HEX-encoded strings.
pub fn to_writer_hex_bytes<W, T>(writer: W, value: &T) -> Result<(), W::Error>
where
    W: SerWrite,
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
    T: Serialize + ?Sized,
{
    to_writer_with_encoder::<HexStrByteEncoder, _, _>(writer, value)
}

/// Serialize `value` as JSON to a [`SerWrite`] implementation.
///
/// Serialize bytes as Base-64 strings.
pub fn to_writer_base64_bytes<W, T>(writer: W, value: &T) -> Result<(), W::Error>
where
    W: SerWrite,
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
    T: Serialize + ?Sized,
{
    to_writer_with_encoder::<Base64ByteEncoder, _, _>(writer, value)
}

/// Serialize `value` as JSON to a [`SerWrite`] implementation.
///
/// Serialize bytes passing them through.
/// The notion here is that byte arrays can hold already serialized JSON fragments.
///
/// **NOTE**: the content of the serialized bytes may impact the validity of the produced JSON!
pub fn to_writer_pass_bytes<W, T>(writer: W, value: &T) -> Result<(), W::Error>
where
    W: SerWrite,
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
    T: Serialize + ?Sized,
{
    to_writer_with_encoder::<PassThroughByteEncoder, _, _>(writer, value)
}

impl<W, B> Serializer<W, B> {
    /// Create a new `Serializer` with the given `output` object that should
    /// implement [`SerWrite`].
    #[inline(always)]
    pub fn new(output: W) -> Self {
        Serializer {
            output,
            format: PhantomData,
        }
    }
    /// Destruct self returning the `output` object.
    #[inline(always)]
    pub fn into_inner(self) -> W {
        self.output
    }
    /// Provide access to the inner writer for implementors of [`ByteEncoder`] and more.
    #[inline(always)]
    pub fn writer(&mut self) -> &mut W {
        &mut self.output
    }
}

impl<W: SerWrite, B> Serializer<W, B> {
    /// LP fork: offer `token` to the sink; write `json` if it declines.
    //
    // G-F3: the serializer's methods inline into every `Serialize` impl, so
    // what these three helpers inline is paid once per field of every wire
    // type. Measured 2026-09-23 on the C6 image (`just fw-esp32c6-size-check`)
    // and on a steady ~11.8 KB choker lens reply's JSON measure pass
    // (`minstret`, lp-emu:esp32c6:t2), against upstream's 2,414,432 B and
    // ~496 k instructions: `#[inline(always)]` +49,472 B and +7 %;
    // `#[inline(never)]` -4,688 B and +27 %. Out of line is the plan's ruling.
    #[inline(never)] // G-F3
    fn tok(&mut self, token: Token<'_>, json: &[u8]) -> Result<(), W::Error> {
        if !self.output.token(token)? {
            self.output.write(json)?;
        }
        Ok(())
    }

    /// LP fork: an object key, as a [`Token::Key`] or a quoted JSON string.
    #[inline(never)] // G-F3
    fn key_str(&mut self, key: &str) -> Result<(), W::Error> {
        if !self.output.token(Token::Key(key))? {
            self.output.write_byte(b'"')?;
            format_escaped_str_contents(&mut self.output, key)?;
            self.output.write_byte(b'"')?;
        }
        Ok(())
    }

    /// LP fork: one struct field's prefix, `,` (unless first) key `:`.
    #[inline(never)] // G-F3
    fn field_prefix(&mut self, first: bool, key: &str) -> Result<(), W::Error> {
        if !first {
            self.tok(Token::Separator, b",")?;
        }
        self.key_str(key)?;
        self.tok(Token::Separator, b":")
    }

    /// Serialize given slice of bytes as ASCII HEX nibbles
    pub fn serialize_bytes_as_hex_str(&mut self, v: &[u8]) -> Result<(), W::Error> {
        let writer = self.writer();
        for &byte in v.iter() {
            writer.write(&hex(byte))?;
        }
        Ok(())
    }
}

#[inline(always)]
fn hex_4bit(c: u8) -> u8 {
    if c <= 9 {
        0x30 + c
    } else {
        0x41 + (c - 10)
    }
}

/// Upper-case hex for value in 0..256, encoded as ASCII bytes
#[inline(always)]
fn hex(c: u8) -> [u8; 2] {
    [hex_4bit(c >> 4), hex_4bit(c & 0x0F)]
}

macro_rules! serialize_unsigned {
    ($self:ident, $N:expr, $v:expr) => {{
        let mut buf: [MaybeUninit<u8>; $N] =
            unsafe { MaybeUninit::<[MaybeUninit<u8>; $N]>::uninit().assume_init() };

        let mut v = $v;
        let mut i = $N - 1;
        loop {
            buf[i].write((v % 10) as u8 + b'0');
            v /= 10;

            if v == 0 {
                break;
            } else {
                i -= 1;
            }
        }

        // Note(feature): maybe_uninit_slice
        let buf = unsafe { &*(&buf[i..] as *const _ as *const [u8]) };
        Ok($self.output.write(buf)?)
    }};
}

macro_rules! serialize_signed {
    ($self:ident, $N:expr, $v:expr, $ixx:ident, $uxx:ident) => {{
        let v = $v;
        let (signed, mut v) = if v == $ixx::MIN {
            (true, $ixx::MAX as $uxx + 1)
        } else if v < 0 {
            (true, -v as $uxx)
        } else {
            (false, v as $uxx)
        };

        let mut buf: [MaybeUninit<u8>; $N] =
            unsafe { MaybeUninit::<[MaybeUninit<u8>; $N]>::uninit().assume_init() };
        let mut i = $N - 1;
        loop {
            buf[i].write((v % 10) as u8 + b'0');
            v /= 10;

            i -= 1;

            if v == 0 {
                break;
            }
        }

        if signed {
            buf[i].write(b'-');
        } else {
            i += 1;
        }

        // Note(feature): maybe_uninit_slice
        let buf = unsafe { &*(&buf[i..] as *const _ as *const [u8]) };

        Ok($self.output.write(buf)?)
    }};
}

macro_rules! serialize_ryu {
    ($self:ident, $v:expr) => {{
        let mut buffer = ryu_js::Buffer::new();
        let printed = buffer.format_finite($v);
        Ok($self.output.write_str(printed)?)
    }};
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::Serializer for &'a mut Serializer<W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    type SerializeSeq = SeqMapSerializer<'a, W, B>;
    type SerializeTuple = SeqMapSerializer<'a, W, B>;
    type SerializeTupleStruct = SeqMapSerializer<'a, W, B>;
    type SerializeTupleVariant = SeqMapSerializer<'a, W, B>;
    type SerializeMap = SeqMapSerializer<'a, W, B>;
    type SerializeStruct = SeqMapSerializer<'a, W, B>;
    type SerializeStructVariant = SeqMapSerializer<'a, W, B>;

    fn serialize_bool(self, v: bool) -> Result<(), W::Error> {
        Ok(self.tok(Token::Bool(v), if v { b"true" } else { b"false" })?)
    }
    #[inline(always)]
    fn serialize_i8(self, v: i8) -> Result<(), W::Error> {
        self.serialize_i32(i32::from(v))
    }
    #[inline(always)]
    fn serialize_i16(self, v: i16) -> Result<(), W::Error> {
        self.serialize_i32(i32::from(v))
    }

    fn serialize_i32(self, v: i32) -> Result<(), W::Error> {
        if self.output.token(Token::I64(i64::from(v)))? {
            return Ok(());
        }
        // "-2147483648"
        serialize_signed!(self, 11, v, i32, u32)
    }

    fn serialize_i64(self, v: i64) -> Result<(), W::Error> {
        if self.output.token(Token::I64(v))? {
            return Ok(());
        }
        // "-9223372036854775808"
        serialize_signed!(self, 20, v, i64, u64)
    }
    #[inline(always)]
    fn serialize_u8(self, v: u8) -> Result<(), W::Error> {
        self.serialize_u32(u32::from(v))
    }
    #[inline(always)]
    fn serialize_u16(self, v: u16) -> Result<(), W::Error> {
        self.serialize_u32(u32::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, W::Error> {
        if self.output.token(Token::U64(u64::from(v)))? {
            return Ok(());
        }
        // "4294967295"
        serialize_unsigned!(self, 10, v)
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, W::Error> {
        if self.output.token(Token::U64(v))? {
            return Ok(());
        }
        // "18446744073709551615"
        serialize_unsigned!(self, 20, v)
    }

    fn serialize_f32(self, v: f32) -> Result<(), W::Error> {
        if v.is_finite() {
            if self.output.token(Token::F32(v))? {
                return Ok(());
            }
            serialize_ryu!(self, v)
        } else {
            self.serialize_none()
        }
    }

    fn serialize_f64(self, v: f64) -> Result<(), W::Error> {
        if v.is_finite() {
            if self.output.token(Token::F64(v))? {
                return Ok(());
            }
            serialize_ryu!(self, v)
        } else {
            self.serialize_none()
        }
    }

    fn serialize_char(self, v: char) -> Result<(), W::Error> {
        let mut encoding_tmp = [0u8; 4];
        let encoded = v.encode_utf8(&mut encoding_tmp);
        self.serialize_str(encoded)
    }

    fn serialize_str(self, v: &str) -> Result<(), W::Error> {
        if self.output.token(Token::Str(v))? {
            return Ok(());
        }
        self.output.write_byte(b'"')?;
        format_escaped_str_contents(&mut self.output, v)?;
        Ok(self.output.write_byte(b'"')?)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<(), W::Error> {
        B::serialize_bytes(self, v)
    }

    fn serialize_none(self) -> Result<(), W::Error> {
        Ok(self.tok(Token::Null, b"null")?)
    }

    fn serialize_some<T>(self, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), W::Error> {
        self.serialize_none()
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), W::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<(), W::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        if name == BLOB_MARKER && value.serialize(BlobOffer { ser: &mut *self })? {
            return Ok(());
        }
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        self.tok(Token::MapBegin, b"{")?;
        self.key_str(variant)?;
        self.tok(Token::Separator, b":")?;
        value.serialize(&mut *self)?;
        Ok(self.tok(Token::MapEnd, b"}")?)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, W::Error> {
        self.tok(Token::SeqBegin, b"[")?;
        Ok(SeqMapSerializer {
            first: true,
            raw_value: false,
            ser: self,
        })
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, W::Error> {
        self.serialize_seq(None)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, W::Error> {
        self.serialize_seq(None)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, W::Error> {
        self.tok(Token::MapBegin, b"{")?;
        self.key_str(variant)?;
        self.tok(Token::Separator, b":")?;
        self.tok(Token::SeqBegin, b"[")?;
        Ok(SeqMapSerializer {
            first: true,
            raw_value: false,
            ser: self,
        })
    }

    // Maps are represented in JSON as `{ K: V, K: V, ... }`.
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, W::Error> {
        self.tok(Token::MapBegin, b"{")?;
        Ok(SeqMapSerializer {
            first: true,
            raw_value: false,
            ser: self,
        })
    }

    fn serialize_struct(
        self,
        name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, W::Error> {
        if name == SERDE_JSON_RAW_VALUE_TOKEN {
            return Ok(SeqMapSerializer {
                first: true,
                raw_value: true,
                ser: self,
            });
        }
        self.serialize_map(None)
    }

    // Struct variants are represented in JSON as `{ NAME: { K: V, ... } }`.
    // This is the externally tagged representation.
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, W::Error> {
        self.tok(Token::MapBegin, b"{")?;
        self.key_str(variant)?;
        self.tok(Token::Separator, b":")?;
        self.tok(Token::MapBegin, b"{")?;
        Ok(SeqMapSerializer {
            first: true,
            raw_value: false,
            ser: self,
        })
    }

    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, W::Error>
    where
        T: fmt::Display + ?Sized,
    {
        self.output.write_byte(b'"')?;
        let mut col = StringCollector::new(&mut self.output);
        fmt::write(&mut col, format_args!("{}", value)).map_err(|_| Error::FormatError)?;
        Ok(self.output.write_byte(b'"')?)
    }
}

/// Object key serializer
struct KeySer<'a, W, B> {
    ser: &'a mut Serializer<W, B>,
}

impl<'a, W: SerWrite, B: ByteEncoder> KeySer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    /// LP fork: an integer key as its decimal digits, offered as a
    /// [`Token::Key`] or written as the same quoted digits as upstream.
    fn key_int(self, neg: bool, mut v: u64) -> Result<(), W::Error> {
        let mut buf = [0u8; 21];
        let mut i = buf.len();
        loop {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        if neg {
            i -= 1;
            buf[i] = b'-';
        }
        let text = core::str::from_utf8(&buf[i..]).map_err(|_| Error::FormatError)?;
        Ok(self.ser.key_str(text)?)
    }
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::Serializer for KeySer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    type SerializeSeq = SeqMapSerializer<'a, W, B>;
    type SerializeTuple = SeqMapSerializer<'a, W, B>;
    type SerializeTupleStruct = SeqMapSerializer<'a, W, B>;
    type SerializeTupleVariant = SeqMapSerializer<'a, W, B>;
    type SerializeMap = SeqMapSerializer<'a, W, B>;
    type SerializeStruct = SeqMapSerializer<'a, W, B>;
    type SerializeStructVariant = SeqMapSerializer<'a, W, B>;

    fn serialize_bool(self, v: bool) -> Result<(), W::Error> {
        Ok(self.ser.key_str(if v { "true" } else { "false" })?)
    }
    #[inline(always)]
    fn serialize_i8(self, v: i8) -> Result<(), W::Error> {
        self.key_int(v < 0, i64::from(v).unsigned_abs())
    }
    #[inline(always)]
    fn serialize_i16(self, v: i16) -> Result<(), W::Error> {
        self.key_int(v < 0, i64::from(v).unsigned_abs())
    }
    #[inline(always)]
    fn serialize_i32(self, v: i32) -> Result<(), W::Error> {
        self.key_int(v < 0, i64::from(v).unsigned_abs())
    }
    #[inline(always)]
    fn serialize_i64(self, v: i64) -> Result<(), W::Error> {
        self.key_int(v < 0, i64::from(v).unsigned_abs())
    }
    #[inline(always)]
    fn serialize_u8(self, v: u8) -> Result<(), W::Error> {
        self.key_int(false, u64::from(v))
    }
    #[inline(always)]
    fn serialize_u16(self, v: u16) -> Result<(), W::Error> {
        self.key_int(false, u64::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, W::Error> {
        self.key_int(false, u64::from(v))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, W::Error> {
        self.key_int(false, u64::from(v))
    }

    fn serialize_f32(self, _v: f32) -> Result<(), W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_f64(self, _v: f64) -> Result<(), W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_char(self, v: char) -> Result<(), W::Error> {
        let mut encoding_tmp = [0u8; 4];
        Ok(self.ser.key_str(v.encode_utf8(&mut encoding_tmp))?)
    }

    fn serialize_str(self, v: &str) -> Result<(), W::Error> {
        Ok(self.ser.key_str(v)?)
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<(), W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_none(self) -> Result<(), W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_some<T>(self, _value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::InvalidKeyType)
    }

    fn serialize_unit(self) -> Result<(), W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<(), W::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::InvalidKeyType)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, W::Error> {
        Err(Error::InvalidKeyType)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, W::Error> {
        Err(Error::InvalidKeyType)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, W::Error> {
        Err(Error::InvalidKeyType)
    }
    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, W::Error> {
        Err(Error::InvalidKeyType)
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, W::Error> {
        Err(Error::InvalidKeyType)
    }
    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, W::Error>
    where
        T: fmt::Display + ?Sized,
    {
        self.ser.collect_str(value)
    }
}

pub struct SeqMapSerializer<'a, W, B> {
    ser: &'a mut Serializer<W, B>,
    first: bool,
    raw_value: bool,
}

/// Strings written to this object using [`fmt::Write`] trait are written
/// to the underlying writer with characters escaped using JSON syntax for
/// strings.
///
/// This object is used internally by [`Serializer::collect_str`] method.
///
/// [`Serializer::collect_str`]: ser::Serializer::collect_str
pub struct StringCollector<'a, W> {
    output: &'a mut W,
}

struct RawValueStrEmitter<'a, W, B> {
    ser: &'a mut Serializer<W, B>,
}

impl<'a, W> StringCollector<'a, W> {
    /// Create a new `StringCollector` with the given `output` object that
    /// should implement [`SerWrite`].
    #[inline(always)]
    pub fn new(output: &'a mut W) -> Self {
        Self { output }
    }
}

impl<'a, W: SerWrite> fmt::Write for StringCollector<'a, W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        format_escaped_str_contents(self.output, s).map_err(|_| fmt::Error)
    }
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::Serializer for RawValueStrEmitter<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    type SerializeSeq = ser::Impossible<(), Error<W::Error>>;
    type SerializeTuple = ser::Impossible<(), Error<W::Error>>;
    type SerializeTupleStruct = ser::Impossible<(), Error<W::Error>>;
    type SerializeTupleVariant = ser::Impossible<(), Error<W::Error>>;
    type SerializeMap = ser::Impossible<(), Error<W::Error>>;
    type SerializeStruct = ser::Impossible<(), Error<W::Error>>;
    type SerializeStructVariant = ser::Impossible<(), Error<W::Error>>;

    fn serialize_bool(self, _v: bool) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_i8(self, _v: i8) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_i16(self, _v: i16) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_i32(self, _v: i32) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_i64(self, _v: i64) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_u8(self, _v: u8) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_u16(self, _v: u16) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_u32(self, _v: u32) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_u64(self, _v: u64) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_f32(self, _v: f32) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_f64(self, _v: f64) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_char(self, _v: char) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_str(self, value: &str) -> Result<(), W::Error> {
        Ok(self.ser.output.write(value.as_bytes())?)
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_none(self) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_some<T>(self, _value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_unit(self) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<(), W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, _value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, W::Error> {
        Err(ser::Error::custom("expected RawValue"))
    }
}

/// LP fork: reads a [`BLOB_MARKER`] newtype's bytes and offers them to the
/// sink as a [`Token::Blob`]. `Ok(true)` means the sink took them.
///
/// It is not human-readable, which is what tells the marker's inner value to
/// call `serialize_bytes` here. Any other scalar answers `Ok(false)` (not a
/// blob, serialize the value as usual); a compound value is an error, since
/// nothing but `lpc-wire`'s base64 wrapper uses the marker.
struct BlobOffer<'a, W, B> {
    ser: &'a mut Serializer<W, B>,
}

macro_rules! blob_offer_not_bytes {
    ($($method:ident($($arg:ty),*);)*) => {$(
        fn $method(self, $(_: $arg),*) -> Result<bool, W::Error> {
            Ok(false)
        }
    )*};
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::Serializer for BlobOffer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = bool;
    type Error = Error<W::Error>;

    type SerializeSeq = ser::Impossible<bool, Self::Error>;
    type SerializeTuple = ser::Impossible<bool, Self::Error>;
    type SerializeTupleStruct = ser::Impossible<bool, Self::Error>;
    type SerializeTupleVariant = ser::Impossible<bool, Self::Error>;
    type SerializeMap = ser::Impossible<bool, Self::Error>;
    type SerializeStruct = ser::Impossible<bool, Self::Error>;
    type SerializeStructVariant = ser::Impossible<bool, Self::Error>;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<bool, W::Error> {
        Ok(self.ser.output.token(Token::Blob(v))?)
    }

    blob_offer_not_bytes! {
        serialize_bool(bool);
        serialize_i8(i8);
        serialize_i16(i16);
        serialize_i32(i32);
        serialize_i64(i64);
        serialize_u8(u8);
        serialize_u16(u16);
        serialize_u32(u32);
        serialize_u64(u64);
        serialize_f32(f32);
        serialize_f64(f64);
        serialize_char(char);
        serialize_str(&str);
        serialize_none();
        serialize_unit();
        serialize_unit_struct(&'static str);
        serialize_unit_variant(&'static str, u32, &'static str);
    }

    fn serialize_some<T>(self, _value: &T) -> Result<bool, W::Error>
    where
        T: ?Sized + Serialize,
    {
        Ok(false)
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, _value: &T) -> Result<bool, W::Error>
    where
        T: ?Sized + Serialize,
    {
        Ok(false)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<bool, W::Error>
    where
        T: ?Sized + Serialize,
    {
        Ok(false)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, W::Error> {
        Err(ser::Error::custom("expected blob bytes"))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, W::Error> {
        Err(ser::Error::custom("expected blob bytes"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, W::Error> {
        Err(ser::Error::custom("expected blob bytes"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, W::Error> {
        Err(ser::Error::custom("expected blob bytes"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, W::Error> {
        Err(ser::Error::custom("expected blob bytes"))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, W::Error> {
        Err(ser::Error::custom("expected blob bytes"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, W::Error> {
        Err(ser::Error::custom("expected blob bytes"))
    }

    fn collect_str<T>(self, _value: &T) -> Result<bool, W::Error>
    where
        T: fmt::Display + ?Sized,
    {
        Ok(false)
    }
}

// This impl is SerializeSeq so these methods are called after `serialize_seq`
// is called on the Serializer.
impl<'a, W: SerWrite, B: ByteEncoder> ser::SerializeSeq for SeqMapSerializer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        if self.first {
            self.first = false;
        } else {
            self.ser.tok(Token::Separator, b",")?;
        }
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), W::Error> {
        Ok(self.ser.tok(Token::SeqEnd, b"]")?)
    }
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::SerializeTuple for SeqMapSerializer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        if self.first {
            self.first = false;
        } else {
            self.ser.tok(Token::Separator, b",")?;
        }
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), W::Error> {
        Ok(self.ser.tok(Token::SeqEnd, b"]")?)
    }
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::SerializeTupleStruct for SeqMapSerializer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        if self.first {
            self.first = false;
        } else {
            self.ser.tok(Token::Separator, b",")?;
        }
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), W::Error> {
        Ok(self.ser.tok(Token::SeqEnd, b"]")?)
    }
}

// Tuple variants are a little different. { NAME: [ ... ]}
impl<'a, W: SerWrite, B: ByteEncoder> ser::SerializeTupleVariant for SeqMapSerializer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        if self.first {
            self.first = false;
        } else {
            self.ser.tok(Token::Separator, b",")?;
        }
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), W::Error> {
        self.ser.tok(Token::SeqEnd, b"]")?;
        Ok(self.ser.tok(Token::MapEnd, b"}")?)
    }
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::SerializeMap for SeqMapSerializer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    /// The Serde data model allows map keys to be any serializable type.
    /// JSON only allows string keys so the implementation below will produce invalid
    /// JSON if the key serializes as something other than a string.
    fn serialize_key<T>(&mut self, key: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        if self.first {
            self.first = false;
        } else {
            self.ser.tok(Token::Separator, b",")?;
        }
        key.serialize(KeySer { ser: self.ser })
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        self.ser.tok(Token::Separator, b":")?;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), W::Error> {
        Ok(self.ser.tok(Token::MapEnd, b"}")?)
    }
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::SerializeStruct for SeqMapSerializer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        if self.raw_value {
            if !self.first || key != SERDE_JSON_RAW_VALUE_TOKEN {
                return Err(ser::Error::custom("expected RawValue"));
            }
            self.first = false;
            return value.serialize(RawValueStrEmitter { ser: self.ser });
        }

        let first = core::mem::replace(&mut self.first, false);
        self.ser.field_prefix(first, key)?;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), W::Error> {
        if self.raw_value {
            return Ok(());
        }
        Ok(self.ser.tok(Token::MapEnd, b"}")?)
    }
}

impl<'a, W: SerWrite, B: ByteEncoder> ser::SerializeStructVariant for SeqMapSerializer<'a, W, B>
where
    <W as SerWrite>::Error: fmt::Display + fmt::Debug,
{
    type Ok = ();
    type Error = Error<W::Error>;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), W::Error>
    where
        T: ?Sized + Serialize,
    {
        let first = core::mem::replace(&mut self.first, false);
        self.ser.field_prefix(first, key)?;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), W::Error> {
        self.ser.tok(Token::MapEnd, b"}")?;
        Ok(self.ser.tok(Token::MapEnd, b"}")?)
    }
}

fn format_escaped_str_contents<W>(writer: &mut W, value: &str) -> Result<(), W::Error>
where
    W: ?Sized + SerWrite,
{
    let bytes = value.as_bytes();

    let mut start = 0;

    for (i, &byte) in bytes.iter().enumerate() {
        let escape = match byte {
            0x00..=0x1F => ESCAPE[byte as usize],
            QU | BS => byte,
            _ => continue,
        };

        if start < i {
            writer.write_str(&value[start..i])?;
        }

        if escape == UU {
            writer.write(b"\\u00")?;
            writer.write(&hex(byte))?;
        } else {
            writer.write(&[b'\\', escape])?;
        }

        start = i + 1;
    }

    if start == bytes.len() {
        return Ok(());
    }

    Ok(writer.write_str(&value[start..])?)
}

const BB: u8 = b'b'; // \x08
const TT: u8 = b't'; // \x09
const NN: u8 = b'n'; // \x0A
const FF: u8 = b'f'; // \x0C
const RR: u8 = b'r'; // \x0D
const QU: u8 = b'"'; // \x22
const BS: u8 = b'\\'; // \x5C
const UU: u8 = b'u'; // \x00...\x1F except the ones above

// Lookup table of escape sequences. A value of b'x' at index i means that byte
// i is escaped as "\x" in JSON.
static ESCAPE: [u8; 32] = [
    //   1   2   3   4   5   6   7   8   9   A   B   C   D   E   F
    UU, UU, UU, UU, UU, UU, UU, UU, BB, TT, NN, UU, FF, RR, UU, UU, // 0
    UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, // 1
];

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use std::{collections::BTreeMap, format, vec};

    #[cfg(all(feature = "alloc", not(feature = "std")))]
    use alloc::{collections::BTreeMap, format, vec};

    use super::*;
    use crate::ser_write::{SerError, SliceWriter};

    fn to_str<'a, T>(buf: &'a mut [u8], value: &T) -> Result<&'a str, SerError>
    where
        T: Serialize + ?Sized,
    {
        let mut writer = SliceWriter::new(buf);
        to_writer(&mut writer, value)?;
        Ok(core::str::from_utf8(writer.split().0).unwrap())
    }

    fn to_str_hex_bytes<'a, T>(buf: &'a mut [u8], value: &T) -> Result<&'a str, SerError>
    where
        T: Serialize + ?Sized,
    {
        let mut writer = SliceWriter::new(buf);
        to_writer_hex_bytes(&mut writer, value)?;
        Ok(core::str::from_utf8(writer.split().0).unwrap())
    }

    fn to_str_base64_bytes<'a, T>(buf: &'a mut [u8], value: &T) -> Result<&'a str, SerError>
    where
        T: Serialize + ?Sized,
    {
        let mut writer = SliceWriter::new(buf);
        to_writer_base64_bytes(&mut writer, value)?;
        Ok(core::str::from_utf8(writer.split().0).unwrap())
    }

    fn to_str_pass_bytes<'a, T>(buf: &'a mut [u8], value: &T) -> Result<&'a str, SerError>
    where
        T: Serialize + ?Sized,
    {
        let mut writer = SliceWriter::new(buf);
        to_writer_pass_bytes(&mut writer, value)?;
        Ok(core::str::from_utf8(writer.split().0).unwrap())
    }

    #[test]
    fn test_json_serializer() {
        let mut buf = [0u8; 1];
        let writer = SliceWriter::new(&mut buf);
        let ser = SerializerByteArray::new(writer);
        let mut writer: SliceWriter = ser.into_inner();
        assert_eq!(writer.write_byte(0), Ok(()));
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_json_tuple() {
        #[derive(Serialize)]
        struct Test {
            int: u32,
            seq: Vec<&'static str>,
        }

        let test = Test {
            int: 1,
            seq: vec!["a", "b"],
        };
        let expected = r#"[100000,"bam!",0.4,{"int":1,"seq":["a","b"]},null]"#;
        let tup = (100000u64, "bam!", 0.4f64, test, 0.0f64 / 0.0);
        assert_eq!(to_string(&tup).unwrap(), expected);
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_json_struct() {
        #[derive(Serialize)]
        struct Test {
            int: u32,
            seq: Vec<&'static str>,
        }

        let test = Test {
            int: 1,
            seq: vec!["a", "b"],
        };
        let expected = r#"{"int":1,"seq":["a","b"]}"#;
        assert_eq!(to_string(&test).unwrap(), expected);
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_json_struct_to_array() {
        use serde::ser::SerializeSeq;
        struct Test {
            int: u32,
            seq: Vec<&'static str>,
        }
        impl serde::Serialize for Test {
            fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let mut seq = serializer.serialize_seq(Some(2)).unwrap();
                seq.serialize_element(&self.int).unwrap();
                seq.serialize_element(&self.seq).unwrap();
                seq.end()
            }
        }
        let test = Test {
            int: 1,
            seq: vec!["a", "b"],
        };
        let expected = r#"[1,["a","b"]]"#;
        assert_eq!(to_string(&test).unwrap(), expected);
    }

    #[test]
    fn test_json_enum() {
        #[derive(Serialize)]
        enum E {
            Unit,
            Newtype(u32),
            Tuple(u32, f32),
            Struct { a: u32 },
        }

        let mut buf = [0u8; 23];
        let mut writer = SliceWriter::new(&mut buf);

        let u = E::Unit;
        let expected = br#""Unit""#;
        to_writer(&mut writer, &u).unwrap();
        assert_eq!(writer.as_ref(), expected);

        let n = E::Newtype(1);
        let expected = br#"{"Newtype":1}"#;
        writer.clear();
        to_writer(&mut writer, &n).unwrap();
        assert_eq!(writer.as_ref(), expected);

        let t = E::Tuple(1, core::f32::consts::PI);
        let expected = br#"{"Tuple":[1,3.1415927]}"#;
        writer.clear();
        to_writer(&mut writer, &t).unwrap();
        assert_eq!(writer.as_ref(), expected);

        let s = E::Struct { a: 1 };
        let expected = br#"{"Struct":{"a":1}}"#;
        writer.clear();
        to_writer(&mut writer, &s).unwrap();
        assert_eq!(writer.as_ref(), expected);
    }

    #[test]
    fn test_json_string() {
        let mut buf = [0u8; 39];
        let mut writer = SliceWriter::new(&mut buf);

        let s = "\"\x00\x08\x09\n\x0C\rłączka\x1f\\\x7f\"";
        let expected = "\"\\\"\\u0000\\b\\t\\n\\f\\rłączka\\u001F\\\\\x7f\\\"\"";
        to_writer(&mut writer, &s).unwrap();
        assert_eq!(writer.as_ref(), expected.as_bytes());
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_json_bytes_owned() {
        #[derive(Serialize)]
        struct Test {
            #[serde(with = "serde_bytes")]
            key: Vec<u8>,
        }
        let expected = r#"[{"key":{"Struct":{"a":1}}}]"#;
        let value = [Test {
            key: r#"{"Struct":{"a":1}}"#.as_bytes().into(),
        }];
        assert_eq!(to_string_pass_bytes(&value).unwrap(), expected);
        let expected = r#"[{"key":"7B22537472756374223A7B2261223A317D7D"}]"#;
        assert_eq!(&to_string_hex_bytes(&value).unwrap(), expected);
        let expected = r#"[{"key":"eyJTdHJ1Y3QiOnsiYSI6MX19"}]"#;
        assert_eq!(&to_string_base64_bytes(&value).unwrap(), expected);
        let expected =
            r#"[{"key":[123,34,83,116,114,117,99,116,34,58,123,34,97,34,58,49,125,125]}]"#;
        assert_eq!(&to_string(&value).unwrap(), expected);
    }

    #[test]
    fn test_json_bytes() {
        #[derive(Serialize)]
        struct Test<'a> {
            #[serde(with = "serde_bytes")]
            key: &'a [u8],
        }
        let mut buf = [0u8; 73];
        let expected = r#"[{"key":{"Struct":{"a":1}}}]"#;
        let value = [Test {
            key: r#"{"Struct":{"a":1}}"#.as_bytes(),
        }];
        assert_eq!(to_str_pass_bytes(&mut buf, &value).unwrap(), expected);
        let expected = r#"[{"key":"7B22537472756374223A7B2261223A317D7D"}]"#;
        assert_eq!(to_str_hex_bytes(&mut buf, &value).unwrap(), expected);
        let expected = r#"[{"key":"eyJTdHJ1Y3QiOnsiYSI6MX19"}]"#;
        assert_eq!(to_str_base64_bytes(&mut buf, &value).unwrap(), expected);
        let expected =
            r#"[{"key":[123,34,83,116,114,117,99,116,34,58,123,34,97,34,58,49,125,125]}]"#;
        assert_eq!(to_str(&mut buf, &value).unwrap(), expected);
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_json_map() {
        #[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
        struct Wrap(bool);
        let mut buf = [0u8; 68];
        macro_rules! test_map_with_key_int {
            ($($ty:ty),*) => {$(
                let mut amap = BTreeMap::<$ty,&str>::new();
                let expected = r#"{}"#;
                assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
                amap.insert(1, "one");
                let expected = r#"{"1":"one"}"#;
                assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
                amap.insert(<$ty>::MIN, "min");
                let expected = format!(r#"{{"{}":"min","1":"one"}}"#, <$ty>::MIN);
                assert_eq!(to_str(&mut buf, &amap).unwrap(), &expected);
                amap.insert(<$ty>::MAX, "max");
                let expected = format!(r#"{{"{}":"min","1":"one","{}":"max"}}"#, <$ty>::MIN, <$ty>::MAX);
                assert_eq!(to_str(&mut buf, &amap).unwrap(), &expected);
            )*};
        }
        test_map_with_key_int!(i8, u8, i16, u16, i32, u32, i64, u64);
        let mut amap = BTreeMap::<&str, i32>::new();
        amap.insert("key", 118);
        let expected = r#"{"key":118}"#;
        assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
        let mut amap = BTreeMap::<char, [i8; 2]>::new();
        amap.insert('ℝ', [-128, 127]);
        let expected = r#"{"ℝ":[-128,127]}"#;
        assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
        let mut amap = BTreeMap::<bool, &str>::new();
        amap.insert(false, "");
        let expected = r#"{"false":""}"#;
        assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
        amap.insert(true, "1");
        let expected = r#"{"false":"","true":"1"}"#;
        assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
        let mut amap = BTreeMap::<Wrap, bool>::new();
        amap.insert(Wrap(true), false);
        let expected = r#"{"true":false}"#;
        assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
        #[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
        enum CKey {
            Foo,
            Bar,
        }
        let mut amap = BTreeMap::<CKey, char>::new();
        amap.insert(CKey::Foo, 'x');
        amap.insert(CKey::Bar, 'y');
        let expected = r#"{"Foo":"x","Bar":"y"}"#;
        assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
        #[derive(PartialEq, Eq, PartialOrd, Ord)]
        struct DecimalPoint(u32, u32);
        impl fmt::Display for DecimalPoint {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}.{}", &self.0, &self.1)
            }
        }
        impl serde::Serialize for DecimalPoint {
            fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.collect_str(self)
            }
        }
        let mut amap = BTreeMap::<DecimalPoint, char>::new();
        amap.insert(DecimalPoint(3, 14), 'x');
        let expected = r#"{"3.14":"x"}"#;
        assert_eq!(to_str(&mut buf, &amap).unwrap(), expected);
        let mut amap = BTreeMap::<[i32; 2], char>::new();
        amap.insert([1, 2], 'x');
        assert!(to_string(&amap).is_err());
        assert!(to_string_hex_bytes(&amap).is_err());
        assert!(to_string_base64_bytes(&amap).is_err());
        assert!(to_string_pass_bytes(&amap).is_err());
    }

    #[test]
    fn test_json_map_err() {
        use serde::ser::SerializeMap;
        struct PhonyMap<'a, K, V>(&'a [(K, V)]);
        impl<'a, K, V> serde::Serialize for PhonyMap<'a, K, V>
        where
            K: serde::Serialize,
            V: serde::Serialize,
        {
            fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let mut ma = serializer.serialize_map(None)?;
                for (k, v) in self.0.iter() {
                    ma.serialize_entry(k, v)?;
                }
                ma.end()
            }
        }

        let mut buf = [0u8; 9];

        let amap = PhonyMap(&[((), 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        #[derive(Serialize)]
        struct Key {
            key: i32,
        }
        let amap = PhonyMap(&[(Key { key: 0 }, 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let amap = PhonyMap(&[((1, 2), 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        #[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
        struct TKey(i32, u32);
        let amap = PhonyMap(&[(TKey(-1, 1), 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let amap: PhonyMap<Option<&str>, char> = PhonyMap(&[(None, 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let amap: PhonyMap<Option<&str>, char> = PhonyMap(&[(Some(""), 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        #[derive(Serialize)]
        struct Unit;
        let amap = PhonyMap(&[(Unit, 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        #[derive(Serialize)]
        enum EKey {
            A(i32),
        }
        let amap = PhonyMap(&[(EKey::A(-1), 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        #[derive(Serialize)]
        enum ETKey {
            A(i32, u32),
        }
        let amap = PhonyMap(&[(ETKey::A(-1, 1), 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        #[derive(Serialize)]
        enum ESKey {
            A { a: i32, b: u32 },
        }
        let amap = PhonyMap(&[(ESKey::A { a: -1, b: 1 }, 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        #[derive(Serialize)]
        struct Bytes<'a>(#[serde(with = "serde_bytes")] &'a [u8]);
        let amap = PhonyMap(&[(Bytes(b"_"), 'x')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let binding = [(&[1i32, 2][..], 'x')];
        let amap = PhonyMap(&binding);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let amap = PhonyMap(&[(0.1f64, '-')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let amap = PhonyMap(&[(0.1f32, '-')]);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let key = PhonyMap(&[(0i8, '-')]);
        let expected = r#"{"0":"-"}"#;
        assert_eq!(to_str(&mut buf, &key).unwrap(), expected);
        let binding = [(key, '-')];
        let amap = PhonyMap(&binding);
        assert_eq!(to_str(&mut buf, &amap), Err(Error::InvalidKeyType));
        let mut buf = [0u8; 20];
        let amap = PhonyMap(&[(false, 0), (true, 1)]);
        assert_eq!(to_str(&mut buf, &amap).unwrap(), r#"{"false":0,"true":1}"#);
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], &amap),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_bool() {
        let mut buf = [0u8; 6];
        assert_eq!(to_str(&mut buf, &true).unwrap(), "true");
        assert_eq!(to_str(&mut buf, &false).unwrap(), "false");
    }

    #[test]
    fn test_ser_str() {
        let mut buf = [0u8; 13];
        assert_eq!(to_str(&mut buf, "hello").unwrap(), r#""hello""#);
        assert_eq!(to_str(&mut buf, "").unwrap(), r#""""#);

        // Characters unescaped if possible
        assert_eq!(to_str(&mut buf, "ä").unwrap(), r#""ä""#);
        assert_eq!(to_str(&mut buf, "৬").unwrap(), r#""৬""#);
        assert_eq!(to_str(&mut buf, "\u{A0}").unwrap(), "\"\u{A0}\""); // non-breaking space
        assert_eq!(to_str(&mut buf, "ℝ").unwrap(), r#""ℝ""#); // 3 byte character
        assert_eq!(to_str(&mut buf, "💣").unwrap(), r#""💣""#); // 4 byte character

        // " and \ must be escaped
        assert_eq!(to_str(&mut buf, "foo\"bar").unwrap(), r#""foo\"bar""#);
        assert_eq!(to_str(&mut buf, "foo\\bar").unwrap(), r#""foo\\bar""#);

        // \b, \t, \n, \f, \r must be escaped in their two-character escaping
        assert_eq!(to_str(&mut buf, " \u{0008} ").unwrap(), r#"" \b ""#);
        assert_eq!(to_str(&mut buf, " \u{0009} ").unwrap(), r#"" \t ""#);
        assert_eq!(to_str(&mut buf, " \u{000A} ").unwrap(), r#"" \n ""#);
        assert_eq!(to_str(&mut buf, " \u{000C} ").unwrap(), r#"" \f ""#);
        assert_eq!(to_str(&mut buf, " \u{000D} ").unwrap(), r#"" \r ""#);

        // U+0000 through U+001F is escaped using six-character \u00xx uppercase hexadecimal escape sequences
        assert_eq!(to_str(&mut buf, " \u{0000} ").unwrap(), r#"" \u0000 ""#);
        assert_eq!(to_str(&mut buf, " \u{0001} ").unwrap(), r#"" \u0001 ""#);
        assert_eq!(to_str(&mut buf, " \u{0007} ").unwrap(), r#"" \u0007 ""#);
        assert_eq!(to_str(&mut buf, " \u{000e} ").unwrap(), r#"" \u000E ""#);
        assert_eq!(to_str(&mut buf, " \u{001D} ").unwrap(), r#"" \u001D ""#);
        assert_eq!(to_str(&mut buf, " \u{001f} ").unwrap(), r#"" \u001F ""#);
        assert_eq!(to_str(&mut buf, " \t \x00 ").unwrap(), r#"" \t \u0000 ""#);

        struct SimpleDecimal(f32);
        impl fmt::Display for SimpleDecimal {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:.2}", &self.0)
            }
        }
        impl serde::Serialize for SimpleDecimal {
            fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.collect_str(self)
            }
        }
        let a = SimpleDecimal(core::f32::consts::PI);
        assert_eq!(to_str(&mut buf, &a).unwrap(), r#""3.14""#);
        // errors
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], " \t \x00 "),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
        assert_eq!(
            to_str(&mut buf[..0], &a),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(to_str(&mut buf[..1], &a), Err(Error::FormatError));
        assert_eq!(
            to_str(&mut buf[..5], &a),
            Err(Error::Writer(SerError::BufferFull))
        );
    }

    #[test]
    fn test_ser_array() {
        let mut buf = [0u8; 7];
        let empty: [&str; 0] = [];
        assert_eq!(to_str(&mut buf, &empty).unwrap(), "[]");
        assert_eq!(to_str(&mut buf, &[0, 1, 2]).unwrap(), "[0,1,2]");
        // errors
        let a: &[u8] = &[0, 1, 2][..];
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], a),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_tuple() {
        let mut buf = [0u8; 7];
        assert_eq!(to_str(&mut buf, &(0i32, 1u8)).unwrap(), "[0,1]");
        let a = (0i8, 1u32, 2i16);
        assert_eq!(to_str(&mut buf, &a).unwrap(), "[0,1,2]");
        // errors
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], &a),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_enum() {
        #[derive(Serialize)]
        enum Type {
            #[serde(rename = "boolean")]
            Boolean,
            #[serde(rename = "number")]
            Number,
        }
        let mut buf = [0u8; 9];

        assert_eq!(to_str(&mut buf, &Type::Boolean).unwrap(), r#""boolean""#);

        assert_eq!(to_str(&mut buf, &Type::Number).unwrap(), r#""number""#);
    }

    #[test]
    fn test_ser_struct_bool() {
        #[derive(Serialize)]
        struct Led {
            led: bool,
        }

        let mut buf = [0u8; 12];

        assert_eq!(
            to_str(&mut buf, &Led { led: true }).unwrap(),
            r#"{"led":true}"#
        );
    }

    #[test]
    fn test_ser_struct_i8() {
        #[derive(Serialize)]
        struct Temperature {
            temperature: i8,
        }

        let mut buf = [0u8; 20];

        assert_eq!(
            to_str(&mut buf, &Temperature { temperature: 127 }).unwrap(),
            r#"{"temperature":127}"#
        );

        assert_eq!(
            to_str(&mut buf, &Temperature { temperature: 20 }).unwrap(),
            r#"{"temperature":20}"#
        );

        assert_eq!(
            to_str(&mut buf, &Temperature { temperature: -17 }).unwrap(),
            r#"{"temperature":-17}"#
        );

        assert_eq!(
            to_str(&mut buf, &Temperature { temperature: -128 }).unwrap(),
            r#"{"temperature":-128}"#
        );
    }

    #[test]
    fn test_ser_struct_u8() {
        #[derive(Serialize)]
        struct Temperature {
            temperature: u8,
        }

        let mut buf = [0u8; 18];

        assert_eq!(
            to_str(&mut buf, &Temperature { temperature: 20 }).unwrap(),
            r#"{"temperature":20}"#
        );
    }

    #[test]
    fn test_ser_struct_f32() {
        #[derive(Serialize)]
        struct Temperature {
            temperature: f32,
        }

        let mut buf = [0u8; 30];

        assert_eq!(
            to_str(&mut buf, &Temperature { temperature: -20.0 }).unwrap(),
            r#"{"temperature":-20}"#
        );

        assert_eq!(
            to_str(
                &mut buf,
                &Temperature {
                    temperature: -20345.
                }
            )
            .unwrap(),
            r#"{"temperature":-20345}"#
        );

        assert_eq!(
            to_str(
                &mut buf,
                &Temperature {
                    temperature: -2.3456789012345e-23
                }
            )
            .unwrap(),
            r#"{"temperature":-2.3456788e-23}"#
        );

        assert_eq!(
            to_str(
                &mut buf,
                &Temperature {
                    temperature: f32::NAN
                }
            )
            .unwrap(),
            r#"{"temperature":null}"#
        );

        assert_eq!(
            to_str(
                &mut buf,
                &Temperature {
                    temperature: f32::NEG_INFINITY
                }
            )
            .unwrap(),
            r#"{"temperature":null}"#
        );
    }

    #[test]
    fn test_ser_struct_option() {
        #[derive(Serialize)]
        struct Property<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            description: Option<&'a str>,
            value: Option<u32>,
        }

        let mut buf = [0u8; 61];

        assert_eq!(
            to_str(
                &mut buf,
                &Property {
                    description: Some("An ambient temperature sensor"),
                    value: None,
                }
            )
            .unwrap(),
            r#"{"description":"An ambient temperature sensor","value":null}"#
        );

        assert_eq!(
            to_str(
                &mut buf,
                &Property {
                    description: None,
                    value: None
                }
            )
            .unwrap(),
            r#"{"value":null}"#
        );

        assert_eq!(
            to_str(
                &mut buf,
                &Property {
                    description: None,
                    value: Some(0)
                }
            )
            .unwrap(),
            r#"{"value":0}"#
        );

        assert_eq!(
            to_str(
                &mut buf,
                &Property {
                    description: Some("Answer to the Ultimate Question?"),
                    value: Some(42)
                }
            )
            .unwrap(),
            r#"{"description":"Answer to the Ultimate Question?","value":42}"#
        );
    }

    #[test]
    fn test_ser_struct_() {
        #[derive(Serialize)]
        struct Empty {}

        let mut buf = [0u8; 20];

        assert_eq!(to_str(&mut buf, &Empty {}).unwrap(), r#"{}"#);

        #[derive(Serialize)]
        struct Tuple {
            a: bool,
            b: bool,
        }

        let t = Tuple { a: true, b: false };
        assert_eq!(to_str(&mut buf, &t).unwrap(), r#"{"a":true,"b":false}"#);
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], &t),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_unit() {
        let mut buf = [0u8; 4];
        let a = ();
        assert_eq!(to_str(&mut buf, &a).unwrap(), r#"null"#);
        #[derive(Serialize)]
        struct Unit;
        let a = Unit;
        assert_eq!(to_str(&mut buf, &a).unwrap(), r#"null"#);
    }

    #[test]
    fn test_ser_newtype_struct() {
        #[derive(Serialize)]
        struct A(u32);

        let mut buf = [0u8; 2];

        let a = A(54);
        assert_eq!(to_str(&mut buf, &a).unwrap(), r#"54"#);
    }

    #[test]
    fn test_ser_newtype_variant() {
        #[derive(Serialize)]
        enum A {
            A(u32),
        }
        let mut buf = [0u8; 8];

        let a = A::A(54);
        assert_eq!(to_str(&mut buf, &a).unwrap(), r#"{"A":54}"#);
        // errors
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], &a),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_struct_variant() {
        #[derive(Serialize)]
        enum A {
            A { x: u32, y: u16 },
        }
        let mut buf = [0u8; 22];
        let a = A::A { x: 54, y: 720 };

        assert_eq!(to_str(&mut buf, &a).unwrap(), r#"{"A":{"x":54,"y":720}}"#);
        // errors
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], &a),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_tuple_variant() {
        #[derive(Serialize)]
        enum A {
            A(u32, u16),
        }
        let mut buf = [0u8; 14];
        let a = A::A(54, 720);

        assert_eq!(to_str(&mut buf, &a).unwrap(), r#"{"A":[54,720]}"#);
        // errors
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], &a),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_tuple_struct() {
        #[derive(Serialize)]
        struct A<'a>(u32, Option<&'a str>, u16, bool);

        let mut buf = [0u8; 25];
        let a = A(42, Some("A string"), 720, false);

        assert_eq!(
            to_str(&mut buf, &a).unwrap(),
            r#"[42,"A string",720,false]"#
        );
        for len in 0..buf.len() {
            assert_eq!(
                to_str(&mut buf[..len], &a),
                Err(Error::Writer(SerError::BufferFull))
            );
        }
    }

    #[test]
    fn test_ser_tuple_struct_roundtrip() {
        use serde::Deserialize;

        #[derive(Debug, Deserialize, Serialize, PartialEq)]
        struct A<'a>(u32, Option<&'a str>, u16, bool);

        let mut buf = [0u8; 25];
        let a1 = A(42, Some("A string"), 720, false);

        let mut writer = SliceWriter::new(&mut buf);
        to_writer(&mut writer, &a1).unwrap();
        let mut serialized = writer.split().0;
        let a2: A<'_> = crate::from_mut_slice(&mut serialized).unwrap();
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_ser_serialize_bytes() {
        use core::fmt::Write;

        struct SimpleDecimal(f32);

        impl serde::Serialize for SimpleDecimal {
            fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let mut buf = [0u8; 20];
                let mut aux = SliceWriter::new(&mut buf);
                write!(aux, "{:.2}", self.0).unwrap();
                serializer.serialize_bytes(&aux.as_ref())
            }
        }

        let mut buf = [0u8; 8];

        let sd1 = SimpleDecimal(1.55555);
        assert_eq!(to_str_pass_bytes(&mut buf, &sd1).unwrap(), r#"1.56"#);

        let sd2 = SimpleDecimal(0.000);
        assert_eq!(to_str_pass_bytes(&mut buf, &sd2).unwrap(), r#"0.00"#);

        let sd3 = SimpleDecimal(22222.777777);
        assert_eq!(to_str_pass_bytes(&mut buf, &sd3).unwrap(), r#"22222.78"#);
    }

    #[test]
    fn test_ser_error() {
        let mut buf = [0u8; 0];
        #[derive(Serialize)]
        struct Bytes<'a>(#[serde(with = "serde_bytes")] &'a [u8]);
        let bytes = Bytes(b"_");
        assert_eq!(
            to_str(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str_hex_bytes(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str_base64_bytes(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str_pass_bytes(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str(&mut buf, "_"),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str(&mut buf, &true),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str(&mut buf, &()),
            Err(Error::Writer(SerError::BufferFull))
        );
        let mut buf = [0u8; 1];
        assert_eq!(
            to_str(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str_hex_bytes(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str_base64_bytes(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str(&mut buf, "_"),
            Err(Error::Writer(SerError::BufferFull))
        );
        let mut buf = [0u8; 3];
        assert_eq!(
            to_str(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str_hex_bytes(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str_base64_bytes(&mut buf, &bytes),
            Err(Error::Writer(SerError::BufferFull))
        );
        assert_eq!(
            to_str(&mut buf, "__"),
            Err(Error::Writer(SerError::BufferFull))
        );
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_ser_error_string() {
        assert_eq!(
            format!("{}", Error::from(SerError::BufferFull)),
            "buffer is full"
        );
        assert_eq!(
            format!("{}", Error::<SerError>::InvalidKeyType),
            "invalid JSON object key data type"
        );
        assert_eq!(
            format!("{}", Error::<SerError>::Utf8Encode),
            "error encoding JSON as UTF-8 string"
        );
        assert_eq!(
            format!("{}", Error::<SerError>::FormatError),
            "error while collecting a string"
        );
        let custom: Error<SerError> = serde::ser::Error::custom("xxx");
        assert_eq!(format!("{}", custom), "xxx while serializing JSON");

        #[derive(Serialize)]
        struct Bytes<'a>(#[serde(with = "serde_bytes")] &'a [u8]);
        let bytes = Bytes(b"\xFF\xFE");
        assert_eq!(to_string_pass_bytes(&bytes), Err(Error::Utf8Encode));
    }

    #[cfg(not(any(feature = "std", feature = "alloc")))]
    #[test]
    fn test_ser_error_fmt() {
        use core::fmt::Write;
        let mut buf = [0u8; 28];
        let mut writer = SliceWriter::new(&mut buf);
        let custom: Error<SerError> = serde::ser::Error::custom("xxx");
        write!(writer, "{}", custom).unwrap();
        assert_eq!(writer.as_ref(), b"error while serializing JSON");
    }

    #[test]
    fn test_ser_string_collector() {
        use core::fmt::Write;
        let mut buf = [0u8; 22];
        let mut writer = SliceWriter::new(&mut buf);
        let mut col = StringCollector::new(&mut writer);
        col.write_str("foo bar").unwrap();
        writeln!(col, "ℝ\tä\x00").unwrap();
        let (res, writer) = writer.split();
        assert_eq!(res, b"foo bar\xe2\x84\x9d\\t\xc3\xa4\\u0000\\n");
        assert_eq!(writer.capacity(), 0);
    }
}

/// LP fork: the token hook. A sink that takes every token sees the whole
/// structure and no text; a sink that declines sees upstream's exact text.
#[cfg(all(test, feature = "std"))]
mod token_tests {
    use std::collections::BTreeMap;
    use std::format;
    use std::string::{String, ToString};
    use std::vec;
    use std::vec::Vec;

    use super::*;
    use crate::ser_write::Token;
    use core::result::Result;
    use serde::Serialize;

    /// Takes every token (recording it) or declines every one.
    struct TokenLog {
        take: bool,
        tokens: Vec<String>,
        text: Vec<u8>,
    }

    impl TokenLog {
        fn new(take: bool) -> Self {
            Self {
                take,
                tokens: Vec::new(),
                text: Vec::new(),
            }
        }
    }

    impl SerWrite for TokenLog {
        type Error = ser_write::SerError;
        const TAKES_TOKENS: bool = true;

        fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
            self.text.extend_from_slice(buf);
            Ok(())
        }

        fn token(&mut self, token: Token<'_>) -> Result<bool, Self::Error> {
            if self.take {
                self.tokens.push(format!("{token:?}"));
            }
            Ok(self.take)
        }
    }

    /// Bytes behind the blob marker: base64-ish text when human-readable
    /// (here: hex, to keep the test independent of a base64 encoder), the
    /// bytes themselves otherwise.
    struct Blob(&'static [u8]);

    struct HexText(&'static [u8]);

    impl fmt::Display for HexText {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for b in self.0 {
                write!(f, "{b:02x}")?;
            }
            Ok(())
        }
    }

    struct BlobInner(&'static [u8]);

    impl Serialize for BlobInner {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if s.is_human_readable() {
                s.collect_str(&HexText(self.0))
            } else {
                s.serialize_bytes(self.0)
            }
        }
    }

    impl Serialize for Blob {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_newtype_struct(BLOB_MARKER, &BlobInner(self.0))
        }
    }

    #[derive(Serialize)]
    enum Kind {
        Unit,
        Newtype(u8),
        Tuple(i16, bool),
        Struct { x: f32 },
    }

    #[derive(Serialize)]
    struct Inner {
        name: &'static str,
        kinds: Vec<Kind>,
    }

    #[derive(Serialize)]
    struct Outer {
        id: u64,
        delta: i64,
        ok: bool,
        none: Option<u32>,
        ratio: f64,
        inner: Inner,
        by_index: BTreeMap<u32, &'static str>,
        blob: Blob,
        pair: (u8, char),
    }

    fn value() -> Outer {
        let mut by_index = BTreeMap::new();
        by_index.insert(7, "a\"b");
        by_index.insert(12, "c");
        Outer {
            id: 42,
            delta: -3,
            ok: true,
            none: None,
            ratio: 0.5,
            inner: Inner {
                name: "n",
                kinds: vec![
                    Kind::Unit,
                    Kind::Newtype(9),
                    Kind::Tuple(-1, false),
                    Kind::Struct { x: 1.5 },
                ],
            },
            by_index,
            blob: Blob(&[0xde, 0xad]),
            pair: (1, 'z'),
        }
    }

    const TEXT: &str = concat!(
        r#"{"id":42,"delta":-3,"ok":true,"none":null,"ratio":0.5,"#,
        r#""inner":{"name":"n","kinds":["Unit",{"Newtype":9},{"Tuple":[-1,false]},{"Struct":{"x":1.5}}]},"#,
        r#""by_index":{"7":"a\"b","12":"c"},"blob":"dead","pair":[1,"z"]}"#,
    );

    #[test]
    fn a_sink_that_declines_gets_upstreams_text() {
        let mut plain = Vec::new();
        to_writer(&mut plain, &value()).unwrap();
        assert_eq!(core::str::from_utf8(&plain).unwrap(), TEXT);

        let mut log = TokenLog::new(false);
        to_writer(&mut log, &value()).unwrap();
        assert_eq!(core::str::from_utf8(&log.text).unwrap(), TEXT);
        assert!(log.tokens.is_empty());
    }

    #[test]
    fn a_sink_that_takes_every_token_sees_the_whole_structure_and_no_text() {
        let mut log = TokenLog::new(true);
        to_writer(&mut log, &value()).unwrap();
        assert_eq!(log.text, b"", "no text reaches a sink that takes tokens");
        let expected = [
            "MapBegin",
            "Key(\"id\")",
            "Separator",
            "U64(42)",
            "Separator",
            "Key(\"delta\")",
            "Separator",
            "I64(-3)",
            "Separator",
            "Key(\"ok\")",
            "Separator",
            "Bool(true)",
            "Separator",
            "Key(\"none\")",
            "Separator",
            "Null",
            "Separator",
            "Key(\"ratio\")",
            "Separator",
            "F64(0.5)",
            "Separator",
            "Key(\"inner\")",
            "Separator",
            "MapBegin",
            "Key(\"name\")",
            "Separator",
            "Str(\"n\")",
            "Separator",
            "Key(\"kinds\")",
            "Separator",
            "SeqBegin",
            "Str(\"Unit\")",
            "Separator",
            "MapBegin",
            "Key(\"Newtype\")",
            "Separator",
            "U64(9)",
            "MapEnd",
            "Separator",
            "MapBegin",
            "Key(\"Tuple\")",
            "Separator",
            "SeqBegin",
            "I64(-1)",
            "Separator",
            "Bool(false)",
            "SeqEnd",
            "MapEnd",
            "Separator",
            "MapBegin",
            "Key(\"Struct\")",
            "Separator",
            "MapBegin",
            "Key(\"x\")",
            "Separator",
            "F32(1.5)",
            "MapEnd",
            "MapEnd",
            "SeqEnd",
            "MapEnd",
            "Separator",
            "Key(\"by_index\")",
            "Separator",
            "MapBegin",
            "Key(\"7\")",
            "Separator",
            "Str(\"a\\\"b\")",
            "Separator",
            "Key(\"12\")",
            "Separator",
            "Str(\"c\")",
            "MapEnd",
            "Separator",
            "Key(\"blob\")",
            "Separator",
            "Blob([222, 173])",
            "Separator",
            "Key(\"pair\")",
            "Separator",
            "SeqBegin",
            "U64(1)",
            "Separator",
            "Str(\"z\")",
            "SeqEnd",
            "MapEnd",
        ];
        assert_eq!(log.tokens, expected.map(ToString::to_string));
    }

    #[test]
    fn non_finite_floats_are_null_tokens() {
        let mut log = TokenLog::new(true);
        to_writer(&mut log, &[f32::NAN, f32::INFINITY]).unwrap();
        assert_eq!(
            log.tokens,
            ["SeqBegin", "Null", "Separator", "Null", "SeqEnd"]
        );
    }

    #[test]
    fn negative_and_extreme_integer_keys_keep_their_text() {
        let mut map = BTreeMap::new();
        map.insert(i64::MIN, 0u8);
        map.insert(-5, 1);
        map.insert(i64::MAX, 2);
        let mut plain = Vec::new();
        to_writer(&mut plain, &map).unwrap();
        assert_eq!(
            core::str::from_utf8(&plain).unwrap(),
            r#"{"-9223372036854775808":0,"-5":1,"9223372036854775807":2}"#
        );
        let mut log = TokenLog::new(true);
        to_writer(&mut log, &map).unwrap();
        assert_eq!(log.tokens[1], "Key(\"-9223372036854775808\")");
    }

    #[test]
    fn the_blob_marker_is_transparent_to_serde_json() {
        let json = serde_json::to_string(&value()).unwrap();
        // serde_json writes the f32 1.5 and f64 0.5 the same way ryu-js does.
        assert_eq!(json, TEXT);
    }
}
