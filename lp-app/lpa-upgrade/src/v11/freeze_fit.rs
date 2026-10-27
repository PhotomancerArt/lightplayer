//! Hand-placed lamps (`PathPoints`) as a map2d's `points`, with every lamp
//! reaching the sampler where it does today, bit for bit (Q52).
//!
//! Today a `PathPoints` lamp's position is used as written (clamped to
//! `[0, 1]`), and its sample diameter is in texture pixels. A map2d's lamps
//! go through the aspect fit instead (`lpc_mapping::fit_points_in_place`
//! against the document's `canvas`), and its sample diameter is a document
//! length scaled by the same fit (`fit_scale`). So the step picks a canvas
//! and writes each coordinate and the diameter as the document value the
//! fit carries back onto today's number exactly.
//!
//! **The canvas** is `[0, 0, 1, h]`, `h` at most `H / W`: the fit's scale
//! is then 1 and x passes through untouched, while y is multiplied by the
//! aspect `W / H` and moved down by an offset `(1 − (W / H) · h) / 2`.
//! **Each coordinate** (and the diameter) is found by bisection over
//! `f32`s: the fit is monotone in its input, so the values it lands exactly
//! on today's number, if any, are one run, and the one nearest the natural
//! guess is taken. A product by the aspect does not reach every `f32` (on
//! five-wire's 16 × 5 texture no y multiplied by 3.2 rounds to 0.9), so the
//! canvases are tried in a fixed order: first `h = H / W` (no offset), then
//! offsets of 1/64, 2/64 … 31/64 of the texture's height, until one lands
//! every number exactly. If none does the fixture is refused by name —
//! never "close".
//!
//! The fit is frozen from `lp-core/lpc-mapping/src/map2d_fit.rs` at
//! origin/main a35e3cd1f (`fit_points_in_place`, `fit_scale`,
//! `Map2dDoc::canvas_bounds`), operation for operation, so its roundings
//! are the engine's. Every step is one IEEE-754 `f32` operation, so the
//! answer is the same on every host (Q110).

/// A canvas `[min_x, min_y, width, height]` and the document values that
/// fit onto today's numbers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExactFit {
    pub(crate) canvas: [f32; 4],
    /// Each lamp's document position.
    pub(crate) at: Vec<[f32; 2]>,
    pub(crate) sample_diameter: f32,
}

/// How many offsets are tried, in 64ths of the texture's height.
const OFFSET_STEPS: u32 = 32;

/// Document positions and diameter that a `width` × `height` texture fits
/// onto `positions` (as the sampler clamps them) and `sample_diameter` (in
/// pixels) exactly. `Err` names a number no canvas tried lands exactly.
pub(crate) fn exact_fit(
    positions: &[[f32; 2]],
    sample_diameter: f32,
    width: u32,
    height: u32,
) -> Result<ExactFit, String> {
    if width == 0 || height == 0 {
        return Err(format!("its render size {width} × {height} is empty"));
    }
    let mut first_miss = None;
    for canvas in canvases(width, height) {
        match fit_on(canvas, positions, sample_diameter, width, height) {
            Ok(fit) => return Ok(fit),
            Err(miss) => {
                first_miss.get_or_insert(miss);
            }
        }
    }
    Err(first_miss.unwrap_or_default())
}

/// The canvases tried, in order (module doc).
fn canvases(width: u32, height: u32) -> Vec<[f32; 4]> {
    let aspect = width as f32 / height as f32;
    let guess = 1.0 / aspect;
    let exact = (0..8)
        .flat_map(|step| [step_by(guess, step), step_by(guess, -step)])
        .find(|h| 1.0 / *h == aspect)
        .unwrap_or(guess);
    let mut out = vec![[0.0, 0.0, 1.0, exact]];
    for k in 1..OFFSET_STEPS {
        let offset = k as f32 / 64.0;
        out.push([0.0, 0.0, 1.0, (1.0 - 2.0 * offset) / aspect]);
    }
    out
}

/// Every number solved on one canvas, or the first that is not.
fn fit_on(
    canvas: [f32; 4],
    positions: &[[f32; 2]],
    sample_diameter: f32,
    width: u32,
    height: u32,
) -> Result<ExactFit, String> {
    let aspect = width as f32 / height as f32;
    let origin = fit_point([0.0, 0.0], canvas, width, height);
    let mut at = Vec::with_capacity(positions.len());
    for (lamp, [x, y]) in positions.iter().copied().enumerate() {
        let target = [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)];
        let mut doc = [0.0f32; 2];
        for axis in 0..2 {
            // The natural guess: what the fit would undo without rounding.
            let guess = if axis == 0 {
                target[0] - origin[0]
            } else {
                (target[1] - origin[1]) / aspect
            };
            doc[axis] = preimage(target[axis], guess, |v| {
                let mut point = [0.0f32; 2];
                point[axis] = v;
                fit_point(point, canvas, width, height)[axis]
            })
            .ok_or_else(|| {
                format!(
                    "lamp {lamp}'s {} ({}) has no document value the map2d fit lands \
                     on exactly",
                    if axis == 0 { "x" } else { "y" },
                    target[axis]
                )
            })?;
        }
        at.push(doc);
    }
    let scale = fit_scale(canvas, width, height);
    let diameter =
        preimage(sample_diameter, sample_diameter / scale, |d| d * scale).ok_or_else(|| {
            format!(
                "its sample diameter ({sample_diameter}) has no document value the map2d fit \
                 scales onto exactly"
            )
        })?;
    Ok(ExactFit {
        canvas,
        at,
        sample_diameter: diameter,
    })
}

