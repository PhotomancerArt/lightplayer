//! Pattern-project templates: the "New → 1D / 2D pattern project" scaffolds.
//!
//! A **pattern project** (module authoring unit, D9/D14/D15) is a library
//! project whose point is one exported module: the rig around it exists so
//! the author can *see* the effect, and the export folder is what another
//! project vendors. These two compositions are that shape made concrete —
//! a workbench rig at the root, one self-contained `effect/` folder, and
//! `kind = "pattern"` / `exports = ["effect"]` on the container manifest so
//! the export boundary is visible from the very first gesture.
//!
//! ```text
//! project.json          kind: pattern, exports: ["effect"]
//! module.json           clock · effect/ · the rig
//! clock.json
//! strip_300.json        300-lamp strand      (1D template only)
//! strip_300_out.json
//! matrix_32x16.json     32x16 panel
//! matrix_32x16_out.json
//! effect/module.json    ← THE export: self-contained, provenanced
//! effect/shader.json
//! effect/shader.glsl
//! ```
//!
//! Two things make this work with no wiring the author has to do:
//!
//! - **The sub-module contributes its visual outward for free** (modules.md
//!   R7): the `effect/` module node mirrors its own scope's `visual.out` as
//!   a produced `output`, and the loader publishes that to the parent's
//!   `visual.out` at fallback priority. The rig fixtures read `bus:visual.out`
//!   at the root and see the effect.
//! - **The shader's phasor rides the enclosing scope's time product** (R5):
//!   the root `clock` is what animates it, and `effect/` authors no `bus:time`
//!   binding — which is exactly why the folder stays self-contained and lints
//!   clean under [`crate::check_exports`].
//!
//! Each rig fixture gets its **own** control channel (`control.strip_300`,
//! `control.matrix_32x16`) and its own output, the `examples/plasma-duo`
//! idiom: two fixtures writing one `control.out` would be a contention the
//! author never asked for.
//!
//! Honest today (T2 owns shader dimensionality): the 1D template's shader is
//! a `render(vec2)` that reads the long axis and ignores the short one. The
//! template is named 1D because its *rig* is; when the space work lands, the
//! template gains the space declaration and nothing else here moves.
//!
//! Pure data + serialization, exactly like [`crate::nodes::starter_project`]:
//! the compositions return `(relative path, bytes)` pairs and never touch a
//! filesystem.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_collection::VecMap;

use crate::node::kind::NodeKind;
use crate::nodes::fixture::{ColorOrder, FixtureSamplingConfig, MappingConfig};
use crate::nodes::node_def::NodeDefWriteError;
use crate::nodes::provenance_def::ProvenanceDef;
use crate::nodes::starter::{starter_def_for_kind, starter_for_kind};
use crate::project::manifest::ProjectKind;
use crate::{
    ArtifactSpec, AssetSlot, BindingDef, BindingDefs, BindingRef, BusSlotRef, ChannelName, Dim2u,
    EnumSlot, HwEndpointSpec, MapSlot, ModuleDef, NodeDef, NodeInvocation, NodeInvocationSlot,
    OptionSlot, OutputDef, ProjectManifest, SlotShapeRegistry, ValueSlot,
};

/// The one export folder a pattern template authors. Named `effect` (not
/// the project's name) so the vendoring gesture reads the same in every
/// pattern project, and so P3's exports rail has a stable thing to show.
pub const PATTERN_EXPORT_FOLDER: &str = "effect";

/// License stamped on the exported module. An export with no license is a
/// lint warning (D8) and, worse, a module nobody who imports it may safely
/// reuse — a template must never hand the author that problem. CC0 matches
/// what the repo's own vendored modules carry.
const PATTERN_EXPORT_LICENSE: &str = "CC0-1.0";

/// Authored version of the exported module. No semver semantics yet
/// (`ProvenanceDef`), so a new module starts at `1`.
const PATTERN_EXPORT_VERSION: &str = "1";

/// One workbench rig: a fixture with its mapping document and the output
/// driving it. `name` is both the node name and the file stem, so it must
/// satisfy [`crate::NodeName`] (ASCII alphanumerics and `_`, no leading
/// digit) — the template tests assert exactly that.
struct RigSpec {
    name: &'static str,
    /// Fixture render size: the texture the effect is materialized into
    /// before the lamps sample it.
    size: Dim2u,
    /// The rig's `*.map2d.json` document.
    map2d: &'static str,
    /// Wire the rig's output drives. Unauthored hardware is not a thing —
    /// an output node always names a wire — so the templates take the
    /// default wire and the next one along, the `examples/plasma-duo`
    /// shape. This is NOT a rig chooser: retargeting is an edit.
    endpoint: &'static str,
}

