use alloc::vec::Vec;

use crate::{Diagnostic, Span, Token, TokenKind};

use crate::syntax::{ParsedExpr, ParsedFunctionBody, ParsedStmt};

pub fn parse_function_body(
    source: &str,
    tokens: &[Token],
    body_span: Span,
    struct_names: &[alloc::string::String],
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
    BodyParser::new(source, &body_tokens, struct_names).parse()
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
    let mut parser = BodyParser::new(source, &expr_tokens, &[]);
    let expr = parser.parse_expr(0)?;
    if !parser.at_end() {
        return Err(Diagnostic::error(
            parser.current_span(),
            "unexpected tokens after expression",
        ));
    }
    Ok(expr)
}

pub(super) struct BodyParser<'src, 'tok> {
    pub(super) source: &'src str,
    pub(super) tokens: &'tok [Token],
    pub(super) struct_names: &'tok [alloc::string::String],
    pub(super) pos: usize,
}

impl<'src, 'tok> BodyParser<'src, 'tok> {
    fn new(
        source: &'src str,
        tokens: &'tok [Token],
        struct_names: &'tok [alloc::string::String],
    ) -> Self {
        Self {
            source,
            tokens,
            struct_names,
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
}

mod cursor;
mod expr;
mod stmt;
mod ty;

pub(super) fn stmt_end(stmt: &ParsedStmt) -> usize {
    match stmt {
        ParsedStmt::Let { span, .. }
        | ParsedStmt::LetGroup { span, .. }
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
        ParsedStmt::Return { span, .. } => span.end,
    }
}
