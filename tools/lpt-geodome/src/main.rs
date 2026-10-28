//! Generates the small-dome example's map2d documents (format 6: each
//! fixture's layout and patching in one file), its output documents and its
//! Arrange placements from the real Small Dome's structure: a 2-frequency
//! geodesic dome (16', 1" EMT struts) hoisted on a riser ring, glowing via
//! 2'-tall triangular lucite panels — **50 of them, 119 LEDs each** —
//! suspended inside the strut triangles, with one chevron door (a big
//! triangle, no bottom edge). See `docs/use-cases/2026-08-09-mini-dome.md`
//! ("The Small Dome, as built") and `docs/use-cases/2026-08-28-three-domes.md`.
//!
//! The output is a **plan view** (azimuthal equidistant: azimuth preserved,
//! radius proportional to polar angle from the zenith), the same projection
//! lp2014's `GeodesicIcosahedralFixture` mapper used. One 72-degree sector
//! carries the dome's five-fold symmetry: its EIGHT 2V faces (5 icosa faces
//! x 4 = 20 cap + 5 + 15 = 40 per dome) plus TWO riser-band faces (the rung
//! below the 2V shell keeps only its downward-pointing triangles — 10 per
//! dome) each hold one panel — the host triangle shrunk toward its centroid
//! at the physical ratio. Each panel position is one part (a 5-way repeat of
//! a 119-lamp polygon), so each copy is its own segment (`band-a@3`), the
//! unit the crew actually plugs.
//!
//! The patching models an as-built install: each control box feeds half the
//! dome through 13 ports (CNLINKO connectors in miniature, two chained
//! panels per port), with the crew's quirks included — one panel seated
//! reversed, one rotated by a panel side, the door run rotated by a leg and
//! sharing the tail of a box-1 port. A fixture's patching is its start plus
//! an override on every segment that does not simply follow on.
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

use lpc_mapping::part::{self, Mapping, Part, PartId, PartKind, Repeat, Shape, TreeId};
use lpc_mapping::patching::{
    self, Amount, Direction, OutputOp, OutputPorts, Override, PortOp, Start,
};
use lpc_mapping::{
    EditorFootprint, EditorMetaDoc, Map2dDoc, Map2dObject, Map2dObjectId, Map2dShape, PathAlign,
    PathShape, PolygonShape, RepeatShape, map2d6, object_instance_spans, resolve,
};
use std::collections::BTreeMap;
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
/// The rotated panel's offset: one panel side. 119 does not divide by 3, so
/// the polygon has no intrinsic per-side step; 40 lamps is the closest
/// meaningful "re-seat one corner on" step.
const PANEL_STRIDE: u32 = 40;
/// Lamps on the door chevron: two ~10' strut legs of 60 LED/m strip.
const DOOR_LAMPS: u32 = 360;
/// The door's offset: one leg — the meaningful re-seat step for an open
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
/// Channels per lamp: the panels and the door are RGB strip.
const CHANNELS_PER_LAMP: u32 = 3;

/// 2V vertex rings as polar angles from the zenith, degrees. A/B are the
/// sphere-projected midpoints of the icosahedron's zenith-to-upper and
/// upper-to-upper edges; U is the icosahedral upper pentagon itself.
const POLAR_A: f32 = 31.717_474;
const POLAR_B: f32 = 58.282_526;
const POLAR_U: f32 = 63.434_949;

fn main() {
    let generated = Generated::new();
    generated.validate();

    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
        .join(EXAMPLE);
    for (relative, text) in generated.files() {
        let path = example.join(relative);
        std::fs::write(&path, &text).expect("write artifact");
        println!("wrote {} ({} bytes)", path.display(), text.len());
    }
}

/// The example's directory, from the workspace root.
const EXAMPLE: &str = "catalog/projects/small-dome";

/// Everything the generator derives, before it is written.
struct Generated {
    /// The dome's layout in today's resolver's form: the geometry the
    /// wiring plan and the Arrange footprints are measured on.
    dome: Map2dDoc,
    doors: Map2dDoc,
    /// The documents written: layout plus patching.
    dome_mapping: Mapping,
    doors_mapping: Mapping,
    /// Where every segment of both fixtures was plugged.
    landings: BTreeMap<TreeId, Landing>,
    ports_a: Vec<(String, u32)>,
    ports_b: Vec<(String, u32)>,
    editor: EditorMetaDoc,
}

