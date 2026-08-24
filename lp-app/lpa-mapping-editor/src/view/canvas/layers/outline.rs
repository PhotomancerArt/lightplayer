//! Aligned object OUTLINES: the band a lamp strand sweeps, straddling the
//! path or pushed wholly to one side — the design-language round's body
//! for path objects.
//!
//! The round rejected two earlier bodies. Convex hulls (hull.rs) swallow
//! every concavity — a C-shaped strand becomes a blob whose whole mouth is
//! clickable dead air. The spike's distance-field outlines carried their
//! own defect classes (floating-island cells, unfilled gaps, apex fling).
//! This module is the ruled replacement: GEOMETRIC offsetting — offset
//! each strand polyline, insert round joins on turns that open toward the
//! offset side and clamped miters on turns that close on it — so the band
//! follows the strand at a fixed reach by construction.
//!
//! Alignment speaks Illustrator stroke language (`PathAlign`): `On`
//! straddles the path symmetrically with round caps; `Inside`/`Outside`
//! put the whole band on one side so the lamp path ITSELF is one edge of
//! the body (the channel-letter ruling), with flat caps at the path edge.
//! "Inside" is deterministic: the winding interior for closed strands, the
//! side nearer the pooled lamp centroid (probed at the arc-length
//! midpoint) for open ones; an exact tie picks left-of-travel.
//!
//! Loops come out with positive shoelace winding in y-down space — except
//! the hole loop of a closed strand's band, which is negative — and are
//! meant for fill-rule "nonzero", which merges overlapping strand loops
//! with no boolean union. Nonzero also hides the one known limitation:
//! where the band is wider than the local feature size (tight hairpins,
//! rings narrower than the band) offset edges self-intersect, and the
//! extra winding is invisible. [`point_in_loops`] applies the same nonzero
//! rule so hit answers match what gets painted.
//!
//! Pure math on purpose — no Dioxus, no sprites (hull.rs's contract).
//! Recomputed on every sprite-memo rebuild at dome scale (~150 objects ×
//! ≤60 displayed lamps), so everything here is O(vertices).

use lpc_mapping::PathAlign;

/// Round joins and caps subdivide their arcs so no step sweeps more than
/// 30° — coarse enough to stay cheap, fine enough to read as round.
const MAX_ARC_STEP: f64 = std::f64::consts::PI / 6.0;

/// Miter clamp in multiples of the offset distance: a needle apex may not
/// fling a vertex past 2.5× the band's reach. (`pad_hull`'s 4× fling is
/// the shipped defect this replaces.)
const MITER_LIMIT: f64 = 2.5;

/// Points closer than this (own-space units) coincide: consecutive
/// duplicates collapse, and a strand whose first and last lamp coincide is
/// a closed ring.
const CLOSE_EPS: f64 = 1e-4;

/// Unit-direction cross products below this count as collinear.
const TURN_EPS: f64 = 1e-7;

type P = [f64; 2];

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul(a: P, k: f64) -> P {
    [a[0] * k, a[1] * k]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn norm(a: P) -> f64 {
    dot(a, a).sqrt()
}

/// Left-of-travel normal in y-down space (travel right on screen ⇒ up).
fn left_normal(dir: P) -> P {
    [dir[1], -dir[0]]
}

fn to_f64(p: [f32; 2]) -> P {
    [f64::from(p[0]), f64::from(p[1])]
}

fn to_f32_loop(polygon: Vec<P>) -> Vec<[f32; 2]> {
    polygon
        .into_iter()
        .map(|p| [p[0] as f32, p[1] as f32])
        .collect()
}

/// The strand with consecutive duplicates dropped — a lamp repeated in
/// place must not produce a zero-length segment with no direction.
fn distinct_points(strand: &[[f32; 2]]) -> Vec<P> {
    let mut out: Vec<P> = Vec::with_capacity(strand.len());
    for point in strand {
        let point = to_f64(*point);
        if out
            .last()
            .is_none_or(|prev| norm(sub(point, *prev)) > CLOSE_EPS)
        {
            out.push(point);
        }
    }
    out
}

/// A strand whose ends meet is a ring; the cyclic point list drops the
/// duplicated closing point. Fewer than three distinct points cannot ring
/// (an A→B→A strand is an open hairpin, which the joins handle).
fn closed_ring(pts: &[P]) -> Option<&[P]> {
    if pts.len() >= 4 && norm(sub(pts[0], pts[pts.len() - 1])) <= CLOSE_EPS {
        Some(&pts[..pts.len() - 1])
    } else {
        None
    }
}

/// Unit directions per segment; `closed` adds the wrap-around segment.
/// Callers guarantee distinct consecutive points (see [`distinct_points`]).
fn segment_dirs(pts: &[P], closed: bool) -> Vec<P> {
    let n = pts.len();
    let count = if closed { n } else { n - 1 };
    (0..count)
        .map(|i| {
            let d = sub(pts[(i + 1) % n], pts[i]);
            mul(d, 1.0 / norm(d))
        })
        .collect()
}

/// Twice the signed area; only the sign and relative magnitude matter.
fn shoelace(polygon: &[P]) -> f64 {
    let n = polygon.len();
    let mut sum = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        sum += cross(polygon[i], polygon[j]);
    }
    sum
}

