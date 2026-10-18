//! A fixture's live view: each lamp as a cell of light, the way Studio's
//! live view draws them (`point_cells` in `lpa-mapping-editor`, built again
//! here on the lab's rasterizer: the lab depends on no product crate).
//!
//! Every lamp claims the ground nearer to it than to any other lamp, out to
//! a radius of 0.92 × the median distance between nearest neighbours, and
//! each cell is drawn shrunk 10 % toward its lamp so neighbours keep a
//! seam. Cells can't overlap, and a lamp's cell never vanishes (coincident
//! lamps share one). The cells are worked out once per layout and zoom
//! ([`CellMap`]); a new frame of colours only fills them.

use crate::dense_canvas::Raster;

/// A cell's radius: this fraction of the median nearest-neighbour distance,
/// just under half the pitch, so neighbouring cells meet at the line
/// between their lamps rather than at their rims.
const NN_FRACTION: f64 = 0.92;
/// How much of a cell is kept after the seam: shrunk toward its lamp.
const INSET_KEEP: f64 = 0.9;
/// No lamp's cell covers this pixel.
const NONE: u32 = u32::MAX;

/// Which lamp's cell covers each device pixel of a face's raster.
pub struct CellMap {
    owner: Vec<u32>,
}

impl CellMap {
    /// The cells of `seeds` (lamps, in device pixels of a `w` × `h`
    /// raster), never smaller than `min_r` device pixels.
    pub fn new(w: u32, h: u32, seeds: &[(f64, f64)], min_r: f64) -> CellMap {
        let mut owner = vec![NONE; (w * h) as usize];
        if seeds.is_empty() {
            return CellMap { owner };
        }
        let r = cell_radius(seeds, w, h).max(min_r).max(1.0);
        let grid = Grid::new(seeds, w, h, 2.0 * r);
        let reach = r * INSET_KEEP;
        let mut near: Vec<usize> = Vec::new();
        for (i, &(cx, cy)) in seeds.iter().enumerate() {
            // A lamp more than two radii away can't reach this cell.
            near.clear();
            grid.around(cx, cy, 2.0 * r, |j| {
                if j != i {
                    near.push(j);
                }
            });
            let x0 = (cx - reach).floor().max(0.0) as u32;
            let y0 = (cy - reach).floor().max(0.0) as u32;
            let x1 = ((cx + reach).ceil().max(0.0) as u32).min(w);
            let y1 = ((cy + reach).ceil().max(0.0) as u32).min(h);
            for py in y0..y1 {
                for px in x0..x1 {
                    let (dx, dy) = (px as f64 + 0.5 - cx, py as f64 + 0.5 - cy);
                    if dx * dx + dy * dy > reach * reach {
                        continue;
                    }
                    // Where this pixel sits in the cell before the seam
                    // shrank it: it is the lamp's if that point is nearer
                    // to it than to any neighbour.
                    let (qx, qy) = (cx + dx / INSET_KEEP, cy + dy / INSET_KEEP);
                    let own = (qx - cx).powi(2) + (qy - cy).powi(2);
                    let mine = near.iter().all(|&j| {
                        let (sx, sy) = seeds[j];
                        (qx - sx).powi(2) + (qy - sy).powi(2) >= own
                    });
                    if mine {
                        owner[(py * w + px) as usize] = i as u32;
                    }
                }
            }
        }
        CellMap { owner }
    }

    /// Fill each cell with its lamp's colour (`colours` in seed order).
    pub fn paint(&self, raster: &mut Raster, colours: &[[f64; 4]]) {
        raster.fill_owned(&self.owner, colours);
    }
}

/// [`NN_FRACTION`] × the median distance from each lamp to its nearest
/// neighbour, in device pixels. A lone lamp gets a quarter of the face.
fn cell_radius(seeds: &[(f64, f64)], w: u32, h: u32) -> f64 {
    if seeds.len() < 2 {
        return w.min(h) as f64 / 4.0;
    }
    // Buckets about one lamp each, so the nearest is a ring or two away.
    let side = ((w as f64 * h as f64) / seeds.len() as f64).sqrt().max(1.0);
    let grid = Grid::new(seeds, w, h, side);
    let mut nearest: Vec<f64> = seeds
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| grid.nearest(seeds, i, x, y))
        .filter(|d| d.is_finite())
        .collect();
    if nearest.is_empty() {
        return w.min(h) as f64 / 4.0;
    }
    let mid = nearest.len() / 2;
    nearest.select_nth_unstable_by(mid, f64::total_cmp);
    NN_FRACTION * nearest[mid]
}

/// Lamps bucketed in squares `side` device pixels wide over the raster;
/// one outside it sits in the nearest edge bucket.
struct Grid {
    side: f64,
    cols: i64,
    rows: i64,
    buckets: Vec<Vec<u32>>,
}

impl Grid {
    fn new(seeds: &[(f64, f64)], w: u32, h: u32, side: f64) -> Grid {
        let side = side.max(1.0);
        let cols = ((w as f64 / side).ceil() as i64).max(1);
        let rows = ((h as f64 / side).ceil() as i64).max(1);
        let mut grid = Grid {
            side,
            cols,
            rows,
            buckets: vec![Vec::new(); (cols * rows) as usize],
        };
        for (i, &(x, y)) in seeds.iter().enumerate() {
            let (c, r) = grid.cell(x, y);
            grid.buckets[(r * cols + c) as usize].push(i as u32);
        }
        grid
    }

    fn cell(&self, x: f64, y: f64) -> (i64, i64) {
        let c = ((x / self.side).floor() as i64).clamp(0, self.cols - 1);
        let r = ((y / self.side).floor() as i64).clamp(0, self.rows - 1);
        (c, r)
    }

    /// Every lamp in a bucket within `reach` of `(x, y)`.
    fn around(&self, x: f64, y: f64, reach: f64, mut f: impl FnMut(usize)) {
        let (c0, r0) = self.cell(x - reach, y - reach);
        let (c1, r1) = self.cell(x + reach, y + reach);
        for r in r0..=r1 {
            for c in c0..=c1 {
                for &j in &self.buckets[(r * self.cols + c) as usize] {
                    f(j as usize);
                }
            }
        }
    }

    /// How far lamp `i`, at `(x, y)`, is from its nearest neighbour,
    /// searching rings of buckets outward until no nearer one can be left.
    fn nearest(&self, seeds: &[(f64, f64)], i: usize, x: f64, y: f64) -> f64 {
        let (c, r) = self.cell(x, y);
        let mut best = f64::INFINITY;
        let most = self.cols.max(self.rows);
        for ring in 0..=most {
            for rr in (r - ring)..=(r + ring) {
                for cc in (c - ring)..=(c + ring) {
                    let edge = (rr - r).abs() == ring || (cc - c).abs() == ring;
                    if !edge || rr < 0 || cc < 0 || rr >= self.rows || cc >= self.cols {
                        continue;
                    }
                    for &j in &self.buckets[(rr * self.cols + cc) as usize] {
                        if j as usize != i {
                            let (sx, sy) = seeds[j as usize];
                            best = best.min(((sx - x).powi(2) + (sy - y).powi(2)).sqrt());
                        }
                    }
                }
            }
            // Everything nearer than `ring` buckets has been seen.
            if best <= ring as f64 * self.side {
                break;
            }
        }
        best
    }
}
