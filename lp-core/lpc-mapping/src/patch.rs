//! Authored per-fixture **patch** document: where a fixture's lamps land on
//! an output's wire.
//!
//! Mapping and patching are different jobs. A mapping says where a lamp *is*
//! in space and never changes once a fixture is built; a patch says which
//! physical jack that strand ended up in, and changes on every install. They
//! are edited as modes of one surface and stored as two documents, so an
//! installer can clear the patch without touching a single lamp position.
//!
//! # Two formats, two serialized shapes, ONE in-memory type
//!
//! **Format 1** (the original grain) is verbose and keyed — fixture-relative
//! lamp ranges anchored at a wire position:
//!
//! ```jsonc
//! {
//!   "format": 1,
//!   "entries": [
//!     { "range": { "start": 0,  "count": 22 }, "at": { "channel": 0 } },
//!     { "range": { "start": 22, "count": 22 }, "at": { "channel": 34 },
//!       "reversed": true }
//!   ]
//! }
//! ```
//!
//! **Format 2** (the object grain, D46/D47) is compact tuple rows plus an
//! outputs header table:
//!
//! ```jsonc
//! {
//!   "format": 2,
//!   "outputs": ["1", "Box 2"],
//!   "entries": [
//!     ["/sector/0", 0, 78],              // path source, output 0, wire lamp 78
//!     ["/sector/1", 1, 0, "r"],          // reversed
//!     ["/sector/2", -1, 0, "", 10],      // default output, rotated 10 lamps
//!     [["/door/1", 0, 3], 0, 36],        // sub-span: path + node-relative range
//!     [[0, 22], -1, 0],                  // fixture-relative range (format-1 grain)
//!     [[22], -1, 30]                     // one-element range = to the end
//!   ]
//! }
//! ```
//!
//! Row = `[from, out#, lamp, flags?, offset?]`:
//!
//! - `from` — a [`MapObjectPath`] string (`"/sector/2"`), OR
//!   `[path, start]` / `[path, start, count]` for a node-relative sub-span,
//!   OR `[start]` / `[start, count]` for the fixture-relative grain (the
//!   shape-less-strip case; one element = to the end).
//! - `out#` — index into the `outputs` header table of output **names**
//!   (D39); **−1 = the default output** (no explicit output — the entry
//!   flows to whatever output is first on the bus). Names live ONLY in the
//!   table, so it doubles as a scannable declaration of touched outputs.
//! - `lamp` — the wire anchor, in lamps (never "channel": D45 reserves that
//!   word for real 512-limited DMX, which this system does not touch).
//! - `flags` — `"r"` = reversed; present (possibly `""`) whenever `offset`
//!   follows.
//! - `offset` — rotation in lamps (see *Rotation* below); trailing defaults
//!   are omitted, so rows are 3–5 columns.
//!
//! There is ONE canonical form per format, read and write — no dual decode,
//! no aliases (wire-posture discipline; hand-authoring targets the same
//! rows the writer emits). Why rows: a radiance-scale patch is ~200
//! entries, and the bulk of the keyed form is JSON keys, not data — rows
//! cut ~25 KB to ~6 KB while keeping line-per-entry hand-editing and
//! diffing. VLQ/source-map-style blobs were measured and REJECTED (they
//! kill the `.patch.json` text-editor escape hatch for a few more KB);
//! recorded trigger for the next tier: a real patch doc exceeding ~32 KB
//! on-device.
//!
//! # Minimal stamping
//!
//! Writers stamp the lowest format that can represent the document
//! ([`PatchDoc::required_format`], mirroring [`crate::Map2dDoc`]): a doc
//! using only format-1 constructs writes the keyed shape stamped 1 (the
//! peach corpus stays readable by old builds), and any format-2 construct —
//! a path source, an output reference, a nonzero rotation — switches the
//! whole doc to rows stamped 2. The format gate keeps refusing newer
//! ([`format_gate`]): minimal stamping governs WRITING, the gate governs
//! READING, and nothing ever migrates in place.
//!
//! A format-1 document carrying the reserved `at.output`/`offset` fields
//! still parses and is refused at RESOLVE time with an error naming the fix
//! (stamp format 2) — refusing to apply them silently is what keeps the
//! light where the author put it.
//!
//! # Path identity (D46)
//!
//! `from: "/sector/2"` means "instance 2 of the object whose stable id is
//! `sector`, wherever its lamps currently are". Instance boundaries
//! re-derive from the mapping at resolve time through
//! [`crate::ObjectInstanceSpan`], so the dome that grows two lamps per
//! strut keeps every entry pointing at the right physical panel — where a
//! stored lamp range would shift. Ranges survive as the sub-span
//! refinement and as the whole story for shape-less strips.
//!
//! # Rotation (D41) — the kernel owns this definition
//!
//! An entry of `N` lamps anchored at wire lamp `c` with rotation `offset`
//! `k` maps its lamps within the window `[c, c+N)`: **orientation first,
//! then rotation.** Source lamp `j` (0-based within the run, fixture
//! order) sits at
//!
//! ```text
//! wire = c + ((j' + k) mod N)      where j' = reversed ? N-1-j : j
//! ```
//!
//! Worked 5-lamp example, `c = 10`, `k = 2`: forward lamps `[0,1,2,3,4]`
//! land on wire `[12,13,14,10,11]`; reversed, the same lamps land on
//! `[11,10,14,13,12]`. Rotation permutes *within* the window, so a rotated
//! run still occupies exactly `[c, c+N)` and the collision rules below are
//! unchanged. The engine renders this arithmetic; UIs step `k` by the
//! object's stride hint ([`crate::object_stride`]).
//!
//! # Anchor and reflow
//!
//! A patch is **sparse**. Entries anchor the ranges an installer cares
//! about; every lamp no entry claims flows on after the highest anchored
//! end **on the default output**, in fixture order. Runs addressed to a
//! named output neither consume nor extend the default-output cursor.
//! An EMPTY document resolves to the whole fixture at wire lamp 0.
//!
//! # Refusals
//!
//! Two entries claiming one fixture lamp are a document error regardless of
//! output (a physical lamp exists once). Two entries landing on the same
//! wire lamp are an error only **on the same output** — different outputs
//! are different wires. Unknown object paths and (when the context names
//! the allowed set) dangling output names refuse with locators. `format`
//! is peeked before the parse and anything newer is refused whole.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::map_object_path::MapObjectPath;
use crate::map2d_resolve::ObjectInstanceSpan;

/// Newest patch document format this crate can read.
pub const PATCH_FORMAT: u32 = 2;

/// The format every document using only the original constructs declares.
pub const PATCH_FORMAT_BASE: u32 = 1;

/// The format a document needs once any entry uses the object grain: a
/// path source, an output reference, or a nonzero rotation offset.
pub const PATCH_FORMAT_OBJECT_GRAIN: u32 = 2;

/// An authored patch document: sparse placement entries over auto-flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchDoc {
    /// Format version gate; see [`PATCH_FORMAT`]. [`Self::to_json`] always
    /// writes [`Self::required_format`] (stamp and shape cannot diverge),
    /// and [`Self::normalize_format`] folds it back into the value.
    pub format: u32,
    /// Anchored entries, in authoring order. An empty list is a valid
    /// document meaning "everything auto-flows" — the state a cleared patch
    /// leaves behind.
    pub entries: Vec<PatchEntry>,
}

/// One anchored entry: this fixture source goes at that wire position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchEntry {
    /// Which of the fixture's lamps this entry places.
    pub source: PatchSource,
    /// Which output, by its authored NAME (D39); `None` = the default
    /// output. Format-2 construct.
    pub output: Option<String>,
    /// The wire anchor, in lamps (format 1 spelled this `at.channel`).
    pub lamp: u32,
    /// Lay the run down end-first (a strand fed from its far end).
    pub reversed: bool,
    /// Rotation in lamps within the run's window (0 = none). Format-2
    /// construct; see the module doc for the definition.
    pub offset: u32,
}

