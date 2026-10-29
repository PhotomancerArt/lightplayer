//! Every file of the radiance dome project, as text.
//!
//! Node files follow `catalog/projects/small-dome/` (fixture, output,
//! shader, clock) but sit under ONE root module with no sub-modules or
//! exports. The map is a `lpc_mapping::part::Mapping` written by
//! `lpc_mapping::map2d6::write` (map2d format 6): the layout, and the
//! placeholder patch frozen into its start and overrides
//! ([`crate::frozen_patching`]). There is no patch file.

use lpc_mapping::map2d6::{self, MAP2D6_FORMAT};
use lpc_mapping::part::{Mapping, Part, PartId, PartKind, Repeat, Shape, TreeId, render};
use lpc_mapping::{PathAlign, PolygonShape};

use crate::dome_geometry::{
    self, CLASSES, Dome, MARGIN, P2, RADIUS, SECTORS, panel_outline, rim_radius,
};
use crate::frozen_patching::freeze_patching;
use crate::json_text::{Json, number};
use crate::patch_plan::{PatchRow, ROTATED_PANEL, output_name, patch_rows};
use crate::{OUTPUTS, PANEL_LAMPS, PORTS_PER_OUTPUT};

/// The project manifest's format (`PROJECT_FORMAT_VERSION`).
const PROJECT_FORMAT: u32 = 12;
/// Fixture render size, square. At 256 px over a ~760-inch plan one texel
/// is about 3 inches; a panel spans roughly ten.
const RENDER_SIZE: u32 = 256;
/// Lamp sample diameter, in doc units (inches): about one texel.
const SAMPLE_DIAMETER: f64 = 3.0;

/// The project's files, `(file name, contents)`, in a fixed order.
pub fn project_files() -> Vec<(String, String)> {
    let plan = DomePlan::build();
    let rows = patch_rows(
        &plan
            .classes
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
    );
    let mapping = dome_mapping(&plan, &rows);
    let mut files = vec![
        ("project.json".to_string(), project_json()),
        ("module.json".to_string(), module_json()),
        (
            "clock.json".to_string(),
            Json::obj([("kind", Json::str("Clock"))]).pretty() + "\n",
        ),
        ("shader.json".to_string(), shader_json()),
        ("shader.glsl".to_string(), shader_glsl(&plan)),
        ("dome.json".to_string(), fixture_json()),
        (
            "dome.map2d.json".to_string(),
            map2d6::write(&mapping).expect("the dome's mapping writes"),
        ),
    ];
    for output in 0..OUTPUTS {
        files.push((format!("out_{}.json", output + 1), output_json(output)));
    }
    files.push(("README.md".to_string(), readme(&plan, &rows, &mapping)));
    files
}

/// The dome in doc space: the canvas, the zenith, and each class's
/// sector-0 outline.
struct DomePlan {
    /// Canvas width and height; the canvas starts at (0, 0).
    size: P2,
    /// The zenith: every repeat's rotation center.
    center: P2,
    classes: Vec<ClassObject>,
    /// Plan radius of the lowest lit ring, doc units.
    rim: f64,
}

struct ClassObject {
    name: String,
    id: String,
    points: Vec<P2>,
}

