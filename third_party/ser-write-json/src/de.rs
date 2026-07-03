//! JSON serde deserializer
// use std::println;
#[cfg(feature = "std")]
use std::{string::String, string::ToString};

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::{string::String, string::ToString};

use core::cell::Cell;
use core::num::{ParseFloatError, ParseIntError};
use core::ops::Neg;
use core::slice::from_raw_parts_mut;
use core::str::{FromStr, Utf8Error};
use core::{fmt, str};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::forward_to_deserialize_any;

/// JSON deserializer with bytes deserialized from JSON strings (with unescaping)
/// without any additional decoding
pub type DeserializerNopeByteStr<'de> = Deserializer<'de, StringByteNopeDecoder>;
/// JSON deserializer with bytes deserialized from HEX-encoded strings
pub type DeserializerHexByteStr<'de> = Deserializer<'de, StringByteHexDecoder>;
/// JSON deserializer with bytes deserialized from BASE-64 encoded strings
pub type DeserializerBase64ByteStr<'de> = Deserializer<'de, StringByteBase64Decoder>;

/// Deserialize an instance of type `T` from a mutable slice of bytes of JSON text.
///
/// `P` must implement [`StringByteDecoder`] and determines how strings are converted
/// to bytes.
///
/// The provided slice must be writable so the deserializer can unescape strings
/// and parse bytes from arrays or strings in-place.
///
/// __NOTE__: Assume the original slice content will be modified!
///
/// Any `&str` or `&[u8]` in the returned type will contain references to the provided slice.
pub fn from_mut_slice_with_decoder<'a, P, T>(v: &'a mut [u8]) -> Result<T>
where
    T: de::Deserialize<'a>,
    P: StringByteDecoder<'a>,
{
    let mut de = Deserializer::<P>::from_mut_slice(v);
    let value = de::Deserialize::deserialize(&mut de)?;
    de.end()?;

    Ok(value)
}

/// Deserialize an instance of type `T` from a mutable slice of bytes of JSON text.
///
/// Byte arrays deserialized from a string retain the original content after
/// unescaping all `\` tokens. The content of such a byte array is **not** UTF-8
/// validated.
///
/// The provided slice must be writable so the deserializer can unescape strings
/// and parse bytes from arrays or strings in-place.
///
/// __NOTE__: Assume the original slice content will be modified!
///
/// Any `&str` or `&[u8]` in the returned type will contain references to the provided slice.
pub fn from_mut_slice<'a, T>(v: &'a mut [u8]) -> Result<T>
where
    T: de::Deserialize<'a>,
{
    from_mut_slice_with_decoder::<StringByteNopeDecoder, _>(v)
}

/// Deserialize an instance of type `T` from a mutable slice of bytes of JSON text.
///
/// Byte arrays deserialized from a string are decoded expecting two hexadecimal ASCII
/// characters per byte.
///
/// The provided slice must be writable so the deserializer can unescape strings
/// and parse bytes from arrays or strings in-place.
///
/// __NOTE__: Assume the original slice content will be modified!
///
/// Any `&str` or `&[u8]` in the returned type will contain references to the provided slice.
pub fn from_mut_slice_hex_bytes<'a, T>(v: &'a mut [u8]) -> Result<T>
where
    T: de::Deserialize<'a>,
{
    from_mut_slice_with_decoder::<StringByteHexDecoder, _>(v)
}

/// Deserialize an instance of type `T` from a mutable slice of bytes of JSON text.
///
/// Byte arrays deserialized from a string are decoded expecting [Base64] standard encoding
/// with optional padding.
///
/// The provided slice must be writable so the deserializer can unescape strings
/// and parse bytes from arrays or strings in-place.
///
/// __NOTE__: Assume the original slice content will be modified!
///
/// Any `&str` or `&[u8]` in the returned type will contain references to the provided slice.
///
/// [Base64]: https://datatracker.ietf.org/doc/html/rfc4648#section-4
pub fn from_mut_slice_base64_bytes<'a, T>(v: &'a mut [u8]) -> Result<T>
where
    T: de::Deserialize<'a>,
{
    from_mut_slice_with_decoder::<StringByteBase64Decoder, _>(v)
}

/// Serde JSON deserializer.
///
/// `P` must implement [`StringByteDecoder`].
///
/// * deserializes data from a mutable slice,
/// * unescapes strings in-place,
/// * decodes strings or number arrays into bytes in-place,
/// * deserializes borrowed references to `&str` and `&[u8]` types,
/// * deserializes bytes from arrays of numbers,
/// * deserializes bytes from strings using `P` as a string decoder,
/// * deserializes structs from JSON objects or arrays.
pub struct Deserializer<'de, P> {
    input: &'de mut [u8],
    index: usize,
    _parser: core::marker::PhantomData<P>,
}

/// Deserialization result
pub type Result<T> = core::result::Result<T, Error>;

/// Deserialization error
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum Error {
    /// EOF while parsing
    UnexpectedEof,
    /// Invalid JSON string escape sequence
    InvalidEscapeSequence,
    /// A control ASCII character detected in a JSON input
    StringControlChar,
    /// Expected this character to be a `':'`.
    ExpectedColon,
    /// Expected this character to be either a `','` or a `']'`.
    ExpectedArrayCommaOrEnd,
    /// Expected a `']'` character
    ExpectedArrayEnd,
    /// Array content starts with a leading `','`.
    LeadingArrayComma,
    /// Array content ends with a trailing `','`.
    TrailingArrayComma,
    /// Expected this character to be either a `','` or a `'}'`.
    ExpectedObjectCommaOrEnd,
    /// Expected this character to be `'}'`.
    ExpectedObjectEnd,
    /// Object content starts with a leading `,`.
    LeadingObjectComma,
    /// Object content ends with a trailing `,`.
    TrailingObjectComma,
    /// Expected to parse either `true`, `false`, or `null`.
    ExpectedToken,
    /// Expected `null`
    ExpectedNull,
    /// Expected a `"` character
    ExpectedString,
    /// Expected an array
    ExpectedArray,
    /// Expected an object
    ExpectedObject,
    /// Expected an object or an array
    ExpectedStruct,
    /// Expected this character to start an enum variant.
    ExpectedEnumValue,
    /// Expected this character to be `'}'`.
    ExpectedEnumObjectEnd,
    /// Invalid number
    InvalidNumber,
    /// Invalid type
    InvalidType,
    /// Invalid unicode code point
    InvalidUnicodeCodePoint,
    /// Object key is not a string
    KeyMustBeAString,
    /// JSON has non-whitespace trailing characters after the value
    TrailingCharacters,
    /// Unexpected character
    UnexpectedChar,
    /// Invalid length
    InvalidLength,
    #[cfg(any(feature = "std", feature = "alloc"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
    /// An error passed down from a [`serde::de::Deserialize`] implementation
    DeserializeError(String),
    #[cfg(not(any(feature = "std", feature = "alloc")))]
    DeserializeError,
}

impl serde::de::StdError for Error {}

#[cfg(any(feature = "std", feature = "alloc"))]
impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::DeserializeError(msg.to_string())
    }
}

#[cfg(not(any(feature = "std", feature = "alloc")))]
impl de::Error for Error {
    fn custom<T: fmt::Display>(_msg: T) -> Self {
        Error::DeserializeError
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::UnexpectedEof => "Unexpected end of JSON input",
            Error::InvalidEscapeSequence => "Invalid JSON string escape sequence",
            Error::StringControlChar => "A control ASCII character found in a JSON string",
            Error::ExpectedArrayCommaOrEnd => "Expected `','` or `']'`",
            Error::ExpectedArrayEnd => "Expected ']'",
            Error::LeadingArrayComma => "JSON array content starts with a leading `','`",
            Error::TrailingArrayComma => "JSON array content ends with a trailing `','`",
            Error::ExpectedObjectCommaOrEnd => "Expected `','` or `'}'`",
            Error::ExpectedObjectEnd => "Expected `'}'`",
            Error::LeadingObjectComma => "JSON object content starts with a leading `','`",
            Error::TrailingObjectComma => "JSON object content ends with a trailing `','`",
            Error::ExpectedColon => "Expected `':'`",
            Error::ExpectedToken => "Expected either `true`, `false`, or `null`.",
            Error::ExpectedNull => "Expected `null`",
            Error::ExpectedString => r#"Expected `'"'`"#,
            Error::ExpectedArray => "Expeced a JSON array",
            Error::ExpectedObject => "Expected a JSON object",
            Error::ExpectedStruct => "Expected a JSON object or an array",
            Error::ExpectedEnumValue => r#"Expected this character to be `'"'` or `'{'`"#,
            Error::ExpectedEnumObjectEnd => "Expected this character to be `'}'`",
            Error::InvalidNumber => "Invalid number",
            Error::InvalidType => "Invalid type",
            Error::InvalidUnicodeCodePoint => "Invalid unicode code point",
            Error::KeyMustBeAString => "Object key is not a string",
            Error::TrailingCharacters => {
                "JSON has non-whitespace trailing character after the value"
            }
            Error::UnexpectedChar => "Unexpected token while parsing a JSON value",
            Error::InvalidLength => "Invalid length",
            #[cfg(any(feature = "std", feature = "alloc"))]
            Error::DeserializeError(s) => return write!(f, "{} while deserializing JSON", s),
            #[cfg(not(any(feature = "std", feature = "alloc")))]
            Error::DeserializeError => "JSON does not match deserializer’s expected format",
        })
    }
}

impl From<Utf8Error> for Error {
    fn from(_err: Utf8Error) -> Self {
        Error::InvalidUnicodeCodePoint
    }
}

impl From<ParseFloatError> for Error {
    fn from(_err: ParseFloatError) -> Self {
        Error::InvalidNumber
    }
}

impl From<ParseIntError> for Error {
    fn from(_err: ParseIntError) -> Self {
        Error::InvalidNumber
    }
}

/// Convert strings to byte arrays by unescaping original JSON strings
/// without any additional decoding
pub struct StringByteNopeDecoder;
/// Convert strings to byte arrays by decoding HEX-encoded strings
pub struct StringByteHexDecoder;
/// Convert strings to byte arrays by decoding BASE-64 encoded strings
pub struct StringByteBase64Decoder;

/// Auxiliary trait for objects implementing string to bytes decoding.
pub trait StringByteDecoder<'de>: Sized {
    /// Should decode bytes from the JSON string after the opening `b'"'`
    /// has been consumed and until the closing `b'"'` is found in the input slice.
    ///
    /// A decoded byte slice must fit in place where the encoded string originaly was.
    fn decode_string_to_bytes(de: &mut Deserializer<'de, Self>) -> Result<&'de [u8]>;
}

/* special JSON characters */
const SP: u8 = b' ';
const QU: u8 = b'"';
const RS: u8 = b'\\';
const SO: u8 = b'/';
/* special JSON string escape characters */
const B_: u8 = 0x08;
const BB: u8 = b'b'; // \b -> \x08
const T_: u8 = 0x09;
const TT: u8 = b't'; // \t -> \x09
const N_: u8 = 0x0A; // const NN: u8 = b'n'; // \n -> \x0A
const F_: u8 = 0x0C; // const FF: u8 = b'f'; // \f => \x0C
const R_: u8 = 0x0D; // const RR: u8 = b'r'; // \r => \x0D
                     /* \uUUUU */
const UU: u8 = b'u';
const __: u8 = 0;
/* only selected (un)escape codes are permitted */
static UNESCAPE: [u8; 19] = [
    /* \b,  c,  d,  e, \f,  g,  h,  i,  j,  k,  l,  m, \n,  o,  p,  q, \r,  s, \t */
    B_, __, __, __, F_, __, __, __, __, __, __, __, N_, __, __, __, R_, __, T_,
];

#[inline(always)]
fn parse_hex_nib(ch: u8) -> Option<u8> {
    match ch {
        n @ b'0'..=b'9' => Some(n - b'0'),
        _ => match ch | 0x20 {
            n @ b'a'..=b'f' => Some(n - b'a' + 10),
            _ => None,
        },
    }
}

#[inline(always)]
fn parse_uuuu([a, b, c, d]: [u8; 4]) -> Option<u32> {
    Some(
        u16::from_le_bytes([
            (parse_hex_nib(c)? << 4) + parse_hex_nib(d)?,
            (parse_hex_nib(a)? << 4) + parse_hex_nib(b)?,
        ])
        .into(),
    )
}

/// Helper trait for parsing integers
pub trait NumParseTool: Sized + Copy {
    const ZERO: Self;
    fn try_from_ascii_decimal(code: u8) -> Option<Self>;
    fn checked_mul_ten(self) -> Result<Self>;
    fn checked_add(self, rhs: Self) -> Result<Self>;
}

