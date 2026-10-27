//! The format-6 document as it is written: the top level.
//!
//! Field order here is the order they are written in. Every field a
//! document may leave out is skipped when it holds its default, so the one
//! canonical form is the shortest one.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::map2d_doc::DEFAULT_SAMPLE_DIAMETER;
use crate::map2d6::op_doc::{FlowDoc, StartDoc};
use crate::map2d6::override_doc::OverrideDoc;
use crate::map2d6::part_doc::PartDoc;

/// A map2d format-6 document: one fixture's layout and patching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Map2d6Doc {
    pub format: u32,
    #[serde(default = "default_sample_diameter")]
    pub sample_diameter: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<[f32; 4]>,
    /// The next number per kind of part: `{"path": 4, "circle": 2}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub next: BTreeMap<String, u32>,
    /// The fixture's start.
    #[serde(default, skip_serializing_if = "StartDoc::is_keep")]
    pub start: StartDoc,
    /// How the top-level parts follow each other.
    #[serde(default, skip_serializing_if = "FlowDoc::is_end_to_end")]
    pub flow: FlowDoc,
    #[serde(default)]
    pub parts: Vec<PartDoc>,
    /// By node id, sorted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, OverrideDoc>,
}

/// Only the format, read before anything else (`Map2dDoc::from_json`'s
/// two-stage gate): a newer document is refused as newer, not as malformed.
#[derive(Deserialize)]
pub(crate) struct FormatPeek {
    pub format: u32,
}

fn default_sample_diameter() -> f32 {
    DEFAULT_SAMPLE_DIAMETER
}
