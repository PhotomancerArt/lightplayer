//! Slot-native streaming reader foundation.
//!
//! Syntax sources emit shape-agnostic events. [`SlotReader`] layers domain
//! construction helpers on top without materializing a generic syntax tree.

mod json_syntax_source;
mod slot_json_writer;
mod slot_reader;
mod syntax;
mod toml_syntax_source;

pub use json_syntax_source::JsonSyntaxSource;
pub use slot_json_writer::{
    SlotJsonArray, SlotJsonObject, SlotJsonValue, SlotJsonWrite, SlotJsonWriter,
    SlotJsonWriterError,
};
pub use slot_reader::{ArrayReader, ObjectReader, PropReader, SlotReader, ValueReader};
pub use syntax::{SourceSpan, SyntaxError, SyntaxEvent, SyntaxEventSource};
pub use toml_syntax_source::TomlSyntaxSource;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;
    use toml::Value;

    use crate::SlotShapeRegistry;

    #[test]
    fn json_source_emits_syntax_events_incrementally() {
        let mut source = JsonSyntaxSource::new(r#"{"pin":18,"name":"abc","ok":true}"#).unwrap();

        assert_eq!(source.next_event().unwrap(), Some(start_object()));
        assert_eq!(
            source.next_event().unwrap(),
            Some(SyntaxEvent::Prop {
                name: "pin".to_string(),
                span: Some(SourceSpan { start: 1, end: 6 }),
            })
        );
        assert_eq!(
            source.next_event().unwrap(),
            Some(SyntaxEvent::Number {
                text: "18".to_string(),
                span: Some(SourceSpan { start: 7, end: 9 }),
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
        let long = "x".repeat(syntax::STRING_CHUNK_SIZE + 7);
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
    fn object_reader_expect_discriminator_reports_expected_values() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"kind":"Blark12","pin":18}"#).unwrap(),
            &registry,
        );
        let mut object = reader.object().unwrap();

        let error = object
            .expect_discriminator("kind", &["TextureDef", "OutputDef"])
            .unwrap_err();

        assert!(error.message().contains("Blark12"));
        assert!(error.message().contains("TextureDef"));
        assert!(error.message().contains("OutputDef"));
    }

    #[test]
    fn object_finish_consumes_unit_variant_end() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"mapping":{"kind":"Disabled"},"after":true}"#).unwrap(),
            &registry,
        );
        let mut object = reader.object().unwrap();
        let mut mapping = object.next_prop().unwrap().unwrap();
        let mut mapping_object = mapping.value().object().unwrap();

        assert_eq!(
            mapping_object
                .expect_discriminator("kind", &["Disabled"])
                .unwrap(),
            "Disabled"
        );
        mapping_object.finish().unwrap();
        drop(mapping);

        let mut after = object.next_prop().unwrap().unwrap();
        assert!(after.value().bool().unwrap());
    }

    #[test]
    fn value_reader_reads_fixed_f32_arrays_with_length_errors() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"xy":[0.1,0.2]}"#).unwrap(),
            &registry,
        );
        let mut object = reader.object().unwrap();
        let mut prop = object.next_prop().unwrap().unwrap();

        assert_eq!(prop.value().f32_array::<2>().unwrap(), [0.1, 0.2]);

        let mut reader =
            SlotReader::new(JsonSyntaxSource::new(r#"{"xy":[0.1]}"#).unwrap(), &registry);
        let mut object = reader.object().unwrap();
        let mut prop = object.next_prop().unwrap().unwrap();
        let error = prop.value().f32_array::<2>().unwrap_err();

        assert!(error.message().contains("expected array of 2 f32 values"));
        assert!(error.message().contains("found 1"));
    }

    #[test]
    fn array_reader_uses_stable_item_paths() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"items":[{"ok":true},{"bad":"x"}]}"#).unwrap(),
            &registry,
        );
        let mut object = reader.object().unwrap();
        let mut prop = object.next_prop().unwrap().unwrap();
        let mut array = prop.value().array().unwrap();

        let first = array.next_item().unwrap().unwrap();
        let mut first_object = first.object().unwrap();
        let mut ok = first_object.next_prop().unwrap().unwrap();
        assert!(ok.value().bool().unwrap());
        drop(ok);
        first_object.finish().unwrap();

        let second = array.next_item().unwrap().unwrap();
        let mut second_object = second.object().unwrap();
        let mut bad = second_object.next_prop().unwrap().unwrap();
        let error = bad.value().u32().unwrap_err();

        assert_eq!(error.path(), "items[1].bad");
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
            span: Some(SourceSpan { start: 0, end: 1 }),
        }
    }
}