/// Enforce the emitted winding contract: positive shoelace for body
/// loops, negative for hole loops.
fn oriented(mut polygon: Vec<P>, positive: bool) -> Vec<P> {
    if (shoelace(&polygon) < 0.0) == positive {
        polygon.reverse();
    }
    polygon
}

fn wrap_angle(angle: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut angle = angle % tau;
    if angle > std::f64::consts::PI {
        angle -= tau;
    } else if angle < -std::f64::consts::PI {
        angle += tau;
    }
    angle
}

/// Interior points of an arc (endpoints excluded — the caller already
/// owns them), stepping at most [`MAX_ARC_STEP`].
fn push_arc(out: &mut Vec<P>, center: P, radius: f64, start_angle: f64, sweep: f64) {
    let steps = (sweep.abs() / MAX_ARC_STEP).ceil().max(1.0) as usize;
    for k in 1..steps {
        let a = start_angle + sweep * (k as f64 / steps as f64);
        out.push([center[0] + radius * a.cos(), center[1] + radius * a.sin()]);
    }
}

/// The join at one strand vertex of an offset edge. `sign * left_normal`
/// is the offset side; a turn that OPENS on that side gaps the offset
/// endpoints apart and gets a round join, a turn that CLOSES on it makes
/// the offset edges cross and gets the clamped miter intersection.
fn push_join(out: &mut Vec<P>, vertex: P, dir_prev: P, dir_next: P, dist: f64, sign: f64) {
    let n_prev = left_normal(dir_prev);
    let n_next = left_normal(dir_next);
    let a = add(vertex, mul(n_prev, sign * dist));
    let b = add(vertex, mul(n_next, sign * dist));
    let turn = cross(dir_prev, dir_next);
    if turn.abs() <= TURN_EPS {
        if dot(dir_prev, dir_next) >= 0.0 {
            // Straight through: a ≈ b, one point carries the edge.
            out.push(a);
        } else {
            // An exact hairpin: no miter exists on either side — wrap the
            // tip with a semicircle through the spike's outgoing
            // direction (each side contributes half the tip circle).
            out.push(a);
            let start = (sign * n_prev[1]).atan2(sign * n_prev[0]);
            push_arc(out, vertex, dist, start, sign * std::f64::consts::PI);
            out.push(b);
        }
        return;
    }
    if turn * sign > 0.0 {
        // The turn opens on the offset side: round join around the vertex.
        out.push(a);
        let start = (sign * n_prev[1]).atan2(sign * n_prev[0]);
        let end = (sign * n_next[1]).atan2(sign * n_next[0]);
        push_arc(out, vertex, dist, start, wrap_angle(end - start));
        out.push(b);
    } else {
        // The turn closes on the offset side: intersect the two offset
        // edges. Clamped, because a needle apex sends the true
        // intersection toward infinity.
        let t = cross(sub(b, a), dir_next) / turn;
        let x = add(a, mul(dir_prev, t));
        let v = sub(x, vertex);
        let reach = norm(v);
        let clamp = MITER_LIMIT * dist;
        out.push(if reach > clamp {
            add(vertex, mul(v, clamp / reach))
        } else {
            x
        });
    }
}