impl RigSpec {
    fn out_name(&self) -> String {
        format!("{}_out", self.name)
    }

    /// The rig's private control channel: one fixture, one output, one
    /// channel — never the shared `control.out`.
    fn control_channel(&self) -> String {
        format!("control.{}", self.name)
    }
}

/// A 300-lamp strand: the length a real strip usually is, laid out along x
/// so a 1D effect has somewhere honest to run.
const STRIP_300: RigSpec = RigSpec {
    name: "strip_300",
    size: Dim2u {
        width: 300,
        height: 1,
    },
    map2d: STRIP_300_MAP2D,
    endpoint: "ws281x:local:D10",
};

/// The 32x16 panel every 2D effect is first judged on.
const MATRIX_32X16: RigSpec = RigSpec {
    name: "matrix_32x16",
    size: Dim2u {
        width: 32,
        height: 16,
    },
    map2d: MATRIX_32X16_MAP2D,
    endpoint: "ws281x:local:D11",
};

/// The **1D pattern project**: a 300-lamp strand plus the 32x16 panel, so
/// the author sees the effect on the shape it is for AND on the shape
/// someone will inevitably run it on.
pub fn pattern_project_files_1d(
    name: &str,
    registry: &SlotShapeRegistry,
) -> Result<Vec<(String, Vec<u8>)>, NodeDefWriteError> {
    pattern_project_files(
        name,
        registry,
        &[STRIP_300, MATRIX_32X16],
        PATTERN_1D_BODY_GLSL,
    )
}

/// The **2D pattern project**: the panel alone. Same export, same wiring;
/// only the rig differs.
pub fn pattern_project_files_2d(
    name: &str,
    registry: &SlotShapeRegistry,
) -> Result<Vec<(String, Vec<u8>)>, NodeDefWriteError> {
    // The lone rig takes the DEFAULT wire: a one-output project should not
    // author the second one just because the 1D template's matrix does.
    let matrix = RigSpec {
        endpoint: STRIP_300.endpoint,
        ..MATRIX_32X16
    };
    pattern_project_files(name, registry, &[matrix], PATTERN_2D_BODY_GLSL)
}

/// Both templates, differing only in rig list and shader body.
fn pattern_project_files(
    name: &str,
    registry: &SlotShapeRegistry,
    rigs: &[RigSpec],
    shader_body: &str,
) -> Result<Vec<(String, Vec<u8>)>, NodeDefWriteError> {
    let mut files = Vec::new();

    let mut manifest = ProjectManifest::new_current(name);
    manifest.set_kind(ProjectKind::Pattern {
        exports: alloc::vec![String::from(PATTERN_EXPORT_FOLDER)],
    });
    files.push((
        String::from("project.json"),
        manifest.write_json().into_bytes(),
    ));

    files.push((
        String::from("module.json"),
        node_json(&root_module_def(rigs), registry)?,
    ));

    files.push((
        String::from("clock.json"),
        node_json(&starter_def_for_kind(NodeKind::Clock), registry)?,
    ));

    for rig in rigs {
        files.extend(rig_files(rig, registry)?);
    }

    files.push((
        format!("{PATTERN_EXPORT_FOLDER}/module.json"),
        node_json(&effect_module_def(), registry)?,
    ));
    files.push((
        format!("{PATTERN_EXPORT_FOLDER}/shader.json"),
        node_json(&effect_shader_def(), registry)?,
    ));
    files.push((
        format!("{PATTERN_EXPORT_FOLDER}/shader.glsl"),
        shader_glsl(shader_body).into_bytes(),
    ));

    Ok(files)
}

/// Root module: the clock, the export folder, and every rig's fixture and
/// output. Node order is authored order — clock first, then the export
/// (what the project is *about*), then the rig it is judged on.
fn root_module_def(rigs: &[RigSpec]) -> NodeDef {
    let mut nodes = VecMap::new();
    nodes.insert(String::from("clock"), path_invocation("./clock.json"));
    nodes.insert(
        String::from(PATTERN_EXPORT_FOLDER),
        path_invocation(&format!("./{PATTERN_EXPORT_FOLDER}/module.json")),
    );
    for rig in rigs {
        nodes.insert(
            String::from(rig.name),
            path_invocation(&format!("./{}.json", rig.name)),
        );
        let out = rig.out_name();
        nodes.insert(out.clone(), path_invocation(&format!("./{out}.json")));
    }
    NodeDef::Module(ModuleDef {
        nodes: MapSlot::new(nodes),
        ..ModuleDef::default()
    })
}

