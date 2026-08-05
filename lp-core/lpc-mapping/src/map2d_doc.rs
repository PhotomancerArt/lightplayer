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
//! 2. **Constructs an old parser would misread bump the format, and old
//!    parsers refuse loudly.** An unknown [`Map2dShape`] variant cannot be
//!    ignored — the document would silently lose lamps — and neither can an
//!    additive field that changes what existing fields *mean*
//!    ([`PathShape::gaps`] re-parameterizes the whole path), so a document
//!    using one declares a higher `format` and older builds reject the whole
//!    document. [`Map2dDoc::from_json`]
//!    *peeks* the `format` field before running the full parse, so such a
//!    document fails as [`Map2dError::UnsupportedFormat`] with an honest
//!    "this build reads up to N" message instead of an opaque parse error.
//! 3. **Writers stamp the minimal required format.** [`Map2dDoc::required_format`]
//!    reports the lowest format that can represent a document's actual
//!    content, and [`Map2dDoc::normalize_format`] stamps it. Removing a newer
//!    construct drops the document back to the older format, so it becomes
//!    readable by older builds again.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::map2d_error::Map2dError;
use crate::map2d_fit::Bounds2d;

/// Newest document format this crate can read.
pub const MAP2D_FORMAT: u32 = 2;

/// The format every document using only the original constructs declares.
/// [`Map2dDoc::required_format`] returns this unless the content needs more.
const MAP2D_FORMAT_BASE: u32 = 1;

/// The format a document needs once any path carries inert [`PathShape::gaps`].
/// A format-1 build parses the field away silently — it would light the jumper
/// wire and shift every downstream wiring index — so gapped documents declare
/// this and old builds refuse them whole.
const MAP2D_FORMAT_PATH_GAPS: u32 = 2;

/// The format a document needs once any object is a [`RepeatShape`]. A
/// format-1 build has never heard of the variant and cannot ignore it — it
/// would lose every lamp the object carries — so repeated documents declare
/// this and old builds refuse them whole.
const MAP2D_FORMAT_REPEAT: u32 = 2;

/// Largest [`RepeatShape::count`] an editor may author. The resolver itself
/// has no ceiling — a hand-authored document is the author's business — but a
/// slider or a typo should not be able to multiply a 300-lamp strand into a
/// six-figure document.
pub const MAX_REPEAT_COUNT: u32 = 64;

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
    Repeat(RepeatShape),
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
///
/// With [`gaps`](Self::gaps), "arc length" means *active* arc length: the
/// polyline may include jumper-wire segments that carry no lamps, so one
/// physical channel can stay one object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathShape {
    pub points: Vec<[f32; 2]>,
    pub count: u32,
    #[serde(default)]
    pub reversed: bool,
    /// Indices of inert segments (segment `i` runs `points[i] → points[i+1]`):
    /// physical jumper wire that carries no lamps. Lamps distribute evenly
    /// over the remaining (active) length only — fixed-pitch strip cut at hubs
    /// and jumpered keeps its pitch across the whole channel — and an inert
    /// segment emits no lamp entries at all, so wiring indices downstream are
    /// unshifted.
    ///
    /// `skip_serializing_if` keeps gap-free documents byte-identical: the
    /// field only ever appears on a document that actually uses it — which is
    /// also the document that stamps format 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<u32>,
}

/// `count` rotated instances of an inner shape, equally spaced over a full
/// circle around [`center`](Self::center).
///
/// Instance `k` is the inner shape rotated `k * (360 / count)` degrees;
/// instance 0 is the inner shape unrotated. Instances are consecutive in
/// wiring order and each one resolves to its **own span** — a repeated
/// document's instances are physical strands, not one long run, so the
/// fixture's honest spans and the output face's strip boundaries see N
/// strands of `inner_count` lamps. Nesting is allowed; spans multiply, and
/// the innermost instances are the strands.
///
/// The inner shape is boxed so the enum stays small (a `Map2dShape` is
/// otherwise dominated by [`PathShape`]'s vectors), and the nesting is plain
/// external tagging — `{"repeat": {"shape": {"path": {...}}, ...}}` — because
/// the firmware graph admits no serde `tag`/`untagged`/`flatten`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatShape {
    pub shape: Box<Map2dShape>,
    /// Rotation center in doc space.
    pub center: [f32; 2],
    pub count: u32,
}

