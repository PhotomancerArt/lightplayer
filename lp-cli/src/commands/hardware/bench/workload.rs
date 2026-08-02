//! The bench workload: the project the soft-limit bench uploads at each ramp
//! step.
//!
//! One clock, one shader, one fixture wired as a single strip, one output.
//! The only thing that varies between steps is the LED count, which lives in
//! two places that must agree — the fixture's `render_size.width` and the
//! map2d grid's `cols` — so the workload is generated rather than authored.
//!
//! The workload *is* part of the metric definition (`leds.max-safe@1`, see
//! `lpc_model::measurement`): changing what it renders changes what a
//! checked-in measurement means. The shader is a frozen copy embedded at
//! compile time ([`workload_shader.glsl`](./workload_shader.glsl)), never a
//! runtime read of `examples/`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use lp_collection::VecMap;
use lpc_hardware::manifest::hw_manifest_file::HardwareResourceFile;
use lpc_hardware::{HardwareManifestFile, HwCapability};
use lpc_mapping::{GridShape, Map2dDoc, Map2dObject, Map2dShape};
use lpc_model::nodes::clock::ClockDef;
use lpc_model::nodes::fixture::{
    Brightness, ColorOrder, FixtureDef, FixtureDiagnosticMode, FixtureSamplingConfig, MappingConfig,
};
use lpc_model::nodes::output::{OutputDef, OutputDriverOptionsConfig};
use lpc_model::nodes::shader::{ShaderDef, ShaderSlotDef};
use lpc_model::{
    ArtifactSpec, AssetSlot, BindingDef, BindingDefs, BindingRef, BusSlotRef, ChannelName, Dim2u,
    Dim2uSlot, EnumSlot, HwEndpointSpec, MapSlot, NodeDef, NodeInvocation, NodeInvocationSlot,
    OptionSlot, ProjectDef, SlotShapeRegistry, ValueSlot,
};

/// Frozen copy of `examples/basic/shader.glsl` — see the header in that file
/// for why it is a copy.
const WORKLOAD_SHADER_GLSL: &str = include_str!("workload_shader.glsl");

/// Doc-space width of the generated strip. Arbitrary but fixed: the map2d
/// canvas is aspect-fit into the render target, so only the canvas *ratio*
/// matters, and pinning the width keeps generated docs comparable.
const STRIP_DOC_WIDTH: f32 = 100.0;

/// Fixture brightness for the bench. The metric is memory, not photons, and
/// buffer sizes do not depend on brightness — so the workload stays dim in
/// case a real strip is attached to the board under test.
const WORKLOAD_BRIGHTNESS: u32 = 32;

/// The workload for one ramp step.
#[derive(Debug, Clone)]
pub struct BenchWorkload {
    /// Lamps on the strip: the quantity being measured.
    pub led_count: u32,
    /// Where the output drives them.
    pub endpoint: HwEndpointSpec,
}

/// One generated project file: name and contents.
#[derive(Debug, Clone)]
pub struct WorkloadFile {
    pub name: String,
    pub contents: String,
}

impl BenchWorkload {
    pub fn new(led_count: u32, endpoint: HwEndpointSpec) -> Self {
        Self {
            led_count,
            endpoint,
        }
    }

    /// The whole project, as files to write beside each other.
    pub fn files(&self) -> Result<Vec<WorkloadFile>> {
        if self.led_count == 0 {
            bail!("bench workload needs at least one LED");
        }
        let registry = SlotShapeRegistry::default();

        Ok(vec![
            file("project.json", node_json(&registry, &self.project_def())?),
            file("clock.json", node_json(&registry, &self.clock_def())?),
            file("shader.json", node_json(&registry, &self.shader_def())?),
            file("shader.glsl", WORKLOAD_SHADER_GLSL.to_string()),
            file("fixture.json", node_json(&registry, &self.fixture_def())?),
            file("fixture.map2d.json", self.map2d_doc().to_json_pretty()),
            file("output.json", node_json(&registry, &self.output_def())?),
        ])
    }

    /// Write the workload into `dir`, creating it if needed.
    pub fn write_to(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating workload directory {}", dir.display()))?;
        for file in self.files()? {
            let path = dir.join(&file.name);
            std::fs::write(&path, file.contents)
                .with_context(|| format!("writing {}", path.display()))?;
        }
        Ok(())
    }