/// One rig's three files: fixture, mapping document, output.
fn rig_files(
    rig: &RigSpec,
    registry: &SlotShapeRegistry,
) -> Result<Vec<(String, Vec<u8>)>, NodeDefWriteError> {
    let control = rig.control_channel();

    let mut fixture = starter_for_kind(NodeKind::Fixture)
        .expect("fixture kind has a starter")
        .for_stem(rig.name)
        .def;
    if let NodeDef::Fixture(fixture) = &mut fixture {
        fixture.render_size = ValueSlot::new(rig.size);
        fixture.bindings = fixture_binding_defs(&control);
        // One lamp per texel on both rigs (the mapping documents are
        // authored at exactly the fixture's texture size), so sampling
        // straight through is both cheaper and sharper than area-sampling.
        fixture.sampling = ValueSlot::new(FixtureSamplingConfig::Direct);
        fixture.color_order = ValueSlot::new(ColorOrder::Rgb);
        // The starter fixture's map2d asset name follows `for_stem`; the
        // document itself is this template's, not the starter's 8x8 grid.
        fixture.mapping = EnumSlot::new(MappingConfig::Map2d {
            source: AssetSlot::path(format!("{}.map2d.json", rig.name)),
        });
    }

    let mut output = OutputDef::new(HwEndpointSpec::from_static(rig.endpoint));
    output.bindings = bus_input_binding_defs(&control);

    Ok(alloc::vec![
        (format!("{}.json", rig.name), node_json(&fixture, registry)?,),
        (
            format!("{}.map2d.json", rig.name),
            rig.map2d.as_bytes().to_vec(),
        ),
        (
            format!("{}.json", rig.out_name()),
            node_json(&NodeDef::Output(output), registry)?,
        ),
    ])
}

/// The exported module: one shader and the provenance that makes it
/// vendorable. Nothing here reaches outside the folder — that is the whole
/// property [`crate::check_exports`] tests for, and the template must ship
/// with zero findings.
fn effect_module_def() -> NodeDef {
    let mut nodes = VecMap::new();
    nodes.insert(String::from("shader"), path_invocation("./shader.json"));
    NodeDef::Module(ModuleDef {
        nodes: MapSlot::new(nodes),
        provenance: OptionSlot::some(ProvenanceDef {
            // Author left unstated: Studio does not know who this is, and
            // guessing a name into someone's license header is worse than
            // an empty field they fill in.
            author: OptionSlot::none(),
            version: OptionSlot::some(ValueSlot::new(String::from(PATTERN_EXPORT_VERSION))),
            license: OptionSlot::some(ValueSlot::new(String::from(PATTERN_EXPORT_LICENSE))),
            created: OptionSlot::none(),
        }),
        ..ModuleDef::default()
    })
}

/// The exported module's shader: the starter scaffold's phasor slot and
/// `shader.glsl` source, publishing to the module scope's own `visual.out`
/// (which R7 then mirrors outward to the rig).
fn effect_shader_def() -> NodeDef {
    let mut def = starter_for_kind(NodeKind::Shader)
        .expect("shader starter exists")
        .for_stem("shader")
        .def;
    if let NodeDef::Shader(shader) = &mut def {
        shader.bindings = bus_output_binding_defs("visual.out");
    }
    def
}

fn path_invocation(path: &str) -> NodeInvocationSlot {
    NodeInvocationSlot::new(NodeInvocation::path(ArtifactSpec::path(path)))
}

fn node_json(node: &NodeDef, registry: &SlotShapeRegistry) -> Result<Vec<u8>, NodeDefWriteError> {
    node.write_json(registry).map(String::into_bytes)
}

fn fixture_binding_defs(control: &str) -> BindingDefs {
    let mut entries = VecMap::new();
    entries.insert(
        String::from("input"),
        BindingDef::source(bus_ref("visual.out")),
    );
    entries.insert(String::from("output"), BindingDef::target(bus_ref(control)));
    BindingDefs::new(entries)
}

