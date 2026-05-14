//! Slot-native syntax reader and writer foundation.
//!
//! This module separates syntax parsing from slot/domain construction. The
//! first implementation can build a small syntax tree from events; generated
//! construction can later consume the same event vocabulary more directly when
//! large wire messages need tighter peak-memory behavior.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use base64::Engine;
use lpc_model::SlotShapeRegistry;
use toml::Value;

use crate::json::json_write::JsonWrite;
use crate::json::json_writer::{JsonArray, JsonObject, JsonValue, JsonWriter, JsonWriterError};
use crate::json::streaming_base64::write_base64_value;

const STRING_CHUNK_SIZE: usize = 1024;

/// Shape-agnostic syntax event.
#[derive(Clone, Debug, PartialEq)]
pub enum SyntaxEvent {
    StartObject,
    Prop(String),
    EndObject,
    StartArray,
    EndArray,
    StringChunk { text: String, is_last: bool },
    Number(String),
    Bool(bool),
    Null,
}

/// Pull-based source for syntax events.
pub trait SyntaxEventSource {
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError>;
}

/// Error returned by syntax readers and adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    path: String,
    message: String,
}

impl SyntaxError {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl core::error::Error for SyntaxError {}

/// Direct JSON text syntax source.
pub struct JsonSyntaxSource {
    events: Vec<SyntaxEvent>,
    index: usize,
}

impl JsonSyntaxSource {
    pub fn new(input: &str) -> Result<Self, SyntaxError> {
        let mut parser = JsonParser {
            input,
            cursor: 0,
            events: Vec::new(),
        };
        parser.parse_value()?;
        parser.skip_ws();
        if !parser.is_eof() {
            return Err(parser.error("unexpected trailing JSON input"));
        }
        Ok(Self {
            events: parser.events,
            index: 0,
        })
    }
}

impl SyntaxEventSource for JsonSyntaxSource {
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError> {
        let event = self.events.get(self.index).cloned();
        self.index += usize::from(event.is_some());
        Ok(event)
    }
}

/// TOML value syntax source.
pub struct TomlSyntaxSource {
    events: Vec<SyntaxEvent>,
    index: usize,
}

impl TomlSyntaxSource {
    pub fn new(value: &Value) -> Result<Self, SyntaxError> {
        let mut events = Vec::new();
        push_toml_events(value, &mut events, "")?;
        Ok(Self { events, index: 0 })
    }
}

impl SyntaxEventSource for TomlSyntaxSource {
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError> {
        let event = self.events.get(self.index).cloned();
        self.index += usize::from(event.is_some());
        Ok(event)
    }
}

/// Temporary syntax tree built from syntax events.
#[derive(Clone, Debug, PartialEq)]
pub enum SyntaxNode {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<SyntaxNode>),
    Object(Vec<(String, SyntaxNode)>),
}

impl SyntaxNode {
    pub fn from_source(source: &mut impl SyntaxEventSource) -> Result<Self, SyntaxError> {
        let node = Self::read_node(source, "")?;
        if let Some(event) = source.next_event()? {
            return Err(SyntaxError::new(
                "",
                format!("unexpected trailing syntax event {event:?}"),
            ));
        }
        Ok(node)
    }

    fn read_node(source: &mut dyn SyntaxEventSource, path: &str) -> Result<Self, SyntaxError> {
        let Some(event) = source.next_event()? else {
            return Err(SyntaxError::new(path, "unexpected end of syntax events"));
        };
        match event {
            SyntaxEvent::StartObject => {
                let mut fields = Vec::new();
                loop {
                    let Some(event) = source.next_event()? else {
                        return Err(SyntaxError::new(path, "unterminated object"));
                    };
                    match event {
                        SyntaxEvent::Prop(name) => {
                            let child_path = join_prop_path(path, &name);
                            let value = Self::read_node(source, &child_path)?;
                            fields.push((name, value));
                        }
                        SyntaxEvent::EndObject => return Ok(Self::Object(fields)),
                        other => {
                            return Err(SyntaxError::new(
                                path,
                                format!("expected object property, got {other:?}"),
                            ));
                        }
                    }
                }
            }
            SyntaxEvent::StartArray => {
                let mut items = Vec::new();
                loop {
                    let Some(event) = source.next_event()? else {
                        return Err(SyntaxError::new(path, "unterminated array"));
                    };
                    if event == SyntaxEvent::EndArray {
                        return Ok(Self::Array(items));
                    }
                    let mut replay = ReplaySource::new(event, source);
                    let child_path = format!("{path}[{}]", items.len());
                    items.push(Self::read_node(&mut replay, &child_path)?);
                }
            }
            SyntaxEvent::StringChunk { text, is_last } => {
                let mut value = text;
                if !is_last {
                    loop {
                        let Some(event) = source.next_event()? else {
                            return Err(SyntaxError::new(path, "unterminated string chunks"));
                        };
                        match event {
                            SyntaxEvent::StringChunk { text, is_last } => {
                                value.push_str(&text);
                                if is_last {
                                    break;
                                }
                            }
                            other => {
                                return Err(SyntaxError::new(
                                    path,
                                    format!("expected string chunk, got {other:?}"),
                                ));
                            }
                        }
                    }
                }
                Ok(Self::String(value))
            }
            SyntaxEvent::Number(value) => Ok(Self::Number(value)),
            SyntaxEvent::Bool(value) => Ok(Self::Bool(value)),
            SyntaxEvent::Null => Ok(Self::Null),
            SyntaxEvent::Prop(_) | SyntaxEvent::EndObject | SyntaxEvent::EndArray => Err(
                SyntaxError::new(path, format!("unexpected syntax event {event:?}")),
            ),
        }
    }
}