/// What an entry places: an object-tree path (format 2) or a plain
/// fixture-relative range (the format-1 grain, still first-class for
/// shape-less strips).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    /// D46 path identity, with an optional node-relative sub-range.
    Path {
        path: MapObjectPath,
        range: Option<PatchRange>,
    },
    /// Fixture-relative lamps.
    Range(PatchRange),
}

/// A run of lamps: fixture-relative under [`PatchSource::Range`],
/// node-relative under [`PatchSource::Path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchRange {
    pub start: u32,
    /// Lamp count. Absent means "to the end" — lengths derive from the
    /// mapping, so a fixture that grows keeps its tail patched.
    pub count: Option<u32>,
}

/// One resolved placement: `count` fixture lamps starting at `start` land
/// on `output`'s wire at `lamp`, oriented and rotated as stated.
///
/// This is what the engine turns into output fragments. Units are LAMPS on
/// both sides — the sample arithmetic belongs to whoever knows the channel
/// count per lamp, which is not this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchedRange {
    /// First fixture lamp of the run.
    pub start: u32,
    /// Lamps in the run. Never zero.
    pub count: u32,
    /// First wire lamp of the run's window.
    pub lamp: u32,
    /// Lay the run down end-first (applied before rotation — module doc).
    pub reversed: bool,
    /// Rotation in lamps within the window (0 = none).
    pub offset: u32,
    /// The addressed output's name; `None` = the default output.
    pub output: Option<String>,
}

impl PatchedRange {
    /// One past this run's last fixture lamp.
    #[must_use]
    pub fn end(&self) -> u32 {
        self.start.saturating_add(self.count)
    }

    /// One past this run's last wire lamp.
    #[must_use]
    pub fn lamp_end(&self) -> u32 {
        self.lamp.saturating_add(self.count)
    }
}

/// Everything [`resolve_patch`] needs beyond the document itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct PatchResolveContext<'a> {
    /// The fixture's total lamp count (what format-1 ranges resolve
    /// against, and the reflow's universe).
    pub fixture_lamp_count: u32,
    /// Per-strand instance spans from the fixture's resolved mapping
    /// ([`crate::object_instance_spans`]); empty for shape-less strips.
    pub object_spans: &'a [ObjectInstanceSpan],
    /// When `Some`, entries naming an output outside this set refuse as
    /// [`PatchError::DanglingOutput`]; `None` defers the check to the
    /// engine (which degrades per run instead of refusing the document).
    pub allowed_outputs: Option<&'a [String]>,
    /// The default output's name, when the caller knows it: entries naming
    /// it explicitly join the unaddressed group for reflow and collision.
    pub default_output: Option<&'a str>,
}

impl<'a> PatchResolveContext<'a> {
    /// The common case: a bare fixture, no object table, no output info.
    #[must_use]
    pub fn for_fixture(fixture_lamp_count: u32) -> Self {
        Self {
            fixture_lamp_count,
            object_spans: &[],
            allowed_outputs: None,
            default_output: None,
        }
    }
}

/// Errors reading or resolving a patch document.
#[derive(Debug, Clone, PartialEq)]
pub enum PatchError {
    /// The document is not valid JSON for its format's shape.
    Parse(String),
    /// The document's `format` is zero or newer than this crate supports.
    UnsupportedFormat { found: u32, supported: u32 },
    /// One entry's parameters are invalid; `entry` is its index in
    /// `entries` and `from` names its source when it has one.
    InvalidEntry {
        entry: u32,
        from: Option<String>,
        reason: String,
    },
    /// An entry's path names no object/instance in the mapping (unknown id,
    /// or an instance index beyond the repeat count).
    UnknownPath { entry: u32, path: String },
    /// An entry names an output outside the context's allowed set.
    DanglingOutput { entry: u32, name: String },
    /// Two entries claim the same fixture lamp.
    LampClaimedTwice { entry: u32, other: u32, lamp: u32 },
    /// Two entries land on the same wire lamp of one output.
    WireLampClaimedTwice { entry: u32, other: u32, lamp: u32 },
}

impl core::fmt::Display for PatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(reason) => write!(f, "invalid patch document: {reason}"),
            Self::UnsupportedFormat { found, supported } => write!(
                f,
                "unsupported patch format {found} (this build reads up to {supported})"
            ),
            Self::InvalidEntry {
                entry,
                from,
                reason,
            } => match from {
                Some(from) => write!(f, "patch entry {entry} ({from}): {reason}"),
                None => write!(f, "patch entry {entry}: {reason}"),
            },
            Self::UnknownPath { entry, path } => write!(
                f,
                "patch entry {entry}: path {path} names no object or instance in the mapping"
            ),
            Self::DanglingOutput { entry, name } => {
                write!(f, "patch entry {entry}: no output is named {name:?}")
            }
            Self::LampClaimedTwice { entry, other, lamp } => write!(
                f,
                "patch entries {other} and {entry} both place fixture lamp {lamp}"
            ),
            Self::WireLampClaimedTwice { entry, other, lamp } => write!(
                f,
                "patch entries {other} and {entry} both land on wire lamp {lamp}"
            ),
        }
    }
}

impl core::error::Error for PatchError {}

impl PatchDoc {
    /// An empty patch: valid, equivalent to no patch at all, and stamped at
    /// the base format (empty content needs nothing newer).
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: PATCH_FORMAT_BASE,
            entries: Vec::new(),
        }
    }

    /// Parse and format-gate a patch document.
    ///
    /// Two-stage like [`crate::Map2dDoc::from_json`]: `format` alone first,
    /// so a newer document reports as newer rather than as malformed. Each
    /// format then parses its OWN shape only — format-1 rows or format-2
    /// keyed entries are parse errors, never leniently accepted.
    pub fn from_json(json: &str) -> Result<Self, PatchError> {
        let peek: FormatPeek =
            serde_json::from_str(json).map_err(|e| PatchError::Parse(e.to_string()))?;
        format_gate(peek.format, PATCH_FORMAT)?;
        match peek.format {
            PATCH_FORMAT_BASE => parse_v1(json),
            PATCH_FORMAT_OBJECT_GRAIN => parse_v2(json),
            _ => unreachable!("format_gate admits only known formats"),
        }
    }

    /// The lowest `format` that can represent this document's content —
    /// minimal stamping, exactly like [`crate::Map2dDoc::required_format`].
    #[must_use]
    pub fn required_format(&self) -> u32 {
        let object_grain = self.entries.iter().any(|entry| {
            matches!(entry.source, PatchSource::Path { .. })
                || entry.output.is_some()
                || entry.offset != 0
        });
        if object_grain {
            PATCH_FORMAT_OBJECT_GRAIN
        } else {
            PATCH_FORMAT_BASE
        }
    }

    /// Stamp [`Self::required_format`] onto the document.
    pub fn normalize_format(&mut self) {
        self.format = self.required_format();
    }

    /// Serialize compactly. The shape AND the written `format` follow
    /// [`Self::required_format`] — the writer cannot emit a stamp its shape
    /// contradicts.
    #[must_use]
    pub fn to_json(&self) -> String {
        match self.required_format() {
            PATCH_FORMAT_BASE => {
                serde_json::to_string(&v1_doc(self)).expect("patch doc serializes")
            }
            _ => serde_json::to_string(&v2_value(self)).expect("patch doc serializes"),
        }
    }

    /// Serialize for humans: 2-space indent, one entry per line (format-2
    /// rows stay single-line — that is the point of rows).
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        match self.required_format() {
            PATCH_FORMAT_BASE => {
                serde_json::to_string_pretty(&v1_doc(self)).expect("patch doc serializes")
            }
            _ => v2_pretty(self),
        }
    }
}