/// One side of an open strand offset by `dist`, joins inserted.
fn offset_open(pts: &[P], dirs: &[P], dist: f64, sign: f64) -> Vec<P> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n * 2);
    out.push(add(pts[0], mul(left_normal(dirs[0]), sign * dist)));
    for j in 1..n - 1 {
        push_join(&mut out, pts[j], dirs[j - 1], dirs[j], dist, sign);
    }
    out.push(add(pts[n - 1], mul(left_normal(dirs[n - 2]), sign * dist)));
    out
}

/// Cyclic version: every vertex is a join and the loop closes itself.
fn offset_closed(pts: &[P], dirs: &[P], dist: f64, sign: f64) -> Vec<P> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n * 2);
    for j in 0..n {
        push_join(&mut out, pts[j], dirs[(j + n - 1) % n], dirs[j], dist, sign);
    }
    out
}

/// Mean of every lamp across every strand — the pooled reference that
/// "inside" leans toward for open strands.
pub(crate) fn object_centroid(strands: &[Vec<[f32; 2]>]) -> P {
    let mut sum = [0.0, 0.0];
    let mut count = 0.0;
    for strand in strands {
        for point in strand {
            sum = add(sum, to_f64(*point));
            count += 1.0;
        }
    }
    if count == 0.0 {
        sum
    } else {
        mul(sum, 1.0 / count)
    }
}

/// The centroid-side probe for an open strand: sample the point at half
/// the arc length and ask which side of its segment the pooled centroid
/// lies on. Ties (a straight strand through its own centroid) pick left,
/// deterministically.
fn open_inside_sign(pts: &[P], dirs: &[P], centroid: P) -> f64 {
    let total: f64 = (0..pts.len() - 1)
        .map(|i| norm(sub(pts[i + 1], pts[i])))
        .sum();
    let mut remaining = total / 2.0;
    for i in 0..pts.len() - 1 {
        let seg = norm(sub(pts[i + 1], pts[i]));
        if remaining <= seg || i == pts.len() - 2 {
            let mid = add(pts[i], mul(dirs[i], remaining.min(seg)));
            let side = dot(left_normal(dirs[i]), sub(centroid, mid));
            return if side < 0.0 { -1.0 } else { 1.0 };
        }
        remaining -= seg;
    }
    1.0
}

/// `sign` such that `sign * left_normal` points to the strand's INSIDE:
/// the winding interior for closed strands, the centroid side for open
/// ones. Degenerate strands answer +1 (left) so callers stay total.
pub(crate) fn strand_inside_sign(strand: &[[f32; 2]], centroid: P) -> f64 {
    let pts = distinct_points(strand);
    if let Some(ring) = closed_ring(&pts) {
        // Positive shoelace in y-down space puts the interior on the
        // RIGHT of travel, so inside = -left.
        return if shoelace(ring) > 0.0 { -1.0 } else { 1.0 };
    }
    if pts.len() < 2 {
        return 1.0;
    }
    let dirs = segment_dirs(&pts, false);
    open_inside_sign(&pts, &dirs, centroid)
}

/// The strand's left normal at one lamp (adjacent segment normals
/// averaged), or `None` when the strand has no direction there (a single
/// lamp). A hairpin lamp, where the average cancels, answers the outgoing
/// side — arbitrary but deterministic.
pub(crate) fn lamp_normal(strand: &[[f32; 2]], lamp: usize) -> Option<P> {
    let here = to_f64(*strand.get(lamp)?);
    let before = strand[..lamp]
        .iter()
        .rev()
        .map(|q| to_f64(*q))
        .find(|q| norm(sub(here, *q)) > CLOSE_EPS);
    let after = strand[lamp + 1..]
        .iter()
        .map(|q| to_f64(*q))
        .find(|q| norm(sub(*q, here)) > CLOSE_EPS);
    let mut sum = [0.0, 0.0];
    let mut last = None;
    if let Some(q) = before {
        let d = sub(here, q);
        let n = left_normal(mul(d, 1.0 / norm(d)));
        sum = add(sum, n);
        last = Some(n);
    }
    if let Some(q) = after {
        let d = sub(q, here);
        let n = left_normal(mul(d, 1.0 / norm(d)));
        sum = add(sum, n);
        last = Some(n);
    }
    let len = norm(sum);
    if len > TURN_EPS {
        Some(mul(sum, 1.0 / len))
    } else {
        last
    }
}

