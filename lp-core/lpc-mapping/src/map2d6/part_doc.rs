//! A part as it is written: its id, name, start and flow, and its kind.
//!
//! The polygon, grid and filled polygon are today's shape structs, written
//! with today's field names and defaults, so a migrated grid's bytes read
//! like today's (and the device links one deserializer for each, not two).
//! The path is SVG path data; the circle holds keyed rings; points list
//! their positions; a group holds parts; a repeat holds one part.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::map2d_doc::{FilledPolygonShape, GridShape, PathAlign, PolygonShape, RingDir};
use crate::map2d6::op_doc::{FlowDoc, StartDoc};
use crate::part::{CircleSplit, PathSplit};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PartDoc {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Applies to its node in every copy.
    #[serde(default, skip_serializing_if = "StartDoc::is_keep")]
    pub start: StartDoc,
    #[serde(default, skip_serializing_if = "FlowDoc::is_end_to_end")]
    pub flow: FlowDoc,
    pub kind: KindDoc,
}

/// `{"path": {…}}`, `{"circle": {…}}`, `{"group": {…}}`…
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KindDoc {
    Path(PathDoc),
    Polygon(PolygonShape),
    Circle(CircleDoc),
    Grid(GridShape),
    FilledPolygon(FilledPolygonShape),
    Points(PointsDoc),
    Group(GroupDoc),
    Repeat(RepeatDoc),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PathDoc {
    /// SVG path data, absolute: `M 10 10 L 40 10 M 50 10 L 80 10`.
    pub d: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reversed: bool,
    #[serde(default, skip_serializing_if = "PathAlign::is_on")]
    pub align: PathAlign,
    /// `"subpaths"`: one segment per subpath.
    #[serde(default, skip_serializing_if = "is_one_path_segment")]
    pub split: PathSplit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CircleDoc {
    pub center: [f32; 2],
    #[serde(default = "default_start_angle")]
    pub start_angle_deg: f32,
    #[serde(default)]
    pub dir: RingDir,
    #[serde(default)]
    pub drawn_from: DrawnFromDoc,
    /// `"one"`: one segment for every ring (today's ring set).
    #[serde(default, skip_serializing_if = "is_split_by_ring")]
    pub split: CircleSplit,
    #[serde(default = "default_next_ring")]
    pub next_ring: u32,
    /// Listed outside in.
    pub rings: Vec<RingDoc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RingDoc {
    pub key: String,
    /// 0 is the centre.
    pub radius: f32,
    pub count: u32,
}

/// Which ring comes first: `"outside"` (the list's order) or `"inside"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DrawnFromDoc {
    #[default]
    Outside,
    Inside,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PointsDoc {
    pub at: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GroupDoc {
    #[serde(default)]
    pub parts: Vec<PartDoc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RepeatDoc {
    pub center: [f32; 2],
    pub count: u32,
    pub part: Box<PartDoc>,
}

fn default_start_angle() -> f32 {
    -90.0
}

fn default_next_ring() -> u32 {
    1
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_one_path_segment(split: &PathSplit) -> bool {
    *split == PathSplit::One
}

fn is_split_by_ring(split: &CircleSplit) -> bool {
    *split == CircleSplit::Rings
}