impl Default for PatchDoc {
    fn default() -> Self {
        Self::new()
    }
}

/// Stage one of [`PatchDoc::from_json`]: the `format` field alone.
#[derive(Deserialize)]
struct FormatPeek {
    format: u32,
}

/// The format gate, `supported` passed in so an old build's refusal stays
/// testable from inside the build that superseded it.
fn format_gate(found: u32, supported: u32) -> Result<(), PatchError> {
    if found == 0 || found > supported {
        return Err(PatchError::UnsupportedFormat { found, supported });
    }
    Ok(())
}

// ---- format 1: the verbose keyed shape ---------------------------------

#[derive(Serialize, Deserialize)]
struct V1Doc {
    format: u32,
    #[serde(default)]
    entries: Vec<V1Entry>,
}

#[derive(Serialize, Deserialize)]
struct V1Entry {
    range: V1Range,
    at: V1Anchor,
    #[serde(default, skip_serializing_if = "is_false")]
    reversed: bool,
    /// Reserved in format 1: parses, then refuses at resolve (see the
    /// module doc). The writer never emits it at this format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offset: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct V1Range {
    start: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    count: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct V1Anchor {
    channel: u32,
    /// Reserved in format 1, like `offset`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn parse_v1(json: &str) -> Result<PatchDoc, PatchError> {
    let doc: V1Doc = serde_json::from_str(json).map_err(|e| PatchError::Parse(e.to_string()))?;
    Ok(PatchDoc {
        format: doc.format,
        entries: doc
            .entries
            .into_iter()
            .map(|entry| PatchEntry {
                source: PatchSource::Range(PatchRange {
                    start: entry.range.start,
                    count: entry.range.count,
                }),
                output: entry.at.output,
                lamp: entry.at.channel,
                reversed: entry.reversed,
                offset: entry.offset.unwrap_or(0),
            })
            .collect(),
    })
}

/// The keyed shape for a format-1-expressible document. Callers checked
/// [`PatchDoc::required_format`] first; a format-2 construct here is a bug.
fn v1_doc(doc: &PatchDoc) -> V1Doc {
    V1Doc {
        format: PATCH_FORMAT_BASE,
        entries: doc
            .entries
            .iter()
            .map(|entry| {
                let PatchSource::Range(range) = &entry.source else {
                    unreachable!("required_format() said this doc is format-1-expressible");
                };
                V1Entry {
                    range: V1Range {
                        start: range.start,
                        count: range.count,
                    },
                    at: V1Anchor {
                        channel: entry.lamp,
                        output: None,
                    },
                    reversed: entry.reversed,
                    offset: None,
                }
            })
            .collect(),
    }
}

// ---- format 2: tuple rows + outputs header table -----------------------

fn parse_v2(json: &str) -> Result<PatchDoc, PatchError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| PatchError::Parse(e.to_string()))?;
    let outputs: Vec<String> = match value.get("outputs") {
        None => Vec::new(),
        Some(serde_json::Value::Array(names)) => names
            .iter()
            .map(|name| {
                name.as_str()
                    .map(ToString::to_string)
                    .ok_or_else(|| parse_error("outputs table entries must be strings"))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(parse_error("\"outputs\" must be an array of names")),
    };
    let rows = match value.get("entries") {
        None => &Vec::new(),
        Some(serde_json::Value::Array(rows)) => rows,
        Some(_) => return Err(parse_error("\"entries\" must be an array of rows")),
    };
    let entries = rows
        .iter()
        .enumerate()
        .map(|(index, row)| parse_v2_row(index, row, &outputs))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PatchDoc {
        format: PATCH_FORMAT_OBJECT_GRAIN,
        entries,
    })
}

/// One `[from, out#, lamp, flags?, offset?]` row.
fn parse_v2_row(
    index: usize,
    row: &serde_json::Value,
    outputs: &[String],
) -> Result<PatchEntry, PatchError> {
    let row_error = |reason: &str| parse_error(&alloc::format!("entry row {index}: {reason}"));
    let serde_json::Value::Array(columns) = row else {
        return Err(row_error(
            "a format-2 entry is a row array (format-1 keyed entries are not accepted at this format)",
        ));
    };
    if columns.len() < 3 || columns.len() > 5 {
        return Err(row_error(
            "rows carry 3 to 5 columns: [from, out#, lamp, flags?, offset?]",
        ));
    }
    let source = parse_v2_source(&columns[0]).map_err(|reason| row_error(&reason))?;
    let out_index = columns[1]
        .as_i64()
        .ok_or_else(|| row_error("out# must be an integer (-1 = default output)"))?;
    let output = if out_index == -1 {
        None
    } else {
        let name = usize::try_from(out_index)
            .ok()
            .and_then(|i| outputs.get(i))
            .ok_or_else(|| row_error("out# indexes past the outputs table"))?;
        Some(name.clone())
    };
    let lamp =
        parse_u32(&columns[2]).ok_or_else(|| row_error("lamp must be a non-negative integer"))?;
    let reversed = match columns.get(3) {
        None => false,
        Some(serde_json::Value::String(flags)) => match flags.as_str() {
            "" => false,
            "r" => true,
            other => {
                return Err(row_error(&alloc::format!(
                    "unknown flags {other:?} (only \"r\")"
                )));
            }
        },
        Some(_) => return Err(row_error("flags must be a string")),
    };
    let offset = match columns.get(4) {
        None => 0,
        Some(value) => {
            parse_u32(value).ok_or_else(|| row_error("offset must be a non-negative integer"))?
        }
    };
    Ok(PatchEntry {
        source,
        output,
        lamp,
        reversed,
        offset,
    })
}

/// The `from` column: a path string, `[path, start, count?]`, or
/// `[start, count?]`.
fn parse_v2_source(value: &serde_json::Value) -> Result<PatchSource, String> {
    match value {
        serde_json::Value::String(text) => Ok(PatchSource::Path {
            path: MapObjectPath::parse(text)?,
            range: None,
        }),
        serde_json::Value::Array(parts) => match parts.first() {
            Some(serde_json::Value::String(text)) => {
                let path = MapObjectPath::parse(text)?;
                let range = parse_source_range(&parts[1..]).ok_or_else(|| {
                    "a path sub-span is [path, start] or [path, start, count]".to_string()
                })?;
                Ok(PatchSource::Path {
                    path,
                    range: Some(range),
                })
            }
            Some(serde_json::Value::Number(_)) => {
                let range = parse_source_range(parts)
                    .ok_or_else(|| "a range source is [start] or [start, count]".to_string())?;
                Ok(PatchSource::Range(range))
            }
            _ => Err(
                "from must be a path string, [path, start, count?], or [start, count?]".to_string(),
            ),
        },
        _ => Err("from must be a path string or an array".to_string()),
    }
}

/// `[start]` (to the end) or `[start, count]`.
fn parse_source_range(parts: &[serde_json::Value]) -> Option<PatchRange> {
    match parts {
        [start] => Some(PatchRange {
            start: parse_u32(start)?,
            count: None,
        }),
        [start, count] => Some(PatchRange {
            start: parse_u32(start)?,
            count: Some(parse_u32(count)?),
        }),
        _ => None,
    }
}

fn parse_u32(value: &serde_json::Value) -> Option<u32> {
    value.as_u64().and_then(|v| u32::try_from(v).ok())
}

fn parse_error(reason: &str) -> PatchError {
    PatchError::Parse(reason.to_string())
}

