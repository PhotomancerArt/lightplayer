//! Parsed SVG mapping-group shapes.

use alloc::vec::Vec;

use crate::map2d_fit::Bounds2d;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSvgPathGroups {
    pub groups: Vec<SvgPathGroup>,
    pub view_box: Option<Bounds2d>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SvgPathGroup {
    pub path_index: u32,
    pub count: u32,
    pub geometry: SvgPathGeometry,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SvgPathGeometry {
    Polyline(Vec<[f32; 2]>),
}
