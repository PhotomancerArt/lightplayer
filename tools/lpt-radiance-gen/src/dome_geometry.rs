//! The radiance dome's panels and their plan-view outlines.
//!
//! Reimplemented from lp2014's description of the dome, not transliterated
//! (`RadianceDomeGeometry`, `GeodesicSphereGeometry`,
//! `GeodesicIcosahedralFixture.remap()`):
//!
//! - **Sphere.** An icosahedron with a vertex at the zenith `(0, 0, 1)`,
//!   the upper pentagon at azimuths `(i + ½)·72°` and the lower one at
//!   `i·72°` (lp2014's vertices 6–10 and 1–5 after its −90° turn about X).
//!   Each face is split class-I into `FREQUENCY²` triangles and pushed out
//!   to the unit sphere.
//! - **Rings.** Ring `r` is every face whose nearest vertex is `r` edges
//!   from the zenith (lp2014's `VertexFaceGrouping`); the top
//!   [`LIGHTED_RINGS`] rings are the lit panels: 5, 15, 25, 35, 45, then
//!   50, 50, 50 = 275.
//! - **Sectors.** lp2014 groups the dome in fifths by icosahedron face
//!   (`restrictToFifth`: top face `g`, downward middle face `5 + g`,
//!   upward middle face `10 + g`); 55 panels each. Rotating fifth `g` by
//!   +72° of azimuth gives fifth `g + 1` exactly.
//! - **Plan view.** Azimuthal equidistant about the zenith with lp2014's
//!   orientation: `(x, y) = −c·(sin λ, cos λ)` for colatitude `c` and
//!   azimuth `λ`, y down. Unlike lp2014 the scale is uniform (lp2014
//!   stretched x and y separately to fill a unit box): one doc unit is one
//!   inch of arc from the zenith on lp2014's 19-foot-radius dome.
//! - **Edges.** Each panel edge is cut into lp2014's `expansionFactor`
//!   segments (`2 + ⌊|z_low − ½|·5⌋`, `z_low` the face's lowest vertex)
//!   along its great circle, so long edges curve in the plan.
//!
//! In that plan increasing azimuth runs **counter-clockwise** on screen,
//! while the product's repeat turns instance `k` **clockwise** by `k·72°`
//! (`Rotation2d`, y-down). So sectors are numbered clockwise: sector `k`
//! is lp2014's fifth `(1 − k) mod 5`. Sector 0 is fifth 1 because that is
//! the fifth holding lp2014's panel 0 (its rings run by descending
//! azimuth from +180°), and within every ring lp2014's descending-azimuth
//! order is clockwise on screen too, so panel numbers here and there run
//! the same way round.

use std::collections::{BTreeMap, VecDeque};

/// Geodesic frequency (`RadianceDomeSpecs.RADIANCE_DOME`).
pub const FREQUENCY: usize = 5;
/// Lit face rings from the zenith: 8 is lp2014's 2017 config (275
/// panels); its `RadianceDomeSpecs` says 7 (225). Fit-check Q-E.
pub const LIGHTED_RINGS: usize = 8;
/// The plan's rotational symmetry.
pub const SECTORS: usize = 5;
/// Panels per sector.
pub const CLASSES: usize = 55;
/// Dome radius in doc units: 19 ft (`RADIANCE_DOME`) in inches, so a plan
/// distance from the zenith is inches of arc along the shell.
pub const RADIUS: f64 = 228.0;
/// Each panel is its face shrunk toward the face's centroid. lp2014 drew
/// the whole face, but the lucite panel hangs inside it (2 ft tall,
/// `RADIANCE_DOME.maxPanelHeight`, in a face ≈ 3.6 ft tall at this radius),
/// and a whole-face outline puts two panels' lamps on every shared edge.
pub const PANEL_SHRINK: f64 = 0.55;
/// Blank doc-space margin around the plan, so a lamp rotated in f32 by the
/// product's resolver never lands a hair outside the canvas.
pub const MARGIN: f64 = 1.0;

pub type V3 = [f64; 3];
pub type P2 = [f64; 2];

/// One lit panel: its face on the unit sphere and where it sits.
#[derive(Debug, Clone)]
pub struct Panel {
    /// lp2014's fifth (icosahedron face group, `restrictToFifth − 1`).
    pub fifth: usize,
    /// Face ring from the zenith, 0-based.
    pub ring: usize,
    /// The face's corners on the unit sphere.
    pub corners: [V3; 3],
}

