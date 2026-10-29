//! Voronoi LAMP CELLS for the product display: each lamp claims a small
//! polygon of territory, so a control product's layout reads as a mosaic
//! of lamps rather than a row of disconnected dots. [`LampView`] paints
//! these polygons; this file is only their geometry.
//!
//! The display layout carries positions only (strand grain is invisible
//! at preview size, ruled at the design-language G1), so every lamp
//! shares one radius — [`NN_FRACTION`] × the median nearest-neighbour
//! distance, floored by the layout's own lamp footprint. Both inputs ride
//! whatever unit the layout chose, so the look is scale-free: never an
//! absolute unit.
//!
//! A cell is geometric by construction: a 12-gon of that radius,
//! Sutherland–Hodgman clipped against the perpendicular-bisector
//! half-plane of every neighbour close enough to matter, then inset ~10%
//! toward its center so adjacent cells keep a visible seam. Every clip
//! half-plane strictly contains the cell's own center, so a cell can
//! shrink but never vanish; and the construction cannot overlap pairwise
//! — centers closer than a cell's diameter share a bisector clip, while a
//! neighbour beyond it cannot reach the shared bisector at all.
//!
//! Neighbour candidates come from a uniform grid ([`SeedGrid`]) sized by
//! the seed radius, so clipping stays O(n·k) at whole-product scale (a
//! dome is ~1500 lamps). Coincident lamps have no bisector and simply
//! share territory.
//!
//! This began as the point-field mode of the mapping canvas's cell layer
//! (`lpa-mapping-editor`); the product display is its one remaining user.
//!
//! [`LampView`]: super::lamp_view::LampView

/// Cell radius = `NN_FRACTION` × the field's MEDIAN nearest-neighbour
/// distance: just under half-pitch-per-side so neighbouring 12-gons
/// overlap and the bisector clip, not the disc edge, draws the shared
/// wall. The median (not a per-lamp distance) keeps authoring jitter from
/// becoming a salad of cell sizes.
const NN_FRACTION: f64 = 0.92;

/// 12 sides read as round at cell scale and stay cheap to clip.
const CELL_SIDES: usize = 12;

/// Fraction of the clipped cell kept after the seam inset (~10% toward
/// the center).
const INSET_KEEP: f64 = 0.9;

/// Centers closer than this are coincident: no bisector exists.
const COINCIDENT_EPS: f64 = 1e-9;

/// The voronoi cells of a bare point field — a product display layout,
/// which knows lamp positions and the physical footprint but no strand
/// structure. One shared radius: [`NN_FRACTION`] × the median
/// nearest-neighbour distance (coincident pairs excluded), floored by
/// `floor_radius` (the layout's own lamp footprint — however sparse the
/// field, a cell never shrinks below the lamp itself).
///
/// One polygon per position, in input order: convex, wound
/// positive-shoelace in y-down space, and empty only when coincident
/// lamps' shared territory was entirely claimed (the index never shifts).
#[must_use]
pub(crate) fn point_cells(positions: &[[f32; 2]], floor_radius: f32) -> Vec<Vec<[f32; 2]>> {
    let positions: Vec<[f64; 2]> = positions
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    let floor = f64::from(floor_radius).max(f64::EPSILON);
    let radius =
        median_nearest_neighbour(&positions).map_or(floor, |nn| (NN_FRACTION * nn).max(floor));
    let seeds: Vec<Seed> = positions
        .into_iter()
        .map(|center| Seed { center, radius })
        .collect();
    cells_from_seeds(&seeds)
}

/// Where a cell grows from and how far.
struct Seed {
    center: [f64; 2],
    radius: f64,
}

