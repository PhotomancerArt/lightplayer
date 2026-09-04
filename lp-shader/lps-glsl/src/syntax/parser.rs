use crate::index::StructDecl;
use crate::{Diagnostic, Span, Token, TokenKind};

use crate::syntax::{ParsedExpr, ParsedFunctionBody, ParsedStmt};

/// Slice `tokens` (emitted in ascending span order by the lexer, see
/// `src/lexer.rs`) down to the contiguous run fully contained in `span`.
///
/// Tokens are non-overlapping and ascending, so both `span.start` and
/// `span.end` predicates below are monotonic over the whole tape, and their
/// partition points bound a single contiguous subslice — no per-call Vec or
/// full-tape scan required.
///
/// The trailing Eof token is dropped first. Its span is empty *at* source
/// end, not past it, so a body that closes on the last byte of the source
/// (no trailing newline) ends exactly where Eof sits and would otherwise
/// swallow it — the parser then reports "unexpected tokens after function
/// body". Dropping it here restores the filter this slicing replaced.
fn token_subslice<'tok>(tokens: &'tok [Token], span: Span) -> &'tok [Token] {
    let tokens = match tokens.split_last() {
        Some((last, rest)) if matches!(last.kind, TokenKind::Eof) => rest,
        _ => tokens,
    };
    let lo = tokens.partition_point(|t| t.span.start < span.start);
    let hi = tokens.partition_point(|t| t.span.end <= span.end).max(lo);
    let subslice = &tokens[lo..hi];
    debug_assert!(
        subslice.iter().all(|t| !matches!(t.kind, TokenKind::Eof)),
        "Eof token unexpectedly fell inside a body/init span"
    );
    subslice
}

pub fn parse_function_body<'src>(
    source: &'src str,
    tokens: &[Token],
    body_span: Span,
    struct_names: &[StructDecl],
) -> Result<ParsedFunctionBody<'src>, Diagnostic> {
    let body_tokens = token_subslice(tokens, body_span);
    BodyParser::new(source, body_tokens, struct_names).parse()
}

pub fn parse_expr_tokens<'src>(
    source: &'src str,
    tokens: &[Token],
    span: Span,
) -> Result<ParsedExpr<'src>, Diagnostic> {
    let expr_tokens = token_subslice(tokens, span);
    let mut parser = BodyParser::new(source, expr_tokens, &[]);
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
    pub(super) struct_names: &'tok [StructDecl],
    pub(super) pos: usize,
}

impl<'src, 'tok> BodyParser<'src, 'tok> {
    fn new(source: &'src str, tokens: &'tok [Token], struct_names: &'tok [StructDecl]) -> Self {
        Self {
            source,
            tokens,
            struct_names,
            pos: 0,
        }
    }

    fn parse(mut self) -> Result<ParsedFunctionBody<'src>, Diagnostic> {
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

pub(super) fn stmt_end(stmt: &ParsedStmt<'_>) -> usize {
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
