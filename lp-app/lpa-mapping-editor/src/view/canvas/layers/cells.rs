//! Voronoi LAMP CELLS: each displayed lamp claims a small polygon of
//! territory, so a selected path object reads as a mosaic of lamps rather
//! than a row of disconnected dots.
//!
//! The spike proved the LOOK but built it from a distance field, whose
//! floating-island cells and unfilled gaps are rejected defect classes.
//! Here a cell is geometric by construction: a 12-gon of the lamp's LOCAL
//! radius (nearest-neighbour scaled, clamped), Sutherland–Hodgman clipped
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
//! Neighbour searches are brute force per object: after subsampling an
//! object displays at most a few hundred lamps (dome scale is ~150
//! objects × ≤60 displayed lamps), so O(n·k) clipping plus an O(n²)
//! nearest-neighbour pass with a tiny constant beats any spatial index it
//! could build. Coincident lamps have no bisector and simply share
//! territory.

use lpc_mapping::PathAlign;

use super::outline::{lamp_normal, object_centroid, strand_inside_sign};

/// Local radius = `NN_FRACTION` × the lamp's nearest-neighbour distance:
/// just under half-pitch-per-side so neighbouring 12-gons overlap and the
/// bisector clip, not the disc edge, draws the shared wall.
const NN_FRACTION: f64 = 0.92;

/// Radius clamp in own-space units. The spike ruled the look at ~3.2/13
/// CSS px; own space is px-scale for typical docs (the outline pad
/// heuristic clamps at 0.75..14 in the same units), so these carry the
/// same intent — P3/P4's visual gate owns the final tune.
const RADIUS_MIN: f64 = 1.0;
const RADIUS_MAX: f64 = 13.0;

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

fn cell_seeds(strands: &[Vec<[f32; 2]>], align: PathAlign) -> Vec<Seed> {
    let positions: Vec<[f64; 2]> = strands
        .iter()
        .flatten()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    let nearest = |index: usize| {
        positions
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != index)
            .map(|(_, q)| dist(positions[index], *q))
            .fold(f64::INFINITY, f64::min)
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
        for lamp in 0..strand.len() {
            let position = positions[index];
            // A lone lamp's nearest neighbour is at infinity; the clamp
            // turns that into the full RADIUS_MAX disc.
            let radius = (NN_FRACTION * nearest(index)).clamp(RADIUS_MIN, RADIUS_MAX);
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
pub fn lamp_cells(strands: &[Vec<[f32; 2]>], align: PathAlign) -> Vec<LampCell> {
    let seeds = cell_seeds(strands, align);
    let tau = std::f64::consts::TAU;
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
            for (other_index, other) in seeds.iter().enumerate() {
                if other_index == index {
                    continue;
                }
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
        let cells = lamp_cells(&strands, PathAlign::On);
        assert_eq!(cells.len(), 12);
        for (index, cell) in cells.iter().enumerate() {
            assert_eq!(cell.lamp, index);
        }
    }

    /// A lone lamp has no neighbours to yield to: a full 12-gon at the
    /// clamp ceiling.
    #[test]
    fn a_lone_lamp_gets_a_full_twelve_gon() {
        let cells = lamp_cells(&[vec![[5.0_f32, 5.0]]], PathAlign::On);
        assert_eq!(cells.len(), 1);
        let polygon = &cells[0].polygon;
        assert_eq!(polygon.len(), CELL_SIDES);
        for v in polygon {
            let d = dist([f64::from(v[0]), f64::from(v[1])], [5.0, 5.0]);
            assert!((d - RADIUS_MAX * INSET_KEEP).abs() < 1e-3, "{v:?} at {d}");
        }
    }

    /// After bisector clipping (and the seam inset) no cell's vertex sits
    /// inside another cell — the tiling never overlaps, whatever the
    /// alignment.
    #[test]
    fn cells_do_not_overlap_after_clipping() {
        let strands = line_lamps(10, 4.0);
        for align in [PathAlign::On, PathAlign::Inside, PathAlign::Outside] {
            let cells = lamp_cells(&strands, align);
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
            let seeds = cell_seeds(&strands, align);
            let cells = lamp_cells(&strands, align);
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
        let inside = cell_seeds(&strands, PathAlign::Inside);
        let outside = cell_seeds(&strands, PathAlign::Outside);
        for (index, (seed_in, seed_out)) in inside.iter().zip(&outside).enumerate() {
            let lamp = [index as f64 * 4.0, 0.0];
            assert!(
                (dist(seed_in.center, lamp) - seed_in.radius).abs() < 1e-6,
                "center shifted by exactly its radius"
            );
            assert!(seed_in.center[1] < 0.0, "inside tie picks left of travel");
            assert!(seed_out.center[1] > 0.0, "outside is the mirror");
        }
        let cells = lamp_cells(&strands, PathAlign::Inside);
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
            let cells = lamp_cells(&strands, align);
            assert_eq!(cells.len(), 60);
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "accidental quadratic blowup? {elapsed:?}"
        );
    }
}
