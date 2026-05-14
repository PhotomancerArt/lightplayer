//! Slot-native streaming reader and writer foundation.
//!
//! Syntax sources emit shape-agnostic events. [`SlotReader`] layers domain
//! construction helpers on top without materializing a generic syntax tree.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use base64::Engine;
use lpc_model::SlotShapeRegistry;
use toml::Value;

use crate::json::json_write::JsonWrite;
use crate::json::json_writer::{JsonArray, JsonObject, JsonValue, JsonWriter, JsonWriterError};
use crate::json::streaming_base64::write_base64_value;

const STRING_CHUNK_SIZE: usize = 1024;

/// Byte span in the source syntax input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Shape-agnostic syntax event.
#[derive(Clone, Debug, PartialEq)]
pub enum SyntaxEvent {
    StartObject {
        span: Option<SourceSpan>,
    },
    Prop {
        name: String,
        span: Option<SourceSpan>,
    },
    EndObject {
        span: Option<SourceSpan>,
    },
    StartArray {
        span: Option<SourceSpan>,
    },
    EndArray {
        span: Option<SourceSpan>,
    },
    StringChunk {
        text: String,
        is_last: bool,
        span: Option<SourceSpan>,
    },
    Number {
        text: String,
        span: Option<SourceSpan>,
    },
    Bool {
        value: bool,
        span: Option<SourceSpan>,
    },
    Null {
        span: Option<SourceSpan>,
    },
}

impl SyntaxEvent {
    fn span(&self) -> Option<SourceSpan> {
        match self {
            Self::StartObject { span }
            | Self::Prop { span, .. }
            | Self::EndObject { span }
            | Self::StartArray { span }
            | Self::EndArray { span }
            | Self::StringChunk { span, .. }
            | Self::Number { span, .. }
            | Self::Bool { span, .. }
            | Self::Null { span } => *span,
        }
    }
}

/// Pull-based source for syntax events.
pub trait SyntaxEventSource {
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError>;
}

/// Error returned by syntax readers and adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    path: String,
    span: Option<SourceSpan>,
    message: String,
}

