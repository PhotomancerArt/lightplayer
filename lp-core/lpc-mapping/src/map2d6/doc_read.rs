//! The format-6 document, read from its JSON text by hand ([`JsonText`];
//! why not serde is there) into what `read.rs` checks: the document as
//! written, with starts, flows, overrides and the plain shapes already in
//! the model's types (they have no rules to check), and parts, paths and
//! circles as text until their ids, keys and path data are checked.
//!
//! Each struct reads as its serde derive did: fields in any order, unknown
//! fields read and ignored, a field given twice refused, a missing field
//! refused (the first in declaration order) unless it has a default, `null`
//! only where the field is optional (absent and `null` alike are "not
//! set"). Enums are externally tagged: a unit variant is `"word"` (or
//! `{"word": null}`), a variant with data `{"word": value}`. A map's key
//! given twice keeps its last value.
//!
//! The defaults here are the ones the writer leaves out (`write.rs`); the
//! round-trip statements in `tests/format6.rs` hold the two together.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::map2d_doc::{
    DEFAULT_SAMPLE_DIAMETER, FilledPolygonShape, GridCorner, GridRouting, GridShape, PathAlign,
    PolygonShape, RingDir, RingOrder,
};
use crate::map2d6::json_text::{JsonText, Read};
use crate::part::{CircleSplit, PathSplit, Points, Shape};
use crate::patching::{
    Amount, ChannelOp, Direction, Flow, OutputName, OutputOp, Override, PortKey, PortOp, Start,
};

/// A format-6 document as written, before its rules are checked.
pub(crate) struct DocText {
    pub sample_diameter: f32,
    pub canvas: Option<[f32; 4]>,
    pub next: BTreeMap<String, u32>,
    pub start: Start,
    pub flow: Flow,
    pub parts: Vec<PartText>,
    /// By the key's text, sorted, one per key (its last).
    pub overrides: Vec<(String, Override)>,
}

pub(crate) struct PartText {
    pub id: String,
    pub name: String,
    pub start: Start,
    pub flow: Flow,
    pub kind: KindText,
}

pub(crate) enum KindText {
    /// Polygon, grid, filled polygon, points: nothing to check.
    Shape(Shape),
    Path(PathText),
    Circle(CircleText),
    Group(Vec<PartText>),
    Repeat {
        center: [f32; 2],
        count: u32,
        part: Box<PartText>,
    },
}

pub(crate) struct PathText {
    pub d: String,
    pub count: u32,
    pub reversed: bool,
    pub align: PathAlign,
    pub split: PathSplit,
}

pub(crate) struct CircleText {
    pub center: [f32; 2],
    pub start_angle_deg: f32,
    pub dir: RingDir,
    pub drawn_from: RingOrder,
    pub split: CircleSplit,
    pub next_ring: u32,
    pub rings: Vec<RingText>,
}

pub(crate) struct RingText {
    pub key: String,
    pub radius: f32,
    pub count: u32,
}

/// A document's `format`, the rest read and ignored.
pub(crate) fn read_format(t: &mut JsonText) -> Read<u32> {
    let mut format = None;
    let mut seen = 0;
    t.begin_object()?;
    while let Some(key) = t.next_key()? {
        if key != "format" {
            t.skip()?;
            continue;
        }
        t.once(&mut seen, 0, &key)?;
        format = Some(t.u32()?);
    }
    format.ok_or_else(|| t.missing("format"))
}

const DOC_FIELDS: [&str; 8] = [
    "format",
    "sample_diameter",
    "canvas",
    "next",
    "start",
    "flow",
    "parts",
    "overrides",
];

