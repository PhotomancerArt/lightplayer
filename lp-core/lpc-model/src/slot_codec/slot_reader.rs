use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use base64::Engine;

use crate::SlotShapeRegistry;

use super::syntax::{SourceSpan, SyntaxError, SyntaxEvent, SyntaxEventSource};

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
        Ok(ObjectReader {
            reader: self,
            ended: false,
        })
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
        let path = self.path.clone();
        Ok(ArrayReader {
            reader: self,
            index: 0,
            path,
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

    pub fn missing_required_field(&self, name: &str) -> SyntaxError {
        self.error(format!("missing required field `{name}`"))
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
                Some(event) => return Err(self.error_at(event.span(), "expected string chunk")),
                None => return Err(self.error("unterminated string chunks")),
            }
        }
    }

    fn skip_nested_object(&mut self) -> Result<(), SyntaxError> {
        loop {
            match self.next_event()? {
                Some(SyntaxEvent::EndObject { .. }) => return Ok(()),
                Some(SyntaxEvent::Prop { .. }) => self.skip_value()?,
                Some(event) => return Err(self.error_at(event.span(), "expected object property")),
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
    ended: bool,
}

impl<'r, 'a, S> ObjectReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    pub fn next_prop(&mut self) -> Result<Option<PropReader<'_, 'a, S>>, SyntaxError> {
        if self.ended {
            return Ok(None);
        }

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
            Some(SyntaxEvent::EndObject { .. }) => {
                self.ended = true;
                Ok(None)
            }
            Some(event) => Err(self
                .reader
                .error_at(event.span(), "expected object property or end of object")),
            None => Err(self.reader.error("unterminated object")),
        }
    }

    pub fn finish(mut self) -> Result<(), SyntaxError> {
        if self.next_prop()?.is_some() {
            return Err(self.reader.error("expected end of object"));
        }
        Ok(())
    }

    pub fn expect_discriminator(
        &mut self,
        name: &str,
        expected: &[&str],
    ) -> Result<String, SyntaxError> {
        let Some(mut prop) = self.next_prop()? else {
            return Err(self.missing_required_field(name));
        };
        let actual_name = prop.name().to_string();
        if actual_name != name {
            return Err(prop.unknown_field(&actual_name, &[name]));
        }
        let actual = prop.value().string()?;
        drop(prop);

        if expected.contains(&actual.as_str()) {
            Ok(actual)
        } else {
            Err(self.invalid_discriminator_value(name, &actual, expected))
        }
    }

    pub fn missing_required_field(&self, name: &str) -> SyntaxError {
        self.reader.missing_required_field(name)
    }

    pub fn invalid_discriminator_value(
        &self,
        name: &str,
        actual: &str,
        expected: &[&str],
    ) -> SyntaxError {
        self.reader
            .invalid_discriminator_value(name, actual, expected)
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
    path: String,
}

impl<'r, 'a, S> ArrayReader<'r, 'a, S>
where
    S: SyntaxEventSource,
{
    pub fn next_item(&mut self) -> Result<Option<ValueReader<'_, 'a, S>>, SyntaxError> {
        match self.reader.next_event()? {
            Some(SyntaxEvent::EndArray { .. }) => {
                self.reader.path = self.path.clone();
                Ok(None)
            }
            Some(event) => {
                self.reader.push_back(event);
                let index = self.index;
                self.index += 1;
                self.reader.path = format!("{}[{index}]", self.path);
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

    pub fn f32_array<const N: usize>(self) -> Result<[f32; N], SyntaxError> {
        let mut array = self.array()?;
        let mut values = [0.0; N];
        let mut count = 0;

        while let Some(item) = array.next_item()? {
            if count >= N {
                item.skip_value()?;
                return Err(array
                    .reader
                    .error(format!("expected array of {N} f32 values, found more")));
            }
            values[count] = item.f32()?;
            count += 1;
        }

        if count != N {
            return Err(array
                .reader
                .error(format!("expected array of {N} f32 values, found {count}")));
        }

        Ok(values)
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

    pub fn skip_value(self) -> Result<(), SyntaxError> {
        self.reader.skip_value()
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

fn join_prop_path(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.into()
    } else {
        format!("{path}.{name}")
    }
}