impl Generated {
    fn new() -> Self {
        let dome = dome_doc();
        let doors = doors_doc();
        let instances = panel_instances(&dome);
        let (box_a, box_b) = plan_boxes(&instances);
        let ports_a = port_table('A', box_a.len(), true);
        let ports_b = port_table('B', box_b.len(), false);
        let mut landings = dome_landings(&instances, &box_a, &box_b);
        landings.insert(door_id(), door_landing(box_a.len()));
        let outputs = [(OUT_A, ports_a.as_slice()), (OUT_B, ports_b.as_slice())];
        let dome_mapping = fixture_mapping(&dome, &landings, &outputs);
        let doors_mapping = fixture_mapping(&doors, &landings, &outputs);
        let editor = editor_doc(&dome, &doors);
        Self {
            dome,
            doors,
            dome_mapping,
            doors_mapping,
            landings,
            ports_a,
            ports_b,
            editor,
        }
    }

    /// Every file written, by its path inside the example, with its text.
    fn files(&self) -> Vec<(&'static str, String)> {
        let write = |mapping: &Mapping| map2d6::write(mapping).expect("mapping writes");
        vec![
            ("dome/dome.map2d.json", write(&self.dome_mapping)),
            ("doors/doors.map2d.json", write(&self.doors_mapping)),
            ("editor.json", self.editor.to_json_pretty() + "\n"),
            ("out_a.json", output_json(OUT_A, &self.ports_a)),
            ("out_b.json", output_json(OUT_B, &self.ports_b)),
        ]
    }

    /// Every invariant the example depends on, checked against the real
    /// resolver and patcher so a tweak here cannot silently break the
    /// shipped panel grain or the wiring.
    fn validate(&self) {
        validate_layout(&self.dome, &self.doors);
        self.validate_patching();
        assert_eq!(
            EditorMetaDoc::from_json(&self.editor.to_json_pretty()).expect("editor doc parses"),
            self.editor,
            "editor.json round-trips"
        );
    }

    /// Both fixtures patch with no problems, every segment where the crew
    /// plugged it, every run inside one port of its box, no two runs
    /// overlapping (the dome and the door share box 1's last port), and
    /// every port lamp claimed.
    fn validate_patching(&self) {
        let outputs = [
            output_ports(OUT_A, &self.ports_a),
            output_ports(OUT_B, &self.ports_b),
        ];
        let mut claimed: BTreeMap<(String, u32), Vec<(u32, u32)>> = BTreeMap::new();
        for mapping in [&self.dome_mapping, &self.doors_mapping] {
            let tree = part::render(mapping).expect("mapping renders");
            let patched = patching::patch_mapping(mapping, &tree, &outputs);
            assert!(
                patched.problems.is_empty(),
                "patching problems: {:?}",
                patched.problems
            );
            assert_eq!(
                patched.segments.len(),
                tree.segments().len(),
                "every segment lands"
            );
            for segment in &patched.segments {
                let landing = &self.landings[&segment.id];
                assert_eq!(
                    (
                        segment.address.output.as_str(),
                        segment.address.port.0,
                        segment.address.channel,
                        segment.direction.is_reverse(),
                        segment.offset,
                    ),
                    (
                        landing.output,
                        landing.port,
                        landing.lamp * CHANNELS_PER_LAMP,
                        landing.reversed,
                        landing.rotation,
                    ),
                    "{} lands where the crew plugged it",
                    segment.id
                );
                assert_eq!(segment.limit, None, "{} fits its port", segment.id);
                let window = (
                    segment.address.channel,
                    segment.address.channel + segment.channels(),
                );
                let capacity = port_capacity(&outputs, landing.output, landing.port);
                assert!(
                    window.1 <= capacity,
                    "run {window:?} does not sit inside port {} of box {:?}",
                    landing.port,
                    landing.output
                );
                let runs = claimed
                    .entry((landing.output.to_string(), landing.port))
                    .or_default();
                for other in runs.iter() {
                    assert!(
                        window.1 <= other.0 || window.0 >= other.1,
                        "run {window:?} overlaps {other:?} on port {} of box {:?}",
                        landing.port,
                        landing.output
                    );
                }
                runs.push(window);
            }
        }
        // Full occupancy: every port lamp is claimed (no dark wire).
        for output in &outputs {
            for port in &output.ports {
                let key = (output.output.as_str().to_string(), port.key.0);
                let total: u32 = claimed
                    .get(&key)
                    .map(|runs| runs.iter().map(|(start, end)| end - start).sum())
                    .unwrap_or(0);
                assert_eq!(
                    Some(total),
                    port.channels,
                    "port {} of box {:?} not fully claimed",
                    key.1,
                    key.0
                );
            }
        }
    }
}