/// The lit dome, by sector and class: `panels[sector][class]`, classes in
/// lp2014's order (ring by ring from the top, each ring clockwise).
pub struct Dome {
    pub panels: Vec<Vec<Panel>>,
}

impl Dome {
    pub fn build() -> Self {
        let lit = lit_faces();
        let panels = (0..SECTORS)
            .map(|sector| {
                let fifth = fifth_of_sector(sector);
                let mut faces: Vec<Panel> =
                    lit.iter().filter(|p| p.fifth == fifth).cloned().collect();
                faces.sort_by(|a, b| {
                    a.ring
                        .cmp(&b.ring)
                        .then(relative_azimuth(b, fifth).total_cmp(&relative_azimuth(a, fifth)))
                });
                faces
            })
            .collect();
        Self { panels }
    }

    /// The ring and 0-based position within the ring of a class.
    pub fn ring_position(&self, class: usize) -> (usize, usize) {
        let ring = self.panels[0][class].ring;
        let first = self.panels[0].iter().position(|p| p.ring == ring);
        (
            ring,
            class - first.expect("the class's ring has a first panel"),
        )
    }
}

/// lp2014's fifth that sector `sector` is (see the module doc).
pub fn fifth_of_sector(sector: usize) -> usize {
    (1 + SECTORS - sector % SECTORS) % SECTORS
}

/// The plan point (zenith at the origin, doc units) of a direction.
pub fn project(v: V3) -> P2 {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let colatitude = (v[2] / length).clamp(-1.0, 1.0).acos();
    let azimuth = v[1].atan2(v[0]);
    [
        -RADIUS * colatitude * azimuth.sin(),
        -RADIUS * colatitude * azimuth.cos(),
    ]
}

/// A panel's plan outline about the zenith (unrounded): the shrunken face,
/// each edge cut into the face's expansion factor of great-circle
/// segments, wound clockwise on screen from the corner farthest from the
/// zenith (where the feed arrives from below). When two corners tie, the
/// strip starts on the one whose next corner is the other, so it runs the
/// bottom edge first.
pub fn panel_outline(panel: &Panel) -> Vec<P2> {
    let factor = expansion_factor(panel);
    let centroid = mean3(&panel.corners);
    let mut corners: Vec<V3> = panel
        .corners
        .iter()
        .map(|c| lerp3(centroid, *c, PANEL_SHRINK))
        .collect();
    if signed_area(&corners.iter().map(|c| project(*c)).collect::<Vec<_>>()) < 0.0 {
        corners.reverse();
    }
    let start = feed_corner(&corners);
    let mut outline = Vec::with_capacity(3 * factor);
    for side in 0..3 {
        let from = corners[(start + side) % 3];
        let to = corners[(start + side + 1) % 3];
        for step in 0..factor {
            outline.push(project(lerp3(from, to, step as f64 / factor as f64)));
        }
    }
    outline
}

/// lp2014's `expansionFactor`: segments per edge, more toward the top and
/// the rim where the projection bends edges most.
pub fn expansion_factor(panel: &Panel) -> usize {
    let lowest = panel
        .corners
        .iter()
        .map(|c| c[2])
        .fold(f64::INFINITY, f64::min);
    2 + ((lowest - 0.5).abs() * 5.0).floor() as usize
}

/// The largest plan distance of any lit face corner from the zenith: the
/// lowest lit ring, the dome's rim in the plan.
pub fn rim_radius(dome: &Dome) -> f64 {
    dome.panels
        .iter()
        .flatten()
        .flat_map(|p| p.corners.iter())
        .map(|c| {
            let p = project(*c);
            p[0].hypot(p[1])
        })
        .fold(0.0, f64::max)
}

/// Every lit face of the dome, with its fifth and ring.
fn lit_faces() -> Vec<Panel> {
    let mut faces = Vec::new();
    for fifth in 0..SECTORS {
        for icosa_face in fifth_icosa_faces(fifth) {
            faces.extend(split_face(icosa_face).into_iter().map(|corners| Panel {
                fifth,
                ring: 0,
                corners,
            }));
        }
    }
    let rings = face_rings(&faces);
    faces
        .into_iter()
        .zip(rings)
        .filter(|(_, ring)| *ring < LIGHTED_RINGS)
        .map(|(face, ring)| Panel { ring, ..face })
        .collect()
}

