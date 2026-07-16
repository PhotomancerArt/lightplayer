//! Merged-outline geometry: union of axis-aligned rects → one rounded SVG path.
//!
//! This is the core of the "contiguous popup" chrome: the popover trigger and
//! panel are plain DOM elements with no border or background of their own; one
//! SVG path — the union of their rects, every corner rounded — draws the
//! shared fill, border, and shadow. Convex and concave corners use the same
//! arc construction (the sweep flag flips with the turn direction), so the
//! concave fillets where trigger meets panel are not a special case.
//!
//! Pure geometry: no DOM, unit-tested on the host. Ported from the spike at
//! `spikes/contiguous-popup/index.html`; technique write-up:
//! <https://lab.photomancer.art/post/2026-07-15-contiguous-popup/>

use std::collections::HashMap;

/// Degenerate-rect / collinearity epsilon.
const EPS: f64 = 1e-4;

/// Coordinates closer than this are welded onto one grid line. Deliberately
/// generous: edges that ALMOST line up (sub-1.5px steps from layout rounding)
/// would otherwise render as hairline jogs in the outline.
const COORD_TOL: f64 = 1.25;

/// One participating rectangle, in viewport CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutlineRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl OutlineRect {
    /// Grow (positive `by`) or shrink the rect on all sides.
    pub fn inflate(self, by: f64) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            w: self.w + 2.0 * by,
            h: self.h + 2.0 * by,
        }
    }
}

/// Union of `rects` → one SVG path string (`fill-rule: evenodd` on the
/// consumer), every corner rounded with `radius` (clamped per vertex so short
/// segments shrink their corners instead of self-intersecting), coordinates
/// snapped to device pixels for `dpr` so 1px strokes stay crisp.
///
/// Returns an empty string when no non-degenerate rect is given.
pub fn merged_outline_path(rects: &[OutlineRect], radius: f64, dpr: f64) -> String {
    let snapped: Vec<OutlineRect> = rects.iter().map(|r| snap_rect(*r, dpr)).collect();
    rounded_path(&union_loops(&snapped), radius)
}

fn snap(v: f64, dpr: f64) -> f64 {
    let d = if dpr > EPS { dpr } else { 1.0 };
    let snapped = (v * d).round() / d;
    // At dpr 1 an integer-aligned 1px stroke straddles two pixels; center it.
    if (d - 1.0).abs() < EPS {
        snapped + 0.5
    } else {
        snapped
    }
}

fn snap_rect(r: OutlineRect, dpr: f64) -> OutlineRect {
    let x = snap(r.x, dpr);
    let y = snap(r.y, dpr);
    OutlineRect {
        x,
        y,
        w: snap(r.x + r.w, dpr) - x,
        h: snap(r.y + r.h, dpr) - y,
    }
}

/// Sorted values collapsed so entries within `tol` weld onto the first of
/// their cluster.
fn dedupe_sorted(sorted: &[f64], tol: f64) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::with_capacity(sorted.len());
    for &v in sorted {
        match out.last() {
            Some(&last) if v - last <= tol => {}
            _ => out.push(v),
        }
    }
    out
}