/// The document (its `format` is read on its own first, [`read_format`]).
pub(crate) fn read_doc(t: &mut JsonText) -> Read<DocText> {
    let mut doc = DocText {
        sample_diameter: DEFAULT_SAMPLE_DIAMETER,
        canvas: None,
        next: BTreeMap::new(),
        start: Start::KEEP,
        flow: Flow::EndToEnd,
        parts: Vec::new(),
        overrides: Vec::new(),
    };
    let mut seen = 0;
    t.begin_object()?;
    while let Some(key) = t.next_key()? {
        let Some(field) = DOC_FIELDS.iter().position(|f| **f == *key) else {
            t.skip()?;
            continue;
        };
        t.once(&mut seen, field as u32, &key)?;
        match field {
            0 => drop(t.u32()?),
            1 => doc.sample_diameter = t.f32()?,
            2 => {
                if !t.null()? {
                    let mut canvas = [0.0; 4];
                    t.f32s(&mut canvas)?;
                    doc.canvas = Some(canvas);
                }
            }
            3 => {
                t.begin_object()?;
                while let Some(word) = t.next_key()? {
                    let n = t.u32()?;
                    doc.next.insert(word.into_owned(), n);
                }
            }
            4 => doc.start = read_start(t)?,
            5 => doc.flow = read_flow(t)?,
            6 => doc.parts = read_parts(t)?,
            _ => {
                t.begin_object()?;
                while let Some(id) = t.next_key()? {
                    let entry = read_override(t)?;
                    let id = id.into_owned();
                    let list = &mut doc.overrides;
                    match list.binary_search_by(|(held, _)| held.as_str().cmp(&id)) {
                        Ok(at) => list[at].1 = entry,
                        Err(at) => list.insert(at, (id, entry)),
                    }
                }
            }
        }
    }
    if seen & 1 == 0 {
        return Err(t.missing("format"));
    }
    Ok(doc)
}

fn read_parts(t: &mut JsonText) -> Read<Vec<PartText>> {
    let mut parts = Vec::new();
    t.begin_array()?;
    while t.next_element()? {
        parts.push(read_part(t)?);
    }
    Ok(parts)
}

const PART_FIELDS: [&str; 5] = ["id", "name", "start", "flow", "kind"];

fn read_part(t: &mut JsonText) -> Read<PartText> {
    let mut id = None;
    let mut name = String::new();
    let mut start = Start::KEEP;
    let mut flow = Flow::EndToEnd;
    let mut kind = None;
    let mut seen = 0;
    t.begin_object()?;
    while let Some(key) = t.next_key()? {
        let Some(field) = PART_FIELDS.iter().position(|f| **f == *key) else {
            t.skip()?;
            continue;
        };
        t.once(&mut seen, field as u32, &key)?;
        match field {
            0 => id = Some(t.string()?),
            1 => name = t.string()?,
            2 => start = read_start(t)?,
            3 => flow = read_flow(t)?,
            _ => kind = Some(read_kind(t)?),
        }
    }
    let Some(id) = id else {
        return Err(t.missing("id"));
    };
    let Some(kind) = kind else {
        return Err(t.missing("kind"));
    };
    Ok(PartText {
        id,
        name,
        start,
        flow,
        kind,
    })
}

// ---- kinds ------------------------------------------------------------------

const PATH: usize = 0;
const POLYGON: usize = 1;
const CIRCLE: usize = 2;
const GRID: usize = 3;
const FILLED_POLYGON: usize = 4;
const POINTS: usize = 5;
const GROUP: usize = 6;

const KIND_WORDS: [&str; 8] = [
    "path",
    "polygon",
    "circle",
    "grid",
    "filled_polygon",
    "points",
    "group",
    "repeat",
];

/// Every field a kind has, by name; a kind lists its own as indexes here.
const FIELD_NAMES: [&str; 21] = [
    "d",
    "count",
    "reversed",
    "align",
    "split",
    "points",
    "center",
    "start_angle_deg",
    "dir",
    "drawn_from",
    "next_ring",
    "rings",
    "origin",
    "cols",
    "rows",
    "pitch",
    "angle_deg",
    "routing",
    "start_corner",
    "parts",
    "part",
];

/// Marks a field with no default in [`KIND_FIELDS`].
const NEEDED: u8 = 0x80;

