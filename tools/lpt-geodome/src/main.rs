//! Generates the small-dome example's map2d, patch, and output documents
//! from the real Small Dome's structure: a 2-frequency geodesic dome (16',
//! 1" EMT struts) hoisted on a riser ring, glowing via 2'-tall triangular
//! lucite panels — **50 of them, 119 LEDs each** — suspended inside the
//! strut triangles, with one chevron door (a big triangle, no bottom edge).
//! See `docs/use-cases/2026-08-09-mini-dome.md` ("The Small Dome, as
//! built") and `docs/use-cases/2026-08-28-three-domes.md`.
//!
//! The output is a **plan view** (azimuthal equidistant: azimuth preserved,
//! radius proportional to polar angle from the zenith), the same projection
//! lp2014's `GeodesicIcosahedralFixture` mapper used. One 72-degree sector
//! carries the dome's five-fold symmetry: its EIGHT 2V faces (5 icosa faces
//! x 4 = 20 cap + 5 + 15 = 40 per dome) plus TWO riser-band faces (the rung
//! below the 2V shell keeps only its downward-pointing triangles — 10 per
//! dome) each hold one panel — the host triangle shrunk toward its centroid
//! at the physical ratio. Each panel is authored as one object (a 5-way
//! repeat of a 119-lamp polygon), so the patch grain is the PANEL
//! (`/band-a/3`), which is the unit the crew actually plugs.
//!
//! The patch documents model an as-built install: each control box feeds
//! half the dome through 13 ports (CNLINKO connectors in miniature, two
//! chained panels per port), with the crew's quirks included — one panel
//! seated reversed, one rotated by a panel side, the door run rotated by a
//! leg and sharing the tail of a box-1 port.
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
    EditorFootprint, EditorMetaDoc, Map2dDoc, Map2dObject, Map2dObjectId, Map2dShape,
    MapObjectPath, PatchDoc, PatchEntry, PatchResolveContext, PatchSource, PathAlign, PathShape,
    PolygonShape, RepeatShape, object_instance_spans, resolve, resolve_patch,
};
use std::path::Path;

/// Doc-space center of the dome's plan view.
const CENTER: [f32; 2] = [60.0, 60.0];
/// Plan radius of the dome's equator ring (polar 90 degrees).
const R_EQUATOR: f32 = 42.0;
/// Plan radius of the riser's ground circle — the equator plus the hoist.
const R_GROUND: f32 = 52.0;
/// Panel inset: each panel is its host strut triangle shrunk toward the
/// host's centroid — the 2' lucite panel suspended inside the ~4.3' face,
/// at the physical ratio.
const PANEL_SHRINK: f32 = 0.46;
/// Lamps per panel: 5 mm WS2812B at 60 LED/m wrapped around the 2' lucite
/// triangle — the real panel's count.
const PANEL_LAMPS: u32 = 119;
/// Authored rotation stride: one panel side. 119 does not divide by 3, so
/// the polygon has no intrinsic per-side stride; 40 lamps is the closest
/// meaningful "re-seat one corner on" step.
const PANEL_STRIDE: u32 = 40;
/// Lamps on the door chevron: two ~10' strut legs of 60 LED/m strip.
const DOOR_LAMPS: u32 = 360;
/// Door rotation stride: one leg — the meaningful re-seat step for an open
/// chevron with no intrinsic period.
const DOOR_STRIDE: u32 = 180;
/// Lamp sample diameter in fixture texture space: panel lamps sit ~0.2 doc
/// units apart, so at the fixtures' render sizes a 1-px footprint keeps
/// neighboring lamps sampling distinct texels instead of one smear.
const SAMPLE_DIAMETER: f32 = 1.0;

/// Panels chained per port (CNLINKO connector): the crew jumpers two
/// adjacent panels onto one feed; the odd panel out gets the last port.
const PANELS_PER_PORT: usize = 2;
/// Output names, matching the two control boxes on the build ("1" and
/// "Box 2" — the sheet labels the boxes 1 and 2).
const OUT_A: &str = "1";
const OUT_B: &str = "Box 2";

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

