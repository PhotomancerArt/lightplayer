use alloc::string::{String, ToString};
use alloc::vec::Vec;

use toml::Value;

use super::syntax::{SourceSpan, SyntaxError, SyntaxEvent, SyntaxEventSource, split_string_events};

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
                let mut entries: Vec<_> = table.iter().collect();
                entries.sort_by(
                    |(left, _), (right, _)| match (left.as_str(), right.as_str()) {
                        ("kind", "kind") => core::cmp::Ordering::Equal,
                        ("kind", _) => core::cmp::Ordering::Less,
                        (_, "kind") => core::cmp::Ordering::Greater,
                        _ => core::cmp::Ordering::Equal,
                    },
                );
                self.stack.push(TomlFrame::Object { entries, index: 0 });
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    use crate::SlotShapeRegistry;
    use crate::slot_codec::SlotReader;

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
    fn toml_source_emits_kind_before_payload_fields() {
        let mut table = toml::Table::new();
        table.insert("bindings".to_string(), Value::Table(toml::Table::new()));
        table.insert(
            "kind".to_string(),
            Value::String("ComputeShader".to_string()),
        );
        let mut source = toml::Table::new();
        source.insert(
            "path".to_string(),
            Value::String("compute.glsl".to_string()),
        );
        table.insert("source".to_string(), Value::Table(source));
        let value = Value::Table(table);
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(TomlSyntaxSource::new(&value).unwrap(), &registry);
        let mut object = reader.object().unwrap();
        let mut seen = Vec::new();

        while let Some(mut prop) = object.next_prop().unwrap() {
            seen.push(prop.name().to_string());
            prop.value().skip_value().unwrap();
        }

        assert_eq!(seen[0], "kind");
    }
}