/// Every kind's fields ([`FIELD_NAMES`]), in the order its serde derive
/// declared them. The points kind's one field, `at`, is read as `points`.
const KIND_FIELDS: [&[u8]; 8] = [
    &[NEEDED, 1 | NEEDED, 2, 3, 4],
    &[5 | NEEDED, 1 | NEEDED, 3],
    &[6 | NEEDED, 7, 8, 9, 4, 10, 11 | NEEDED],
    &[12 | NEEDED, 13 | NEEDED, 14 | NEEDED, 15 | NEEDED, 17, 18],
    &[5 | NEEDED, 15 | NEEDED, 16, 12, 17, 18],
    &[5 | NEEDED],
    &[19],
    &[6 | NEEDED, 1 | NEEDED, 20 | NEEDED],
];

/// `{"path": {…}}`, `{"circle": {…}}`…: every kind is read into one set of
/// fields, then takes the ones it has.
fn read_kind(t: &mut JsonText) -> Read<KindText> {
    let kind = t.variant_of(&KIND_WORDS, 0)?;
    let fields = KIND_FIELDS[kind];
    let mut f = Fields::default();
    let mut seen = 0;
    t.begin_object()?;
    while let Some(key) = t.next_key()? {
        // The points kind's `at` is read as `points` (and its `points` is
        // no field of it).
        let name = match (kind == POINTS, &*key) {
            (true, "at") => "points",
            (true, "points") => "",
            (_, key) => key,
        };
        let found = FIELD_NAMES.iter().position(|n| *n == name).and_then(|index| {
            fields
                .iter()
                .position(|field| (field & !NEEDED) as usize == index)
                .map(|at| (index, at))
        });
        let Some((index, at)) = found else {
            t.skip()?;
            continue;
        };
        t.once(&mut seen, at as u32, &key)?;
        read_field(t, &mut f, kind, index)?;
    }
    for (at, field) in fields.iter().enumerate() {
        if seen & (1 << at) == 0 && field & NEEDED != 0 {
            let index = (field & !NEEDED) as usize;
            return Err(t.missing(if kind == POINTS { "at" } else { FIELD_NAMES[index] }));
        }
    }
    t.end_variant()?;
    Ok(match kind {
        PATH => KindText::Path(PathText {
            d: f.d,
            count: f.count,
            reversed: f.reversed,
            align: f.align,
            split: [PathSplit::One, PathSplit::Subpaths][f.split],
        }),
        POLYGON => KindText::Shape(Shape::Polygon(PolygonShape {
            points: f.points,
            count: f.count,
            align: f.align,
        })),
        CIRCLE => KindText::Circle(CircleText {
            center: f.center,
            start_angle_deg: f.start_angle_deg.0,
            dir: f.dir,
            drawn_from: f.drawn_from,
            split: [CircleSplit::Rings, CircleSplit::One][f.split],
            next_ring: f.next_ring.0,
            rings: f.rings,
        }),
        GRID => KindText::Shape(Shape::Grid(GridShape {
            origin: f.origin,
            cols: f.cols,
            rows: f.rows,
            pitch: f.pitch,
            routing: f.routing,
            start_corner: f.start_corner,
        })),
        FILLED_POLYGON => KindText::Shape(Shape::FilledPolygon(FilledPolygonShape {
            points: f.points,
            pitch: f.pitch,
            angle_deg: f.angle_deg,
            origin: f.origin,
            routing: f.routing,
            start_corner: f.start_corner,
        })),
        POINTS => KindText::Shape(Shape::Points(Points { at: f.points })),
        GROUP => KindText::Group(f.parts),
        _ => KindText::Repeat {
            center: f.center,
            count: f.count,
            part: match f.part {
                Some(part) => part,
                None => return Err(t.missing("part")),
            },
        },
    })
}

/// Every kind's fields in one place, at their defaults.
#[derive(Default)]
struct Fields {
    d: String,
    count: u32,
    reversed: bool,
    align: PathAlign,
    /// The index of the kind's own `split` word (a path's first is `one`, a
    /// circle's `rings`).
    split: usize,
    points: Vec<[f32; 2]>,
    center: [f32; 2],
    start_angle_deg: StartAngle,
    dir: RingDir,
    drawn_from: RingOrder,
    next_ring: NextRing,
    rings: Vec<RingText>,
    origin: [f32; 2],
    cols: u32,
    rows: u32,
    pitch: f32,
    angle_deg: f32,
    routing: GridRouting,
    start_corner: GridCorner,
    parts: Vec<PartText>,
    part: Option<Box<PartText>>,
}

