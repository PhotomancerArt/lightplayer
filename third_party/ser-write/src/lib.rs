//! Tools for implementing writer style serializers dedicated for `no_std` targets.
//!
//! Features a [trait][SerWrite] for objects which are byte-oriented sinks, akin to `std::io::Write`.
//!
//! Serializers can be implemented using this trait as a writer.
//!
//! Embedded or otherwise `no_std` projects can implement [`SerWrite`] for custom sinks.
//!
//! Some [implemenentations] for foreign types are provided depending on the enabled features.
//!
//! [implemenentations]: SerWrite#foreign-impls
#![no_std]
#![cfg_attr(not(test), forbid(unsafe_code))]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "std")]
extern crate std;

#[cfg(all(feature = "alloc",not(feature = "std")))]
extern crate alloc;

use core::fmt;

mod foreign;

pub type SerResult<T> = Result<T, SerError>;

/// A simple error type that can be used for [`SerWrite::Error`] implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SerError {
    /// Buffer is full
    BufferFull,
}

impl fmt::Display for SerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerError::BufferFull => f.write_str("buffer is full"),
        }
    }
}

impl core::error::Error for SerError {}

/// LP fork: one structural token a serializer offers its sink *before* it
/// writes the text for it. See [`SerWrite::token`].
///
/// The variants are the shape of a JSON document, not of any one text form:
/// a sink that takes them can encode the same value some other way (the
/// packed wire encoding) without re-lexing the serializer's text.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Token<'a> {
    /// A string value.
    Str(&'a str),
    /// An object key, as text (integer and bool keys arrive as their digits).
    Key(&'a str),
    /// A non-negative integer.
    U64(u64),
    /// A signed integer.
    I64(i64),
    /// A finite `f32` (non-finite floats are offered as [`Token::Null`]).
    F32(f32),
    /// A finite `f64` (non-finite floats are offered as [`Token::Null`]).
    F64(f64),
    /// `true` or `false`.
    Bool(bool),
    /// `null`, and every unit.
    Null,
    /// `{`: an object (a map, a struct, or an enum variant's wrapper) begins.
    MapBegin,
    /// `}`.
    MapEnd,
    /// `[`: an array (a sequence or a tuple) begins.
    SeqBegin,
    /// `]`.
    SeqEnd,
    /// Punctuation the text form needs between items (`,` and `:`).
    Separator,
    /// Raw bytes that the text form writes as a base64 string.
    Blob(&'a [u8]),
}

/// Serializers should write data to the implementations of this trait.
pub trait SerWrite {
    /// An error type returned from the trait methods.
    type Error;
    /// Write **all** bytes from `buf` to the internal buffer.
    ///
    /// Otherwise return an error.
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error>;
    /// Write a single `byte` to the internal buffer.
    ///
    /// Return an error if the operation could not succeed.
    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<(), Self::Error> {
        self.write(core::slice::from_ref(&byte))
    }
    /// Write a **whole** string to the internal buffer.
    ///
    /// Otherwise return an error.
    #[inline]
    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        self.write(s.as_bytes())
    }
    /// LP fork: whether [`SerWrite::token`] can ever return `Ok(true)`.
    ///
    /// A type-erasing wrapper reads it once, so a text sink pays one branch
    /// per token instead of a virtual call.
    const TAKES_TOKENS: bool = false;
    /// LP fork: offered one structural [`Token`] before the serializer writes
    /// its own text for it.
    ///
    /// Return `Ok(true)` to take it: the serializer then writes nothing for
    /// it. Return `Ok(false)` to decline: the serializer writes its text as
    /// usual. The default declines, so every existing sink is unchanged.
    #[inline(always)]
    fn token(&mut self, _token: Token<'_>) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

impl<T: SerWrite> SerWrite for &'_ mut T {
    type Error = T::Error;

    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        (*self).write(buf)
    }

    #[inline(always)]
    fn write_byte(&mut self, byte: u8) -> Result<(), Self::Error> {
        (*self).write_byte(byte)
    }

    #[inline(always)]
    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        (*self).write_str(s)
    }

    const TAKES_TOKENS: bool = T::TAKES_TOKENS;

    #[inline(always)]
    fn token(&mut self, token: Token<'_>) -> Result<bool, Self::Error> {
        (*self).token(token)
    }
}

/// A simple slice writer (example implementation)
#[derive(Debug, PartialEq)]
pub struct SliceWriter<'a> {
    pub buf: &'a mut [u8],
    pub len: usize
}