/// One panel position within sector 0 (phi 0..72): its object identity and
/// its host strut triangle. Repeated five ways these tile the 50-panel dome.
struct PanelSpec {
    name: &'static str,
    id: &'static str,
    host: [[f32; 2]; 3],
}

/// The ten panel positions of sector 0, ground up: the two riser-band
/// faces (the rung below the 2V shell keeps only its downward-pointing
/// triangles), the four bottom-band 2V faces, the three cap faces, and the
/// zenith face.
fn sector_panels() -> Vec<PanelSpec> {
    let z = CENTER;
    let a0 = pt(plan_r(POLAR_A), 0.0);
    let a1 = pt(plan_r(POLAR_A), 72.0);
    let b0 = pt(plan_r(POLAR_B), 36.0);
    let u0 = pt(plan_r(POLAR_U), 0.0);
    let u1 = pt(plan_r(POLAR_U), 72.0);
    let e_m18 = pt(R_EQUATOR, -18.0);
    let e18 = pt(R_EQUATOR, 18.0);
    let e54 = pt(R_EQUATOR, 54.0);
    let g0 = pt(R_GROUND, 0.0);
    let g36 = pt(R_GROUND, 36.0);
    vec![
        // Riser band: base on an equator-decagon edge, apex on the ground
        // circle — the downward-pointing triangles of the riser strip.
        PanelSpec {
            name: "rim a",
            id: "rim-a",
            host: [e_m18, e18, g0],
        },
        PanelSpec {
            name: "rim b",
            id: "rim-b",
            host: [e18, e54, g36],
        },
        // 2V bottom band, walked left to right.
        PanelSpec {
            name: "band a",
            id: "band-a",
            host: [u0, e_m18, e18], // straddling the sector boundary
        },
        PanelSpec {
            name: "band b",
            id: "band-b",
            host: [u0, b0, e18],
        },
        PanelSpec {
            name: "band c",
            id: "band-c",
            host: [b0, e18, e54], // base on the equator
        },
        PanelSpec {
            name: "band d",
            id: "band-d",
            host: [u1, b0, e54],
        },
        // Cap.
        PanelSpec {
            name: "cap a",
            id: "cap-a",
            host: [a1, b0, u1], // right
        },
        PanelSpec {
            name: "cap b",
            id: "cap-b",
            host: [a0, b0, a1], // inverted middle
        },
        PanelSpec {
            name: "cap c",
            id: "cap-c",
            host: [a0, u0, b0], // left
        },
        // Zenith face.
        PanelSpec {
            name: "zenith",
            id: "zenith",
            host: [z, a0, a1],
        },
    ]
}

fn centroid(t: &[[f32; 2]; 3]) -> [f32; 2] {
    [
        (t[0][0] + t[1][0] + t[2][0]) / 3.0,
        (t[0][1] + t[1][1] + t[2][1]) / 3.0,
    ]
}

/// The suspended panel: its host face shrunk toward the host centroid,
/// point list rotated so lamp 0 sits on the outermost corner (the strip's
/// feed arrives from below), host winding kept.
fn panel_polygon(host: &[[f32; 2]; 3]) -> Vec<[f32; 2]> {
    let c = centroid(host);
    let shrunk: Vec<[f32; 2]> = host
        .iter()
        .map(|v| {
            [
                round1(c[0] + PANEL_SHRINK * (v[0] - c[0])),
                round1(c[1] + PANEL_SHRINK * (v[1] - c[1])),
            ]
        })
        .collect();
    let entry = (0..3)
        .max_by(|i, j| {
            dist(shrunk[*i], CENTER)
                .total_cmp(&dist(shrunk[*j], CENTER))
                .then(i.cmp(j))
        })
        .expect("panel has corners");
    (0..3).map(|step| shrunk[(entry + step) % 3]).collect()
}

