//! Optional human-facing control product display metadata.
//!
//! Display layout is distinct from sample layout. Sample layout describes the
//! native output buffer; display layout describes where logical lamps should be
//! drawn in a UI when a producer can provide that information.
//!
//! # Wire form: packed spans
//!
//! In memory a layout is a plain `Vec<ControlLamp2d>`. On the wire that
//! representation is ruinously redundant — every lamp repeats its index, its
//! sample offset (`index * stride`), and a radius shared with its whole
//! strand, all as JSON text (~75 bytes per lamp). A dome-scale layout could
//! not fit a project-read frame, which is why layouts used to be refused as
//! `Unsupported` over the embedded link.
//!
//! The serde impls therefore emit a PACKED form and nothing else:
//!
//! ```json
//! {"rev":9,"w":16,"h":9,
//!  "s":[[first_lamp,count,sample_start,sample_stride,radius], ...],
//!  "c":"<base64 u16le (x,y) pairs, one per lamp>",
//!  "p":[[first_lamp,lamp_count], ...]}
//! ```
//!
//! `"s"` carries *packing spans*: maximal runs of lamps whose indexes are
//! sequential, whose sample offsets advance by a constant stride, and whose
//! radius is uniform. They are an encoding artifact derived at serialize
//! time — packing is total (any layout packs; the worst case is one span per
//! lamp) and integer fields round-trip exactly. `"p"` remains the *semantic*
//! path-span list (wiring-order visualization); it is not derived from
//! `"s"`, so a layout that never knew its paths stays honest about that
//! after a round trip.
//!
//! Lamp centers ride `"c"` quantized to u16 against the normalized [0, 1]
//! extent — a 1/65535 grid, far below a lamp radius at any real render
//! extent, and 4 bytes per lamp instead of ~30. Centers are the one lossy
//! field; everything else is exact.
//!
//! At ~5.4 bytes per lamp on the wire, a 2048-lamp layout — the declared
//! embedded ceiling — fits a single 16 KiB project-read frame (pinned by
//! `a_2048_lamp_layout_fits_the_serial_frame_budget` in `lpc-wire`).

use alloc::string::String;
use alloc::vec::Vec;

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
///
/// Serializes as the packed span form — see the module docs. In memory the
/// lamps stay a plain vector; consumers never see the packed shape.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlLayout2d {
    pub revision: Revision,
    pub width_hint: u32,
    pub height_hint: u32,
    pub lamps: Vec<ControlLamp2d>,
    /// Contiguous per-path lamp spans in wiring order, when the producer
    /// knows them (fixtures do). Consumers use spans to draw wiring-order
    /// visualizations (arrows stay within a path; the chain hops between
    /// consecutive spans). Optional on the wire; absent means unknown.
    pub paths: Vec<ControlPathSpan2d>,
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
            paths: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_paths(mut self, paths: Vec<ControlPathSpan2d>) -> Self {
        self.paths = paths;
        self
    }
}

/// One path's contiguous lamp span within a 2D display layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlPathSpan2d {
    pub first_lamp: u32,
    pub lamp_count: u32,
}

/// One logical lamp in a two-dimensional display layout.
///
/// Not individually serializable: lamps only cross the wire inside a
/// [`ControlLayout2d`], reconstructed from its packed spans.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlLamp2d {
    pub lamp_index: u32,
    pub sample_start: u32,
    pub center: [f32; 2],
    pub radius: f32,
}

/// The sample stride recorded for a packing span that never saw a second
/// lamp — the value is unused on reconstruction (a one-lamp span needs only
/// its `sample_start`) but the wire field must hold something; RGB's stride
/// is the house convention.
const SINGLE_LAMP_SPAN_STRIDE: u32 = 3;

/// A maximal run of lamps the packed wire form states as one 5-tuple:
/// `[first_lamp, count, sample_start, sample_stride, radius]`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PackingSpan {
    first_lamp: u32,
    count: u32,
    sample_start: u32,
    sample_stride: u32,
    radius: f32,
}