/// One kind field's value, by its index in [`FIELD_NAMES`].
fn read_field(t: &mut JsonText, f: &mut Fields, kind: usize, index: usize) -> Read<()> {
    match index {
        0 => f.d = t.string()?,
        1 => f.count = t.u32()?,
        2 => f.reversed = t.bool()?,
        3 => {
            let words = ["on", "inside", "outside"];
            f.align = [PathAlign::On, PathAlign::Inside, PathAlign::Outside]
                [t.unit_variant(&words)?];
        }
        4 => {
            let words: &[&str] = if kind == PATH {
                &["one", "subpaths"]
            } else {
                &["rings", "one"]
            };
            f.split = t.unit_variant(words)?;
        }
        5 => f.points = t.points()?,
        6 => f.center = t.point()?,
        7 => f.start_angle_deg = StartAngle(t.f32()?),
        8 => f.dir = [RingDir::Cw, RingDir::Ccw][t.unit_variant(&["cw", "ccw"])?],
        9 => {
            let words = ["outside", "inside"];
            f.drawn_from = [RingOrder::OuterFirst, RingOrder::InnerFirst][t.unit_variant(&words)?];
        }
        10 => f.next_ring = NextRing(t.u32()?),
        11 => {
            t.begin_array()?;
            while t.next_element()? {
                f.rings.push(read_ring(t)?);
            }
        }
        12 => f.origin = t.point()?,
        13 => f.cols = t.u32()?,
        14 => f.rows = t.u32()?,
        15 => f.pitch = t.f32()?,
        16 => f.angle_deg = t.f32()?,
        17 => {
            let words = ["snake", "raster"];
            f.routing = [GridRouting::Snake, GridRouting::Raster][t.unit_variant(&words)?];
        }
        18 => {
            let words = ["tl", "tr", "bl", "br"];
            let corners = [GridCorner::Tl, GridCorner::Tr, GridCorner::Bl, GridCorner::Br];
            f.start_corner = corners[t.unit_variant(&words)?];
        }
        19 => f.parts = read_parts(t)?,
        _ => f.part = Some(Box::new(read_part(t)?)),
    }
    Ok(())
}

const RING_FIELDS: [&str; 3] = ["key", "radius", "count"];

fn read_ring(t: &mut JsonText) -> Read<RingText> {
    let mut ring = RingText {
        key: String::new(),
        radius: 0.0,
        count: 0,
    };
    let mut seen = 0;
    t.begin_object()?;
    while let Some(key) = t.next_key()? {
        let Some(field) = RING_FIELDS.iter().position(|f| **f == *key) else {
            t.skip()?;
            continue;
        };
        t.once(&mut seen, field as u32, &key)?;
        match field {
            0 => ring.key = t.string()?,
            1 => ring.radius = t.f32()?,
            _ => ring.count = t.u32()?,
        }
    }
    if let Some(field) = (0..3).find(|bit| seen & (1 << bit) == 0) {
        return Err(t.missing(RING_FIELDS[field]));
    }
    Ok(ring)
}

// ---- starts, flows, overrides ------------------------------------------------

/// A start: each field absent (or `null`) is Keep.
fn read_start(t: &mut JsonText) -> Read<Start> {
    let mut entry = Override::default();
    read_ops(t, &mut entry, 3)?;
    Ok(Start {
        output: entry.output.unwrap_or_default(),
        port: entry.port.unwrap_or_default(),
        channel: entry.channel.unwrap_or_default(),
    })
}

fn read_override(t: &mut JsonText) -> Read<Override> {
    let mut entry = Override::default();
    read_ops(t, &mut entry, OP_FIELDS.len())?;
    Ok(entry)
}

