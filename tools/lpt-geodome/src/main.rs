//! Generates the mini-dome example's map2d documents from the real dome's
//! structure: a 2-frequency geodesic dome (16', 1" EMT struts) hoisted on a
//! riser ring, glowing via 2'-tall triangular lucite panels suspended inside
//! the strut triangles, with chevron doors (a big triangle, no bottom edge).
//! See `docs/use-cases/2026-08-09-mini-dome.md`, "The real mini dome".
//!
//! The output is a **plan view** (azimuthal equidistant: azimuth preserved,
//! radius proportional to polar angle from the zenith), the same projection
//! lp2014's `GeodesicIcosahedralFixture` mapper used. One 72-degree sector
//! carries the dome's five-fold symmetry: its EIGHT 2V faces (5 icosa faces
//! x 4 = 20 cap + 5 + 15 = 40 per dome) each hold one panel — the host
//! triangle shrunk toward its centroid — and the sector's 30 lamps run
//! around the panels as ONE path whose connector segments are inert `gaps`
//! (jumper wire), so pitch stays uniform and the patch grain stays
//! `/sector/k`. Doors are open chevron paths with an authored stride.
//!
//! Run from anywhere in the workspace:
//!
//! ```bash
//! cargo run -p lpt-geodome
//! ```
//!
//! Deterministic and idempotent: same source, same bytes. Eventually this
//! wants to become a parametric shape (or import flow) in the mapper itself;
//! until then the emitted JSON is the checked-in truth and this tool is how
//! it regenerates.

use lpc_mapping::{
    Map2dDoc, Map2dObject, Map2dObjectId, Map2dShape, PathAlign, PathShape, RepeatShape, resolve,
};
use std::path::Path;

/// Doc-space center of the dome's plan view.
const CENTER: [f32; 2] = [60.0, 60.0];
/// Plan radius of the dome's equator ring (polar 90 degrees).
const R_EQUATOR: f32 = 42.0;
/// Plan radius of the riser's ground circle — the equator plus the hoist.
const R_GROUND: f32 = 52.0;
/// Panel inset: each panel is its host strut triangle shrunk toward the
/// host's centroid (the 2' lucite panel suspended inside the ~4.3' face).
/// Tighter than the physical ratio (~0.46) on purpose: with ~30 lamps
/// spread over eight panels, a sector's three-to-four dots per panel only
/// read as a glowing triangle when they sit close enough for their sample
/// footprints to fuse — the panel glows as a surface in real life, and a
/// compact cluster is the closest a 150-lamp miniature gets to that.
const PANEL_SHRINK: f32 = 0.3;
/// Lamps per dome sector (5 sectors x 30 = the example's 150-lamp dome).
const SECTOR_LAMPS: u32 = 30;
/// Lamps per door (3 doors x 9 = the example's 27-lamp doors fixture).
const DOOR_LAMPS: u32 = 9;

/// 2V vertex rings as polar angles from the zenith, degrees. A/B are the
/// sphere-projected midpoints of the icosahedron's zenith-to-upper and
/// upper-to-upper edges; U is the icosahedral upper pentagon itself.
const POLAR_A: f32 = 31.717_474;
const POLAR_B: f32 = 58.282_526;
const POLAR_U: f32 = 63.434_949;

fn plan_r(polar_deg: f32) -> f32 {
    polar_deg / 90.0 * R_EQUATOR
}

