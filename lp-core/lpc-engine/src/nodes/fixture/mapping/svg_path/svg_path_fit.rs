use super::svg_path_error::SvgPathError;
use super::svg_path_group::{SvgBounds, geometry_bounds};

pub fn fit_points(
    points: &[[f32; 2]],
    view_box: Option<SvgBounds>,
    texture_width: u32,
    texture_height: u32,
) -> Result<alloc::vec::Vec<[f32; 2]>, SvgPathError> {
    let bounds = view_box
        .or_else(|| geometry_bounds(points))
        .ok_or(SvgPathError::InvalidViewBox)?;
    if bounds.width <= f32::EPSILON
        || bounds.height <= f32::EPSILON
        || texture_width == 0
        || texture_height == 0
    {
        return Err(SvgPathError::InvalidViewBox);
    }

    let source_aspect = bounds.width / bounds.height;
    let destination_aspect = texture_width as f32 / texture_height as f32;
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
