use alloc::string::ToString;
use alloc::vec::Vec;

use crate::syntax::{AssignOp, BinaryOp, IncDecOp, ParsedExpr, ParsedExprKind, UnaryOp};
use crate::{Diagnostic, Span, TokenKind};

use super::BodyParser;
use super::ty::token_text_is_type_name;

impl<'src, 'tok> BodyParser<'src, 'tok> {
    pub(super) fn parse_expr(&mut self, min_binding_power: u8) -> Result<ParsedExpr, Diagnostic> {
        let mut lhs = self.parse_prefix()?;
        loop {
            if let Some(op) = self.current_assign_op() {
                if min_binding_power > 1 {
                    break;
                }
                if !is_assignment_target(&lhs) {
                    return Err(Diagnostic::error(lhs.span, "invalid assignment target"));
                }
                self.bump();
                let value = self.parse_expr(1)?;
                let span = Span::new(lhs.span.start, value.span.end);
                lhs = ParsedExpr {
                    span,
                    kind: ParsedExprKind::Assign {
                        target: alloc::boxed::Box::new(lhs),
                        op,
                        value: alloc::boxed::Box::new(value),
                    },
                };
                continue;
            }
            if self.at_punct("?") {
                if min_binding_power > 1 {
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

    pub(super) fn parse_prefix(&mut self) -> Result<ParsedExpr, Diagnostic> {
        if self.at_punct("++") || self.at_punct("--") {
            let op_tok = self.bump();
            let op = if op_tok.lexeme(self.source) == "++" {
                IncDecOp::Increment
            } else {
                IncDecOp::Decrement
            };
            let target = self.parse_postfix()?;
            if !is_assignment_target(&target) {
                return Err(Diagnostic::error(target.span, "invalid increment target"));
            }
            return Ok(ParsedExpr {
                span: Span::new(op_tok.span.start, target.span.end),
                kind: ParsedExprKind::IncDec {
                    target: alloc::boxed::Box::new(target),
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
        if self.at_punct("+") {
            self.bump();
            return self.parse_expr(15);
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

    pub(super) fn parse_postfix(&mut self) -> Result<ParsedExpr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.at_punct(".") {
                self.bump();
                let fields = self.expect_identifier_like()?.to_string();
                if fields == "length" && self.at_punct("(") {
                    self.bump();
                    let end = self.expect_punct(")")?.span.end;
                    expr = ParsedExpr {
                        span: Span::new(expr.span.start, end),
                        kind: ParsedExprKind::Length {
                            base: alloc::boxed::Box::new(expr),
                        },
                    };
                    continue;
                }
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
            if self.at_punct("[") {
                if let Some(name) = self.try_parse_array_constructor_name(&expr)? {
                    self.expect_punct("(")?;
                    let mut args = Vec::new();
                    if !self.at_punct(")") {
                        loop {
                            args.push(self.parse_expr(1)?);
                            if self.at_punct(",") {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                    }
                    let end = self.expect_punct(")")?.span.end;
                    expr = ParsedExpr {
                        span: Span::new(expr.span.start, end),
                        kind: ParsedExprKind::Call { name, args },
                    };
                    continue;
                }
                self.bump();
                let index = self.parse_expr(0)?;
                let end = self.expect_punct("]")?.span.end;
                expr = ParsedExpr {
                    span: Span::new(expr.span.start, end),
                    kind: ParsedExprKind::Index {
                        base: alloc::boxed::Box::new(expr),
                        index: alloc::boxed::Box::new(index),
                    },
                };
                if self.at_punct("(")
                    && let Some(name) =
                        array_constructor_name(&expr, self.source, self.struct_names)
                {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at_punct(")") {
                        loop {
                            args.push(self.parse_expr(1)?);
                            if self.at_punct(",") {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                    }
                    let end = self.expect_punct(")")?.span.end;
                    expr = ParsedExpr {
                        span: Span::new(expr.span.start, end),
                        kind: ParsedExprKind::Call { name, args },
                    };
                }
                continue;
            }
            if self.at_punct("++") || self.at_punct("--") {
                if !is_assignment_target(&expr) {
                    return Err(Diagnostic::error(expr.span, "invalid increment target"));
                }
                let op_tok = self.bump();
                let op = if op_tok.lexeme(self.source) == "++" {
                    IncDecOp::Increment
                } else {
                    IncDecOp::Decrement
                };
                expr = ParsedExpr {
                    span: Span::new(expr.span.start, op_tok.span.end),
                    kind: ParsedExprKind::IncDec {
                        target: alloc::boxed::Box::new(expr),
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

    pub(super) fn parse_primary(&mut self) -> Result<ParsedExpr, Diagnostic> {
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
                            args.push(self.parse_expr(1)?);
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
            TokenKind::Punct if tok.lexeme(self.source) == "{" => {
                let mut elements = Vec::new();
                if !self.at_punct("}") {
                    loop {
                        elements.push(self.parse_expr(1)?);
                        if self.at_punct(",") {
                            self.bump();
                            if self.at_punct("}") {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                let end = self.expect_punct("}")?.span.end;
                Ok(ParsedExpr {
                    span: Span::new(tok.span.start, end),
                    kind: ParsedExprKind::InitList { elements },
                })
            }
            _ => Err(Diagnostic::expected(
                tok.span,
                "expression",
                tok.lexeme(self.source),
            )),
        }
    }

    fn try_parse_array_constructor_name(
        &mut self,
        expr: &ParsedExpr,
    ) -> Result<Option<alloc::string::String>, Diagnostic> {
        let ParsedExprKind::Name(base_name) = &expr.kind else {
            return Ok(None);
        };
        if !token_text_is_type_name(base_name, self.struct_names) {
            return Ok(None);
        }
        let checkpoint = self.pos;
        let mut name = base_name.clone();
        while self.at_punct("[") {
            let start = self.expect_punct("[")?.span.start;
            if !self.at_punct("]") {
                let Some(len) = self.current() else {
                    self.pos = checkpoint;
                    return Ok(None);
                };
                if !matches!(len.kind, TokenKind::IntLiteral | TokenKind::UintLiteral) {
                    self.pos = checkpoint;
                    return Ok(None);
                }
                self.bump();
            }
            let end = self.expect_punct("]")?.span.end;
            name.push_str(&self.source[start..end]);
        }
        if self.at_punct("(") {
            Ok(Some(name))
        } else {
            self.pos = checkpoint;
            Ok(None)
        }
    }

    pub(super) fn current_binary_op(&self) -> Option<(BinaryOp, u8, u8)> {
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
            "," => BinaryOp::Comma,
            _ => return None,
        };
        let bp = match op {
            BinaryOp::Comma => (0, 1),
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

    pub(super) fn current_assign_op(&self) -> Option<AssignOp> {
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
}

pub(super) fn is_assignment_target(expr: &ParsedExpr) -> bool {
    matches!(
        expr.kind,
        ParsedExprKind::Name(_) | ParsedExprKind::Swizzle { .. } | ParsedExprKind::Index { .. }
    )
}

pub(super) fn array_constructor_name(
    expr: &ParsedExpr,
    source: &str,
    struct_names: &[alloc::string::String],
) -> Option<alloc::string::String> {
    let ParsedExprKind::Index { base, index } = &expr.kind else {
        return None;
    };
    let ParsedExprKind::Name(base_name) = &base.kind else {
        return None;
    };
    if !token_text_is_type_name(base_name, struct_names) {
        return None;
    }
    Some(alloc::format!(
        "{}[{}]",
        base_name,
        index.span_text(source).trim_end_matches(['u', 'U'])
    ))
}

trait SpanText {
    fn span_text<'a>(&self, source: &'a str) -> &'a str;
}

impl SpanText for ParsedExpr {
    fn span_text<'a>(&self, source: &'a str) -> &'a str {
        source.get(self.span.start..self.span.end).unwrap_or("")
    }
}