/// Slot-aware typed reader over syntax data.
pub struct SlotReader<'a> {
    node: &'a SyntaxNode,
    registry: &'a SlotShapeRegistry,
    path: String,
}

impl<'a> SlotReader<'a> {
    pub fn new(node: &'a SyntaxNode, registry: &'a SlotShapeRegistry) -> Self {
        Self {
            node,
            registry,
            path: String::new(),
        }
    }

    pub fn registry(&self) -> &'a SlotShapeRegistry {
        self.registry
    }

    pub fn prop(&self, name: &str) -> Result<Self, SyntaxError> {
        let SyntaxNode::Object(fields) = self.node else {
            return Err(self.error("expected object"));
        };
        let child = fields
            .iter()
            .find(|(field_name, _)| field_name == name)
            .map(|(_, value)| value)
            .ok_or_else(|| self.error(format!("missing property {name:?}")))?;
        Ok(Self {
            node: child,
            registry: self.registry,
            path: join_prop_path(&self.path, name),
        })
    }

    pub fn optional_prop(&self, name: &str) -> Result<Option<Self>, SyntaxError> {
        let SyntaxNode::Object(fields) = self.node else {
            return Err(self.error("expected object"));
        };
        Ok(fields
            .iter()
            .find(|(field_name, _)| field_name == name)
            .map(|(_, value)| Self {
                node: value,
                registry: self.registry,
                path: join_prop_path(&self.path, name),
            }))
    }

    pub fn array_items(&self) -> Result<Vec<Self>, SyntaxError> {
        let SyntaxNode::Array(items) = self.node else {
            return Err(self.error("expected array"));
        };
        Ok(items
            .iter()
            .enumerate()
            .map(|(index, item)| Self {
                node: item,
                registry: self.registry,
                path: format!("{}[{index}]", self.path),
            })
            .collect())
    }

    pub fn f32(&self) -> Result<f32, SyntaxError> {
        match self.node {
            SyntaxNode::Number(value) => value.parse().map_err(|_| self.error("expected f32")),
            _ => Err(self.error("expected number")),
        }
    }

    pub fn u32(&self) -> Result<u32, SyntaxError> {
        match self.node {
            SyntaxNode::Number(value) => value.parse().map_err(|_| self.error("expected u32")),
            _ => Err(self.error("expected number")),
        }
    }

    pub fn bool(&self) -> Result<bool, SyntaxError> {
        match self.node {
            SyntaxNode::Bool(value) => Ok(*value),
            _ => Err(self.error("expected bool")),
        }
    }

    pub fn string(&self) -> Result<&'a str, SyntaxError> {
        match self.node {
            SyntaxNode::String(value) => Ok(value),
            _ => Err(self.error("expected string")),
        }
    }

    pub fn binary_base64_tuple(&self) -> Result<Vec<u8>, SyntaxError> {
        let SyntaxNode::Array(items) = self.node else {
            return Err(self.error("expected binary tuple array"));
        };
        if items.len() != 2 {
            return Err(self.error("expected binary tuple [len, base64]"));
        }
        let expected_len = match &items[0] {
            SyntaxNode::Number(value) => value
                .parse::<usize>()
                .map_err(|_| self.error("expected binary byte length"))?,
            _ => return Err(self.error("expected binary byte length")),
        };
        let encoded = match &items[1] {
            SyntaxNode::String(value) => value.as_str(),
            _ => return Err(self.error("expected binary base64 string")),
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| self.error("invalid base64 payload"))?;
        if bytes.len() != expected_len {
            return Err(self.error(format!(
                "base64 payload decoded to {} bytes, expected {expected_len}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    fn error(&self, message: impl Into<String>) -> SyntaxError {
        SyntaxError::new(self.path.clone(), message)
    }
}

/// Slot-native JSON writer facade.
pub struct SlotJsonWriter<W>
where
    W: JsonWrite,
{
    writer: JsonWriter<W>,
}

impl<W> SlotJsonWriter<W>
where
    W: JsonWrite,
{
    pub fn new(out: W) -> Self {
        Self {
            writer: JsonWriter::new(out),
        }
    }

    pub fn into_inner(self) -> W {
        self.writer.into_inner()
    }

    pub fn object(&mut self) -> Result<SlotJsonObject<'_, W>, JsonWriterError<W::Error>> {
        Ok(SlotJsonObject {
            object: self.writer.object()?,
        })
    }
}

pub struct SlotJsonObject<'a, W>
where
    W: JsonWrite,
{
    object: JsonObject<'a, W>,
}

impl<'a, W> SlotJsonObject<'a, W>
where
    W: JsonWrite,
{
    pub fn prop(&mut self, name: &str) -> Result<SlotJsonValue<'_, W>, JsonWriterError<W::Error>> {
        Ok(SlotJsonValue {
            value: self.object.prop(name)?,
        })
    }

    pub fn finish(self) -> Result<(), JsonWriterError<W::Error>> {
        self.object.finish()
    }
}

pub struct SlotJsonArray<'a, W>
where
    W: JsonWrite,
{
    array: JsonArray<'a, W>,
}

impl<'a, W> SlotJsonArray<'a, W>
where
    W: JsonWrite,
{
    pub fn item(&mut self) -> Result<SlotJsonValue<'_, W>, JsonWriterError<W::Error>> {
        Ok(SlotJsonValue {
            value: self.array.item()?,
        })
    }

    pub fn finish(self) -> Result<(), JsonWriterError<W::Error>> {
        self.array.finish()
    }
}