    fn project_def(&self) -> NodeDef {
        let mut nodes = VecMap::new();
        for name in ["clock", "shader", "fixture", "output"] {
            nodes.insert(
                name.to_string(),
                NodeInvocationSlot::new(NodeInvocation::new(ArtifactSpec::path(format!(
                    "./{name}.json"
                )))),
            );
        }
        NodeDef::Project(ProjectDef {
            format: ProjectDef::current_format_slot(),
            uid: OptionSlot::none(),
            name: OptionSlot::some(ValueSlot::new(format!(
                "Soft-limit bench ({} LEDs)",
                self.led_count
            ))),
            nodes: MapSlot::new(nodes),
        })
    }

    fn clock_def(&self) -> NodeDef {
        NodeDef::Clock(ClockDef::default())
    }

    fn shader_def(&self) -> NodeDef {
        let mut consumed = VecMap::new();
        consumed.insert(
            "time".to_string(),
            ShaderSlotDef::value_f32("Time", "Project clock time in seconds", 0.0, None)
                .with_default_bind(BindingRef::parse("bus:time").expect("bus:time binding")),
        );
        NodeDef::Shader(ShaderDef {
            source: AssetSlot::path("shader.glsl"),
            render_order: Default::default(),
            bindings: bindings([("output", target("visual.out"))]),
            float_mode: ValueSlot::default(),
            param_defs: MapSlot::default(),
            consumed_slots: MapSlot::new(consumed),
        })
    }

    fn fixture_def(&self) -> NodeDef {
        NodeDef::Fixture(FixtureDef {
            render_size: Dim2uSlot::new(Dim2u {
                width: self.led_count,
                height: 1,
            }),
            bindings: bindings([
                ("input", source("visual.out")),
                ("output", target("control.out")),
            ]),
            sampling: ValueSlot::new(FixtureSamplingConfig::Direct),
            diagnostic_mode: ValueSlot::new(FixtureDiagnosticMode::Off),
            mapping: EnumSlot::new(MappingConfig::Map2d {
                source: AssetSlot::path("fixture.map2d.json"),
            }),
            color_order: ValueSlot::new(ColorOrder::Rgb),
            brightness: OptionSlot::some(ValueSlot::new(Brightness(WORKLOAD_BRIGHTNESS))),
            gamma_correction: OptionSlot::some(ValueSlot::new(false)),
            ..FixtureDef::default()
        })
    }

    fn output_def(&self) -> NodeDef {
        NodeDef::Output(OutputDef {
            endpoint: ValueSlot::new(self.endpoint.clone()),
            bindings: bindings([("input", source("control.out"))]),
            options: OptionSlot::some(OutputDriverOptionsConfig {
                white_point: ValueSlot::new([1.0, 1.0, 1.0]),
                interpolation_enabled: ValueSlot::new(false),
                dithering_enabled: ValueSlot::new(false),
                lut_enabled: ValueSlot::new(false),
            }),
            ..OutputDef::default()
        })
    }

    /// A single row of `led_count` lamps.
    ///
    /// The canvas is sized to the strip's own aspect (`width : width/N`) so
    /// the aspect-fit into the `N × 1` render target is exactly one lamp per
    /// texel column. A square canvas would squeeze every lamp into the middle
    /// column instead.
    fn map2d_doc(&self) -> Map2dDoc {
        let pitch = STRIP_DOC_WIDTH / self.led_count as f32;
        Map2dDoc {
            format: lpc_mapping::MAP2D_FORMAT,
            sample_diameter: pitch,
            canvas: Some([0.0, 0.0, STRIP_DOC_WIDTH, pitch]),
            objects: vec![Map2dObject {
                name: "strip".to_string(),
                shape: Map2dShape::Grid(GridShape {
                    origin: [pitch * 0.5, pitch * 0.5],
                    cols: self.led_count,
                    rows: 1,
                    pitch,
                    routing: Default::default(),
                    start_corner: Default::default(),
                }),
            }],
        }
    }
}

