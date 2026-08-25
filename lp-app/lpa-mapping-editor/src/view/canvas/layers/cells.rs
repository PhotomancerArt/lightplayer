//! Voronoi LAMP CELLS: each displayed lamp claims a small polygon of
//! territory, so a selected path object reads as a mosaic of lamps rather
//! than a row of disconnected dots.
//!
//! The spike proved the LOOK but built it from a distance field, whose
//! floating-island cells and unfilled gaps are rejected defect classes.
//! Here a cell is geometric by construction: a 12-gon of its strand's
//! pitch-derived radius (median spacing, floored by the doc's declared
//! lamp footprint — never an absolute unit), Sutherland–Hodgman clipped
//! against the perpendicular-bisector half-plane of every same-object
//! neighbour close enough to matter, then inset ~10% toward its center so
//! adjacent cells keep a visible seam. Every clip half-plane strictly
//! contains the cell's own center, so a cell can shrink but never vanish;
//! and the construction cannot overlap pairwise — centers closer than the
//! current cell's diameter share a bisector clip, while a neighbour
//! beyond its own diameter cannot reach the shared bisector at all.
//!
//! Alignment shifts a cell's CENTER off the lamp by its radius along the
//! strand normal (outline.rs's inside/outside definition), so the cell's
//! edge kisses the path like an aligned stroke. Neighbour distances are
//! measured at CELL CENTERS, so a shifted ribbon tiles against itself.
//!
//! Two seeding modes share one clipping core:
//!
//! - [`lamp_cells`] — the mapping canvas: strand-seeded, per-strand
//!   median pitch, alignment shift along the strand normal.
//! - [`point_cells`] — product previews (`lamp_view.rs`): the display
//!   layout carries positions only (strand grain is invisible at preview
//!   size, ruled at the design-language G1), so every seed shares one
//!   radius — 0.92 × the median nearest-neighbour distance, floored by
//!   the layout's own lamp footprint. Still scale-free: both inputs ride
//!   whatever unit the layout chose.
//!
//! Neighbour candidates come from a uniform grid ([`SeedGrid`]) sized by
//! the largest seed radius, so clipping stays O(n·k) at whole-product
//! scale (a dome preview is ~1500 lamps — the editor's per-object few
//! hundred never needed the grid, but takes it harmlessly). Coincident
//! lamps have no bisector and simply share territory.

use lpc_mapping::PathAlign;

use super::outline::{lamp_normal, object_centroid, strand_inside_sign};

/// Cell radius = `NN_FRACTION` × the strand's MEDIAN lamp spacing: just
/// under half-pitch-per-side so neighbouring 12-gons overlap and the
/// bisector clip, not the disc edge, draws the shared wall.
///
/// Two G1 lessons live in this derivation. Per-LAMP nearest-neighbour
/// radii amplified authoring jitter into wildly different cell sizes on
/// tight freehand paths (the fyeah sign) — the median makes a strand's
/// ribbon uniform. And absolute own-space clamps assumed a scale that
/// doc coordinates simply do not have (they are arbitrary per doc): the
/// peach, authored ~5× coarser, had its mesh capped back into separated
/// balls. Every length here is now derived from the doc's own numbers —
/// spacing statistics and `sample_diameter` — which share whatever unit
/// the doc chose, so the look survives any authoring scale.
const NN_FRACTION: f64 = 0.92;

/// 12 sides read as round at cell scale and stay cheap to clip.
const CELL_SIDES: usize = 12;

/// Fraction of the clipped cell kept after the seam inset (~10% toward
/// the center).
const INSET_KEEP: f64 = 0.9;

/// Centers closer than this are coincident: no bisector exists.
const COINCIDENT_EPS: f64 = 1e-9;

/// One displayed lamp's territory.
#[derive(Clone, Debug, PartialEq)]
pub struct LampCell {
    /// Index into the object's DISPLAYED lamp list (subsample grain).
    pub lamp: usize,
    /// Convex, possibly clipped; wound positive-shoelace in y-down space.
    pub polygon: Vec<[f32; 2]>,
}

