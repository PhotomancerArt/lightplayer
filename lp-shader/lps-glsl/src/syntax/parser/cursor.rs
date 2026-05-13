use crate::{Diagnostic, Keyword, Span, Token, TokenKind};

use super::BodyParser;

impl<'src, 'tok> BodyParser<'src, 'tok> {
    pub(super) fn expect_keyword(&mut self, keyword: Keyword) -> Result<Token, Diagnostic> {
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

    pub(super) fn expect_punct(&mut self, punct: &str) -> Result<Token, Diagnostic> {
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

    pub(super) fn expect_identifier_like(&mut self) -> Result<&'src str, Diagnostic> {
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

    pub(super) fn at_identifier_like(&self) -> bool {
        matches!(
            self.current().map(|t| t.kind),
            Some(TokenKind::Identifier | TokenKind::Keyword(_))
        )
    }

    pub(super) fn at_keyword(&self, keyword: Keyword) -> bool {
        matches!(self.current().map(|t| t.kind), Some(TokenKind::Keyword(k)) if k == keyword)
    }

    pub(super) fn at_punct(&self, punct: &str) -> bool {
        self.current()
            .is_some_and(|t| t.kind == TokenKind::Punct && t.lexeme(self.source) == punct)
    }

    pub(super) fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub(super) fn current(&self) -> Option<Token> {
        self.tokens.get(self.pos).copied()
    }

    pub(super) fn previous(&self) -> Token {
        self.tokens[self.pos.saturating_sub(1)]
    }

    pub(super) fn current_span(&self) -> Span {
        self.current().map_or_else(
            || {
                self.tokens
                    .last()
                    .map_or_else(|| Span::new(0, 0), |t| t.span)
            },
            |t| t.span,
        )
    }

    pub(super) fn current_text(&self) -> &str {
        self.current()
            .map_or("end of input", |t| t.lexeme(self.source))
    }

    pub(super) fn bump(&mut self) -> Token {
        let tok = self
            .current()
            .expect("body parser bump called at end of token stream");
        self.pos += 1;
        tok
    }
}
