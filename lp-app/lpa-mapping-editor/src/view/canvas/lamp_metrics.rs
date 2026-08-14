//! Pure doc-space metrics the canvas derives from a resolved document.

use lpc_mapping::{Bounds2d, ResolvedMap2d};

/// Lamp display radius in doc units: a fraction of the median consecutive
/// lamp spacing, so dense grids stay dots and sparse strips stay visible.
#[must_use]
pub fn lamp_display_radius(resolved: &ResolvedMap2d) -> f32 {
    let mut gaps: Vec<f32> = Vec::new();
    for span in &resolved.spans {
        let start = span.start as usize;
        let end = start + span.count as usize;
        for index in start..end.saturating_sub(1) {
            let a = resolved.lamps[index].pos;
            let b = resolved.lamps[index + 1].pos;
            let gap = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            if gap > f32::EPSILON {
                gaps.push(gap);
            }
        }
    }
    if gaps.is_empty() {
        return 7.0;
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).expect("finite gaps"));
    (gaps[gaps.len() / 2] * 0.30).clamp(1.5, 22.0)
}

/// The doc-space region a `target_aspect` render texture covers after
/// aspect-fit: the smallest rect with the target aspect that contains
/// `frame`, centered.
#[must_use]
pub fn fit_region(frame: Bounds2d, target_aspect: f32) -> Bounds2d {
    let frame_aspect = frame.width / frame.height.max(1e-6);
    let (width, height) = if frame_aspect >= target_aspect {
        (frame.width, frame.width / target_aspect)
    } else {
        (frame.height * target_aspect, frame.height)
    };
    Bounds2d {
        min_x: frame.min_x - (width - frame.width) / 2.0,
        min_y: frame.min_y - (height - frame.height) / 2.0,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::resolve;

    #[test]
    fn fit_region_pads_the_short_axis_and_centers() {
        let frame = Bounds2d {
            min_x: 0.0,
            min_y: 0.0,
            width: 200.0,
            height: 50.0,
        };
        let region = fit_region(frame, 1.0);
        assert!((region.width - 200.0).abs() < 1e-3);
        assert!((region.height - 200.0).abs() < 1e-3);
        assert!((region.min_y - (-75.0)).abs() < 1e-3);
        let tall = Bounds2d {
            min_x: 10.0,
            min_y: 0.0,
            width: 50.0,
            height: 100.0,
        };
        let region = fit_region(tall, 2.0);
        assert!((region.width - 200.0).abs() < 1e-3);
        assert!((region.min_x - (10.0 - 75.0)).abs() < 1e-3);
    }

    #[test]
    fn lamp_radius_tracks_median_spacing() {
        let doc = lpc_mapping::corpus::panel_16x16();
        let resolved = resolve(&doc).unwrap();
        let radius = lamp_display_radius(&resolved);
        assert!((radius - 26.0 * 0.30).abs() < 0.5, "radius {radius}");
    }
}
