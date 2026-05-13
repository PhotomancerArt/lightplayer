use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::{Token, TokenKind, lex};

pub(super) type ArraySizeConsts = BTreeMap<String, u32>;

pub(super) fn eval_array_size_expr(source: &str, consts: &ArraySizeConsts) -> Option<u32> {
    let tokens = lex(source).ok()?;
    let mut parser = ConstIntParser {
        source,
        tokens: &tokens,
        consts,
        pos: 0,
    };
    let value = parser.parse_add_sub()?;
    if !parser.at_end() || value < 0 {
        return None;
    }
    u32::try_from(value).ok()
}

struct ConstIntParser<'src, 'tok> {
    source: &'src str,
    tokens: &'tok [Token],
    consts: &'tok ArraySizeConsts,
    pos: usize,
}

impl<'src, 'tok> ConstIntParser<'src, 'tok> {
    fn parse_add_sub(&mut self) -> Option<i64> {
        let mut value = self.parse_mul_div()?;
        loop {
            if self.at_punct("+") {
                self.bump();
                value = value.checked_add(self.parse_mul_div()?)?;
            } else if self.at_punct("-") {
                self.bump();
                value = value.checked_sub(self.parse_mul_div()?)?;
            } else {
                return Some(value);
            }
        }
    }

    fn parse_mul_div(&mut self) -> Option<i64> {
        let mut value = self.parse_unary()?;
        loop {
            if self.at_punct("*") {
                self.bump();
                value = value.checked_mul(self.parse_unary()?)?;
            } else if self.at_punct("/") {
                self.bump();
                value = value.checked_div(self.parse_unary()?)?;
            } else if self.at_punct("%") {
                self.bump();
                value = value.checked_rem(self.parse_unary()?)?;
            } else {
                return Some(value);
            }
        }
    }

    fn parse_unary(&mut self) -> Option<i64> {
        if self.at_punct("+") {
            self.bump();
            return self.parse_unary();
        }
        if self.at_punct("-") {
            self.bump();
            return self.parse_unary()?.checked_neg();
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<i64> {
        if self.at_punct("(") {
            self.bump();
            let value = self.parse_add_sub()?;
            if !self.at_punct(")") {
                return None;
            }
            self.bump();
            return Some(value);
        }
        let tok = self.current();
        match tok.kind {
            TokenKind::IntLiteral | TokenKind::UintLiteral => {
                self.bump();
                tok.lexeme(self.source)
                    .trim_end_matches(['u', 'U'])
                    .parse::<i64>()
                    .ok()
            }
            TokenKind::Identifier => {
                self.bump();
                self.consts
                    .get(tok.lexeme(self.source))
                    .copied()
                    .map(i64::from)
            }
            _ => None,
        }
    }

    fn at_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn at_punct(&self, punct: &str) -> bool {
        let tok = self.current();
        tok.kind == TokenKind::Punct && tok.lexeme(self.source) == punct
    }

    fn current(&self) -> Token {
        self.tokens.get(self.pos).copied().unwrap_or_else(|| {
            self.tokens
                .last()
                .copied()
                .expect("lexer always emits eof token")
        })
    }

    fn bump(&mut self) -> Token {
        let tok = self.current();
        self.pos += 1;
        tok
    }
}
