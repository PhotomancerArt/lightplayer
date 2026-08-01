//! Aspect-fit doc-space positions into a fixture render target.
//!
//! Semantics match the legacy SVG importer: fit the source bounds inside the
//! destination rectangle without stretching, centered on the short axis,
//! emitting normalized `[0, 1]` coordinates.

use alloc::vec::Vec;

use crate::map2d_error::Map2dError;

/// Axis-aligned doc-space bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds2d {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
}

/// Tight bounds of a point set; `None` when empty.
pub fn bounds_of_points(points: &[[f32; 2]]) -> Option<Bounds2d> {
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
    Some(Bounds2d {
        min_x,
        min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

/// Fit doc-space points into a `target_width` × `target_height` texture,
/// preserving aspect, centered on the short axis, normalized to `[0, 1]`.
///
/// `frame` overrides the framed region (e.g. an authored canvas / imported
/// viewBox); geometry bounds are used when absent.
pub fn fit_points(
    points: &[[f32; 2]],
    frame: Option<Bounds2d>,
    target_width: u32,
    target_height: u32,
) -> Result<Vec<[f32; 2]>, Map2dError> {
    let bounds = frame
        .or_else(|| bounds_of_points(points))
        .ok_or(Map2dError::EmptyBounds)?;
    if bounds.width <= f32::EPSILON
        || bounds.height <= f32::EPSILON
        || target_width == 0
        || target_height == 0
    {
        return Err(Map2dError::EmptyBounds);
    }

    let source_aspect = bounds.width / bounds.height;
    let destination_aspect = target_width as f32 / target_height as f32;
    let (scale, offset_x, offset_y) = if source_aspect >= destination_aspect {
        let fitted_height = destination_aspect / source_aspect;
        (1.0 / bounds.width, 0.0, (1.0 - fitted_height) / 2.0)
    } else {
        let fitted_width = source_aspect / destination_aspect;
        (
            1.0 / bounds.height / destination_aspect,
            (1.0 - fitted_width) / 2.0,
            0.0,
        )
    };

    Ok(points
        .iter()
        .map(|[x, y]| {
            [
                ((*x - bounds.min_x) * scale + offset_x).clamp(0.0, 1.0),
                ((*y - bounds.min_y) * scale * destination_aspect + offset_y).clamp(0.0, 1.0),
            ]
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_wide_bounds_into_square_with_vertical_padding() {
        let fitted = fit_points(&[[0.0, 0.0], [20.0, 10.0]], None, 10, 10).unwrap();
        assert_eq!(fitted[0], [0.0, 0.25]);
        assert_eq!(fitted[1], [1.0, 0.75]);
    }

    #[test]
    fn fits_tall_bounds_into_square_with_horizontal_padding() {
        let fitted = fit_points(&[[0.0, 0.0], [10.0, 20.0]], None, 10, 10).unwrap();
        assert_eq!(fitted[0], [0.25, 0.0]);
        assert_eq!(fitted[1], [0.75, 1.0]);
    }

    #[test]
    fn frame_overrides_geometry_bounds() {
        let frame = Bounds2d {
            min_x: 0.0,
            min_y: 0.0,
            width: 40.0,
            height: 10.0,
        };
        // Geometry occupies the left half of a wider frame: it stays left.
        let fitted = fit_points(&[[0.0, 5.0], [20.0, 5.0]], Some(frame), 40, 10).unwrap();
        assert_eq!(fitted[0], [0.0, 0.5]);
        assert_eq!(fitted[1], [0.5, 0.5]);
    }

    #[test]
    fn rejects_degenerate_input() {
        assert!(matches!(
            fit_points(&[], None, 10, 10),
            Err(Map2dError::EmptyBounds)
        ));
        assert!(matches!(
            fit_points(&[[1.0, 1.0], [1.0, 1.0]], None, 10, 10),
            Err(Map2dError::EmptyBounds)
        ));
        assert!(matches!(
            fit_points(&[[0.0, 0.0], [1.0, 1.0]], None, 0, 10),
            Err(Map2dError::EmptyBounds)
        ));
    }
}