/// Where a cell grows from and how far: the aligned center plus the local
/// radius. Split from [`lamp_cells`] so tests can check the aligned
/// centers without reverse-engineering polygons.
struct Seed {
    center: [f64; 2],
    radius: f64,
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

/// Median of a strand's consecutive lamp gaps (zero-length duplicates
/// skipped) — the strand's pitch as authored, immune to per-lamp jitter
/// and to one outlier jump.
fn median_spacing(strand: &[[f32; 2]]) -> Option<f64> {
    let mut gaps: Vec<f64> = strand
        .windows(2)
        .map(|w| {
            dist(
                [f64::from(w[0][0]), f64::from(w[0][1])],
                [f64::from(w[1][0]), f64::from(w[1][1])],
            )
        })
        .filter(|g| *g > COINCIDENT_EPS)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_by(f64::total_cmp);
    Some(gaps[gaps.len() / 2])
}

fn cell_seeds(strands: &[Vec<[f32; 2]>], align: PathAlign, sample_diameter: f64) -> Vec<Seed> {
    let positions: Vec<[f64; 2]> = strands
        .iter()
        .flatten()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    // The physical lamp footprint is the radius FLOOR: however sparse the
    // strand, a cell never shrinks below the lamp the doc itself declared.
    // It is also the whole answer for strands with no spacing to measure.
    let floor = (sample_diameter / 2.0).max(f64::EPSILON);
    // A strand with one lamp (or all-coincident lamps) borrows the
    // object's overall pitch before falling back to the footprint, so a
    // lone tail lamp matches its siblings instead of shrinking.
    let object_median = {
        let mut medians: Vec<f64> = strands.iter().filter_map(|s| median_spacing(s)).collect();
        medians.sort_by(f64::total_cmp);
        medians.get(medians.len() / 2).copied()
    };
    let centroid = object_centroid(strands);
    let mut seeds = Vec::with_capacity(positions.len());
    let mut index = 0;
    for strand in strands {
        // Which way is "off the path": 0 for On, else the strand's
        // inside/outside sign — the same deterministic definition the
        // outline uses, so cell ribbons and bands agree.
        let side = match align {
            PathAlign::On => 0.0,
            PathAlign::Inside => strand_inside_sign(strand, centroid),
            PathAlign::Outside => -strand_inside_sign(strand, centroid),
        };
        // ONE radius per strand: uniform cells are what make the ribbon
        // read as a mesh instead of a row of mismatched pebbles.
        let pitch = median_spacing(strand).or(object_median);
        let radius = pitch.map_or(floor, |p| (NN_FRACTION * p).max(floor));
        for lamp in 0..strand.len() {
            let position = positions[index];
            let center = match lamp_normal(strand, lamp) {
                Some(normal) if side != 0.0 => [
                    position[0] + side * radius * normal[0],
                    position[1] + side * radius * normal[1],
                ],
                // No direction (single-lamp strand) or On: stay put.
                _ => position,
            };
            seeds.push(Seed { center, radius });
            index += 1;
        }
    }
    seeds
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

/// The voronoi lamp cells of one object's strands, aligned like its
/// outline. One cell per displayed lamp, in display order — a cell whose
/// territory was entirely claimed keeps an empty polygon rather than
/// shifting every later index.
#[must_use]
pub fn lamp_cells(
    strands: &[Vec<[f32; 2]>],
    align: PathAlign,
    sample_diameter: f32,
) -> Vec<LampCell> {
    cells_from_seeds(&cell_seeds(strands, align, f64::from(sample_diameter)))
}

/// The voronoi cells of a bare point field — a product preview's display
/// layout, which knows lamp positions and the physical footprint but no
/// strand structure. One shared radius: 0.92 × the median
/// nearest-neighbour distance (coincident pairs excluded), floored by
/// `floor_radius` (the layout's own lamp footprint — however sparse the
/// field, a cell never shrinks below the lamp itself). One cell per
/// position, in input order.
#[must_use]
pub fn point_cells(positions: &[[f32; 2]], floor_radius: f32) -> Vec<LampCell> {
    let positions: Vec<[f64; 2]> = positions
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    let floor = f64::from(floor_radius).max(f64::EPSILON);
    let radius = median_nearest_neighbour(&positions)
        .map_or(floor, |nn| (NN_FRACTION * nn).max(floor));
    let seeds: Vec<Seed> = positions
        .into_iter()
        .map(|center| Seed { center, radius })
        .collect();
    cells_from_seeds(&seeds)
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
/// close-enough neighbour — the shared core of both seeding modes.
/// Candidates come from the grid; they are visited in index order so the
/// result is independent of bucket layout.
fn cells_from_seeds(seeds: &[Seed]) -> Vec<LampCell> {
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
            let polygon = polygon
                .into_iter()
                .map(|v| {
                    [
                        (seed.center[0] + (v[0] - seed.center[0]) * INSET_KEEP) as f32,
                        (seed.center[1] + (v[1] - seed.center[1]) * INSET_KEEP) as f32,
                    ]
                })
                .collect();
            LampCell {
                lamp: index,
                polygon,
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use super::super::outline::{aligned_outline, dist_to_loops, point_in_loops};
    use super::*;

    fn line_lamps(count: usize, pitch: f32) -> Vec<Vec<[f32; 2]>> {
        vec![(0..count).map(|i| [i as f32 * pitch, 0.0]).collect()]
    }

    /// One cell per displayed lamp, indices in display order across
    /// strands.
    #[test]
    fn cell_count_matches_the_displayed_lamps_in_order() {
        let strands = vec![
            (0..5).map(|i| [i as f32 * 4.0, 0.0]).collect::<Vec<_>>(),
            (0..7).map(|i| [i as f32 * 4.0, 30.0]).collect::<Vec<_>>(),
        ];
        let cells = lamp_cells(&strands, PathAlign::On, 2.0);
        assert_eq!(cells.len(), 12);
        for (index, cell) in cells.iter().enumerate() {
            assert_eq!(cell.lamp, index);
        }
    }

    /// A lone lamp has no spacing to measure: its cell is the physical
    /// footprint the doc declared, and nothing else.
    #[test]
    fn a_lone_lamp_gets_a_full_footprint_twelve_gon() {
        let cells = lamp_cells(&[vec![[5.0_f32, 5.0]]], PathAlign::On, 8.0);
        assert_eq!(cells.len(), 1);
        let polygon = &cells[0].polygon;
        assert_eq!(polygon.len(), CELL_SIDES);
        for v in polygon {
            let d = dist([f64::from(v[0]), f64::from(v[1])], [5.0, 5.0]);
            assert!((d - 4.0 * INSET_KEEP).abs() < 1e-3, "{v:?} at {d}");
        }
    }

    /// The two G1 scale regressions, pinned with the real docs' numbers.
    /// Peach: pitch 39 at sample_diameter 26 — the old absolute cap turned
    /// the mesh into separated balls; cells must now MEET at the bisector.
    /// Fyeah: jittered pitch around 17 — per-lamp radii made a size salad;
    /// every cell in a strand now shares the strand's median radius.
    #[test]
    fn cells_mesh_at_peach_scale_and_stay_uniform_under_jitter() {
        // Peach-scale: uniform pitch 39, footprint 26. Adjacent cells
        // share a wall at the bisector: a probe just inside the wall (the
        // seam inset plus a hair) belongs to each cell on its own side.
        let strands = line_lamps(6, 39.0);
        let cells = lamp_cells(&strands, PathAlign::On, 26.0);
        let seam = (1.0 - INSET_KEEP) * NN_FRACTION * 39.0 + 0.5;
        for pair in cells.windows(2) {
            let mid_x = f64::from(pair[0].lamp as u32) * 39.0 + 39.0 / 2.0;
            let probe_a = [(mid_x - seam) as f32, 0.0];
            let probe_b = [(mid_x + seam) as f32, 0.0];
            assert!(
                super::super::outline::point_in_loops(from_ref(&pair[0].polygon), probe_a),
                "cell {} misses its wall at {probe_a:?}",
                pair[0].lamp
            );
            assert!(
                super::super::outline::point_in_loops(from_ref(&pair[1].polygon), probe_b),
                "cell {} misses its wall at {probe_b:?}",
                pair[1].lamp
            );
        }
        // Fyeah-scale: pitch jittering 13..21 — one radius per strand.
        let jittered: Vec<[f32; 2]> = {
            let steps = [13.0_f32, 21.0, 15.0, 19.0, 14.0, 20.0, 17.0];
            let mut x = 0.0;
            let mut pts = vec![[0.0, 0.0]];
            for step in steps {
                x += step;
                pts.push([x, 0.0]);
            }
            pts
        };
        let seeds = cell_seeds(&[jittered], PathAlign::On, 2.0);
        let first = seeds[0].radius;
        for seed in &seeds {
            assert!((seed.radius - first).abs() < 1e-9, "uniform per strand");
        }
        assert!((first - 0.92 * 17.0).abs() < 1e-6, "median pitch drives it");
    }

    /// After bisector clipping (and the seam inset) no cell's vertex sits
    /// inside another cell — the tiling never overlaps, whatever the
    /// alignment.
    #[test]
    fn cells_do_not_overlap_after_clipping() {
        let strands = line_lamps(10, 4.0);
        for align in [PathAlign::On, PathAlign::Inside, PathAlign::Outside] {
            let cells = lamp_cells(&strands, align, 2.0);
            for (i, cell) in cells.iter().enumerate() {
                assert!(!cell.polygon.is_empty(), "cell {i} vanished ({align:?})");
                for (j, other) in cells.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    for v in &cell.polygon {
                        assert!(
                            !point_in_loops(from_ref(&other.polygon), *v),
                            "cell {i} vertex {v:?} inside cell {j} ({align:?})"
                        );
                    }
                }
            }
        }
    }

    /// Clipping can shrink a cell but never push it off its own center:
    /// every bisector half-plane contains the center by construction.
    #[test]
    fn every_cell_contains_its_aligned_center() {
        let strands = line_lamps(6, 4.0);
        for align in [PathAlign::On, PathAlign::Inside, PathAlign::Outside] {
            let seeds = cell_seeds(&strands, align, 2.0);
            let cells = lamp_cells(&strands, align, 2.0);
            for (cell, seed) in cells.iter().zip(&seeds) {
                let center = [seed.center[0] as f32, seed.center[1] as f32];
                assert!(
                    point_in_loops(from_ref(&cell.polygon), center),
                    "cell {} lost its center ({align:?})",
                    cell.lamp
                );
            }
        }
    }

    /// Aligned cells shift their center a full radius off the path — the
    /// cell edge kisses the path like an aligned stroke. A straight
    /// strand's centroid tie deterministically picks left-of-travel
    /// (-y here), and Outside mirrors it.
    #[test]
    fn aligned_cells_kiss_the_path_from_their_side() {
        let strands = line_lamps(6, 4.0);
        let inside = cell_seeds(&strands, PathAlign::Inside, 2.0);
        let outside = cell_seeds(&strands, PathAlign::Outside, 2.0);
        for (index, (seed_in, seed_out)) in inside.iter().zip(&outside).enumerate() {
            let lamp = [index as f64 * 4.0, 0.0];
            assert!(
                (dist(seed_in.center, lamp) - seed_in.radius).abs() < 1e-6,
                "center shifted by exactly its radius"
            );
            assert!(seed_in.center[1] < 0.0, "inside tie picks left of travel");
            assert!(seed_out.center[1] > 0.0, "outside is the mirror");
        }
        let cells = lamp_cells(&strands, PathAlign::Inside, 2.0);
        for (index, (cell, seed)) in cells.iter().zip(&inside).enumerate() {
            let lamp = [index as f32 * 4.0, 0.0];
            let d = f64::from(dist_to_loops(from_ref(&cell.polygon), lamp));
            assert!(
                d <= 0.2 * seed.radius,
                "cell {index} floats {d} off the path (radius {})",
                seed.radius
            );
        }
    }

    /// On a uniform line the two seeding modes agree: the median
    /// nearest-neighbour distance IS the strand pitch, alignment is On,
    /// so `point_cells` reproduces `lamp_cells` exactly.
    #[test]
    fn point_cells_match_lamp_cells_on_a_uniform_line() {
        let strands = line_lamps(6, 4.0);
        let positions: Vec<[f32; 2]> = strands[0].clone();
        let from_points = point_cells(&positions, 1.0);
        let from_strands = lamp_cells(&strands, PathAlign::On, 2.0);
        assert_eq!(from_points.len(), from_strands.len());
        for (a, b) in from_points.iter().zip(&from_strands) {
            assert_eq!(a.lamp, b.lamp);
            assert_eq!(a.polygon.len(), b.polygon.len(), "cell {}", a.lamp);
            for (va, vb) in a.polygon.iter().zip(&b.polygon) {
                assert!(
                    (va[0] - vb[0]).abs() < 1e-4 && (va[1] - vb[1]).abs() < 1e-4,
                    "cell {}: {va:?} vs {vb:?}",
                    a.lamp
                );
            }
        }
    }

    /// A lone point (and an all-coincident field) has no neighbour
    /// distance to measure: the floor is the whole radius, and coincident
    /// seeds share territory rather than clipping each other away.
    #[test]
    fn point_cells_fall_back_to_the_floor_footprint() {
        let lone = point_cells(&[[5.0, 5.0]], 4.0);
        assert_eq!(lone.len(), 1);
        assert_eq!(lone[0].polygon.len(), CELL_SIDES);
        for v in &lone[0].polygon {
            let d = dist([f64::from(v[0]), f64::from(v[1])], [5.0, 5.0]);
            assert!((d - 4.0 * INSET_KEEP).abs() < 1e-3, "{v:?} at {d}");
        }
        let coincident = point_cells(&[[5.0, 5.0], [5.0, 5.0]], 4.0);
        assert_eq!(coincident.len(), 2);
        for cell in &coincident {
            assert_eq!(cell.polygon.len(), CELL_SIDES, "shared territory");
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
            .polygon
            .iter()
            .map(|v| dist([f64::from(v[0]), f64::from(v[1])], [50.0, 0.0]))
            .fold(0.0, f64::max);
        assert!(
            (max_reach - 13.0 * INSET_KEEP).abs() < 1e-3,
            "floor radius should shape the cell, got reach {max_reach}"
        );
        assert!(!cells.is_empty());
        for cell in [&cells, &tight_cells].into_iter().flatten() {
            assert!(!cell.polygon.is_empty(), "cell {} vanished", cell.lamp);
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
        for (i, cell) in cells.iter().enumerate() {
            assert!(!cell.polygon.is_empty(), "cell {i} vanished");
            assert!(
                point_in_loops(from_ref(&cell.polygon), positions[i]),
                "cell {i} lost its seed"
            );
            for (j, other) in cells.iter().enumerate() {
                if i == j {
                    continue;
                }
                for v in &cell.polygon {
                    assert!(
                        !point_in_loops(from_ref(&other.polygon), *v),
                        "cell {i} vertex {v:?} inside cell {j}"
                    );
                }
            }
        }
    }

    /// The preview perf contract: a dome-scale point field (1500 lamps)
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
        for (i, cell) in cells.iter().enumerate() {
            assert!(!cell.polygon.is_empty(), "cell {i} vanished");
        }
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "grid failed to keep the build sub-frame: {elapsed:?}"
        );
    }

    /// The perf contract: outline + cells for a dome-scale strand stay
    /// far under a frame even in debug — catches accidental quadratic
    /// blowups, nothing subtler.
    #[test]
    fn a_sixty_lamp_strand_computes_outline_and_cells_quickly() {
        let strand: Vec<[f32; 2]> = (0..60)
            .map(|i| {
                let t = i as f32 * 0.5;
                [t * 4.0, t.sin() * 10.0]
            })
            .collect();
        let strands = vec![strand];
        let start = std::time::Instant::now();
        for align in [PathAlign::On, PathAlign::Inside, PathAlign::Outside] {
            let loops = aligned_outline(&strands, align, 3.0);
            assert!(!loops.is_empty());
            let cells = lamp_cells(&strands, align, 2.0);
            assert_eq!(cells.len(), 60);
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "accidental quadratic blowup? {elapsed:?}"
        );
    }
}
