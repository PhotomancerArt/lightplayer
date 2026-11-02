//! The boxes each answer to O1 gives a fixture, shared by the look report
//! and the look pictures.

use lpc_mapping::space::{Fit, FitAlign, FitMode};

/// The drawing region today's `width` × `height` render covers: today's fit
/// contains `canvas` in the render, centred, so the render reaches past
/// the canvas on one axis.
pub fn footprint(canvas: [f32; 4], [width, height]: [u32; 2]) -> [f32; 4] {
    let [x, y, w, h] = canvas.map(f64::from);
    let scale = (f64::from(width) / w).min(f64::from(height) / h);
    let (fw, fh) = (f64::from(width) / scale, f64::from(height) / scale);
    [x + (w - fw) / 2.0, y + (h - fh) / 2.0, fw, fh].map(|v| v as f32)
}

pub fn fill() -> Fit {
    Fit::new(FitMode::Fill, FitAlign::CENTRE)
}

/// The box today's fit frames: the view box when it has a size, else the
/// lamps' bounds.
pub fn old_canvas(view_box: [f32; 4], lamp_bounds: [f32; 4]) -> [f32; 4] {
    let [_, _, w, h] = view_box;
    if w > f32::EPSILON && h > f32::EPSILON {
        view_box
    } else {
        lamp_bounds
    }
}

/// The lean's box: the lamps' bounds, an axis thinner than one pitch padded
/// symmetrically to one pitch but never past the long side. `None` when
/// both axes are empty and there is no pitch to pad by.
pub fn lean_box(bounds: [f32; 4], pitch: f64) -> Option<[f32; 4]> {
    let [x, y, w, h] = bounds;
    let long = w.max(h);
    let pad = |min: f32, size: f32| -> (f32, f32) {
        let target = (pitch as f32).min(long.max(pitch as f32));
        if size >= target || target <= 0.0 {
            (min, size)
        } else {
            (min - (target - size) / 2.0, target)
        }
    };
    let (x, w) = pad(x, w);
    let (y, h) = pad(y, h);
    (w > 0.0 && h > 0.0).then_some([x, y, w, h])
}

pub fn bounds(points: &[[f32; 2]]) -> [f32; 4] {
    let Some(first) = points.first() else {
        return [0.0; 4];
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first[0], first[1], first[0], first[1]);
    for p in points {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    [x0, y0, reaching(x0, x1), reaching(y0, y1)]
}

/// `max − min` in `f32`, rounded up where rounding to nearest would leave
/// the far edge `min + size` short of `max`: a box written from these
/// numbers must hold its own last lamp, or that lamp is dark.
fn reaching(min: f32, max: f32) -> f32 {
    let mut size = max - min;
    while size > 0.0 && f64::from(min) + f64::from(size) < f64::from(max) {
        size = f32::from_bits(size.to_bits() + 1);
    }
    size
}

/// The lamp pitch in the drawing's units: the mean distance between
/// consecutive lamps within a segment, else √(area / count), else the long
/// side over `count − 1` (pattern space's rule).
pub fn doc_pitch(points: &[[f32; 2]], segments: &[u32], bounds: [f32; 4]) -> f64 {
    let mut sum = 0.0f64;
    let mut pairs = 0usize;
    let mut at = 0usize;
    for &count in segments {
        let run = &points[at..(at + count as usize).min(points.len())];
        for pair in run.windows(2) {
            let dx = f64::from(pair[1][0] - pair[0][0]);
            let dy = f64::from(pair[1][1] - pair[0][1]);
            sum += (dx * dx + dy * dy).sqrt();
            pairs += 1;
        }
        at += count as usize;
    }
    if pairs > 0 {
        return sum / pairs as f64;
    }
    let (w, h) = (f64::from(bounds[2]), f64::from(bounds[3]));
    let n = points.len() as f64;
    if w * h > 0.0 {
        (w * h / n).sqrt()
    } else if n > 1.0 {
        w.max(h) / (n - 1.0)
    } else {
        0.0
    }
}