fn bus_input_binding_defs(slot: &str) -> BindingDefs {
    single_binding_defs("input", BindingDef::source(bus_ref(slot)))
}

fn bus_output_binding_defs(slot: &str) -> BindingDefs {
    single_binding_defs("output", BindingDef::target(bus_ref(slot)))
}

fn single_binding_defs(slot: &str, binding: BindingDef) -> BindingDefs {
    let mut entries = VecMap::new();
    entries.insert(String::from(slot), binding);
    BindingDefs::new(entries)
}

fn bus_ref(slot: &str) -> BindingRef {
    BindingRef::Bus(BusSlotRef::new(ChannelName(String::from(slot))))
}

/// 300 lamps evenly along a one-texel-tall canvas: the strand IS the
/// texture's long axis, so `pos.x` in the shader is position along the
/// strip with no mapping arithmetic in between.
const STRIP_300_MAP2D: &str = r#"{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [
    0.0,
    0.0,
    300.0,
    1.0
  ],
  "objects": [
    {
      "name": "strand",
      "shape": {
        "path": {
          "points": [
            [
              0.5,
              0.5
            ],
            [
              299.5,
              0.5
            ]
          ],
          "count": 300
        }
      }
    }
  ]
}
"#;

/// A 32x16 panel wired the way panels usually are (snake routing from the
/// top-left), one lamp per texel of the fixture texture.
const MATRIX_32X16_MAP2D: &str = r#"{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [
    0.0,
    0.0,
    32.0,
    16.0
  ],
  "objects": [
    {
      "name": "panel",
      "shape": {
        "grid": {
          "origin": [
            0.5,
            0.5
          ],
          "cols": 32,
          "rows": 16,
          "pitch": 1.0,
          "routing": "snake",
          "start_corner": "tl"
        }
      }
    }
  ]
}
"#;

/// Shared HSV helper, prepended to both template shaders (see
/// [`shader_glsl`]) so each `shader.glsl` is a complete, self-contained
/// body the author can read top to bottom.
const HSV_TO_RGB_GLSL: &str = r#"// Hue (0-1), saturation, value -> linear RGB.
vec3 hsv_to_rgb(float h, float s, float v) {
    float c = v * s;
    float x = c * (1.0 - abs(mod(h * 6.0, 2.0) - 1.0));
    float m = v - c;

    if (h < 1.0 / 6.0) {
        return vec3(v, m + x, m);
    } else if (h < 2.0 / 6.0) {
        return vec3(m + x, v, m);
    } else if (h < 3.0 / 6.0) {
        return vec3(m, v, m + x);
    } else if (h < 4.0 / 6.0) {
        return vec3(m, m + x, v);
    } else if (h < 5.0 / 6.0) {
        return vec3(m + x, m, v);
    }
    return vec3(v, m, m + x);
}
"#;

/// The 1D template's body: a comet running the long axis over a slow hue
/// wash. `render(vec2)` is honest — the entry point takes a pixel position
/// today (T2 owns shader-side dimensionality); a 1D pattern simply reads
/// the long axis and lets the short one be.
const PATTERN_1D_BODY_GLSL: &str = r#"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec4 render_2d(vec2 pos) {
    // Position along the strand, 0 at one end and 1 at the other. A 1D
    // pattern reads this axis and ignores pos.y entirely.
    float along = pos.x / max(outputSize.x, 1.0);

    // Hue washes the whole strand and drifts with the phasor. `phase`
    // already wraps in [0,1) each period, so nothing here folds seconds.
    float hue = fract(along - phase);

    // A comet head rides the strand once per period; `behind` is how far
    // this lamp sits BEHIND the head, so the tail trails the motion.
    float behind = fract(fract(phase) - along);
    float comet = pow(1.0 - behind, 12.0);

    vec3 rgb = hsv_to_rgb(hue, 1.0, 0.25 + 0.75 * comet);
    return vec4(clamp(rgb, vec3(0.0), vec3(1.0)), 1.0);
}"#;

/// The 2D template's body: a rotating color wheel — the same visual the
/// `lp create` starter project ships, which reads instantly on a panel.
const PATTERN_2D_BODY_GLSL: &str = r#"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec4 render_2d(vec2 pos) {
    vec2 center = outputSize * 0.5;
    vec2 dir = pos - center;

    // Angle around the center, turned one full rotation per phasor period.
    float angle = atan(dir.y, dir.x) + phase * 6.28318;
    float hue = fract((angle + 3.14159) / 6.28318);

    // Brightest at the middle, easing off toward the corners.
    float dist = min(length(dir) / max(length(center), 1.0), 1.0);
    vec3 rgb = hsv_to_rgb(hue, 1.0, 1.0 - dist * 0.5);

    return vec4(clamp(rgb, vec3(0.0), vec3(1.0)), 1.0);
}"#;

