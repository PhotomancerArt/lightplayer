use alloc::vec::Vec;

use crate::{Diagnostic, Keyword, Span, Token, TokenKind};

pub fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).lex_all()
}

struct Lexer<'src> {
    source: &'src str,
    pos: usize,
}

impl<'src> Lexer<'src> {
    fn new(source: &'src str) -> Self {
        Self { source, pos: 0 }
    }

    fn lex_all(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia()?;
            let start = self.pos;
            let Some(b) = self.peek() else {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(self.pos, self.pos),
                });
                return Ok(tokens);
            };
            let kind = if is_ident_start(b) {
                self.bump();
                while self.peek().is_some_and(is_ident_continue) {
                    self.bump();
                }
                let text = &self.source[start..self.pos];
                Keyword::parse(text).map_or(TokenKind::Identifier, TokenKind::Keyword)
            } else if b.is_ascii_digit()
                || (b == b'.' && self.peek_next().is_some_and(|n| n.is_ascii_digit()))
            {
                self.lex_number(start)
            } else {
                self.lex_punct()?
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, self.pos),
            });
        }
    }

    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
                self.bump();
            }
            if self.peek() == Some(b'/') && self.peek_next() == Some(b'/') {
                while let Some(b) = self.peek() {
                    self.bump();
                    if b == b'\n' {
                        break;
                    }
                }
                continue;
            }
            if self.peek() == Some(b'/') && self.peek_next() == Some(b'*') {
                let start = self.pos;
                self.bump();
                self.bump();
                let mut closed = false;
                while self.peek().is_some() {
                    if self.peek() == Some(b'*') && self.peek_next() == Some(b'/') {
                        self.bump();
                        self.bump();
                        closed = true;
                        break;
                    }
                    self.bump();
                }
                if !closed {
                    return Err(Diagnostic::error(
                        Span::new(start, self.pos),
                        "unterminated block comment",
                    ));
                }
                continue;
            }
            return Ok(());
        }
    }

    fn lex_number(&mut self, start: usize) -> TokenKind {
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.bump();
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some(b'.') {
            is_float = true;
            self.bump();
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.bump();
            }
        }
        if self.peek() == Some(b'u') || self.peek() == Some(b'U') {
            self.bump();
            return TokenKind::UintLiteral;
        }
        let _ = start;
        if is_float {
            TokenKind::FloatLiteral
        } else {
            TokenKind::IntLiteral
        }
    }

    fn lex_punct(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.pos;
        let b = self.bump().expect("punct byte");
        let two_char = matches!(
            (b, self.peek()),
            (b'+', Some(b'+' | b'='))
                | (b'-', Some(b'-' | b'='))
                | (b'*' | b'/' | b'%' | b'=' | b'!' | b'<' | b'>', Some(b'='))
                | (b'&', Some(b'&'))
                | (b'|', Some(b'|'))
                | (b'^', Some(b'^'))
        );
        if two_char {
            self.bump();
            return Ok(TokenKind::Punct);
        }
        if matches!(
            b,
            b'(' | b')'
                | b'{'
                | b'}'
                | b'['
                | b']'
                | b';'
                | b','
                | b'.'
                | b':'
                | b'?'
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'='
                | b'!'
                | b'^'
                | b'<'
                | b'>'
        ) {
            Ok(TokenKind::Punct)
        } else {
            Err(Diagnostic::error(
                Span::new(start, self.pos),
                "unexpected character",
            ))
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}