pub struct SlotJsonValue<'a, W>
where
    W: JsonWrite,
{
    value: JsonValue<'a, W>,
}

impl<'a, W> SlotJsonValue<'a, W>
where
    W: JsonWrite,
{
    pub fn object(self) -> Result<SlotJsonObject<'a, W>, JsonWriterError<W::Error>> {
        Ok(SlotJsonObject {
            object: self.value.object()?,
        })
    }

    pub fn array(self) -> Result<SlotJsonArray<'a, W>, JsonWriterError<W::Error>> {
        Ok(SlotJsonArray {
            array: self.value.array()?,
        })
    }

    pub fn f32(self, value: f32) -> Result<(), JsonWriterError<W::Error>> {
        self.value.serde(&value)
    }

    pub fn u32(self, value: u32) -> Result<(), JsonWriterError<W::Error>> {
        self.value.u64(u64::from(value))
    }

    pub fn bool(self, value: bool) -> Result<(), JsonWriterError<W::Error>> {
        self.value.bool(value)
    }

    pub fn string(self, value: &str) -> Result<(), JsonWriterError<W::Error>> {
        self.value.string(value)
    }

    pub fn binary_base64_tuple(self, bytes: &[u8]) -> Result<(), JsonWriterError<W::Error>> {
        let mut array = self.value.array()?;
        array.item()?.u64(bytes.len() as u64)?;
        write_base64_value(array.item()?, bytes)?;
        array.finish()
    }
}

struct JsonParser<'a> {
    input: &'a str,
    cursor: usize,
    events: Vec<SyntaxEvent>,
}

