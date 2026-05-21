use alloc::vec::Vec;

use super::svg_path_error::SvgPathError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSvgPathGroups {
    pub groups: Vec<SvgPathGroup>,
    pub view_box: Option<SvgBounds>,
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

impl SvgPathGroup {
    pub fn sample_points(&self) -> Result<Vec<[f32; 2]>, SvgPathError> {
        if self.count == 0 {
            return Err(SvgPathError::ZeroCount {
                path_index: self.path_index,
            });
        }

        let points = self.geometry.points();
        if points.len() < 2 {
            return Err(SvgPathError::EmptyPath {
                path_index: self.path_index,
            });
        }

        let total = polyline_length(points);
        if total <= f32::EPSILON {
            return Err(SvgPathError::EmptyPath {
                path_index: self.path_index,
            });
        }

        let mut sampled = Vec::new();
        if self.count == 1 {
            sampled.push(points[0]);
            return Ok(sampled);
        }

        for index in 0..self.count {
            let distance = total * (index as f32 / (self.count - 1) as f32);
            sampled.push(point_at_distance(points, distance));
        }
        Ok(sampled)
    }
}

impl SvgPathGeometry {
    pub fn points(&self) -> &[[f32; 2]] {
        match self {
            Self::Polyline(points) => points,
        }
    }
}

pub fn geometry_bounds(points: &[[f32; 2]]) -> Option<SvgBounds> {
    let first = points.first()?;
    let mut min_x = first[0];
    let mut max_x = first[0];
    let mut min_y = first[1];
    let mut max_y = first[1];
    for [x, y] in points.iter().copied() {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    Some(SvgBounds {
        min_x,
        min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn polyline_length(points: &[[f32; 2]]) -> f32 {
    points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum()
}

fn point_at_distance(points: &[[f32; 2]], target_distance: f32) -> [f32; 2] {
    let mut remaining = target_distance;
    for pair in points.windows(2) {
        let start = pair[0];
        let end = pair[1];
        let segment = distance(start, end);
        if segment <= f32::EPSILON {
            continue;
        }
        if remaining <= segment {
            let t = remaining / segment;
            return [
                start[0] + (end[0] - start[0]) * t,
                start[1] + (end[1] - start[1]) * t,
            ];
        }
        remaining -= segment;
    }
    *points.last().expect("non-empty points")
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    libm::sqrtf(dx * dx + dy * dy)
}
