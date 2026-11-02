//! Format 12's load-time fit, frozen: a fixture's lamps contain-fitted into
//! its `render_size`, as fractions in `[0, 1]` (`lpc-mapping`
//! `map2d_fit.rs` `fit_points_in_place`; `lpc-engine`
//! `nodes/fixture/parts_carrier.rs` `view_box_bounds` at `f90a0388a`).

/// The fixture's lamps (document positions, in the order the render gives
/// them) fitted into a `width` × `height` render: the view box framed when it
/// has a width and a height (more than `f32::EPSILON`), else the lamps' own
/// bounds; aspect kept, centred on the short axis, each coordinate clamped
/// to `[0, 1]`.
///
/// `Err` is the engine's refusal (a frame or a render of no size), in its
/// words; an empty lamp list fits to an empty list, as the engine skips the
/// fit when there is nothing to fit.
pub fn fit_lamps(
    positions: &[[f32; 2]],
    view_box: [f32; 4],
    width: u32,
    height: u32,
) -> Result<Vec<[f32; 2]>, String> {
    let mut points = positions.to_vec();
    if points.is_empty() {
        return Ok(points);
    }
    let [min_x, min_y, box_width, box_height] = view_box;
    let frame = (box_width > f32::EPSILON && box_height > f32::EPSILON)
        .then_some([min_x, min_y, box_width, box_height]);
    let bounds = match frame {
        Some(frame) => frame,
        None => bounds_of_points(&points),
    };
    let [min_x, min_y, bounds_width, bounds_height] = bounds;
    if bounds_width <= f32::EPSILON || bounds_height <= f32::EPSILON || width == 0 || height == 0 {
        return Err(
            "fit fixture mapping: fit was asked for but the geometry (or target) has \
                    no usable extent"
                .to_string(),
        );
    }
    let source_aspect = bounds_width / bounds_height;
    let destination_aspect = width as f32 / height as f32;
    let (scale, offset_x, offset_y) = if source_aspect >= destination_aspect {
        let fitted_height = destination_aspect / source_aspect;
        (1.0 / bounds_width, 0.0, (1.0 - fitted_height) / 2.0)
    } else {
        let fitted_width = source_aspect / destination_aspect;
        (
            1.0 / bounds_height / destination_aspect,
            (1.0 - fitted_width) / 2.0,
            0.0,
        )
    };
    for [x, y] in points.iter_mut() {
        *x = ((*x - min_x) * scale + offset_x).clamp(0.0, 1.0);
        *y = ((*y - min_y) * scale * destination_aspect + offset_y).clamp(0.0, 1.0);
    }
    Ok(points)
}

/// Tight bounds `[min_x, min_y, width, height]` of a non-empty point set
/// (`bounds_of_points`).
fn bounds_of_points(points: &[[f32; 2]]) -> [f32; 4] {
    let first = points[0];
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first[0], first[0], first[1], first[1]);
    for [x, y] in points.iter().copied() {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    [min_x, min_y, max_x - min_x, max_y - min_y]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wide bounds in a square render are centred with room above and below.
    #[test]
    fn wide_bounds_in_a_square_render_are_centred_vertically() {
        let fitted = fit_lamps(&[[0.0, 0.0], [20.0, 10.0]], [0.0; 4], 10, 10).unwrap();
        assert_eq!(fitted, vec![[0.0, 0.25], [1.0, 0.75]]);
    }

    /// A view box frames the lamps in place of their own bounds.
    #[test]
    fn a_view_box_frames_the_lamps() {
        let fitted = fit_lamps(&[[0.0, 5.0], [20.0, 5.0]], [0.0, 0.0, 40.0, 10.0], 40, 10).unwrap();
        assert_eq!(fitted, vec![[0.0, 0.5], [0.5, 0.5]]);
    }

    /// Lamps on one spot, with no view box, have no extent and are refused.
    #[test]
    fn lamps_on_one_spot_with_no_view_box_are_refused() {
        assert!(fit_lamps(&[[1.0, 1.0], [1.0, 1.0]], [0.0; 4], 10, 10).is_err());
    }
}