impl SyntaxError {
    fn new(path: impl Into<String>, span: Option<SourceSpan>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            span,
            message: message.into(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn span(&self) -> Option<SourceSpan> {
        self.span
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

/// TOML value syntax source.
pub struct TomlSyntaxSource<'a> {
    stack: Vec<TomlFrame<'a>>,
    pending: Vec<SyntaxEvent>,
    started: bool,
    root: &'a Value,
}

impl<'a> TomlSyntaxSource<'a> {
    pub fn new(value: &'a Value) -> Result<Self, SyntaxError> {
        Ok(Self {
            stack: Vec::new(),
            pending: Vec::new(),
            started: false,
            root: value,
        })
    }
}

impl SyntaxEventSource for TomlSyntaxSource<'_> {
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError> {
        if let Some(event) = self.pending.pop() {
            return Ok(Some(event));
        }

        if !self.started {
            self.started = true;
            return self.emit_value(self.root).map(Some);
        }

        let Some(frame) = self.stack.pop() else {
            return Ok(None);
        };

        match frame {
            TomlFrame::Array { values, index } => {
                if index >= values.len() {
                    Ok(Some(SyntaxEvent::EndArray { span: None }))
                } else {
                    self.stack.push(TomlFrame::Array {
                        values,
                        index: index + 1,
                    });
                    self.emit_value(&values[index]).map(Some)
                }
            }
            TomlFrame::Object { entries, index } => {
                if index >= entries.len() {
                    Ok(Some(SyntaxEvent::EndObject { span: None }))
                } else {
                    let (name, value) = entries[index];
                    self.stack.push(TomlFrame::Object {
                        entries,
                        index: index + 1,
                    });
                    let event = self.event_for_value(value)?;
                    self.pending.push(event);
                    Ok(Some(SyntaxEvent::Prop {
                        name: name.clone(),
                        span: None,
                    }))
                }
            }
        }
    }
}

/// Streaming semantic reader over a syntax event source.
pub struct SlotReader<'a, S>
where
    S: SyntaxEventSource,
{
    source: S,
    registry: &'a SlotShapeRegistry,
    path: String,
    replay: Option<SyntaxEvent>,
}

impl<'a, S> SlotReader<'a, S>
where
    S: SyntaxEventSource,
{
    pub fn new(source: S, registry: &'a SlotShapeRegistry) -> Self {
        Self {
            source,
            registry,
            path: String::new(),
            replay: None,
        }
    }

    pub fn registry(&self) -> &'a SlotShapeRegistry {
        self.registry
    }

    pub fn start_object(&mut self) -> Result<(), SyntaxError> {
        let event = self.next_event()?;
        match event {
            Some(SyntaxEvent::StartObject { .. }) => Ok(()),
            Some(event) => Err(self.error_at(event.span(), "expected object")),
            None => Err(self.error("expected object, found end of input")),
        }
    }

    pub fn object(&mut self) -> Result<ObjectReader<'_, 'a, S>, SyntaxError> {
        self.start_object()?;
        Ok(ObjectReader { reader: self })
    }

    pub fn start_array(&mut self) -> Result<(), SyntaxError> {
        let event = self.next_event()?;
        match event {
            Some(SyntaxEvent::StartArray { .. }) => Ok(()),
            Some(event) => Err(self.error_at(event.span(), "expected array")),
            None => Err(self.error("expected array, found end of input")),
        }
    }

    pub fn array(&mut self) -> Result<ArrayReader<'_, 'a, S>, SyntaxError> {
        self.start_array()?;
        Ok(ArrayReader {
            reader: self,
            index: 0,
        })
    }

    pub fn expect_discriminator(
        &mut self,
        name: &str,
    ) -> Result<ValueReader<'_, 'a, S>, SyntaxError> {
        let event = self.next_event()?;
        match event {
            Some(SyntaxEvent::Prop { name: actual, span }) if actual == name => {
                Ok(ValueReader { reader: self, span })
            }
            Some(SyntaxEvent::Prop { name: actual, span }) => Err(self.error_at(
                span,
                format!("expected discriminator `{name}`, found property `{actual}`"),
            )),
            Some(event) => {
                Err(self.error_at(event.span(), format!("expected discriminator `{name}`")))
            }
            None => Err(self.error(format!(
                "expected discriminator `{name}`, found end of input"
            ))),
        }
    }

    pub fn invalid_discriminator_value(
        &self,
        name: &str,
        actual: &str,
        expected: &[&str],
    ) -> SyntaxError {
        self.error(format!(
            "invalid discriminator `{name}`: {actual:?}. Expected one of: {}.",
            expected.join(", ")
        ))
    }

    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError> {
        if let Some(event) = self.replay.take() {
            Ok(Some(event))
        } else {
            self.source.next_event()
        }
    }

    fn push_back(&mut self, event: SyntaxEvent) {
        debug_assert!(self.replay.is_none());
        self.replay = Some(event);
    }

    fn skip_value(&mut self) -> Result<(), SyntaxError> {
        let Some(event) = self.next_event()? else {
            return Err(self.error("expected value to skip, found end of input"));
        };
        match event {
            SyntaxEvent::StartObject { .. } => self.skip_nested_object(),
            SyntaxEvent::StartArray { .. } => self.skip_nested_array(),
            SyntaxEvent::StringChunk { is_last, .. } => {
                if !is_last {
                    self.finish_string_chunks()?;
                }
                Ok(())
            }
            SyntaxEvent::Number { .. } | SyntaxEvent::Bool { .. } | SyntaxEvent::Null { .. } => {
                Ok(())
            }
            SyntaxEvent::Prop { span, .. }
            | SyntaxEvent::EndObject { span }
            | SyntaxEvent::EndArray { span } => Err(self.error_at(span, "expected value to skip")),
        }
    }

    fn finish_string_chunks(&mut self) -> Result<String, SyntaxError> {
        let mut value = String::new();
        loop {
            match self.next_event()? {
                Some(SyntaxEvent::StringChunk { text, is_last, .. }) => {
                    value.push_str(&text);
                    if is_last {
                        return Ok(value);
                    }
                }
                Some(event) => {
                    return Err(self.error_at(event.span(), "expected string chunk"));
                }
                None => return Err(self.error("unterminated string chunks")),
            }
        }
    }

    fn skip_nested_object(&mut self) -> Result<(), SyntaxError> {
        loop {
            match self.next_event()? {
                Some(SyntaxEvent::EndObject { .. }) => return Ok(()),
                Some(SyntaxEvent::Prop { .. }) => self.skip_value()?,
                Some(event) => {
                    return Err(self.error_at(event.span(), "expected object property"));
                }
                None => return Err(self.error("unterminated object while skipping value")),
            }
        }
    }

    fn skip_nested_array(&mut self) -> Result<(), SyntaxError> {
        loop {
            match self.next_event()? {
                Some(SyntaxEvent::EndArray { .. }) => return Ok(()),
                Some(event) => {
                    self.push_back(event);
                    self.skip_value()?;
                }
                None => return Err(self.error("unterminated array while skipping value")),
            }
        }
    }

    fn error(&self, message: impl Into<String>) -> SyntaxError {
        SyntaxError::new(self.path.clone(), None, message)
    }

    fn error_at(&self, span: Option<SourceSpan>, message: impl Into<String>) -> SyntaxError {
        SyntaxError::new(self.path.clone(), span, message)
    }
}

/// Streaming object scanner.
pub struct ObjectReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    reader: &'r mut SlotReader<'a, S>,
}