/// The endpoint the bench drives on `board`: `ws281x:rmt:<label>` of its first
/// usable LED data pin.
///
/// Bench safety (plan.md): the board manifest is the allowlist. A board that
/// declares no WS281x timing resource is not benched at all, and the pin is
/// picked exactly the way the firmware's WS281x driver offers endpoints — a
/// GPIO that can output, that the board gives a silkscreen label (a resource
/// still labelled `GPIO<n>` is one the board does not name), and that the
/// manifest has not reserved. Strapping and USB pins are kept out of reach by
/// those two filters, which is the same guarantee the device relies on.
pub fn workload_endpoint(board: &HardwareManifestFile) -> Result<HwEndpointSpec> {
    let declares_ws281x = board
        .gpio
        .iter()
        .chain(board.resource.iter())
        .any(|resource| resource.capabilities.contains(&HwCapability::Ws281xOutput));
    if !declares_ws281x {
        bail!(
            "board `{}` declares no ws281x-output resource — nothing to bench",
            board.id
        );
    }

    board
        .gpio
        .iter()
        .chain(board.resource.iter())
        .find(|resource| is_ws281x_pin_candidate(resource))
        .map(|resource| ws281x_rmt_spec(&resource.display_label))
        .transpose()?
        .with_context(|| {
            format!(
                "board `{}` has no board-labelled, unreserved GPIO output to drive LEDs from",
                board.id
            )
        })
}

/// A GPIO the board names, can drive, and has not reserved.
fn is_ws281x_pin_candidate(resource: &HardwareResourceFile) -> bool {
    if !resource.capabilities.contains(&HwCapability::GpioOutput)
        || resource.reserved_reason.is_some()
    {
        return false;
    }
    let Some(number) = resource.address.strip_prefix("/gpio/") else {
        return false;
    };
    !resource
        .display_label
        .eq_ignore_ascii_case(&format!("GPIO{number}"))
}

fn ws281x_rmt_spec(label: &str) -> Result<HwEndpointSpec> {
    HwEndpointSpec::parse(format!("ws281x:rmt:{label}"))
        .with_context(|| format!("board label `{label}` does not form an endpoint spec"))
}

fn node_json(registry: &SlotShapeRegistry, node: &NodeDef) -> Result<String> {
    node.write_json(registry)
        .map_err(|error| anyhow::anyhow!("{error}"))
        .context("serializing a generated workload node")
}

fn file(name: &str, contents: String) -> WorkloadFile {
    WorkloadFile {
        name: name.to_string(),
        contents,
    }
}

fn bindings<const N: usize>(entries: [(&str, BindingDef); N]) -> BindingDefs {
    let mut map = VecMap::new();
    for (slot, binding) in entries {
        map.insert(slot.to_string(), binding);
    }
    BindingDefs::new(map)
}

fn source(channel: &str) -> BindingDef {
    BindingDef::source(BindingRef::Bus(BusSlotRef::new(ChannelName(
        channel.to_string(),
    ))))
}

fn target(channel: &str) -> BindingDef {
    BindingDef::target(BindingRef::Bus(BusSlotRef::new(ChannelName(
        channel.to_string(),
    ))))
}

#[cfg(test)]
mod tests {
    use lpc_mapping::resolve;

    use super::*;

    fn workload(led_count: u32) -> BenchWorkload {
        BenchWorkload::new(
            led_count,
            HwEndpointSpec::parse("ws281x:rmt:D0").expect("endpoint"),
        )
    }

    /// The LED count reaches both places it has to: the fixture render size
    /// and the map2d grid. They disagreeing is the whole reason this is
    /// generated.
    #[test]
    fn led_count_lands_in_the_fixture_and_the_mapping() {
        let files = workload(30).files().unwrap();
        let by_name = |name: &str| {
            files
                .iter()
                .find(|file| file.name == name)
                .unwrap_or_else(|| panic!("{name} generated"))
                .contents
                .clone()
        };

        let fixture: serde_json::Value = serde_json::from_str(&by_name("fixture.json")).unwrap();
        assert_eq!(fixture["render_size"]["width"], 30);
        assert_eq!(fixture["render_size"]["height"], 1);
        assert_eq!(fixture["mapping"]["source"], "fixture.map2d.json");

        let doc = Map2dDoc::from_json(&by_name("fixture.map2d.json")).unwrap();
        let resolved = resolve(&doc).unwrap();
        assert_eq!(resolved.lamps.len(), 30);

        let output: serde_json::Value = serde_json::from_str(&by_name("output.json")).unwrap();
        assert_eq!(output["endpoint"], "ws281x:rmt:D0");
    }