/// The door: a chevron — feet on the riser's ground circle spanning the
/// azimuth a 10' chord takes on a 16' dome (~72 degrees), apex projecting
/// inboard where the top of the opening sits on the shell. Centered at the
/// bottom of the plan (phi 90, y-down), where the build puts it.
fn door_points() -> Vec<[f32; 2]> {
    let (base, half, apex_r) = (90.0, 36.0, 23.0);
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
    let mut doc = Map2dDoc::new();
    doc.sample_diameter = SAMPLE_DIAMETER;
    doc.canvas = Some([0.0, 0.0, 120.0, 120.0]);
    for spec in sector_panels() {
        doc.objects.push(Map2dObject {
            name: spec.name.to_string(),
            id: Some(Map2dObjectId::new(spec.id).expect("valid id")),
            stride: Some(PANEL_STRIDE),
            shape: Map2dShape::Repeat(RepeatShape {
                shape: Box::new(Map2dShape::Polygon(PolygonShape {
                    points: panel_polygon(&spec.host),
                    count: PANEL_LAMPS,
                    align: PathAlign::Inside,
                })),
                center: CENTER,
                count: 5,
            }),
        });
    }
    doc.normalize_format();
    doc
}

fn doors_doc() -> Map2dDoc {
    let mut doc = Map2dDoc::new();
    doc.sample_diameter = SAMPLE_DIAMETER;
    doc.canvas = Some([0.0, 0.0, 120.0, 120.0]);
    doc.objects.push(Map2dObject {
        name: "door".to_string(),
        id: Some(Map2dObjectId::new("door").expect("valid id")),
        // An open chevron has no intrinsic period; one leg is the
        // meaningful re-seat step the patch rotates by.
        stride: Some(DOOR_STRIDE),
        shape: Map2dShape::Path(PathShape {
            points: door_points(),
            count: DOOR_LAMPS,
            reversed: false,
            gaps: Vec::new(),
            align: PathAlign::Inside,
        }),
    });
    doc.normalize_format();
    doc
}

/// One panel instance in the as-built plan: which object path it is and
/// where its plan-view centroid sits.
struct PanelInstance {
    path: MapObjectPath,
    centroid: [f32; 2],
}

/// Every panel instance with its centroid, derived from the RESOLVED
/// document (so the repeat's rotation convention cannot be second-guessed
/// here): span k of object i is instance path `/<id>/<k>`.
fn panel_instances(dome: &Map2dDoc) -> Vec<PanelInstance> {
    let resolved = resolve(dome).expect("dome resolves");
    let spans = object_instance_spans(dome, &resolved);
    spans
        .iter()
        .map(|span| {
            let lamps = &resolved.lamps[span.start as usize..(span.start + span.count) as usize];
            let sum = lamps.iter().fold([0.0f32, 0.0], |acc, lamp| {
                [acc[0] + lamp.pos[0], acc[1] + lamp.pos[1]]
            });
            let count = lamps.len() as f32;
            PanelInstance {
                path: MapObjectPath {
                    id: span.id.clone().expect("panel objects carry ids"),
                    instances: span.instances.clone(),
                },
                centroid: [sum[0] / count, sum[1] / count],
            }
        })
        .collect()
}