/// The two loops of a closed strand's band: the bigger one is the outer
/// edge (wound positive), the smaller the hole (negative), so nonzero
/// fill leaves the middle open.
fn push_ring_pair(out: &mut Vec<Vec<[f32; 2]>>, a: Vec<P>, b: Vec<P>) {
    let (outer, inner) = if shoelace(&a).abs() >= shoelace(&b).abs() {
        (a, b)
    } else {
        (b, a)
    };
    out.push(to_f32_loop(oriented(outer, true)));
    out.push(to_f32_loop(oriented(inner, false)));
}

fn strand_loops(
    strand: &[[f32; 2]],
    align: PathAlign,
    r: f64,
    centroid: P,
    out: &mut Vec<Vec<[f32; 2]>>,
) {
    let pts = distinct_points(strand);
    if pts.is_empty() {
        return;
    }
    if pts.len() == 1 {
        // One lamp has no direction to align to: the pad_hull square,
        // whatever the alignment — an object with no area would be an
        // object nobody could click.
        let [x, y] = pts[0];
        out.push(to_f32_loop(vec![
            [x - r, y - r],
            [x + r, y - r],
            [x + r, y + r],
            [x - r, y + r],
        ]));
        return;
    }
    if let Some(ring) = closed_ring(&pts) {
        let ring = ring.to_vec();
        let dirs = segment_dirs(&ring, true);
        match align {
            PathAlign::On => {
                let left = offset_closed(&ring, &dirs, r, 1.0);
                let right = offset_closed(&ring, &dirs, r, -1.0);
                push_ring_pair(out, left, right);
            }
            PathAlign::Inside | PathAlign::Outside => {
                let inside = if shoelace(&ring) > 0.0 { -1.0 } else { 1.0 };
                let sign = if align == PathAlign::Inside {
                    inside
                } else {
                    -inside
                };
                let edge = offset_closed(&ring, &dirs, 2.0 * r, sign);
                // The ring itself IS the band's path-side edge.
                push_ring_pair(out, ring, edge);
            }
        }
        return;
    }
    let dirs = segment_dirs(&pts, false);
    match align {
        PathAlign::On => {
            // Left side out, semicircle cap, right side back, semicircle
            // cap — an SVG stroke of width 2r, expanded by hand.
            let mut band = offset_open(&pts, &dirs, r, 1.0);
            let n_last = left_normal(dirs[dirs.len() - 1]);
            push_arc(
                &mut band,
                pts[pts.len() - 1],
                r,
                n_last[1].atan2(n_last[0]),
                std::f64::consts::PI,
            );
            band.extend(offset_open(&pts, &dirs, r, -1.0).into_iter().rev());
            let n_first = left_normal(dirs[0]);
            push_arc(
                &mut band,
                pts[0],
                r,
                (-n_first[1]).atan2(-n_first[0]),
                std::f64::consts::PI,
            );
            out.push(to_f32_loop(oriented(band, true)));
        }
        PathAlign::Inside | PathAlign::Outside => {
            let inside = open_inside_sign(&pts, &dirs, centroid);
            let sign = if align == PathAlign::Inside {
                inside
            } else {
                -inside
            };
            // The strand forward, the 2r offset edge backward: the lamp
            // path is one edge of the loop, the end connectors are the
            // flat caps.
            let edge = offset_open(&pts, &dirs, 2.0 * r, sign);
            let mut band = pts;
            band.extend(edge.into_iter().rev());
            out.push(to_f32_loop(oriented(band, true)));
        }
    }
}

/// The aligned outline of an object's lamp strands: one closed loop per
/// open strand, two (edge + hole) per closed one, plus the degenerate
/// bodies (one lamp → square, two lamps → capsule). Render with
/// fill-rule "nonzero" so overlapping strand loops merge visually.
///
/// `r` is the band's reach: `On` spans `±r` around the path, `Inside`/
/// `Outside` span `2r` wholly on one side, so every alignment paints the
/// same band width.
#[must_use]
pub fn aligned_outline(strands: &[Vec<[f32; 2]>], align: PathAlign, r: f32) -> Vec<Vec<[f32; 2]>> {
    let r = f64::from(r.max(0.0));
    let centroid = object_centroid(strands);
    let mut out = Vec::with_capacity(strands.len());
    for strand in strands {
        strand_loops(strand, align, r, centroid, &mut out);
    }
    out
}

