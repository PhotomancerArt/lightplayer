use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{Diagnostic, Keyword, Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFunctionBody {
    pub statements: Vec<ParsedStmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedStmt {
    Let {
        is_const: bool,
        ty: String,
        name: String,
        init: Option<ParsedExpr>,
        span: Span,
    },
    Assign {
        name: String,
        value: ParsedExpr,
        span: Span,
    },
    If {
        condition: ParsedExpr,
        accept: Vec<ParsedStmt>,
        reject: Vec<ParsedStmt>,
        span: Span,
    },
    Return(ParsedExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedExpr {
    pub span: Span,
    pub kind: ParsedExprKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedExprKind {
    BoolLiteral(bool),
    FloatLiteral(f32),
    IntLiteral(i32),
    UIntLiteral(u32),
    Name(String),
    Call {
        name: String,
        args: Vec<ParsedExpr>,
    },
    Swizzle {
        base: alloc::boxed::Box<ParsedExpr>,
        fields: String,
    },
    Unary {
        op: UnaryOp,
        expr: alloc::boxed::Box<ParsedExpr>,
    },
    Binary {
        op: BinaryOp,
        lhs: alloc::boxed::Box<ParsedExpr>,
        rhs: alloc::boxed::Box<ParsedExpr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

pub fn parse_function_body(
    source: &str,
    tokens: &[Token],
    body_span: Span,
) -> Result<ParsedFunctionBody, Diagnostic> {
    let body_tokens = tokens
        .iter()
        .copied()
        .filter(|t| {
            t.span.start >= body_span.start
                && t.span.end <= body_span.end
                && !matches!(t.kind, TokenKind::Eof)
        })
        .collect::<Vec<_>>();
    BodyParser::new(source, &body_tokens).parse()
}

pub fn parse_expr_tokens(
    source: &str,
    tokens: &[Token],
    span: Span,
) -> Result<ParsedExpr, Diagnostic> {
    let expr_tokens = tokens
        .iter()
        .copied()
        .filter(|t| {
            t.span.start >= span.start
                && t.span.end <= span.end
                && !matches!(t.kind, TokenKind::Eof)
        })
        .collect::<Vec<_>>();
    let mut parser = BodyParser::new(source, &expr_tokens);
    let expr = parser.parse_expr(0)?;
    if !parser.at_end() {
        return Err(Diagnostic::error(
            parser.current_span(),
            "unexpected tokens after expression",
        ));
    }
    Ok(expr)
}

struct BodyParser<'src, 'tok> {
    source: &'src str,
    tokens: &'tok [Token],
    pos: usize,
}

impl<'src, 'tok> BodyParser<'src, 'tok> {
    fn new(source: &'src str, tokens: &'tok [Token]) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
        }
    }

    fn parse(mut self) -> Result<ParsedFunctionBody, Diagnostic> {
        self.expect_punct("{")?;
        let statements = self.parse_block_contents()?;
        self.expect_punct("}")?;
        if !self.at_end() {
            return Err(Diagnostic::error(
                self.current_span(),
                "unexpected tokens after function body",
            ));
        }
        Ok(ParsedFunctionBody { statements })
    }

    fn parse_block_contents(&mut self) -> Result<Vec<ParsedStmt>, Diagnostic> {
        let mut statements = Vec::new();
        while !self.at_punct("}") {
            if self.at_end() {
                return Err(Diagnostic::error(
                    self.current_span(),
                    "unterminated statement block",
                ));
            }
            statements.push(self.parse_statement()?);
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<ParsedStmt, Diagnostic> {
        if self.at_keyword(Keyword::Return) {
            self.bump();
            let expr = self.parse_expr(0)?;
            self.expect_punct(";")?;
            return Ok(ParsedStmt::Return(expr));
        }
        if self.at_keyword(Keyword::If) {
            return self.parse_if();
        }
        if self.at_keyword(Keyword::Const) || self.starts_type_name() {
            return self.parse_let();
        }
        if self.at_identifier_like() {
            let checkpoint = self.pos;
            let name_tok = self.bump();
            if self.at_punct("=") {
                self.bump();
                let value = self.parse_expr(0)?;
                let end = self.expect_punct(";")?.span.end;
                return Ok(ParsedStmt::Assign {
                    name: name_tok.lexeme(self.source).to_string(),
                    value,
                    span: Span::new(name_tok.span.start, end),
                });
            }
            self.pos = checkpoint;
        }
        Err(Diagnostic::error(
            self.current_span(),
            "M3 lps-glsl supports only declarations, assignment, if, and return statements",
        ))
    }

    fn parse_if(&mut self) -> Result<ParsedStmt, Diagnostic> {
        let start = self.expect_keyword(Keyword::If)?.span.start;
        self.expect_punct("(")?;
        let condition = self.parse_expr(0)?;
        self.expect_punct(")")?;
        let accept = self.parse_statement_or_block()?;
        let reject = if self.at_keyword(Keyword::Else) {
            self.bump();
            self.parse_statement_or_block()?
        } else {
            Vec::new()
        };
        let end = reject.last().map_or_else(
            || accept.last().map_or(condition.span.end, stmt_end),
            stmt_end,
        );
        Ok(ParsedStmt::If {
            condition,
            accept,
            reject,
            span: Span::new(start, end),
        })
    }

    fn parse_statement_or_block(&mut self) -> Result<Vec<ParsedStmt>, Diagnostic> {
        if self.at_punct("{") {
            self.bump();
            let statements = self.parse_block_contents()?;
            self.expect_punct("}")?;
            Ok(statements)
        } else {
            Ok(alloc::vec![self.parse_statement()?])
        }
    }

    fn parse_let(&mut self) -> Result<ParsedStmt, Diagnostic> {
        let start = self.current_span().start;
        let is_const = if self.at_keyword(Keyword::Const) {
            self.bump();
            true
        } else {
            false
        };
        let ty = self.expect_type_name()?.to_string();
        let name = self.expect_identifier_like()?.to_string();
        let init = if self.at_punct("=") {
            self.bump();
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        let end = self.expect_punct(";")?.span.end;
        Ok(ParsedStmt::Let {
            is_const,
            ty,
            name,
            init,
            span: Span::new(start, end),
        })
    }

    fn parse_expr(&mut self, min_binding_power: u8) -> Result<ParsedExpr, Diagnostic> {
        let mut lhs = self.parse_prefix()?;
        loop {
            let Some((op, left_bp, right_bp)) = self.current_binary_op() else {
                break;
            };
            if left_bp < min_binding_power {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(right_bp)?;
            let span = Span::new(lhs.span.start, rhs.span.end);
            lhs = ParsedExpr {
                span,
                kind: ParsedExprKind::Binary {
                    op,
                    lhs: alloc::boxed::Box::new(lhs),
                    rhs: alloc::boxed::Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<ParsedExpr, Diagnostic> {
        if self.at_punct("-") {
            let start = self.bump().span.start;
            let expr = self.parse_expr(7)?;
            return Ok(ParsedExpr {
                span: Span::new(start, expr.span.end),
                kind: ParsedExprKind::Unary {
                    op: UnaryOp::Neg,
                    expr: alloc::boxed::Box::new(expr),
                },
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<ParsedExpr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        while self.at_punct(".") {
            self.bump();
            let fields = self.expect_identifier_like()?.to_string();
            let end = self.previous().span.end;
            expr = ParsedExpr {
                span: Span::new(expr.span.start, end),
                kind: ParsedExprKind::Swizzle {
                    base: alloc::boxed::Box::new(expr),
                    fields,
                },
            };
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<ParsedExpr, Diagnostic> {
        let tok = self.bump();
        match tok.kind {
            TokenKind::FloatLiteral => {
                let text = tok.lexeme(self.source);
                let value = text
                    .parse::<f32>()
                    .map_err(|_| Diagnostic::error(tok.span, "failed to parse float literal"))?;
                Ok(ParsedExpr {
                    span: tok.span,
                    kind: ParsedExprKind::FloatLiteral(value),
                })
            }
            TokenKind::IntLiteral => {
                let text = tok.lexeme(self.source);
                let value = text
                    .parse::<i32>()
                    .map_err(|_| Diagnostic::error(tok.span, "failed to parse int literal"))?;
                Ok(ParsedExpr {
                    span: tok.span,
                    kind: ParsedExprKind::IntLiteral(value),
                })
            }
            TokenKind::UintLiteral => {
                let text = tok.lexeme(self.source);
                let trimmed = text.trim_end_matches(['u', 'U']);
                let value = trimmed
                    .parse::<u32>()
                    .map_err(|_| Diagnostic::error(tok.span, "failed to parse uint literal"))?;
                Ok(ParsedExpr {
                    span: tok.span,
                    kind: ParsedExprKind::UIntLiteral(value),
                })
            }
            TokenKind::Identifier | TokenKind::Keyword(_) => {
                let name = tok.lexeme(self.source).to_string();
                if name == "true" || name == "false" {
                    return Ok(ParsedExpr {
                        span: tok.span,
                        kind: ParsedExprKind::BoolLiteral(name == "true"),
                    });
                }
                if self.at_punct("(") {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at_punct(")") {
                        loop {
                            args.push(self.parse_expr(0)?);
                            if self.at_punct(",") {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                    }
                    let end = self.expect_punct(")")?.span.end;
                    return Ok(ParsedExpr {
                        span: Span::new(tok.span.start, end),
                        kind: ParsedExprKind::Call { name, args },
                    });
                }
                Ok(ParsedExpr {
                    span: tok.span,
                    kind: ParsedExprKind::Name(name),
                })
            }
            TokenKind::Punct if tok.lexeme(self.source) == "(" => {
                let expr = self.parse_expr(0)?;
                self.expect_punct(")")?;
                Ok(expr)
            }
            _ => Err(Diagnostic::expected(
                tok.span,
                "expression",
                tok.lexeme(self.source),
            )),
        }
    }

    fn current_binary_op(&self) -> Option<(BinaryOp, u8, u8)> {
        let tok = self.current()?;
        let op = match tok.lexeme(self.source) {
            "==" => BinaryOp::Eq,
            "!=" => BinaryOp::Ne,
            "<" => BinaryOp::Lt,
            "<=" => BinaryOp::Le,
            ">" => BinaryOp::Gt,
            ">=" => BinaryOp::Ge,
            "+" => BinaryOp::Add,
            "-" => BinaryOp::Sub,
            "*" => BinaryOp::Mul,
            "/" => BinaryOp::Div,
            _ => return None,
        };
        let bp = match op {
            BinaryOp::Eq | BinaryOp::Ne => (1, 2),
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => (3, 4),
            BinaryOp::Add | BinaryOp::Sub => (5, 6),
            BinaryOp::Mul | BinaryOp::Div => (7, 8),
        };
        Some((op, bp.0, bp.1))
    }

    fn starts_type_name(&self) -> bool {
        self.current()
            .is_some_and(|t| token_is_type_name(t, self.source))
    }

    fn expect_type_name(&mut self) -> Result<&'src str, Diagnostic> {
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

    fn expect_keyword(&mut self, keyword: Keyword) -> Result<Token, Diagnostic> {
        if self.at_keyword(keyword) {
            Ok(self.bump())
        } else {
            Err(Diagnostic::expected(
                self.current_span(),
                "keyword",
                self.current_text(),
            ))
        }
    }

    fn expect_punct(&mut self, punct: &str) -> Result<Token, Diagnostic> {
        if self.at_punct(punct) {
            Ok(self.bump())
        } else {
            Err(Diagnostic::expected(
                self.current_span(),
                punct,
                self.current_text(),
            ))
        }
    }

    fn expect_identifier_like(&mut self) -> Result<&'src str, Diagnostic> {
        if self.at_identifier_like() {
            Ok(self.bump().lexeme(self.source))
        } else {
            Err(Diagnostic::expected(
                self.current_span(),
                "identifier",
                self.current_text(),
            ))
        }
    }

    fn at_identifier_like(&self) -> bool {
        matches!(
            self.current().map(|t| t.kind),
            Some(TokenKind::Identifier | TokenKind::Keyword(_))
        )
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        matches!(self.current().map(|t| t.kind), Some(TokenKind::Keyword(k)) if k == keyword)
    }

    fn at_punct(&self, punct: &str) -> bool {
        self.current()
            .is_some_and(|t| t.kind == TokenKind::Punct && t.lexeme(self.source) == punct)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn current(&self) -> Option<Token> {
        self.tokens.get(self.pos).copied()
    }

    fn previous(&self) -> Token {
        self.tokens[self.pos.saturating_sub(1)]
    }

    fn current_span(&self) -> Span {
        self.current().map_or_else(
            || {
                self.tokens
                    .last()
                    .map_or_else(|| Span::new(0, 0), |t| t.span)
            },
            |t| t.span,
        )
    }

    fn current_text(&self) -> &str {
        self.current()
            .map_or("end of input", |t| t.lexeme(self.source))
    }

    fn bump(&mut self) -> Token {
        let tok = self
            .current()
            .expect("body parser bump called at end of token stream");
        self.pos += 1;
        tok
    }
}

fn stmt_end(stmt: &ParsedStmt) -> usize {
    match stmt {
        ParsedStmt::Let { span, .. }
        | ParsedStmt::Assign { span, .. }
        | ParsedStmt::If { span, .. } => span.end,
        ParsedStmt::Return(expr) => expr.span.end,
    }
}

fn token_is_type_name(tok: Token, source: &str) -> bool {
    match tok.kind {
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
            "bool" | "float" | "int" | "uint" | "vec2" | "vec3" | "vec4" | "void"
        ),
        _ => false,
    }
}