/// Derive the packing spans for a lamp list. Total: every lamp lands in
/// exactly one span, splitting wherever an invariant breaks — a non-
/// sequential `lamp_index`, a `sample_start` off the span's stride (the
/// stride is established by the span's second lamp), or a radius change
/// (bit-exact compare; merged multi-fixture layouts legitimately mix
/// sample diameters, and those runs must stay distinct).
fn packing_spans(lamps: &[ControlLamp2d]) -> Vec<PackingSpan> {
    struct Building {
        first_lamp: u32,
        count: u32,
        sample_start: u32,
        stride: Option<u32>,
        radius: f32,
        last_sample_start: u32,
    }

    impl Building {
        fn finish(&self) -> PackingSpan {
            PackingSpan {
                first_lamp: self.first_lamp,
                count: self.count,
                sample_start: self.sample_start,
                sample_stride: self.stride.unwrap_or(SINGLE_LAMP_SPAN_STRIDE),
                radius: self.radius,
            }
        }
    }

    let mut spans = Vec::new();
    let mut building: Option<Building> = None;
    for lamp in lamps {
        if let Some(run) = building.as_mut() {
            let sequential = lamp.lamp_index == run.first_lamp.wrapping_add(run.count);
            let same_radius = lamp.radius.to_bits() == run.radius.to_bits();
            let sample_delta = lamp.sample_start.checked_sub(run.last_sample_start);
            let stride_ok = match (run.stride, sample_delta) {
                (_, None) => false, // sample offsets never run backwards within a span
                (None, Some(_)) => true,
                (Some(stride), Some(delta)) => delta == stride,
            };
            if sequential && same_radius && stride_ok {
                if run.stride.is_none() {
                    run.stride = sample_delta;
                }
                run.count += 1;
                run.last_sample_start = lamp.sample_start;
                continue;
            }
            spans.push(run.finish());
        }
        building = Some(Building {
            first_lamp: lamp.lamp_index,
            count: 1,
            sample_start: lamp.sample_start,
            stride: None,
            radius: lamp.radius,
            last_sample_start: lamp.sample_start,
        });
    }
    if let Some(run) = building {
        spans.push(run.finish());
    }
    spans
}

/// Quantize a normalized [0, 1] coordinate onto the u16 wire grid.
fn quantize_center(v: f32) -> u16 {
    let clamped = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to [0,1] then scaled to u16 range; the cast cannot truncate or lose sign"
    )]
    {
        (clamped * 65535.0 + 0.5) as u16
    }
}

/// Recover a normalized coordinate from the u16 wire grid.
fn dequantize_center(q: u16) -> f32 {
    f32::from(q) / 65535.0
}