    /// Every generated node file parses back as the node kind it claims,
    /// through the same reader the server uses.
    #[test]
    fn generated_nodes_read_back() {
        let registry = SlotShapeRegistry::default();
        for file in workload(64).files().unwrap() {
            if !file.name.ends_with(".json") || file.name.ends_with(".map2d.json") {
                continue;
            }
            NodeDef::read_json(&registry, &file.contents)
                .unwrap_or_else(|error| panic!("{} should read back: {error}", file.name));
        }
    }

    /// One lamp per texel column: the canvas aspect must follow the strip, or
    /// the aspect-fit collapses every lamp onto the same column.
    #[test]
    fn lamps_fit_one_per_texel_column() {
        let led_count = 12;
        let doc = workload(led_count).map2d_doc();
        let resolved = resolve(&doc).unwrap();
        let fitted =
            lpc_mapping::fit_points(&resolved.positions(), doc.canvas_bounds(), led_count, 1)
                .unwrap();

        let mut columns: Vec<u32> = fitted
            .iter()
            .map(|[x, _]| (x * led_count as f32) as u32)
            .collect();
        columns.dedup();
        assert_eq!(
            columns.len(),
            led_count as usize,
            "each lamp should land in its own column, got {columns:?}"
        );
    }

    #[test]
    fn rejects_an_empty_strip() {
        assert!(workload(0).files().is_err());
    }

    /// The pin comes from the board manifest and skips the pins the board
    /// does not name or has reserved.
    #[test]
    fn endpoint_skips_unnamed_and_reserved_pins() {
        let board = HardwareManifestFile::read_json(
            r#"{
  "id": "test/board",
  "target": "esp32c6",
  "vendor": "test",
  "product": "Board",
  "gpio": [
    {
      "address": "/gpio/0",
      "display_label": "GPIO0",
      "capabilities": ["gpio-output", "gpio-input"]
    },
    {
      "address": "/gpio/1",
      "display_label": "D1",
      "capabilities": ["gpio-output"],
      "reserved_reason": "crashed during calibration"
    },
    {
      "address": "/gpio/2",
      "display_label": "D2",
      "capabilities": ["gpio-input"]
    },
    {
      "address": "/gpio/3",
      "display_label": "D3",
      "capabilities": ["gpio-output", "gpio-input"]
    }
  ],
  "resource": [
    {
      "address": "/rmt/ws281x0",
      "display_label": "RMT WS281x 0",
      "capabilities": ["rmt", "ws281x-output"]
    }
  ]
}"#,
        )
        .unwrap();

        assert_eq!(workload_endpoint(&board).unwrap().as_str(), "ws281x:rmt:D3");
    }

    /// A board with no WS281x timing resource is not benched: the manifest is
    /// the allowlist, so there is no pin to fall back to.
    #[test]
    fn endpoint_refuses_a_board_without_ws281x() {
        let board = HardwareManifestFile::read_json(
            r#"{
  "id": "test/no-leds",
  "target": "esp32c6",
  "vendor": "test",
  "product": "Board",
  "gpio": [
    {
      "address": "/gpio/3",
      "display_label": "D3",
      "capabilities": ["gpio-output"]
    }
  ]
}"#,
        )
        .unwrap();

        let error = workload_endpoint(&board).unwrap_err().to_string();
        assert!(error.contains("ws281x-output"), "{error}");
    }

    /// The checked-in bench boards each resolve to a real data pin.
    #[test]
    fn bench_boards_resolve_to_a_pin() {
        let repo_root = crate::commands::firmware::build_def::find_repo_root().unwrap();
        for (board_id, expected) in [
            ("seeed/xiao-esp32-c6", "ws281x:rmt:D0"),
            ("seeed/xiao-esp32-s3-plus", "ws281x:rmt:D0"),
            ("domraem/dom-z-102", "ws281x:rmt:IO2"),
        ] {
            let (vendor, product) = board_id.split_once('/').unwrap();
            let path = repo_root
                .join("lp-core/lpc-hardware/boards")
                .join(vendor)
                .join(format!("{product}.json"));
            let board =
                HardwareManifestFile::read_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(workload_endpoint(&board).unwrap().as_str(), expected);
        }
    }
}