impl<'r, 'a, S> ObjectReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    pub fn next_prop(&mut self) -> Result<Option<PropReader<'_, 'a, S>>, SyntaxError> {
        match self.reader.next_event()? {
            Some(SyntaxEvent::Prop { name, span }) => {
                let next_path = join_prop_path(&self.reader.path, &name);
                let previous_path = core::mem::replace(&mut self.reader.path, next_path);
                Ok(Some(PropReader {
                    reader: self.reader,
                    name,
                    span,
                    previous_path,
                    consumed: false,
                }))
            }
            Some(SyntaxEvent::EndObject { .. }) => Ok(None),
            Some(event) => Err(self
                .reader
                .error_at(event.span(), "expected object property or end of object")),
            None => Err(self.reader.error("unterminated object")),
        }
    }

    pub fn finish(self) -> Result<(), SyntaxError> {
        Ok(())
    }
}

/// One object property and its value cursor.
pub struct PropReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    reader: &'r mut SlotReader<'a, S>,
    name: String,
    span: Option<SourceSpan>,
    previous_path: String,
    consumed: bool,
}

impl<'r, 'a, S> PropReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&mut self) -> ValueReader<'_, 'a, S> {
        self.consumed = true;
        ValueReader {
            reader: self.reader,
            span: self.span,
        }
    }

    pub fn unknown_field(&self, name: &str, expected: &[&str]) -> SyntaxError {
        self.reader.error_at(
            self.span,
            format!(
                "unknown field {name:?}. Expected one of: {}.",
                expected.join(", ")
            ),
        )
    }
}

impl<S> Drop for PropReader<'_, '_, S>
where
    S: SyntaxEventSource,
{
    fn drop(&mut self) {
        self.reader.path = core::mem::take(&mut self.previous_path);
        if !self.consumed {
            let _ = self.reader.skip_value();
        }
    }
}

/// Streaming array scanner.
pub struct ArrayReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    reader: &'r mut SlotReader<'a, S>,
    index: usize,
}

impl<'r, 'a, S> ArrayReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    pub fn next_item(&mut self) -> Result<Option<ValueReader<'_, 'a, S>>, SyntaxError> {
        match self.reader.next_event()? {
            Some(SyntaxEvent::EndArray { .. }) => Ok(None),
            Some(event) => {
                self.reader.push_back(event);
                let index = self.index;
                self.index += 1;
                self.reader.path = format!("{}[{index}]", self.reader.path);
                Ok(Some(ValueReader {
                    reader: self.reader,
                    span: None,
                }))
            }
            None => Err(self.reader.error("unterminated array")),
        }
    }
}

/// One typed value read from the stream.
pub struct ValueReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    reader: &'r mut SlotReader<'a, S>,
    span: Option<SourceSpan>,
}

