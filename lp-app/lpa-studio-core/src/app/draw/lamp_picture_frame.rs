//! Where a lamp field is drawn in its box, and where a point on that picture
//! is in pattern space: the one place both directions live.
//!
//! The web's lamp renderer (`lpa-studio-web`'s `LampView`) draws each lamp as
//! a voronoi cell (`lpa_mapping_editor::point_cells`) and aspect-FITS the
//! bounding box of the cells — not of the lamp centres — into its canvas, so
//! an edge lamp's cell is never clipped. A cell reaches
//! [`LAMP_CELL_INSET_KEEP`] × its radius from its lamp, and the extreme lamp
//! on each side always keeps that vertex (no neighbour lies beyond it), so
//! the drawn picture is the lamp-centre box grown by that much on every
//! side: about 0.83 of a lamp spacing (0.92 × 0.9) on a field dense enough
//! for the spacing to set the radius.
//!
//! [`LampPictureFrame`] holds that geometry, computed once per layout:
//!
//! - **forward** ([`LampPictureFrame::fit`]): the transform the renderer
//!   paints through, and the picture box it lands on, for a destination box.
//!   The web normalizes a pointer to that picture box with
//!   [`PictureFit::picture_point`];
//! - **inverse** ([`LampPictureFrame::picture_to_pattern`]): a point given
//!   as `0..1` of the picture box, back to the layout's hint space and on
//!   into pattern space through the lamp-centre box — the engine's own
//!   [`ScopeGeometry`] and [`PatternFrame::two_d`], built over the same
//!   Q16.16 pixel coordinates a fixture's `ScopeGeometry::from_mapping`
//!   streams, so a finger at a lamp lands where the shader sees that lamp.
//!
//! Pattern space (`docs/adr/2026-09-24-pattern-space.md`): the lamp box,
//! centred, long side −1…1, y up. A touch is always 2D here, also on a strip
//! (`drawing-on-the-picture` Q7). A point outside the box is allowed: a
//! finger can drag off the lamps, and a pattern may use it (an emitter just
//! off the edge still pushes dye in).

use lpc_engine::products::visual::{
    PatternFrame, ScopeGeometry, normalized_f32_to_q16, normalized_q16_to_pixel_q16,
};
use lpc_model::ControlLayout2d;

/// A cell's radius is this × the field's median nearest-neighbour distance,
/// floored by the layout's declared footprint. Mirrors
/// `lpa_mapping_editor`'s `point_cells` (its `NN_FRACTION`); the web's
/// `LampView` grows its cells by [`LampPictureFrame::cell_radius`], and its
/// test pins the drawn cells to [`LampPictureFrame::bounds`].
pub const LAMP_CELL_PITCH_FRACTION: f64 = 0.92;

/// The part of a cell kept after its seam inset (`point_cells`'
/// `INSET_KEEP`): a cell's outermost vertex is this × its radius from its
/// lamp.
pub const LAMP_CELL_INSET_KEEP: f64 = 0.9;

/// Lamps closer than this are coincident: no neighbour distance.
const COINCIDENT_EPS: f64 = 1e-9;

/// One Q16.16 unit.
const Q16_ONE: f64 = 65536.0;

/// One lamp layout's drawn picture and its pattern-space frame.
#[derive(Clone, Debug, PartialEq)]
pub struct LampPictureFrame {
    /// The layout's hint space (`width_hint`, `height_hint`, at least 1).
    hint: [u32; 2],
    /// The radius every cell grows to, in hint units.
    cell_radius: f64,
    /// The drawn picture in hint space, `[x, y, width, height]`: the
    /// lamp-centre box grown by the cells' reach.
    bounds: [f64; 4],
    /// The engine's pixel → pattern map over the lamp-centre box.
    pattern: PatternFrame,
}

/// Where a frame's picture lands in one destination box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PictureFit {
    /// Hint units → destination units, the same on both axes.
    pub scale: f64,
    /// Destination position of hint `(0, 0)`: a hint point `h` draws at
    /// `offset + scale × h`.
    pub offset: [f64; 2],
    /// The picture box in destination units, `[x, y, width, height]`.
    pub picture: [f64; 4],
}

impl PictureFit {
    /// A destination point as `0..1` of the picture box (outside it below 0
    /// or past 1): what the web reports a pointer as.
    #[must_use]
    pub fn picture_point(&self, point: [f64; 2]) -> [f32; 2] {
        let [x, y, width, height] = self.picture;
        [
            ((point[0] - x) / width.max(f64::MIN_POSITIVE)) as f32,
            ((point[1] - y) / height.max(f64::MIN_POSITIVE)) as f32,
        ]
    }
}

