use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lps_shared::ParamQualifier;

use crate::{Diagnostic, Keyword, Span, Token, TokenKind, lex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniformDecl {
    pub name: String,
    pub ty: TypeRef,
    pub binding: Option<u32>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
    pub name: String,
    pub ty: TypeRef,
    pub init_span: Option<Span>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructMemberDecl {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDecl {
    pub name: String,
    pub members: Vec<StructMemberDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: Option<String>,
    pub ty: TypeRef,
    pub qualifier: ParamQualifier,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    pub name: String,
    pub return_ty: TypeRef,
    pub params: Vec<FunctionParam>,
    pub signature_span: Span,
    pub body_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopLevelIndex {
    pub structs: Vec<StructDecl>,
    pub uniforms: Vec<UniformDecl>,
    pub consts: Vec<ConstDecl>,
    pub functions: Vec<FunctionDecl>,
}

pub fn index_source(source: &str) -> Result<TopLevelIndex, Diagnostic> {
    let tokens = lex(source)?;
    index_tokens(source, &tokens)
}

pub(crate) fn index_tokens(source: &str, tokens: &[Token]) -> Result<TopLevelIndex, Diagnostic> {
    Parser::new(source, tokens).parse()
}

struct Parser<'src, 'tok> {
    source: &'src str,
    tokens: &'tok [Token],
    pos: usize,
    struct_names: Vec<String>,
}

impl<'src, 'tok> Parser<'src, 'tok> {
    fn new(source: &'src str, tokens: &'tok [Token]) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            struct_names: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<TopLevelIndex, Diagnostic> {
        let mut index = TopLevelIndex::default();
        while !self.at_eof() {
            let binding = if self.at_keyword(Keyword::Layout) {
                self.parse_layout_binding()?
            } else {
                None
            };
            if self.current().lexeme(self.source) == "struct" {
                index.structs.push(self.parse_struct()?);
            } else if self.at_keyword(Keyword::Uniform) {
                index.uniforms.push(self.parse_uniform(binding)?);
            } else if self.at_keyword(Keyword::Const) {
                index.consts.push(self.parse_const()?);
            } else if self.starts_type_name() {
                if let Some(function) = self.try_parse_function()? {
                    index.functions.push(function);
                } else {
                    self.skip_to_semicolon()?;
                }
            } else {
                return Err(Diagnostic::error(
                    self.current().span,
                    "expected top-level declaration",
                ));
            }
        }
        Ok(index)
    }

    fn parse_layout_binding(&mut self) -> Result<Option<u32>, Diagnostic> {
        self.expect_keyword(Keyword::Layout)?;
        self.expect_punct("(")?;
        let mut binding = None;
        while !self.at_punct(")") {
            let key = self.expect_identifier_like()?;
            self.expect_punct("=")?;
            let value = self.expect_number_text()?;
            if key == "binding" {
                binding = parse_u32_text(value);
            }
            if self.at_punct(",") {
                self.bump();
            } else if !self.at_punct(")") {
                return Err(Diagnostic::expected(
                    self.current().span,
                    "',' or ')'",
                    self.describe_current(),
                ));
            }
        }
        self.expect_punct(")")?;
        Ok(binding)
    }

    fn parse_uniform(&mut self, binding: Option<u32>) -> Result<UniformDecl, Diagnostic> {
        let start = self.expect_keyword(Keyword::Uniform)?.span.start;
        let ty = self.expect_type_ref()?;
        let name = self.expect_identifier_like()?.to_string();
        let end = self.expect_punct(";")?.span.end;
        Ok(UniformDecl {
            name,
            ty,
            binding,
            span: Span::new(start, end),
        })
    }

    fn parse_const(&mut self) -> Result<ConstDecl, Diagnostic> {
        let start = self.expect_keyword(Keyword::Const)?.span.start;
        let ty = self.expect_type_ref()?;
        let name = self.expect_identifier_like()?.to_string();
        let init_span = if self.at_punct("=") {
            self.bump();
            Some(self.span_until_semicolon()?)
        } else {
            None
        };
        let end = self.expect_punct(";")?.span.end;
        Ok(ConstDecl {
            name,
            ty,
            init_span,
            span: Span::new(start, end),
        })
    }

    fn parse_struct(&mut self) -> Result<StructDecl, Diagnostic> {
        let start = self.current().span.start;
        self.expect_identifier_text("struct")?;
        let name_tok = self.current();
        let name = self.expect_identifier_like()?.to_string();
        self.struct_names.push(name.clone());
        self.expect_punct("{")?;
        let mut members = Vec::new();
        while !self.at_punct("}") {
            let ty = self.expect_type_ref()?;
            loop {
                let member_start = self.current().span.start;
                let member_name = self.expect_identifier_like()?.to_string();
                members.push(StructMemberDecl {
                    name: member_name,
                    ty: ty.clone(),
                    span: Span::new(member_start, self.previous().span.end),
                });
                if self.at_punct(",") {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect_punct(";")?;
        }
        self.expect_punct("}")?;
        let end = self.expect_punct(";")?.span.end;
        Ok(StructDecl {
            name,
            members,
            span: Span::new(start, end.max(name_tok.span.end)),
        })
    }

    fn try_parse_function(&mut self) -> Result<Option<FunctionDecl>, Diagnostic> {
        let checkpoint = self.pos;
        let return_ty = self.expect_type_ref()?;
        let name = self.expect_identifier_like()?.to_string();
        if !self.at_punct("(") {
            self.pos = checkpoint;
            return Ok(None);
        }
        self.expect_punct("(")?;
        let params = self.parse_params()?;
        self.expect_punct(")")?;
        let signature_end = self.previous().span.end;
        if self.at_punct(";") {
            self.bump();
            return Ok(None);
        }
        let body_start = self.expect_punct("{")?.span.start;
        let body_end = self.skip_balanced_brace_body()?;
        let signature_span = Span::new(return_ty.span.start, signature_end);
        Ok(Some(FunctionDecl {
            name,
            return_ty,
            params,
            signature_span,
            body_span: Span::new(body_start, body_end),
        }))
    }

    fn parse_params(&mut self) -> Result<Vec<FunctionParam>, Diagnostic> {
        let mut params = Vec::new();
        if self.at_punct(")") {
            return Ok(params);
        }
        loop {
            let qualifier = self.parse_param_qualifier();
            let ty = self.expect_type_ref()?;
            let name = if self.at_identifier_like() {
                Some(self.expect_identifier_like()?.to_string())
            } else {
                None
            };
            let span_end = self.previous().span.end;
            let span_start = ty.span.start;
            params.push(FunctionParam {
                name,
                ty,
                qualifier,
                span: Span::new(span_start, span_end),
            });
            if self.at_punct(",") {
                self.bump();
            } else {
                break;
            }
        }
        Ok(params)
    }

    fn parse_param_qualifier(&mut self) -> ParamQualifier {
        if self.current().lexeme(self.source) == "const" {
            self.bump();
        }
        let tok = self.current();
        if tok.kind != TokenKind::Identifier {
            return ParamQualifier::In;
        }
        match tok.lexeme(self.source) {
            "in" => {
                self.bump();
                ParamQualifier::In
            }
            "out" => {
                self.bump();
                ParamQualifier::Out
            }
            "inout" => {
                self.bump();
                ParamQualifier::InOut
            }
            _ => ParamQualifier::In,
        }
    }

    fn skip_balanced_brace_body(&mut self) -> Result<usize, Diagnostic> {
        let mut depth = 1usize;
        while !self.at_eof() {
            let tok = self.bump();
            if tok.lexeme(self.source) == "{" {
                depth += 1;
            } else if tok.lexeme(self.source) == "}" {
                depth -= 1;
                if depth == 0 {
                    return Ok(tok.span.end);
                }
            }
        }
        Err(Diagnostic::error(
            self.previous().span,
            "unterminated function body",
        ))
    }

    fn skip_to_semicolon(&mut self) -> Result<usize, Diagnostic> {
        while !self.at_eof() {
            let tok = self.bump();
            if tok.lexeme(self.source) == ";" {
                return Ok(tok.span.end);
            }
        }
        Err(Diagnostic::error(
            self.previous().span,
            "expected ';' before end of file",
        ))
    }

    fn span_until_semicolon(&mut self) -> Result<Span, Diagnostic> {
        let start = self.current().span.start;
        let mut end = start;
        let mut paren_depth = 0usize;
        while !self.at_eof() {
            if paren_depth == 0 && self.at_punct(";") {
                return Ok(Span::new(start, end));
            }
            let tok = self.bump();
            match tok.lexeme(self.source) {
                "(" => paren_depth += 1,
                ")" => paren_depth = paren_depth.saturating_sub(1),
                _ => {}
            }
            end = tok.span.end;
        }
        Err(Diagnostic::error(
            self.previous().span,
            "expected ';' before end of file",
        ))
    }

    fn expect_type_ref(&mut self) -> Result<TypeRef, Diagnostic> {
        let tok = self.current();
        if self.is_type_name(tok) {
            self.bump();
            let mut name = tok.lexeme(self.source).to_string();
            while self.at_punct("[") {
                name.push_str(self.parse_array_suffix()?);
            }
            Ok(TypeRef {
                name,
                span: Span::new(tok.span.start, self.previous().span.end),
            })
        } else {
            Err(Diagnostic::expected(
                tok.span,
                "type name",
                self.describe_current(),
            ))
        }
    }

    fn parse_array_suffix(&mut self) -> Result<&'src str, Diagnostic> {
        let start = self.expect_punct("[")?.span.start;
        if !matches!(
            self.current().kind,
            TokenKind::IntLiteral | TokenKind::UintLiteral
        ) {
            return Err(Diagnostic::expected(
                self.current().span,
                "array length",
                self.describe_current(),
            ));
        }
        self.bump();
        let end = self.expect_punct("]")?.span.end;
        Ok(&self.source[start..end])
    }

    fn starts_type_name(&self) -> bool {
        self.is_type_name(self.current())
    }

    fn is_type_name(&self, tok: Token) -> bool {
        matches!(
            tok.kind,
            TokenKind::Identifier
                | TokenKind::Keyword(
                    Keyword::Bool
                        | Keyword::Float
                        | Keyword::Int
                        | Keyword::Uint
                        | Keyword::Vec2
                        | Keyword::Vec3
                        | Keyword::Vec4
                        | Keyword::Void
                )
        )
    }

    fn at_identifier_like(&self) -> bool {
        matches!(self.current().kind, TokenKind::Identifier)
    }

    fn expect_identifier_like(&mut self) -> Result<&'src str, Diagnostic> {
        let tok = self.current();
        if matches!(tok.kind, TokenKind::Identifier) {
            self.bump();
            Ok(tok.lexeme(self.source))
        } else {
            Err(Diagnostic::expected(
                tok.span,
                "identifier",
                self.describe_current(),
            ))
        }
    }

    fn expect_identifier_text(&mut self, text: &str) -> Result<Token, Diagnostic> {
        let tok = self.current();
        if matches!(tok.kind, TokenKind::Identifier) && tok.lexeme(self.source) == text {
            self.bump();
            Ok(tok)
        } else {
            Err(Diagnostic::expected(
                tok.span,
                text,
                self.describe_current(),
            ))
        }
    }

    fn expect_number_text(&mut self) -> Result<&'src str, Diagnostic> {
        let tok = self.current();
        if matches!(
            tok.kind,
            TokenKind::IntLiteral | TokenKind::UintLiteral | TokenKind::FloatLiteral
        ) {
            self.bump();
            Ok(tok.lexeme(self.source).trim_end_matches(['u', 'U']))
        } else {
            Err(Diagnostic::expected(
                tok.span,
                "number",
                self.describe_current(),
            ))
        }
    }

    fn expect_keyword(&mut self, kw: Keyword) -> Result<Token, Diagnostic> {
        let tok = self.current();
        if tok.kind == TokenKind::Keyword(kw) {
            self.bump();
            Ok(tok)
        } else {
            Err(Diagnostic::expected(
                tok.span,
                "keyword",
                self.describe_current(),
            ))
        }
    }

    fn at_keyword(&self, kw: Keyword) -> bool {
        self.current().kind == TokenKind::Keyword(kw)
    }

    fn expect_punct(&mut self, punct: &str) -> Result<Token, Diagnostic> {
        let tok = self.current();
        if tok.kind == TokenKind::Punct && tok.lexeme(self.source) == punct {
            self.bump();
            Ok(tok)
        } else {
            Err(Diagnostic::expected(
                tok.span,
                punct,
                self.describe_current(),
            ))
        }
    }

    fn at_punct(&self, punct: &str) -> bool {
        let tok = self.current();
        tok.kind == TokenKind::Punct && tok.lexeme(self.source) == punct
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn current(&self) -> Token {
        self.tokens[self.pos]
    }

    fn previous(&self) -> Token {
        self.tokens[self.pos.saturating_sub(1)]
    }

    fn bump(&mut self) -> Token {
        let tok = self.current();
        if !matches!(tok.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        tok
    }

    fn describe_current(&self) -> &'src str {
        self.current().lexeme(self.source)
    }
}

fn parse_u32_text(text: &str) -> Option<u32> {
    text.parse().ok()
}
