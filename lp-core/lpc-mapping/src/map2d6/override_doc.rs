//! An override as it is written: the ops a person set on one node, and its
//! off, direction and offset.
//!
//! Absent means "what the node would get" (its part's start, else its
//! parent's flow). `"keep"` is written and means "hold the cursor here,
//! whatever the flow says" (G1, 2026-10-27: an explicit keep in overrides).

use serde::Serialize;

use crate::map2d6::op_doc::{ChannelOpDoc, OutputOpDoc, PortOpDoc};
use crate::patching::Direction;

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub(crate) struct OverrideDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputOpDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<PortOpDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<ChannelOpDoc>,
    #[serde(skip_serializing_if = "is_false")]
    pub off: bool,
    /// `"forward"` or `"reverse"`; written whenever set, since a segment's
    /// own forward overrides a part's reverse (F14).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// In lamps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

fn is_false(value: &bool) -> bool {
    !*value
}