fn plan_r(polar_deg: f32) -> f32 {
    polar_deg / 90.0 * R_EQUATOR
}

// The generator's trigonometry is `libm`'s, never std's: std's `sin`,
// `atan2` and `hypot` are the platform's C library, which differs by an ulp
// between macOS and glibc, and the committed catalog must be what this
// writes on every host CI runs (see `plan_boxes`).

fn pt(r: f32, phi_deg: f32) -> [f32; 2] {
    let a = phi_deg.to_radians();
    [CENTER[0] + r * libm::cosf(a), CENTER[1] + r * libm::sinf(a)]
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    libm::hypotf(a[0] - b[0], a[1] - b[1])
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
            stride: None,
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
        stride: None,
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

/// A fixture's format-6 mapping: its layout's parts (each repeated object a
/// minted `repeat<n>` holding the object's own part, id and name kept) and
/// its patching, frozen from where the crew plugged each segment.
fn fixture_mapping(
    doc: &Map2dDoc,
    landings: &BTreeMap<TreeId, Landing>,
    outputs: &[(&str, &[(String, u32)])],
) -> Mapping {
    let mut mapping = Mapping::new();
    mapping.sample_diameter = doc.sample_diameter;
    mapping.canvas = doc.canvas;
    for object in &doc.objects {
        let id = object.id.as_ref().expect("every object carries an id");
        let part = match &object.shape {
            Map2dShape::Repeat(repeat) => {
                let held = shape_part(id, &object.name, &repeat.shape);
                mapping.new_part(PartKind::Repeat(Repeat::new(
                    held,
                    repeat.center,
                    repeat.count,
                )))
            }
            shape => shape_part(id, &object.name, shape),
        };
        mapping.parts.push(part);
    }

    let tree = part::render(&mapping).expect("mapping renders");
    let segments: Vec<(TreeId, u32)> = tree
        .segments()
        .iter()
        .map(|segment| (segment.id.clone(), segment.count()))
        .collect();
    let (start, overrides) = freeze_patching(&segments, landings, outputs);
    mapping.start = start;
    mapping.overrides = overrides;
    mapping
}

/// One drawn object as a part with the object's id and name.
fn shape_part(id: &Map2dObjectId, name: &str, shape: &Map2dShape) -> Part {
    let shape = match shape {
        Map2dShape::Polygon(polygon) => Shape::Polygon(polygon.clone()),
        Map2dShape::Path(path) => {
            assert!(path.gaps.is_empty(), "the generator draws no jumpers");
            let mut drawn = part::Path::through(&path.points, path.count);
            drawn.reversed = path.reversed;
            drawn.align = path.align;
            Shape::Path(drawn)
        }
        other => panic!("the generator draws no {other:?}"),
    };
    let mut part = Part::new(
        PartId::new(id.as_str()).expect("object id is a part id"),
        PartKind::Shape(shape),
    );
    part.name = name.to_string();
    part
}

/// One panel instance in the as-built plan: which segment it is and where
/// its plan-view centroid sits.
struct PanelInstance {
    id: TreeId,
    centroid: [f32; 2],
}

/// Every panel instance with its centroid, derived from the RESOLVED
/// document (so the repeat's rotation convention cannot be second-guessed
/// here): span k of object `<id>` is segment `<id>@k`.
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
            let object = span.id.as_ref().expect("panel objects carry ids");
            // Instances are outermost first; a tree id's copies innermost.
            let id = span.instances.iter().rev().fold(
                TreeId::of(&PartId::new(object.as_str()).expect("object id is a part id")),
                |id, &copy| id.in_copy(copy),
            );
            PanelInstance {
                id: id.canonical(),
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
    // outer at equal sweep so chains run ground-up, and of two panels at
    // the same sweep and ring, the one on the low-y half of the plan first.
    //
    // The five-fold copies put panels at mirrored azimuths (rim a's copies 1
    // and 4 at +-72 degrees from box 1), whose computed sweeps differ only
    // in their last bit or two. Compared raw, that last bit chose the wire
    // order, and it was the C library's: macOS and glibc ordered such a pair
    // differently, so the catalog written on a Mac failed CI on Linux. So
    // two sweeps (or rings) closer than `SWEEP_SAME` (`RING_SAME`) are the
    // same, a tolerance far above that noise and below any real difference
    // between two panels (band a's copies sit 4e-4 degrees off their
    // mirror), the tie is broken by a stated rule, and the trigonometry is
    // `libm`'s, the same on every host. `sweep_keys` refuses a pair in the
    // band between, where the tolerance itself would be the coin toss.
    let sweep = |list: &[usize], box_phi: f32| -> Vec<usize> {
        let keys = sweep_keys(instances, list, box_phi);
        let mut sorted: Vec<usize> = (0..list.len()).collect();
        sorted.sort_by(|&a, &b| {
            let (ka, kb) = (keys[a], keys[b]);
            let by = |x: f32, y: f32, same: f32| {
                if (x - y).abs() < same {
                    core::cmp::Ordering::Equal
                } else {
                    x.total_cmp(&y)
                }
            };
            by(ka.sweep, kb.sweep, SWEEP_SAME)
                .then(by(kb.ring, ka.ring, RING_SAME))
                .then(ka.high_y.cmp(&kb.high_y))
                .then(list[a].cmp(&list[b]))
        });
        sorted.into_iter().map(|position| list[position]).collect()
    };
    (sweep(right, 0.0), sweep(left, 180.0))
}

/// Two panels' sweeps (degrees) closer than this are the same sweep.
const SWEEP_SAME: f32 = 1e-4;
/// Two panels' rings (canvas units from the center) closer than this are
/// the same ring.
const RING_SAME: f32 = 1e-3;

/// A panel's place in its box's sweep.
#[derive(Clone, Copy)]
struct SweepKey {
    /// Degrees from the box's own side, either way.
    sweep: f32,
    /// Distance from the plan's center (outer first).
    ring: f32,
    /// On the high-y half of the plan (the low-y half first).
    high_y: bool,
}

/// The sweep keys of `list`'s panels, refusing any two whose sweeps or rings
/// are neither clearly the same (within a third of the tolerance) nor
/// clearly apart (beyond twice it).
fn sweep_keys(instances: &[PanelInstance], list: &[usize], box_phi: f32) -> Vec<SweepKey> {
    let keys: Vec<SweepKey> = list
        .iter()
        .map(|&index| {
            let c = instances[index].centroid;
            let phi = libm::atan2f(c[1] - CENTER[1], c[0] - CENTER[0]).to_degrees();
            let mut delta = phi - box_phi;
            while delta < -180.0 {
                delta += 360.0;
            }
            while delta >= 180.0 {
                delta -= 360.0;
            }
            SweepKey {
                sweep: delta.abs(),
                ring: dist(c, CENTER),
                high_y: c[1] >= CENTER[1],
            }
        })
        .collect();
    for a in &keys {
        for b in &keys {
            for (x, y, same) in [(a.sweep, b.sweep, SWEEP_SAME), (a.ring, b.ring, RING_SAME)] {
                let gap = (x - y).abs();
                assert!(
                    gap < same / 3.0 || gap > same * 2.0,
                    "two panels {gap} apart are neither the same place nor clearly apart"
                );
            }
        }
    }
    keys
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

/// Where the crew plugged one segment: its output, the port (its index in
/// the output's port table), the lamp its run starts at on that port, and
/// how it is seated.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Landing {
    output: &'static str,
    port: u32,
    lamp: u32,
    reversed: bool,
    /// In lamps; 0 is none.
    rotation: u32,
}

/// Every panel plugged into its box and port, with the as-built quirks —
/// box 2's sixth panel seated reversed, box 1's tenth rotated one panel
/// side.
fn dome_landings(
    instances: &[PanelInstance],
    box_a: &[usize],
    box_b: &[usize],
) -> BTreeMap<TreeId, Landing> {
    let mut landings = BTreeMap::new();
    let mut plug =
        |output: &'static str, wire: &[usize], quirk_reversed: usize, quirk_rotated: usize| {
            for (position, index) in wire.iter().enumerate() {
                landings.insert(
                    instances[*index].id.clone(),
                    Landing {
                        output,
                        port: (position / PANELS_PER_PORT) as u32,
                        lamp: (position % PANELS_PER_PORT) as u32 * PANEL_LAMPS,
                        reversed: position == quirk_reversed,
                        rotation: if position == quirk_rotated {
                            PANEL_STRIDE
                        } else {
                            0
                        },
                    },
                );
            }
        };
    // usize::MAX = no quirk at that position in this box.
    plug(OUT_A, box_a, usize::MAX, 9);
    plug(OUT_B, box_b, 5, usize::MAX);
    landings
}

