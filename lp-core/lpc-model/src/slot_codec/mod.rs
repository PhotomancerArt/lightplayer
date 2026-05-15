//! Slot-native streaming codec foundation.
//!
//! Syntax sources emit shape-agnostic events. [`SlotReader`] layers domain
//! construction helpers on top without materializing a generic syntax tree.

mod json_syntax_source;
mod slot_codec;
mod slot_reader;
mod slot_value_codec;
mod slot_writer;
mod syntax;
mod toml_syntax_source;

pub use json_syntax_source::JsonSyntaxSource;
pub use slot_codec::SlotCodec;
pub use slot_reader::{ArrayReader, ObjectReader, PropReader, SlotReader, ValueReader};
pub use slot_value_codec::{read_lp_value, write_lp_value, write_untyped_lp_value};
pub use slot_writer::{
    SlotArrayWriter, SlotJsonArray, SlotJsonObject, SlotJsonValue, SlotJsonWrite, SlotJsonWriter,
    SlotJsonWriterError, SlotObjectWriter, SlotValueWriter, SlotWrite, SlotWriteError, SlotWriter,
};
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
            JsonSyntaxSource::new(r#"{"brightness":0.25,"pin":18,"order":-1,"name":"main"}"#)
                .unwrap(),
            &registry,
        );

        let mut object = reader.object().unwrap();
        let mut brightness = None;
        let mut pin = None;
        let mut order = None;
        let mut name = None;

        while let Some(mut prop) = object.next_prop().unwrap() {
            match prop.name() {
                "brightness" => brightness = Some(prop.value().f32().unwrap()),
                "pin" => pin = Some(prop.value().u32().unwrap()),
                "order" => order = Some(prop.value().i32().unwrap()),
                "name" => name = Some(prop.value().string().unwrap()),
                other => panic!(
                    "{}",
                    prop.unknown_field(other, &["brightness", "pin", "order", "name"])
                ),
            }
        }

        assert_eq!(brightness, Some(0.25));
        assert_eq!(pin, Some(18));
        assert_eq!(order, Some(-1));
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
    fn value_reader_reads_string_and_u32_key_maps() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"nodes":{"output":18},"counts":{"0":1,"1":96}}"#).unwrap(),
            &registry,
        );
        let mut object = reader.object().unwrap();

        {
            let mut prop = object.next_prop().unwrap().unwrap();
            let nodes = prop.value().string_key_map(|value| value.u32()).unwrap();
            assert_eq!(nodes.get("output"), Some(&18));
        }

        let mut counts = object.next_prop().unwrap().unwrap();
        let counts = counts.value().u32_key_map(|value| value.u32()).unwrap();
        assert_eq!(counts.get(&0), Some(&1));
        assert_eq!(counts.get(&1), Some(&96));
    }

    #[test]
    fn json_writer_writes_string_maps_and_fixed_f32_arrays() {
        let mut values = alloc::collections::BTreeMap::new();
        values.insert("white_point".to_string(), [0.9, 1.0, 1.0]);

        let mut out = Vec::new();
        let mut writer = SlotJsonWriter::new(&mut out);
        let mut object = writer.object().unwrap();
        object
            .prop("values")
            .unwrap()
            .string_key_map(&values, |value, item| value.f32_array(item))
            .unwrap();
        object.finish().unwrap();

        assert_eq!(
            core::str::from_utf8(&out).unwrap(),
            r#"{"values":{"white_point":[0.9,1,1]}}"#
        );
    }

    #[test]
    fn slot_codec_reads_and_writes_one_cursor_value() {
        #[derive(Debug, PartialEq, Eq)]
        struct PinRecord {
            pin: u32,
        }

        impl SlotCodec for PinRecord {
            fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
            where
                S: SyntaxEventSource,
            {
                const FIELDS: &[&str] = &["pin"];
                let mut pin = None;
                let mut object = value.object()?;
                while let Some(mut prop) = object.next_prop()? {
                    match prop.name() {
                        "pin" => pin = Some(prop.value().u32()?),
                        other => return Err(prop.unknown_field(other, FIELDS)),
                    }
                }
                Ok(Self {
                    pin: pin.ok_or_else(|| object.missing_required_field("pin"))?,
                })
            }

            fn write_slot<W>(
                &self,
                value: SlotValueWriter<'_, W>,
            ) -> Result<(), SlotWriteError<W::Error>>
            where
                W: SlotWrite,
            {
                let mut object = value.object()?;
                object.prop("pin")?.u32(self.pin)?;
                object.finish()
            }
        }

        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"record":{"pin":18}}"#).unwrap(),
            &registry,
        );
        let mut root = reader.object().unwrap();
        let mut prop = root.next_prop().unwrap().unwrap();
        let record = PinRecord::read_slot(prop.value()).unwrap();
        assert_eq!(record, PinRecord { pin: 18 });

        let mut out = Vec::new();
        let mut writer = SlotWriter::new(&mut out);
        let mut object = writer.object().unwrap();
        record.write_slot(object.prop("record").unwrap()).unwrap();
        object.finish().unwrap();
        assert_eq!(
            core::str::from_utf8(&out).unwrap(),
            r#"{"record":{"pin":18}}"#
        );
    }

    #[test]
    fn slot_codec_round_trips_value_slots_through_lp_value_shapes() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(
                r#"{"size":{"width":64,"height":32},"transform":[[1,0,2],[0,1,3],[0,0,1]]}"#,
            )
            .unwrap(),
            &registry,
        );
        let mut object = reader.object().unwrap();

        let size = {
            let mut prop = object.next_prop().unwrap().unwrap();
            crate::Dim2uSlot::read_slot(prop.value()).unwrap()
        };
        assert_eq!(
            *size.value(),
            crate::Dim2u {
                width: 64,
                height: 32
            }
        );

        let transform = {
            let mut prop = object.next_prop().unwrap().unwrap();
            crate::Affine2dSlot::read_slot(prop.value()).unwrap()
        };
        assert_eq!(
            *transform.value(),
            crate::Affine2d {
                m00: 1.0,
                m01: 0.0,
                m10: 0.0,
                m11: 1.0,
                tx: 2.0,
                ty: 3.0,
            }
        );

        let mut out = Vec::new();
        let mut writer = SlotWriter::new(&mut out);
        let mut object = writer.object().unwrap();
        size.write_slot(object.prop("size").unwrap()).unwrap();
        transform
            .write_slot(object.prop("transform").unwrap())
            .unwrap();
        object.finish().unwrap();

        assert_eq!(
            core::str::from_utf8(&out).unwrap(),
            r#"{"size":{"width":64,"height":32},"transform":[[1,0,2],[0,1,3],[0,0,1]]}"#
        );
    }

    #[test]
    fn slot_codec_round_trips_map_and_option_slots() {
        let registry = SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"counts":{"0":12,"1":24},"enabled":true}"#).unwrap(),
            &registry,
        );
        let mut object = reader.object().unwrap();

        let counts = {
            let mut prop = object.next_prop().unwrap().unwrap();
            crate::MapSlot::<u32, crate::ValueSlot<u32>>::read_slot(prop.value()).unwrap()
        };
        assert_eq!(
            counts.entries.get(&0).map(crate::ValueSlot::value),
            Some(&12)
        );
        assert_eq!(
            counts.entries.get(&1).map(crate::ValueSlot::value),
            Some(&24)
        );

        let enabled = {
            let mut prop = object.next_prop().unwrap().unwrap();
            crate::OptionSlot::<crate::ValueSlot<bool>>::read_slot(prop.value()).unwrap()
        };
        assert!(enabled.should_write_slot());
        assert_eq!(
            enabled.data.as_ref().map(crate::ValueSlot::value),
            Some(&true)
        );

        let empty = crate::OptionSlot::<crate::ValueSlot<bool>>::none();
        assert!(!empty.should_write_slot());

        let mut out = Vec::new();
        let mut writer = SlotWriter::new(&mut out);
        let mut object = writer.object().unwrap();
        counts.write_slot(object.prop("counts").unwrap()).unwrap();
        enabled.write_slot(object.prop("enabled").unwrap()).unwrap();
        object.finish().unwrap();

        assert_eq!(
            core::str::from_utf8(&out).unwrap(),
            r#"{"counts":{"0":12,"1":24},"enabled":true}"#
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
            span: Some(SourceSpan { start: 0, end: 1 }),
        }
    }
}
