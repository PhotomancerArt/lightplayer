//! Authored 2D mapping document schema.
//!
//! Schema evolution has three rules:
//!
//! 1. **Additive fields need no bump.** Unknown fields are ignored, so a new
//!    optional field is readable by older parsers. Dense geometry is stored
//!    as plain JSON arrays today; a packed base64 alternative (e.g.
//!    `points_packed` beside [`PathShape::points`]) is deliberately left room
//!    for and would arrive as an additive field — do not encode bulk data in
//!    ways that would make that transition breaking.
//! 2. **New variants bump the format, and old parsers refuse loudly.** An
//!    unknown [`Map2dShape`] variant cannot be ignored — the document would
//!    silently lose lamps — so a document using one declares a higher
//!    `format` and older builds reject the whole document. [`Map2dDoc::from_json`]
//!    *peeks* the `format` field before running the full parse, so such a
//!    document fails as [`Map2dError::UnsupportedFormat`] with an honest
//!    "this build reads up to N" message instead of an opaque parse error.
//! 3. **Writers stamp the minimal required format.** [`Map2dDoc::required_format`]
//!    reports the lowest format that can represent a document's actual
//!    content, and [`Map2dDoc::normalize_format`] stamps it. Removing a newer
//!    construct drops the document back to the older format, so it becomes
//!    readable by older builds again.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::map2d_error::Map2dError;
use crate::map2d_fit::Bounds2d;

/// Newest document format this crate can read.
pub const MAP2D_FORMAT: u32 = 1;

/// The format every document using only the original constructs declares.
/// [`Map2dDoc::required_format`] returns this unless the content needs more.
const MAP2D_FORMAT_BASE: u32 = 1;

/// Default lamp sample diameter (texture-space units, matches the legacy
/// per-fixture default).
pub const DEFAULT_SAMPLE_DIAMETER: f32 = 2.0;

/// An authored 2D mapping document. Object order **is** wiring order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Map2dDoc {
    /// Format version gate; see [`MAP2D_FORMAT`].
    pub format: u32,
    /// Lamp sample diameter in fixture texture space (doc-level default;
    /// per-object overrides are future work).
    #[serde(default = "default_sample_diameter")]
    pub sample_diameter: f32,
    /// Optional authored canvas `[min_x, min_y, width, height]` in doc space.
    /// When present, aspect-fit frames this rectangle instead of the geometry
    /// bounds — this is how an SVG viewBox survives import, and how an editor
    /// can deliberately frame a fixture with margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<[f32; 4]>,
    #[serde(default)]
    pub objects: Vec<Map2dObject>,
}

/// One parametric mapping object: a name plus its shape parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Map2dObject {
    #[serde(default)]
    pub name: String,
    pub shape: Map2dShape,
}

/// Shape parameters. Externally tagged on purpose — `{"grid": {...}}` — so
/// the device deserializer stays free of serde's Content machinery (repo
/// rule: no `tag`/`untagged`/`flatten` in the firmware graph; see
/// `scripts/check-serde-content.sh`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Map2dShape {
    Grid(GridShape),
    Ring(RingShape),
    Path(PathShape),
}

/// A rectilinear lamp grid with snake or raster routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridShape {
    /// Top-left lamp position in doc space (before corner-based ordering).
    pub origin: [f32; 2],
    pub cols: u32,
    pub rows: u32,
    /// Lamp-to-lamp spacing, both axes.
    pub pitch: f32,
    #[serde(default)]
    pub routing: GridRouting,
    #[serde(default)]
    pub start_corner: GridCorner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridRouting {
    /// Alternate row direction every row (typical LED panel wiring).
    #[default]
    Snake,
    /// Every row runs the same direction.
    Raster,
}

/// Which corner holds lamp 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridCorner {
    #[default]
    Tl,
    Tr,
    Bl,
    Br,
}