/// The as-built wiring plan: box 1 takes the right half of the plan view,
/// box 2 the left (the sheet's purple/green halves), each box's panels
/// walked by azimuth from its own side, ground ring first at equal azimuth.
/// Returns the two boxes' panel lists in wire order.
fn plan_boxes(instances: &[PanelInstance]) -> (Vec<usize>, Vec<usize>) {
    let mut order: Vec<usize> = (0..instances.len()).collect();
    // Split: the 25 panels furthest right (largest x) belong to box 1.
    order.sort_by(|a, b| {
        instances[*b].centroid[0]
            .total_cmp(&instances[*a].centroid[0])
            .then(a.cmp(b))
    });
    let (right, left) = order.split_at(instances.len() / 2);

    // Wire order within a box: sweep by azimuth away from the box's own
    // side (box 1 sits at phi 0, box 2 at phi 180), inner rings after
    // outer at equal sweep so chains run ground-up.
    let sweep = |list: &[usize], box_phi: f32| -> Vec<usize> {
        let mut sorted = list.to_vec();
        sorted.sort_by(|a, b| {
            let key = |index: usize| {
                let c = instances[index].centroid;
                let phi = (c[1] - CENTER[1]).atan2(c[0] - CENTER[0]).to_degrees();
                let mut delta = phi - box_phi;
                while delta < -180.0 {
                    delta += 360.0;
                }
                while delta >= 180.0 {
                    delta -= 360.0;
                }
                (delta.abs(), -dist(c, CENTER))
            };
            let (ka, kb) = (key(*a), key(*b));
            ka.0.total_cmp(&kb.0)
                .then(ka.1.total_cmp(&kb.1))
                .then(a.cmp(b))
        });
        sorted
    };
    (sweep(right, 0.0), sweep(left, 180.0))
}

/// One output's port table: `(endpoint token, lamp count)` per port.
fn port_table(prefix: char, panels: usize, door_tail: bool) -> Vec<(String, u32)> {
    let full_ports = panels / PANELS_PER_PORT;
    let mut ports: Vec<(String, u32)> = (0..full_ports)
        .map(|port| {
            (
                format!("{prefix}{:02}", port + 1),
                PANEL_LAMPS * PANELS_PER_PORT as u32,
            )
        })
        .collect();
    let mut tail = (panels % PANELS_PER_PORT) as u32 * PANEL_LAMPS;
    if door_tail {
        tail += DOOR_LAMPS;
    }
    if tail > 0 {
        ports.push((format!("{prefix}{:02}", full_ports + 1), tail));
    }
    ports
}

/// The dome patch: every panel assigned to its box and port anchor, with
/// the as-built quirks — box 2's sixth panel seated reversed, box 1's
/// tenth rotated one panel side.
fn dome_patch(instances: &[PanelInstance], box_a: &[usize], box_b: &[usize]) -> PatchDoc {
    let mut doc = PatchDoc::new();
    let mut push = |output: &str, wire: &[usize], quirk_reversed: usize, quirk_rotated: usize| {
        for (position, index) in wire.iter().enumerate() {
            let port = position / PANELS_PER_PORT;
            let lamp = (port * PANELS_PER_PORT * PANEL_LAMPS as usize
                + (position % PANELS_PER_PORT) * PANEL_LAMPS as usize)
                as u32;
            doc.entries.push(PatchEntry {
                source: PatchSource::Path {
                    path: instances[*index].path.clone(),
                    range: None,
                },
                output: Some(output.to_string()),
                lamp,
                reversed: position == quirk_reversed,
                offset: if position == quirk_rotated {
                    PANEL_STRIDE
                } else {
                    0
                },
            });
        }
    };
    // usize::MAX = no quirk at that position in this box.
    push(OUT_A, box_a, usize::MAX, 9);
    push(OUT_B, box_b, 5, usize::MAX);
    doc.normalize_format();
    doc
}

/// The doors patch: the door run shares the tail of box 1's last port,
/// plugged with its legs swapped (rotated by one leg).
fn doors_patch(box_a_panels: usize) -> PatchDoc {
    let mut doc = PatchDoc::new();
    let full_ports = box_a_panels / PANELS_PER_PORT;
    let tail_start = (full_ports * PANELS_PER_PORT + box_a_panels % PANELS_PER_PORT) as u32;
    doc.entries.push(PatchEntry {
        source: PatchSource::Path {
            path: MapObjectPath {
                id: Map2dObjectId::new("door").expect("valid id"),
                instances: Vec::new(),
            },
            range: None,
        },
        output: Some(OUT_A.to_string()),
        lamp: tail_start * PANEL_LAMPS,
        reversed: false,
        offset: DOOR_STRIDE,
    });
    doc.normalize_format();
    doc
}