fn encode_centers(lamps: &[ControlLamp2d]) -> String {
    use base64::Engine;
    let mut bytes = Vec::with_capacity(lamps.len() * 4);
    for lamp in lamps {
        for v in lamp.center {
            bytes.extend_from_slice(&quantize_center(v).to_le_bytes());
        }
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

impl serde::Serialize for ControlLayout2d {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let spans = packing_spans(&self.lamps);
        let centers = encode_centers(&self.lamps);
        let fields = if self.paths.is_empty() { 5 } else { 6 };
        let mut state = serializer.serialize_struct("ControlLayout2d", fields)?;
        state.serialize_field("rev", &self.revision)?;
        state.serialize_field("w", &self.width_hint)?;
        state.serialize_field("h", &self.height_hint)?;
        state.serialize_field("s", &spans)?;
        state.serialize_field("c", &centers)?;
        if self.paths.is_empty() {
            state.skip_field("p")?;
        } else {
            state.serialize_field("p", &self.paths)?;
        }
        state.end()
    }
}

/// The packed wire shape, named so deserialization errors read well.
#[derive(serde::Deserialize)]
struct ControlLayout2dWire {
    rev: Revision,
    w: u32,
    h: u32,
    s: Vec<PackingSpan>,
    c: String,
    #[serde(default)]
    p: Vec<ControlPathSpan2d>,
}

impl<'de> serde::Deserialize<'de> for ControlLayout2d {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use base64::Engine;
        use serde::de::Error;

        let wire = ControlLayout2dWire::deserialize(deserializer)?;
        let centers = base64::engine::general_purpose::STANDARD
            .decode(&wire.c)
            .map_err(|err| D::Error::custom(alloc::format!("layout centers: {err}")))?;
        let lamp_count: usize = wire.s.iter().map(|span| span.count as usize).sum();
        if centers.len() != lamp_count * 4 {
            return Err(D::Error::custom(alloc::format!(
                "layout centers carry {} bytes for {lamp_count} lamps (need {})",
                centers.len(),
                lamp_count * 4
            )));
        }

        let mut lamps = Vec::with_capacity(lamp_count);
        let mut at = 0usize;
        for span in &wire.s {
            for k in 0..span.count {
                let x = u16::from_le_bytes([centers[at], centers[at + 1]]);
                let y = u16::from_le_bytes([centers[at + 2], centers[at + 3]]);
                at += 4;
                lamps.push(ControlLamp2d {
                    lamp_index: span.first_lamp.wrapping_add(k),
                    sample_start: span.sample_start.wrapping_add(k * span.sample_stride),
                    center: [dequantize_center(x), dequantize_center(y)],
                    radius: span.radius,
                });
            }
        }

        Ok(ControlLayout2d {
            revision: wire.rev,
            width_hint: wire.w,
            height_hint: wire.h,
            lamps,
            paths: wire.p,
        })
    }
}

impl serde::Serialize for PackingSpan {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeTuple;

        let mut tuple = serializer.serialize_tuple(5)?;
        tuple.serialize_element(&self.first_lamp)?;
        tuple.serialize_element(&self.count)?;
        tuple.serialize_element(&self.sample_start)?;
        tuple.serialize_element(&self.sample_stride)?;
        tuple.serialize_element(&self.radius)?;
        tuple.end()
    }
}

impl<'de> serde::Deserialize<'de> for PackingSpan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (first_lamp, count, sample_start, sample_stride, radius) =
            <(u32, u32, u32, u32, f32)>::deserialize(deserializer)?;
        Ok(Self {
            first_lamp,
            count,
            sample_start,
            sample_stride,
            radius,
        })
    }
}

impl serde::Serialize for ControlPathSpan2d {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeTuple;

        let mut tuple = serializer.serialize_tuple(2)?;
        tuple.serialize_element(&self.first_lamp)?;
        tuple.serialize_element(&self.lamp_count)?;
        tuple.end()
    }
}

impl<'de> serde::Deserialize<'de> for ControlPathSpan2d {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (first_lamp, lamp_count) = <(u32, u32)>::deserialize(deserializer)?;
        Ok(Self {
            first_lamp,
            lamp_count,
        })
    }
}