/// The row form of a document. The outputs table lists names in first-use
/// order; rows reference it by index (−1 = default).
fn v2_value(doc: &PatchDoc) -> serde_json::Value {
    let (outputs, rows) = v2_parts(doc);
    let mut object = serde_json::Map::new();
    object.insert("format".into(), PATCH_FORMAT_OBJECT_GRAIN.into());
    if !outputs.is_empty() {
        object.insert("outputs".into(), outputs.into());
    }
    object.insert("entries".into(), serde_json::Value::Array(rows));
    serde_json::Value::Object(object)
}

fn v2_parts(doc: &PatchDoc) -> (Vec<String>, Vec<serde_json::Value>) {
    let mut outputs: Vec<String> = Vec::new();
    let mut rows = Vec::with_capacity(doc.entries.len());
    for entry in &doc.entries {
        let out_index: i64 = match &entry.output {
            None => -1,
            Some(name) => {
                let index = outputs
                    .iter()
                    .position(|known| known == name)
                    .unwrap_or_else(|| {
                        outputs.push(name.clone());
                        outputs.len() - 1
                    });
                index as i64
            }
        };
        let from = match &entry.source {
            PatchSource::Path { path, range: None } => serde_json::Value::String(path.to_text()),
            PatchSource::Path {
                path,
                range: Some(range),
            } => {
                let mut parts = alloc::vec![serde_json::Value::String(path.to_text())];
                push_range(&mut parts, range);
                serde_json::Value::Array(parts)
            }
            PatchSource::Range(range) => {
                let mut parts = Vec::new();
                push_range(&mut parts, range);
                serde_json::Value::Array(parts)
            }
        };
        let mut row = alloc::vec![from, out_index.into(), entry.lamp.into()];
        if entry.reversed || entry.offset != 0 {
            row.push(serde_json::Value::String(
                if entry.reversed { "r" } else { "" }.to_string(),
            ));
        }
        if entry.offset != 0 {
            row.push(entry.offset.into());
        }
        rows.push(serde_json::Value::Array(row));
    }
    (outputs, rows)
}

fn push_range(parts: &mut Vec<serde_json::Value>, range: &PatchRange) {
    parts.push(range.start.into());
    if let Some(count) = range.count {
        parts.push(count.into());
    }
}

/// The canonical pretty form: 2-space indent, `outputs` inline, one row
/// per line. Hand-authored documents target exactly this.
fn v2_pretty(doc: &PatchDoc) -> String {
    let (outputs, rows) = v2_parts(doc);
    let mut text = String::from("{\n  \"format\": 2,\n");
    if !outputs.is_empty() {
        text.push_str("  \"outputs\": ");
        text.push_str(&serde_json::to_string(&outputs).expect("outputs table serializes"));
        text.push_str(",\n");
    }
    if rows.is_empty() {
        text.push_str("  \"entries\": []\n}");
        return text;
    }
    text.push_str("  \"entries\": [\n");
    for (index, row) in rows.iter().enumerate() {
        text.push_str("    ");
        text.push_str(&serde_json::to_string(row).expect("row serializes"));
        text.push_str(if index + 1 < rows.len() { ",\n" } else { "\n" });
    }
    text.push_str("  ]\n}");
    text
}

// ---- resolution --------------------------------------------------------

/// Resolve a patch against a fixture.
///
/// Returns every lamp of the fixture exactly once: the document's entries
/// in authoring order, then the lamps no entry claimed, flowing on the
/// default output in fixture order from the highest default-anchored wire
/// end. An empty document therefore resolves to the single full-length run
/// at wire lamp 0 that auto-flow would have produced.
///
/// The format gate is [`PatchDoc::from_json`]'s job, not this function's —
/// a document that got this far was read by a build that understands it.
pub fn resolve_patch(
    ctx: &PatchResolveContext<'_>,
    doc: &PatchDoc,
) -> Result<Vec<PatchedRange>, PatchError> {
    let mut anchored: Vec<PatchedRange> = Vec::with_capacity(doc.entries.len());

    for (index, entry) in doc.entries.iter().enumerate() {
        let index = index as u32;
        let range = resolve_entry(index, entry, doc.format, ctx)?;
        for (other_index, other) in anchored.iter().enumerate() {
            // A fixture lamp exists once — claiming it twice is an error on
            // ANY pair of outputs.
            if let Some(lamp) = overlap_start(range.start, range.end(), other.start, other.end()) {
                return Err(PatchError::LampClaimedTwice {
                    entry: index,
                    other: other_index as u32,
                    lamp,
                });
            }
            // Wire lamps are per-output: two runs collide only on one wire.
            if output_group(ctx, &range.output) == output_group(ctx, &other.output)
                && let Some(lamp) =
                    overlap_start(range.lamp, range.lamp_end(), other.lamp, other.lamp_end())
            {
                return Err(PatchError::WireLampClaimedTwice {
                    entry: index,
                    other: other_index as u32,
                    lamp,
                });
            }
        }
        anchored.push(range);
    }

    let mut resolved = anchored.clone();

    // Reflow: whatever the entries left over, in fixture order, on the
    // DEFAULT output, starting past every default-group anchor. Sorting a
    // COPY keeps `resolved` in authoring order — the order an editor lists
    // rows in — while the walk below needs lamp order.
    let mut claimed = anchored;
    claimed.sort_unstable_by_key(|range| range.start);
    let mut cursor = resolved
        .iter()
        .filter(|range| output_group(ctx, &range.output).is_none())
        .map(PatchedRange::lamp_end)
        .max()
        .unwrap_or(0);
    let mut lamp = 0u32;
    let sentinel = PatchedRange {
        // Past the end, so the tail after the last anchor is walked by the
        // same arm as every interior hole.
        start: ctx.fixture_lamp_count,
        count: 0,
        lamp: 0,
        reversed: false,
        offset: 0,
        output: None,
    };
    for range in claimed.iter().chain(core::iter::once(&sentinel)) {
        if range.start > lamp {
            let count = range.start - lamp;
            resolved.push(PatchedRange {
                start: lamp,
                count,
                lamp: cursor,
                reversed: false,
                offset: 0,
                output: None,
            });
            cursor = cursor.saturating_add(count);
        }
        lamp = lamp.max(range.end());
    }

    Ok(resolved)
}

/// The collision/reflow group an output reference belongs to: `None` = the
/// default output's wire (including a name the context says IS the
/// default).
fn output_group<'e>(ctx: &PatchResolveContext<'_>, output: &'e Option<String>) -> Option<&'e str> {
    match output {
        None => None,
        Some(name) if ctx.default_output == Some(name.as_str()) => None,
        Some(name) => Some(name.as_str()),
    }
}

/// One entry's fixture-relative run, lowered and validated.
fn resolve_entry(
    index: u32,
    entry: &PatchEntry,
    doc_format: u32,
    ctx: &PatchResolveContext<'_>,
) -> Result<PatchedRange, PatchError> {
    let locator = || match &entry.source {
        PatchSource::Path { path, .. } => Some(path.to_text()),
        PatchSource::Range(_) => None,
    };
    // Format-1 reserved fields: parsed, never applied — the honest refusal
    // names the fix.
    if doc_format == PATCH_FORMAT_BASE {
        if let Some(output) = &entry.output {
            return Err(PatchError::InvalidEntry {
                entry: index,
                from: locator(),
                reason: alloc::format!(
                    "names output {output:?}, which format 1 cannot express — stamp the \
                     document format 2 to place runs on named outputs"
                ),
            });
        }
        if entry.offset != 0 {
            return Err(PatchError::InvalidEntry {
                entry: index,
                from: locator(),
                reason: String::from(
                    "carries a rotation offset, which format 1 cannot express — stamp the \
                     document format 2 to rotate runs",
                ),
            });
        }
    }
    if let (Some(allowed), Some(name)) = (ctx.allowed_outputs, &entry.output)
        && !allowed.iter().any(|known| known == name)
    {
        return Err(PatchError::DanglingOutput {
            entry: index,
            name: name.clone(),
        });
    }

    let (start, count) = lower_source(index, entry, ctx)?;
    Ok(PatchedRange {
        start,
        count,
        lamp: entry.lamp,
        reversed: entry.reversed,
        offset: if count > 0 { entry.offset % count } else { 0 },
        output: entry.output.clone(),
    })
}