/// Union of axis-aligned rects as closed rectilinear loops.
///
/// Grid method: distinct edge coordinates form a small grid; cells covered by
/// any rect are marked; boundary edges (where coverage flips) are chained into
/// loops; collinear midpoints are dropped. For the 2–3 rects of a popover the
/// grid is a handful of cells.
pub(crate) fn union_loops(rects: &[OutlineRect]) -> Vec<Vec<(f64, f64)>> {
    let rects: Vec<OutlineRect> = rects
        .iter()
        .copied()
        .filter(|r| r.w > EPS && r.h > EPS)
        .collect();
    if rects.is_empty() {
        return Vec::new();
    }

    let mut xs: Vec<f64> = rects.iter().flat_map(|r| [r.x, r.x + r.w]).collect();
    let mut ys: Vec<f64> = rects.iter().flat_map(|r| [r.y, r.y + r.h]).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let xs = dedupe_sorted(&xs, COORD_TOL);
    let ys = dedupe_sorted(&ys, COORD_TOL);
    let nx = xs.len() - 1;
    let ny = ys.len() - 1;

    let mut cov = vec![vec![false; ny]; nx];
    for (i, col) in cov.iter_mut().enumerate() {
        let cx = (xs[i] + xs[i + 1]) / 2.0;
        for (j, cell) in col.iter_mut().enumerate() {
            let cy = (ys[j] + ys[j + 1]) / 2.0;
            *cell = rects
                .iter()
                .any(|r| cx > r.x && cx < r.x + r.w && cy > r.y && cy < r.y + r.h);
        }
    }
    let covered = |i: isize, j: isize| -> bool {
        if i < 0 || j < 0 || i >= nx as isize || j >= ny as isize {
            false
        } else {
            cov[i as usize][j as usize]
        }
    };

    // Boundary segments: grid edges where coverage flips.
    let mut segs: Vec<[(f64, f64); 2]> = Vec::new();
    for i in 0..=nx {
        for j in 0..ny {
            if covered(i as isize - 1, j as isize) != covered(i as isize, j as isize) {
                segs.push([(xs[i], ys[j]), (xs[i], ys[j + 1])]);
            }
        }
    }
    for j in 0..=ny {
        for i in 0..nx {
            if covered(i as isize, j as isize - 1) != covered(i as isize, j as isize) {
                segs.push([(xs[i], ys[j]), (xs[i + 1], ys[j])]);
            }
        }
    }

    // Chain segments into closed loops.
    let key = |p: (f64, f64)| -> (i64, i64) {
        ((p.0 * 100.0).round() as i64, (p.1 * 100.0).round() as i64)
    };
    let mut adj: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (idx, seg) in segs.iter().enumerate() {
        adj.entry(key(seg[0])).or_default().push(idx);
        adj.entry(key(seg[1])).or_default().push(idx);
    }

    let mut used = vec![false; segs.len()];
    let mut loops: Vec<Vec<(f64, f64)>> = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut lp: Vec<(f64, f64)> = vec![segs[start][0], segs[start][1]];
        loop {
            let cur = lp[lp.len() - 1];
            let prev = lp[lp.len() - 2];
            let k = key(cur);
            let cands: Vec<usize> = adj
                .get(&k)
                .map(|v| v.iter().copied().filter(|&i| !used[i]).collect())
                .unwrap_or_default();
            let Some(&first) = cands.first() else {
                break;
            };
            // At degree-4 junctions (two rects touching corner to corner)
            // prefer a turning continuation over crossing straight through.
            let pick = if cands.len() > 1 {
                let din = (cur.0 - prev.0, cur.1 - prev.1);
                cands
                    .iter()
                    .copied()
                    .find(|&i| {
                        let s = segs[i];
                        let other = if key(s[0]) == k { s[1] } else { s[0] };
                        let dout = (other.0 - cur.0, other.1 - cur.1);
                        (din.0 * dout.1 - din.1 * dout.0).abs() > EPS
                    })
                    .unwrap_or(first)
            } else {
                first
            };
            used[pick] = true;
            let s = segs[pick];
            let next = if key(s[0]) == k { s[1] } else { s[0] };
            if key(next) == key(lp[0]) {
                break;
            }
            lp.push(next);
        }

        // Drop collinear midpoints (merges runs of grid edges).
        let n = lp.len();
        let simplified: Vec<(f64, f64)> = (0..n)
            .filter_map(|i| {
                let a = lp[(i + n - 1) % n];
                let b = lp[i];
                let c = lp[(i + 1) % n];
                let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
                (cross.abs() > EPS).then_some(b)
            })
            .collect();
        if simplified.len() >= 3 {
            loops.push(simplified);
        }
    }
    loops
}

