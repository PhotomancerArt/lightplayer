use crate::{Diagnostic, Keyword, Token, TokenKind};

use super::BodyParser;

impl<'src, 'tok> BodyParser<'src, 'tok> {
    pub(super) fn starts_type_name(&self) -> bool {
        self.current()
            .is_some_and(|t| token_is_type_name(t, self.source, self.struct_names))
    }

    pub(super) fn expect_type_name(&mut self) -> Result<&'src str, Diagnostic> {
        if self.starts_type_name() {
            Ok(self.bump().lexeme(self.source))
        } else {
            Err(Diagnostic::expected(
                self.current_span(),
                "type name",
                self.current_text(),
            ))
        }
    }

    pub(super) fn parse_array_suffix(&mut self) -> Result<&'src str, Diagnostic> {
        let start = self.expect_punct("[")?.span.start;
        if self.at_punct("]") {
            let end = self.expect_punct("]")?.span.end;
            return Ok(&self.source[start..end]);
        }
        let mut paren_depth = 0usize;
        while !self.at_end() {
            if paren_depth == 0 && self.at_punct("]") {
                break;
            }
            if self.at_punct("(") {
                paren_depth += 1;
            } else if self.at_punct(")") {
                paren_depth = paren_depth.saturating_sub(1);
            }
            self.bump();
        }
        let end = self.expect_punct("]")?.span.end;
        Ok(&self.source[start..end])
    }
}

pub(super) fn token_is_type_name(
    tok: Token,
    source: &str,
    struct_names: &[alloc::string::String],
) -> bool {
    match tok.kind {
        TokenKind::Identifier if struct_names.iter().any(|name| name == tok.lexeme(source)) => true,
        TokenKind::Keyword(
            Keyword::Bool
            | Keyword::Float
            | Keyword::Int
            | Keyword::Uint
            | Keyword::Vec2
            | Keyword::Vec3
            | Keyword::Vec4
            | Keyword::Void,
        ) => true,
        TokenKind::Identifier => matches!(
            tok.lexeme(source),
            "bool"
                | "float"
                | "int"
                | "uint"
                | "vec2"
                | "vec3"
                | "vec4"
                | "ivec2"
                | "ivec3"
                | "ivec4"
                | "uvec2"
                | "uvec3"
                | "uvec4"
                | "bvec2"
                | "bvec3"
                | "bvec4"
                | "mat2"
                | "mat3"
                | "mat4"
                | "void"
        ),
        _ => false,
    }
}

pub(in crate::syntax::parser) fn token_text_is_type_name(
    text: &str,
    struct_names: &[alloc::string::String],
) -> bool {
    struct_names.iter().any(|name| name == text)
        || matches!(
            text,
            "bool"
                | "float"
                | "int"
                | "uint"
                | "vec2"
                | "vec3"
                | "vec4"
                | "ivec2"
                | "ivec3"
                | "ivec4"
                | "uvec2"
                | "uvec3"
                | "uvec4"
                | "bvec2"
                | "bvec3"
                | "bvec4"
                | "mat2"
                | "mat3"
                | "mat4"
                | "void"
        )
}
