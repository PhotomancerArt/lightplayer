//! Object HULLS: the plain geometry behind "an object is a THING".
//!
//! The G1 round-3 walk rejected objects rendered as loose fields of tiny
//! circles — "betrays that they should feel like individual THINGS not
//! collections of hard to target tiny things". The answer is one closed
//! outline per object, in the same padded-frame idiom the canvas already
//! speaks for fixtures: a convex hull of the object's lamps, pushed outward
//! so the shape reads as a body rather than as a taut rubber band, and
//! filled faintly so it is a click target everywhere inside.
//!
//! Pure math on purpose — no Dioxus, no sprites. The hull is computed ONCE
//! per surface build (the shell's render pass) and then only rendered and
//! hit-tested, because dome-scale means ~150 of these on screen at once.

/// Andrew's monotone chain: the convex hull of `points`, counter-clockwise
/// in a y-DOWN space (which is what SVG user space is), no repeated last
/// vertex.
///
/// Collinear runs are dropped — a straight strand comes back as its two
/// endpoints, which [`pad_hull`] then knows to thicken into a body rather
/// than a zero-area line.
#[must_use]
pub fn convex_hull(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut sorted: Vec<[f64; 2]> = points
        .iter()
        .map(|point| [f64::from(point[0]), f64::from(point[1])])
        .collect();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    sorted.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if sorted.len() < 3 {
        return sorted
            .into_iter()
            .map(|point| [point[0] as f32, point[1] as f32])
            .collect();
    }
    // Cross product of (o→a) × (o→b): > 0 turns one way, < 0 the other.
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut chain: Vec<[f64; 2]> = Vec::with_capacity(sorted.len() * 2);
    for point in sorted.iter().copied() {
        while chain.len() >= 2
            && cross(chain[chain.len() - 2], chain[chain.len() - 1], point) <= 0.0
        {
            chain.pop();
        }
        chain.push(point);
    }
    let lower = chain.len() + 1;
    for point in sorted.iter().rev().copied() {
        while chain.len() >= lower
            && cross(chain[chain.len() - 2], chain[chain.len() - 1], point) <= 0.0
        {
            chain.pop();
        }
        chain.push(point);
    }
    chain.pop();
    chain
        .into_iter()
        .map(|point| [point[0] as f32, point[1] as f32])
        .collect()
}

/// Push a convex hull outward by `pad` (own-space units), so the outline
/// stands OFF the lamps it contains.
///
/// The degenerate cases are the point: a one-lamp object becomes a small
/// square and a perfectly straight strand becomes a capsule-ish rectangle,
/// because an object with no area would be an object nobody could click.
///
/// For a real polygon each vertex slides along the miter of its two edge
/// normals, clamped so a needle-sharp corner cannot fling a vertex to
/// infinity.
#[must_use]
pub fn pad_hull(hull: &[[f32; 2]], pad: f32) -> Vec<[f32; 2]> {
    let pad = pad.max(0.0);
    match hull.len() {
        0 => Vec::new(),
        1 => {
            let [x, y] = hull[0];
            Vec::from([
                [x - pad, y - pad],
                [x + pad, y - pad],
                [x + pad, y + pad],
                [x - pad, y + pad],
            ])
        }
        2 => {
            let [ax, ay] = hull[0];
            let [bx, by] = hull[1];
            let (dx, dy) = (bx - ax, by - ay);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                return pad_hull(&hull[..1], pad);
            }
            // Along the strand, and across it.
            let (ux, uy) = (dx / len * pad, dy / len * pad);
            let (nx, ny) = (-uy, ux);
            Vec::from([
                [ax - ux + nx, ay - uy + ny],
                [bx + ux + nx, by + uy + ny],
                [bx + ux - nx, by + uy - ny],
                [ax - ux - nx, ay - uy - ny],
            ])
        }
        count => {
            let mut out = Vec::with_capacity(count);
            for index in 0..count {
                let prev = hull[(index + count - 1) % count];
                let here = hull[index];
                let next = hull[(index + 1) % count];
                let normal = |from: [f32; 2], to: [f32; 2]| {
                    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 1e-6 {
                        [0.0, 0.0]
                    } else {
                        // Outward for the CCW-in-y-down winding
                        // [`convex_hull`] produces.
                        [dy / len, -dx / len]
                    }
                };
                let a = normal(prev, here);
                let b = normal(here, next);
                let sum = [a[0] + b[0], a[1] + b[1]];
                let len = (sum[0] * sum[0] + sum[1] * sum[1]).sqrt();
                let offset = if len < 1e-6 {
                    // A 180° spike: there is no miter, so step off the
                    // outgoing edge.
                    [b[0] * pad, b[1] * pad]
                } else {
                    // Miter length is pad / cos(θ/2), and |sum| / 2 IS
                    // cos(θ/2). Clamped: a sliver corner must not explode.
                    let miter = (pad * 2.0 / len).min(pad * 4.0);
                    [sum[0] / len * miter, sum[1] / len * miter]
                };
                out.push([here[0] + offset[0], here[1] + offset[1]]);
            }
            out
        }
    }
}