/// The HIT body (planning Q7): always the symmetric on-path band,
/// whatever the visual alignment — a thin one-sided visual must not make
/// the object harder to click. A named alias so call sites read as
/// policy, not as an arbitrary `PathAlign::On`.
#[must_use]
pub fn hit_body(strands: &[Vec<[f32; 2]>], r: f32) -> Vec<Vec<[f32; 2]>> {
    aligned_outline(strands, PathAlign::On, r)
}

/// Nonzero-winding containment over every loop — the same rule the paint
/// uses, so a closed strand's hole really is a hole and overlapping
/// strand loops really do merge. (An even-odd any-hit per loop, the rule
/// the old convex-hull body was tested with, would fill the hole.)
#[must_use]
pub fn point_in_loops(loops: &[Vec<[f32; 2]>], point: [f32; 2]) -> bool {
    let p = to_f64(point);
    let mut winding = 0_i32;
    for polygon in loops {
        if polygon.len() < 3 {
            continue;
        }
        let mut a = to_f64(polygon[polygon.len() - 1]);
        for vertex in polygon {
            let b = to_f64(*vertex);
            if a[1] <= p[1] {
                if b[1] > p[1] && cross(sub(b, a), sub(p, a)) > 0.0 {
                    winding += 1;
                }
            } else if b[1] <= p[1] && cross(sub(b, a), sub(p, a)) < 0.0 {
                winding -= 1;
            }
            a = b;
        }
    }
    winding != 0
}

/// Min distance from `point` to any loop EDGE (not the filled area) —
/// P5's hover/click slop ring. Empty input is infinitely far away.
#[must_use]
pub fn dist_to_loops(loops: &[Vec<[f32; 2]>], point: [f32; 2]) -> f32 {
    let p = to_f64(point);
    let mut best = f64::INFINITY;
    for polygon in loops {
        match polygon.len() {
            0 => {}
            1 => best = best.min(norm(sub(to_f64(polygon[0]), p))),
            _ => {
                let mut a = to_f64(polygon[polygon.len() - 1]);
                for vertex in polygon {
                    let b = to_f64(*vertex);
                    best = best.min(point_segment_dist(p, a, b));
                    a = b;
                }
            }
        }
    }
    best as f32
}