/// Helper trait for parsing negative integers
pub trait CheckedSub: Sized + Copy {
    fn checked_sub(self, rhs: Self) -> Result<Self>;
}

macro_rules! impl_parse_tool {
    ($($ty:ty),*) => {$(
        impl NumParseTool for $ty {
            const ZERO: Self = 0;
            #[inline(always)]
            fn try_from_ascii_decimal(code: u8) -> Option<Self> {
                if matches!(code, b'0'..=b'9') {
                    Some((code - b'0') as Self)
                }
                else {
                    None
                }
            }
            #[inline(always)]
            fn checked_mul_ten(self) -> Result<Self> {
                self.checked_mul(10)
                .ok_or(Error::InvalidNumber)
            }
            #[inline(always)]
            fn checked_add(self, rhs: Self) -> Result<Self> {
                self.checked_add(rhs)
                .ok_or(Error::InvalidNumber)
            }
        }
    )*};
}

macro_rules! impl_checked_sub {
    ($($ty:ty),*) => {$(
        impl CheckedSub for $ty {
            #[inline(always)]
            fn checked_sub(self, rhs: Self) -> Result<Self> {
                self.checked_sub(rhs)
                .ok_or(Error::InvalidNumber)
            }
        }
    )*};
}

impl_parse_tool!(u8, u16, u32, u64, i8, i16, i32, i64);
impl_checked_sub!(i8, i16, i32, i64);

#[cfg(feature = "de-any-f32")]
type AnyFloat = f32;
#[cfg(not(feature = "de-any-f32"))]
type AnyFloat = f64;

enum AnyNumber {
    PosInt(u64),
    NegInt(i64),
    Float(AnyFloat),
}

/// Implementation exposes some helper functions for custom [`StringByteDecoder`] implementations.
impl<'de, P> Deserializer<'de, P> {
    /// Provide a mutable slice, so data can be deserialized in-place
    pub fn from_mut_slice(input: &'de mut [u8]) -> Self {
        Deserializer {
            input,
            index: 0,
            _parser: core::marker::PhantomData,
        }
    }

    /// Consume deserializer and check if trailing characters only consist of whitespace
    pub fn end(mut self) -> Result<()> {
        // println!("end: {}", core::str::from_utf8(&self.input[self.index..]).unwrap());
        self.eat_whitespace()
            .err()
            .map(|_| ())
            .ok_or(Error::TrailingCharacters)
    }

    /// Peek at the next byte code, otherwise return `Err(Error::UnexpectedEof)`.
    pub fn peek(&self) -> Result<u8> {
        self.input
            .get(self.index)
            .copied()
            .ok_or(Error::UnexpectedEof)
    }

    /// Advance the input cursor by `len` characters.
    ///
    /// _Note_: this function only increases a cursor without any checks!
    pub fn eat_some(&mut self, len: usize) {
        self.index += len;
    }

    /// Advance cursor while discarding any JSON whitespace characters from the input slice
    /// and peek at the next non-whitespace character.
    /// Otherwise return `Err(Error::UnexpectedEof)`.
    pub fn eat_whitespace(&mut self) -> Result<u8> {
        let index = self.index;
        self.input[index..]
            .iter()
            .position(|&b| !matches!(b, SP | T_ | N_ | R_))
            .map(|pos| {
                self.index = index + pos;
                self.input[index + pos]
            })
            .ok_or(Error::UnexpectedEof)
    }

    /// Return a mutable reference to the unparsed portion of the input slice on success.
    /// Otherwise return `Err(Error::UnexpectedEof)`.
    pub fn input_mut(&mut self) -> Result<&mut [u8]> {
        self.input.get_mut(self.index..).ok_or(Error::UnexpectedEof)
    }

    /// Split the unparsed portion of the input slice between `0..len` and return it with
    /// the lifetime of the original slice container.
    ///
    /// The returned slice can be passed to `visit_borrowed_*` functions of a [`Visitor`].
    ///
    /// Drop already parsed bytes and bytes between `len..len+skip` and the new unparsed
    /// input slice will begin at `len + skip`.
    ///
    /// __Panics__ if `len + skip` overflows or is larger than the size of the unparsed input slice.
    pub fn split_input(&mut self, len: usize, skip: usize) -> &'de mut [u8] {
        let total_len = self.input.len();
        let ptr = self.input.as_mut_ptr();
        let index = self.index;
        let nstart = index.checked_add(len).unwrap().checked_add(skip).unwrap();
        let newlen = total_len.checked_sub(nstart).unwrap();
        self.index = 0;
        // SAFETY: We just checked that `[index;len]` and `[nstart; newlen]`
        // are not overlapping, because (index + len + skip) <= (nstart + newlen) == total_len
        // so returning a reference is fine.
        unsafe {
            // we can't use slice::split_at_mut here because that would require to re-borrow
            // self.input (it is a mutable reference) thus shorting the originaly referenced
            // lifetime 'de
            self.input = from_raw_parts_mut(ptr.add(nstart), newlen);
            from_raw_parts_mut(ptr.add(index), len)
        }
    }

    #[inline]
    fn parse_positive_number<T: NumParseTool>(&mut self, mut number: T) -> Result<T> {
        let mut pos = 0usize;
        for ch in self.input_mut()?.iter().copied() {
            match T::try_from_ascii_decimal(ch) {
                Some(n) => number = number.checked_mul_ten()?.checked_add(n)?,
                _ => break,
            }
            pos += 1;
        }
        self.eat_some(pos);
        Ok(number)
    }

    #[inline]
    fn parse_negative_number<T: NumParseTool + CheckedSub>(&mut self, mut number: T) -> Result<T> {
        let mut pos = 0usize;
        for ch in self.input_mut()?.iter().copied() {
            match T::try_from_ascii_decimal(ch) {
                Some(n) => number = number.checked_mul_ten()?.checked_sub(n)?,
                _ => break,
            }
            pos += 1;
        }
        self.eat_some(pos);
        Ok(number)
    }

    /// Consume whitespace and then parse a number as an unsigned integer
    #[inline]
    pub fn parse_unsigned<T: NumParseTool>(&mut self) -> Result<T> {
        let peek = self.eat_whitespace()?;

        match peek {
            b'-' => Err(Error::InvalidNumber),
            b'0' => {
                self.eat_some(1);
                Ok(T::ZERO)
            }
            _ => {
                if let Some(number) = T::try_from_ascii_decimal(peek) {
                    self.eat_some(1);
                    self.parse_positive_number(number)
                } else {
                    Err(Error::InvalidType)
                }
            }
        }
    }

    /// Consume whitespace and then parse a number as a signed integer
    #[inline]
    pub fn parse_signed<T>(&mut self) -> Result<T>
    where
        T: NumParseTool + CheckedSub + Neg<Output = T>,
    {
        let mut peek = self.eat_whitespace()?;
        let is_neg = if peek == b'-' {
            self.eat_some(1);
            peek = self.peek()?;
            true
        } else {
            false
        };

        match peek {
            b'0' => {
                self.eat_some(1);
                Ok(T::ZERO)
            }
            _ => {
                if let Some(number) = T::try_from_ascii_decimal(peek) {
                    self.eat_some(1);
                    if is_neg {
                        self.parse_negative_number(number.neg())
                    } else {
                        self.parse_positive_number(number)
                    }
                } else {
                    Err(Error::InvalidType)
                }
            }
        }
    }

    /// Parse a token and if match is found advance the cursor.
    ///
    /// Example tokens: `b"null"`, `b"true"`, `b"false"`.
    pub fn parse_token_content(&mut self, token: &[u8]) -> Result<()> {
        let size = token.len();
        if let Some(slice) = self.input.get(self.index..self.index + size) {
            if slice == token {
                self.eat_some(size);
                Ok(())
            } else {
                Err(Error::ExpectedToken)
            }
        } else {
            Err(Error::UnexpectedEof)
        }
    }

    /// Simple heuristics to decide float or integer,
    /// call this method ONLY after ensuring the peek character is '0'..='9'|'-'
    #[inline]
    fn parse_float_or_int(&mut self, peek: u8) -> Result<AnyNumber> {
        let is_negative = peek == b'-';
        let mut is_float = false;
        let input = &self.input[self.index..];
        let input = input
            .iter()
            .position(|&b| match b {
                b'0'..=b'9' | b'+' | b'-' => false,
                b'.' | b'e' | b'E' => {
                    is_float = true;
                    false
                }
                _ => true,
            })
            .map(|len| &input[..len])
            .unwrap_or(input);
        // SAFETY: We already checked that it only contains ASCII. This is only true if the
        // caller has guaranteed that `pattern` contains only ASCII characters.
        let s = unsafe { str::from_utf8_unchecked(input) };
        let num = if is_float {
            AnyNumber::Float(AnyFloat::from_str(s)?)
        } else if is_negative {
            AnyNumber::NegInt(i64::from_str(s)?)
        } else {
            AnyNumber::PosInt(u64::from_str(s)?)
        };
        self.eat_some(input.len());
        Ok(num)
    }

    /// Return a slice containing only number characters: `0..=9` and `+-.eE`
    #[inline]
    fn match_float(&self) -> &[u8] {
        let input = &self.input[self.index..];
        input
            .iter()
            .position(|&b| !matches!(b, b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'))
            .map(|len| &input[..len])
            .unwrap_or(input)
    }

    /// Consume whitespace and then parse a number as a float
    #[inline]
    fn parse_float<E, F: FromStr<Err = E>>(&mut self) -> Result<Option<F>>
    where
        Error: From<E>,
    {
        if b'n' == self.eat_whitespace()? {
            self.eat_some(1);
            self.parse_token_content(b"ull")?;
            return Ok(None);
        }
        let input = self.match_float();
        // SAFETY: We already checked that it only contains ASCII. This is only true if the
        // caller has guaranteed that `pattern` contains only ASCII characters.
        let s = unsafe { str::from_utf8_unchecked(input) };
        let v = F::from_str(s)?;
        self.eat_some(input.len());
        Ok(Some(v))
    }

    /// Eats whitespace and checks if the next character is a colon
    fn parse_key_colon(&mut self) -> Result<()> {
        if b':' == self.eat_whitespace()? {
            self.eat_some(1);
            Ok(())
        } else {
            Err(Error::ExpectedColon)
        }
    }

    /// Consume a content of a string until the closing `'"'`, ignoring all escape codes
    /// except immediately before any `'"'`.
    ///
    /// Call after consuming the initial `'"'`.
    pub fn eat_str_content(&mut self) -> Result<()> {
        let mut start = self.index;
        loop {
            if let Some(found) = self
                .input
                .get(start..)
                .and_then(|slice| slice.iter().position(|&b| b == QU || b <= 0x1F))
            {
                let end = start + found;
                // note: we ignore any invalid \ escape codes, but we check for control chars
                match self.input[end] {
                    QU => {
                        let count = self.input[start..end]
                            .iter()
                            .rev()
                            .position(|&b| b != RS)
                            .unwrap_or_else(|| end - start);
                        if count % 2 == 0 {
                            /* even number of '\' */
                            // println!("`{}'", core::str::from_utf8(&self.input[start..end]).unwrap());
                            self.index = end + 1;
                            return Ok(());
                        }
                        /* odd number of '/', continue */
                        start = end + 1;
                    }
                    _ => break Err(Error::StringControlChar),
                }
            } else {
                break Err(Error::UnexpectedEof);
            }
        }
    }
    /// Parse a string until a closing `'"'` is found, return a decoded `str` slice.
    ///
    /// Handles escape sequences using in-place copy, call after consuming an opening `'"'`
    pub fn parse_str_content(&mut self) -> Result<&'de str> {
        core::str::from_utf8(self.parse_str_bytes_content()?).map_err(From::from)
    }

    /// Parse a string until a closing `'"'` is found.
    /// Return decoded in-place string data on success.
    ///
    /// Handles escape sequences using in-place copy, call after consuming an opening `'"'`
    pub fn parse_str_bytes_content(&mut self) -> Result<&'de [u8]> {
        let mut index = self.index;
        let mut dest = index;
        let mut start = index;
        loop {
            // "....{dest}<-{gap}->{index}{start}..{end}..."
            if let Some(found) = self.input.get(start..).and_then(|slice|
                // println!("slice: {:?} {}", slice, core::str::from_utf8(&self.input[start..]).unwrap());
                /* search for either '\', '"' or a control character */
                slice.iter().position(|&b| matches!(b, RS|QU) || b <= 0x1F))
            {
                let end = start + found;
                let gap = index - dest;
                if gap != 0 {
                    self.input.copy_within(index..end, dest);
                }
                match self.input[end] {
                    QU => {
                        /* '"' found */
                        /* return as str and eat a gap with a closing '"' */
                        break Ok(self.split_input(end - gap - self.index, gap + 1));
                    }
                    RS => {
                        /* '\' found */
                        dest += end - index;
                        index = end + 1;
                        match self.input.get(index).copied() {
                            Some(QU | RS | SO) => {
                                /* preserve escaped */
                                start = index + 1;
                            }
                            Some(c @ (BB..=TT)) => {
                                /* control codes */
                                let unescaped = UNESCAPE[(c - BB) as usize];
                                if unescaped == 0 {
                                    break Err(Error::InvalidEscapeSequence);
                                }
                                self.input[dest] = unescaped;
                                dest += 1;
                                index += 1;
                                start = index;
                            }
                            Some(UU) => {
                                /* u0000 */
                                // let s = core::str::from_utf8(&self.input[index+1..index+5])?;
                                // let code = u32::from_str_radix(s, 16)?;
                                let code = self
                                    .input
                                    .get(index + 1..index + 5)
                                    .ok_or(Error::UnexpectedEof)?
                                    .try_into()
                                    .unwrap();
                                let code = parse_uuuu(code).ok_or(Error::InvalidEscapeSequence)?;
                                let ch =
                                    char::from_u32(code).ok_or(Error::InvalidUnicodeCodePoint)?;
                                dest += ch.encode_utf8(&mut self.input[dest..]).len();
                                index += 5;
                                start = index;
                            }
                            Some(..) => break Err(Error::InvalidEscapeSequence),
                            None => break Err(Error::UnexpectedEof),
                        }
                    }
                    _ => break Err(Error::StringControlChar),
                }
            } else {
                break Err(Error::UnexpectedEof);
            }
        }
    }

    /// Parse a string as pairs of hexadecimal nibbles until a closing `'"'` is found.
    /// Return decoded in-place binary data on success.
    ///
    /// Call after consuming an opening `'"'`.
    pub fn parse_hex_bytes_content(&mut self) -> Result<&'de [u8]> {
        let input = self.input_mut()?;
        let cells = Cell::from_mut(input).as_slice_of_cells();
        let mut src = cells.chunks_exact(2);
        let mut len = 0;
        let mut iter = src.by_ref().zip(cells.iter());
        while let Some(([a, b], t)) = iter.next() {
            if let Some(n) = parse_hex_nib(a.get()) {
                if let Some(m) = parse_hex_nib(b.get()) {
                    t.set((n << 4) + m);
                } else {
                    return Err(Error::UnexpectedChar);
                }
            } else if a.get() == QU {
                return Ok(self.split_input(len, len + 1));
            } else {
                return Err(Error::UnexpectedChar);
            }
            len += 1;
        }
        match src.remainder() {
            [c] if c.get() == QU => Ok(self.split_input(len, len + 1)),
            _ => Err(Error::UnexpectedEof),
        }
    }

    /// Parse a string as BASE-64 encoded bytes until a closing '"' is found.
    /// Return decoded in-place binary data on success.
    ///
    /// Call after consuming an opening `'"'`.
    pub fn parse_base64_bytes_content(&mut self) -> Result<&'de [u8]> {
        let input = self.input_mut()?;
        let (dlen, mut elen) = crate::base64::decode(input);
        match input.get(elen) {
            Some(&QU) => Ok(self.split_input(dlen, elen + 1 - dlen)),
            Some(&b'=') => {
                /* eat padding */
                if let Some(pos) = input
                    .get(elen + 1..)
                    .and_then(|slice| slice.iter().position(|&b| b != b'='))
                {
                    elen = elen + 1 + pos;
                    return if input[elen] == QU {
                        Ok(self.split_input(dlen, elen + 1 - dlen))
                    } else {
                        Err(Error::UnexpectedChar)
                    };
                }
                Err(Error::UnexpectedEof)
            }
            Some(..) => Err(Error::UnexpectedChar),
            None => Err(Error::UnexpectedEof),
        }
    }

    fn parse_array_bytes_content(&mut self) -> Result<&'de [u8]> {
        if b']' == self.eat_whitespace()? {
            return Ok(self.split_input(0, 1));
        }
        /* save index */
        let start = self.index;
        let mut index = start;
        #[allow(unused_variables)]
        #[allow(clippy::let_unit_value)]
        let input = {
            #[cfg(debug_assertions)]
            #[allow(clippy::unused_unit)]
            {
                ()
            }
            #[cfg(not(debug_assertions))]
            {
                self.input.as_mut_ptr()
            }
        };
        loop {
            let byte = self.parse_unsigned()?;
            #[cfg(debug_assertions)]
            {
                self.input[index] = byte;
            }
            #[cfg(not(debug_assertions))]
            {
                // SAFETY: depends on parse_unsigned to validate if there is enough room in input
                // any number in ASCII is >= byte
                unsafe {
                    input.add(index).write(byte);
                }
            }
            index += 1;
            match self.eat_whitespace()? {
                b',' => self.eat_some(1),
                b']' => break,
                _ => return Err(Error::UnexpectedChar),
            }
        }
        let offs = self.index + 1 - index;
        /* restore index back */
        self.index = start;
        Ok(self.split_input(index - start, offs))
    }
}