/// The door's segment id.
fn door_id() -> TreeId {
    TreeId::of(&PartId::new("door").expect("valid id"))
}

/// The door run shares the tail of box 1's last port, after its odd panel,
/// plugged with its legs swapped (rotated by one leg).
fn door_landing(box_a_panels: usize) -> Landing {
    Landing {
        output: OUT_A,
        port: (box_a_panels / PANELS_PER_PORT) as u32,
        lamp: (box_a_panels % PANELS_PER_PORT) as u32 * PANEL_LAMPS,
        reversed: false,
        rotation: DOOR_STRIDE,
    }
}

/// A fixture's patching, frozen from where each segment was plugged: the
/// fixture's start and an override on every segment that does not simply
/// follow on.
///
/// The segments (id and lamp count, in tree order) are walked with a
/// predicted cursor that moves as the patcher's does, on ports of
/// `count × 3` channels:
///
/// - The start is the first segment's address: output, port and channel
///   (in lamps) Set.
/// - A later segment whose address is where the cursor would lay it — at
///   the cursor, or on the next port with room from its channel 0 when it
///   does not fit what is left — gets no override for its address. Any
///   other gets port Set and channel Set (in lamps), and output Set only
///   when its output is not the cursor's.
/// - Reversed adds `direction: reverse`; a rotation `k` adds `offset: k`.
/// - The cursor then sits at the end of the segment's true address.
///
/// The upgrade step that froze the catalog's patching used the same rule
/// (`lpa-upgrade`'s `v11/freeze_patching.rs`), so this writes its output.
fn freeze_patching(
    segments: &[(TreeId, u32)],
    landings: &BTreeMap<TreeId, Landing>,
    outputs: &[(&str, &[(String, u32)])],
) -> (Start, BTreeMap<TreeId, Override>) {
    let mut start = Start::default();
    let mut overrides = BTreeMap::new();
    let mut cursor: Option<Cursor<'_>> = None;
    for (id, lamps) in segments {
        let landing = landings
            .get(id)
            .unwrap_or_else(|| panic!("segment {id} is plugged"));
        let ports = outputs
            .iter()
            .find(|(name, _)| *name == landing.output)
            .map(|(_, ports)| *ports)
            .unwrap_or_else(|| panic!("{id} lands on {}, an output", landing.output));
        let index = landing.port as usize;
        assert!(index < ports.len(), "{id} lands on a port its box has");
        let first_channel = landing.lamp * CHANNELS_PER_LAMP;
        let need = lamps * CHANNELS_PER_LAMP;

        let mut entry = Override {
            direction: landing.reversed.then_some(Direction::Reverse),
            offset: (landing.rotation != 0).then_some(landing.rotation),
            ..Override::default()
        };
        match &cursor {
            None => start = Start::at(landing.output, landing.port, Amount::lamps(landing.lamp)),
            Some(at) => {
                let follows =
                    at.output == landing.output && at.lay(need) == Some((index, first_channel));
                if !follows {
                    if at.output != landing.output {
                        entry.output =
                            Some(OutputOp::Set(patching::OutputName::new(landing.output)));
                    }
                    entry.port = Some(PortOp::set(landing.port));
                    entry.channel = Some(patching::ChannelOp::Set(Amount::lamps(landing.lamp)));
                }
            }
        }
        if !entry.is_empty() {
            overrides.insert(id.clone(), entry);
        }
        cursor = Some(Cursor {
            output: landing.output,
            ports,
            port: index,
            channel: first_channel + need,
        });
    }
    (start, overrides)
}