const OP_FIELDS: [&str; 6] = ["output", "port", "channel", "off", "direction", "offset"];

/// A start's fields (the first three of [`OP_FIELDS`]) or an override's
/// (all six); `null` is not set, except for `off`, which has no `null`.
fn read_ops(t: &mut JsonText, entry: &mut Override, fields: usize) -> Read<()> {
    let mut seen = 0;
    t.begin_object()?;
    while let Some(key) = t.next_key()? {
        let Some(field) = OP_FIELDS[..fields].iter().position(|f| **f == *key) else {
            t.skip()?;
            continue;
        };
        t.once(&mut seen, field as u32, &key)?;
        if field == 3 {
            entry.off = t.bool()?;
            continue;
        }
        if t.null()? {
            continue;
        }
        match field {
            0 => entry.output = Some(read_output_op(t)?),
            1 => entry.port = Some(read_port_op(t)?),
            2 => entry.channel = Some(read_channel_op(t)?),
            4 => {
                let words = ["forward", "reverse"];
                entry.direction =
                    Some([Direction::Forward, Direction::Reverse][t.unit_variant(&words)?]);
            }
            _ => entry.offset = Some(t.u32()?),
        }
    }
    Ok(())
}

/// The op words: `"keep"`, `{"set": …}`, `{"add": …}`.
const OP_WORDS: [&str; 3] = ["keep", "set", "add"];

/// `"keep"` or `{"set": "main"}`.
fn read_output_op(t: &mut JsonText) -> Read<OutputOp> {
    if t.variant_of(&OP_WORDS[..2], 1)? == 0 {
        return Ok(OutputOp::Keep);
    }
    let name = t.string()?;
    t.end_variant()?;
    Ok(OutputOp::Set(OutputName::new(&name)))
}

/// `"keep"`, `{"set": 0}` or `{"add": 1}`.
fn read_port_op(t: &mut JsonText) -> Read<PortOp> {
    let word = t.variant_of(&OP_WORDS, 1)?;
    if word == 0 {
        return Ok(PortOp::Keep);
    }
    let n = t.u32()?;
    t.end_variant()?;
    Ok(if word == 1 { PortOp::set(n) } else { PortOp::Add(n) })
}

/// `"keep"`, `{"set": {"lamps": 0}}` or `{"add": {"channels": 13}}`.
fn read_channel_op(t: &mut JsonText) -> Read<ChannelOp> {
    let word = t.variant_of(&OP_WORDS, 1)?;
    if word == 0 {
        return Ok(ChannelOp::Keep);
    }
    let lamps = t.variant_of(&["lamps", "channels"], 0)? == 0;
    let n = t.u32()?;
    t.end_variant()?;
    t.end_variant()?;
    let amount = if lamps {
        Amount::lamps(n)
    } else {
        Amount::channels(n)
    };
    Ok(if word == 1 {
        ChannelOp::Set(amount)
    } else {
        ChannelOp::Add(amount)
    })
}

/// `"end_to_end"` or `{"one_per_port": {"from": 0}}`.
fn read_flow(t: &mut JsonText) -> Read<Flow> {
    if t.variant_of(&["end_to_end", "one_per_port"], 1)? == 0 {
        return Ok(Flow::EndToEnd);
    }
    let mut from = None;
    let mut seen = 0;
    t.begin_object()?;
    while let Some(key) = t.next_key()? {
        if key != "from" {
            t.skip()?;
            continue;
        }
        t.once(&mut seen, 0, &key)?;
        from = Some(t.u32()?);
    }
    let Some(from) = from else {
        return Err(t.missing("from"));
    };
    t.end_variant()?;
    Ok(Flow::OnePerPort {
        from: PortKey(from),
    })
}

/// A circle's `start_angle_deg` when it has none: -90, twelve o'clock.
struct StartAngle(f32);

impl Default for StartAngle {
    fn default() -> Self {
        Self(-90.0)
    }
}

/// A circle's `next_ring` when it has none: 1.
struct NextRing(u32);

impl Default for NextRing {
    fn default() -> Self {
        Self(1)
    }
}