fn pt(r: f32, phi_deg: f32) -> [f32; 2] {
    let a = phi_deg.to_radians();
    [CENTER[0] + r * a.cos(), CENTER[1] + r * a.sin()]
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// The eight 2V faces of sector 0 (phi 0..72), in wiring order: the bottom
/// band walked left to right, then the cap walked back right to left and up
/// to the zenith face. Repeated five ways these tile the 40-face dome.
fn sector_faces() -> Vec<[[f32; 2]; 3]> {
    let z = CENTER;
    let a0 = pt(plan_r(POLAR_A), 0.0);
    let a1 = pt(plan_r(POLAR_A), 72.0);
    let b0 = pt(plan_r(POLAR_B), 36.0);
    let u0 = pt(plan_r(POLAR_U), 0.0);
    let u1 = pt(plan_r(POLAR_U), 72.0);
    let e_m18 = pt(R_EQUATOR, -18.0);
    let e18 = pt(R_EQUATOR, 18.0);
    let e54 = pt(R_EQUATOR, 54.0);
    vec![
        [u0, e_m18, e18], // bottom, straddling the sector boundary
        [u0, b0, e18],
        [b0, e18, e54], // bottom center, base on the equator
        [u1, b0, e54],
        [a1, b0, u1], // cap, right
        [a0, b0, a1], // cap, inverted middle
        [a0, u0, b0], // cap, left
        [z, a0, a1],  // zenith face
    ]
}

fn centroid(t: &[[f32; 2]; 3]) -> [f32; 2] {
    [
        (t[0][0] + t[1][0] + t[2][0]) / 3.0,
        (t[0][1] + t[1][1] + t[2][1]) / 3.0,
    ]
}

/// The suspended panel: its host face shrunk toward the host centroid.
fn panel(host: &[[f32; 2]; 3]) -> [[f32; 2]; 3] {
    let c = centroid(host);
    host.map(|v| {
        [
            round1(c[0] + PANEL_SHRINK * (v[0] - c[0])),
            round1(c[1] + PANEL_SHRINK * (v[1] - c[1])),
        ]
    })
}

/// One sector's lamp path: every panel wrapped corner-to-corner-to-corner
/// and back (the strip enters and leaves a panel at the same corner, like
/// the real pigtails), panels joined by inert connector segments. Returns
/// the path points and the connector segment indices (`gaps`).
fn sector_path() -> (Vec<[f32; 2]>, Vec<u32>) {
    let panels: Vec<[[f32; 2]; 3]> = sector_faces().iter().map(panel).collect();
    let mut points: Vec<[f32; 2]> = Vec::new();
    let mut gaps: Vec<u32> = Vec::new();
    // First entry: the outermost corner (the feed arrives from the ground).
    let mut cursor = *panels[0]
        .iter()
        .max_by(|a, b| dist(**a, CENTER).total_cmp(&dist(**b, CENTER)))
        .expect("panel has corners");
    for (index, corners) in panels.iter().enumerate() {
        let entry = (0..3)
            .min_by(|i, j| dist(corners[*i], cursor).total_cmp(&dist(corners[*j], cursor)))
            .expect("panel has corners");
        if index > 0 {
            // The connector from the previous panel's exit is the segment
            // that ends at this panel's entry point.
            gaps.push(points.len() as u32 - 1);
        }
        for step in 0..=3 {
            points.push(corners[(entry + step) % 3]);
        }
        cursor = corners[entry];
    }
    (points, gaps)
}

/// One door: a chevron — feet on the riser's ground circle spanning the
/// azimuth a 10' chord takes on a 16' dome (~72 degrees), apex projecting
/// inboard where the top of the opening sits on the shell.
fn door_points() -> Vec<[f32; 2]> {
    let (base, half, apex_r) = (300.0, 36.0, 23.0);
    [
        pt(R_GROUND, base - half),
        pt(apex_r, base),
        pt(R_GROUND, base + half),
    ]
    .iter()
    .map(|p| [round1(p[0]), round1(p[1])])
    .collect()
}

fn dome_doc() -> Map2dDoc {
    let (points, gaps) = sector_path();
    let mut doc = Map2dDoc::new();
    doc.canvas = Some([0.0, 0.0, 120.0, 120.0]);
    doc.objects.push(Map2dObject {
        name: "sector".to_string(),
        id: Some(Map2dObjectId::new("sector").expect("valid id")),
        stride: None,
        shape: Map2dShape::Repeat(RepeatShape {
            shape: Box::new(Map2dShape::Path(PathShape {
                points,
                count: SECTOR_LAMPS,
                reversed: false,
                gaps,
                align: PathAlign::On,
            })),
            center: CENTER,
            count: 5,
        }),
    });
    doc.normalize_format();
    doc
}

fn doors_doc() -> Map2dDoc {
    let mut doc = Map2dDoc::new();
    doc.canvas = Some([0.0, 0.0, 120.0, 120.0]);
    doc.objects.push(Map2dObject {
        name: "door".to_string(),
        id: Some(Map2dObjectId::new("door").expect("valid id")),
        // An open chevron has no intrinsic period; three lamps is the
        // meaningful re-seat step the patch rotates by.
        stride: Some(3),
        shape: Map2dShape::Repeat(RepeatShape {
            shape: Box::new(Map2dShape::Path(PathShape {
                points: door_points(),
                count: DOOR_LAMPS,
                reversed: false,
                gaps: Vec::new(),
                align: PathAlign::On,
            })),
            center: CENTER,
            count: 3,
        }),
    });
    doc.normalize_format();
    doc
}

/// Every invariant the example depends on, checked against the real
/// resolver so a tweak here cannot silently break the shipped patch grain.
fn validate(dome: &Map2dDoc, doors: &Map2dDoc) {
    let resolved = resolve(dome).expect("dome resolves");
    assert_eq!(resolved.lamps.len(), 150, "the dome is 150 lamps");
    assert_eq!(resolved.spans.len(), 5, "five sector strands");
    assert!(
        resolved.spans.iter().all(|span| span.count == SECTOR_LAMPS),
        "every sector is one 30-lamp strand"
    );
    for lamp in &resolved.lamps {
        assert!(
            (0.0..=120.0).contains(&lamp.pos[0]) && (0.0..=120.0).contains(&lamp.pos[1]),
            "dome lamp on canvas: {:?}",
            lamp.pos
        );
    }

    let resolved = resolve(doors).expect("doors resolve");
    assert_eq!(resolved.lamps.len(), 27, "the doors are 27 lamps");
    assert_eq!(resolved.spans.len(), 3, "three door strands");
    for lamp in &resolved.lamps {
        assert!(
            (0.0..=120.0).contains(&lamp.pos[0]) && (0.0..=120.0).contains(&lamp.pos[1]),
            "door lamp on canvas: {:?}",
            lamp.pos
        );
    }

    // The door chevron's legs are equal, so the middle lamp sits on the apex.
    let door = door_points();
    let legs = (dist(door[0], door[1]), dist(door[1], door[2]));
    assert!((legs.0 - legs.1).abs() < 0.15, "door legs uneven: {legs:?}");

    // Every panel keeps enough lamps to read as a triangle.
    let (points, gaps) = sector_path();
    let seg_len: Vec<f32> = points.windows(2).map(|w| dist(w[0], w[1])).collect();
    let active: f32 = seg_len
        .iter()
        .enumerate()
        .filter(|(i, _)| !gaps.contains(&(*i as u32)))
        .map(|(_, l)| l)
        .sum();
    let spacing = active / (SECTOR_LAMPS - 1) as f32;
    let mut walked = 0.0;
    let mut lamp = 0;
    for panel_index in 0..8 {
        let perimeter: f32 = (0..3).map(|s| seg_len[panel_index * 4 + s]).sum();
        let boundary = walked + perimeter;
        let mut count = 0;
        while lamp < SECTOR_LAMPS && spacing * lamp as f32 <= boundary + 1e-3 {
            count += 1;
            lamp += 1;
        }
        assert!(count >= 2, "panel {panel_index} holds {count} lamp(s)");
        walked = boundary;
    }
    assert_eq!(lamp, SECTOR_LAMPS, "the walk accounts for every lamp");
}

fn main() {
    let dome = dome_doc();
    let doors = doors_doc();
    validate(&dome, &doors);

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let example = root.join("examples/mini-dome");
    for (path, doc) in [
        (example.join("dome/dome.map2d.json"), &dome),
        (example.join("doors/doors.map2d.json"), &doors),
    ] {
        let json = doc.to_json_pretty() + "\n";
        std::fs::write(&path, &json).expect("write map2d");
        println!("wrote {} ({} bytes)", path.display(), json.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator's invariants hold without touching the filesystem.
    #[test]
    fn generated_documents_validate() {
        validate(&dome_doc(), &doors_doc());
    }

    /// Both documents round-trip through the parser they ship into.
    #[test]
    fn generated_documents_parse_back() {
        for doc in [dome_doc(), doors_doc()] {
            assert_eq!(Map2dDoc::from_json(&doc.to_json()).unwrap(), doc);
        }
    }
}