/// Where the patcher's cursor stands after a laid segment.
struct Cursor<'a> {
    output: &'a str,
    /// The output's ports: `(endpoint token, lamp count)`, in key order.
    ports: &'a [(String, u32)],
    /// The port's index.
    port: usize,
    /// In channels.
    channel: u32,
}

impl Cursor<'_> {
    /// Where the patcher lays the next segment of `need` channels when
    /// nothing moves the cursor: here when it fits, else the next port
    /// from its channel 0 that has room (`None`: past the last port).
    fn lay(&self, need: u32) -> Option<(usize, u32)> {
        if self.room(self.port, self.channel) >= need {
            return Some((self.port, self.channel));
        }
        ((self.port + 1)..self.ports.len())
            .find(|&index| self.room(index, 0) >= need)
            .map(|index| (index, 0))
    }

    /// What is left of port `index` from `channel`.
    fn room(&self, index: usize, channel: u32) -> u32 {
        (self.ports[index].1 * CHANNELS_PER_LAMP).saturating_sub(channel)
    }
}

/// One output's ports as the patcher is given them.
fn output_ports(name: &str, ports: &[(String, u32)]) -> OutputPorts {
    let lamps: Vec<Option<u32>> = ports.iter().map(|(_, count)| Some(*count)).collect();
    OutputPorts::of_lamps(name, &lamps, CHANNELS_PER_LAMP)
}