fn point_segment_dist(p: P, a: P, b: P) -> f64 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    if len2 <= 0.0 {
        return norm(sub(p, a));
    }
    let t = (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0);
    norm(sub(p, add(a, mul(ab, t))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An L: two runs meeting at (20,0), centroid below-left of the
    /// corner, so "inside" is unambiguous (+y for the horizontal run,
    /// -x for the vertical one).
    fn l_strand() -> Vec<Vec<[f32; 2]>> {
        vec![vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0]]]
    }

    /// Min distance from a point to the OPEN strand polyline (no phantom
    /// closing edge — [`dist_to_loops`] closes its loops, which would
    /// flatter a flung vertex near the strand's open end).
    fn dist_to_strand(strand: &[[f32; 2]], point: [f32; 2]) -> f64 {
        let p = to_f64(point);
        (0..strand.len() - 1)
            .map(|i| point_segment_dist(p, to_f64(strand[i]), to_f64(strand[i + 1])))
            .fold(f64::INFINITY, f64::min)
    }

    /// The symmetric band contains the strand and its `r` margin, and
    /// stops before `2r`.
    #[test]
    fn symmetric_band_hugs_the_strand_and_stops_at_its_reach() {
        let loops = aligned_outline(&l_strand(), PathAlign::On, 2.0);
        assert_eq!(loops.len(), 1);
        for p in [[0.0, 0.0], [10.0, 0.0], [20.0, 5.0], [20.0, 10.0]] {
            assert!(point_in_loops(&loops, p), "strand point {p:?} escaped");
        }
        for p in [[10.0, 1.5], [10.0, -1.5], [21.5, 5.0], [-1.5, 0.0]] {
            assert!(point_in_loops(&loops, p), "margin point {p:?} escaped");
        }
        for p in [[10.0, 4.0], [10.0, -4.0], [26.0, 5.0], [-5.0, 0.0]] {
            assert!(!point_in_loops(&loops, p), "far point {p:?} contained");
        }
    }

    /// The one-sided band lies wholly on the centroid side and the strand
    /// itself is its edge — the channel-letter ruling.
    #[test]
    fn inside_band_lies_on_the_centroid_side_with_the_strand_as_edge() {
        let strands = l_strand();
        let loops = aligned_outline(&strands, PathAlign::Inside, 2.0);
        assert_eq!(loops.len(), 1);
        assert!(point_in_loops(&loops, [10.0, 2.0]));
        assert!(point_in_loops(&loops, [18.0, 5.0]));
        assert!(!point_in_loops(&loops, [10.0, -2.0]), "wrong side");
        assert!(
            !point_in_loops(&loops, [21.5, 5.0]),
            "wrong side of the vertical run"
        );
        for p in &strands[0] {
            let d = dist_to_loops(&loops, *p);
            assert!(d < 1e-3, "strand point {p:?} is {d} off the band edge");
        }
        // The whole 2r width sits on the one side.
        assert!(point_in_loops(&loops, [10.0, 3.5]));
        assert!(!point_in_loops(&loops, [10.0, 4.5]));
    }

    /// Outside is inside's mirror ACROSS THE PATH: reflect the plane
    /// across a straight strand and the two bands trade places exactly.
    /// (Reflecting the whole strand cannot swap sides — the centroid
    /// reflects with it, so inside stays inside.)
    #[test]
    fn outside_band_is_the_mirror_of_inside() {
        let strands = vec![vec![[0.0_f32, 0.0], [20.0, 0.0]]];
        let inside = aligned_outline(&strands, PathAlign::Inside, 2.0);
        let outside = aligned_outline(&strands, PathAlign::Outside, 2.0);
        // The centroid tie deterministically picks left of travel (-y).
        assert!(point_in_loops(&inside, [10.0, -2.0]));
        assert!(point_in_loops(&outside, [10.0, 2.0]));
        // Sample grid offset off the construction lines so no probe sits
        // exactly on a boundary.
        for xi in -2..=11 {
            for yi in -5..=5 {
                let p = [xi as f32 * 2.0 + 0.13, yi as f32 + 0.17];
                assert_eq!(
                    point_in_loops(&inside, p),
                    point_in_loops(&outside, [p[0], -p[1]]),
                    "{p:?}"
                );
            }
        }
        // On an asymmetric strand the two alignments claim opposite
        // sides of the same path.
        let l = l_strand();
        let l_inside = aligned_outline(&l, PathAlign::Inside, 2.0);
        let l_outside = aligned_outline(&l, PathAlign::Outside, 2.0);
        assert!(point_in_loops(&l_inside, [10.0, 2.0]));
        assert!(!point_in_loops(&l_outside, [10.0, 2.0]));
        assert!(point_in_loops(&l_outside, [10.0, -2.0]));
        assert!(!point_in_loops(&l_inside, [10.0, -2.0]));
    }

    /// A closed square's inside band hugs the interior wall whichever way
    /// the points wind — and the middle is a hole, not a fill.
    #[test]
    fn closed_square_inside_band_stays_interior_for_both_windings() {
        let forward = vec![
            [0.0_f32, 0.0],
            [30.0, 0.0],
            [30.0, 30.0],
            [0.0, 30.0],
            [0.0, 0.0],
        ];
        let mut reverse = forward.clone();
        reverse.reverse();
        for pts in [forward, reverse] {
            let loops = aligned_outline(&[pts], PathAlign::Inside, 2.0);
            assert_eq!(loops.len(), 2, "a ring's band is an annulus");
            for p in [[15.0, 2.0], [2.0, 15.0], [28.0, 15.0], [15.0, 28.0]] {
                assert!(point_in_loops(&loops, p), "{p:?} not in the wall band");
            }
            assert!(
                !point_in_loops(&loops, [15.0, 15.0]),
                "the middle is a hole"
            );
            assert!(!point_in_loops(&loops, [15.0, -2.0]), "outside the ring");
        }
    }

    /// A C-shaped strand's band never enters the C's mouth — the
    /// convex-hull regression this whole round exists to fix.
    #[test]
    fn the_band_never_enters_a_concave_mouth() {
        let arc: Vec<[f32; 2]> = (0..=27)
            .map(|i| {
                let a = (45.0 + 10.0 * i as f32).to_radians();
                [20.0 * a.cos(), 20.0 * a.sin()]
            })
            .collect();
        let loops = aligned_outline(&[arc], PathAlign::On, 2.0);
        assert!(point_in_loops(&loops, [-19.0, 0.0]), "on the arc itself");
        for p in [[0.0, 0.0], [10.0, 0.0], [15.0, 0.0], [0.0, 10.0]] {
            assert!(!point_in_loops(&loops, p), "mouth point {p:?} swallowed");
        }
    }

    /// A needle apex may not fling a vertex past the 2.5r miter clamp —
    /// the regression against `pad_hull`'s 4x fling.
    #[test]
    fn a_needle_apex_stays_within_the_miter_clamp() {
        let strand = vec![[0.0_f32, 0.0], [20.0, 2.0], [0.0, 4.0]];
        let r = 2.0_f32;
        let loops = aligned_outline(&[strand.clone()], PathAlign::On, r);
        let max_reach = f64::from(r) * MITER_LIMIT + 1e-3;
        for polygon in &loops {
            for v in polygon {
                let d = dist_to_strand(&strand, *v);
                assert!(d <= max_reach, "vertex {v:?} flung to {d} > {max_reach}");
            }
        }
    }

    /// Strands split at a gap stay separate loops with nothing bridging
    /// the gap.
    #[test]
    fn strands_split_at_a_gap_stay_two_separate_loops() {
        let strands = vec![
            vec![[0.0_f32, 0.0], [10.0, 0.0]],
            vec![[30.0, 0.0], [40.0, 0.0]],
        ];
        let loops = aligned_outline(&strands, PathAlign::On, 2.0);
        assert_eq!(loops.len(), 2);
        assert!(point_in_loops(&loops, [5.0, 0.0]));
        assert!(point_in_loops(&loops, [35.0, 0.0]));
        assert!(!point_in_loops(&loops, [20.0, 0.0]), "the gap stays open");
    }

    /// One lamp squares, two lamps make a capsule, nothing stays nothing.
    #[test]
    fn degenerate_strands_make_a_square_a_capsule_and_nothing() {
        for align in [PathAlign::On, PathAlign::Inside, PathAlign::Outside] {
            let loops = aligned_outline(&[vec![[5.0_f32, 5.0]]], align, 2.0);
            assert_eq!(loops.len(), 1);
            assert_eq!(loops[0].len(), 4);
            for corner in [[3.0_f32, 3.0], [7.0, 3.0], [7.0, 7.0], [3.0, 7.0]] {
                assert!(
                    loops[0].contains(&corner),
                    "{corner:?} missing from {:?}",
                    loops[0]
                );
            }
        }
        let loops = aligned_outline(&[vec![[0.0_f32, 0.0], [10.0, 0.0]]], PathAlign::On, 2.0);
        assert_eq!(loops.len(), 1);
        assert!(point_in_loops(&loops, [-1.5, 0.0]), "round start cap");
        assert!(point_in_loops(&loops, [11.5, 0.0]), "round end cap");
        assert!(point_in_loops(&loops, [5.0, 1.5]));
        assert!(!point_in_loops(&loops, [5.0, 3.0]));
        assert!(!point_in_loops(&loops, [-3.0, 0.0]));
        assert!(aligned_outline(&[], PathAlign::On, 2.0).is_empty());
        assert!(aligned_outline(&[vec![]], PathAlign::On, 2.0).is_empty());
        assert!(dist_to_loops(&[], [0.0, 0.0]).is_infinite());
    }

    /// The hit body is the symmetric band no matter what the VISUAL
    /// alignment paints — thin bodies must not make clicking harder.
    #[test]
    fn hit_body_is_the_symmetric_band_whatever_the_visual_alignment() {
        let strands = l_strand();
        let hit = hit_body(&strands, 2.0);
        assert_eq!(hit, aligned_outline(&strands, PathAlign::On, 2.0));
        let p = [10.0, -1.5];
        assert!(point_in_loops(&hit, p), "hit body keeps the far side");
        assert!(
            !point_in_loops(&aligned_outline(&strands, PathAlign::Inside, 2.0), p),
            "the inside visual excludes it"
        );
    }
}