// Schemas describe the WIRE form (the packed object), not the in-memory
// struct — mirroring the house convention set by the old compact-tuple
// impls.
#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ControlPathSpan2d {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "ControlPathSpan2d".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = <(u32, u32) as schemars::JsonSchema>::json_schema(generator);
        schema.insert(
            "description".into(),
            "Compact path span tuple: [first_lamp, lamp_count].".into(),
        );
        schema
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ControlLayout2d {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "ControlLayout2d".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        #[derive(schemars::JsonSchema)]
        #[allow(dead_code, reason = "schema mirror: described, never constructed")]
        struct PackingSpanSchema((u32, u32, u32, u32, f32));

        #[derive(schemars::JsonSchema)]
        #[allow(dead_code, reason = "schema mirror: described, never constructed")]
        struct ControlLayout2dSchema {
            /// Layout revision.
            rev: Revision,
            /// Width hint of the normalized extent.
            w: u32,
            /// Height hint of the normalized extent.
            h: u32,
            /// Packing spans: [first_lamp, count, sample_start, sample_stride, radius].
            s: Vec<PackingSpanSchema>,
            /// Base64 u16le lamp centers, one (x, y) pair per lamp.
            c: String,
            /// Optional wiring-order path spans: [first_lamp, lamp_count].
            #[serde(default)]
            p: Vec<ControlPathSpan2d>,
        }

        let mut schema = <ControlLayout2dSchema as schemars::JsonSchema>::json_schema(generator);
        schema.insert(
            "description".into(),
            "Packed 2D display layout: lamp runs as spans plus base64 u16 centers.".into(),
        );
        schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn lamp(index: u32, sample_start: u32, center: [f32; 2], radius: f32) -> ControlLamp2d {
        ControlLamp2d {
            lamp_index: index,
            sample_start,
            center,
            radius,
        }
    }

    /// A run of `count` lamps from `first` with RGB stride and one radius.
    fn run(first: u32, count: u32, sample_start: u32, radius: f32) -> Vec<ControlLamp2d> {
        (0..count)
            .map(|k| lamp(first + k, sample_start + k * 3, [0.25, 0.75], radius))
            .collect()
    }

    #[test]
    fn display_layout_exposes_revision() {
        let revision = Revision::new(9);
        let layout =
            ControlDisplayLayout::Layout2d(ControlLayout2d::new(revision, 16, 9, Vec::new()));

        assert_eq!(layout.revision(), revision);
    }

    /// A merged-output shape: three wire runs (the peach's patched split),
    /// mixed radii, a non-zero-based middle run. Integer fields round-trip
    /// exactly; centers within one quantization step.
    #[test]
    fn a_merged_layout_round_trips_through_the_packed_form() {
        let mut lamps = run(0, 22, 0, 0.01);
        lamps.extend(run(22, 12, 102, 0.02)); // the leaf: different radius + sample origin
        lamps.extend(run(34, 22, 66, 0.01));
        let layout = ControlLayout2d::new(Revision::new(7), 64, 48, lamps).with_paths(vec![
            ControlPathSpan2d {
                first_lamp: 0,
                lamp_count: 22,
            },
            ControlPathSpan2d {
                first_lamp: 22,
                lamp_count: 12,
            },
            ControlPathSpan2d {
                first_lamp: 34,
                lamp_count: 22,
            },
        ]);

        let json = serde_json::to_string(&layout).unwrap();
        let back: ControlLayout2d = serde_json::from_str(&json).unwrap();

        assert_eq!(back.revision, layout.revision);
        assert_eq!((back.width_hint, back.height_hint), (64, 48));
        assert_eq!(back.paths, layout.paths);
        assert_eq!(back.lamps.len(), layout.lamps.len());
        for (a, b) in layout.lamps.iter().zip(&back.lamps) {
            assert_eq!(a.lamp_index, b.lamp_index);
            assert_eq!(a.sample_start, b.sample_start);
            assert_eq!(a.radius, b.radius);
            assert!((a.center[0] - b.center[0]).abs() <= 1.0 / 65535.0);
            assert!((a.center[1] - b.center[1]).abs() <= 1.0 / 65535.0);
        }
    }

    /// Splits happen exactly where an invariant breaks: a radius change and
    /// a sample_start jump each start a new span, and a second round trip is
    /// byte-identical (packing is deterministic).
    #[test]
    fn packing_splits_on_radius_and_sample_discontinuities() {
        let mut lamps = run(0, 4, 0, 0.01);
        lamps.extend(run(4, 3, 200, 0.01)); // sample jump, same radius
        lamps.extend(run(7, 2, 209, 0.03)); // radius change
        let layout = ControlLayout2d::new(Revision::new(1), 16, 16, lamps);

        let spans = packing_spans(&layout.lamps);
        assert_eq!(
            spans
                .iter()
                .map(|s| (s.first_lamp, s.count, s.sample_start))
                .collect::<Vec<_>>(),
            vec![(0, 4, 0), (4, 3, 200), (7, 2, 209)],
        );

        let json = serde_json::to_string(&layout).unwrap();
        let back: ControlLayout2d = serde_json::from_str(&json).unwrap();
        let json_again = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json_again);
    }

    /// A layout that never declared its paths must not grow synthetic ones
    /// from the packing spans — "absent means unknown" survives the wire.
    #[test]
    fn undeclared_paths_stay_undeclared_across_the_wire() {
        let layout = ControlLayout2d::new(Revision::new(2), 8, 8, run(0, 5, 0, 0.01));

        let json = serde_json::to_string(&layout).unwrap();
        assert!(!json.contains("\"p\""), "no paths field emitted: {json}");
        let back: ControlLayout2d = serde_json::from_str(&json).unwrap();
        assert!(back.paths.is_empty());
    }

    #[test]
    fn empty_layout_round_trips() {
        let layout = ControlLayout2d::new(Revision::new(3), 16, 16, Vec::new());

        let json = serde_json::to_string(&layout).unwrap();
        let back: ControlLayout2d = serde_json::from_str(&json).unwrap();

        assert!(back.lamps.is_empty());
        assert!(back.paths.is_empty());
        assert_eq!((back.width_hint, back.height_hint), (16, 16));
    }

    /// The wire shape itself: single-letter fields, spans as 5-tuples,
    /// centers as base64 (never raw bytes or per-lamp tuples).
    #[test]
    fn display_layout_serializes_as_packed_spans_and_base64_centers() {
        let layout = ControlDisplayLayout::Layout2d(
            ControlLayout2d::new(Revision::new(9), 16, 9, run(0, 2, 0, 0.1)).with_paths(vec![
                ControlPathSpan2d {
                    first_lamp: 0,
                    lamp_count: 2,
                },
            ]),
        );

        let json = serde_json::to_string(&layout).unwrap();

        assert!(json.contains("\"rev\""), "{json}");
        assert!(json.contains("\"s\":[[0,2,0,3,0.1]]"), "{json}");
        assert!(json.contains("\"p\":[[0,2]]"), "{json}");
        assert!(!json.contains("lamp_index"), "{json}");
        // Two lamps at (0.25, 0.75): quantized 16384/49151 → 8 bytes b64.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let centers = value["layout2d"]["c"].as_str().unwrap();
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(centers)
            .unwrap();
        assert_eq!(bytes.len(), 8);
    }

    /// A one-lamp span records the conventional stride; reconstruction
    /// ignores it.
    #[test]
    fn single_lamp_spans_round_trip() {
        let lamps = vec![lamp(0, 0, [0.0, 0.0], 0.01), lamp(5, 900, [1.0, 1.0], 0.02)];
        let layout = ControlLayout2d::new(Revision::new(4), 4, 4, lamps.clone());

        let json = serde_json::to_string(&layout).unwrap();
        let back: ControlLayout2d = serde_json::from_str(&json).unwrap();

        assert_eq!(back.lamps.len(), 2);
        assert_eq!(back.lamps[0].lamp_index, 0);
        assert_eq!(back.lamps[1].lamp_index, 5);
        assert_eq!(back.lamps[1].sample_start, 900);
        assert_eq!(back.lamps[1].center, [1.0, 1.0]);
    }

    /// Truncated or oversized center payloads are a decode error, not a
    /// silently short lamp list.
    #[test]
    fn mismatched_center_length_refuses_to_decode() {
        let json = r#"{"rev":1,"w":4,"h":4,"s":[[0,2,0,3,0.1]],"c":"AAAA"}"#;
        let err = serde_json::from_str::<ControlLayout2d>(json).unwrap_err();
        assert!(err.to_string().contains("centers"), "{err}");
    }
}