/// One or more concentric lamp rings. Inner ring counts derive from the
/// outer count by circumference ratio: `max(1, round(outer_count * r / radius))`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingShape {
    pub center: [f32; 2],
    /// Outer ring radius.
    pub radius: f32,
    /// Lamp count on the outer ring.
    pub outer_count: u32,
    /// Number of concentric rings, auto-spaced evenly from `radius` inward
    /// (ring `k` of `n` sits at `radius * (n - k) / n`).
    #[serde(default = "default_rings")]
    pub rings: u32,
    /// Optional per-ring lamp counts, listed outer→inner. Missing or zero
    /// entries fall back to the circumference-derived count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counts: Vec<u32>,
    #[serde(default)]
    pub order: RingOrder,
    /// Angle of lamp 1 in degrees; -90 is 12 o'clock (screen coordinates,
    /// y-down).
    #[serde(default = "default_start_angle")]
    pub start_angle_deg: f32,
    #[serde(default)]
    pub dir: RingDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingOrder {
    #[default]
    OuterFirst,
    InnerFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingDir {
    /// Clockwise on screen (y-down coordinates).
    #[default]
    Cw,
    Ccw,
}

/// `count` lamps sampled evenly by arc length along a polyline; the first and
/// last lamps sit exactly on the endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathShape {
    pub points: Vec<[f32; 2]>,
    pub count: u32,
    #[serde(default)]
    pub reversed: bool,
}

impl Map2dDoc {
    /// An empty document (the editor's starting point). Empty content needs
    /// nothing newer than the base format — see [`Self::required_format`].
    pub fn new() -> Self {
        Self {
            format: MAP2D_FORMAT_BASE,
            sample_diameter: DEFAULT_SAMPLE_DIAMETER,
            canvas: None,
            objects: Vec::new(),
        }
    }

    /// Parse and format-gate a document.
    ///
    /// The format gate runs **before** the full parse (two-stage: peek, then
    /// deserialize). A newer document may well use constructs this build has
    /// never heard of — an unknown [`Map2dShape`] variant, say — and serde
    /// would report that as an opaque "unknown variant" parse error. Peeking
    /// `format` first means the honest answer wins: the document is newer
    /// than this build, not malformed. The double parse costs nothing that
    /// matters — documents are ≤10 KiB and this is a load-time path.
    pub fn from_json(json: &str) -> Result<Self, Map2dError> {
        let peek: FormatPeek =
            serde_json::from_str(json).map_err(|e| Map2dError::Parse(e.to_string()))?;
        if peek.format == 0 || peek.format > MAP2D_FORMAT {
            return Err(Map2dError::UnsupportedFormat {
                found: peek.format,
                supported: MAP2D_FORMAT,
            });
        }
        let doc: Self = serde_json::from_str(json).map_err(|e| Map2dError::Parse(e.to_string()))?;
        Ok(doc)
    }

    /// The lowest `format` that can represent this document's content.
    ///
    /// Minimal stamping: a document declares the oldest format able to read
    /// it, never simply "the newest format the writer knows". Everything in
    /// today's schema is format 1; as newer constructs land, this predicate
    /// grows a case per construct, so removing the last newer construct from
    /// a document drops it back to 1 and makes it readable by old builds
    /// again.
    pub fn required_format(&self) -> u32 {
        MAP2D_FORMAT_BASE
    }

    /// Stamp [`Self::required_format`] onto the document. Every writer runs
    /// this before serializing so the declared format tracks the content.
    pub fn normalize_format(&mut self) {
        self.format = self.required_format();
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("map2d doc serializes")
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("map2d doc serializes")
    }

    /// The authored canvas as bounds, when present and non-degenerate.
    pub fn canvas_bounds(&self) -> Option<Bounds2d> {
        let [min_x, min_y, width, height] = self.canvas?;
        (width > f32::EPSILON && height > f32::EPSILON).then_some(Bounds2d {
            min_x,
            min_y,
            width,
            height,
        })
    }
}

impl Default for Map2dDoc {
    fn default() -> Self {
        Self::new()
    }
}

/// Stage one of [`Map2dDoc::from_json`]: the `format` field alone. Unknown
/// fields are ignored by default, so this parses a document from any future
/// version — including one whose shapes this build cannot represent.
#[derive(Deserialize)]
struct FormatPeek {
    format: u32,
}