/// Median over points of the distance to their nearest non-coincident
/// neighbour — the field's pitch as laid out, immune to per-lamp jitter.
/// `None` when no point has a non-coincident neighbour.
fn median_nearest_neighbour(positions: &[[f64; 2]]) -> Option<f64> {
    let grid = SeedGrid::over(positions.iter().map(|p| (*p, 0.0)));
    let mut nearest: Vec<f64> = Vec::with_capacity(positions.len());
    for (index, position) in positions.iter().enumerate() {
        let mut best = f64::INFINITY;
        // Expand the candidate ring until a hit exists and the ring
        // already covers its distance — the standard grid NN walk.
        let mut ring = 1_usize;
        loop {
            for other in grid.candidates(*position, ring) {
                if other == index {
                    continue;
                }
                let gap = dist(*position, positions[other]);
                if gap > COINCIDENT_EPS {
                    best = best.min(gap);
                }
            }
            let covered = grid.cell * ring as f64;
            if (best <= covered) || !grid.can_grow(*position, ring) {
                break;
            }
            ring += 1;
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

/// Grow every seed's 12-gon and clip it against the bisector of each
/// close-enough neighbour. Candidates come from the grid; they are
/// visited in index order so the result is independent of bucket layout.
fn cells_from_seeds(seeds: &[Seed]) -> Vec<Vec<[f32; 2]>> {
    let tau = std::f64::consts::TAU;
    let grid = SeedGrid::over(seeds.iter().map(|s| (s.center, s.radius)));
    let mut candidates: Vec<usize> = Vec::new();
    seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| {
            // Positive-shoelace 12-gon in y-down space.
            let mut polygon: Vec<[f64; 2]> = (0..CELL_SIDES)
                .map(|k| {
                    let a = tau * (k as f64) / (CELL_SIDES as f64);
                    [
                        seed.center[0] + seed.radius * a.cos(),
                        seed.center[1] + seed.radius * a.sin(),
                    ]
                })
                .collect();
            // Any clipping neighbour satisfies gap < 2·radius ≤ the
            // grid's coverage at ring 1, so ring 1 sees them all.
            candidates.clear();
            candidates.extend(grid.candidates(seed.center, 1));
            candidates.sort_unstable();
            for &other_index in &candidates {
                if other_index == index {
                    continue;
                }
                let other = &seeds[other_index];
                let gap = dist(seed.center, other.center);
                if gap <= COINCIDENT_EPS || gap >= 2.0 * seed.radius {
                    continue;
                }
                let mid = [
                    (seed.center[0] + other.center[0]) / 2.0,
                    (seed.center[1] + other.center[1]) / 2.0,
                ];
                let toward = [
                    (other.center[0] - seed.center[0]) / gap,
                    (other.center[1] - seed.center[1]) / gap,
                ];
                polygon = clip_halfplane(polygon, mid, toward);
                if polygon.is_empty() {
                    break;
                }
            }
            polygon
                .into_iter()
                .map(|v| {
                    [
                        (seed.center[0] + (v[0] - seed.center[0]) * INSET_KEEP) as f32,
                        (seed.center[1] + (v[1] - seed.center[1]) * INSET_KEEP) as f32,
                    ]
                })
                .collect()
        })
        .collect()
}

/// Keep the part of `polygon` on the seed's side of the bisector: points
/// `v` with `(v − mid) · toward ≤ 0`, `toward` being the unit vector at
/// the neighbour. Standard Sutherland–Hodgman against one half-plane.
fn clip_halfplane(polygon: Vec<[f64; 2]>, mid: [f64; 2], toward: [f64; 2]) -> Vec<[f64; 2]> {
    let side = |v: [f64; 2]| (v[0] - mid[0]) * toward[0] + (v[1] - mid[1]) * toward[1];
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
            let t = sa / (sa - sb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    if out.len() < 3 { Vec::new() } else { out }
}

/// A uniform bucket grid over seed centers. The cell edge is at least
/// 2 × the largest seed radius, so every pair that can interact (bisector
/// clip needs `gap < 2·radius`) sits within one ring of buckets; `ring`
/// widens the walk for nearest-neighbour searches that must look
/// further. Bucket count is capped so a pathological spread cannot
/// allocate unbounded memory — a coarser grid only adds candidates,
/// never loses one.
struct SeedGrid {
    origin: [f64; 2],
    cell: f64,
    cols: usize,
    rows: usize,
    buckets: Vec<Vec<usize>>,
}

impl SeedGrid {
    fn over(seeds: impl Iterator<Item = ([f64; 2], f64)> + Clone) -> Self {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        let mut max_radius = 0.0_f64;
        let mut count = 0_usize;
        for (center, radius) in seeds.clone() {
            min[0] = min[0].min(center[0]);
            min[1] = min[1].min(center[1]);
            max[0] = max[0].max(center[0]);
            max[1] = max[1].max(center[1]);
            max_radius = max_radius.max(radius);
            count += 1;
        }
        if count == 0 {
            return Self {
                origin: [0.0; 2],
                cell: 1.0,
                cols: 1,
                rows: 1,
                buckets: vec![Vec::new()],
            };
        }
        let span = [(max[0] - min[0]).max(0.0), (max[1] - min[1]).max(0.0)];
        // ≥ 2·max radius (the interaction reach), and never finer than
        // ~1 bucket per point per axis.
        let per_axis = ((count as f64).sqrt().ceil()).max(1.0);
        let cell = (2.0 * max_radius)
            .max(span[0] / per_axis)
            .max(span[1] / per_axis)
            .max(f64::EPSILON);
        let cols = ((span[0] / cell).floor() as usize + 1).max(1);
        let rows = ((span[1] / cell).floor() as usize + 1).max(1);
        let mut buckets = vec![Vec::new(); cols * rows];
        for (index, (center, _)) in seeds.enumerate() {
            let (col, row) = Self::bucket_of(min, cell, cols, rows, center);
            buckets[row * cols + col].push(index);
        }
        Self {
            origin: min,
            cell,
            cols,
            rows,
            buckets,
        }
    }

    fn bucket_of(
        origin: [f64; 2],
        cell: f64,
        cols: usize,
        rows: usize,
        p: [f64; 2],
    ) -> (usize, usize) {
        let col = (((p[0] - origin[0]) / cell).floor() as isize).clamp(0, cols as isize - 1);
        let row = (((p[1] - origin[1]) / cell).floor() as isize).clamp(0, rows as isize - 1);
        (col as usize, row as usize)
    }

    /// Indices in the `(2·ring + 1)²` bucket block around `p`.
    fn candidates(&self, p: [f64; 2], ring: usize) -> impl Iterator<Item = usize> + '_ {
        let (col, row) = Self::bucket_of(self.origin, self.cell, self.cols, self.rows, p);
        let ring = ring as isize;
        let col0 = (col as isize - ring).max(0) as usize;
        let col1 = ((col as isize + ring) as usize).min(self.cols - 1);
        let row0 = (row as isize - ring).max(0) as usize;
        let row1 = ((row as isize + ring) as usize).min(self.rows - 1);
        (row0..=row1).flat_map(move |r| {
            (col0..=col1).flat_map(move |c| self.buckets[r * self.cols + c].iter().copied())
        })
    }

    /// Whether widening the ring around `p` can reach any new bucket.
    fn can_grow(&self, p: [f64; 2], ring: usize) -> bool {
        let (col, row) = Self::bucket_of(self.origin, self.cell, self.cols, self.rows, p);
        let ring = ring as isize;
        col as isize - ring > 0
            || row as isize - ring > 0
            || ((col as isize + ring) as usize) < self.cols - 1
            || ((row as isize + ring) as usize) < self.rows - 1
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a uniform line every cell is the 0.92 × pitch 12-gon clipped at
    /// the bisectors, then inset: an interior wall sits at half-pitch ×
    /// the inset on both sides, and an end cell's outward vertex (on the
    /// axis, unclipped) reaches the full radius × the inset.
    #[test]
    fn point_cells_on_a_uniform_line_meet_at_the_bisectors() {
        let pitch = 4.0_f64;
        let positions: Vec<[f32; 2]> = (0..6).map(|i| [i as f32 * pitch as f32, 0.0]).collect();
        let cells = point_cells(&positions, 1.0);
        assert_eq!(cells.len(), 6);
        let wall = pitch / 2.0 * INSET_KEEP;
        let reach = NN_FRACTION * pitch * INSET_KEEP;
        for (index, polygon) in cells.iter().enumerate() {
            let seed_x = index as f64 * pitch;
            let left = polygon
                .iter()
                .map(|v| f64::from(v[0]))
                .fold(f64::INFINITY, f64::min);
            let right = polygon
                .iter()
                .map(|v| f64::from(v[0]))
                .fold(f64::NEG_INFINITY, f64::max);
            let want_left = if index == 0 { reach } else { wall };
            let want_right = if index == 5 { reach } else { wall };
            assert!(
                (seed_x - left - want_left).abs() < 1e-4,
                "cell {index} left edge at {left}"
            );
            assert!(
                (right - seed_x - want_right).abs() < 1e-4,
                "cell {index} right edge at {right}"
            );
        }
    }

    /// A lone point (and an all-coincident field) has no neighbour
    /// distance to measure: the floor is the whole radius, and coincident
    /// seeds share territory rather than clipping each other away.
    #[test]
    fn point_cells_fall_back_to_the_floor_footprint() {
        let lone = point_cells(&[[5.0, 5.0]], 4.0);
        assert_eq!(lone.len(), 1);
        assert_eq!(lone[0].len(), CELL_SIDES);
        for v in &lone[0] {
            let d = dist([f64::from(v[0]), f64::from(v[1])], [5.0, 5.0]);
            assert!((d - 4.0 * INSET_KEEP).abs() < 1e-3, "{v:?} at {d}");
        }
        let coincident = point_cells(&[[5.0, 5.0], [5.0, 5.0]], 4.0);
        assert_eq!(coincident.len(), 2);
        for polygon in &coincident {
            assert_eq!(polygon.len(), CELL_SIDES, "shared territory");
        }
    }

    /// The footprint floor wins over a tighter pitch — a dense field of
    /// physically large lamps keeps lamp-sized cells (the bisector clip,
    /// not the radius, prevents overlap). The fyeah sign's numbers:
    /// pitch ~17.5, diameter 26.
    #[test]
    fn point_cells_floor_beats_a_tighter_median_pitch() {
        let positions: Vec<[f32; 2]> = (0..6).map(|i| [i as f32 * 17.5, 0.0]).collect();
        let cells = point_cells(&positions, 13.0);
        // 0.92 × 17.5 = 16.1 > 13: the median wins here. Shrink the
        // pitch below the floor and the floor takes over.
        let tight: Vec<[f32; 2]> = (0..6).map(|i| [i as f32 * 10.0, 0.0]).collect();
        let tight_cells = point_cells(&tight, 13.0);
        // An end cell's outward vertex sits at the full radius (nothing
        // clips it): footprint 13 × inset, not 0.92 × 10 × inset.
        let end = tight_cells.last().expect("cells");
        let max_reach = end
            .iter()
            .map(|v| dist([f64::from(v[0]), f64::from(v[1])], [50.0, 0.0]))
            .fold(0.0, f64::max);
        assert!(
            (max_reach - 13.0 * INSET_KEEP).abs() < 1e-3,
            "floor radius should shape the cell, got reach {max_reach}"
        );
        assert!(!cells.is_empty());
        for (index, polygon) in cells.iter().chain(&tight_cells).enumerate() {
            assert!(!polygon.is_empty(), "cell {index} vanished");
        }
    }

    /// Grid-accelerated clipping keeps the brute-force guarantees on a
    /// jittered 2D field: no cell overlaps another, none vanishes, and
    /// every cell contains its own seed.
    #[test]
    fn point_cells_tile_a_jittered_field_without_overlap() {
        // Deterministic jitter — no RNG in tests.
        let mut state = 0x2F6E_2B1Fu32;
        let mut jitter = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(state >> 8) / f64::from(1u32 << 24) - 0.5) as f32
        };
        let positions: Vec<[f32; 2]> = (0..120)
            .map(|i| {
                let (col, row) = (i % 12, i / 12);
                [
                    col as f32 * 5.0 + jitter() * 2.0,
                    row as f32 * 5.0 + jitter() * 2.0,
                ]
            })
            .collect();
        let cells = point_cells(&positions, 0.5);
        assert_eq!(cells.len(), positions.len());
        for (i, polygon) in cells.iter().enumerate() {
            assert!(!polygon.is_empty(), "cell {i} vanished");
            assert!(
                point_in_polygon(polygon, positions[i]),
                "cell {i} lost its seed"
            );
            for (j, other) in cells.iter().enumerate() {
                if i == j {
                    continue;
                }
                for v in polygon {
                    assert!(
                        !point_in_polygon(other, *v),
                        "cell {i} vertex {v:?} inside cell {j}"
                    );
                }
            }
        }
    }

    /// The display perf contract: a dome-scale point field (1500 lamps)
    /// builds its cells far under a frame even in debug — the uniform
    /// grid is what keeps the neighbour searches from going quadratic.
    #[test]
    fn a_dome_scale_point_field_computes_cells_quickly() {
        let mut state = 0x00C0_FFEEu32;
        let mut jitter = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(state >> 8) / f64::from(1u32 << 24) - 0.5) as f32
        };
        let positions: Vec<[f32; 2]> = (0..1500)
            .map(|i| {
                let (col, row) = (i % 50, i / 50);
                [
                    col as f32 * 4.0 + jitter() * 1.5,
                    row as f32 * 4.0 + jitter() * 1.5,
                ]
            })
            .collect();
        let start = std::time::Instant::now();
        let cells = point_cells(&positions, 1.0);
        let elapsed = start.elapsed();
        assert_eq!(cells.len(), 1500);
        for (i, polygon) in cells.iter().enumerate() {
            assert!(!polygon.is_empty(), "cell {i} vanished");
        }
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "grid failed to keep the build sub-frame: {elapsed:?}"
        );
    }

    /// Nonzero-winding containment of `point` in one polygon (boundary
    /// points count as outside on some edges — fine for the strict
    /// interior probes above).
    fn point_in_polygon(polygon: &[[f32; 2]], point: [f32; 2]) -> bool {
        if polygon.len() < 3 {
            return false;
        }
        let p = [f64::from(point[0]), f64::from(point[1])];
        let cross = |a: [f64; 2], b: [f64; 2]| {
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
        };
        let mut winding = 0_i32;
        let last = polygon[polygon.len() - 1];
        let mut a = [f64::from(last[0]), f64::from(last[1])];
        for vertex in polygon {
            let b = [f64::from(vertex[0]), f64::from(vertex[1])];
            if a[1] <= p[1] {
                if b[1] > p[1] && cross(a, b) > 0.0 {
                    winding += 1;
                }
            } else if b[1] <= p[1] && cross(a, b) < 0.0 {
                winding -= 1;
            }
            a = b;
        }
        winding != 0
    }
}