impl<'a> AsRef<[u8]> for SliceWriter<'a> {
    /// Returns a populated portion of the slice
    fn as_ref(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl<'a> AsMut<[u8]> for SliceWriter<'a> {
    /// Returns a populated portion of the slice
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buf[..self.len]
    }
}

impl<'a> SliceWriter<'a> {
    /// Create a new instance
    pub fn new(buf: &'a mut [u8]) -> Self {
        SliceWriter { buf, len: 0 }
    }
    /// Return populated length
    pub fn len(&self) -> usize {
        self.len
    }
    /// Return whether the output is not populated.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Return total capacity of the container
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }
    /// Return remaining free capacity
    pub fn rem_capacity(&self) -> usize {
        self.buf.len() - self.len
    }
    /// Reset cursor to the beginning of a container slice
    pub fn clear(&mut self) {
        self.len = 0;
    }
    /// Split the underlying buffer and return the portion of the populated buffer
    /// with an underlying buffer's borrowed lifetime.
    ///
    /// Once a [`SliceWriter`] is dropped the slice stays borrowed as long as the
    /// original container lives.
    pub fn split(self) -> (&'a mut[u8], Self) {
        let (res, buf) = self.buf.split_at_mut(self.len);
        (res, Self { buf, len: 0 })
    }
}

impl SerWrite for SliceWriter<'_> {
    type Error = SerError;

    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        let end = self.len + buf.len();
        match self.buf.get_mut(self.len..end) {
            Some(chunk) => {
                chunk.copy_from_slice(buf);
                self.len = end;
                Ok(())
            }
            None => Err(SerError::BufferFull)
        }
    }
}

impl<'a> fmt::Write for SliceWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        SerWrite::write_str(self, s).map_err(|_| fmt::Error)
    }
}

#[cfg(test)]
mod tests {
    use core::fmt::Write;
    use super::*;

    /// LP fork: a sink that takes tokens.
    struct Taker(usize);

    impl SerWrite for Taker {
        type Error = SerError;
        const TAKES_TOKENS: bool = true;
        fn write(&mut self, _buf: &[u8]) -> SerResult<()> {
            Ok(())
        }
        fn token(&mut self, _token: Token<'_>) -> SerResult<bool> {
            self.0 += 1;
            Ok(true)
        }
    }

    #[test]
    fn test_token_default_declines() {
        let mut buf = [0u8; 4];
        let mut writer = SliceWriter::new(&mut buf);
        assert!(!<SliceWriter<'_> as SerWrite>::TAKES_TOKENS);
        assert_eq!(writer.token(Token::Blob(b"ab")), Ok(false));
        assert_eq!(writer.len(), 0, "declining writes nothing");
    }

    #[test]
    fn test_token_forwards_through_mut_ref() {
        let mut taker = Taker(0);
        assert!(<&mut Taker as SerWrite>::TAKES_TOKENS);
        let mut by_ref = &mut taker;
        assert_eq!(<&mut Taker as SerWrite>::token(&mut by_ref, Token::MapBegin), Ok(true));
        assert_eq!(taker.0, 1);
    }

    #[test]
    fn test_ser_error() {
        #[cfg(feature = "std")]
        {
            assert_eq!(std::format!("{}", SerError::BufferFull), "buffer is full");
        }
        let mut buf = [0u8;0];
        let mut writer = SliceWriter::new(&mut buf);
        assert_eq!(write!(writer, "!"), Err(fmt::Error));
    }

    #[test]
    fn test_slice_writer() {
        let mut buf = [0u8;22];
        let mut writer = SliceWriter::new(&mut buf);
        assert_eq!(writer.capacity(), 22);
        assert_eq!(writer.rem_capacity(), 22);
        assert_eq!(writer.len(), 0);
        assert_eq!(writer.is_empty(), true);
        writer.write(b"Hello World!").unwrap();
        assert_eq!(writer.rem_capacity(), 10);
        assert_eq!(writer.len(), 12);
        assert_eq!(writer.is_empty(), false);
        writer.write_byte(b' ').unwrap();
        assert_eq!(writer.rem_capacity(), 9);
        assert_eq!(writer.len(), 13);
        assert_eq!(writer.is_empty(), false);
        SerWrite::write_str(&mut writer, "Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.as_ref(), expected);
        assert_eq!(writer.as_mut(), expected);
        assert_eq!(writer.capacity(), 22);
        assert_eq!(writer.rem_capacity(), 0);
        assert_eq!(writer.is_empty(), false);
        assert_eq!(writer.len(), 22);
        let (head, mut writer) = writer.split();
        assert_eq!(head, expected);
        assert_eq!(writer.write_byte(b' ').unwrap_err(), SerError::BufferFull);
    }
}