/// The three icosahedron faces of one fifth: top, downward middle, upward
/// middle (lp2014's faces `g`, `5 + g`, `10 + g`).
fn fifth_icosa_faces(fifth: usize) -> [[V3; 3]; 3] {
    let beta = 2f64.atan(); // polar angle of the pentagons from the poles
    let ring_vertex = |azimuth_deg: f64, z: f64| -> V3 {
        let a = azimuth_deg.to_radians();
        let r = beta.sin();
        [r * a.cos(), r * a.sin(), z]
    };
    let upper = |i: usize| ring_vertex((i % 5) as f64 * 72.0 + 36.0, beta.cos());
    let lower = |i: usize| ring_vertex((i % 5) as f64 * 72.0, -beta.cos());
    let zenith = [0.0, 0.0, 1.0];
    let g = fifth;
    [
        [upper(g), zenith, upper(g + 1)],
        [upper(g), upper(g + 1), lower(g + 1)],
        [lower(g), upper(g), lower(g + 1)],
    ]
}

/// Class-I split of one flat face into `FREQUENCY²` triangles, each
/// corner pushed out to the unit sphere.
fn split_face([a, b, c]: [V3; 3]) -> Vec<[V3; 3]> {
    let n = FREQUENCY;
    let lattice = |i: usize, j: usize| -> V3 {
        let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
        normalize([
            a[0] + (b[0] - a[0]) * u + (c[0] - a[0]) * v,
            a[1] + (b[1] - a[1]) * u + (c[1] - a[1]) * v,
            a[2] + (b[2] - a[2]) * u + (c[2] - a[2]) * v,
        ])
    };
    let mut triangles = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n - i {
            triangles.push([lattice(i, j), lattice(i + 1, j), lattice(i, j + 1)]);
            if i + j + 1 < n {
                triangles.push([lattice(i + 1, j), lattice(i + 1, j + 1), lattice(i, j + 1)]);
            }
        }
    }
    triangles
}

/// Each face's ring: the edge distance from the zenith of its nearest
/// corner, by breadth-first search over the faces' shared vertices.
fn face_rings(faces: &[Panel]) -> Vec<usize> {
    let key = |v: V3| v.map(|x| (x * 1e9).round() as i64);
    let mut ids: BTreeMap<[i64; 3], usize> = BTreeMap::new();
    let face_ids: Vec<[usize; 3]> = faces
        .iter()
        .map(|f| {
            f.corners.map(|c| {
                let next = ids.len();
                *ids.entry(key(c)).or_insert(next)
            })
        })
        .collect();
    let mut neighbours = vec![Vec::new(); ids.len()];
    for [a, b, c] in &face_ids {
        for (x, y) in [(a, b), (b, c), (c, a)] {
            neighbours[*x].push(*y);
            neighbours[*y].push(*x);
        }
    }
    let mut distance = vec![usize::MAX; ids.len()];
    let zenith = ids[&key([0.0, 0.0, 1.0])];
    distance[zenith] = 0;
    let mut queue = VecDeque::from([zenith]);
    while let Some(v) = queue.pop_front() {
        for &n in &neighbours[v] {
            if distance[n] == usize::MAX {
                distance[n] = distance[v] + 1;
                queue.push_back(n);
            }
        }
    }
    face_ids
        .iter()
        .map(|corners| corners.iter().map(|v| distance[*v]).min().unwrap_or(0))
        .collect()
}

/// The face centroid's azimuth in degrees, measured from its fifth's first
/// edge (azimuth `72·fifth`), so every fifth sorts the same way.
fn relative_azimuth(panel: &Panel, fifth: usize) -> f64 {
    let c = mean3(&panel.corners);
    (c[1].atan2(c[0]).to_degrees() - 72.0 * fifth as f64).rem_euclid(360.0)
}

/// The corner the strip starts on (see [`panel_outline`]).
fn feed_corner(corners: &[V3]) -> usize {
    let lowest = corners.iter().map(|c| c[2]).fold(f64::INFINITY, f64::min);
    let tied: Vec<usize> = (0..3).filter(|i| corners[*i][2] - lowest < 1e-9).collect();
    match tied.as_slice() {
        [only] => *only,
        [a, b] if (a + 1) % 3 == *b => *a,
        [_, b] => *b,
        _ => 0,
    }
}