/// A source's flat fixture-relative `(start, count)`.
fn lower_source(
    index: u32,
    entry: &PatchEntry,
    ctx: &PatchResolveContext<'_>,
) -> Result<(u32, u32), PatchError> {
    match &entry.source {
        PatchSource::Range(range) => {
            let (start, count) =
                clamp_range(index, None, range, 0, ctx.fixture_lamp_count, "the fixture")?;
            Ok((start, count))
        }
        PatchSource::Path { path, range } => {
            // The addressed node's lamps: every strand whose object id
            // matches and whose instance path extends the addressed one.
            let matching: Vec<&ObjectInstanceSpan> = ctx
                .object_spans
                .iter()
                .filter(|span| {
                    span.id.as_ref() == Some(&path.id)
                        && span.instances.len() >= path.instances.len()
                        && span.instances[..path.instances.len()] == path.instances[..]
                })
                .collect();
            let (Some(first), Some(_last)) = (matching.first(), matching.last()) else {
                return Err(PatchError::UnknownPath {
                    entry: index,
                    path: path.to_text(),
                });
            };
            // An object's strands are contiguous in wiring order, and a
            // prefix-matched instance run is consecutive within them.
            let node_start = first.start;
            let node_count: u32 = matching.iter().map(|span| span.count).sum();
            match range {
                None => Ok((node_start, node_count)),
                Some(range) => clamp_range(
                    index,
                    Some(path.to_text()),
                    range,
                    node_start,
                    node_count,
                    "the addressed node",
                ),
            }
        }
    }
}

/// Bounds-check a range against a node of `extent` lamps starting at
/// `base`, returning the absolute `(start, count)`.
fn clamp_range(
    index: u32,
    from: Option<String>,
    range: &PatchRange,
    base: u32,
    extent: u32,
    what: &str,
) -> Result<(u32, u32), PatchError> {
    let invalid = |reason: String| PatchError::InvalidEntry {
        entry: index,
        from: from.clone(),
        reason,
    };
    if range.start >= extent {
        return Err(invalid(alloc::format!(
            "starts at lamp {} but {what} has {extent}",
            range.start
        )));
    }
    let remaining = extent - range.start;
    let count = range.count.unwrap_or(remaining);
    if count == 0 {
        return Err(invalid(String::from(
            "places zero lamps; delete the entry instead",
        )));
    }
    if count > remaining {
        return Err(invalid(alloc::format!(
            "places {count} lamps from lamp {}, past {what}'s {extent}",
            range.start
        )));
    }
    Ok((base + range.start, count))
}

/// First index two half-open ranges share, if any.
fn overlap_start(start: u32, end: u32, other_start: u32, other_end: u32) -> Option<u32> {
    let first = start.max(other_start);
    (first < end.min(other_end)).then_some(first)
}