fn default_sample_diameter() -> f32 {
    DEFAULT_SAMPLE_DIAMETER
}

fn default_rings() -> u32 {
    1
}

fn default_start_angle() -> f32 {
    -90.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;
    use alloc::string::ToString;

    #[test]
    fn round_trips_a_document_through_json() {
        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "panel".to_string(),
            shape: Map2dShape::Grid(GridShape {
                origin: [100.0, 80.0],
                cols: 16,
                rows: 16,
                pitch: 26.0,
                routing: GridRouting::Snake,
                start_corner: GridCorner::Tl,
            }),
        });
        let parsed = Map2dDoc::from_json(&doc.to_json()).unwrap();
        assert_eq!(parsed, doc);
    }

    #[test]
    fn parses_minimal_fields_with_defaults() {
        let doc = Map2dDoc::from_json(
            r#"{"format":1,"objects":[
                {"shape":{"ring":{"center":[0,0],"radius":10,"outer_count":8}}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(doc.sample_diameter, DEFAULT_SAMPLE_DIAMETER);
        assert_eq!(doc.canvas, None);
        let Map2dShape::Ring(ring) = &doc.objects[0].shape else {
            panic!("expected ring");
        };
        assert_eq!(ring.rings, 1);
        assert_eq!(ring.order, RingOrder::OuterFirst);
        assert_eq!(ring.start_angle_deg, -90.0);
        assert_eq!(ring.dir, RingDir::Cw);
    }

    #[test]
    fn rejects_newer_and_zero_formats() {
        assert!(matches!(
            Map2dDoc::from_json(r#"{"format":2}"#),
            Err(Map2dError::UnsupportedFormat {
                found: 2,
                supported: MAP2D_FORMAT
            })
        ));
        assert!(matches!(
            Map2dDoc::from_json(r#"{"format":0}"#),
            Err(Map2dError::UnsupportedFormat { found: 0, .. })
        ));
    }

    /// A newer document is *newer*, not malformed: the format peek must beat
    /// serde's "unknown variant" error so the user is told to upgrade rather
    /// than told their file is broken.
    #[test]
    fn newer_format_with_unknown_variant_refuses_on_format_not_parse() {
        let newer = r#"{
            "format": 99,
            "objects": [
                { "name": "helix", "shape": { "helix": { "turns": 3, "count": 60 } } }
            ]
        }"#;
        assert_eq!(
            Map2dDoc::from_json(newer),
            Err(Map2dError::UnsupportedFormat {
                found: 99,
                supported: 1
            })
        );
    }

    /// A *current*-format document with an unknown variant really is broken —
    /// the peek must not swallow that.
    #[test]
    fn unknown_variant_at_a_supported_format_is_still_a_parse_error() {
        let broken = r#"{"format":1,"objects":[{"shape":{"helix":{"turns":3}}}]}"#;
        assert!(matches!(
            Map2dDoc::from_json(broken),
            Err(Map2dError::Parse(_))
        ));
    }

    #[test]
    fn a_document_missing_format_is_a_parse_error() {
        assert!(matches!(
            Map2dDoc::from_json(r#"{"objects":[]}"#),
            Err(Map2dError::Parse(_))
        ));
    }

    #[test]
    fn normalize_stamps_the_minimal_required_format() {
        let mut doc = corpus::cat_ears();
        assert_eq!(doc.required_format(), 1);
        // A doc stamped higher than its content needs drops back down.
        doc.format = 7;
        doc.normalize_format();
        assert_eq!(doc.format, 1);
        // And a normalized doc parses on this build.
        assert!(Map2dDoc::from_json(&doc.to_json()).is_ok());
    }

    #[test]
    fn ignores_unknown_fields_for_additive_evolution() {
        let doc = Map2dDoc::from_json(
            r#"{"format":1,"future_field":true,"objects":[
                {"shape":{"path":{"points":[[0,0],[1,0]],"count":2,"future":1}}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(doc.objects.len(), 1);
    }
}