/// The Arrange placements (`editor.json`): both fixtures at IDENTITY in the
/// shared 120-unit plan, so the door renders nestled among the panels at its
/// authored bottom-center spot instead of the unarranged side-by-side tiling.
///
/// Keys are the fixtures' full studio node addresses. The root segment
/// derives from the seeded project directory (`examples-small-dome` →
/// `/examples_small_dome.show`), so the shipped keys match the gallery
/// copy; a renamed user copy degrades benignly to "unarranged".
fn editor_doc(dome: &Map2dDoc, doors: &Map2dDoc) -> EditorMetaDoc {
    let mut doc = EditorMetaDoc::new();
    for (key, map2d) in [
        ("/examples_small_dome.show/dome.module/dome.fixture", dome),
        (
            "/examples_small_dome.show/doors.module/doors.fixture",
            doors,
        ),
    ] {
        let resolved = resolve(map2d).expect("map2d resolves");
        let (mut min, mut max) = ([f32::MAX, f32::MAX], [f32::MIN, f32::MIN]);
        for lamp in &resolved.lamps {
            for axis in 0..2 {
                min[axis] = min[axis].min(lamp.pos[axis]);
                max[axis] = max[axis].max(lamp.pos[axis]);
            }
        }
        let surface = doc.mapping_surface_mut(key);
        // Transform stays identity (the writer omits it); the footprint
        // cache lets an unloaded fixture render as an honest block. Values
        // pre-rounded to the writer's four-decimal grid so the document
        // round-trips byte-for-byte.
        let grid4 = |v: f32| (f64::from(v) * 10_000.0).round() / 10_000.0;
        surface.footprint = Some(EditorFootprint {
            bbox: [
                grid4(min[0]),
                grid4(min[1]),
                grid4(max[0] - min[0]),
                grid4(max[1] - min[1]),
            ],
            lamps: resolved.lamps.len() as u32,
        });
    }
    doc
}

/// An Output node artifact: the box's port table plus the standard bus
/// binding, formatted the way the studio serializer writes node files.
fn output_json(name: &str, ports: &[(String, u32)]) -> String {
    let mut text = String::from("{\n  \"kind\": \"Output\",\n");
    text.push_str(&format!("  \"name\": {name:?},\n"));
    text.push_str("  \"ports\": {\n");
    for (index, (token, count)) in ports.iter().enumerate() {
        text.push_str(&format!(
            "    \"{index}\": {{\n      \"endpoint\": \"ws281x:local:{token}\",\n      \"count\": {count}\n    }}{}\n",
            if index + 1 < ports.len() { "," } else { "" }
        ));
    }
    text.push_str(
        "  },\n  \"bindings\": {\n    \"input\": {\n      \"source\": \"bus:control.out\"\n    }\n  }\n}\n",
    );
    text
}

