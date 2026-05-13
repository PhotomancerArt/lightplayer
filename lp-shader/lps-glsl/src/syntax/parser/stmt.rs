use alloc::string::ToString;
use alloc::vec::Vec;

use crate::syntax::ParsedStmt;
use crate::{Diagnostic, Keyword, Span};

use super::BodyParser;
use super::stmt_end;

impl<'src, 'tok> BodyParser<'src, 'tok> {
    pub(super) fn parse_block_contents(&mut self) -> Result<Vec<ParsedStmt>, Diagnostic> {
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

    pub(super) fn parse_statement(&mut self) -> Result<ParsedStmt, Diagnostic> {
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
            let start = self.bump().span.start;
            let expr = if self.at_punct(";") {
                None
            } else {
                Some(self.parse_expr(0)?)
            };
            let end = self.expect_punct(";")?.span.end;
            return Ok(ParsedStmt::Return {
                expr,
                span: Span::new(start, end),
            });
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

    pub(super) fn parse_if(&mut self) -> Result<ParsedStmt, Diagnostic> {
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

    pub(super) fn parse_for(&mut self) -> Result<ParsedStmt, Diagnostic> {
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

    pub(super) fn parse_while(&mut self) -> Result<ParsedStmt, Diagnostic> {
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

    pub(super) fn parse_do_while(&mut self) -> Result<ParsedStmt, Diagnostic> {
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

    pub(super) fn parse_statement_or_block(&mut self) -> Result<Vec<ParsedStmt>, Diagnostic> {
        if self.at_punct("{") {
            self.bump();
            let statements = self.parse_block_contents()?;
            self.expect_punct("}")?;
            Ok(statements)
        } else {
            Ok(alloc::vec![self.parse_statement()?])
        }
    }

    pub(super) fn parse_let(&mut self) -> Result<ParsedStmt, Diagnostic> {
        let start = self.current_span().start;
        let is_const = if self.at_keyword(Keyword::Const) {
            self.bump();
            true
        } else {
            false
        };
        let mut base_ty = self.expect_type_name()?.to_string();
        while self.at_punct("[") {
            base_ty.push_str(self.parse_array_suffix()?);
        }
        let mut declarations = Vec::new();
        loop {
            let decl_start = self.current_span().start;
            let mut ty = base_ty.clone();
            let name = self.expect_identifier_like()?.to_string();
            while self.at_punct("[") {
                ty.push_str(self.parse_array_suffix()?);
            }
            let init = if self.at_punct("=") {
                self.bump();
                Some(self.parse_expr(1)?)
            } else {
                None
            };
            let decl_end = init
                .as_ref()
                .map_or(self.previous().span.end, |expr| expr.span.end);
            declarations.push(crate::syntax::ParsedLetDecl {
                ty,
                name,
                init,
                span: Span::new(decl_start, decl_end),
            });
            if self.at_punct(",") {
                self.bump();
            } else {
                break;
            }
        }
        let end = self.expect_punct(";")?.span.end;
        let span = Span::new(start, end);
        if declarations.len() == 1 {
            let decl = declarations.remove(0);
            Ok(ParsedStmt::Let {
                is_const,
                ty: decl.ty,
                name: decl.name,
                init: decl.init,
                span,
            })
        } else {
            Ok(ParsedStmt::LetGroup {
                is_const,
                ty: base_ty,
                declarations,
                span,
            })
        }
    }
}