impl<'de> StringByteDecoder<'de> for StringByteNopeDecoder {
    #[inline(always)]
    fn decode_string_to_bytes(de: &mut Deserializer<'de, Self>) -> Result<&'de [u8]> {
        de.parse_str_bytes_content()
    }
}

impl<'de> StringByteDecoder<'de> for StringByteHexDecoder {
    #[inline(always)]
    fn decode_string_to_bytes(de: &mut Deserializer<'de, Self>) -> Result<&'de [u8]> {
        de.parse_hex_bytes_content()
    }
}

impl<'de> StringByteDecoder<'de> for StringByteBase64Decoder {
    #[inline(always)]
    fn decode_string_to_bytes(de: &mut Deserializer<'de, Self>) -> Result<&'de [u8]> {
        de.parse_base64_bytes_content()
    }
}

impl<'de, 'a, P> de::Deserializer<'de> for &'a mut Deserializer<'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.eat_whitespace()? {
            b'n' => self.deserialize_unit(visitor),
            b't' | b'f' => self.deserialize_bool(visitor),
            b'"' => self.deserialize_str(visitor),
            c @ (b'0'..=b'9' | b'-') => match self.parse_float_or_int(c)? {
                AnyNumber::PosInt(n) => visitor.visit_u64(n),
                AnyNumber::NegInt(n) => visitor.visit_i64(n),
                #[cfg(feature = "de-any-f32")]
                AnyNumber::Float(f) => visitor.visit_f32(f),
                #[cfg(not(feature = "de-any-f32"))]
                AnyNumber::Float(f) => visitor.visit_f64(f),
            },
            b'[' => self.deserialize_seq(visitor),
            b'{' => self.deserialize_map(visitor),
            _ => Err(Error::UnexpectedChar),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let boolean = match self.eat_whitespace()? {
            b't' => {
                self.eat_some(1);
                self.parse_token_content(b"rue")?;
                true
            }
            b'f' => {
                self.eat_some(1);
                self.parse_token_content(b"alse")?;
                false
            }
            _ => return Err(Error::UnexpectedChar),
        };
        visitor.visit_bool(boolean)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i8(self.parse_signed()?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i16(self.parse_signed()?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.parse_signed()?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(self.parse_signed()?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(self.parse_unsigned()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(self.parse_unsigned()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.parse_unsigned()?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(self.parse_unsigned()?)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f32(self.parse_float()?.unwrap_or(f32::NAN))
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f64(self.parse_float()?.unwrap_or(f64::NAN))
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if b'"' == self.eat_whitespace()? {
            self.eat_some(1);
            let s = self.parse_str_content()?;
            let ch = char::from_str(s).map_err(|_| Error::InvalidLength)?;
            visitor.visit_char(ch)
        } else {
            Err(Error::ExpectedString)
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if b'"' == self.eat_whitespace()? {
            self.eat_some(1);
            visitor.visit_borrowed_str(self.parse_str_content()?)
        } else {
            Err(Error::ExpectedString)
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let bytes = match self.eat_whitespace()? {
            b'"' => {
                self.eat_some(1);
                P::decode_string_to_bytes(&mut *self)?
            }
            b'[' => {
                self.eat_some(1);
                self.parse_array_bytes_content()?
            }
            _ => return Err(Error::UnexpectedChar),
        };
        visitor.visit_borrowed_bytes(bytes)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.eat_whitespace()? {
            b'n' => {
                self.eat_some(1);
                self.parse_token_content(b"ull")?;
                visitor.visit_none()
            }
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.eat_whitespace()? {
            b'n' => {
                self.eat_some(1);
                self.parse_token_content(b"ull")?;
                visitor.visit_unit()
            }
            _ => Err(Error::ExpectedNull),
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    // As is done here, serializers are encouraged to treat newtype structs as
    // insignificant wrappers around the data they contain. That means not
    // parsing anything other than the contained value.
    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if b'[' == self.eat_whitespace()? {
            self.eat_some(1);
            let value = visitor.visit_seq(CommaSeparated::new(self))?;
            if b']' == self.eat_whitespace()? {
                self.eat_some(1);
                Ok(value)
            } else {
                Err(Error::ExpectedArrayEnd)
            }
        } else {
            Err(Error::ExpectedArray)
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if b'{' == self.eat_whitespace()? {
            self.eat_some(1);
            let value = visitor.visit_map(CommaSeparated::new(self))?;
            if b'}' == self.eat_whitespace()? {
                self.eat_some(1);
                Ok(value)
            } else {
                Err(Error::ExpectedObjectEnd)
            }
        } else {
            Err(Error::ExpectedObject)
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.eat_whitespace()? {
            b'{' => self.deserialize_map(visitor),
            b'[' => self.deserialize_seq(visitor),
            _ => Err(Error::ExpectedStruct),
        }
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.eat_whitespace()? {
            b'"' => visitor.visit_enum(UnitVariantAccess { de: self }),
            b'{' => {
                self.eat_some(1);
                let value = visitor.visit_enum(VariantAccess { de: self })?;
                if b'}' == self.eat_whitespace()? {
                    self.eat_some(1);
                    Ok(value)
                } else {
                    Err(Error::ExpectedEnumObjectEnd)
                }
            }
            _ => Err(Error::ExpectedEnumValue),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.eat_whitespace()? {
            b'n' => self.deserialize_unit(visitor),
            b't' | b'f' => self.deserialize_bool(visitor),
            b'"' => {
                self.eat_some(1);
                self.eat_str_content()?;
                visitor.visit_unit()
            }
            b'0'..=b'9' | b'-' => {
                let len = self.match_float().len();
                self.eat_some(len);
                visitor.visit_unit()
            }
            b'[' => self.deserialize_seq(visitor),
            b'{' => self.deserialize_map(visitor),
            _ => Err(Error::UnexpectedChar),
        }
    }
}

struct CommaSeparated<'a, 'de: 'a, P> {
    de: &'a mut Deserializer<'de, P>,
    first: bool,
}

impl<'a, 'de, P> CommaSeparated<'a, 'de, P> {
    fn new(de: &'a mut Deserializer<'de, P>) -> Self {
        CommaSeparated { de, first: true }
    }
}

impl<'de, 'a, P> SeqAccess<'de> for CommaSeparated<'a, 'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        match self.de.eat_whitespace()? {
            b']' => return Ok(None),
            b',' => {
                if self.first {
                    return Err(Error::LeadingArrayComma);
                } else {
                    self.de.eat_some(1);
                    if b']' == self.de.eat_whitespace()? {
                        return Err(Error::TrailingArrayComma);
                    }
                }
            }
            _ => {
                if self.first {
                    self.first = false;
                } else {
                    return Err(Error::ExpectedArrayCommaOrEnd);
                }
            }
        }
        seed.deserialize(&mut *self.de).map(Some)
    }
}

impl<'a, 'de, P> MapAccess<'de> for CommaSeparated<'a, 'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: DeserializeSeed<'de>,
    {
        let peek = match self.de.eat_whitespace()? {
            b'}' => return Ok(None),
            b',' => {
                if self.first {
                    return Err(Error::LeadingObjectComma);
                } else {
                    self.de.eat_some(1);
                    match self.de.eat_whitespace()? {
                        b'}' => return Err(Error::TrailingObjectComma),
                        ch => ch,
                    }
                }
            }
            ch => {
                if self.first {
                    self.first = false;
                    ch
                } else {
                    return Err(Error::ExpectedObjectCommaOrEnd);
                }
            }
        };
        if peek == b'"' {
            seed.deserialize(MapKey { de: &mut *self.de }).map(Some)
        } else {
            Err(Error::KeyMustBeAString)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: DeserializeSeed<'de>,
    {
        self.de.parse_key_colon()?;
        seed.deserialize(&mut *self.de)
    }
}

struct MapKey<'a, 'de, P> {
    de: &'a mut Deserializer<'de, P>,
}

impl<'de, 'a, P> MapKey<'a, 'de, P> {
    #[inline]
    fn parse_unsigned_numkey<T: NumParseTool>(self) -> Result<T> {
        self.de.eat_some(1); // eat '"', the presence of which is checked in MapAccess
        let n = self.de.parse_unsigned()?;
        // check if we have a closing '"' immediately following a number
        if b'"' == self.de.peek()? {
            self.de.eat_some(1);
            Ok(n)
        } else {
            Err(Error::InvalidNumber)
        }
    }

    #[inline]
    fn parse_signed_numkey<T>(self) -> Result<T>
    where
        T: NumParseTool + CheckedSub + Neg<Output = T>,
    {
        self.de.eat_some(1); // eat '"', the presence of which is checked in MapAccess
        let n = self.de.parse_signed()?;
        // check if we have a closing '"' immediately following a number
        if b'"' == self.de.peek()? {
            self.de.eat_some(1);
            Ok(n)
        } else {
            Err(Error::InvalidNumber)
        }
    }
}

// attempt to deserialize integers directly from string keys if that's what the type expects
impl<'de, 'a, P> de::Deserializer<'de> for MapKey<'a, 'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.de.deserialize_str(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.de.deserialize_char(visitor)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i8(self.parse_signed_numkey()?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i16(self.parse_signed_numkey()?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.parse_signed_numkey()?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(self.parse_signed_numkey()?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(self.parse_unsigned_numkey()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(self.parse_unsigned_numkey()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.parse_unsigned_numkey()?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(self.parse_unsigned_numkey()?)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.de.eat_some(1); // eat '"', the presence of which is checked in MapAccess
        let b = self.de.deserialize_bool(visitor)?;
        if b'"' == self.de.peek()? {
            self.de.eat_some(1);
            Ok(b)
        } else {
            Err(Error::InvalidType)
        }
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(UnitVariantAccess { de: self.de })
    }

    forward_to_deserialize_any! {
        i128 u128 f32 f64 string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

struct UnitVariantAccess<'a, 'de, P> {
    de: &'a mut Deserializer<'de, P>,
}

impl<'a, 'de, P> de::EnumAccess<'de> for UnitVariantAccess<'a, 'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self)>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(&mut *self.de)?;
        Ok((variant, self))
    }
}

impl<'a, 'de, P> de::VariantAccess<'de> for UnitVariantAccess<'a, 'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value>
    where
        T: de::DeserializeSeed<'de>,
    {
        Err(Error::InvalidType)
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::InvalidType)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::InvalidType)
    }
}

struct VariantAccess<'a, 'de, P> {
    de: &'a mut Deserializer<'de, P>,
}

impl<'a, 'de, P> de::EnumAccess<'de> for VariantAccess<'a, 'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self)>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(&mut *self.de)?;
        self.de.parse_key_colon()?;
        Ok((variant, self))
    }
}

impl<'a, 'de, P> de::VariantAccess<'de> for VariantAccess<'a, 'de, P>
where
    P: StringByteDecoder<'de>,
{
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Err(Error::InvalidType)
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.de)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.de, visitor)
    }