impl RepeatShape {
    /// Clamp `count` into the range an editor may author
    /// (`1..=`[`MAX_REPEAT_COUNT`]). Pure so the editor's sanitize pass and
    /// any other writer agree on the bound.
    pub fn clamp_count(&mut self) {
        self.count = self.count.clamp(1, MAX_REPEAT_COUNT);
    }
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
        format_gate(peek.format, MAP2D_FORMAT)?;
        let doc: Self = serde_json::from_str(json).map_err(|e| Map2dError::Parse(e.to_string()))?;
        Ok(doc)
    }

    /// The lowest `format` that can represent this document's content.
    ///
    /// Minimal stamping: a document declares the oldest format able to read
    /// it, never simply "the newest format the writer knows". This predicate
    /// grows a case per newer construct, so removing the last newer construct
    /// from a document drops it back to 1 and makes it readable by old builds
    /// again.
    /// The walk recurses through [`RepeatShape`] inners: a gapped path nested
    /// inside a repeat is still a gapped path, and a repeat is itself a
    /// construct no format-1 build can represent.
    pub fn required_format(&self) -> u32 {
        let mut required = MAP2D_FORMAT_BASE;
        for object in &self.objects {
            required = required.max(shape_required_format(&object.shape));
        }
        required
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

/// The format gate itself, `supported` passed in rather than read from
/// [`MAP2D_FORMAT`].
///
/// The parameter is what makes the *old build's* behavior testable: this crate
/// can only ever be its own version, so "would a build that reads up to 1
/// refuse this document?" is otherwise unaskable. That refusal is the designed
/// outcome for every format-2 construct, so it gets a test, not a promise.
fn format_gate(found: u32, supported: u32) -> Result<(), Map2dError> {
    if found == 0 || found > supported {
        return Err(Map2dError::UnsupportedFormat { found, supported });
    }
    Ok(())
}

/// The lowest format able to read one shape, inner shapes included.
fn shape_required_format(shape: &Map2dShape) -> u32 {
    match shape {
        // Inert path segments (format 2): an old build would parse `gaps`
        // away and light the jumper wire.
        Map2dShape::Path(path) if !path.gaps.is_empty() => MAP2D_FORMAT_PATH_GAPS,
        // The rotational repeat (format 2): an unknown variant cannot be
        // ignored — the whole object's lamps would vanish.
        Map2dShape::Repeat(repeat) => MAP2D_FORMAT_REPEAT.max(shape_required_format(&repeat.shape)),
        _ => MAP2D_FORMAT_BASE,
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
    use alloc::vec;

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
            Map2dDoc::from_json(r#"{"format":3}"#),
            Err(Map2dError::UnsupportedFormat {
                found: 3,
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
                supported: 2
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

    /// Inert path segments are the first format-2 construct: a document that
    /// uses them stamps 2, and dropping the last gap puts it back to 1 so old
    /// builds can read it again.
    #[test]
    fn inert_path_gaps_require_format_two_and_release_it() {
        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "sector".to_string(),
            shape: Map2dShape::Path(PathShape {
                points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                count: 4,
                reversed: false,
                gaps: vec![1],
            }),
        });
        assert_eq!(doc.required_format(), 2);
        doc.normalize_format();
        assert_eq!(doc.format, 2);
        assert_eq!(Map2dDoc::from_json(&doc.to_json()).unwrap(), doc);

        let Map2dShape::Path(path) = &mut doc.objects[0].shape else {
            panic!("expected path");
        };
        path.gaps.clear();
        doc.normalize_format();
        assert_eq!(doc.format, 1);
    }

    /// Byte-stability: `gaps` is skipped when empty, so every document written
    /// before this field existed round-trips through the current writer
    /// unchanged.
    #[test]
    fn a_gap_free_path_serializes_without_the_gaps_field() {
        let doc = corpus::cat_ears();
        let json = doc.to_json();
        assert!(!json.contains("gaps"), "{json}");
        assert!(json.contains("\"format\":1"));
    }

    /// The repeat variant is the other format-2 construct, and the first new
    /// [`Map2dShape`] variant ever added — the case the whole loud-refusal
    /// posture was designed around.
    #[test]
    fn a_repeat_requires_format_two_and_releases_it() {
        let mut doc = repeated_doc(4);
        assert_eq!(doc.required_format(), 2);
        doc.normalize_format();
        assert_eq!(doc.format, 2);
        assert_eq!(Map2dDoc::from_json(&doc.to_json()).unwrap(), doc);

        // Unwrap the repeat and the document is plain again.
        let Map2dShape::Repeat(repeat) = &doc.objects[0].shape else {
            panic!("expected repeat");
        };
        doc.objects[0].shape = (*repeat.shape).clone();
        doc.normalize_format();
        assert_eq!(doc.format, 1);
    }

    /// `required_format` walks *into* repeats: the format a document needs is
    /// the highest anything inside it needs, however deep.
    #[test]
    fn the_format_walk_recurses_through_repeat_inners() {
        // A repeat wrapping a repeat wrapping a plain path is still just 2 —
        // recursion must not inflate the stamp.
        let nested = Map2dDoc {
            objects: vec![Map2dObject {
                name: "nested".to_string(),
                shape: Map2dShape::Repeat(RepeatShape {
                    shape: Box::new(Map2dShape::Repeat(RepeatShape {
                        shape: Box::new(plain_path(Vec::new())),
                        center: [0.0, 0.0],
                        count: 2,
                    })),
                    center: [10.0, 10.0],
                    count: 3,
                }),
            }],
            ..Map2dDoc::new()
        };
        assert_eq!(nested.required_format(), 2);

        // And a gapped path buried inside a repeat still counts as gapped —
        // the inner shape is not out of sight of the stamp.
        let gapped_inner = Map2dDoc {
            objects: vec![Map2dObject {
                name: "sector".to_string(),
                shape: Map2dShape::Repeat(RepeatShape {
                    shape: Box::new(plain_path(vec![1])),
                    center: [0.0, 0.0],
                    count: 5,
                }),
            }],
            ..Map2dDoc::new()
        };
        assert_eq!(gapped_inner.required_format(), 2);
    }

    /// The designed failure: a build that reads up to format 1 meets a
    /// repeated document and refuses it whole, with an honest "you need a
    /// newer build" — never a half-read document missing every repeated lamp.
    #[test]
    fn a_format_one_build_refuses_a_repeated_document_honestly() {
        let mut doc = repeated_doc(5);
        doc.normalize_format();
        let stamped = doc.format;

        assert_eq!(
            format_gate(stamped, 1),
            Err(Map2dError::UnsupportedFormat {
                found: 2,
                supported: 1
            })
        );
        // The same build reads every format-1 document it always could.
        assert_eq!(format_gate(corpus::cat_ears().format, 1), Ok(()));
        // And this build reads the repeated one.
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

    /// A three-point path, optionally with inert segments.
    fn plain_path(gaps: Vec<u32>) -> Map2dShape {
        Map2dShape::Path(PathShape {
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
            count: 4,
            reversed: false,
            gaps,
        })
    }

    fn repeated_doc(count: u32) -> Map2dDoc {
        Map2dDoc {
            objects: vec![Map2dObject {
                name: "sector".to_string(),
                shape: Map2dShape::Repeat(RepeatShape {
                    shape: Box::new(plain_path(Vec::new())),
                    center: [5.0, 5.0],
                    count,
                }),
            }],
            ..Map2dDoc::new()
        }
    }
}
