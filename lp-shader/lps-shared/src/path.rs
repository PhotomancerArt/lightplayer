//! Parse dotted / indexed paths: `lights[3].color.r`.

use alloc::string::String;
use alloc::vec::Vec;

/// One step in a value path (`obj.things[2].prop`)
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LpsPathSeg {
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

/// Parse a path into segments. Allows a leading `[n]` for top-level arrays.
pub fn parse_path(path: &str) -> Result<Vec<LpsPathSeg>, PathParseError> {
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
            segments.push(LpsPathSeg::Index(idx));
        }
        _ => {
            let ident = parse_identifier(&mut chars)?;
            segments.push(LpsPathSeg::Field(ident));
        }
    }

    while chars.peek().is_some() {
        match chars.peek() {
            Some('.') => {
                chars.next();
                let field = parse_identifier(&mut chars)?;
                segments.push(LpsPathSeg::Field(field));
            }
            Some('[') => {
                chars.next();
                let idx = parse_usize(&mut chars)?;
                match chars.next() {
                    Some(']') => {}
                    _ => return Err(PathParseError::MissingBracket),
                }
                segments.push(LpsPathSeg::Index(idx));
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
    use super::*;
    use alloc::vec;

    #[test]
    fn parse_simple_field() {
        assert_eq!(
            parse_path("position").unwrap(),
            vec![LpsPathSeg::Field(String::from("position"))]
        );
    }

    #[test]
    fn parse_nested() {
        assert_eq!(
            parse_path("light.position.x").unwrap(),
            vec![
                LpsPathSeg::Field(String::from("light")),
                LpsPathSeg::Field(String::from("position")),
                LpsPathSeg::Field(String::from("x")),
            ]
        );
    }

    #[test]
    fn parse_array_then_field() {
        assert_eq!(
            parse_path("lights[3].color").unwrap(),
            vec![
                LpsPathSeg::Field(String::from("lights")),
                LpsPathSeg::Index(3),
                LpsPathSeg::Field(String::from("color")),
            ]
        );
    }

    #[test]
    fn parse_leading_index() {
        assert_eq!(parse_path("[2]").unwrap(), vec![LpsPathSeg::Index(2)]);
    }

    #[test]
    fn lps_path_seg_field_roundtrip() {
        let original = LpsPathSeg::Field(String::from("foo"));
        let json = serde_json::to_string(&original).unwrap();
        let decoded: LpsPathSeg = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn lps_path_seg_index_roundtrip() {
        let original = LpsPathSeg::Index(3);
        let json = serde_json::to_string(&original).unwrap();
        let decoded: LpsPathSeg = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }
}
