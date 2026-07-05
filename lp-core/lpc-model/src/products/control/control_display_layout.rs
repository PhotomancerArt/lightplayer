//! Optional human-facing control product display metadata.
//!
//! Display layout is distinct from sample layout. Sample layout describes the
//! native output buffer; display layout describes where logical lamps should be
//! drawn in a UI when a producer can provide that information.

use alloc::vec::Vec;
use core::fmt;

use crate::project::Revision;

/// Optional control-product geometry for user-facing previews.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ControlDisplayLayout {
    /// A normalized two-dimensional lamp layout.
    Layout2d(ControlLayout2d),
}

impl ControlDisplayLayout {
    #[must_use]
    pub const fn revision(&self) -> Revision {
        match self {
            Self::Layout2d(layout) => layout.revision,
        }
    }
}

/// Normalized two-dimensional lamp display layout.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlLayout2d {
    #[serde(rename = "rev")]
    pub revision: Revision,
    #[serde(rename = "w")]
    pub width_hint: u32,
    #[serde(rename = "h")]
    pub height_hint: u32,
    #[serde(rename = "l")]
    pub lamps: Vec<ControlLamp2d>,
}

impl ControlLayout2d {
    #[must_use]
    pub const fn new(
        revision: Revision,
        width_hint: u32,
        height_hint: u32,
        lamps: Vec<ControlLamp2d>,
    ) -> Self {
        Self {
            revision,
            width_hint,
            height_hint,
            lamps,
        }
    }
}

/// One logical lamp in a two-dimensional display layout.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlLamp2d {
    pub lamp_index: u32,
    pub sample_start: u32,
    pub center: [f32; 2],
    pub radius: f32,
}

// `ControlLamp2d` has a custom `Serialize` impl that emits a fixed 5-element
// tuple `[lamp_index, sample_start, center_x, center_y, radius]`, so its schema
// must describe that wire form — not the named-field struct. It mirrors the
// `(u32, u32, f32, f32, f32)` tuple schema (`type: array` with `prefixItems` and
// `minItems`/`maxItems` of 5).
#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ControlLamp2d {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "ControlLamp2d".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema =
            <(u32, u32, f32, f32, f32) as schemars::JsonSchema>::json_schema(generator);
        schema.insert(
            "description".into(),
            "Compact lamp tuple: [lamp_index, sample_start, center_x, center_y, radius].".into(),
        );
        schema
    }
}

impl serde::Serialize for ControlLamp2d {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeTuple;

        let mut tuple = serializer.serialize_tuple(5)?;
        tuple.serialize_element(&self.lamp_index)?;
        tuple.serialize_element(&self.sample_start)?;
        tuple.serialize_element(&self.center[0])?;
        tuple.serialize_element(&self.center[1])?;
        tuple.serialize_element(&self.radius)?;
        tuple.end()
    }
}

impl<'de> serde::Deserialize<'de> for ControlLamp2d {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(ControlLamp2dVisitor)
    }
}

struct ControlLamp2dVisitor;

impl<'de> serde::de::Visitor<'de> for ControlLamp2dVisitor {
    type Value = ControlLamp2d;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a compact lamp tuple or legacy lamp object")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let lamp_index = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
        let sample_start = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
        let center_x = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
        let center_y = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
        let radius = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;

        Ok(ControlLamp2d {
            lamp_index,
            sample_start,
            center: [center_x, center_y],
            radius,
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut lamp_index = None;
        let mut sample_start = None;
        let mut center = None;
        let mut radius = None;

        while let Some(key) = map.next_key::<alloc::borrow::Cow<'de, str>>()? {
            match key.as_ref() {
                "i" | "lamp_index" => lamp_index = Some(map.next_value()?),
                "s" | "sample_start" => sample_start = Some(map.next_value()?),
                "c" | "center" => center = Some(map.next_value()?),
                "r" | "radius" => radius = Some(map.next_value()?),
                _ => {
                    let _: serde::de::IgnoredAny = map.next_value()?;
                }
            }
        }

        Ok(ControlLamp2d {
            lamp_index: lamp_index.ok_or_else(|| serde::de::Error::missing_field("lamp_index"))?,
            sample_start: sample_start
                .ok_or_else(|| serde::de::Error::missing_field("sample_start"))?,
            center: center.ok_or_else(|| serde::de::Error::missing_field("center"))?,
            radius: radius.ok_or_else(|| serde::de::Error::missing_field("radius"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_layout_exposes_revision() {
        let revision = Revision::new(9);
        let layout =
            ControlDisplayLayout::Layout2d(ControlLayout2d::new(revision, 16, 9, Vec::new()));

        assert_eq!(layout.revision(), revision);
    }

    #[test]
    fn display_layout_serializes_with_compact_field_names() {
        let layout = ControlDisplayLayout::Layout2d(ControlLayout2d::new(
            Revision::new(9),
            16,
            9,
            Vec::from([ControlLamp2d {
                lamp_index: 3,
                sample_start: 9,
                center: [0.25, 0.75],
                radius: 0.1,
            }]),
        ));

        let json = serde_json::to_string(&layout).unwrap();

        assert!(json.contains("\"rev\""));
        assert!(json.contains("\"w\""));
        assert!(json.contains("\"h\""));
        assert!(json.contains("\"l\""));
        assert!(!json.contains("lamp_index"));
        assert!(!json.contains("sample_start"));
    }

    #[test]
    fn display_lamp_serializes_as_compact_tuple() {
        let lamp = ControlLamp2d {
            lamp_index: 3,
            sample_start: 9,
            center: [0.25, 0.75],
            radius: 0.1,
        };

        let json = serde_json::to_string(&lamp).unwrap();

        assert_eq!(json, "[3,9,0.25,0.75,0.1]");
        assert_eq!(serde_json::from_str::<ControlLamp2d>(&json).unwrap(), lamp);
    }

    #[test]
    fn display_lamp_deserializes_from_owned_value() {
        // `serde_json::from_value` hands the visitor owned keys, so `visit_map`
        // must accept `Cow`/owned strings, not just borrowed `&str`.
        let value = serde_json::json!([3, 9, 0.25, 0.75, 0.1]);
        let lamp: ControlLamp2d = serde_json::from_value(value).unwrap();

        assert_eq!(
            lamp,
            ControlLamp2d {
                lamp_index: 3,
                sample_start: 9,
                center: [0.25, 0.75],
                radius: 0.1,
            }
        );
    }

    #[test]
    fn display_lamp_map_form_deserializes_from_owned_value() {
        let value = serde_json::json!({"i": 3, "s": 9, "c": [0.25, 0.75], "r": 0.1});
        let lamp: ControlLamp2d = serde_json::from_value(value).unwrap();

        assert_eq!(lamp.lamp_index, 3);
        assert_eq!(lamp.sample_start, 9);
        assert_eq!(lamp.center, [0.25, 0.75]);
        assert_eq!(lamp.radius, 0.1);
    }
}