/// Every invariant the example depends on, checked against the real
/// resolvers so a tweak here cannot silently break the shipped patch grain.
fn validate(
    dome: &Map2dDoc,
    doors: &Map2dDoc,
    dome_patch: &PatchDoc,
    doors_patch: &PatchDoc,
    ports_a: &[(String, u32)],
    ports_b: &[(String, u32)],
) {
    // Inside alignment (lamps line the panel/chevron form) stamps format 4;
    // assert it directly so a generator edit that drops back to `On`
    // (silently releasing the format) fails loudly here.
    assert_eq!(dome.format, 4, "inside-aligned dome stamps format 4");
    assert_eq!(doors.format, 4, "inside-aligned doors stamp format 4");
    assert_eq!(dome.objects.len(), 10, "ten panel positions per sector");

    let resolved = resolve(dome).expect("dome resolves");
    assert_eq!(resolved.lamps.len(), 50 * PANEL_LAMPS as usize, "50 panels");
    assert_eq!(resolved.spans.len(), 50, "every panel is its own strand");
    assert!(
        resolved.spans.iter().all(|span| span.count == PANEL_LAMPS),
        "every panel is one 119-lamp strand"
    );
    for lamp in &resolved.lamps {
        assert!(
            (0.0..=120.0).contains(&lamp.pos[0]) && (0.0..=120.0).contains(&lamp.pos[1]),
            "dome lamp on canvas: {:?}",
            lamp.pos
        );
    }
    // Panel lamp pitch stays in the band where the wrap reads as a strip
    // (the plan projection stretches zenith panels and squeezes rim ones).
    for object in &dome.objects {
        let Map2dShape::Repeat(repeat) = &object.shape else {
            panic!("panel object {:?} is not a repeat", object.name);
        };
        let Map2dShape::Polygon(polygon) = repeat.shape.as_ref() else {
            panic!("panel object {:?} is not a polygon", object.name);
        };
        let mut closed = polygon.points.clone();
        closed.push(polygon.points[0]);
        let perimeter: f32 = closed.windows(2).map(|w| dist(w[0], w[1])).sum();
        let pitch = perimeter / PANEL_LAMPS as f32;
        assert!(
            (0.1..=0.5).contains(&pitch),
            "panel {:?} pitch out of band: {pitch}",
            object.name
        );
    }

    let resolved_doors = resolve(doors).expect("doors resolve");
    assert_eq!(
        resolved_doors.lamps.len(),
        DOOR_LAMPS as usize,
        "one 360-lamp door"
    );
    assert_eq!(resolved_doors.spans.len(), 1, "one door strand");
    for lamp in &resolved_doors.lamps {
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

    // The patches resolve without refusals through the real resolver, and
    // every run lands inside exactly one port of its box.
    let outputs = [OUT_A.to_string(), OUT_B.to_string()];
    let windows = |patch: &PatchDoc, doc: &Map2dDoc, resolved| {
        let spans = object_instance_spans(doc, resolved);
        let ctx = PatchResolveContext {
            fixture_lamp_count: spans.iter().map(|span| span.count).sum(),
            object_spans: &spans,
            allowed_outputs: Some(&outputs),
            default_output: None,
        };
        let resolution = resolve_patch(&ctx, patch).expect("patch resolves");
        assert!(
            resolution.refusals.is_empty(),
            "patch refusals: {:?}",
            resolution.refusals
        );
        resolution
    };
    let dome_resolution = windows(dome_patch, dome, &resolved);
    let doors_resolution = windows(doors_patch, doors, &resolved_doors);

    let port_bounds = |ports: &[(String, u32)]| -> Vec<(u32, u32)> {
        let mut start = 0;
        ports
            .iter()
            .map(|(_, count)| {
                let bounds = (start, start + count);
                start += count;
                bounds
            })
            .collect()
    };
    let bounds = [port_bounds(ports_a), port_bounds(ports_b)];
    let mut claimed: [Vec<(u32, u32)>; 2] = [Vec::new(), Vec::new()];
    for range in dome_resolution
        .ranges
        .iter()
        .chain(doors_resolution.ranges.iter())
    {
        let box_index = match range.output.as_deref() {
            Some(OUT_A) => 0,
            Some(OUT_B) => 1,
            other => panic!("unexpected output: {other:?}"),
        };
        let window = (range.lamp, range.lamp_end());
        assert!(
            bounds[box_index]
                .iter()
                .any(|(start, end)| window.0 >= *start && window.1 <= *end),
            "run {window:?} does not sit inside one box-{} port",
            box_index + 1
        );
        for other in &claimed[box_index] {
            assert!(
                window.1 <= other.0 || window.0 >= other.1,
                "run {window:?} overlaps {other:?} on box {}",
                box_index + 1
            );
        }
        claimed[box_index].push(window);
    }
    // Full occupancy: every port lamp is claimed (no dark wire).
    for (box_index, claims) in claimed.iter().enumerate() {
        let claimed_total: u32 = claims.iter().map(|(start, end)| end - start).sum();
        let capacity: u32 = bounds[box_index].last().map(|(_, end)| *end).unwrap_or(0);
        assert_eq!(
            claimed_total,
            capacity,
            "box {} wire not fully claimed",
            box_index + 1
        );
    }
}

fn main() {
    let dome = dome_doc();
    let doors = doors_doc();
    let instances = panel_instances(&dome);
    let (box_a, box_b) = plan_boxes(&instances);
    let dome_patch_doc = dome_patch(&instances, &box_a, &box_b);
    let doors_patch_doc = doors_patch(box_a.len());
    let ports_a = port_table('A', box_a.len(), true);
    let ports_b = port_table('B', box_b.len(), false);
    let editor = editor_doc(&dome, &doors);
    validate(
        &dome,
        &doors,
        &dome_patch_doc,
        &doors_patch_doc,
        &ports_a,
        &ports_b,
    );
    assert_eq!(
        EditorMetaDoc::from_json(&editor.to_json_pretty()).expect("editor doc parses"),
        editor,
        "editor.json round-trips"
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let example = root.join("examples/small-dome");
    for (path, json) in [
        (
            example.join("dome/dome.map2d.json"),
            dome.to_json_pretty() + "\n",
        ),
        (
            example.join("doors/doors.map2d.json"),
            doors.to_json_pretty() + "\n",
        ),
        (
            example.join("dome/dome.patch.json"),
            dome_patch_doc.to_json_pretty() + "\n",
        ),
        (
            example.join("doors/doors.patch.json"),
            doors_patch_doc.to_json_pretty() + "\n",
        ),
        (example.join("editor.json"), editor.to_json_pretty() + "\n"),
        (example.join("out_a.json"), output_json(OUT_A, &ports_a)),
        (example.join("out_b.json"), output_json(OUT_B, &ports_b)),
    ] {
        std::fs::write(&path, &json).expect("write artifact");
        println!("wrote {} ({} bytes)", path.display(), json.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator's invariants hold without touching the filesystem.
    #[test]
    fn generated_documents_validate() {
        let dome = dome_doc();
        let doors = doors_doc();
        let instances = panel_instances(&dome);
        let (box_a, box_b) = plan_boxes(&instances);
        validate(
            &dome,
            &doors,
            &dome_patch(&instances, &box_a, &box_b),
            &doors_patch(box_a.len()),
            &port_table('A', box_a.len(), true),
            &port_table('B', box_b.len(), false),
        );
    }

    /// Both mapping documents round-trip through the parser they ship into.
    #[test]
    fn generated_documents_parse_back() {
        for doc in [dome_doc(), doors_doc()] {
            assert_eq!(Map2dDoc::from_json(&doc.to_json()).unwrap(), doc);
        }
    }

    /// The Arrange placements round-trip and carry both fixtures' entries.
    #[test]
    fn generated_editor_doc_parses_back() {
        let doc = editor_doc(&dome_doc(), &doors_doc());
        let parsed = EditorMetaDoc::from_json(&doc.to_json_pretty()).unwrap();
        assert_eq!(parsed, doc);
        assert_eq!(parsed.nodes.len(), 2, "both fixtures arranged");
    }

    /// Both patch documents round-trip through the parser they ship into.
    #[test]
    fn generated_patches_parse_back() {
        let dome = dome_doc();
        let instances = panel_instances(&dome);
        let (box_a, box_b) = plan_boxes(&instances);
        for doc in [
            dome_patch(&instances, &box_a, &box_b),
            doors_patch(box_a.len()),
        ] {
            assert_eq!(PatchDoc::from_json(&doc.to_json()).unwrap(), doc);
        }
    }
}