/// Loops → SVG path string with rounded corners.
///
/// At every vertex both adjacent segments are trimmed by the (clamped) radius
/// and joined with an arc whose sweep follows the turn direction — in screen
/// coordinates (y down), `cross > 0` is a clockwise turn, sweep flag 1.
pub(crate) fn rounded_path(loops: &[Vec<(f64, f64)>], radius: f64) -> String {
    let fmt = |v: f64| -> String {
        let r = (v * 100.0).round() / 100.0;
        if (r - r.trunc()).abs() < 1e-9 {
            format!("{}", r.trunc() as i64)
        } else {
            format!("{r}")
        }
    };

    let mut d = String::new();
    for pts in loops {
        let n = pts.len();
        let clamped: Vec<f64> = (0..n)
            .map(|i| {
                let p = pts[i];
                let a = pts[(i + n - 1) % n];
                let c = pts[(i + 1) % n];
                let lp = (p.0 - a.0).hypot(p.1 - a.1);
                let ln = (c.0 - p.0).hypot(c.1 - p.1);
                radius.min(lp / 2.0).min(ln / 2.0).max(0.0)
            })
            .collect();
        for i in 0..n {
            let p = pts[i];
            let a = pts[(i + n - 1) % n];
            let c = pts[(i + 1) % n];
            let lin = (p.0 - a.0).hypot(p.1 - a.1);
            let lout = (c.0 - p.0).hypot(c.1 - p.1);
            if lin < EPS || lout < EPS {
                continue;
            }
            let din = ((p.0 - a.0) / lin, (p.1 - a.1) / lin);
            let dout = ((c.0 - p.0) / lout, (c.1 - p.1) / lout);
            let r = clamped[i];
            let p1 = (p.0 - din.0 * r, p.1 - din.1 * r);
            let p2 = (p.0 + dout.0 * r, p.1 + dout.1 * r);
            let sweep = if din.0 * dout.1 - din.1 * dout.0 > 0.0 {
                1
            } else {
                0
            };
            let cmd = if i == 0 { 'M' } else { 'L' };
            d.push_str(&format!("{cmd}{} {}", fmt(p1.0), fmt(p1.1)));
            if r > 0.05 {
                d.push_str(&format!(
                    "A{r} {r} 0 0 {sweep} {} {}",
                    fmt(p2.0),
                    fmt(p2.1),
                    r = fmt(r)
                ));
            }
        }
        d.push('Z');
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> OutlineRect {
        OutlineRect { x, y, w, h }
    }

    /// Sweep flags of every arc in a path string, in order.
    fn sweeps(d: &str) -> Vec<u8> {
        d.split('A')
            .skip(1)
            .map(|arc| {
                // "rx ry 0 0 sweep x y…" — sweep is the 5th field.
                let field = arc.split_whitespace().nth(4).expect("arc sweep field");
                field.parse::<u8>().expect("sweep flag parses")
            })
            .collect()
    }

    #[test]
    fn single_rect_is_one_loop_of_four_corners() {
        let loops = union_loops(&[rect(0.0, 0.0, 100.0, 50.0)]);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 4);

        let d = merged_outline_path(&[rect(0.0, 0.0, 100.0, 50.0)], 8.0, 2.0);
        let s = sweeps(&d);
        assert_eq!(s.len(), 4);
        assert!(
            s.iter().all(|&f| f == s[0]),
            "uniform turn direction on a plain rect: {d}"
        );
    }

    #[test]
    fn trigger_plus_wider_panel_has_two_concave_corners() {
        // Trigger 40..90 x 10..40; panel overlaps its bottom edge by 1px and
        // is wider on both sides — the classic contiguous-popup shape.
        let trigger = rect(40.0, 10.0, 50.0, 30.0);
        let panel = rect(10.0, 39.0, 200.0, 80.0);
        let loops = union_loops(&[trigger, panel]);
        assert_eq!(loops.len(), 1, "overlapping rects merge into one loop");
        assert_eq!(
            loops[0].len(),
            8,
            "4 convex panel + 2 convex trigger + 2 concave"
        );

        let d = merged_outline_path(&[trigger, panel], 8.0, 2.0);
        let s = sweeps(&d);
        assert_eq!(s.len(), 8);
        let ones = s.iter().filter(|&&f| f == 1).count();
        let minority = ones.min(s.len() - ones);
        assert_eq!(minority, 2, "exactly two concave fillets: {d}");
    }

    #[test]
    fn sub_tolerance_step_welds_away() {
        // Two rects whose bottom edges differ by 1px (< COORD_TOL): the step
        // must weld, giving the same vertex count as perfect alignment.
        let aligned = union_loops(&[rect(0.0, 0.0, 50.0, 40.0), rect(50.0, 0.0, 50.0, 40.0)]);
        let stepped = union_loops(&[rect(0.0, 0.0, 50.0, 40.0), rect(50.0, 0.0, 50.0, 41.0)]);
        assert_eq!(aligned.len(), 1);
        assert_eq!(stepped.len(), 1);
        assert_eq!(
            stepped[0].len(),
            aligned[0].len(),
            "1px step should weld onto one grid line"
        );
        assert_eq!(aligned[0].len(), 4);
    }

    #[test]
    fn radius_clamps_on_short_segments() {
        // 10px-tall rect with radius 8: vertical segments allow at most r=5.
        let d = rounded_path(&union_loops(&[rect(0.0, 0.0, 100.0, 10.0)]), 8.0);
        assert!(d.contains("A5 5 0"), "clamped to half the short edge: {d}");
        assert!(
            !d.contains("A8 8 0"),
            "unclamped radius must not appear: {d}"
        );
    }

    #[test]
    fn disjoint_rects_are_separate_subpaths() {
        let d = merged_outline_path(
            &[rect(0.0, 0.0, 40.0, 40.0), rect(100.0, 100.0, 40.0, 40.0)],
            8.0,
            2.0,
        );
        assert_eq!(d.matches('M').count(), 2);
        assert_eq!(d.matches('Z').count(), 2);
    }

    #[test]
    fn degenerate_input_is_ignored() {
        assert_eq!(merged_outline_path(&[], 8.0, 2.0), "");
        assert_eq!(
            merged_outline_path(&[rect(10.0, 10.0, 0.0, 50.0)], 8.0, 2.0),
            ""
        );
        // A degenerate rect alongside a real one changes nothing.
        let alone = merged_outline_path(&[rect(0.0, 0.0, 50.0, 50.0)], 8.0, 2.0);
        let with_degenerate = merged_outline_path(
            &[rect(0.0, 0.0, 50.0, 50.0), rect(10.0, 10.0, 0.0, 0.0)],
            8.0,
            2.0,
        );
        assert_eq!(alone, with_degenerate);
    }

    #[test]
    fn snapping_lands_on_device_pixels() {
        // dpr 2: coordinates snap to halves.
        let r2 = snap_rect(rect(10.3, 20.6, 100.2, 50.4), 2.0);
        for v in [r2.x, r2.y, r2.x + r2.w, r2.y + r2.h] {
            assert!(
                ((v * 2.0) - (v * 2.0).round()).abs() < 1e-9,
                "{v} not on a half-pixel"
            );
        }
        // dpr 1: edges sit on x.5 so a 1px stroke fills one pixel row.
        let r1 = snap_rect(rect(10.3, 20.6, 100.2, 50.4), 1.0);
        for v in [r1.x, r1.y, r1.x + r1.w, r1.y + r1.h] {
            assert!(((v - v.trunc()) - 0.5).abs() < 1e-9, "{v} not on x.5");
        }
    }

    #[test]
    fn inflate_grows_symmetrically() {
        let r = rect(10.0, 20.0, 30.0, 40.0).inflate(3.0);
        assert_eq!(r, rect(7.0, 17.0, 36.0, 46.0));
    }
}
