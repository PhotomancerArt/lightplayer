//! Metadata describing rendered logical control samples.

use alloc::vec::Vec;

use lpc_model::ColorOrder;

/// Debug/inspection metadata for one rendered control range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlSpan {
    pub row: u32,
    pub start: u32,
    pub len: u32,
    pub hint: ControlHint,
}

/// Semantic hint for interpreting a range of control samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlHint {
    RgbPixels { count: u32, color_order: ColorOrder },
    Raw,
}

/// Debug/inspection metadata returned after control rendering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlLayout {
    pub spans: Vec<ControlSpan>,
}

impl ControlLayout {
    #[must_use]
    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }
}