/// One template's complete `effect/shader.glsl`: the shared HSV helper
/// followed by the template's own body. Deterministic (pure concatenation
/// of two `const`s), so two calls produce identical bytes.
fn shader_glsl(body: &str) -> String {
    format!("{HSV_TO_RGB_GLSL}{body}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::export_check::{ExportFileSet, check_exports};
    use crate::{NodeName, PROJECT_FORMAT_VERSION};
    use alloc::string::ToString;
    use alloc::vec;

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }

    fn file<'a>(files: &'a [(String, Vec<u8>)], path: &str) -> &'a [u8] {
        files
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, bytes)| bytes.as_slice())
            .unwrap_or_else(|| panic!("template ships {path}: {:?}", names(files)))
    }

    fn text(files: &[(String, Vec<u8>)], path: &str) -> String {
        core::str::from_utf8(file(files, path))
            .expect("utf-8")
            .to_string()
    }

    fn names(files: &[(String, Vec<u8>)]) -> Vec<&str> {
        files.iter().map(|(name, _)| name.as_str()).collect()
    }

    /// The 1D template's file list, in authored order. Pinned because the
    /// tree IS the thing the New menu promises the author.
    #[test]
    fn pattern_1d_ships_the_promised_tree() {
        let files = pattern_project_files_1d("demo", &registry()).expect("compose");
        assert_eq!(
            names(&files),
            [
                "project.json",
                "module.json",
                "clock.json",
                "strip_300.json",
                "strip_300.map2d.json",
                "strip_300_out.json",
                "matrix_32x16.json",
                "matrix_32x16.map2d.json",
                "matrix_32x16_out.json",
                "effect/module.json",
                "effect/shader.json",
                "effect/shader.glsl",
            ]
        );
    }

    /// The 2D template is the 1D one minus the strand — same export, same
    /// wiring.
    #[test]
    fn pattern_2d_is_the_matrix_rig_alone() {
        let files = pattern_project_files_2d("demo", &registry()).expect("compose");
        assert_eq!(
            names(&files),
            [
                "project.json",
                "module.json",
                "clock.json",
                "matrix_32x16.json",
                "matrix_32x16.map2d.json",
                "matrix_32x16_out.json",
                "effect/module.json",
                "effect/shader.json",
                "effect/shader.glsl",
            ]
        );
    }

    /// Every `.json` node artifact parses and is already canonical
    /// `write_json` output — a template that needs re-normalizing on first
    /// save would show the author a dirty project it never edited.
    #[test]
    fn every_template_artifact_parses_and_is_canonical() {
        let registry = registry();
        for files in [
            pattern_project_files_1d("demo", &registry).expect("1d"),
            pattern_project_files_2d("demo", &registry).expect("2d"),
        ] {
            for (name, bytes) in &files {
                if !name.ends_with(".json")
                    || name.ends_with(".map2d.json")
                    || name == "project.json"
                {
                    continue;
                }
                let text = core::str::from_utf8(bytes).expect("utf-8");
                let def = NodeDef::read_json(&registry, text)
                    .unwrap_or_else(|e| panic!("{name} parses: {e}"));
                assert_eq!(
                    def.write_json(&registry).expect("re-write"),
                    text,
                    "{name} must be canonical write_json output"
                );
            }
        }
    }

    /// P1: the manifest is authored as a library project exporting exactly
    /// the one folder, at the current format, canonically.
    #[test]
    fn the_manifest_declares_a_pattern_project_exporting_effect() {
        let registry = registry();
        for files in [
            pattern_project_files_1d("porch sign", &registry).expect("1d"),
            pattern_project_files_2d("porch sign", &registry).expect("2d"),
        ] {
            let text = text(&files, "project.json");
            let manifest = ProjectManifest::read_json(&text).expect("container manifest");
            assert_eq!(manifest.format, Some(PROJECT_FORMAT_VERSION));
            assert_eq!(manifest.name.as_deref(), Some("porch sign"));
            assert_eq!(
                manifest.project_kind(),
                ProjectKind::Pattern {
                    exports: vec![String::from("effect")]
                }
            );
            assert_eq!(manifest.write_json(), text, "manifest must be canonical");
        }
    }

    /// P2: **the** gate on these templates — the export folder must lint
    /// clean, warnings included. A template that ships a finding teaches
    /// the author that findings are normal.
    #[test]
    fn the_export_folder_lints_clean_for_both_templates() {
        let registry = registry();
        for files in [
            pattern_project_files_1d("demo", &registry).expect("1d"),
            pattern_project_files_2d("demo", &registry).expect("2d"),
        ] {
            // The static check sees the export subtree, exactly as Studio
            // hands it over: every file under `effect/`.
            let set: ExportFileSet<'_> = files
                .iter()
                .filter(|(name, _)| name.starts_with("effect/"))
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect();
            let report = check_exports(&[String::from("effect")], &set);
            assert!(
                report.is_empty(),
                "template exports must lint clean: {:?}",
                report.findings
            );
        }
    }

    /// The export is self-contained by construction: nothing under
    /// `effect/` may name a path that climbs out of it. (The lint proves
    /// this too; this asserts it on the *bytes*, so a future edit that
    /// adds a `../` ref fails with an obvious message.)
    #[test]
    fn nothing_in_the_export_folder_reaches_outside_it() {
        let registry = registry();
        for files in [
            pattern_project_files_1d("demo", &registry).expect("1d"),
            pattern_project_files_2d("demo", &registry).expect("2d"),
        ] {
            for (name, bytes) in files.iter().filter(|(n, _)| n.starts_with("effect/")) {
                let text = core::str::from_utf8(bytes).expect("utf-8");
                assert!(!text.contains("\"../"), "{name} escapes the export folder");
            }
        }
    }

    /// The exported module carries provenance with a license — the field
    /// that decides whether anyone may reuse what they vendored (D8).
    #[test]
    fn the_exported_module_is_licensed() {
        let registry = registry();
        let files = pattern_project_files_1d("demo", &registry).expect("compose");
        let def =
            NodeDef::read_json(&registry, &text(&files, "effect/module.json")).expect("parse");
        let NodeDef::Module(module) = def else {
            panic!("the export root is a module");
        };
        let provenance = module.provenance.data.as_ref().expect("provenance");
        assert_eq!(
            provenance
                .license
                .data
                .as_ref()
                .map(|slot| slot.value().as_str()),
            Some(PATTERN_EXPORT_LICENSE)
        );
        assert_eq!(
            provenance
                .version
                .data
                .as_ref()
                .map(|slot| slot.value().as_str()),
            Some(PATTERN_EXPORT_VERSION)
        );
    }

    /// Every module node key is a legal [`NodeName`] — underscores, no
    /// hyphens, no leading digit (`matrix_32x16`, not `matrix-32x16`).
    #[test]
    fn every_node_key_is_a_legal_node_name() {
        let registry = registry();
        for files in [
            pattern_project_files_1d("demo", &registry).expect("1d"),
            pattern_project_files_2d("demo", &registry).expect("2d"),
        ] {
            for (name, bytes) in files.iter().filter(|(n, _)| n.ends_with("module.json")) {
                let def = NodeDef::read_json(&registry, core::str::from_utf8(bytes).unwrap())
                    .expect("module parses");
                let NodeDef::Module(module) = def else {
                    panic!("{name} is a module");
                };
                for key in module.nodes.entries.keys() {
                    NodeName::parse(key).unwrap_or_else(|e| panic!("{name} node key {key:?}: {e}"));
                }
            }
        }
    }

    /// Each rig owns its own control channel and its own wire: two
    /// fixtures on one `control.out` would be a contention the author
    /// never asked for, and two outputs on one pin would be a lie.
    #[test]
    fn each_rig_drives_its_own_channel_and_wire() {
        let registry = registry();
        let files = pattern_project_files_1d("demo", &registry).expect("compose");
        assert!(text(&files, "strip_300.json").contains("bus:control.strip_300"));
        assert!(text(&files, "matrix_32x16.json").contains("bus:control.matrix_32x16"));
        assert!(text(&files, "strip_300_out.json").contains("bus:control.strip_300"));
        assert!(text(&files, "matrix_32x16_out.json").contains("bus:control.matrix_32x16"));
        assert!(text(&files, "strip_300_out.json").contains("ws281x:local:D10"));
        assert!(text(&files, "matrix_32x16_out.json").contains("ws281x:local:D11"));

        // The 2D template's lone output takes the DEFAULT wire.
        let files = pattern_project_files_2d("demo", &registry).expect("compose");
        assert!(text(&files, "matrix_32x16_out.json").contains("ws281x:local:D10"));
    }

    /// The rig fixtures read the root `visual.out`, which is where R7's
    /// module mirror publishes the effect — this is what makes the
    /// template animate with no wiring gesture at all.
    #[test]
    fn the_rig_reads_the_bus_the_effect_module_publishes_to() {
        let registry = registry();
        let files = pattern_project_files_1d("demo", &registry).expect("compose");
        assert!(text(&files, "strip_300.json").contains("bus:visual.out"));
        // Inside the folder, the shader publishes to the EFFECT scope's
        // visual.out; the mirror carries it outward (loader-registered).
        assert!(text(&files, "effect/shader.json").contains("bus:visual.out"));
        // …and the export authors no `bus:time` binding: the phasor rides
        // the enclosing scope's time product (R5), which is what keeps the
        // folder self-contained.
        assert!(!text(&files, "effect/shader.json").contains("bus:time"));
    }

    /// The fixture render sizes match their mapping documents' canvases —
    /// one lamp per texel, which is what `direct` sampling assumes.
    #[test]
    fn fixture_render_sizes_match_their_mappings() {
        let registry = registry();
        let files = pattern_project_files_1d("demo", &registry).expect("compose");

        let def = NodeDef::read_json(&registry, &text(&files, "strip_300.json")).expect("fixture");
        let NodeDef::Fixture(strip) = def else {
            panic!("expected fixture");
        };
        assert_eq!(strip.render_width(), 300);
        assert_eq!(strip.render_height(), 1);
        assert_eq!(*strip.sampling.value(), FixtureSamplingConfig::Direct);
        let MappingConfig::Map2d { source } = strip.mapping.value() else {
            panic!("expected Map2d mapping");
        };
        assert_eq!(
            source.artifact_value().unwrap().to_string(),
            "strip_300.map2d.json"
        );
        assert!(text(&files, "strip_300.map2d.json").contains("\"count\": 300"));

        let def =
            NodeDef::read_json(&registry, &text(&files, "matrix_32x16.json")).expect("fixture");
        let NodeDef::Fixture(matrix) = def else {
            panic!("expected fixture");
        };
        assert_eq!(matrix.render_width(), 32);
        assert_eq!(matrix.render_height(), 16);
    }

    /// The shader body is a `render(vec2)` in both templates — honest
    /// about what the entry point is today (T2 owns dimensionality) — and
    /// it declares the `phase` phasor the clock drives.
    #[test]
    fn both_templates_ship_a_render_vec2_body_riding_the_phasor() {
        let registry = registry();
        for files in [
            pattern_project_files_1d("demo", &registry).expect("1d"),
            pattern_project_files_2d("demo", &registry).expect("2d"),
        ] {
            let glsl = text(&files, "effect/shader.glsl");
            assert!(glsl.contains("vec4 render_2d(vec2 pos)"), "{glsl}");
            assert!(glsl.contains("uniform float phase"), "{glsl}");
            assert!(glsl.contains("vec3 hsv_to_rgb"), "{glsl}");

            let def =
                NodeDef::read_json(&registry, &text(&files, "effect/shader.json")).expect("shader");
            let NodeDef::Shader(shader) = def else {
                panic!("expected shader");
            };
            assert_eq!(
                shader.shader_source().artifact_value().unwrap().to_string(),
                "shader.glsl",
                "the source ref must stay inside the export folder"
            );
            assert!(shader.consumed_slots.entries.get("phase").is_some());
        }
    }

    /// Deterministic bytes: two calls agree exactly, so an unchanged
    /// template never shows up as a diff.
    #[test]
    fn templates_are_byte_deterministic() {
        let registry = registry();
        assert_eq!(
            pattern_project_files_1d("demo", &registry).expect("1d"),
            pattern_project_files_1d("demo", &registry).expect("1d again")
        );
        assert_eq!(
            pattern_project_files_2d("demo", &registry).expect("2d"),
            pattern_project_files_2d("demo", &registry).expect("2d again")
        );
        // …and the two templates are genuinely different projects.
        assert_ne!(
            pattern_project_files_1d("demo", &registry).expect("1d"),
            pattern_project_files_2d("demo", &registry).expect("2d")
        );
    }
}