/// Is `point` inside this polygon? Even-odd ray crossing, which is exact
/// enough for a click target and needs no winding assumption.
#[must_use]
pub(crate) fn point_in_polygon(polygon: &[[f32; 2]], point: [f64; 2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (xi, yi) = (f64::from(polygon[i][0]), f64::from(polygon[i][1]));
        let (xj, yj) = (f64::from(polygon[j][0]), f64::from(polygon[j][1]));
        if (yi > point[1]) != (yj > point[1])
            && point[0] < (xj - xi) * (point[1] - yi) / (yj - yi) + xi
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The polygon as an SVG `d`, closed — one path element per object is the
/// whole scale budget (dome-scale: ~150 of them).
#[must_use]
pub(crate) fn hull_path_d(polygon: &[[f32; 2]]) -> String {
    let mut d = String::with_capacity(polygon.len() * 16);
    for (index, point) in polygon.iter().enumerate() {
        d.push_str(if index == 0 { "M" } else { "L" });
        d.push_str(&format!("{:.2},{:.2}", point[0], point[1]));
        if index + 1 < polygon.len() {
            d.push(' ');
        }
    }
    if !d.is_empty() {
        d.push('Z');
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hull of a filled square is its four corners, and the interior
    /// points are dropped.
    #[test]
    fn convex_hull_keeps_the_corners_and_drops_the_inside() {
        let points = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [5.0, 5.0],
            [2.0, 8.0],
        ];
        let hull = convex_hull(&points);
        assert_eq!(hull.len(), 4, "{hull:?}");
        for corner in [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]] {
            assert!(hull.contains(&corner), "{corner:?} missing from {hull:?}");
        }
    }

    /// A straight strand has no area: the hull collapses to its two ends,
    /// which is exactly the case padding has to rescue.
    #[test]
    fn a_collinear_run_hulls_to_its_endpoints() {
        let points: Vec<[f32; 2]> = (0..30).map(|i| [i as f32, 0.0]).collect();
        assert_eq!(convex_hull(&points), vec![[0.0, 0.0], [29.0, 0.0]]);
        // …and one lamp, or none, degenerate honestly rather than panicking.
        assert_eq!(convex_hull(&[[3.0, 4.0]]), vec![[3.0, 4.0]]);
        assert!(convex_hull(&[]).is_empty());
    }

    /// Padding pushes every vertex OUT — the point of the hull being a body
    /// and not a rubber band — and a click just outside the lamps still
    /// lands inside it.
    #[test]
    fn padding_grows_a_polygon_outward() {
        let hull = convex_hull(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
        let padded = pad_hull(&hull, 2.0);
        assert_eq!(padded.len(), 4);
        for point in &padded {
            assert!(
                point[0] < -1.0 || point[0] > 11.0,
                "{point:?} did not move out"
            );
        }
        assert!(point_in_polygon(&padded, [-1.5, 5.0]), "{padded:?}");
        assert!(point_in_polygon(&padded, [5.0, 5.0]));
        assert!(!point_in_polygon(&padded, [-5.0, 5.0]));
    }

    /// The degenerate shapes get real, clickable area — the reason a
    /// straight strand is targetable at all.
    #[test]
    fn a_line_and_a_point_pad_into_clickable_bodies() {
        let strand = pad_hull(&[[0.0, 0.0], [30.0, 0.0]], 3.0);
        assert_eq!(strand.len(), 4);
        assert!(point_in_polygon(&strand, [15.0, 2.0]), "{strand:?}");
        assert!(point_in_polygon(&strand, [-2.0, 0.0]), "the capped end");
        assert!(!point_in_polygon(&strand, [15.0, 9.0]));

        let dot = pad_hull(&[[5.0, 5.0]], 2.0);
        assert!(point_in_polygon(&dot, [6.0, 4.0]));
        assert!(!point_in_polygon(&dot, [9.0, 5.0]));

        assert!(pad_hull(&[], 3.0).is_empty());
    }

    /// One path element per object, closed — the scale contract.
    #[test]
    fn the_path_is_one_closed_subpath() {
        let d = hull_path_d(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!(d, "M0.00,0.00 L10.00,0.00 L10.00,10.00Z");
        assert_eq!(d.matches('M').count(), 1);
        assert!(hull_path_d(&[]).is_empty());
    }
}