impl DomePlan {
    fn build() -> Self {
        let dome = Dome::build();
        let outlines: Vec<Vec<Vec<P2>>> = dome
            .panels
            .iter()
            .map(|sector| sector.iter().map(panel_outline).collect())
            .collect();
        let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
        for point in outlines.iter().flatten().flatten() {
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        let center = [ceil2(MARGIN - min[0]), ceil2(MARGIN - min[1])];
        let size = [
            ceil2(center[0] + max[0] + MARGIN),
            ceil2(center[1] + max[1] + MARGIN),
        ];
        let classes = (0..CLASSES)
            .map(|class| {
                let (ring, position) = dome.ring_position(class);
                ClassObject {
                    name: format!("ring {} panel {}", ring + 1, position + 1),
                    id: format!("r{}-p{:02}", ring + 1, position + 1),
                    points: outlines[0][class]
                        .iter()
                        .map(|p| [round2(center[0] + p[0]), round2(center[1] + p[1])])
                        .collect(),
                }
            })
            .collect();
        Self {
            size,
            center,
            classes,
            rim: rim_radius(&dome),
        }
    }

    /// Where the zenith and the rim land in the fixture's texture, as
    /// fractions of it: the product aspect-fits the canvas into the square
    /// render target, centred on the short axis (`map2d_fit`).
    fn texture_zenith_and_rim(&self) -> (P2, f64) {
        let [w, h] = self.size;
        let (scale, offset) = if w >= h {
            (1.0 / w, [0.0, (1.0 - h / w) / 2.0])
        } else {
            (1.0 / h, [(1.0 - w / h) / 2.0, 0.0])
        };
        let zenith = [
            self.center[0] * scale + offset[0],
            self.center[1] * scale + offset[1],
        ];
        (zenith, self.rim * scale)
    }
}

fn project_json() -> String {
    Json::obj([
        ("format", Json::Int(PROJECT_FORMAT.into())),
        ("name", Json::str("Radiance Dome")),
        (
            "description",
            Json::str(
                "The radiance dome from lp2014: 275 lucite panels on a 5V geodesic dome, \
                 22,000 lamps over ten control boxes. Lamp counts and patch are placeholders.",
            ),
        ),
    ])
    .pretty()
        + "\n"
}

fn module_json() -> String {
    let mut names: Vec<String> = ["clock", "dome", "shader"]
        .iter()
        .map(|s| s.to_string())
        .chain((1..=OUTPUTS).map(|n| format!("out_{n}")))
        .collect();
    // The product's node table is key-sorted (VecMap<String, _>).
    names.sort();
    let nodes = names
        .into_iter()
        .map(|name| {
            let file = format!("./{name}.json");
            (name, Json::obj([("ref", Json::Str(file))]))
        })
        .collect();
    Json::obj([
        ("kind", Json::str("Module")),
        ("nodes", Json::Obj(nodes)),
        (
            "provenance",
            Json::obj([
                ("author", Json::str("Yona Appletree")),
                ("version", Json::str("1")),
                ("license", Json::str("CC0-1.0")),
            ]),
        ),
    ])
    .pretty()
        + "\n"
}

fn shader_json() -> String {
    Json::obj([
        ("kind", Json::str("Shader")),
        ("source", Json::str("shader.glsl")),
        ("render_order", Json::Int(0)),
        (
            "bindings",
            Json::obj([
                ("output", Json::obj([("target", Json::str("bus:visual.out"))])),
                ("phase", Json::obj([("source", Json::str("bus:speed"))])),
            ]),
        ),
        (
            "consumed",
            Json::obj([(
                "phase",
                Json::obj([
                    ("kind", Json::str("phasor")),
                    ("value", Json::str("f32")),
                    (
                        "phasor",
                        Json::obj([
                            ("period_seconds", Json::Num(40.0)),
                            ("waveform", Json::str("ramp")),
                            ("phase_offset", Json::Num(0.0)),
                        ]),
                    ),
                    ("default_bind", Json::str("bus:speed")),
                    ("default", Json::Int(0)),
                    ("label", Json::str("Phase")),
                    (
                        "description",
                        Json::str("Master cycle; the bloom and the falling rings ride whole multiples of it"),
                    ),
                ]),
            )]),
        ),
        ("space", Json::obj([("kind", Json::str("TwoD"))])),
    ])
    .pretty()
        + "\n"
}

fn shader_glsl(plan: &DomePlan) -> String {
    let (zenith, rim) = plan.texture_zenith_and_rim();
    format!(
        r#"// Radiance dome — a five-petal bloom turning over rings that fall from
// the zenith.
//
// The dome is mapped as a plan view about its zenith, so distance from
// ZENITH is height on the dome and angle is azimuth. Generated by
// lab-mapping-radiance-gen: ZENITH and RIM are where the zenith and the
// lowest lit ring land in the fixture's texture (canvas aspect-fit into
// the square render target). Every term rides an integer multiple of the
// phasor, so nothing jumps when it wraps.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

const float TAU = 6.28318530718;
const vec2 ZENITH = vec2({zx:.4}, {zy:.4});
const float RIM = {rim:.4};

vec4 render_2d(vec2 pos) {{
    vec2 p = pos / outputSize - ZENITH;
    float r = length(p) / RIM; // 0 at the zenith, 1 at the lowest lit ring
    float a = atan(p.y, p.x);

    // Rings falling from the zenith: four in view, eight passes per cycle.
    float rings = 0.5 + 0.5 * cos(TAU * (r * 4.0 - phase * 8.0));
    rings = rings * rings;

    // Five petals, matching the dome's symmetry, turning once per cycle
    // and twisting as they fall.
    float petals = 0.5 + 0.5 * cos(5.0 * a + TAU * phase + r * 3.0);

    vec3 deep = vec3(0.05, 0.02, 0.18);
    vec3 bloom = vec3(0.95, 0.35, 0.55);
    vec3 crest = vec3(1.00, 0.80, 0.35);
    vec3 color = mix(deep, bloom, petals) + crest * rings * 0.6;
    return vec4(color, 1.0);
}}
"#,
        zx = zenith[0],
        zy = zenith[1],
        rim = rim,
    )
}

fn fixture_json() -> String {
    Json::obj([
        ("kind", Json::str("Fixture")),
        (
            "render_size",
            Json::obj([
                ("width", Json::Int(RENDER_SIZE.into())),
                ("height", Json::Int(RENDER_SIZE.into())),
            ]),
        ),
        (
            "bindings",
            Json::obj([
                (
                    "input",
                    Json::obj([("source", Json::str("bus:visual.out"))]),
                ),
                (
                    "output",
                    Json::obj([("target", Json::str("bus:control.out"))]),
                ),
            ]),
        ),
        ("sampling", Json::str("direct")),
        ("diagnostic_mode", Json::str("off")),
        (
            "mapping",
            Json::obj([
                ("kind", Json::str("Map2d")),
                ("source", Json::str("dome.map2d.json")),
            ]),
        ),
        ("color_order", Json::str("rgb")),
        ("brightness", Json::Num(1.0)),
        ("gamma_correction", Json::Bool(false)),
    ])
    .pretty()
        + "\n"
}

/// The fixture's mapping: 55 repeats (`repeat1` …), each holding one
/// panel class's sector-0 polygon as a part with the class's id, and the
/// placeholder patch frozen into the fixture's start and overrides.
fn dome_mapping(plan: &DomePlan, rows: &[PatchRow]) -> Mapping {
    let point = |p: &P2| [p[0] as f32, p[1] as f32];
    let mut mapping = Mapping::new();
    mapping.sample_diameter = SAMPLE_DIAMETER as f32;
    mapping.canvas = Some([0.0, 0.0, plan.size[0] as f32, plan.size[1] as f32]);
    for class in &plan.classes {
        // No `stride`: 80 lamps do not split evenly over the subdivided
        // outline's sides, so there is no honest per-side count to write.
        // A repeat's copies then follow each other whole (80 lamps each).
        let polygon = PolygonShape {
            points: class.points.iter().map(point).collect(),
            count: PANEL_LAMPS,
            align: PathAlign::Inside,
        };
        let id = PartId::new(&class.id).expect("a panel id is a part id");
        let mut panel = Part::new(id, PartKind::Shape(Shape::Polygon(polygon)));
        panel.name = class.name.clone();
        let repeat = Repeat::new(panel, point(&plan.center), SECTORS as u32);
        let part = mapping.new_part(PartKind::Repeat(repeat));
        mapping.parts.push(part);
    }

    // The segments in lamp order, each with its plan row.
    let tree = render(&mapping).expect("the dome's mapping renders");
    let segments: Vec<(TreeId, u32, &PatchRow)> = tree
        .segments()
        .iter()
        .map(|segment| {
            let sector = segment.id.copies.first().copied().unwrap_or(0) as usize;
            let row = rows
                .iter()
                .find(|r| r.id == segment.id.part.as_str() && r.sector == sector)
                .expect("every panel copy has a row");
            (segment.id.clone(), segment.count(), row)
        })
        .collect();
    let frozen = freeze_patching(&segments, PANEL_LAMPS);
    mapping.start = frozen.start;
    mapping.overrides = frozen.overrides;
    mapping
}

fn output_json(output: usize) -> String {
    let ports = (0..PORTS_PER_OUTPUT)
        .map(|port| {
            (
                port.to_string(),
                Json::obj([
                    ("endpoint", Json::Str(endpoint(output, port))),
                    ("count", Json::Int(PANEL_LAMPS.into())),
                ]),
            )
        })
        .collect();
    Json::obj([
        ("kind", Json::str("Output")),
        ("name", Json::Str(output_name(output))),
        ("ports", Json::Obj(ports)),
        (
            "bindings",
            Json::obj([(
                "input",
                Json::obj([("source", Json::str("bus:control.out"))]),
            )]),
        ),
    ])
    .pretty()
        + "\n"
}

/// `ws281x:box<N>:P<NN>`: box N's port NN, both 1-based like the labels on
/// a box. It parses (`HwEndpointSpec` is any three non-empty ASCII parts,
/// and nothing checks the target), but no board serves a `box` target yet.
fn endpoint(output: usize, port: usize) -> String {
    format!("ws281x:box{}:P{:02}", output + 1, port + 1)
}

fn readme(plan: &DomePlan, rows: &[PatchRow], mapping: &Mapping) -> String {
    let quirk = |pred: &dyn Fn(&PatchRow) -> bool| {
        let row = rows.iter().find(|r| pred(r)).expect("quirk row");
        let id = PartId::new(&row.id).expect("a panel id is a part id");
        format!(
            "`{}` (panel {}, output \"{}\" port {})",
            TreeId::of(&id).in_copy(row.sector as u32).canonical(),
            row.sector * CLASSES
                + plan
                    .classes
                    .iter()
                    .position(|c| c.id == row.id)
                    .expect("class"),
            output_name(row.output),
            row.port,
        )
    };
    let reversed = quirk(&|r| r.reversed);
    let rotated = quirk(&|r| r.offset != 0);
    let used = rows.iter().map(|r| r.output).max().unwrap_or(0) + 1;
    format!(
        r#"# Radiance Dome

Generated by `lab/mapping/radiance-gen` — do not edit by hand. To change
it, edit the generator and rerun it (byte-identical when nothing changed):

```bash
cargo run -p lab-mapping-radiance-gen            # rewrites this directory
cargo run -p lab-mapping-radiance-gen -- <dir>   # or writes elsewhere
```

A test project for the mapping design lab at the radiance dome's scale,
in the product's own formats: `project.json` (format {PROJECT_FORMAT}), one root
module, a clock, a shader, the `dome` fixture, and ten outputs.

## Placeholders (fit-check Q-E)

**The lamp counts, the port counts and the patch are invented.** What is
known is lp2014's geometry; what is not:

- **Lamps per panel.** Every panel has {PANEL_LAMPS} lamps here ({total} in all).
  The real number is unknown, and lp2014 allowed separate pentagonal and
  hexagonal panel counts.
- **Lit rings.** 8 rings = 275 panels (lp2014's 2017 config);
  `RadianceDomeSpecs.RADIANCE_DOME` says 7 (225).
- **Boxes and ports.** {OUTPUTS} outputs (`out_1` … `out_{OUTPUTS}`, named "1" … "{OUTPUTS}") of
  {PORTS_PER_OUTPUT} ports, every port `count: {PANEL_LAMPS}`.
- **Patch.** One panel per port in sector order: panel `n = sector·55 +
  class` goes to output `n / 32 + 1`, port `n % 32`, from the port's first
  lamp. Outputs 1–8 fill, output {used} takes {tail} panels, the rest of the
  ports are empty. No as-built patch has been found.

Two invented install quirks keep the patch from being a pure identity:

- reversed: {reversed}
- rotated by {rotated_by} lamps (about a third of the strip): {rotated}

They are `REVERSED_PANEL` and `ROTATED_PANEL` in the generator's `src/patch_plan.rs`.

Endpoints are `ws281x:box<N>:P<NN>` (1-based, like a box's labels). The
string parses as an endpoint, but no board serves a `box` target: this
project runs on the simulated board or a host, never on hardware. At
{total} lamps it is also far past the ESP32-C6's 128–512 lamp design target.

## Geometry (from lp2014)

A frequency-5 geodesic icosahedral dome, {RADIUS} in (19 ft) radius, zenith up; the top
8 face rings are lit (`GeodesicIcosahedralFixture`, `RadianceDomeGeometry`).

- **Plan view.** Azimuthal equidistant about the zenith with lp2014's
  orientation. One doc unit is one inch of arc along the shell from the
  zenith, uniform on both axes (lp2014 stretched each axis to a unit box).
  Canvas: `[0, 0, {w}, {h}]`, zenith (the repeat center) at
  `[{cx}, {cy}]`, including a {MARGIN}-unit margin.
- **Panels.** Each panel is its face shrunk to {shrink} toward the face
  centroid (the 2 ft lucite panel inside a ~3.6 ft face), its edges cut into
  lp2014's `expansionFactor` (2–5) great-circle segments so they curve.
  The polygon starts at the corner farthest from the zenith and winds
  clockwise.
- **Parts.** 55 panel classes, each a 5-way repeat (`repeat1` …
  `repeat{CLASSES}`) holding the sector-0 panel's polygon as a part
  (`align: inside`). Part ids are `r<ring>-p<NN>`: ring 1–8 from the
  zenith, position in the ring clockwise; copy `k` of a panel is
  `r<ring>-p<NN>@k` (copy 0 is the part's own id). No stride: {PANEL_LAMPS} lamps
  don't split evenly over a subdivided outline's sides, so a repeat's
  copies follow each other whole ({PANEL_LAMPS} lamps each).
- **Sectors.** Copy `k` is the product's `k·72°` clockwise turn, so
  sectors are numbered clockwise; sector `k` is lp2014's fifth
  (`restrictToFifth`) `(1 − k) mod 5 + 1`. Sector 0 holds lp2014's panel 0.
- **Not applied:** lp2014's per-fixture install transform (scale 0.867 ×
  0.882 plus a translate in `radiance.json`); the fixture keeps the identity.

## Files

- `dome.map2d.json` — map2d format {MAP2D6_FORMAT}, sample diameter {sd} (≈ one texel):
  the layout and the patch. The patch is the fixture's start (the first
  panel's port) and {overrides} overrides, one on every other panel copy, since
  none follows on from the one before it in lamp order.
- `dome.json` — the fixture, {RENDER_SIZE}×{RENDER_SIZE}, reading `bus:visual.out`.
- `shader.json`, `shader.glsl` — a five-petal bloom over falling rings.
- `out_1.json` … `out_{OUTPUTS}.json`, `clock.json`, `module.json`, `project.json`.
"#,
        total = thousands(SECTORS * CLASSES * PANEL_LAMPS as usize),
        tail = rows.iter().filter(|r| r.output + 1 == used).count(),
        rotated_by = ROTATED_PANEL.2,
        w = number(plan.size[0]),
        h = number(plan.size[1]),
        cx = number(plan.center[0]),
        cy = number(plan.center[1]),
        shrink = dome_geometry::PANEL_SHRINK,
        sd = number(SAMPLE_DIAMETER),
        overrides = mapping.overrides.len(),
    )
}

/// `22000` as `22,000`.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn ceil2(x: f64) -> f64 {
    (x * 100.0).ceil() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::patching::{Direction, OutputPorts, patch_mapping};

    fn file(name: &str) -> String {
        project_files()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no {name}"))
            .1
    }

    fn json(name: &str) -> serde_json::Value {
        serde_json::from_str(&file(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// What the generator writes is the checked-in project, file for file
    /// and byte for byte, and nothing else is there.
    #[test]
    fn the_generated_files_are_the_checked_in_project() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../projects/radiance-dome");
        let files = project_files();
        for (name, text) in &files {
            let on_disk =
                std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(on_disk == *text, "{name} differs from the checked-in file");
        }
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        on_disk.sort();
        let mut written: Vec<String> = files.into_iter().map(|(name, _)| name).collect();
        written.sort();
        assert_eq!(on_disk, written);
    }

    #[test]
    fn map_holds_55_repeats_of_five_80_lamp_panels() {
        let text = file("dome.map2d.json");
        assert_eq!(map2d6::peek_format(&text).unwrap(), MAP2D6_FORMAT);
        let mapping = map2d6::read(&text).unwrap();
        assert_eq!(mapping, dome_mapping_built());
        assert_eq!(mapping.parts.len(), 55);
        let tree = render(&mapping).unwrap();
        assert_eq!(tree.segments().len(), 275);
        assert!(tree.segments().iter().all(|s| s.count() == PANEL_LAMPS));
        assert_eq!(tree.positions().len(), 22_000);
    }

    /// Patched by the product's patcher, every panel copy lands where the
    /// plan puts it — its row's output, port and first lamp, reversed and
    /// rotated as the row says — with no problem.
    #[test]
    fn every_panel_lands_where_the_plan_puts_it() {
        let mapping = map2d6::read(&file("dome.map2d.json")).unwrap();
        let tree = render(&mapping).unwrap();
        let outputs: Vec<OutputPorts> = (0..OUTPUTS)
            .map(|o| {
                OutputPorts::of_lamps(&output_name(o), &[Some(PANEL_LAMPS); PORTS_PER_OUTPUT], 3)
            })
            .collect();
        let patching = patch_mapping(&mapping, &tree, &outputs);
        assert!(patching.problems.is_empty(), "{:?}", patching.problems);
        let ids: Vec<String> = DomePlan::build()
            .classes
            .into_iter()
            .map(|c| c.id)
            .collect();
        let rows = patch_rows(&ids);
        assert_eq!(patching.segments.len(), rows.len());
        for row in &rows {
            let id = TreeId::of(&PartId::new(&row.id).unwrap())
                .in_copy(row.sector as u32)
                .canonical();
            let landed = patching
                .segment(&id)
                .unwrap_or_else(|| panic!("{id} is not laid"));
            assert_eq!(
                landed.address.output.as_str(),
                output_name(row.output),
                "{id}"
            );
            assert_eq!(landed.address.port.0 as usize, row.port, "{id}");
            assert_eq!(landed.address.channel, 0, "{id}");
            assert_eq!(landed.direction == Direction::Reverse, row.reversed, "{id}");
            assert_eq!(landed.offset, row.offset, "{id}");
            assert_eq!(landed.limit, None, "{id}");
        }
        assert_eq!(rows.iter().filter(|r| r.reversed).count(), 1);
        assert_eq!(rows.iter().filter(|r| r.offset != 0).count(), 1);
    }

    #[test]
    fn ids_are_unique_product_slugs() {
        let plan = DomePlan::build();
        let mut seen = std::collections::BTreeSet::new();
        for class in &plan.classes {
            let id = class.id.as_str();
            assert!(id.len() <= 24 && id.starts_with(|c: char| c.is_ascii_lowercase()));
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            );
            assert!(seen.insert(id.to_string()), "duplicate {id}");
        }
    }

    #[test]
    fn outputs_declare_320_ports_of_80() {
        let mut ports = 0;
        for output in 1..=OUTPUTS {
            let doc = json(&format!("out_{output}.json"));
            assert_eq!(doc["name"], output.to_string());
            for (key, port) in doc["ports"].as_object().unwrap() {
                assert_eq!(port["count"], 80);
                let endpoint = port["endpoint"].as_str().unwrap();
                assert_eq!(endpoint.split(':').count(), 3);
                assert!(key.parse::<usize>().unwrap() < PORTS_PER_OUTPUT);
                ports += 1;
            }
        }
        assert_eq!(ports, 320);
    }

    #[test]
    fn instance_k_lands_on_sector_k_by_centroid() {
        // Each class's written polygon, turned the way the product turns
        // repeat instance k, against that sector's panel built from its own
        // lp2014 fifth.
        let plan = DomePlan::build();
        let dome = Dome::build();
        for (class, object) in plan.classes.iter().enumerate() {
            for sector in 0..SECTORS {
                let turned: Vec<P2> = object
                    .points
                    .iter()
                    .map(|p| rotation_2d(plan.center, 72.0 * sector as f64, *p))
                    .collect();
                let real: Vec<P2> = panel_outline(&dome.panels[sector][class])
                    .iter()
                    .map(|p| [plan.center[0] + p[0], plan.center[1] + p[1]])
                    .collect();
                let (a, b) = (mean(&turned), mean(&real));
                assert!(
                    (a[0] - b[0]).hypot(a[1] - b[1]) < 0.02,
                    "/{}/{sector} lands at {a:?}, its panel is at {b:?}",
                    object.id
                );
                for p in &turned {
                    assert!(p[0] > 0.0 && p[1] > 0.0 && p[0] < plan.size[0] && p[1] < plan.size[1]);
                }
            }
        }
    }

    #[test]
    fn json_files_parse_and_generation_is_deterministic() {
        let first = project_files();
        assert_eq!(first, project_files());
        for (name, text) in &first {
            assert!(text.ends_with('\n'), "{name}");
            if name.ends_with(".json") {
                serde_json::from_str::<serde_json::Value>(text)
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
            }
        }
        assert_eq!(first.len(), 8 + OUTPUTS);
    }

    fn dome_mapping_built() -> Mapping {
        let plan = DomePlan::build();
        let ids: Vec<String> = plan.classes.iter().map(|c| c.id.clone()).collect();
        dome_mapping(&plan, &patch_rows(&ids))
    }

    /// The product's `Rotation2d::about(center, degrees).apply(point)`:
    /// positive turns clockwise on a y-down screen.
    fn rotation_2d(center: P2, degrees: f64, point: P2) -> P2 {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let (dx, dy) = (point[0] - center[0], point[1] - center[1]);
        [
            center[0] + dx * cos - dy * sin,
            center[1] + dx * sin + dy * cos,
        ]
    }

    fn mean(points: &[P2]) -> P2 {
        [0, 1].map(|axis| points.iter().map(|p| p[axis]).sum::<f64>() / points.len() as f64)
    }
}
