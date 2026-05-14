use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::syntax::{SourceSpan, SyntaxError, SyntaxEvent, SyntaxEventSource, split_string_events};

/// Direct JSON text syntax source.
pub struct JsonSyntaxSource<'a> {
    input: &'a str,
    cursor: usize,
    stack: Vec<JsonFrame>,
    pending: Vec<SyntaxEvent>,
    started: bool,
    finished: bool,
}

impl<'a> JsonSyntaxSource<'a> {
    pub fn new(input: &'a str) -> Result<Self, SyntaxError> {
        let mut source = Self {
            input,
            cursor: 0,
            stack: Vec::new(),
            pending: Vec::new(),
            started: false,
            finished: false,
        };
        source.skip_ws();
        Ok(source)
    }
}

impl SyntaxEventSource for JsonSyntaxSource<'_> {
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError> {
        if let Some(event) = self.pending.pop() {
            return Ok(Some(event));
        }

        if self.finished {
            return Ok(None);
        }

        if self.stack.is_empty() {
            if self.started {
                self.skip_ws();
                if self.is_eof() {
                    self.finished = true;
                    return Ok(None);
                }
                return Err(self.error("unexpected trailing JSON input"));
            }
            self.started = true;
            return self.parse_value_event().map(Some);
        }

        let frame = self.stack.pop().expect("checked non-empty");
        match frame {
            JsonFrame::Object {
                state: JsonObjectState::PropOrEnd { first },
            } => self.next_object_prop_or_end(first).map(Some),
            JsonFrame::Object {
                state: JsonObjectState::Value,
            } => {
                self.stack.push(JsonFrame::Object {
                    state: JsonObjectState::PropOrEnd { first: false },
                });
                self.parse_value_event().map(Some)
            }
            JsonFrame::Array { first } => self.next_array_value_or_end(first).map(Some),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum JsonFrame {
    Object { state: JsonObjectState },
    Array { first: bool },
}

#[derive(Clone, Copy, Debug)]
enum JsonObjectState {
    PropOrEnd { first: bool },
    Value,
}

impl JsonSyntaxSource<'_> {
    fn next_object_prop_or_end(&mut self, first: bool) -> Result<SyntaxEvent, SyntaxError> {
        self.skip_ws();
        if self.consume_byte(b'}') {
            return Ok(SyntaxEvent::EndObject {
                span: Some(SourceSpan::new(self.cursor - 1, self.cursor)),
            });
        }
        if !first {
            self.expect_byte(b',')?;
            self.skip_ws();
        }
        let (name, span) = self.parse_string()?;
        self.skip_ws();
        self.expect_byte(b':')?;
        self.stack.push(JsonFrame::Object {
            state: JsonObjectState::Value,
        });
        Ok(SyntaxEvent::Prop {
            name,
            span: Some(span),
        })
    }

    fn next_array_value_or_end(&mut self, first: bool) -> Result<SyntaxEvent, SyntaxError> {
        self.skip_ws();
        if self.consume_byte(b']') {
            return Ok(SyntaxEvent::EndArray {
                span: Some(SourceSpan::new(self.cursor - 1, self.cursor)),
            });
        }
        if !first {
            self.expect_byte(b',')?;
            self.skip_ws();
        }
        self.stack.push(JsonFrame::Array { first: false });
        self.parse_value_event()
    }

    fn parse_value_event(&mut self) -> Result<SyntaxEvent, SyntaxError> {
        self.skip_ws();
        match self.peek_byte() {
            Some(b'{') => {
                let start = self.cursor;
                self.cursor += 1;
                self.stack.push(JsonFrame::Object {
                    state: JsonObjectState::PropOrEnd { first: true },
                });
                Ok(SyntaxEvent::StartObject {
                    span: Some(SourceSpan::new(start, self.cursor)),
                })
            }
            Some(b'[') => {
                let start = self.cursor;
                self.cursor += 1;
                self.stack.push(JsonFrame::Array { first: true });
                Ok(SyntaxEvent::StartArray {
                    span: Some(SourceSpan::new(start, self.cursor)),
                })
            }
            Some(b'"') => {
                let (value, span) = self.parse_string()?;
                Ok(self.first_string_chunk(value, Some(span)))
            }
            Some(b't') => self.parse_literal(
                "true",
                SyntaxEvent::Bool {
                    value: true,
                    span: None,
                },
            ),
            Some(b'f') => self.parse_literal(
                "false",
                SyntaxEvent::Bool {
                    value: false,
                    span: None,
                },
            ),
            Some(b'n') => self.parse_literal("null", SyntaxEvent::Null { span: None }),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(_) => Err(self.error("unexpected JSON value")),
            None => Err(self.error("unexpected end of JSON input")),
        }
    }

    fn parse_string(&mut self) -> Result<(String, SourceSpan), SyntaxError> {
        let start = self.cursor;
        self.expect_byte(b'"')?;
        let mut output = String::new();
        loop {
            let Some(byte) = self.next_byte() else {
                return Err(self.error("unterminated JSON string"));
            };
            match byte {
                b'"' => return Ok((output, SourceSpan::new(start, self.cursor))),
                b'\\' => self.parse_escape(&mut output)?,
                0x00..=0x1f => return Err(self.error("control character in JSON string")),
                byte if byte < 0x80 => output.push(byte as char),
                byte => {
                    let char_start = self.cursor - 1;
                    let width = utf8_width(byte)
                        .ok_or_else(|| self.error("invalid UTF-8 start byte in JSON string"))?;
                    let end = char_start + width;
                    let slice = self
                        .input
                        .get(char_start..end)
                        .ok_or_else(|| self.error("truncated UTF-8 in JSON string"))?;
                    let ch = slice
                        .chars()
                        .next()
                        .ok_or_else(|| self.error("invalid UTF-8 in JSON string"))?;
                    output.push(ch);
                    self.cursor = end;
                }
            }
        }
    }

    fn parse_escape(&mut self, output: &mut String) -> Result<(), SyntaxError> {
        let Some(byte) = self.next_byte() else {
            return Err(self.error("unterminated JSON escape"));
        };
        match byte {
            b'"' => output.push('"'),
            b'\\' => output.push('\\'),
            b'/' => output.push('/'),
            b'b' => output.push('\u{08}'),
            b'f' => output.push('\u{0c}'),
            b'n' => output.push('\n'),
            b'r' => output.push('\r'),
            b't' => output.push('\t'),
            b'u' => {
                let code = self.parse_hex4()?;
                let ch = char::from_u32(code)
                    .ok_or_else(|| self.error("invalid JSON unicode escape"))?;
                output.push(ch);
            }
            _ => return Err(self.error("invalid JSON escape")),
        }
        Ok(())
    }

    fn parse_hex4(&mut self) -> Result<u32, SyntaxError> {
        let mut value = 0u32;
        for _ in 0..4 {
            let Some(byte) = self.next_byte() else {
                return Err(self.error("truncated JSON unicode escape"));
            };
            value = (value << 4)
                | match byte {
                    b'0'..=b'9' => u32::from(byte - b'0'),
                    b'a'..=b'f' => u32::from(byte - b'a' + 10),
                    b'A'..=b'F' => u32::from(byte - b'A' + 10),
                    _ => return Err(self.error("invalid JSON unicode escape")),
                };
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<SyntaxEvent, SyntaxError> {
        let start = self.cursor;
        if self.consume_byte(b'-') {
            if !self.consume_digits() {
                return Err(self.error("expected digits after minus sign"));
            }
        } else if !self.consume_digits() {
            return Err(self.error("expected digits in JSON number"));
        }
        if self.consume_byte(b'.') && !self.consume_digits() {
            return Err(self.error("expected digits after decimal point"));
        }
        if self.consume_byte(b'e') || self.consume_byte(b'E') {
            let _ = self.consume_byte(b'+') || self.consume_byte(b'-');
            if !self.consume_digits() {
                return Err(self.error("expected exponent digits"));
            }
        }
        Ok(SyntaxEvent::Number {
            text: self.input[start..self.cursor].to_string(),
            span: Some(SourceSpan::new(start, self.cursor)),
        })
    }

    fn parse_literal(
        &mut self,
        literal: &str,
        event: SyntaxEvent,
    ) -> Result<SyntaxEvent, SyntaxError> {
        let start = self.cursor;
        if self.input[self.cursor..].starts_with(literal) {
            self.cursor += literal.len();
            Ok(match event {
                SyntaxEvent::Bool { value, .. } => SyntaxEvent::Bool {
                    value,
                    span: Some(SourceSpan::new(start, self.cursor)),
                },
                SyntaxEvent::Null { .. } => SyntaxEvent::Null {
                    span: Some(SourceSpan::new(start, self.cursor)),
                },
                _ => event,
            })
        } else {
            Err(self.error(format!("expected JSON literal {literal}")))
        }
    }

    fn first_string_chunk(&mut self, value: String, span: Option<SourceSpan>) -> SyntaxEvent {
        let mut chunks = split_string_events(&value, span);
        chunks.reverse();
        let first = chunks
            .pop()
            .expect("split_string_events returns at least one");
        self.pending.extend(chunks);
        first
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek_byte(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.cursor += 1;
        }
    }

    fn consume_digits(&mut self) -> bool {
        let start = self.cursor;
        while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
            self.cursor += 1;
        }
        self.cursor > start
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek_byte() == Some(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), SyntaxError> {
        if self.consume_byte(expected) {
            Ok(())
        } else {
            Err(self.error(format!("expected byte {:?}", expected as char)))
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.cursor).copied()
    }

    fn next_byte(&mut self) -> Option<u8> {
        let byte = self.peek_byte()?;
        self.cursor += 1;
        Some(byte)
    }

    fn is_eof(&self) -> bool {
        self.cursor >= self.input.len()
    }

    fn error(&self, message: impl Into<String>) -> SyntaxError {
        SyntaxError::new(
            String::new(),
            Some(SourceSpan::new(self.cursor, self.cursor)),
            message,
        )
    }
}

fn utf8_width(byte: u8) -> Option<usize> {
    match byte {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}