/// `fit_points_in_place` for one point (frozen; see the module doc).
pub(crate) fn fit_point(point: [f32; 2], canvas: [f32; 4], width: u32, height: u32) -> [f32; 2] {
    let [min_x, min_y, bounds_width, bounds_height] = canvas;
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
    let [x, y] = point;
    [
        ((x - min_x) * scale + offset_x).clamp(0.0, 1.0),
        ((y - min_y) * scale * destination_aspect + offset_y).clamp(0.0, 1.0),
    ]
}

/// `fit_scale` (frozen): texture pixels per document unit.
pub(crate) fn fit_scale(canvas: [f32; 4], width: u32, height: u32) -> f32 {
    let [_, _, bounds_width, bounds_height] = canvas;
    (width as f32 / bounds_width).min(height as f32 / bounds_height)
}

/// The document value `f` carries onto `target` exactly, nearest `guess`,
/// found by bisection over the `f32`s in `[-2^20, 2^20]` (`f` is monotone
/// non-decreasing there). Exactly is `==`: the same number, a zero of
/// either sign being zero.
fn preimage(target: f32, guess: f32, f: impl Fn(f32) -> f32) -> Option<f32> {
    let floor = ordered(-SEARCH_LIMIT);
    let ceiling = ordered(SEARCH_LIMIT);
    // The first value whose image is not below the target, and the first
    // whose image is above it: every value between lands on it.
    let first_at_or_above = bisect(floor, ceiling, |v| f(v) >= target);
    let first_above = bisect(floor, ceiling, |v| f(v) > target);
    if first_at_or_above >= first_above {
        return None;
    }
    let value = from_ordered(ordered(guess).clamp(first_at_or_above, first_above - 1));
    (f(value) == target).then_some(value)
}

/// The bisection's range, `[-2^20, 2^20]`: far past any lamp position or
/// diameter.
const SEARCH_LIMIT: f32 = 1_048_576.0;

/// The first ordered value in `[low, high]` where `above` holds (`high`
/// when none does), `above` being false then true along the range.
fn bisect(mut low: i64, mut high: i64, above: impl Fn(f32) -> bool) -> i64 {
    while low < high {
        let mid = low + (high - low) / 2;
        if above(from_ordered(mid)) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    low
}

/// `f32`s as integers in the same order (negative below positive).
fn ordered(value: f32) -> i64 {
    let bits = value.to_bits() as i32;
    i64::from(if bits < 0 { i32::MIN - bits } else { bits })
}

fn from_ordered(ordered: i64) -> f32 {
    let bits = ordered as i32;
    f32::from_bits((if bits < 0 { i32::MIN - bits } else { bits }) as u32)
}

/// The `f32` `steps` representable values above (or, negative, below) `x`.
fn step_by(x: f32, steps: i64) -> f32 {
    from_ordered(ordered(x) + steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three repo projects' textures (2 × 1, 16 × 5, 20 × 5) fit their
    /// lamps back bit for bit, on a canvas whose fit has scale 1 (16 × 5's
    /// y = 0.9 needs an offset canvas).
    #[test]
    fn hand_placed_lamps_fit_back_exactly_on_the_repo_textures() {
        let positions: Vec<[f32; 2]> = (0..=40)
            .flat_map(|i| {
                let v = i as f32 / 40.0;
                [[v, 0.1], [0.0312, 0.3], [v, 0.9], [0.4688, 0.7], [0.5, 0.5]]
            })
            .collect();
        for (width, height) in [(2, 1), (16, 5), (20, 5), (16, 16)] {
            let fit = exact_fit(&positions, 1.0, width, height).unwrap();
            assert_eq!(fit.canvas[2], 1.0, "{width}×{height}");
            assert!(fit.canvas[3] <= height as f32 / width as f32);
            for (doc, today) in fit.at.iter().zip(&positions) {
                assert_eq!(
                    fit_point(*doc, fit.canvas, width, height).map(f32::to_bits),
                    today.map(f32::to_bits),
                    "{width}×{height}"
                );
            }
            // x is written as it was.
            assert!(
                fit.at
                    .iter()
                    .zip(&positions)
                    .all(|(doc, today)| doc[0] == today[0])
            );
            assert_eq!(
                fit.sample_diameter * fit_scale(fit.canvas, width, height),
                1.0
            );
        }
    }

    /// A lamp outside `[0, 1]` reaches the sampler clamped, and so does its
    /// document position.
    #[test]
    fn a_lamp_outside_the_texture_stays_clamped() {
        let fit = exact_fit(&[[-0.5, 1.5]], 2.0, 4, 2).unwrap();
        assert_eq!(fit_point(fit.at[0], fit.canvas, 4, 2), [0.0, 1.0]);
    }

    #[test]
    fn ordered_floats_round_trip_in_order() {
        // (-0.0 shares 0.0's place: the search never needs both.)
        for x in [-2.0f32, -0.5, -1e-30, 0.0, 1e-30, 0.3125, 3.2, 1e6] {
            assert_eq!(from_ordered(ordered(x)).to_bits(), x.to_bits());
        }
        assert!(ordered(-1.0) < ordered(-0.5));
        assert!(ordered(0.5) < ordered(1.0));
        assert_eq!(step_by(1.0, 1), f32::from_bits(1.0f32.to_bits() + 1));
    }
}
