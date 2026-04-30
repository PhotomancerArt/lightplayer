//! Parse **property** paths: dot fields and array indices (`field`, `a.b[0]`).

use alloc::string::String;
use alloc::vec::Vec;

/// One step in a value path (`obj.things[2].prop`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Segment {
    Field(String),
    Index(usize),
}

/// Parse errors from [`parse_path`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathParseError {
    EmptyPath,
    EmptyIdentifier,
    EmptyIndex,
    InvalidIdentifierStart(char),
    InvalidNumber(String),
    MissingBracket,
    UnexpectedChar(char),
}

impl core::fmt::Display for PathParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "path is empty"),
            Self::EmptyIdentifier => write!(f, "expected field name"),
            Self::EmptyIndex => write!(f, "expected array index"),
            Self::InvalidIdentifierStart(c) => write!(f, "invalid start of identifier: '{c}'"),
            Self::InvalidNumber(s) => write!(f, "invalid number: '{s}'"),
            Self::MissingBracket => write!(f, "missing closing ']'"),
            Self::UnexpectedChar(c) => write!(f, "unexpected character: '{c}'"),
        }
    }
}

impl core::error::Error for PathParseError {}

/// A parsed property path: `field`, `a.b[0]`, `config.spacing`, etc.
pub type PropPath = Vec<Segment>;

/// Parse a path into segments. Allows a leading `[n]` for top-level arrays.
pub fn parse_path(path: &str) -> Result<PropPath, PathParseError> {
    let path = path.trim();
    if path.is_empty() {
        return Err(PathParseError::EmptyPath);
    }

    let mut segments = Vec::new();
    let mut chars = path.chars().peekable();

    match chars.peek() {
        Some('[') => {
            chars.next();
            let idx = parse_usize(&mut chars)?;
            match chars.next() {
                Some(']') => {}
                _ => return Err(PathParseError::MissingBracket),
            }
            segments.push(Segment::Index(idx));
        }
        _ => {
            let ident = parse_identifier(&mut chars)?;
            segments.push(Segment::Field(ident));
        }
    }

    while chars.peek().is_some() {
        match chars.peek() {
            Some('.') => {
                chars.next();
                let field = parse_identifier(&mut chars)?;
                segments.push(Segment::Field(field));
            }
            Some('[') => {
                chars.next();
                let idx = parse_usize(&mut chars)?;
                match chars.next() {
                    Some(']') => {}
                    _ => return Err(PathParseError::MissingBracket),
                }
                segments.push(Segment::Index(idx));
            }
            Some(c) => return Err(PathParseError::UnexpectedChar(*c)),
            None => break,
        }
    }

    Ok(segments)
}

fn parse_identifier<I>(chars: &mut core::iter::Peekable<I>) -> Result<String, PathParseError>
where
    I: Iterator<Item = char>,
{
    let mut ident = String::new();
    match chars.peek() {
        Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
            ident.push(chars.next().unwrap());
        }
        Some(c) => return Err(PathParseError::InvalidIdentifierStart(*c)),
        None => return Err(PathParseError::EmptyIdentifier),
    }
    while let Some(c) = chars.peek() {
        if c.is_ascii_alphanumeric() || *c == '_' {
            ident.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    if ident.is_empty() {
        return Err(PathParseError::EmptyIdentifier);
    }
    Ok(ident)
}

fn parse_usize<I>(chars: &mut core::iter::Peekable<I>) -> Result<usize, PathParseError>
where
    I: Iterator<Item = char>,
{
    let mut num = String::new();
    while let Some(c) = chars.peek() {
        if c.is_ascii_digit() {
            num.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    if num.is_empty() {
        return Err(PathParseError::EmptyIndex);
    }
    num.parse().map_err(|_| PathParseError::InvalidNumber(num))
}

#[cfg(test)]
mod tests {
    use super::{Segment, parse_path};
    use alloc::string::String;

    #[test]
    fn prop_path_parse_speed() {
        let segs = parse_path("speed").unwrap();
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn prop_path_parse_nested() {
        let segs = parse_path("config.spacing").unwrap();
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn prop_path_parse_array_then_field() {
        assert_eq!(
            parse_path("lights[3].color").unwrap(),
            alloc::vec![
                Segment::Field(String::from("lights")),
                Segment::Index(3),
                Segment::Field(String::from("color")),
            ]
        );
    }
}
