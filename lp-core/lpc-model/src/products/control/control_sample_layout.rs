//! Native control sample interpretation metadata.
//!
//! A control product renders the same sample buffer that an output consumes.
//! The sample layout explains how to interpret ranges of that native buffer
//! without changing the bytes into a display-only preview format.

use alloc::vec::Vec;

use crate::ColorOrder;

/// Metadata describing how native control samples are grouped.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlSampleLayout {
    pub spans: Vec<ControlSampleSpan>,
}

impl ControlSampleLayout {
    #[must_use]
    pub const fn empty() -> Self {
        Self { spans: Vec::new() }
    }
}

/// A contiguous range in a native control sample buffer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlSampleSpan {
    pub row: u32,
    pub start: u32,
    pub len: u32,
    pub encoding: ControlSampleEncoding,
}

/// How to interpret a native control sample range.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ControlSampleEncoding {
    /// A run of RGB lamps stored in the product's native RGB channel order.
    RgbPixels { count: u32, color_order: ColorOrder },
    /// Samples with no known higher-level display interpretation.
    Raw,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sample_layout_has_no_spans() {
        assert!(ControlSampleLayout::empty().spans.is_empty());
    }
}
