use crate::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Bool,
    Break,
    Const,
    Continue,
    Do,
    Else,
    Float,
    For,
    If,
    Int,
    Layout,
    Return,
    Uint,
    Uniform,
    Vec2,
    Vec3,
    Vec4,
    Void,
    While,
}

impl Keyword {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "bool" => Self::Bool,
            "break" => Self::Break,
            "const" => Self::Const,
            "continue" => Self::Continue,
            "do" => Self::Do,
            "else" => Self::Else,
            "float" => Self::Float,
            "for" => Self::For,
            "if" => Self::If,
            "int" => Self::Int,
            "layout" => Self::Layout,
            "return" => Self::Return,
            "uint" => Self::Uint,
            "uniform" => Self::Uniform,
            "vec2" => Self::Vec2,
            "vec3" => Self::Vec3,
            "vec4" => Self::Vec4,
            "void" => Self::Void,
            "while" => Self::While,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Identifier,
    Keyword(Keyword),
    IntLiteral,
    UintLiteral,
    FloatLiteral,
    Punct,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn lexeme<'src>(&self, source: &'src str) -> &'src str {
        source.get(self.span.start..self.span.end).unwrap_or("")
    }
}

/// The value of an integer literal's spelling (GLSL ES 3.00 §4.1.3):
/// decimal, octal (a leading `0`) or hexadecimal (`0x`/`0X`), with or without
/// the `u`/`U` suffix. `None` when the digits do not fit the base or 64 bits;
/// the caller checks its own range.
pub fn int_literal_value(text: &str) -> Option<u64> {
    let digits = text.trim_end_matches(['u', 'U']);
    if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16).ok();
    }
    if digits.len() > 1
        && let Some(octal) = digits.strip_prefix('0')
    {
        return u64::from_str_radix(octal, 8).ok();
    }
    digits.parse::<u64>().ok()
}