    fn struct_variant<V>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        de::Deserializer::deserialize_struct(self.de, "", fields, visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ser_write::{SerWrite, SliceWriter};
    #[cfg(all(feature = "alloc", not(feature = "std")))]
    use alloc::{collections::BTreeMap, format, vec, vec::Vec};
    use serde::Deserialize;
    #[cfg(feature = "std")]
    use std::{collections::BTreeMap, format, vec, vec::Vec};

    #[test]
    fn test_parse_str_content() {
        let mut test = [0; 1];
        test.copy_from_slice(br#"""#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.parse_str_content().unwrap(), "");

        let mut test = [0; 13];
        test.copy_from_slice(br#"Hello World!""#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.parse_str_content().unwrap(), "Hello World!");
        assert!(deser.input.is_empty());
        assert_eq!(deser.index, 0);

        let mut test = [0; 46];
        test.copy_from_slice(br#"\u0020Hello\r\\ \b\nW\tor\fld\u007Fy\u0306!\"""#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(
            deser.parse_str_content().unwrap(),
            " Hello\r\\ \x08\nW\tor\x0cld\x7fy̆!\""
        );
        assert!(deser.input.is_empty());
        assert_eq!(deser.index, 0);

        let mut test = [0; 13];
        test.copy_from_slice(br#"Hello World!""#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.parse_str_content().unwrap(), "Hello World!");
        assert!(deser.input.is_empty());
        assert_eq!(deser.index, 0);

        let mut test = [0; 0];
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.parse_str_content(), Err(Error::UnexpectedEof));
        let mut test = [0; 2];
        test.copy_from_slice(b"\n\"");
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.parse_str_content(), Err(Error::StringControlChar));
        let mut test = [0; 1];
        test.copy_from_slice(br"\");
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.parse_str_content(), Err(Error::UnexpectedEof));

        let mut test = [0; 1];
        test.copy_from_slice(br#"""#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.eat_str_content(), Ok(()));
        let mut test = [0; 3];
        test.copy_from_slice(br#"\\""#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.eat_str_content(), Ok(()));
        let mut test = [0; 0];
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.eat_str_content(), Err(Error::UnexpectedEof));
        let mut test = [0; 3];
        test.copy_from_slice(br#" 1_"#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.eat_str_content(), Err(Error::UnexpectedEof));
        let mut test = [0; 2];
        test.copy_from_slice(br#"\""#);
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.eat_str_content(), Err(Error::UnexpectedEof));
        let mut test = [0; 3];
        test.copy_from_slice(b" \t ");
        let mut deser = DeserializerNopeByteStr::from_mut_slice(&mut test);
        assert_eq!(deser.eat_str_content(), Err(Error::StringControlChar));
    }

    #[test]
    fn test_deserializer() {
        let mut test = [0; 14];
        let s: &str = {
            test.copy_from_slice(br#""Hello World!""#);
            from_mut_slice(&mut test).unwrap()
        };
        assert_eq!(s, "Hello World!");
        let mut test = [0; 21];
        let s: &str = {
            test.copy_from_slice(br#" "Hello\tWorld!\r\n" "#);
            from_mut_slice(&mut test).unwrap()
        };
        assert_eq!(s, "Hello\tWorld!\r\n");
        let mut test = [0; 57];
        let tup: (i8, u32, i64, f32, f64) = {
            test.copy_from_slice(br#" [ 0 , 4294967295, -9223372036854775808 ,3.14 , 1.2e+8 ] "#);
            from_mut_slice(&mut test).unwrap()
        };
        assert_eq!(
            tup,
            (0i8, 4294967295u32, -9223372036854775808i64, 3.14f32, 1.2e+8)
        );
        let mut test = [0; 40];
        let ary: [&str; 3] = {
            test.copy_from_slice(br#" ["one\u0031", "\u0032two", "\u003333"] "#);
            from_mut_slice(&mut test).unwrap()
        };
        assert_eq!(ary, ["one1", "2two", "333"]);
    }

    #[test]
    fn test_de_bytes() {
        let mut test = [0; 2];
        test.copy_from_slice(b"[]");
        let bytes: &[u8] = from_mut_slice(&mut test).unwrap();
        assert_eq!(bytes, b"");

        let mut test = [0; 2];
        test.copy_from_slice(br#""""#);
        let bytes: &[u8] = from_mut_slice(&mut test).unwrap();
        assert_eq!(bytes, b"");

        let mut test = [0; 12];
        test.copy_from_slice(br#""Hello!\r\n""#);
        let bytes: &[u8] = from_mut_slice(&mut test).unwrap();
        assert_eq!(bytes, b"Hello!\r\n");

        let mut test = [0; 3];
        test.copy_from_slice(b"[0]");
        let bytes: &[u8] = from_mut_slice(&mut test).unwrap();
        assert_eq!(bytes, [0]);

        let mut test = [0; 10];
        test.copy_from_slice(b"[0,1 , 2 ]");
        let bytes: &[u8] = from_mut_slice(&mut test).unwrap();
        assert_eq!(bytes, [0, 1, 2]);

        let mut test = [0; 10];
        test.copy_from_slice(br#""Ff00ABab""#);
        let bytes: &[u8] = from_mut_slice_hex_bytes(&mut test).unwrap();
        assert_eq!(bytes, [0xff, 0x00, 0xab, 0xab]);

        let mut test = [0; 4];
        test.copy_from_slice(br#""ABC"#);
        assert_eq!(
            from_mut_slice_hex_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 4];
        test.copy_from_slice(br#""ABx"#);
        assert_eq!(
            from_mut_slice_hex_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 5];
        test.copy_from_slice(br#""ABCx"#);
        assert_eq!(
            from_mut_slice_hex_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );
        let mut test = [0; 5];
        test.copy_from_slice(br#""ABxy"#);
        assert_eq!(
            from_mut_slice_hex_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );
        let mut test = [0; 0];
        assert_eq!(
            from_mut_slice_hex_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 3];
        test.copy_from_slice(br#""\""#);
        assert_eq!(
            from_mut_slice_hex_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );

        let mut test = [0; 13];
        test.copy_from_slice(br#""/wCrqw=====""#);
        let bytes: &[u8] = from_mut_slice_base64_bytes(&mut test).unwrap();
        assert_eq!(bytes, [0xff, 0x00, 0xab, 0xab]);
        let mut test = [0; 10];
        test.copy_from_slice(br#""/wCrqw==""#);
        let bytes: &[u8] = from_mut_slice_base64_bytes(&mut test).unwrap();
        assert_eq!(bytes, [0xff, 0x00, 0xab, 0xab]);
        let mut test = [0; 9];
        test.copy_from_slice(br#""/wCrqw=""#);
        let bytes: &[u8] = from_mut_slice_base64_bytes(&mut test).unwrap();
        assert_eq!(bytes, [0xff, 0x00, 0xab, 0xab]);
        let mut test = [0; 8];
        test.copy_from_slice(br#""/wCrqw""#);
        let bytes: &[u8] = from_mut_slice_base64_bytes(&mut test).unwrap();
        assert_eq!(bytes, [0xff, 0x00, 0xab, 0xab]);

        let mut test = [0; 2];
        test.copy_from_slice(br#""\"#);
        assert_eq!(
            from_mut_slice_base64_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );
        let mut test = [0; 1];
        test.copy_from_slice(br#"""#);
        assert_eq!(
            from_mut_slice_base64_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 2];
        test.copy_from_slice(br#""="#);
        assert_eq!(
            from_mut_slice_base64_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 3];
        test.copy_from_slice(br#""//"#);
        assert_eq!(
            from_mut_slice_base64_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 5];
        test.copy_from_slice(br#""//=="#);
        assert_eq!(
            from_mut_slice_base64_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 5];
        test.copy_from_slice(br#""//=x"#);
        assert_eq!(
            from_mut_slice_base64_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );
        let mut test = [0; 7];
        test.copy_from_slice(br#""//===x"#);
        assert_eq!(
            from_mut_slice_base64_bytes::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );

        let mut test = [0; 0];
        test.copy_from_slice(b"");
        assert!(from_mut_slice_hex_bytes::<&[u8]>(&mut test).is_err());
        let mut test = [0; 1];
        test.copy_from_slice(br#"""#);
        assert!(from_mut_slice_hex_bytes::<&[u8]>(&mut test).is_err());
        let mut test = [0; 3];
        test.copy_from_slice(br#""0""#);
        assert!(from_mut_slice_hex_bytes::<&[u8]>(&mut test).is_err());
        let mut test = [0; 5];
        test.copy_from_slice(br#""ABC""#);
        assert!(from_mut_slice_hex_bytes::<&[u8]>(&mut test).is_err());
        let mut test = [0; 4];
        test.copy_from_slice(br#""Xy""#);
        assert!(from_mut_slice_hex_bytes::<&[u8]>(&mut test).is_err());
        let mut test = [0; 1];
        test.copy_from_slice(b"[");
        assert!(from_mut_slice::<&[u8]>(&mut test).is_err());
        let mut test = [0; 4];
        test.copy_from_slice(b"[-1]");
        assert!(from_mut_slice::<&[u8]>(&mut test).is_err());
        let mut test = [0; 5];
        test.copy_from_slice(b"[256]");
        assert!(from_mut_slice::<&[u8]>(&mut test).is_err());
        let mut test = [0; 3];
        test.copy_from_slice(b"[,]");
        assert!(from_mut_slice::<&[u8]>(&mut test).is_err());
        let mut test = [0; 4];
        test.copy_from_slice(b"[0,]");
        assert!(from_mut_slice::<&[u8]>(&mut test).is_err());
        let mut test = [0; 1];
        test.copy_from_slice(br#"["#);
        assert_eq!(
            from_mut_slice::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 2];
        test.copy_from_slice(br#"[0"#);
        assert_eq!(
            from_mut_slice::<&[u8]>(&mut test),
            Err(Error::UnexpectedEof)
        );
        let mut test = [0; 3];
        test.copy_from_slice(br#"[0."#);
        assert_eq!(
            from_mut_slice::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );
        let mut test = [0; 1];
        test.copy_from_slice(br#"{"#);
        assert_eq!(
            from_mut_slice::<&[u8]>(&mut test),
            Err(Error::UnexpectedChar)
        );

        use serde::Serialize;

        #[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        struct Test<'a> {
            #[serde(with = "serde_bytes", skip_serializing_if = "Option::is_none")]
            borrowed: Option<&'a [u8]>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tail: Option<bool>,
        }
        let mut buf = [0u8; 52];
        let mut writer = SliceWriter::new(&mut buf);
        let mut test = Test {
            borrowed: Some(&[0, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 255]),
            ..Test::default()
        };
        let expected = br#"{"borrowed":[0,10,11,12,13,14,15,16,17,18,19,255]}"#;
        crate::to_writer(&mut writer, &test).unwrap();
        assert_eq!(&writer.as_ref(), expected);
        assert_eq!(from_mut_slice::<Test>(writer.split().0).unwrap(), test);

        let mut writer = SliceWriter::new(&mut buf);
        writer
            .write(br#" { "borrowed" : [  255, 127, 128, 0  ] } "#)
            .unwrap();
        assert_eq!(
            from_mut_slice::<Test>(writer.split().0).unwrap(),
            Test {
                borrowed: Some(&[255, 127, 128, 0]),
                ..Test::default()
            }
        );

        let mut writer = SliceWriter::new(&mut buf);
        test.tail = Some(false);
        let expected = br#"{"borrowed":"000A0B0C0D0E0F10111213FF","tail":false}"#;
        crate::to_writer_hex_bytes(&mut writer, &test).unwrap();
        assert_eq!(&writer.as_ref(), expected);
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(writer.split().0).unwrap(),
            test
        );

        let mut writer = SliceWriter::new(&mut buf);
        writer
            .write(br#" { "tail" :true ,"borrowed": "DEADBACA9970" } "#)
            .unwrap();
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(writer.split().0).unwrap(),
            Test {
                tail: Some(true),
                borrowed: Some(&[0xde, 0xad, 0xba, 0xca, 0x99, 0x70]),
                ..Test::default()
            }
        );

        let mut writer = SliceWriter::new(&mut buf);
        test.tail = Some(false);
        let expected = br#"{"borrowed":"AAoLDA0ODxAREhP/","tail":false}"#;
        crate::to_writer_base64_bytes(&mut writer, &test).unwrap();
        assert_eq!(&writer.as_ref(), expected);
        assert_eq!(
            from_mut_slice_base64_bytes::<Test>(writer.split().0).unwrap(),
            test
        );

        let mut writer = SliceWriter::new(&mut buf);
        writer
            .write(br#" { "tail" :true ,"borrowed": "ABCDefgh" } "#)
            .unwrap();
        assert_eq!(
            from_mut_slice_base64_bytes::<Test>(writer.split().0).unwrap(),
            Test {
                tail: Some(true),
                borrowed: Some(&[0, 16, 131, 121, 248, 33]),
                ..Test::default()
            }
        );

        let mut writer = SliceWriter::new(&mut buf);
        writer
            .write(br#" { "borrowed": [  ] , "tail" :  false}  "#)
            .unwrap();
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(writer.split().0).unwrap(),
            Test {
                tail: Some(false),
                borrowed: Some(b"")
            }
        );

        let mut writer = SliceWriter::new(&mut buf);
        writer
            .write(br#"{"tail":null,"owned":[],"borrowed":""}"#)
            .unwrap();
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(writer.split().0).unwrap(),
            Test {
                borrowed: Some(b""),
                tail: None
            }
        );

        let mut writer = SliceWriter::new(&mut buf);
        writer.write(br#" {   }  "#).unwrap();
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(writer.split().0).unwrap(),
            Test::default()
        );
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_de_bytes_own() {
        use serde::Serialize;

        #[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        struct Test<'a> {
            #[serde(with = "serde_bytes", skip_serializing_if = "Option::is_none")]
            owned: Option<Vec<u8>>,
            #[serde(with = "serde_bytes", skip_serializing_if = "Option::is_none")]
            borrowed: Option<&'a [u8]>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tail: Option<bool>,
        }

        let mut vec = Vec::new();
        let mut test = Test {
            owned: Some(vec![0, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 255]),
            ..Test::default()
        };
        let expected = br#"{"owned":[0,10,11,12,13,14,15,16,17,18,19,255]}"#;
        crate::to_writer(&mut vec, &test).unwrap();
        assert_eq!(&vec, expected);
        assert_eq!(from_mut_slice::<Test>(&mut vec).unwrap(), test);

        vec.clear();
        vec.extend_from_slice(br#" { "owned" : [  255, 127, 128, 0  ] } "#);
        assert_eq!(
            from_mut_slice::<Test>(&mut vec).unwrap(),
            Test {
                owned: Some(vec![255, 127, 128, 0]),
                ..Test::default()
            }
        );

        vec.clear();
        test.tail = Some(false);
        let expected = br#"{"owned":"000A0B0C0D0E0F10111213FF","tail":false}"#;
        crate::to_writer_hex_bytes(&mut vec, &test).unwrap();
        assert_eq!(&vec, expected);
        assert_eq!(from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(), test);

        vec.clear();
        vec.extend_from_slice(br#" { "tail" :true ,"owned": "DEADBACA9970" } "#);
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(),
            Test {
                tail: Some(true),
                owned: Some(vec![0xde, 0xad, 0xba, 0xca, 0x99, 0x70]),
                ..Test::default()
            }
        );

        vec.clear();
        test.tail = Some(false);
        let expected = br#"{"owned":"AAoLDA0ODxAREhP/","tail":false}"#;
        crate::to_writer_base64_bytes(&mut vec, &test).unwrap();
        assert_eq!(&vec, expected);
        assert_eq!(from_mut_slice_base64_bytes::<Test>(&mut vec).unwrap(), test);

        vec.clear();
        vec.extend_from_slice(br#" { "tail" :true ,"owned": "ABCDefgh" } "#);
        assert_eq!(
            from_mut_slice_base64_bytes::<Test>(&mut vec).unwrap(),
            Test {
                tail: Some(true),
                owned: Some(vec![0, 16, 131, 121, 248, 33]),
                ..Test::default()
            }
        );

        vec.clear();
        let mut test = Test {
            borrowed: Some(&[0, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 255]),
            ..Test::default()
        };
        let expected = br#"{"borrowed":[0,10,11,12,13,14,15,16,17,18,19,255]}"#;
        crate::to_writer(&mut vec, &test).unwrap();
        assert_eq!(&vec, expected);
        assert_eq!(from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(), test);

        vec.clear();
        vec.extend_from_slice(br#" { "borrowed" : [  255, 127, 128, 0  ] ,"tail"  :false}"#);
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(),
            Test {
                borrowed: Some(&[255, 127, 128, 0]),
                tail: Some(false),
                ..Test::default()
            }
        );

        vec.clear();
        vec.extend_from_slice(br#" { "borrowed" : "DEADBACA9970" ,"tail"  :null, "owned":null } "#);
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(),
            Test {
                borrowed: Some(&[0xde, 0xad, 0xba, 0xca, 0x99, 0x70]),
                ..Test::default()
            }
        );

        vec.clear();
        test.tail = Some(true);
        let expected = br#"{"borrowed":"000A0B0C0D0E0F10111213FF","tail":true}"#;
        crate::to_writer_hex_bytes(&mut vec, &test).unwrap();
        assert_eq!(&vec, expected);
        assert_eq!(from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(), test);

        vec.clear();
        let expected = br#"{"borrowed":"AAoLDA0ODxAREhP/","tail":true}"#;
        crate::to_writer_base64_bytes(&mut vec, &test).unwrap();
        assert_eq!(&vec, expected);
        assert_eq!(from_mut_slice_base64_bytes::<Test>(&mut vec).unwrap(), test);

        vec.clear();
        vec.extend_from_slice(br#" { "borrowed": [  ] , "tail" :  false ,  "owned"   :  "" }  "#);
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(),
            Test {
                borrowed: Some(&[]),
                tail: Some(false),
                owned: Some(vec![])
            }
        );

        vec.clear();
        vec.extend_from_slice(br#"{"tail":null,"owned":[],"borrowed":""}"#);
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(),
            Test {
                borrowed: Some(&[]),
                tail: None,
                owned: Some(vec![])
            }
        );

        vec.clear();
        vec.extend_from_slice(br#" {   }  "#);
        assert_eq!(
            from_mut_slice_hex_bytes::<Test>(&mut vec).unwrap(),
            Test::default()
        );
    }

    #[derive(Debug, Deserialize, PartialEq)]
    enum Type {
        #[serde(rename = "boolean")]
        Boolean,
        #[serde(rename = "number")]
        Number,
        #[serde(rename = "thing")]
        Thing,
    }

    fn from_str<T>(s: &str) -> Result<(T, usize)>
    where
        for<'a> T: de::Deserialize<'a>,
    {
        let mut buf = [0u8; 256];
        from_bufstr(&mut buf, s)
    }

    fn from_bufstr<'a, T>(buf: &'a mut [u8], s: &str) -> Result<(T, usize)>
    where
        T: de::Deserialize<'a>,
    {
        let mut writer = SliceWriter::new(buf);
        writer.write(s.as_bytes()).unwrap();
        let len = writer.len();
        let res: T = from_mut_slice(writer.split().0)?;
        Ok((res, len))
    }

    #[test]
    fn test_de_array() {
        assert_eq!(from_str::<[i32; 0]>("[]"), Ok(([], 2)));
        assert_eq!(from_str("[0, 1, 2]"), Ok(([0, 1, 2], 9)));
        // error
        assert_eq!(from_str::<[i32; 2]>(""), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<[i32; 2]>("{}"), Err(Error::ExpectedArray));
        assert_eq!(from_str::<[i32; 2]>("[0, 1,"), Err(Error::ExpectedArrayEnd));
        assert_eq!(
            from_str::<[i32; 3]>("[0, 1,]"),
            Err(Error::TrailingArrayComma)
        );
        assert_eq!(from_str::<[i32; 2]>("[,]"), Err(Error::LeadingArrayComma));
        assert_eq!(from_str::<[i32; 2]>("[, 0]"), Err(Error::LeadingArrayComma));
        assert_eq!(
            from_str::<[i32; 2]>("[0}"),
            Err(Error::ExpectedArrayCommaOrEnd)
        );
        assert_eq!(from_str::<[i32; 2]>("["), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<[i32; 2]>("[0"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<[i32; 2]>("[0,"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<[i32; 2]>("[0,1"), Err(Error::UnexpectedEof));
    }

    #[test]
    fn test_de_bool() {
        assert_eq!(from_str("true"), Ok((true, 4)));
        assert_eq!(from_str(" true"), Ok((true, 5)));
        assert_eq!(from_str("true "), Ok((true, 5)));

        assert_eq!(from_str("false"), Ok((false, 5)));
        assert_eq!(from_str(" false"), Ok((false, 6)));
        assert_eq!(from_str("false "), Ok((false, 6)));
        // error
        assert_eq!(from_str::<bool>(""), Err(Error::UnexpectedEof));
        assert_eq!(
            from_str::<bool>("true false"),
            Err(Error::TrailingCharacters)
        );
        assert_eq!(from_str::<bool>("tru"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<bool>("truy"), Err(Error::ExpectedToken));
        assert_eq!(from_str::<bool>("fals"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<bool>("falsy"), Err(Error::ExpectedToken));
        assert_eq!(from_str::<bool>("n"), Err(Error::UnexpectedChar));
    }

    #[test]
    fn test_de_floating_point() {
        assert_eq!(from_str("5.0"), Ok((5.0, 3)));
        assert_eq!(from_str("1"), Ok((1.0, 1)));
        assert_eq!(from_str("-999.9"), Ok((-999.9, 6)));
        assert_eq!(from_str("1e5"), Ok((1e5, 3)));
        let (f, len): (f32, _) = from_str("null").unwrap();
        assert_eq!(len, 4);
        assert!(f.is_nan());
        let (f, len): (f64, _) = from_str("null").unwrap();
        assert_eq!(len, 4);
        assert!(f.is_nan());
        assert_eq!(from_str::<f32>(""), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<f32>("n"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<f32>(","), Err(Error::InvalidNumber));
        assert_eq!(from_str::<f64>(","), Err(Error::InvalidNumber));
        assert_eq!(from_str::<f32>("-"), Err(Error::InvalidNumber));
        assert_eq!(from_str::<f64>("-"), Err(Error::InvalidNumber));
    }

    #[test]
    fn test_de_integer() {
        macro_rules! test_de_signed {
            ($($ty:ty),*) => {$(
                assert_eq!(from_str::<$ty>("-128"), Ok((-128, 4)));
                assert_eq!(from_str::<$ty>("-1"), Ok((-1, 2)));
                assert_eq!(from_str::<$ty>("-0"), Ok((0, 2)));
                assert_eq!(from_str::<$ty>("-01"), Err(Error::TrailingCharacters));
                assert_eq!(from_str::<$ty>("-"), Err(Error::UnexpectedEof));
                assert_eq!(from_str::<$ty>("-1234567890123456789012345678901234567890"), Err(Error::InvalidNumber));
            )*};
        }
        macro_rules! test_de_unsigned {
            ($($ty:ty),*) => {$(
                assert_eq!(from_str::<$ty>("0"), Ok((0, 1)));
                assert_eq!(from_str::<$ty>("1"), Ok((1, 1)));
                assert_eq!(from_str::<$ty>("-1"), Err(Error::InvalidNumber));
                assert_eq!(from_str::<$ty>("-01"), Err(Error::InvalidNumber));
            )*};
        }
        macro_rules! test_de_int {
            ($($ty:ty),*) => {$(
                assert_eq!(from_str::<$ty>("5"), Ok((5, 1)));
                assert_eq!(from_str::<$ty>("127"), Ok((127, 3)));
                assert_eq!(from_str::<$ty>(","), Err(Error::InvalidType));
                assert_eq!(from_str::<$ty>(""), Err(Error::UnexpectedEof));
                assert_eq!(from_str::<$ty>("00"), Err(Error::TrailingCharacters));
                assert_eq!(from_str::<$ty>("1e5"), Err(Error::TrailingCharacters));
                assert_eq!(from_str::<$ty>("1234567890123456789012345678901234567890"), Err(Error::InvalidNumber));
            )*};
        }
        test_de_signed!(i8, i16, i32, i64);
        test_de_unsigned!(u8, u16, u32, u64);
        test_de_int!(i8, i16, i32, i64, u8, u16, u32, u64);
        assert_eq!(from_str::<u8>("256"), Err(Error::InvalidNumber));
        assert_eq!(from_str::<i8>("-129"), Err(Error::InvalidNumber));
    }

    #[test]
    fn test_de_enum_clike() {
        assert_eq!(from_str(r#" "boolean" "#), Ok((Type::Boolean, 11)));
        assert_eq!(from_str(r#" "number" "#), Ok((Type::Number, 10)));
        assert_eq!(from_str(r#" "thing" "#), Ok((Type::Thing, 9)));

        #[cfg(any(feature = "std", feature = "alloc"))]
        assert_eq!(
            from_str::<Type>(r#" "" "#),
            Err(Error::DeserializeError(
                r#"unknown variant ``, expected one of `boolean`, `number`, `thing`"#.to_string()
            ))
        );
        #[cfg(not(any(feature = "std", feature = "alloc")))]
        assert_eq!(from_str::<Type>(r#" "" "#), Err(Error::DeserializeError));

        #[cfg(any(feature = "std", feature = "alloc"))]
        assert_eq!(
            from_str::<Type>(r#" "xyz" "#),
            Err(Error::DeserializeError(
                r#"unknown variant `xyz`, expected one of `boolean`, `number`, `thing`"#
                    .to_string()
            ))
        );
        #[cfg(not(any(feature = "std", feature = "alloc")))]
        assert_eq!(from_str::<Type>(r#" "xyz" "#), Err(Error::DeserializeError));

        assert_eq!(
            from_str::<Type>(r#"{"boolean":null}"#),
            Err(Error::InvalidType)
        );
        assert_eq!(from_str::<Type>(r#" {} "#), Err(Error::ExpectedString));
        assert_eq!(from_str::<Type>(r#" [] "#), Err(Error::ExpectedEnumValue));
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_de_string() {
        let buf = &mut [0u8; 9];
        assert_eq!(from_bufstr::<String>(buf, r#""""#), Ok(("".to_string(), 2)));
        assert_eq!(
            from_bufstr::<String>(buf, r#""hello""#),
            Ok(("hello".to_string(), 7))
        );
        assert_eq!(
            from_bufstr::<String>(buf, r#" "" "#),
            Ok(("".to_string(), 4))
        );
        assert_eq!(
            from_bufstr::<String>(buf, r#" "hello" "#),
            Ok(("hello".to_string(), 9))
        );
    }

    #[test]
    fn test_de_str() {
        let buf = &mut [0u8; 20];
        assert_eq!(from_bufstr(buf, r#" "hello" "#), Ok(("hello", 9)));
        assert_eq!(from_bufstr(buf, r#" "" "#), Ok(("", 4)));
        assert_eq!(from_bufstr(buf, r#" " " "#), Ok((" ", 5)));
        assert_eq!(from_bufstr(buf, r#" "👏" "#), Ok(("👏", 8)));

        assert_eq!(from_bufstr(buf, r#" "hel\tlo" "#), Ok(("hel\tlo", 11)));
        assert_eq!(from_bufstr(buf, r#" "hello \\" "#), Ok(("hello \\", 12)));

        // escaped " in the string content
        assert_eq!(from_bufstr(buf, r#" "foo\"bar" "#), Ok((r#"foo"bar"#, 12)));
        assert_eq!(
            from_bufstr(buf, r#" "foo\\\"bar" "#),
            Ok((r#"foo\"bar"#, 14))
        );
        assert_eq!(
            from_bufstr(buf, r#" "foo\"\"bar" "#),
            Ok((r#"foo""bar"#, 14))
        );
        assert_eq!(from_bufstr(buf, r#" "\"bar" "#), Ok((r#""bar"#, 9)));
        assert_eq!(from_bufstr(buf, r#" "foo\"" "#), Ok((r#"foo""#, 9)));
        assert_eq!(from_bufstr(buf, r#" "\"" "#), Ok((r#"""#, 6)));

        // non-excaped " preceded by backslashes
        assert_eq!(
            from_bufstr(buf, r#" "foo bar\\" "#),
            Ok((r#"foo bar\"#, 13))
        );
        assert_eq!(
            from_bufstr(buf, r#" "foo bar\\\\" "#),
            Ok((r#"foo bar\\"#, 15))
        );
        assert_eq!(
            from_bufstr(buf, r#" "foo bar\\\\\\" "#),
            Ok((r#"foo bar\\\"#, 17))
        );
        assert_eq!(
            from_bufstr(buf, r#" "foo bar\\\\\\\\" "#),
            Ok((r#"foo bar\\\\"#, 19))
        );
        assert_eq!(from_bufstr(buf, r#" "\\" "#), Ok((r#"\"#, 6)));
        // error
        assert_eq!(from_bufstr::<&str>(buf, ""), Err(Error::UnexpectedEof));
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\x" "#),
            Err(Error::InvalidEscapeSequence)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\c" "#),
            Err(Error::InvalidEscapeSequence)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\ux000" "#),
            Err(Error::InvalidEscapeSequence)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\u0x00" "#),
            Err(Error::InvalidEscapeSequence)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\u00x0" "#),
            Err(Error::InvalidEscapeSequence)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\u000x" "#),
            Err(Error::InvalidEscapeSequence)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\u000" "#),
            Err(Error::InvalidEscapeSequence)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\uD800" "#),
            Err(Error::InvalidUnicodeCodePoint)
        );
        assert_eq!(
            from_bufstr::<&str>(buf, r#" "\uDFFF" "#),
            Err(Error::InvalidUnicodeCodePoint)
        );
        buf[0..4].copy_from_slice(b"\"\xff\xfe\"");
        assert_eq!(
            from_mut_slice::<&str>(&mut buf[0..4]),
            Err(Error::InvalidUnicodeCodePoint)
        );
    }

    #[test]
    fn test_de_char() {
        assert_eq!(from_str::<char>(r#""A""#), Ok(('A', 3)));
        assert_eq!(from_str::<char>(r#"" ""#), Ok((' ', 3)));
        assert_eq!(from_str::<char>(r#""\t""#), Ok(('\t', 4)));
        assert_eq!(from_str::<char>(r#""👏""#), Ok(('👏', 6)));
        assert_eq!(from_str::<char>(""), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<char>("["), Err(Error::ExpectedString));
        assert_eq!(from_str::<char>(r#""ab""#), Err(Error::InvalidLength));
        assert_eq!(from_str::<char>(r#""\""#), Err(Error::UnexpectedEof));
    }

    #[test]
    fn test_de_struct() {
        #[derive(Default, Debug, Deserialize, PartialEq)]
        #[serde(default)]
        struct Test {
            foo: i8,
            bar: f64,
        }
        assert_eq!(from_str("{}"), Ok((Test { foo: 0, bar: 0.0 }, 2)));
        assert_eq!(
            from_str(r#"{ "foo": 0 }"#),
            Ok((Test { foo: 0, bar: 0.0 }, 12))
        );
        assert_eq!(
            from_str(r#"{"bar":3.14,"foo":-1}"#),
            Ok((Test { bar: 3.14, foo: -1 }, 21))
        );
        assert_eq!(
            from_str(
                r#" {
                "bar" : -9.5e-10 ,
                "foo" : -128
            }"#
            ),
            Ok((
                Test {
                    bar: -9.5e-10,
                    foo: -128
                },
                80
            ))
        );
        assert_eq!(from_str(r#"[]"#), Ok((Test { bar: 0.0, foo: 0 }, 2)));
        assert_eq!(from_str(r#"[5]"#), Ok((Test { foo: 5, bar: 0.0 }, 3)));
        assert_eq!(
            from_str(r#"[5,999.9]"#),
            Ok((Test { foo: 5, bar: 999.9 }, 9))
        );
        // error
        assert_eq!(from_str::<Test>(""), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<Test>(r#""""#), Err(Error::ExpectedStruct));
        assert_eq!(
            from_str::<Test>(r#"{"foo":0]"#),
            Err(Error::ExpectedObjectCommaOrEnd)
        );
        assert_eq!(
            from_str::<Test>(r#"{"foo":0,}"#),
            Err(Error::TrailingObjectComma)
        );
        assert_eq!(from_str::<Test>(r#"{"foo",0}"#), Err(Error::ExpectedColon));
        assert_eq!(from_str::<Test>(r#"{,}"#), Err(Error::LeadingObjectComma));
        assert_eq!(
            from_str::<Test>(r#"{,"foo":0}"#),
            Err(Error::LeadingObjectComma)
        );
    }

    #[test]
    fn test_de_struct_bool() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Led {
            led: bool,
        }

        assert_eq!(from_str(r#"{ "led": true }"#), Ok((Led { led: true }, 15)));
        assert_eq!(
            from_str(r#"{ "led": false }"#),
            Ok((Led { led: false }, 16))
        );
    }

    #[test]
    fn test_de_struct_i8() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Temperature {
            temperature: i8,
        }

        assert_eq!(
            from_str(r#"{ "temperature": -17 }"#),
            Ok((Temperature { temperature: -17 }, 22))
        );

        assert_eq!(
            from_str(r#"{ "temperature": -0 }"#),
            Ok((Temperature { temperature: -0 }, 21))
        );

        assert_eq!(
            from_str(r#"{ "temperature": 0 }"#),
            Ok((Temperature { temperature: 0 }, 20))
        );

        // out of range
        assert!(from_str::<Temperature>(r#"{ "temperature": 128 }"#).is_err());
        assert!(from_str::<Temperature>(r#"{ "temperature": -129 }"#).is_err());
    }

    #[test]
    fn test_de_struct_u8() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Temperature {
            temperature: u8,
        }

        assert_eq!(
            from_str(r#"{ "temperature": 20 }"#),
            Ok((Temperature { temperature: 20 }, 21))
        );

        assert_eq!(
            from_str(r#"{ "temperature": 0 }"#),
            Ok((Temperature { temperature: 0 }, 20))
        );

        // out of range
        assert!(from_str::<Temperature>(r#"{ "temperature": 256 }"#).is_err());
        assert!(from_str::<Temperature>(r#"{ "temperature": -1 }"#).is_err());
    }

    #[test]
    fn test_de_struct_f32() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Temperature {
            temperature: f32,
        }

        assert_eq!(
            from_str(r#"{ "temperature": -17.2 }"#),
            Ok((Temperature { temperature: -17.2 }, 24))
        );

        assert_eq!(
            from_str(r#"{ "temperature": -0.0 }"#),
            Ok((Temperature { temperature: -0. }, 23))
        );

        assert_eq!(
            from_str(r#"{ "temperature": -2.1e-3 }"#),
            Ok((
                Temperature {
                    temperature: -2.1e-3
                },
                26
            ))
        );

        assert_eq!(
            from_str(r#"{ "temperature": -3 }"#),
            Ok((Temperature { temperature: -3. }, 21))
        );

        use core::f32;

        assert_eq!(
            from_str(r#"{ "temperature": -1e500 }"#),
            Ok((
                Temperature {
                    temperature: f32::NEG_INFINITY
                },
                25
            ))
        );

        // NaNs will always compare unequal.
        let (r, n): (Temperature, usize) = from_str(r#"{ "temperature": null }"#).unwrap();
        assert!(r.temperature.is_nan());
        assert_eq!(n, 23);

        assert!(from_str::<Temperature>(r#"{ "temperature": 1e1e1 }"#).is_err());
        assert!(from_str::<Temperature>(r#"{ "temperature": -2-2 }"#).is_err());
        assert!(from_str::<Temperature>(r#"{ "temperature": 1 1 }"#).is_err());
        assert!(from_str::<Temperature>(r#"{ "temperature": 0.0. }"#).is_err());
        assert!(from_str::<Temperature>(r#"{ "temperature": ä }"#).is_err());
        assert!(from_str::<Temperature>(r#"{ "temperature": None }"#).is_err());
        assert!(from_str::<Temperature>(r#"{"temperature":+}"#).is_err());
    }

    #[test]
    fn test_de_struct_option() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Property<'a> {
            #[serde(borrow)]
            description: Option<&'a str>,
        }

        let buf = &mut [0u8; 50];

        assert_eq!(
            from_bufstr(buf, r#"{ "description": "An ambient temperature sensor" }"#),
            Ok((
                Property {
                    description: Some("An ambient temperature sensor"),
                },
                50
            ))
        );

        assert_eq!(
            from_bufstr(buf, r#"{ "description": null }"#),
            Ok((Property { description: None }, 23))
        );

        assert_eq!(
            from_bufstr(buf, r#"{}"#),
            Ok((Property { description: None }, 2))
        );
    }

    #[test]
    fn test_de_unit() {
        assert_eq!(from_str(r#"null"#), Ok(((), 4)));
        #[derive(Debug, Deserialize, PartialEq)]
        struct Unit;
        assert_eq!(from_str(r#"null"#), Ok((Unit, 4)));
        assert_eq!(from_str::<()>("x"), Err(Error::ExpectedNull));
        assert_eq!(from_str::<Unit>("x"), Err(Error::ExpectedNull));
        assert_eq!(from_str::<()>("nil"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<Unit>("nil"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<()>("nill"), Err(Error::ExpectedToken));
        assert_eq!(from_str::<Unit>("nill"), Err(Error::ExpectedToken));
        assert_eq!(from_str::<()>(""), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<Unit>(""), Err(Error::UnexpectedEof));
    }

    #[test]
    fn test_de_option() {
        assert_eq!(from_str::<Option<bool>>(r#"null"#), Ok((None, 4)));
        assert_eq!(from_str::<Option<bool>>(r#"true"#), Ok((Some(true), 4)));
        assert_eq!(from_str::<Option<bool>>(r#"false"#), Ok((Some(false), 5)));
        assert_eq!(from_str::<Option<bool>>("x"), Err(Error::UnexpectedChar));
        assert_eq!(from_str::<Option<bool>>("nil"), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<Option<bool>>("nill"), Err(Error::ExpectedToken));
        assert_eq!(from_str::<Option<bool>>(""), Err(Error::UnexpectedEof));
    }

    #[test]
    fn test_de_newtype_variant() {
        #[derive(Deserialize, Debug, PartialEq)]
        enum A {
            A(u32),
        }
        let a = A::A(54);
        let (x, len) = from_str::<A>(r#"{"A":54}"#).unwrap();
        assert_eq!(len, 8);
        assert_eq!(x, a);
        let (y, len) = from_str::<A>(r#" { "A" : 54 } "#).unwrap();
        assert_eq!(len, 14);
        assert_eq!(x, y);
        assert_eq!(from_str::<A>(r#""A""#), Err(Error::InvalidType));
        assert_eq!(from_str::<A>(r#"{"A","#), Err(Error::ExpectedColon));
        assert_eq!(
            from_str::<A>(r#"{"A":54,"#),
            Err(Error::ExpectedEnumObjectEnd)
        );
        assert_eq!(from_str::<A>(r#"{"A":54"#), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<A>(r#" "#), Err(Error::UnexpectedEof));
    }

    #[test]
    fn test_de_struct_variant() {
        #[derive(Deserialize, Debug, PartialEq)]
        enum A {
            A { x: u32, y: u16 },
        }
        let a = A::A { x: 54, y: 720 };
        let (x, len) = from_str::<A>(r#"{"A":{"x":54,"y":720}}"#).unwrap();
        assert_eq!(len, 22);
        assert_eq!(x, a);
        let (z, len) = from_str::<A>(r#" { "A"  : { "x" : 54 , "y" : 720 } } "#).unwrap();
        assert_eq!(len, 37);
        assert_eq!(z, x);
        let (y, len) = from_str::<A>(r#"{"A":[54,720]}"#).unwrap();
        assert_eq!(len, 14);
        assert_eq!(y, z);
        let (yy, len) = from_str::<A>(r#" { "A" : [ 54 , 720 ] } "#).unwrap();
        assert_eq!(len, 24);
        assert_eq!(yy, y);
        assert_eq!(from_str::<A>(r#""A""#), Err(Error::InvalidType));
    }

    #[test]
    fn test_de_tuple_variant() {
        #[derive(Deserialize, Debug, PartialEq)]
        enum A {
            A(i32, u16),
        }
        let a = A::A(-981, 10000);
        let (x, len) = from_str::<A>(r#"{"A":[-981,10000]}"#).unwrap();
        assert_eq!(len, 18);
        assert_eq!(x, a);
        let (y, len) = from_str::<A>(r#" { "A" : [ -981 , 10000 ] } "#).unwrap();
        assert_eq!(len, 28);
        assert_eq!(y, x);
        assert_eq!(from_str::<A>(r#""A""#), Err(Error::InvalidType));
    }

    #[test]
    fn test_de_newtype_struct() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct A(u32);

        assert_eq!(from_str::<A>(r#"54"#), Ok((A(54), 2)));
    }

    #[test]
    fn test_de_struct_tuple() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Xy(i8, i8);

        assert_eq!(from_str(r#"[10, 20]"#), Ok((Xy(10, 20), 8)));
        assert_eq!(from_str(r#"[10, -20]"#), Ok((Xy(10, -20), 9)));

        // wrong number of args
        #[cfg(any(feature = "std", feature = "alloc"))]
        assert_eq!(
            from_str::<Xy>(r#"[10]"#),
            Err(Error::DeserializeError(
                r#"invalid length 1, expected tuple struct Xy with 2 elements"#.to_string()
            ))
        );
        #[cfg(not(any(feature = "std", feature = "alloc")))]
        assert_eq!(from_str::<Xy>(r#"[10]"#), Err(Error::DeserializeError));
        assert_eq!(
            from_str::<Xy>(r#"[10, 20, 30]"#),
            Err(Error::ExpectedArrayEnd)
        );
    }

    #[test]
    fn test_de_struct_with_array_field() {
        #[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
        struct Test {
            status: bool,
            point: [u32; 3],
        }
        let test = Test {
            status: true,
            point: [1, 2, 3],
        };
        assert_eq!(
            from_str(
                r#"{ "status": true,
                          "point": [1, 2, 3] }"#
            ),
            Ok((test, 64))
        );

        assert_eq!(
            from_str(r#"{"status":true,"point":[1,2,3]}"#),
            Ok((test, 31))
        );
    }

    #[test]
    fn test_de_struct_with_tuple_field() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Test {
            status: bool,
            point: (u32, u32, u32),
        }

        assert_eq!(
            from_str(r#"{ "status": true, "point": [1, 2, 3] }"#),
            Ok((
                Test {
                    status: true,
                    point: (1, 2, 3)
                },
                38
            ))
        );
    }

    #[test]
    fn test_de_ignoring_extra_fields() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Temperature {
            temperature: u8,
        }

        assert_eq!(
            from_str(
                r#"{ "temperature": 20, "high": 80, "low": -10, "updated": true, "unused": null }"#
            ),
            Ok((Temperature { temperature: 20 }, 78))
        );

        assert_eq!(
            from_str(r#"{ "temperature": 20, "conditions": "windy", "forecast": "cloudy" }"#),
            Ok((Temperature { temperature: 20 }, 66))
        );

        assert_eq!(
            from_str(r#"{ "temperature": 20, "hourly_conditions": ["windy", "rainy"] }"#),
            Ok((Temperature { temperature: 20 }, 62))
        );

        assert_eq!(
            from_str(
                r#"{ "temperature"  :  20, "source": { "station": "dock", "sensors": ["front", "back"] } }"#
            ),
            Ok((Temperature { temperature: 20 }, 87))
        );

        assert_eq!(
            from_str(
                r#"{ "source": { "station": "dock", "sensors": ["\\", "\"", "x\\\"y\\"] }, "temperature":20}"#
            ),
            Ok((Temperature { temperature: 20 }, 89))
        );
        // error
        assert_eq!(
            from_str::<Temperature>(r#"{ "temperature": 20, "invalid": this-is-not-ignored }"#),
            Err(Error::ExpectedToken)
        );
        assert_eq!(
            from_str::<Temperature>(r#"{ "temperature": 20, "broken": }"#),
            Err(Error::UnexpectedChar)
        );
        assert_eq!(
            from_str::<Temperature>(r#"{ "temperature": 20, "broken": [ }"#),
            Err(Error::UnexpectedChar)
        );
        assert_eq!(
            from_str::<Temperature>(r#"{ "temperature": 20, "broken": ] }"#),
            Err(Error::UnexpectedChar)
        );
        assert_eq!(
            from_str::<Temperature>(r#"{"temperature":20,"broken":1"#),
            Err(Error::UnexpectedEof)
        );
        assert_eq!(
            from_str::<Temperature>(r#"{"temperature":20,"broken":"#),
            Err(Error::UnexpectedEof)
        );
        assert_eq!(
            from_str::<Temperature>(r#"{"temperature":20,"broken":""#),
            Err(Error::UnexpectedEof)
        );
        assert_eq!(
            from_str::<Temperature>(r#"{"temperature":20,"#),
            Err(Error::UnexpectedEof)
        );
    }

    #[test]
    fn test_de_map_err() {
        use core::marker::PhantomData;
        use serde::de::Deserializer;
        #[derive(Debug, PartialEq)]
        struct PhonyMap(Option<(i32, i32)>);
        struct PhonyMapVisitor(PhantomData<PhonyMap>);
        impl<'de> Visitor<'de> for PhonyMapVisitor {
            type Value = PhonyMap;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut access: M,
            ) -> core::result::Result<Self::Value, M::Error> {
                if let Some((k, v)) = access.next_entry()? {
                    return Ok(PhonyMap(Some((k, v))));
                }
                Ok(PhonyMap(None))
            }
        }
        impl<'de> Deserialize<'de> for PhonyMap {
            fn deserialize<D: Deserializer<'de>>(
                deserializer: D,
            ) -> core::result::Result<Self, D::Error> {
                deserializer.deserialize_any(PhonyMapVisitor(PhantomData))
            }
        }
        assert_eq!(from_str::<PhonyMap>(r#"{}"#), Ok((PhonyMap(None), 2)));
        assert_eq!(
            from_str::<PhonyMap>(r#"{"0":1}"#),
            Ok((PhonyMap(Some((0, 1))), 7))
        );
        assert_eq!(from_str::<PhonyMap>(r#""#), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<PhonyMap>(r#"{"#), Err(Error::UnexpectedEof));
        assert_eq!(from_str::<PhonyMap>(r#"{"":0}"#), Err(Error::InvalidType));
        assert_eq!(from_str::<PhonyMap>(r#"{"0":1"#), Err(Error::UnexpectedEof));
        assert_eq!(
            from_str::<PhonyMap>(r#"{"0":1]"#),
            Err(Error::ExpectedObjectEnd)
        );
        assert_eq!(
            from_str::<PhonyMap>(r#"{"0":1,"#),
            Err(Error::ExpectedObjectEnd)
        );
        assert!(from_str::<PhonyMap>(r#"[]"#).is_err());
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_de_map() {
        let buf = &mut [0u8; 160];
        macro_rules! test_de_map_int {
            ($($ty:ty),*) => {$(
                let mut amap = BTreeMap::<$ty,&str>::new();
                amap.insert(<$ty>::MIN, "Minimum");
                amap.insert(1, "One");
                amap.insert(<$ty>::MAX, "Maximum");
                let s = format!(r#" {{ "  {}" : "Minimum" ,
                    " {}" : "One", 
                    "   {}" : "Maximum"
                }} "#,
                    <$ty>::MIN, 1, <$ty>::MAX);
                assert_eq!(
                    from_bufstr(buf, &s),
                    Ok((amap.clone(), s.len())));
                let s = format!(r#"{{"{}":"Minimum","{}":"One","{}":"Maximum"}}"#,
                            <$ty>::MIN, 1, <$ty>::MAX);
                assert_eq!(
                    from_bufstr(buf, &s),
                    Ok((amap, s.len())));
                // error
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{"0"#),
                    Err(Error::UnexpectedEof));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{"0""#),
                    Err(Error::UnexpectedEof));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{"0":"#),
                    Err(Error::UnexpectedEof));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{"0":"""#),
                    Err(Error::UnexpectedEof));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{"0":"","#),
                    Err(Error::UnexpectedEof));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{ "  0 " : "" }"#),
                    Err(Error::InvalidNumber));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{ "  0." : "" }"#),
                    Err(Error::InvalidNumber));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{ "" : "" }"#),
                    Err(Error::InvalidType));
                assert_eq!(
                    from_bufstr::<BTreeMap::<$ty,&str>>(buf, r#"{ "foo" : "" }"#),
                    Err(Error::InvalidType));
            )*};
        }
        test_de_map_int!(i8, u8, i16, u16, i32, u32, i64, u64);
        let mut amap = BTreeMap::<&str, Option<bool>>::new();
        amap.insert("", None);
        amap.insert("  ", Some(false));
        amap.insert("  1", Some(true));
        amap.insert("\tfoo\n", Some(true));
        assert_eq!(
            from_bufstr(buf, r#"{"  ":false,"":null,"  1":true,"\tfoo\n":true}"#),
            Ok((amap.clone(), 46))
        );
        assert_eq!(
            from_bufstr(
                buf,
                r#" {
                "  " : false ,
                "" : null,
                "  1" :  true,
                "\tfoo\n"  : true
            }"#
            ),
            Ok((amap.clone(), 139))
        );
        let mut amap = BTreeMap::<char, i32>::new();
        amap.insert(' ', 0);
        amap.insert('1', 1);
        amap.insert('\t', -9);
        amap.insert('_', -1);
        amap.insert('ℝ', 999);
        assert_eq!(
            from_bufstr(buf, r#"{" ":0,"1":1,"\t":-9,"ℝ":999,"_":-1}"#),
            Ok((amap.clone(), 38))
        );
        #[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum CKey {
            Foo,
            Bar,
        }
        let mut amap = BTreeMap::<CKey, i8>::new();
        amap.insert(CKey::Foo, 0);
        amap.insert(CKey::Bar, 1);
        assert_eq!(from_bufstr(buf, r#"{"Foo":0,"Bar":1}"#), Ok((amap, 17)));
        let mut amap = BTreeMap::<bool, i8>::new();
        amap.insert(false, 0);
        amap.insert(true, 1);
        assert_eq!(
            from_bufstr(buf, r#"{"true":1,"false":0}"#),
            Ok((amap.clone(), 20))
        );
        // error
        assert_eq!(
            from_bufstr::<BTreeMap::<CKey, i8>>(buf, r#"{"Baz":0}"#),
            Err(Error::DeserializeError(
                "unknown variant `Baz`, expected `Foo` or `Bar`".to_string()
            ))
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<char, i32>>(buf, r#"{"":0}"#),
            Err(Error::InvalidLength)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<char, i32>>(buf, r#"{"ab":0}"#),
            Err(Error::InvalidLength)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<bool, i32>>(buf, r#"{"true ":0}"#),
            Err(Error::InvalidType)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<&str, i8>>(buf, r#"[]"#),
            Err(Error::ExpectedObject)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<&str, i8>>(buf, r#"{"x":9]"#),
            Err(Error::ExpectedObjectCommaOrEnd)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<i8, i8>>(buf, r#"{1:1}"#),
            Err(Error::KeyMustBeAString)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<bool, i8>>(buf, r#"{"null":0}"#),
            Err(Error::UnexpectedChar)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<bool, i8>>(buf, r#"{"truu":0}"#),
            Err(Error::ExpectedToken)
        );
        assert_eq!(
            from_bufstr::<BTreeMap::<bool, i8>>(buf, r#"{"true"#),
            Err(Error::UnexpectedEof)
        );
    }

    #[test]
    fn test_de_wot() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Thing<'a> {
            #[serde(borrow)]
            properties: Properties<'a>,
            #[serde(rename = "type")]
            ty: Type,
        }

        #[derive(Debug, Deserialize, PartialEq)]
        struct Properties<'a> {
            #[serde(borrow)]
            temperature: Property<'a>,
            #[serde(borrow)]
            humidity: Property<'a>,
            #[serde(borrow)]
            led: Property<'a>,
        }

        #[derive(Debug, Deserialize, PartialEq)]
        struct Property<'a> {
            #[serde(rename = "type")]
            ty: Type,
            unit: Option<&'a str>,
            #[serde(borrow)]
            description: Option<&'a str>,
            href: &'a str,
        }

        let buf = &mut [0u8; 852];

        assert_eq!(
            from_bufstr::<Thing<'_>>(
                buf,
                r#"
                    {
                    "type": "thing",
                    "properties": {
                        "temperature": {
                        "type": "number",
                        "unit": "celsius",
                        "description": "An ambient temperature sensor",
                        "href": "/properties/temperature"
                        },
                        "humidity": {
                        "type": "number",
                        "unit": "percent",
                        "href": "/properties/humidity"
                        },
                        "led": {
                        "type": "boolean",
                        "description": "A red LED",
                        "href": "/properties/led"
                        }
                    }
                    }
                    "#
            ),
            Ok((
                Thing {
                    properties: Properties {
                        temperature: Property {
                            ty: Type::Number,
                            unit: Some("celsius"),
                            description: Some("An ambient temperature sensor"),
                            href: "/properties/temperature",
                        },
                        humidity: Property {
                            ty: Type::Number,
                            unit: Some("percent"),
                            description: None,
                            href: "/properties/humidity",
                        },
                        led: Property {
                            ty: Type::Boolean,
                            unit: None,
                            description: Some("A red LED"),
                            href: "/properties/led",
                        },
                    },
                    ty: Type::Thing,
                },
                852
            ))
        )
    }

    #[test]
    fn test_de_any() {
        #[derive(Debug, Deserialize, PartialEq)]
        #[serde(untagged)]
        enum Thing<'a> {
            Nope,
            Bool(bool),
            Str(&'a str),
            Uint(u32),
            Int(i32),
            LongUint(u64),
            LongInt(i64),
            Float(f64),
            Array([&'a str; 2]),
            Map { a: u32, b: &'a str },
        }
        let mut buf = [0u8; 22];
        let input = "null";
        assert_eq!(from_bufstr(&mut buf, input), Ok((Thing::Nope, input.len())));
        let input = "false";
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Bool(false), input.len()))
        );
        let input = "0";
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Uint(0), input.len()))
        );
        let input = "-1";
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Int(-1), input.len()))
        );
        let input = r#""foo""#;
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Str("foo"), input.len()))
        );
        let input = "18446744073709551615";
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::LongUint(u64::MAX), input.len()))
        );
        let input = "-9223372036854775808";
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::LongInt(i64::MIN), input.len()))
        );
        let input = "0.0";
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Float(0.0), input.len()))
        );
        let input = "3.40282347E+38";
        #[cfg(feature = "de-any-f32")]
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Float(f32::MAX as f64), input.len()))
        );
        #[cfg(not(feature = "de-any-f32"))]
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Float(3.40282347E+38), input.len()))
        );
        let input = "1.7976931348623157e308";
        #[cfg(feature = "de-any-f32")]
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Float(f64::INFINITY), input.len()))
        );
        #[cfg(not(feature = "de-any-f32"))]
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Float(f64::MAX), input.len()))
        );
        let input = r#"["xy","abc"]"#;
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Array(["xy", "abc"]), input.len()))
        );
        let input = r#"{"a":126,"b":"zyx"}"#;
        assert_eq!(
            from_bufstr(&mut buf, input),
            Ok((Thing::Map { a: 126, b: "zyx" }, input.len()))
        );
        // error
        assert_eq!(
            from_bufstr::<Thing>(&mut buf, ""),
            Err(Error::UnexpectedEof)
        );
        assert_eq!(
            from_bufstr::<Thing>(&mut buf, "x"),
            Err(Error::UnexpectedChar)
        );
        assert_eq!(
            from_bufstr::<Thing>(&mut buf, "-"),
            Err(Error::InvalidNumber)
        );
        assert_eq!(
            from_bufstr::<Thing>(&mut buf, "-+"),
            Err(Error::InvalidNumber)
        );
        assert_eq!(
            from_bufstr::<Thing>(&mut buf, "2.+"),
            Err(Error::InvalidNumber)
        );
        assert_eq!(
            from_bufstr::<Thing>(&mut buf, "2+"),
            Err(Error::InvalidNumber)
        );
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_de_error_string() {
        assert_eq!(
            &format!("{}", Error::UnexpectedEof),
            "Unexpected end of JSON input"
        );
        assert_eq!(
            &format!("{}", Error::InvalidEscapeSequence),
            "Invalid JSON string escape sequence"
        );
        assert_eq!(
            &format!("{}", Error::StringControlChar),
            "A control ASCII character found in a JSON string"
        );
        assert_eq!(
            &format!("{}", Error::ExpectedArrayCommaOrEnd),
            "Expected `','` or `']'`"
        );
        assert_eq!(&format!("{}", Error::ExpectedArrayEnd), "Expected ']'");
        assert_eq!(
            &format!("{}", Error::LeadingArrayComma),
            "JSON array content starts with a leading `','`"
        );
        assert_eq!(
            &format!("{}", Error::TrailingArrayComma),
            "JSON array content ends with a trailing `','`"
        );
        assert_eq!(
            &format!("{}", Error::ExpectedObjectCommaOrEnd),
            "Expected `','` or `'}'`"
        );
        assert_eq!(&format!("{}", Error::ExpectedObjectEnd), "Expected `'}'`");
        assert_eq!(
            &format!("{}", Error::LeadingObjectComma),
            "JSON object content starts with a leading `','`"
        );
        assert_eq!(
            &format!("{}", Error::TrailingObjectComma),
            "JSON object content ends with a trailing `','`"
        );
        assert_eq!(&format!("{}", Error::ExpectedColon), "Expected `':'`");
        assert_eq!(
            &format!("{}", Error::ExpectedToken),
            "Expected either `true`, `false`, or `null`."
        );
        assert_eq!(&format!("{}", Error::ExpectedNull), "Expected `null`");
        assert_eq!(&format!("{}", Error::ExpectedString), r#"Expected `'"'`"#);
        assert_eq!(&format!("{}", Error::ExpectedArray), "Expeced a JSON array");
        assert_eq!(
            &format!("{}", Error::ExpectedObject),
            "Expected a JSON object"
        );
        assert_eq!(
            &format!("{}", Error::ExpectedStruct),
            "Expected a JSON object or an array"
        );
        assert_eq!(
            &format!("{}", Error::ExpectedEnumValue),
            r#"Expected this character to be `'"'` or `'{'`"#
        );
        assert_eq!(
            &format!("{}", Error::ExpectedEnumObjectEnd),
            "Expected this character to be `'}'`"
        );
        assert_eq!(&format!("{}", Error::InvalidNumber), "Invalid number");
        assert_eq!(&format!("{}", Error::InvalidType), "Invalid type");
        assert_eq!(
            &format!("{}", Error::InvalidUnicodeCodePoint),
            "Invalid unicode code point"
        );
        assert_eq!(
            &format!("{}", Error::KeyMustBeAString),
            "Object key is not a string"
        );
        assert_eq!(
            &format!("{}", Error::TrailingCharacters),
            "JSON has non-whitespace trailing character after the value"
        );
        assert_eq!(
            &format!("{}", Error::UnexpectedChar),
            "Unexpected token while parsing a JSON value"
        );
        assert_eq!(&format!("{}", Error::InvalidLength), "Invalid length");
        let custom: Error = serde::de::Error::custom("xxx");
        assert_eq!(format!("{}", custom), "xxx while deserializing JSON");
    }

    #[cfg(not(any(feature = "std", feature = "alloc")))]
    #[test]
    fn test_de_error_fmt() {
        use core::fmt::Write;
        let mut buf = [0u8; 52];
        let mut writer = SliceWriter::new(&mut buf);
        let custom: Error = serde::de::Error::custom("xxx");
        write!(writer, "{}", custom).unwrap();
        assert_eq!(
            writer.as_ref(),
            "JSON does not match deserializer’s expected format".as_bytes()
        );
    }
}
