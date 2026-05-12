use crate::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Bool,
    Break,
    Const,
    Continue,
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