/// Shoelace sum; positive is clockwise on a y-down screen.
fn signed_area(points: &[P2]) -> f64 {
    (0..points.len())
        .map(|i| {
            let (p, q) = (points[i], points[(i + 1) % points.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum()
}

fn mean3(points: &[V3; 3]) -> V3 {
    [0, 1, 2].map(|axis| points.iter().map(|p| p[axis]).sum::<f64>() / 3.0)
}

fn lerp3(a: V3, b: V3, t: f64) -> V3 {
    [0, 1, 2].map(|axis| a[axis] + (b[axis] - a[axis]) * t)
}

fn normalize(v: V3) -> V3 {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|x| x / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_rings_hold_275_panels() {
        let dome = Dome::build();
        let mut per_ring = [0usize; LIGHTED_RINGS];
        for panel in dome.panels.iter().flatten() {
            per_ring[panel.ring] += 1;
        }
        assert_eq!(per_ring, [5, 15, 25, 35, 45, 50, 50, 50]);
        assert_eq!(dome.panels.iter().flatten().count(), 275);
        assert!(dome.panels.iter().all(|sector| sector.len() == CLASSES));
    }

    #[test]
    fn sector_k_is_sector_0_turned_clockwise_by_k_fifths() {
        // Built independently per fifth, then compared against sector 0's
        // outline turned the way the product's repeat turns it.
        let dome = Dome::build();
        for sector in 0..SECTORS {
            for class in 0..CLASSES {
                let base = panel_outline(&dome.panels[0][class]);
                let turned: Vec<P2> = base
                    .iter()
                    .map(|p| turn_clockwise(*p, 72.0 * sector as f64))
                    .collect();
                let real = panel_outline(&dome.panels[sector][class]);
                assert!(
                    distance(mean2(&turned), mean2(&real)) < 0.01,
                    "class {class} instance {sector} misses its sector by centroid"
                );
                assert!(distance(turned[0], real[0]) < 0.01, "feed corner moved");
            }
        }
    }

    #[test]
    fn rings_run_clockwise_within_a_sector() {
        let dome = Dome::build();
        for pair in dome.panels[0].windows(2) {
            if pair[0].ring != pair[1].ring {
                continue;
            }
            let (a, b) = (
                mean2(&panel_outline(&pair[0])),
                mean2(&panel_outline(&pair[1])),
            );
            // Clockwise on y-down screen = increasing screen angle.
            let angle = |p: P2| p[1].atan2(p[0]);
            let step = (angle(b) - angle(a)).rem_euclid(std::f64::consts::TAU);
            assert!(step > 1e-6 && step < std::f64::consts::PI, "{step}");
        }
    }

    #[test]
    fn outlines_wind_clockwise_with_the_lp2014_edge_cuts() {
        let dome = Dome::build();
        for panel in &dome.panels[0] {
            let outline = panel_outline(panel);
            let factor = expansion_factor(panel);
            assert!((2..=5).contains(&factor), "{factor}");
            assert_eq!(outline.len(), 3 * factor);
            assert!(signed_area(&outline) > 0.0);
        }
    }

    #[test]
    fn panel_shrink_matches_a_two_foot_panel() {
        let dome = Dome::build();
        let altitudes: Vec<f64> = dome
            .panels
            .iter()
            .flatten()
            .map(|p| {
                let [a, b, c] = p.corners.map(|v| v.map(|x| x * RADIUS / 12.0));
                let side = |p: V3, q: V3| {
                    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
                };
                let (x, y, z) = (side(a, b), side(b, c), side(c, a));
                let s = (x + y + z) / 2.0;
                let area = (s * (s - x) * (s - y) * (s - z)).sqrt();
                2.0 * area / x.max(y).max(z)
            })
            .collect();
        let mean = altitudes.iter().sum::<f64>() / altitudes.len() as f64;
        assert!(
            (2.0 / mean - PANEL_SHRINK).abs() < 0.05,
            "mean altitude {mean} ft"
        );
    }

    /// The product's `Rotation2d` about the origin: positive = clockwise
    /// on a y-down screen.
    fn turn_clockwise(p: P2, degrees: f64) -> P2 {
        let (sin, cos) = degrees.to_radians().sin_cos();
        [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos]
    }

    fn mean2(points: &[P2]) -> P2 {
        [0, 1].map(|axis| points.iter().map(|p| p[axis]).sum::<f64>() / points.len() as f64)
    }

    fn distance(a: P2, b: P2) -> f64 {
        (a[0] - b[0]).hypot(a[1] - b[1])
    }
}