impl<'r, 'a, S> ValueReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    pub fn object(self) -> Result<ObjectReader<'r, 'a, S>, SyntaxError> {
        self.reader.object()
    }

    pub fn array(self) -> Result<ArrayReader<'r, 'a, S>, SyntaxError> {
        self.reader.array()
    }

    pub fn f32(self) -> Result<f32, SyntaxError> {
        let span = self.span;
        let path = self.reader.path.clone();
        let text = read_number_text(self.reader, span, "f32")?;
        text.parse()
            .map_err(|_| SyntaxError::new(path, span, "expected f32"))
    }

    pub fn u32(self) -> Result<u32, SyntaxError> {
        let span = self.span;
        let path = self.reader.path.clone();
        let text = read_number_text(self.reader, span, "u32")?;
        text.parse()
            .map_err(|_| SyntaxError::new(path, span, "expected u32"))
    }

    pub fn bool(self) -> Result<bool, SyntaxError> {
        match self.reader.next_event()? {
            Some(SyntaxEvent::Bool { value, .. }) => Ok(value),
            Some(event) => Err(self.reader.error_at(event.span(), "expected bool")),
            None => Err(self.reader.error("expected bool, found end of input")),
        }
    }

    pub fn string(self) -> Result<String, SyntaxError> {
        let Some(event) = self.reader.next_event()? else {
            return Err(self.reader.error("expected string, found end of input"));
        };
        match event {
            SyntaxEvent::StringChunk { text, is_last, .. } => {
                if is_last {
                    return Ok(text);
                }
                let mut value = text;
                value.push_str(&self.reader.finish_string_chunks()?);
                Ok(value)
            }
            event => Err(self.reader.error_at(event.span(), "expected string")),
        }
    }

    pub fn binary_base64_tuple(self) -> Result<Vec<u8>, SyntaxError> {
        let mut array = self.array()?;
        let Some(length_item) = array.next_item()? else {
            return Err(array.reader.error("expected binary tuple byte length"));
        };
        let expected_len = length_item.u32()? as usize;
        let Some(payload_item) = array.next_item()? else {
            return Err(array.reader.error("expected binary tuple base64 payload"));
        };
        let encoded = payload_item.string()?;
        if array.next_item()?.is_some() {
            return Err(array.reader.error("expected binary tuple [len, base64]"));
        }

        let mut bytes = Vec::with_capacity(expected_len);
        base64::engine::general_purpose::STANDARD
            .decode_vec(encoded.as_bytes(), &mut bytes)
            .map_err(|_| array.reader.error("invalid base64 payload"))?;
        if bytes.len() != expected_len {
            return Err(array.reader.error(format!(
                "base64 payload decoded to {} bytes, expected {expected_len}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }
}

fn read_number_text<S>(
    reader: &mut SlotReader<'_, S>,
    span: Option<SourceSpan>,
    expected: &str,
) -> Result<String, SyntaxError>
where
    S: SyntaxEventSource,
{
    match reader.next_event()? {
        Some(SyntaxEvent::Number { text, .. }) => Ok(text),
        Some(event) => Err(reader.error_at(event.span(), format!("expected {expected} number"))),
        None => Err(reader.error_at(
            span,
            format!("expected {expected} number, found end of input"),
        )),
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

enum TomlFrame<'a> {
    Array {
        values: &'a [Value],
        index: usize,
    },
    Object {
        entries: Vec<(&'a String, &'a Value)>,
        index: usize,
    },
}

impl<'a> TomlSyntaxSource<'a> {
    fn emit_value(&mut self, value: &'a Value) -> Result<SyntaxEvent, SyntaxError> {
        match value {
            Value::String(value) => Ok(self.first_string_chunk(value.clone(), None)),
            Value::Integer(value) => Ok(SyntaxEvent::Number {
                text: value.to_string(),
                span: None,
            }),
            Value::Float(value) => Ok(SyntaxEvent::Number {
                text: value.to_string(),
                span: None,
            }),
            Value::Boolean(value) => Ok(SyntaxEvent::Bool {
                value: *value,
                span: None,
            }),
            Value::Array(values) => {
                self.stack.push(TomlFrame::Array { values, index: 0 });
                Ok(SyntaxEvent::StartArray { span: None })
            }
            Value::Table(table) => {
                self.stack.push(TomlFrame::Object {
                    entries: table.iter().collect(),
                    index: 0,
                });
                Ok(SyntaxEvent::StartObject { span: None })
            }
            Value::Datetime(_) => Err(SyntaxError::new(
                String::new(),
                None,
                "TOML datetime is not supported",
            )),
        }
    }

    fn event_for_value(&mut self, value: &'a Value) -> Result<SyntaxEvent, SyntaxError> {
        self.emit_value(value)
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
}

fn split_string_events(value: &str, span: Option<SourceSpan>) -> Vec<SyntaxEvent> {
    if value.is_empty() {
        return vec![SyntaxEvent::StringChunk {
            text: String::new(),
            is_last: true,
            span,
        }];
    }

    let mut events = Vec::new();
    let mut start = 0;
    while start < value.len() {
        let mut end = (start + STRING_CHUNK_SIZE).min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        events.push(SyntaxEvent::StringChunk {
            text: value[start..end].to_string(),
            is_last: end == value.len(),
            span,
        });
        start = end;
    }
    events
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
    fn json_source_emits_syntax_events_incrementally() {
        let mut source = JsonSyntaxSource::new(r#"{"pin":18,"name":"abc","ok":true}"#).unwrap();

        assert_eq!(source.next_event().unwrap(), Some(start_object()));
        assert_eq!(
            source.next_event().unwrap(),
            Some(SyntaxEvent::Prop {
                name: "pin".to_string(),
                span: Some(SourceSpan::new(1, 6)),
            })
        );
        assert_eq!(
            source.next_event().unwrap(),
            Some(SyntaxEvent::Number {
                text: "18".to_string(),
                span: Some(SourceSpan::new(7, 9)),
            })
        );
    }

    #[test]
    fn slot_reader_scans_typed_properties_from_json() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"brightness":0.25,"pin":18,"name":"main"}"#).unwrap(),
            &registry,
        );

        let mut object = reader.object().unwrap();
        let mut brightness = None;
        let mut pin = None;
        let mut name = None;

        while let Some(mut prop) = object.next_prop().unwrap() {
            match prop.name() {
                "brightness" => brightness = Some(prop.value().f32().unwrap()),
                "pin" => pin = Some(prop.value().u32().unwrap()),
                "name" => name = Some(prop.value().string().unwrap()),
                other => panic!(
                    "{}",
                    prop.unknown_field(other, &["brightness", "pin", "name"])
                ),
            }
        }

        assert_eq!(brightness, Some(0.25));
        assert_eq!(pin, Some(18));
        assert_eq!(name.as_deref(), Some("main"));
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
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(TomlSyntaxSource::new(&value).unwrap(), &registry);
        let mut object = reader.object().unwrap();
        let mut seen = Vec::new();

        while let Some(mut prop) = object.next_prop().unwrap() {
            seen.push(prop.name().to_string());
            prop.value().skip_value().unwrap();
        }

        assert_eq!(seen, vec!["brightness", "pin", "name"]);
    }

    #[test]
    fn strings_are_chunked_and_reassembled() {
        let long = "x".repeat(STRING_CHUNK_SIZE + 7);
        let registry = SlotShapeRegistry::default();
        let json = format!(r#"{{"long":"{long}"}}"#);
        let mut reader = SlotReader::new(JsonSyntaxSource::new(&json).unwrap(), &registry);
        let mut object = reader.object().unwrap();
        let mut prop = object.next_prop().unwrap().unwrap();

        assert_eq!(prop.value().string().unwrap(), long);
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

        let registry = SlotShapeRegistry::default();
        let json = core::str::from_utf8(&out).unwrap();
        let mut reader = SlotReader::new(JsonSyntaxSource::new(json).unwrap(), &registry);
        let mut object = reader.object().unwrap();
        let mut prop = object.next_prop().unwrap().unwrap();

        assert_eq!(
            prop.value().binary_base64_tuple().unwrap(),
            vec![1, 2, 3, 253, 254, 255]
        );
    }

    #[test]
    fn discriminator_reports_expected_values() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"kind":"Blark12","pin":18}"#).unwrap(),
            &registry,
        );
        reader.start_object().unwrap();
        let kind = reader
            .expect_discriminator("kind")
            .unwrap()
            .string()
            .unwrap();
        let error = reader.invalid_discriminator_value("kind", &kind, &["TextureDef", "OutputDef"]);

        assert!(error.message().contains("Blark12"));
        assert!(error.message().contains("TextureDef"));
        assert!(error.message().contains("OutputDef"));
    }

    fn start_object() -> SyntaxEvent {
        SyntaxEvent::StartObject {
            span: Some(SourceSpan::new(0, 1)),
        }
    }
}

impl<'r, 'a, S> ValueReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    pub fn skip_value(self) -> Result<(), SyntaxError> {
        self.reader.skip_value()
    }
}