impl LampPictureFrame {
    /// The frame of `layout`; `None` when it has no lamps.
    #[must_use]
    pub fn of(layout: &ControlLayout2d) -> Option<Self> {
        if layout.lamps.is_empty() {
            return None;
        }
        let hint = [layout.width_hint.max(1), layout.height_hint.max(1)];
        let points: Vec<[f64; 2]> = layout
            .lamps
            .iter()
            .map(|lamp| {
                let [x, y] = lamp_hint_position(layout, lamp.center);
                [f64::from(x), f64::from(y)]
            })
            .collect();
        let floor = f64::from(lamp_footprint_floor(layout)).max(f64::EPSILON);
        let cell_radius = median_nearest_neighbour(&points)
            .map_or(floor, |pitch| (LAMP_CELL_PITCH_FRACTION * pitch).max(floor));
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for point in &points {
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        let reach = LAMP_CELL_INSET_KEEP * cell_radius;
        let bounds = [
            min[0] - reach,
            min[1] - reach,
            max[0] - min[0] + 2.0 * reach,
            max[1] - min[1] + 2.0 * reach,
        ];
        Some(Self {
            hint,
            cell_radius,
            bounds,
            pattern: PatternFrame::two_d(&lamp_scope(layout, hint)),
        })
    }

    /// The radius every lamp's cell grows to, in hint units.
    #[must_use]
    pub fn cell_radius(&self) -> f64 {
        self.cell_radius
    }

    /// The drawn picture in hint space, `[x, y, width, height]`.
    #[must_use]
    pub fn bounds(&self) -> [f64; 4] {
        self.bounds
    }

    /// FORWARD: the picture aspect-fitted into `dest` (`[x, y, width,
    /// height]`): one uniform scale, centred, never stretched and never
    /// clipped. What the renderer paints through.
    #[must_use]
    pub fn fit(&self, dest: [f64; 4]) -> PictureFit {
        let [x, y, width, height] = dest;
        let [bounds_x, bounds_y, bounds_w, bounds_h] = self.bounds;
        let scale = (width / bounds_w).min(height / bounds_h);
        let left = x + (width - scale * bounds_w) / 2.0;
        let top = y + (height - scale * bounds_h) / 2.0;
        PictureFit {
            scale,
            offset: [left - scale * bounds_x, top - scale * bounds_y],
            picture: [left, top, scale * bounds_w, scale * bounds_h],
        }
    }

    /// INVERSE: a point given as `0..1` of the picture box (y down, as the
    /// screen), in the layout's hint space.
    #[must_use]
    pub fn picture_to_hint(&self, point: [f32; 2]) -> [f64; 2] {
        let [x, y, width, height] = self.bounds;
        [
            x + f64::from(point[0]) * width,
            y + f64::from(point[1]) * height,
        ]
    }

    /// INVERSE: a point given as `0..1` of the picture box, in pattern
    /// space (lamp box centred, long side −1…1, y up). Not clamped.
    #[must_use]
    pub fn picture_to_pattern(&self, point: [f32; 2]) -> [f32; 2] {
        let [x, y] = self.picture_to_hint(point);
        // Hint space IS the render's pixel space (`width_hint`/`height_hint`
        // = the fixture's render size), so the engine's pixel → pattern map
        // applies as it is.
        let pixel = |value: f64| (value * Q16_ONE).round().clamp(-2.0e9, 2.0e9) as i32;
        let [px, py] = self.pattern.map_point([pixel(x), pixel(y)]);
        [
            (f64::from(px) / Q16_ONE) as f32,
            (f64::from(py) / Q16_ONE) as f32,
        ]
    }

    /// The hint space this frame was computed in.
    #[must_use]
    pub fn hint(&self) -> [u32; 2] {
        self.hint
    }
}

/// One lamp centre in the layout's hint space — normalized, clamped to
/// `0..1`, then × the hint per axis: the renderer's own positions.
#[must_use]
pub fn lamp_hint_position(layout: &ControlLayout2d, center: [f32; 2]) -> [f32; 2] {
    let hint = [
        layout.width_hint.max(1) as f32,
        layout.height_hint.max(1) as f32,
    ];
    [
        center[0].clamp(0.0, 1.0) * hint[0],
        center[1].clamp(0.0, 1.0) * hint[1],
    ]
}

/// The smallest a cell may be, in hint units: the layout's declared lamp
/// footprint (`ControlLamp2d::radius`, normalized against the larger hint
/// dimension), the median across lamps — one outsized declaration does not
/// inflate everyone's floor.
#[must_use]
pub fn lamp_footprint_floor(layout: &ControlLayout2d) -> f32 {
    let mut radii: Vec<f32> = layout.lamps.iter().map(|lamp| lamp.radius).collect();
    radii.sort_by(f32::total_cmp);
    let max_hint = layout.width_hint.max(1).max(layout.height_hint.max(1)) as f32;
    radii
        .get(radii.len() / 2)
        .map_or(f32::EPSILON, |radius| (radius * max_hint).max(f32::EPSILON))
}

/// The lamp-centre box as the engine's [`ScopeGeometry`]: the same Q16.16
/// conversion and clamp `ScopeGeometry::from_mapping` streams. Pitch is
/// unused by the map.
fn lamp_scope(layout: &ControlLayout2d, hint: [u32; 2]) -> ScopeGeometry {
    let mut min = [i32::MAX; 2];
    let mut max = [i32::MIN; 2];
    for lamp in &layout.lamps {
        let point = [
            normalized_q16_to_pixel_q16(normalized_f32_to_q16(lamp.center[0]), hint[0]),
            normalized_q16_to_pixel_q16(normalized_f32_to_q16(lamp.center[1]), hint[1]),
        ];
        for axis in 0..2 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    ScopeGeometry {
        min_q16: min,
        max_q16: max,
        pitch_px: 0.0,
        lamp_count: layout.lamps.len() as u32,
    }
}

/// The median over points of the distance to their nearest non-coincident
/// neighbour; `None` when no point has one. A sweep along the longer axis:
/// the same exact value `point_cells` measures, without its grid.
fn median_nearest_neighbour(points: &[[f64; 2]]) -> Option<f64> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for point in points {
        for axis in 0..2 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let axis = usize::from(max[1] - min[1] > max[0] - min[0]);
    let mut order: Vec<usize> = (0..points.len()).collect();
    order.sort_by(|a, b| points[*a][axis].total_cmp(&points[*b][axis]));
    let mut nearest = Vec::with_capacity(points.len());
    for (rank, &index) in order.iter().enumerate() {
        let point = points[index];
        let mut best = f64::INFINITY;
        let mut consider = |other: usize| -> bool {
            let along = (points[other][axis] - point[axis]).abs();
            if along > best {
                return false;
            }
            let gap = distance(point, points[other]);
            if gap > COINCIDENT_EPS {
                best = best.min(gap);
            }
            true
        };
        for &other in &order[rank + 1..] {
            if !consider(other) {
                break;
            }
        }
        for &other in order[..rank].iter().rev() {
            if !consider(other) {
                break;
            }
        }
        if best.is_finite() {
            nearest.push(best);
        }
    }
    if nearest.is_empty() {
        return None;
    }
    nearest.sort_by(f64::total_cmp);
    Some(nearest[nearest.len() / 2])
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use lpc_model::{ControlLamp2d, Revision};

    use super::*;

    /// The ISSUE's case: a wide strip of lamps in a non-square render,
    /// drawn letterboxed into a square box. The picture's centre is the
    /// lamp box's centre in pattern space, y is up, and the lamp box's
    /// ends are ±1 on its long side.
    #[test]
    fn the_picture_centre_is_the_lamp_box_centre_and_y_is_up() {
        // 5 × 2 lamps over x 0.1..0.9, y 0.4..0.6 of a 200 × 100 render.
        let layout = grid_layout(200, 100, 5, 2, [0.1, 0.4], [0.9, 0.6]);
        let frame = LampPictureFrame::of(&layout).expect("lamps");

        let centre = frame.picture_to_pattern([0.5, 0.5]);
        assert!(near(centre, [0.0, 0.0]), "{centre:?}");

        // Letterboxed into a square: the picture is wider than tall, so
        // it spans the box's width and sits centred vertically.
        let fit = frame.fit([0.0, 0.0, 100.0, 100.0]);
        let [x, y, width, height] = fit.picture;
        assert!((x - 0.0).abs() < 1e-9 && (width - 100.0).abs() < 1e-9);
        assert!(height < 100.0 && (y - (100.0 - height) / 2.0).abs() < 1e-9);
        assert!(near(fit.picture_point([50.0, 50.0]), [0.5, 0.5]));

        // Above the centre on screen is up in pattern space.
        let above = frame.picture_to_pattern([0.5, 0.25]);
        assert!(above[1] > 0.0, "{above:?}");

        // The lamp box's left and right lamps are −1 and +1: the picture
        // reaches past them by the cells' inset reach.
        let [bx, _, bw, _] = frame.bounds();
        let left_lamp = [((20.0 - bx) / bw) as f32, 0.5];
        let right_lamp = [((180.0 - bx) / bw) as f32, 0.5];
        assert!(near(frame.picture_to_pattern(left_lamp), [-1.0, 0.0]));
        assert!(near(frame.picture_to_pattern(right_lamp), [1.0, 0.0]));
        // Hint 40..60 is the short side: the top row is 10 above the
        // centre, 10 / 80 of the long half-side, and up is positive.
        let top_lamp = [0.5, ((40.0 - frame.bounds()[1]) / frame.bounds()[3]) as f32];
        assert!(near(frame.picture_to_pattern(top_lamp), [0.0, 0.125]));
    }

    /// The picture is the lamp-centre box grown by 0.9 of the cell radius,
    /// and the radius is 0.92 of the median spacing on a dense field:
    /// about 0.83 of a lamp spacing on every side.
    #[test]
    fn the_picture_reaches_past_the_lamps_by_the_cells_inset_radius() {
        // Pitch 40 hint units on x, 20 on y (5 × 2 over 160 × 20).
        let layout = grid_layout(200, 100, 5, 2, [0.1, 0.4], [0.9, 0.6]);
        let frame = LampPictureFrame::of(&layout).expect("lamps");
        let radius = 0.92 * 20.0;
        assert!((frame.cell_radius() - radius).abs() < 1e-4);
        let reach = 0.9 * radius;
        let [x, y, width, height] = frame.bounds();
        assert!((x - (20.0 - reach)).abs() < 1e-4, "{x}");
        assert!((y - (40.0 - reach)).abs() < 1e-4, "{y}");
        assert!((width - (160.0 + 2.0 * reach)).abs() < 1e-4);
        assert!((height - (20.0 + 2.0 * reach)).abs() < 1e-4);
    }

    /// A finger off the lamps is not clamped: it keeps going in pattern
    /// space.
    #[test]
    fn a_point_outside_the_picture_is_not_clamped() {
        let layout = grid_layout(100, 100, 3, 3, [0.25, 0.25], [0.75, 0.75]);
        let frame = LampPictureFrame::of(&layout).expect("lamps");
        let past = frame.picture_to_pattern([1.5, -0.5]);
        assert!(past[0] > 1.0 && past[1] > 1.0, "{past:?}");
    }

    /// One lamp, or none: a frame that maps everything to the origin, and
    /// no frame at all.
    #[test]
    fn a_single_lamp_maps_to_the_origin_and_no_lamps_have_no_frame() {
        let one = grid_layout(64, 32, 1, 1, [0.5, 0.5], [0.5, 0.5]);
        let frame = LampPictureFrame::of(&one).expect("one lamp");
        assert!(frame.bounds()[2] > 0.0 && frame.bounds()[3] > 0.0);
        assert!(near(frame.picture_to_pattern([0.9, 0.1]), [0.0, 0.0]));
        let none = ControlLayout2d::new(Revision::new(1), 64, 32, Vec::new());
        assert!(LampPictureFrame::of(&none).is_none());
    }

    /// The sweep finds the same nearest neighbours a brute-force search
    /// does, on a strip standing on its end and on a jittered field.
    #[test]
    fn the_nearest_neighbour_sweep_matches_brute_force() {
        let column: Vec<[f64; 2]> = (0..30).map(|i| [5.0, f64::from(i) * 3.5]).collect();
        let jitter: Vec<[f64; 2]> = (0..200)
            .map(|i| {
                let i = f64::from(i);
                [(i * 7.3) % 41.0 + (i * 0.37).sin(), (i * 3.1) % 29.0]
            })
            .collect();
        for points in [column, jitter] {
            let mut brute: Vec<f64> = points
                .iter()
                .enumerate()
                .filter_map(|(index, point)| {
                    points
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != index)
                        .map(|(_, other)| distance(*point, *other))
                        .filter(|gap| *gap > COINCIDENT_EPS)
                        .min_by(f64::total_cmp)
                })
                .collect();
            brute.sort_by(f64::total_cmp);
            assert_eq!(
                median_nearest_neighbour(&points),
                Some(brute[brute.len() / 2])
            );
        }
    }

    fn grid_layout(
        width: u32,
        height: u32,
        columns: u32,
        rows: u32,
        from: [f32; 2],
        to: [f32; 2],
    ) -> ControlLayout2d {
        let step = |count: u32, axis: usize| {
            if count > 1 {
                (to[axis] - from[axis]) / (count - 1) as f32
            } else {
                0.0
            }
        };
        let lamps = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (row, column)))
            .enumerate()
            .map(|(index, (row, column))| ControlLamp2d {
                lamp_index: index as u32,
                sample_start: index as u32 * 3,
                center: [
                    from[0] + column as f32 * step(columns, 0),
                    from[1] + row as f32 * step(rows, 1),
                ],
                radius: 0.01,
            })
            .collect();
        ControlLayout2d::new(Revision::new(1), width, height, lamps)
    }

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }
}