/// The wire lamp source lamp `j` of a resolved run occupies — the one
/// place the rotation/orientation arithmetic lives (the engine's sample
/// math and any UI preview call this, never re-derive it).
#[must_use]
pub fn patched_wire_lamp(range: &PatchedRange, j: u32) -> u32 {
    debug_assert!(j < range.count);
    let oriented = if range.reversed {
        range.count - 1 - j
    } else {
        j
    };
    range.lamp + ((oriented + range.offset) % range.count.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map2d_object_id::Map2dObjectId;
    use alloc::vec;

    fn entry(start: u32, count: Option<u32>, lamp: u32, reversed: bool) -> PatchEntry {
        PatchEntry {
            source: PatchSource::Range(PatchRange { start, count }),
            output: None,
            lamp,
            reversed,
            offset: 0,
        }
    }

    fn path_entry(path: &str, lamp: u32) -> PatchEntry {
        PatchEntry {
            source: PatchSource::Path {
                path: MapObjectPath::parse(path).unwrap(),
                range: None,
            },
            output: None,
            lamp,
            reversed: false,
            offset: 0,
        }
    }

    fn doc(entries: Vec<PatchEntry>) -> PatchDoc {
        let mut doc = PatchDoc {
            format: PATCH_FORMAT_BASE,
            entries,
        };
        doc.normalize_format();
        doc
    }

    fn range(start: u32, count: u32, lamp: u32, reversed: bool) -> PatchedRange {
        PatchedRange {
            start,
            count,
            lamp,
            reversed,
            offset: 0,
            output: None,
        }
    }

    fn resolve_bare(count: u32, doc: &PatchDoc) -> Result<Vec<PatchedRange>, PatchError> {
        resolve_patch(&PatchResolveContext::for_fixture(count), doc)
    }

    /// The mini-dome-shaped span table: `sector` = 5 instances of 30, then
    /// `door` = 3 instances of 9 (fixture totals 150 + 27 = 177 lamps).
    fn dome_spans() -> Vec<ObjectInstanceSpan> {
        let mut spans = Vec::new();
        for instance in 0..5u32 {
            spans.push(ObjectInstanceSpan {
                id: Some(Map2dObjectId::new("sector").unwrap()),
                instances: vec![instance],
                start: instance * 30,
                count: 30,
            });
        }
        for instance in 0..3u32 {
            spans.push(ObjectInstanceSpan {
                id: Some(Map2dObjectId::new("door").unwrap()),
                instances: vec![instance],
                start: 150 + instance * 9,
                count: 9,
            });
        }
        spans
    }

    // ---- format-1 behavior, unchanged ----------------------------------

    #[test]
    fn an_empty_patch_resolves_to_the_whole_fixture_at_wire_lamp_zero() {
        assert_eq!(
            resolve_bare(24, &PatchDoc::new()).unwrap(),
            vec![range(0, 24, 0, false)]
        );
    }

    #[test]
    fn a_zero_lamp_fixture_resolves_to_nothing() {
        assert_eq!(resolve_bare(0, &PatchDoc::new()).unwrap(), vec![]);
    }

    /// The peach: two halves of one strand, the second one plugged in
    /// backwards twelve lamps further down the wire.
    #[test]
    fn entries_anchor_where_they_say() {
        let resolved = resolve_bare(
            44,
            &doc(vec![
                entry(0, Some(22), 0, false),
                entry(22, Some(22), 34, true),
            ]),
        )
        .unwrap();
        assert_eq!(
            resolved,
            vec![range(0, 22, 0, false), range(22, 22, 34, true)]
        );
    }

    #[test]
    fn unanchored_lamps_flow_after_the_highest_anchored_end() {
        let resolved = resolve_bare(10, &doc(vec![entry(0, Some(4), 20, false)])).unwrap();
        assert_eq!(
            resolved,
            vec![range(0, 4, 20, false), range(4, 6, 24, false)],
            "lamps 4..10 pick up where the anchor's wire run ended"
        );
    }

    #[test]
    fn reflow_starts_past_every_anchor_not_just_the_last_one() {
        let resolved = resolve_bare(
            12,
            &doc(vec![
                entry(8, Some(4), 100, false),
                entry(0, Some(2), 4, false),
            ]),
        )
        .unwrap();
        assert_eq!(
            resolved,
            vec![
                range(8, 4, 100, false),
                range(0, 2, 4, false),
                range(2, 6, 104, false),
            ],
        );
    }

    #[test]
    fn several_holes_reflow_in_fixture_order() {
        let resolved = resolve_bare(
            20,
            &doc(vec![
                entry(4, Some(4), 0, false),
                entry(12, Some(4), 4, false),
            ]),
        )
        .unwrap();
        assert_eq!(
            resolved,
            vec![
                range(4, 4, 0, false),
                range(12, 4, 4, false),
                range(0, 4, 8, false),
                range(8, 4, 12, false),
                range(16, 4, 16, false),
            ]
        );
    }

    #[test]
    fn an_omitted_count_runs_to_the_end_of_the_fixture() {
        assert_eq!(
            resolve_bare(9, &doc(vec![entry(3, None, 50, false)])).unwrap(),
            vec![range(3, 6, 50, false), range(0, 3, 56, false)]
        );
        assert_eq!(
            resolve_bare(12, &doc(vec![entry(3, None, 50, false)])).unwrap()[0],
            range(3, 9, 50, false),
            "the same document over a longer fixture patches the longer tail",
        );
    }

    #[test]
    fn reversed_rides_through_and_reflow_is_never_reversed() {
        let resolved = resolve_bare(8, &doc(vec![entry(0, Some(4), 0, true)])).unwrap();
        assert!(resolved[0].reversed);
        assert!(!resolved[1].reversed);
    }

    #[test]
    fn two_entries_claiming_one_lamp_are_a_document_error() {
        assert_eq!(
            resolve_bare(
                10,
                &doc(vec![
                    entry(0, Some(6), 0, false),
                    entry(4, Some(6), 100, false)
                ])
            ),
            Err(PatchError::LampClaimedTwice {
                entry: 1,
                other: 0,
                lamp: 4
            })
        );
    }

    #[test]
    fn two_entries_landing_on_one_wire_lamp_are_a_document_error() {
        assert_eq!(
            resolve_bare(
                10,
                &doc(vec![
                    entry(0, Some(5), 0, false),
                    entry(5, Some(5), 4, false)
                ])
            ),
            Err(PatchError::WireLampClaimedTwice {
                entry: 1,
                other: 0,
                lamp: 4
            })
        );
    }

    #[test]
    fn abutting_entries_are_fine() {
        assert!(
            resolve_bare(
                10,
                &doc(vec![
                    entry(0, Some(5), 0, false),
                    entry(5, Some(5), 5, false)
                ])
            )
            .is_ok()
        );
    }

    #[test]
    fn out_of_bounds_and_zero_ranges_are_refused() {
        for bad in [
            entry(0, Some(5), 0, false), // past the end of a 4-lamp fixture
            entry(4, Some(1), 0, false), // starts past the end
            entry(0, Some(0), 0, false), // zero lamps
        ] {
            let error = resolve_bare(4, &doc(vec![bad.clone()])).unwrap_err();
            assert!(
                matches!(error, PatchError::InvalidEntry { entry: 0, .. }),
                "{error}"
            );
        }
    }

    /// Format 1's reserved constructs refuse at resolve with the fix named:
    /// the document parses (an old file must never look corrupt), but
    /// applying what format 1 cannot express would move light silently.
    #[test]
    fn format_one_reserved_constructs_refuse_naming_the_fix() {
        let mut with_output = doc(vec![entry(0, Some(4), 0, false)]);
        with_output.entries[0].output = Some(String::from("Box 2"));
        with_output.format = PATCH_FORMAT_BASE; // as parsed from a v1 file
        let error = resolve_bare(4, &with_output).unwrap_err();
        assert!(
            error.to_string().contains("stamp the document format 2"),
            "{error}"
        );

        let mut with_offset = doc(vec![entry(0, Some(4), 0, false)]);
        with_offset.entries[0].offset = 3;
        with_offset.format = PATCH_FORMAT_BASE;
        let error = resolve_bare(4, &with_offset).unwrap_err();
        assert!(error.to_string().contains("rotation"), "{error}");
    }

    #[test]
    fn a_newer_format_is_refused_whole_before_the_parse() {
        assert_eq!(
            PatchDoc::from_json(r#"{"format":3,"entries":[]}"#),
            Err(PatchError::UnsupportedFormat {
                found: 3,
                supported: PATCH_FORMAT
            })
        );
        assert_eq!(
            PatchDoc::from_json(r#"{"format":0,"entries":[]}"#),
            Err(PatchError::UnsupportedFormat {
                found: 0,
                supported: PATCH_FORMAT
            })
        );
        assert_eq!(
            PatchDoc::from_json(r#"{"format":7,"entries":[{"whatever":true}]}"#),
            Err(PatchError::UnsupportedFormat {
                found: 7,
                supported: PATCH_FORMAT
            })
        );
    }

    #[test]
    fn a_broken_document_still_reports_as_broken() {
        assert!(matches!(
            PatchDoc::from_json("{"),
            Err(PatchError::Parse(_))
        ));
        assert!(matches!(
            PatchDoc::from_json(r#"{"entries":[]}"#),
            Err(PatchError::Parse(_))
        ));
    }

    /// The shipped peach bytes, pinned VERBATIM: they parse, resolve as
    /// they always did, and the writer reproduces the same document at
    /// format 1 (minimal stamping keeps old builds reading them).
    #[test]
    fn the_shipped_peach_document_parses_resolves_and_stays_format_one() {
        let text = r#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 22 }, "at": { "channel": 0 } },
    { "range": { "start": 22, "count": 22 }, "at": { "channel": 34 }, "reversed": true }
  ]
}"#;
        let doc = PatchDoc::from_json(text).unwrap();
        assert_eq!(doc.format, 1);
        assert_eq!(doc.required_format(), 1);
        assert_eq!(
            resolve_bare(44, &doc).unwrap(),
            vec![range(0, 22, 0, false), range(22, 22, 34, true)]
        );
        // The canonical writer emits the same document, format 1, with
        // neither reserved field and no `reversed: false` noise.
        let written = doc.to_json();
        assert!(written.contains("\"format\":1"), "{written}");
        assert!(!written.contains("output"), "{written}");
        assert!(!written.contains("offset"), "{written}");
        assert_eq!(PatchDoc::from_json(&written).unwrap(), doc);
    }

    // ---- format 2: parsing + writing -----------------------------------

    /// The module-doc example document, every row form exercised.
    #[test]
    fn format_two_rows_parse_into_the_shared_model() {
        let text = r#"{
          "format": 2,
          "outputs": ["1", "Box 2"],
          "entries": [
            ["/sector/0", 0, 78],
            ["/sector/1", 1, 0, "r"],
            ["/sector/2", -1, 0, "", 10],
            [["/door/1", 0, 3], 0, 36],
            [[0, 22], -1, 0],
            [[22], -1, 30]
          ]
        }"#;
        let doc = PatchDoc::from_json(text).unwrap();
        assert_eq!(doc.format, 2);
        assert_eq!(doc.entries.len(), 6);
        assert_eq!(doc.entries[0].output.as_deref(), Some("1"));
        assert_eq!(doc.entries[0].lamp, 78);
        assert_eq!(doc.entries[1].output.as_deref(), Some("Box 2"));
        assert!(doc.entries[1].reversed);
        assert_eq!(doc.entries[2].output, None);
        assert_eq!(doc.entries[2].offset, 10);
        assert!(matches!(
            &doc.entries[3].source,
            PatchSource::Path {
                range: Some(PatchRange {
                    start: 0,
                    count: Some(3)
                }),
                ..
            }
        ));
        assert!(matches!(
            &doc.entries[4].source,
            PatchSource::Range(PatchRange {
                start: 0,
                count: Some(22)
            })
        ));
        assert!(matches!(
            &doc.entries[5].source,
            PatchSource::Range(PatchRange {
                start: 22,
                count: None
            })
        ));
    }

    /// ONE canonical shape per format: keyed entries at format 2 (and rows
    /// at format 1) are parse errors, never leniently accepted.
    #[test]
    fn cross_format_shapes_are_parse_errors() {
        let keyed_at_two = r#"{"format":2,"entries":[
            {"range":{"start":0,"count":4},"at":{"channel":0}}
        ]}"#;
        assert!(matches!(
            PatchDoc::from_json(keyed_at_two),
            Err(PatchError::Parse(reason)) if reason.contains("row")
        ));
        let rows_at_one = r#"{"format":1,"entries":[["/sector",0,0]]}"#;
        assert!(matches!(
            PatchDoc::from_json(rows_at_one),
            Err(PatchError::Parse(_))
        ));
    }

    #[test]
    fn malformed_rows_report_their_index_and_rule() {
        for (text, needle) in [
            (r#"{"format":2,"entries":[["/sector"]]}"#, "3 to 5"),
            (r#"{"format":2,"entries":[["/sector",0]]}"#, "3 to 5"),
            (
                r#"{"format":2,"entries":[["/sector",2,0]]}"#,
                "outputs table",
            ),
            (
                r#"{"format":2,"entries":[["sector",-1,0]]}"#,
                "must start with '/'",
            ),
            (r#"{"format":2,"entries":[["/sector",-1,0,"x"]]}"#, "flags"),
            (r#"{"format":2,"outputs":[1],"entries":[]}"#, "strings"),
            (
                r#"{"format":2,"entries":[[["/s",0,1,2],-1,0]]}"#,
                "sub-span",
            ),
        ] {
            let error = PatchDoc::from_json(text).unwrap_err();
            assert!(
                matches!(&error, PatchError::Parse(reason) if reason.contains(needle)),
                "{text} → {error}"
            );
        }
    }

    /// Write → parse is the identity, and the bytes are stable across a
    /// second round trip (the canonical-form contract).
    #[test]
    fn format_two_documents_round_trip_byte_stably() {
        let mut original = doc(vec![
            path_entry("/sector/0", 78),
            path_entry("/sector/2", 0),
            entry(0, Some(22), 0, false),
            entry(22, None, 30, true),
        ]);
        original.entries[0].output = Some(String::from("1"));
        original.entries[1].offset = 10;
        original.entries[3].output = Some(String::from("Box 2"));
        original.normalize_format();
        assert_eq!(original.required_format(), 2);

        for text in [original.to_json(), original.to_json_pretty()] {
            let parsed = PatchDoc::from_json(&text).unwrap();
            assert_eq!(parsed, original, "{text}");
            assert_eq!(parsed.to_json(), original.to_json());
        }
        // Pretty rows stay one line each: `{`, format, outputs, `"entries": [`,
        // one line per row, `]`, `}`.
        let pretty = original.to_json_pretty();
        assert_eq!(
            pretty.lines().count(),
            6 + original.entries.len(),
            "{pretty}"
        );
    }

    /// Minimal stamping: a doc whose last format-2 construct is removed
    /// writes as format 1 again, and the writer's stamp always matches its
    /// shape.
    #[test]
    fn the_writer_stamps_minimally_and_never_lies() {
        let mut doc = doc(vec![path_entry("/sector", 0)]);
        assert_eq!(doc.required_format(), 2);
        assert!(doc.to_json().contains("\"format\":2"));

        doc.entries[0] = entry(0, Some(4), 0, true);
        doc.normalize_format();
        assert_eq!(doc.format, 1);
        let written = doc.to_json();
        assert!(written.contains("\"format\":1"), "{written}");
        assert!(written.contains("\"range\""), "{written}");
        assert_eq!(PatchDoc::from_json(&written).unwrap(), doc);
    }

    /// The size pin (D47, wire-fit A1 pattern): a radiance-scale synthetic
    /// patch — 200 rows over 10 outputs — serializes ≤ 8 KB, so the format
    /// never silently regresses toward the keyed bloat it replaced.
    #[test]
    fn a_radiance_scale_patch_stays_within_the_size_budget() {
        let mut entries = Vec::new();
        for panel in 0..200u32 {
            let mut e = path_entry(&alloc::format!("/panel/{panel}"), (panel % 34) * 12);
            e.output = Some(alloc::format!("Box {}", panel % 10 + 1));
            e.reversed = panel % 3 == 0;
            e.offset = if panel % 4 == 0 { 3 } else { 0 };
            entries.push(e);
        }
        let doc = doc(entries);
        let bytes = doc.to_json().len();
        assert!(
            bytes <= 8 * 1024,
            "200-entry patch serialized to {bytes} bytes"
        );
    }

    // ---- path lowering --------------------------------------------------

    /// `/sector/2` is instance 2's lamps WHEREVER they currently are — the
    /// D46 robustness property: boundaries re-derive from the span table,
    /// so the same document survives the dome growing.
    #[test]
    fn path_entries_lower_through_the_span_table() {
        let spans = dome_spans();
        let ctx = PatchResolveContext {
            fixture_lamp_count: 177,
            object_spans: &spans,
            ..Default::default()
        };
        let resolved = resolve_patch(&ctx, &doc(vec![path_entry("/sector/2", 40)])).unwrap();
        assert_eq!(resolved[0].start, 60);
        assert_eq!(resolved[0].count, 30);
        assert_eq!(resolved[0].lamp, 40);

        // The dome grows: 40 lamps per sector now. Same document, new
        // table — the entry follows the instance, not the numbers.
        let mut grown = Vec::new();
        for instance in 0..5u32 {
            grown.push(ObjectInstanceSpan {
                id: Some(Map2dObjectId::new("sector").unwrap()),
                instances: vec![instance],
                start: instance * 40,
                count: 40,
            });
        }
        let ctx = PatchResolveContext {
            fixture_lamp_count: 200,
            object_spans: &grown,
            ..Default::default()
        };
        let resolved = resolve_patch(&ctx, &doc(vec![path_entry("/sector/2", 40)])).unwrap();
        assert_eq!((resolved[0].start, resolved[0].count), (80, 40));
    }

    /// A whole-object path takes every instance; a sub-range refines
    /// node-relative lamps; unknown ids and out-of-range instances refuse
    /// with the path named.
    #[test]
    fn whole_object_subrange_and_unknown_paths() {
        let spans = dome_spans();
        let ctx = PatchResolveContext {
            fixture_lamp_count: 177,
            object_spans: &spans,
            ..Default::default()
        };

        let resolved = resolve_patch(&ctx, &doc(vec![path_entry("/door", 100)])).unwrap();
        assert_eq!((resolved[0].start, resolved[0].count), (150, 27));

        let mut sub = path_entry("/door/1", 0);
        sub.source = PatchSource::Path {
            path: MapObjectPath::parse("/door/1").unwrap(),
            range: Some(PatchRange {
                start: 3,
                count: Some(3),
            }),
        };
        let resolved = resolve_patch(&ctx, &doc(vec![sub])).unwrap();
        assert_eq!((resolved[0].start, resolved[0].count), (162, 3));

        for path in ["/roof", "/sector/5", "/door/1/0"] {
            let error = resolve_patch(&ctx, &doc(vec![path_entry(path, 0)])).unwrap_err();
            assert!(
                matches!(&error, PatchError::UnknownPath { entry: 0, path: p } if p == path),
                "{path} → {error}"
            );
        }

        // A sub-range past the node's extent refuses naming the node.
        let mut over = path_entry("/door/1", 0);
        over.source = PatchSource::Path {
            path: MapObjectPath::parse("/door/1").unwrap(),
            range: Some(PatchRange {
                start: 0,
                count: Some(10),
            }),
        };
        let error = resolve_patch(&ctx, &doc(vec![over])).unwrap_err();
        assert!(
            matches!(&error, PatchError::InvalidEntry { from: Some(from), .. } if from == "/door/1"),
            "{error}"
        );
    }

    // ---- outputs: grouping, reflow, dangling ---------------------------

    /// Wire collision is PER-OUTPUT: the same wire lamp on two outputs is
    /// two different physical wires.
    #[test]
    fn wire_overlap_on_different_outputs_is_not_a_collision() {
        let mut a = entry(0, Some(4), 0, false);
        let mut b = entry(4, Some(4), 0, false);
        a.output = Some(String::from("1"));
        b.output = Some(String::from("Box 2"));
        assert!(resolve_bare(8, &doc(vec![a.clone(), b.clone()])).is_ok());

        // Same output name: collision again.
        b.output = Some(String::from("1"));
        assert!(matches!(
            resolve_bare(8, &doc(vec![a, b])),
            Err(PatchError::WireLampClaimedTwice { .. })
        ));
    }

    /// Reflow is the DEFAULT output's story: runs anchored on named
    /// outputs neither push nor collide with the default cursor — and a
    /// name the context identifies as the default folds into it.
    #[test]
    fn reflow_follows_the_default_output_only() {
        let mut named = entry(0, Some(4), 50, false);
        named.output = Some(String::from("Box 2"));
        let resolved = resolve_bare(8, &doc(vec![named.clone()])).unwrap();
        assert_eq!(
            resolved[1],
            range(4, 4, 0, false),
            "the remainder flows at wire lamp 0 on the default output, not past Box 2's anchor"
        );

        // Same doc, but the context says "Box 2" IS the default: now the
        // anchor owns the default wire and reflow follows it.
        let ctx = PatchResolveContext {
            fixture_lamp_count: 8,
            default_output: Some("Box 2"),
            ..Default::default()
        };
        let resolved = resolve_patch(&ctx, &doc(vec![named])).unwrap();
        assert_eq!(resolved[1].lamp, 54);
    }

    /// The kernel's dangling check is opt-in: with an allowed set, a
    /// stranger name refuses; without one, the engine owns the question.
    #[test]
    fn dangling_output_names_refuse_only_against_an_allowed_set() {
        let mut e = entry(0, Some(4), 0, false);
        e.output = Some(String::from("Box 9"));
        let d = doc(vec![e]);
        assert!(resolve_bare(4, &d).is_ok());

        let allowed = vec![String::from("1"), String::from("Box 2")];
        let ctx = PatchResolveContext {
            fixture_lamp_count: 4,
            allowed_outputs: Some(&allowed),
            ..Default::default()
        };
        assert_eq!(
            resolve_patch(&ctx, &d),
            Err(PatchError::DanglingOutput {
                entry: 0,
                name: String::from("Box 9")
            })
        );
    }

    // ---- rotation -------------------------------------------------------

    /// The kernel-owned definition, worked example from the module doc:
    /// N = 5, anchor 10, offset 2.
    #[test]
    fn rotation_maps_the_worked_example_exactly() {
        let run = PatchedRange {
            start: 0,
            count: 5,
            lamp: 10,
            reversed: false,
            offset: 2,
            output: None,
        };
        let wires: Vec<u32> = (0..5).map(|j| patched_wire_lamp(&run, j)).collect();
        assert_eq!(wires, vec![12, 13, 14, 10, 11]);

        let reversed = PatchedRange {
            reversed: true,
            ..run
        };
        let wires: Vec<u32> = (0..5).map(|j| patched_wire_lamp(&reversed, j)).collect();
        assert_eq!(wires, vec![11, 10, 14, 13, 12]);
    }

    /// The composition order is a DEFINITION: reversed-then-rotated, one
    /// canonical order, property-tested at small N — every (N, k, j) stays
    /// inside the window and the map is a bijection.
    #[test]
    fn rotation_composed_with_reversal_is_a_bijection_within_the_window() {
        for count in 1..8u32 {
            for offset in 0..count {
                for reversed in [false, true] {
                    let run = PatchedRange {
                        start: 0,
                        count,
                        lamp: 100,
                        reversed,
                        offset,
                        output: None,
                    };
                    let mut seen = vec![false; count as usize];
                    for j in 0..count {
                        let wire = patched_wire_lamp(&run, j);
                        assert!(wire >= 100 && wire < 100 + count);
                        let slot = (wire - 100) as usize;
                        assert!(
                            !seen[slot],
                            "N={count} k={offset} r={reversed}: wire {wire} hit twice"
                        );
                        seen[slot] = true;
                    }
                }
            }
        }
    }

    /// Offsets are stored mod N at resolve, so `offset = N` behaves as 0
    /// and UIs can step without wrap bookkeeping.
    #[test]
    fn offsets_normalize_modulo_the_run_length() {
        let mut e = entry(0, Some(6), 0, false);
        e.offset = 6;
        let mut d = doc(vec![e]);
        d.normalize_format();
        let resolved = resolve_bare(6, &d).unwrap();
        assert_eq!(resolved[0].offset, 0);
    }

    // ---- span-table agreement ------------------------------------------

    /// The instance-span table this kernel lowers through matches the
    /// resolver: strand order, ids, instance addresses.
    #[test]
    fn object_instance_spans_agree_with_the_resolver() {
        use crate::map2d_doc::{Map2dDoc, Map2dObject, Map2dShape, PathShape, RepeatShape};
        use crate::map2d_resolve::{object_instance_spans, resolve};
        use alloc::boxed::Box;

        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "sector".to_string(),
            id: Some(Map2dObjectId::new("sector").unwrap()),
            stride: None,
            shape: Map2dShape::Repeat(RepeatShape {
                shape: Box::new(Map2dShape::Path(PathShape {
                    points: vec![[0.0, -10.0], [0.0, -40.0]],
                    count: 4,
                    reversed: false,
                    gaps: Vec::new(),
                })),
                center: [0.0, 0.0],
                count: 3,
            }),
        });
        doc.objects.push(Map2dObject {
            name: "strip".to_string(),
            id: None,
            stride: None,
            shape: Map2dShape::Path(PathShape {
                points: vec![[0.0, 0.0], [10.0, 0.0]],
                count: 5,
                reversed: false,
                gaps: Vec::new(),
            }),
        });
        let resolved = resolve(&doc).unwrap();
        let spans = object_instance_spans(&doc, &resolved);
        assert_eq!(spans.len(), 4);
        for (instance, span) in spans[..3].iter().enumerate() {
            assert_eq!(span.id.as_ref().unwrap().as_str(), "sector");
            assert_eq!(span.instances, vec![instance as u32]);
            assert_eq!(span.start, instance as u32 * 4);
            assert_eq!(span.count, 4);
        }
        assert_eq!(spans[3].id, None, "no ensure-ids, no address");
        assert_eq!(spans[3].instances, Vec::<u32>::new());
        assert_eq!((spans[3].start, spans[3].count), (12, 5));

        // And the kernel resolves a path through the real table.
        let ctx = PatchResolveContext {
            fixture_lamp_count: 17,
            object_spans: &spans,
            ..Default::default()
        };
        let resolved = resolve_patch(
            &ctx,
            &super::PatchDoc {
                format: 2,
                entries: vec![path_entry("/sector/1", 8)],
            },
        )
        .unwrap();
        assert_eq!(
            (resolved[0].start, resolved[0].count, resolved[0].lamp),
            (4, 4, 8)
        );
    }
}
