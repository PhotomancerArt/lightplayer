use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{Diagnostic, Keyword, Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFunctionBody {
    pub statements: Vec<ParsedStmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedStmt {
    Return(ParsedExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedExpr {
    pub span: Span,
    pub kind: ParsedExprKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedExprKind {
    FloatLiteral(f32),
    IntLiteral(i32),
    UIntLiteral(u32),
    Name(String),
    Call {
        name: String,
        args: Vec<ParsedExpr>,
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
        let mut statements = Vec::new();
        while !self.at_punct("}") {
            statements.push(self.parse_statement()?);
        }
        self.expect_punct("}")?;
        if !self.at_end() {
            return Err(Diagnostic::error(
                self.current_span(),
                "unexpected tokens after function body",
            ));
        }
        Ok(ParsedFunctionBody { statements })
    }

    fn parse_statement(&mut self) -> Result<ParsedStmt, Diagnostic> {
        if self.at_keyword(Keyword::Return) {
            self.bump();
            let expr = self.parse_expr(0)?;
            self.expect_punct(";")?;
            return Ok(ParsedStmt::Return(expr));
        }
        Err(Diagnostic::error(
            self.current_span(),
            "M2 lps-glsl supports only return statements in function bodies",
        ))
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
            let expr = self.parse_expr(5)?;
            return Ok(ParsedExpr {
                span: Span::new(start, expr.span.end),
                kind: ParsedExprKind::Unary {
                    op: UnaryOp::Neg,
                    expr: alloc::boxed::Box::new(expr),
                },
            });
        }
        self.parse_primary()
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
            "+" => BinaryOp::Add,
            "-" => BinaryOp::Sub,
            "*" => BinaryOp::Mul,
            "/" => BinaryOp::Div,
            _ => return None,
        };
        let bp = match op {
            BinaryOp::Add | BinaryOp::Sub => (1, 2),
            BinaryOp::Mul | BinaryOp::Div => (3, 4),
        };
        Some((op, bp.0, bp.1))
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