/// A port's capacity in channels.
fn port_capacity(outputs: &[OutputPorts], output: &str, port: u32) -> u32 {
    outputs
        .iter()
        .find(|o| o.output.as_str() == output)
        .and_then(|o| o.ports.iter().find(|p| p.key.0 == port))
        .and_then(|p| p.channels)
        .unwrap_or_else(|| panic!("box {output:?} has port {port}"))
}

/// The Arrange placements (`editor.json`): both fixtures at IDENTITY in the
/// shared 120-unit plan, so the door renders nestled among the panels at its
/// authored bottom-center spot instead of the unarranged side-by-side tiling.
///
/// Keys are PROJECT-RELATIVE node addresses (the studio strips the
/// host's root mount segment before reading/writing `editor.json`), so
/// the shipped arrangement works wherever the project is mounted —
/// gallery preview, saved library copy, or a test harness.
fn editor_doc(dome: &Map2dDoc, doors: &Map2dDoc) -> EditorMetaDoc {
    let mut doc = EditorMetaDoc::new();
    for (key, map2d) in [
        ("/dome.module/dome.fixture", dome),
        ("/doors.module/doors.fixture", doors),
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

/// The layout's invariants, checked against today's resolver: 50 one-strand
/// panels and one door strand, every lamp on the canvas, panel lamp pitch in
/// the band where the wrap reads as a strip, and the door's legs equal.
fn validate_layout(dome: &Map2dDoc, doors: &Map2dDoc) {
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
        assert_eq!(
            polygon.align,
            PathAlign::Inside,
            "panel {:?} lamps line the panel's form",
            object.name
        );
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator's invariants hold without touching the filesystem.
    #[test]
    fn generated_documents_validate() {
        Generated::new().validate();
    }

    /// The generator writes exactly the committed example, byte for byte.
    #[test]
    fn the_generated_documents_are_the_catalogs() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(EXAMPLE);
        for (relative, text) in Generated::new().files() {
            let committed = std::fs::read_to_string(example.join(relative))
                .unwrap_or_else(|e| panic!("read {relative}: {e}"));
            assert!(
                text == committed,
                "{relative} differs from the catalog's; run `cargo run -p lpt-geodome`"
            );
        }
    }

    /// Both format-6 documents read back as the mapping they were written
    /// from.
    #[test]
    fn generated_mappings_read_back() {
        let generated = Generated::new();
        for mapping in [&generated.dome_mapping, &generated.doors_mapping] {
            let text = map2d6::write(mapping).unwrap();
            assert_eq!(&map2d6::read(&text).unwrap(), mapping);
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
}
