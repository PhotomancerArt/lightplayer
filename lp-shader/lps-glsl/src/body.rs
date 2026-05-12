use alloc::string::ToString;
use alloc::vec::Vec;

use crate::{Diagnostic, Keyword, Span, Token, TokenKind};

pub use crate::syntax::{
    AssignOp, BinaryOp, IncDecOp, ParsedExpr, ParsedExprKind, ParsedFunctionBody, ParsedStmt,
    UnaryOp,
};

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
        if self.at_punct(";") {
            let span = self.bump().span;
            return Ok(ParsedStmt::Empty { span });
        }
        if self.at_punct("{") {
            let start = self.bump().span.start;
            let statements = self.parse_block_contents()?;
            let end = self.expect_punct("}")?.span.end;
            return Ok(ParsedStmt::Block {
                statements,
                span: Span::new(start, end),
            });
        }
        if self.at_keyword(Keyword::Return) {
            self.bump();
            let expr = self.parse_expr(0)?;
            self.expect_punct(";")?;
            return Ok(ParsedStmt::Return(expr));
        }
        if self.at_keyword(Keyword::If) {
            return self.parse_if();
        }
        if self.at_keyword(Keyword::For) {
            return self.parse_for();
        }
        if self.at_keyword(Keyword::While) {
            return self.parse_while();
        }
        if self.at_keyword(Keyword::Do) {
            return self.parse_do_while();
        }
        if self.at_keyword(Keyword::Break) {
            let start = self.bump().span.start;
            let end = self.expect_punct(";")?.span.end;
            return Ok(ParsedStmt::Break {
                span: Span::new(start, end),
            });
        }
        if self.at_keyword(Keyword::Continue) {
            let start = self.bump().span.start;
            let end = self.expect_punct(";")?.span.end;
            return Ok(ParsedStmt::Continue {
                span: Span::new(start, end),
            });
        }
        if self.at_keyword(Keyword::Const) || self.starts_type_name() {
            return self.parse_let();
        }
        if self.at_identifier_like() {
            let checkpoint = self.pos;
            let name_tok = self.bump();
            if let Some(op) = self.current_assign_op() {
                self.bump();
                let value = self.parse_expr(0)?;
                let end = self.expect_punct(";")?.span.end;
                return Ok(ParsedStmt::Assign {
                    name: name_tok.lexeme(self.source).to_string(),
                    op,
                    value,
                    span: Span::new(name_tok.span.start, end),
                });
            }
            self.pos = checkpoint;
        }
        let expr = self.parse_expr(0)?;
        let end = self.expect_punct(";")?.span.end;
        Ok(ParsedStmt::Expr {
            span: Span::new(expr.span.start, end),
            expr,
        })
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

    fn parse_for(&mut self) -> Result<ParsedStmt, Diagnostic> {
        let start = self.expect_keyword(Keyword::For)?.span.start;
        self.expect_punct("(")?;
        let init = if self.at_punct(";") {
            self.bump();
            Vec::new()
        } else if self.at_keyword(Keyword::Const) || self.starts_type_name() {
            alloc::vec![self.parse_let()?]
        } else {
            let expr = self.parse_expr(0)?;
            let end = self.expect_punct(";")?.span.end;
            alloc::vec![ParsedStmt::Expr {
                span: Span::new(expr.span.start, end),
                expr,
            }]
        };
        let condition = if self.at_punct(";") {
            self.bump();
            None
        } else {
            let condition = self.parse_expr(0)?;
            self.expect_punct(";")?;
            Some(condition)
        };
        let continuing = if self.at_punct(")") {
            Vec::new()
        } else {
            let expr = self.parse_expr(0)?;
            alloc::vec![ParsedStmt::Expr {
                span: expr.span,
                expr,
            }]
        };
        self.expect_punct(")")?;
        let body = self.parse_statement_or_block()?;
        let end = body
            .last()
            .map_or_else(|| continuing.last().map_or(start, stmt_end), stmt_end);
        Ok(ParsedStmt::For {
            init,
            condition,
            continuing,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_while(&mut self) -> Result<ParsedStmt, Diagnostic> {
        let start = self.expect_keyword(Keyword::While)?.span.start;
        self.expect_punct("(")?;
        let condition = self.parse_expr(0)?;
        self.expect_punct(")")?;
        let body = self.parse_statement_or_block()?;
        let end = body.last().map_or(condition.span.end, stmt_end);
        Ok(ParsedStmt::While {
            condition,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_do_while(&mut self) -> Result<ParsedStmt, Diagnostic> {
        let start = self.expect_keyword(Keyword::Do)?.span.start;
        let body = self.parse_statement_or_block()?;
        self.expect_keyword(Keyword::While)?;
        self.expect_punct("(")?;
        let condition = self.parse_expr(0)?;
        self.expect_punct(")")?;
        let end = self.expect_punct(";")?.span.end;
        Ok(ParsedStmt::DoWhile {
            body,
            condition,
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
            if self.at_punct("=") {
                if min_binding_power > 0 {
                    break;
                }
                let ParsedExprKind::Name(name) = &lhs.kind else {
                    return Err(Diagnostic::error(lhs.span, "invalid assignment target"));
                };
                let name = name.clone();
                self.bump();
                let value = self.parse_expr(0)?;
                let span = Span::new(lhs.span.start, value.span.end);
                lhs = ParsedExpr {
                    span,
                    kind: ParsedExprKind::Assign {
                        name,
                        value: alloc::boxed::Box::new(value),
                    },
                };
                continue;
            }
            if self.at_punct("?") {
                if min_binding_power > 0 {
                    break;
                }
                self.bump();
                let accept = self.parse_expr(0)?;
                self.expect_punct(":")?;
                let reject = self.parse_expr(0)?;
                let span = Span::new(lhs.span.start, reject.span.end);
                lhs = ParsedExpr {
                    span,
                    kind: ParsedExprKind::Conditional {
                        condition: alloc::boxed::Box::new(lhs),
                        accept: alloc::boxed::Box::new(accept),
                        reject: alloc::boxed::Box::new(reject),
                    },
                };
                continue;
            }
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
        if self.at_punct("++") || self.at_punct("--") {
            let op_tok = self.bump();
            let op = if op_tok.lexeme(self.source) == "++" {
                IncDecOp::Increment
            } else {
                IncDecOp::Decrement
            };
            let name_tok = self.expect_identifier_like()?;
            return Ok(ParsedExpr {
                span: Span::new(op_tok.span.start, self.previous().span.end),
                kind: ParsedExprKind::IncDec {
                    name: name_tok.to_string(),
                    op,
                    prefix: true,
                },
            });
        }
        if self.at_punct("-") {
            let start = self.bump().span.start;
            if self
                .current()
                .is_some_and(|t| matches!(t.kind, TokenKind::IntLiteral))
            {
                let tok = self.bump();
                let value = tok
                    .lexeme(self.source)
                    .parse::<i64>()
                    .map_err(|_| Diagnostic::error(tok.span, "failed to parse int literal"))?;
                let value = i32::try_from(-value)
                    .map_err(|_| Diagnostic::error(tok.span, "int literal is out of range"))?;
                return Ok(ParsedExpr {
                    span: Span::new(start, tok.span.end),
                    kind: ParsedExprKind::IntLiteral(value),
                });
            }
            let expr = self.parse_expr(15)?;
            return Ok(ParsedExpr {
                span: Span::new(start, expr.span.end),
                kind: ParsedExprKind::Unary {
                    op: UnaryOp::Neg,
                    expr: alloc::boxed::Box::new(expr),
                },
            });
        }
        if self.at_punct("!") {
            let start = self.bump().span.start;
            let expr = self.parse_expr(15)?;
            return Ok(ParsedExpr {
                span: Span::new(start, expr.span.end),
                kind: ParsedExprKind::Unary {
                    op: UnaryOp::Not,
                    expr: alloc::boxed::Box::new(expr),
                },
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<ParsedExpr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.at_punct(".") {
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
                continue;
            }
            if self.at_punct("++") || self.at_punct("--") {
                let ParsedExprKind::Name(name) = &expr.kind else {
                    return Err(Diagnostic::error(expr.span, "invalid increment target"));
                };
                let name = name.clone();
                let op_tok = self.bump();
                let op = if op_tok.lexeme(self.source) == "++" {
                    IncDecOp::Increment
                } else {
                    IncDecOp::Decrement
                };
                expr = ParsedExpr {
                    span: Span::new(expr.span.start, op_tok.span.end),
                    kind: ParsedExprKind::IncDec {
                        name,
                        op,
                        prefix: false,
                    },
                };
                continue;
            }
            break;
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
            "%" => BinaryOp::Mod,
            "&&" => BinaryOp::LogicalAnd,
            "||" => BinaryOp::LogicalOr,
            "^^" => BinaryOp::LogicalXor,
            _ => return None,
        };
        let bp = match op {
            BinaryOp::LogicalOr => (1, 2),
            BinaryOp::LogicalXor => (3, 4),
            BinaryOp::LogicalAnd => (5, 6),
            BinaryOp::Eq | BinaryOp::Ne => (7, 8),
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => (9, 10),
            BinaryOp::Add | BinaryOp::Sub => (11, 12),
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => (13, 14),
        };
        Some((op, bp.0, bp.1))
    }

    fn current_assign_op(&self) -> Option<AssignOp> {
        let op = match self.current()?.lexeme(self.source) {
            "=" => AssignOp::Set,
            "+=" => AssignOp::Add,
            "-=" => AssignOp::Sub,
            "*=" => AssignOp::Mul,
            "/=" => AssignOp::Div,
            "%=" => AssignOp::Mod,
            _ => return None,
        };
        Some(op)
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
        | ParsedStmt::If { span, .. }
        | ParsedStmt::For { span, .. }
        | ParsedStmt::While { span, .. }
        | ParsedStmt::DoWhile { span, .. }
        | ParsedStmt::Break { span }
        | ParsedStmt::Continue { span }
        | ParsedStmt::Block { span, .. }
        | ParsedStmt::Empty { span }
        | ParsedStmt::Expr { span, .. } => span.end,
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