impl JsonParser<'_> {
    fn parse_value(&mut self) -> Result<(), SyntaxError> {
        self.skip_ws();
        match self.peek_byte() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => {
                let value = self.parse_string()?;
                push_string_events(&value, &mut self.events);
                Ok(())
            }
            Some(b't') => self.parse_literal("true", SyntaxEvent::Bool(true)),
            Some(b'f') => self.parse_literal("false", SyntaxEvent::Bool(false)),
            Some(b'n') => self.parse_literal("null", SyntaxEvent::Null),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(_) => Err(self.error("unexpected JSON value")),
            None => Err(self.error("unexpected end of JSON input")),
        }
    }

    fn parse_object(&mut self) -> Result<(), SyntaxError> {
        self.expect_byte(b'{')?;
        self.events.push(SyntaxEvent::StartObject);
        self.skip_ws();
        if self.consume_byte(b'}') {
            self.events.push(SyntaxEvent::EndObject);
            return Ok(());
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.events.push(SyntaxEvent::Prop(key));
            self.skip_ws();
            self.expect_byte(b':')?;
            self.parse_value()?;
            self.skip_ws();
            if self.consume_byte(b'}') {
                self.events.push(SyntaxEvent::EndObject);
                return Ok(());
            }
            self.expect_byte(b',')?;
        }
    }

    fn parse_array(&mut self) -> Result<(), SyntaxError> {
        self.expect_byte(b'[')?;
        self.events.push(SyntaxEvent::StartArray);
        self.skip_ws();
        if self.consume_byte(b']') {
            self.events.push(SyntaxEvent::EndArray);
            return Ok(());
        }
        loop {
            self.parse_value()?;
            self.skip_ws();
            if self.consume_byte(b']') {
                self.events.push(SyntaxEvent::EndArray);
                return Ok(());
            }
            self.expect_byte(b',')?;
        }
    }

    fn parse_string(&mut self) -> Result<String, SyntaxError> {
        self.expect_byte(b'"')?;
        let mut output = String::new();
        loop {
            let Some(byte) = self.next_byte() else {
                return Err(self.error("unterminated JSON string"));
            };
            match byte {
                b'"' => return Ok(output),
                b'\\' => self.parse_escape(&mut output)?,
                0x00..=0x1f => return Err(self.error("control character in JSON string")),
                byte if byte < 0x80 => output.push(byte as char),
                byte => {
                    let start = self.cursor - 1;
                    let width = utf8_width(byte)
                        .ok_or_else(|| self.error("invalid UTF-8 start byte in JSON string"))?;
                    let end = start + width;
                    let slice = self
                        .input
                        .get(start..end)
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

    fn parse_number(&mut self) -> Result<(), SyntaxError> {
        let start = self.cursor;
        if self.consume_byte(b'-') {
            if !self.consume_digits() {
                return Err(self.error("expected digits after minus sign"));
            }
        } else if !self.consume_digits() {
            return Err(self.error("expected digits in JSON number"));
        }
        if self.consume_byte(b'.') {
            if !self.consume_digits() {
                return Err(self.error("expected digits after decimal point"));
            }
        }
        if self.consume_byte(b'e') || self.consume_byte(b'E') {
            let _ = self.consume_byte(b'+') || self.consume_byte(b'-');
            if !self.consume_digits() {
                return Err(self.error("expected exponent digits"));
            }
        }
        let value = self.input[start..self.cursor].to_string();
        self.events.push(SyntaxEvent::Number(value));
        Ok(())
    }

    fn parse_literal(&mut self, literal: &str, event: SyntaxEvent) -> Result<(), SyntaxError> {
        if self.input[self.cursor..].starts_with(literal) {
            self.cursor += literal.len();
            self.events.push(event);
            Ok(())
        } else {
            Err(self.error(format!("expected JSON literal {literal}")))
        }
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
        SyntaxError::new(format!("@{}", self.cursor), message)
    }
}

struct ReplaySource<'a, S>
where
    S: SyntaxEventSource + ?Sized,
{
    first: Option<SyntaxEvent>,
    inner: &'a mut S,
}

impl<'a, S> ReplaySource<'a, S>
where
    S: SyntaxEventSource + ?Sized,
{
    fn new(first: SyntaxEvent, inner: &'a mut S) -> Self {
        Self {
            first: Some(first),
            inner,
        }
    }
}

impl<S> SyntaxEventSource for ReplaySource<'_, S>
where
    S: SyntaxEventSource + ?Sized,
{
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError> {
        if let Some(event) = self.first.take() {
            Ok(Some(event))
        } else {
            self.inner.next_event()
        }
    }
}

fn push_toml_events(
    value: &Value,
    events: &mut Vec<SyntaxEvent>,
    path: &str,
) -> Result<(), SyntaxError> {
    match value {
        Value::String(value) => push_string_events(value, events),
        Value::Integer(value) => events.push(SyntaxEvent::Number(value.to_string())),
        Value::Float(value) => events.push(SyntaxEvent::Number(value.to_string())),
        Value::Boolean(value) => events.push(SyntaxEvent::Bool(*value)),
        Value::Array(values) => {
            events.push(SyntaxEvent::StartArray);
            for (index, value) in values.iter().enumerate() {
                push_toml_events(value, events, &format!("{path}[{index}]"))?;
            }
            events.push(SyntaxEvent::EndArray);
        }
        Value::Table(table) => {
            events.push(SyntaxEvent::StartObject);
            for (key, value) in table {
                events.push(SyntaxEvent::Prop(key.clone()));
                push_toml_events(value, events, &join_prop_path(path, key))?;
            }
            events.push(SyntaxEvent::EndObject);
        }
        Value::Datetime(_) => {
            return Err(SyntaxError::new(path, "TOML datetime is not supported"));
        }
    }
    Ok(())
}

fn push_string_events(value: &str, events: &mut Vec<SyntaxEvent>) {
    if value.is_empty() {
        events.push(SyntaxEvent::StringChunk {
            text: String::new(),
            is_last: true,
        });
        return;
    }

    let mut start = 0;
    while start < value.len() {
        let mut end = (start + STRING_CHUNK_SIZE).min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        events.push(SyntaxEvent::StringChunk {
            text: value[start..end].to_string(),
            is_last: end == value.len(),
        });
        start = end;
    }
}

fn join_prop_path(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}.{name}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::SlotShapeRegistry;

    #[test]
    fn json_source_emits_syntax_events() {
        let mut source = JsonSyntaxSource::new(r#"{"pin":18,"name":"abc","ok":true}"#).unwrap();
        let mut events = Vec::new();
        while let Some(event) = source.next_event().unwrap() {
            events.push(event);
        }

        assert_eq!(
            events,
            vec![
                SyntaxEvent::StartObject,
                SyntaxEvent::Prop("pin".to_string()),
                SyntaxEvent::Number("18".to_string()),
                SyntaxEvent::Prop("name".to_string()),
                SyntaxEvent::StringChunk {
                    text: "abc".to_string(),
                    is_last: true,
                },
                SyntaxEvent::Prop("ok".to_string()),
                SyntaxEvent::Bool(true),
                SyntaxEvent::EndObject,
            ]
        );
    }

    #[test]
    fn slot_reader_reads_typed_properties_from_json() {
        let mut source =
            JsonSyntaxSource::new(r#"{"brightness":0.25,"pin":18,"name":"main"}"#).unwrap();
        let node = SyntaxNode::from_source(&mut source).unwrap();
        let registry = SlotShapeRegistry::default();
        let reader = SlotReader::new(&node, &registry);

        assert_eq!(reader.prop("brightness").unwrap().f32().unwrap(), 0.25);
        assert_eq!(reader.prop("pin").unwrap().u32().unwrap(), 18);
        assert_eq!(reader.prop("name").unwrap().string().unwrap(), "main");
    }

    #[test]
    fn toml_source_uses_same_reader_semantics() {
        let value: Value = toml::from_str(
            r#"
brightness = 0.5
pin = 19
name = "aux"
"#,
        )
        .unwrap();
        let mut source = TomlSyntaxSource::new(&value).unwrap();
        let node = SyntaxNode::from_source(&mut source).unwrap();
        let registry = SlotShapeRegistry::default();
        let reader = SlotReader::new(&node, &registry);

        assert_eq!(reader.prop("brightness").unwrap().f32().unwrap(), 0.5);
        assert_eq!(reader.prop("pin").unwrap().u32().unwrap(), 19);
        assert_eq!(reader.prop("name").unwrap().string().unwrap(), "aux");
    }

    #[test]
    fn strings_are_chunked_and_reassembled() {
        let long = "x".repeat(STRING_CHUNK_SIZE + 7);
        let mut source = JsonSyntaxSource::new(&format!(r#"{{"long":"{long}"}}"#)).unwrap();
        let node = SyntaxNode::from_source(&mut source).unwrap();
        let registry = SlotShapeRegistry::default();
        let reader = SlotReader::new(&node, &registry);

        assert_eq!(reader.prop("long").unwrap().string().unwrap(), long);
    }

    #[test]
    fn binary_base64_tuple_decodes_length_checked_bytes() {
        let mut out = Vec::new();
        let mut writer = SlotJsonWriter::new(&mut out);
        let mut object = writer.object().unwrap();
        object
            .prop("payload")
            .unwrap()
            .binary_base64_tuple(&[1, 2, 3, 253, 254, 255])
            .unwrap();
        object.finish().unwrap();

        let json = core::str::from_utf8(&out).unwrap();
        let mut source = JsonSyntaxSource::new(json).unwrap();
        let node = SyntaxNode::from_source(&mut source).unwrap();
        let registry = SlotShapeRegistry::default();
        let reader = SlotReader::new(&node, &registry);

        assert_eq!(
            reader
                .prop("payload")
                .unwrap()
                .binary_base64_tuple()
                .unwrap(),
            vec![1, 2, 3, 253, 254, 255]
        );
    }
}
