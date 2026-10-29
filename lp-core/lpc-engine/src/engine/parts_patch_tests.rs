//! The engine's statements for a format-6 fixture (mapping rebuild M2 P3):
//! through a real [`Engine`] with real `OutputNode`s, a fixture holding a
//! parts mapping lights each lamp on the channel the patcher gave it.
//!
//! **The permutation oracle** (`tests/output_patch_reflow.rs`'s): every
//! claim is about WHICH of the fixture's own lamps sits where, never about
//! literal colors. Lamp k's samples are read from the fixture's own control
//! product (a control-product probe: the fixture's lamp order, untouched by
//! any patching), and checked pairwise distinct and lit, so a placement
//! can't pass by a color collision. A shader paints each lamp by its
//! position, so distinct lamps are distinct colors. Then every wire lamp is checked against
//! [`Patching::landing`], computed here from the same mapping, and every
//! lit wire lamp is accounted for: nothing lands that the patcher didn't
//! place.
//!
//! Each project is loaded from memory as a device loads one (authored
//! outputs with names and ports, real bindings), and each format-6
//! fixture's runtime is then replaced by one built with
//! [`FixtureNode::with_parts_mapping`]: no file reads format 6 until M2 P4.
//! The journeys use the real projects' numbers (M1 P2's journeys).

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use lpc_mapping::part::{
    Circle, Mapping, Part, PartId, PartKind, Path, Repeat, Shape, TreeId, render,
};
use lpc_mapping::patching::{
    Address, Amount, ChannelOp, Direction, Flow, OutputName, OutputPorts, Override, Patching, Port,
    PortKey, PortOp, Start, patch_mapping,
};
use lpc_model::nodes::fixture::{FixtureSamplingConfig, MappingConfig};
use lpc_model::{
    ArtifactLocation, LpValue, MutationOp, NodeId, NodeName, NodeRuntimeStatus, Revision, SlotEdit,
    SlotPath, TreePath, current_revision,
};
use lpc_registry::ParseCtx;
use lpfs::{AsLpPath, LpFs, LpFsMemory};

use lpc_model::{ControlExtent, ControlProduct};
use lpc_wire::{
    ControlProductProbeRequest, ControlProductProbeResult, ProjectProbeRequest, ProjectProbeResult,
    ProjectReadRequest, RevisionGateRead, WireChannelSampleFormat,
};

use super::test_support::read_probe_results;
use super::{EngineServices, LoadedProjectRuntime, ProjectLoader};
use crate::node::NodeEntryState;
use crate::nodes::{FixtureNode, FixturePartsSource};

/// The circular display, as soldered: rings r1, r2 on port 0 from 0; r3
/// reversed on port 1 from 0; r4, r5 following on port 1.
#[test]
fn the_circular_display_as_soldered() {
    let mut display = circular_display();
    display.overrides.insert(
        id("circle1/r3"),
        Override {
            port: Some(PortOp::set(1)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            direction: Some(Direction::Reverse),
            ..Override::default()
        },
    );
    let outputs = vec![Out::named("main", &[Some(64); 5])];
    let mut rig = Rig::load(vec![Fix::parts("display", display.clone())], outputs);
    rig.settle();

    let patching = rig.assert_lands("display", &display);
    assert_eq!(at(&patching, "circle1/r2", 0), ("main", 0, 24));
    assert_eq!(
        at(&patching, "circle1/r3", 0),
        ("main", 1, 11),
        "r3 reversed"
    );
    assert_eq!(at(&patching, "circle1/r3", 11), ("main", 1, 0));
    assert_eq!(at(&patching, "circle1/r4", 0), ("main", 1, 12));
    assert_eq!(at(&patching, "circle1/r5", 0), ("main", 1, 20));
    assert_eq!(rig.status("display"), None);
}

/// The circular display, one ring per port by the circle's flow: ring k on
/// port k−1 at lamp 0.
#[test]
fn the_circular_display_one_ring_per_port() {
    let mut display = circular_display();
    display.parts[0].flow = Flow::OnePerPort { from: PortKey(0) };
    let outputs = vec![Out::named("main", &[Some(64); 5])];
    let mut rig = Rig::load(vec![Fix::parts("display", display.clone())], outputs);
    rig.settle();

    let patching = rig.assert_lands("display", &display);
    for k in 1..=5u32 {
        assert_eq!(
            at(&patching, &format!("circle1/r{k}"), 0),
            ("main", k - 1, 0),
            "ring {k}"
        );
    }
}

/// The dome slice, sized to the design target (five copies of a 100-lamp
/// path, ports of 100): copy k on port k; copy 3 overridden to port 4 at 0
/// lands there and copy 4 follows it.
#[test]
fn the_dome_slice() {
    let slice = dome_slice();
    let outputs = vec![Out::named(
        "out",
        &[Some(100), Some(100), Some(100), Some(100), None],
    )];
    let mut rig = Rig::load(vec![Fix::parts("dome", slice.clone())], outputs.clone());
    rig.settle();
    let patching = rig.assert_lands("dome", &slice);
    for k in 0..5u32 {
        let copy = if k == 0 {
            String::from("path1")
        } else {
            format!("path1@{k}")
        };
        assert_eq!(at(&patching, &copy, 0), ("out", k, 0), "copy {k}");
    }

    let mut moved = slice;
    moved.overrides.insert(
        id("path1@3"),
        Override {
            port: Some(PortOp::set(4)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            ..Override::default()
        },
    );
    let mut rig = Rig::load(vec![Fix::parts("dome", moved.clone())], outputs);
    rig.settle();
    let patching = rig.assert_lands("dome", &moved);
    assert_eq!(at(&patching, "path1@3", 0), ("out", 4, 0));
    assert_eq!(at(&patching, "path1@4", 0), ("out", 4, 100));
    assert_eq!(rig.lit("out_out"), 500, "every lamp of the slice lights");
}

/// The PLAYFUL choker: twelve paths end to end on one port; `f-lower` set
/// to lamp 60 restarts the run there.
#[test]
fn the_playful_choker() {
    let choker = playful_choker();
    let outputs = vec![Out::named("main", &[None])];
    let mut rig = Rig::load(vec![Fix::parts("choker", choker.clone())], outputs.clone());
    rig.settle();
    let patching = rig.assert_lands("choker", &choker);
    assert_eq!(at(&patching, "f-lower", 0), ("main", 0, 42));
    assert_eq!(at(&patching, "l2", 0), ("main", 0, 65));

    let mut restarted = choker;
    restarted.overrides.insert(
        id("f-lower"),
        Override {
            channel: Some(ChannelOp::Set(Amount::lamps(60))),
            ..Override::default()
        },
    );
    let mut rig = Rig::load(vec![Fix::parts("choker", restarted.clone())], outputs);
    rig.settle();
    let patching = rig.assert_lands("choker", &restarted);
    assert_eq!(
        at(&patching, "y-right", 0),
        ("main", 0, 39),
        "before it: unchanged"
    );
    assert_eq!(at(&patching, "f-lower", 0), ("main", 0, 60));
    assert_eq!(at(&patching, "f-bar", 0), ("main", 0, 63));
    assert_eq!(at(&patching, "l2", 0), ("main", 0, 83));
}

/// Sean's eaves: `front` reversed fills port 0; `side` wraps to port 1 at 0.
#[test]
fn the_eaves() {
    let eaves = seans_eaves();
    let outputs = vec![Out::named("main", &[Some(60), None])];
    let mut rig = Rig::load(vec![Fix::parts("eaves", eaves.clone())], outputs);
    rig.settle();
    let patching = rig.assert_lands("eaves", &eaves);
    assert_eq!(
        at(&patching, "front", 0),
        ("main", 0, 59),
        "front runs 59 down to 0"
    );
    assert_eq!(at(&patching, "front", 59), ("main", 0, 0));
    assert_eq!(at(&patching, "side", 0), ("main", 1, 0));
    assert!(patching.segment(&id("side")).unwrap().wrapped);
}

/// Nothing flows across fixtures: two format-6 fixtures on one output each
/// land where their own start says.
#[test]
fn nothing_flows_across_fixtures() {
    let first = strip_at("first", 10, 10.0, Start::at("main", 0, Amount::lamps(0)));
    let second = strip_at("second", 10, 60.0, Start::at("main", 0, Amount::lamps(20)));
    let outputs = vec![Out::named("main", &[None])];
    let mut rig = Rig::load(
        vec![
            Fix::parts("one", first.clone()),
            Fix::parts("two", second.clone()),
        ],
        outputs,
    );
    rig.settle();
    rig.assert_lands("one", &first);
    rig.assert_lands("two", &second);
    let wire = rig.published("out_main");
    assert!(
        wire[10..20].iter().all(|lamp| *lamp == DARK),
        "the second never follows on from the first"
    );
    assert_eq!(rig.lit("out_main"), 20);
}

/// A format-6 fixture never lands on an output it doesn't name, even the
/// default one.
#[test]
fn a_format_6_fixture_never_lands_on_an_output_it_doesnt_name() {
    let mapping = strip("strip", 10, Start::at("b", 0, Amount::lamps(0)));
    // `a` is attached first: the default output, where an unpatched
    // fixture would flow.
    let outputs = vec![Out::named("a", &[None]), Out::named("b", &[None])];
    let mut rig = Rig::load(vec![Fix::parts("strip", mapping.clone())], outputs);
    rig.settle();
    rig.assert_lands("strip", &mapping);
    assert_eq!(rig.lit("out_a"), 0, "nothing lands on the default output");
    assert_eq!(rig.lit("out_b"), 10);
}

/// A start naming an output that doesn't read the fixture's bus channel
/// places nothing there, and the fixture says so by name.
#[test]
fn a_start_naming_an_output_off_the_channel_places_nothing_and_says_so() {
    let mapping = strip("strip", 10, Start::at("elsewhere", 0, Amount::lamps(0)));
    let outputs = vec![
        Out::named("main", &[None]),
        Out::named("elsewhere", &[None]).reading("bus:other.out"),
    ];
    // Something publishes on the other channel, so `elsewhere` has a frame.
    let mut rig = Rig::load(
        vec![
            Fix::parts("strip", mapping),
            Fix::Grid("other", 4, "bus:other.out"),
        ],
        outputs,
    );
    rig.settle();
    assert_eq!(rig.lit("out_elsewhere"), 4, "only the other channel's grid");
    assert_eq!(rig.lit("out_main"), 0);
    assert_eq!(
        rig.status("strip"),
        Some(NodeRuntimeStatus::Error(String::from(
            "the fixture's start names output \"elsewhere\", but no named output reads \
             bus:control.out"
        )))
    );
}

/// Editing a port's count, live, re-patches: what follows moves to the new
/// port boundary, on the next frame or the one after.
#[test]
fn editing_a_ports_count_live_re_patches() {
    let eaves = seans_eaves();
    let mut outputs = vec![Out::named("main", &[Some(60), None])];
    let mut rig = Rig::load(vec![Fix::parts("eaves", eaves.clone())], outputs.clone());
    rig.settle();
    rig.assert_lands("eaves", &eaves);

    rig.edit("/out_main.json", "ports[0].count.some", LpValue::U32(70));
    outputs[0].ports = vec![(0, Some(70)), (1, None)];
    rig.outputs = outputs;
    rig.tick(2);
    let patching = rig.assert_lands("eaves", &eaves);
    assert_eq!(
        at(&patching, "side", 0),
        ("main", 1, 0),
        "side doesn't fit the 10 lamps left on port 0, and starts port 1"
    );
    // Port 1 now begins at wire lamp 70: side's first lamp is there.
    let wire = rig.published("out_main");
    assert_eq!(
        wire[60..70],
        [DARK; 10],
        "the 10 lamps port 0 grew by are dark"
    );
    assert_ne!(wire[70], DARK);
}

/// Renaming the output a fixture names leaves its lamps dark and says so;
/// naming it back lights them.
#[test]
fn renaming_the_named_output_darkens_and_naming_it_back_lights() {
    let mapping = strip("strip", 10, Start::at("main", 0, Amount::lamps(0)));
    let outputs = vec![Out::named("main", &[None])];
    let mut rig = Rig::load(vec![Fix::parts("strip", mapping.clone())], outputs);
    rig.settle();
    assert_eq!(rig.lit("out_main"), 10);

    rig.edit(
        "/out_main.json",
        "name.some",
        LpValue::String(String::from("lounge")),
    );
    rig.tick(2);
    assert_eq!(rig.lit("out_main"), 0, "nothing names \"lounge\"");
    let Some(NodeRuntimeStatus::Error(message)) = rig.status("strip") else {
        panic!("the fixture says why it is dark");
    };
    assert!(message.contains("\"main\""), "{message}");

    rig.edit(
        "/out_main.json",
        "name.some",
        LpValue::String(String::from("main")),
    );
    rig.tick(2);
    rig.assert_lands("strip", &mapping);
    assert_eq!(rig.lit("out_main"), 10);
    assert_eq!(rig.status("strip"), None);
}

/// A format ≤ 5 fixture auto-flowing on the same output starts after the
/// format-6 fixture's last placed lamp (lean Q50).
#[test]
fn a_format_5_fixture_auto_flows_after_the_format_6_fixtures_last_lamp() {
    let mapping = strip("strip", 12, Start::at("main", 0, Amount::lamps(0)));
    let outputs = vec![Out::named("main", &[None])];
    let grid = {
        let mut rig = Rig::load(
            vec![Fix::Grid("grid", 6, "bus:control.out")],
            outputs.clone(),
        );
        rig.settle();
        rig.published("out_main")
    };
    assert_eq!(grid.len(), 6);

    let mut rig = Rig::load(
        vec![
            Fix::Grid("grid", 6, "bus:control.out"),
            Fix::parts("strip", mapping.clone()),
        ],
        outputs,
    );
    rig.settle();
    rig.assert_lands("strip", &mapping);
    let wire = rig.published("out_main");
    assert_eq!(wire[12..18], grid[..], "the grid flows on after lamp 11");
}

/// The fixture's status names the first problem, and is clear when there
/// is none.
#[test]
fn the_status_names_the_first_problem_and_is_clear_when_there_is_none() {
    let outputs = vec![Out::named("main", &[Some(4), Some(4)])];
    let fine = strip("fine", 4, Start::at("main", 0, Amount::lamps(0)));
    let mut rig = Rig::load(vec![Fix::parts("fine", fine)], outputs.clone());
    rig.settle();
    assert_eq!(rig.status("fine"), None);

    // No start, and a stale override: one problem says anything.
    let mut lost = strip("lost", 4, Start::KEEP);
    lost.overrides.insert(id("gone"), Override::default());
    let mut rig = Rig::load(vec![Fix::parts("lost", lost)], outputs.clone());
    rig.settle();
    assert_eq!(
        rig.status("lost"),
        Some(NodeRuntimeStatus::Error(String::from(
            "the fixture has no start: nothing says which output and port its lamps begin on"
        )))
    );

    // Set to port 1 lamp 2, eight lamps: held where set, clipped (F9), and
    // the next part runs past the last port.
    let mut over = strip("over", 8, Start::at("main", 1, Amount::lamps(2)));
    over.parts.push(Part::new(
        pid("more"),
        PartKind::Shape(Shape::Path(Path::through(&[[0.0, 40.0], [90.0, 40.0]], 3))),
    ));
    let mut rig = Rig::load(vec![Fix::parts("over", over)], outputs);
    rig.settle();
    let Some(NodeRuntimeStatus::Error(message)) = rig.status("over") else {
        panic!("over has problems");
    };
    assert!(
        message.starts_with("over doesn't fit what is left of port 1"),
        "{message}"
    );
    assert!(message.ends_with("(and 1 more)"), "{message}");
}

// --- the oracle -----------------------------------------------------------

/// An unlit lamp.
const DARK: [u16; 3] = [0, 0, 0];

impl Rig {
    /// Assert every lamp of `mapping` (as fixture `fixture` holds it) is
    /// where [`Patching::landing`] says on the rig's outputs, and that the
    /// rig's outputs light nothing else of it. Returns the patching.
    fn assert_lands(&mut self, fixture: &str, mapping: &Mapping) -> Patching {
        let tree = render(mapping).expect("render");
        let own = self.control_product(fixture, tree.positions().len() as u32);
        let ports: Vec<OutputPorts> = self
            .outputs
            .iter()
            .filter(|out| out.channel == "bus:control.out")
            .filter_map(|out| {
                Some(OutputPorts {
                    output: OutputName::new(out.name?),
                    ports: out
                        .ports
                        .iter()
                        .map(|(key, lamps)| Port {
                            key: PortKey(*key),
                            channels: lamps.map(|n| n * 3),
                        })
                        .collect(),
                })
            })
            .collect();
        let patching = patch_mapping(mapping, &tree, &ports);
        let mut placed: BTreeMap<String, usize> = BTreeMap::new();
        for segment in tree.segments() {
            for j in 0..segment.count() {
                let k = (segment.lamps.start + j) as usize;
                let Some(address) = patching.landing(&segment.id, j) else {
                    continue;
                };
                let out = self.out_named(address.output.as_str());
                let wire_lamp = (out.port_start(address.port.0) + address.channel) / 3;
                let wire = self.published(&out.node);
                assert_eq!(
                    wire.get(wire_lamp as usize),
                    Some(&own[k]),
                    "{fixture}: lamp {j} of {} (fixture lamp {k}) belongs at wire lamp \
                     {wire_lamp} of {}",
                    segment.id,
                    out.node
                );
                *placed.entry(out.node.clone()).or_default() += 1;
            }
        }
        // Nothing else of this fixture's lights anywhere.
        for out in &self.outputs {
            let mine = self
                .published(&out.node)
                .iter()
                .filter(|lamp| own.contains(lamp))
                .count();
            assert_eq!(
                mine,
                placed.get(&out.node).copied().unwrap_or(0),
                "{fixture}: lamps lit on {} that the patcher didn't place there",
                out.node
            );
        }
        patching
    }
}

/// Where lamp `j` of segment `segment` landed, as `(output, port, lamp)`.
fn at(patching: &Patching, segment: &str, j: u32) -> (&'static str, u32, u32) {
    let Address {
        output,
        port,
        channel,
    } = patching
        .landing(&id(segment), j)
        .unwrap_or_else(|| panic!("{segment} lamp {j} landed"));
    let output: &'static str = match output.as_str() {
        "main" => "main",
        "out" => "out",
        "a" => "a",
        "b" => "b",
        other => panic!("unexpected output {other}"),
    };
    (output, port.0, channel / 3)
}

// --- the journeys' mappings --------------------------------------------------

/// Yona's circular display: one circle, rings of 24, 16, 12, 8 and a
/// one-lamp centre (`r1`–`r5`), outside in; the fixture starts at port 0,
/// lamp 0 of `main`.
fn circular_display() -> Mapping {
    let mut m = Mapping::new();
    let circle = m.new_part(PartKind::Shape(Shape::Circle(Circle::new(
        [50.0, 50.0],
        &[(40.0, 24), (32.0, 16), (24.0, 12), (16.0, 8), (8.0, 1)],
    ))));
    m.parts.push(circle);
    m.start = Start::at("main", 0, Amount::lamps(0));
    m
}

/// A dome slice at the design target: a repeat of five copies of a
/// 100-lamp path (`path1`, `path1@k`) round the centre; the fixture starts
/// at port 0, lamp 0 of `out`.
fn dome_slice() -> Mapping {
    let mut m = Mapping::new();
    let path = m.new_part(PartKind::Shape(Shape::Path(Path::through(
        &[[54.0, 50.0], [96.0, 50.0]],
        100,
    ))));
    let repeat = m.new_part(PartKind::Repeat(Repeat::new(path, [50.0, 50.0], 5)));
    m.parts.push(repeat);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m
}

/// The PLAYFUL choker (`catalog/projects/playful-choker`): twelve paths end
/// to end, ids from their names lower-cased, from lamp 0 of `main`.
fn playful_choker() -> Mapping {
    const PATHS: [(&str, u32); 12] = [
        ("p", 13),
        ("l1", 8),
        ("a", 10),
        ("a-bar", 2),
        ("y-stem", 3),
        ("y-left", 3),
        ("y-right", 3),
        ("f-lower", 3),
        ("f-bar", 2),
        ("f-upper", 5),
        ("u", 13),
        ("l2", 8),
    ];
    let mut m = Mapping::new();
    for (row, (name, count)) in PATHS.iter().enumerate() {
        let y = 4.0 + 7.5 * row as f32;
        m.parts.push(Part::new(
            pid(name),
            PartKind::Shape(Shape::Path(Path::through(&[[5.0, y], [95.0, y]], *count))),
        ));
    }
    m.start = Start::at("main", 0, Amount::lamps(0));
    m
}

/// Sean's eaves: `front` (60 lamps) along the front, `side` (50) down the
/// side, `front` wired from its far end; from lamp 0 of `main`.
fn seans_eaves() -> Mapping {
    let mut m = Mapping::new();
    m.parts.push(Part::new(
        pid("front"),
        PartKind::Shape(Shape::Path(Path::through(&[[0.0, 0.0], [59.0, 0.0]], 60))),
    ));
    m.parts.push(Part::new(
        pid("side"),
        PartKind::Shape(Shape::Path(Path::through(&[[60.0, 1.0], [60.0, 50.0]], 50))),
    ));
    m.overrides.insert(
        id("front"),
        Override {
            direction: Some(Direction::Reverse),
            ..Override::default()
        },
    );
    m.start = Start::at("main", 0, Amount::lamps(0));
    m
}

/// One straight path of `count` lamps, starting where `start` says.
fn strip(name: &str, count: u32, start: Start) -> Mapping {
    strip_at(name, count, 10.0, start)
}

/// [`strip`], drawn at height `y`: two strips at different heights are
/// different lamps (and so different colors).
fn strip_at(name: &str, count: u32, y: f32, start: Start) -> Mapping {
    let mut m = Mapping::new();
    m.parts.push(Part::new(
        pid(name),
        PartKind::Shape(Shape::Path(Path::through(&[[0.0, y], [90.0, y]], count))),
    ));
    m.start = start;
    m
}

fn pid(text: &str) -> PartId {
    PartId::new(text).expect("part id")
}

fn id(text: &str) -> TreeId {
    TreeId::parse(text).expect("tree id")
}

// --- the rig ---------------------------------------------------------------

/// One output the project holds.
#[derive(Clone)]
struct Out {
    /// Node and file name: `out_<name>`.
    node: String,
    name: Option<&'static str>,
    ports: Vec<(u32, Option<u32>)>,
    channel: &'static str,
}

impl Out {
    /// Output `name` with ports keyed 0, 1, … of these lamp counts (`None`
    /// takes the rest).
    fn named(name: &'static str, lamps: &[Option<u32>]) -> Self {
        Self {
            node: format!("out_{name}"),
            name: Some(name),
            ports: lamps
                .iter()
                .enumerate()
                .map(|(key, lamps)| (key as u32, *lamps))
                .collect(),
            channel: "bus:control.out",
        }
    }

    fn reading(mut self, channel: &'static str) -> Self {
        self.channel = channel;
        self
    }

    /// The first sample of port `key`: the authored counts before it.
    fn port_start(&self, key: u32) -> u32 {
        self.ports
            .iter()
            .take_while(|(k, _)| *k != key)
            .map(|(_, lamps)| lamps.unwrap_or(0) * 3)
            .sum()
    }

    fn json(&self) -> String {
        let name = self
            .name
            .map(|name| format!("\"name\": \"{name}\",\n"))
            .unwrap_or_default();
        let ports: Vec<String> = self
            .ports
            .iter()
            .map(|(key, count)| {
                let count = count
                    .map(|count| format!(", \"count\": {count}"))
                    .unwrap_or_default();
                format!("\"{key}\": {{ \"endpoint\": \"ws281x:local:D{key}\"{count} }}")
            })
            .collect();
        format!(
            r#"{{
"kind": "Output",
{name}"ports": {{ {} }},
"bindings": {{ "input": {{ "source": "{}" }} }},
"options": {{ "white_point": [1, 1, 1], "interpolation_enabled": false,
  "dithering_enabled": false, "lut_enabled": false }}
}}"#,
            ports.join(", "),
            self.channel
        )
    }
}

/// One fixture the project holds.
enum Fix {
    /// A format-6 fixture: loaded as a placeholder, then given its runtime
    /// with this mapping.
    Parts(&'static str, Box<Mapping>),
    /// A format ≤ 5 fixture, as loaded: a one-row grid of this many lamps,
    /// unpatched, publishing on this channel.
    Grid(&'static str, u32, &'static str),
}

impl Fix {
    fn parts(node: &'static str, mapping: Mapping) -> Self {
        Fix::Parts(node, Box::new(mapping))
    }

    fn node(&self) -> &'static str {
        match self {
            Fix::Parts(node, _) | Fix::Grid(node, ..) => node,
        }
    }
}

/// The texture every fixture renders at.
const RENDER: u32 = 64;

/// A loaded project: a position-painting shader on `bus:visual.out`, the
/// fixtures publishing on `bus:control.out`, and the outputs.
struct Rig {
    fs: LpFsMemory,
    rt: LoadedProjectRuntime,
    outputs: Vec<Out>,
}

impl Rig {
    fn load(fixtures: Vec<Fix>, outputs: Vec<Out>) -> Self {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", "{\n  \"format\": 11\n}\n");
        let mut nodes = vec![String::from("\"shader\": { \"ref\": \"./shader.json\" }")];
        write(
            &fs,
            "/shader.json",
            r#"{ "kind": "Shader", "source": "shader.glsl", "render_order": 0,
  "bindings": { "output": { "target": "bus:visual.out" } }, "consumed": {} }"#,
        );
        // Each lamp painted by where it is: distinct lamps, distinct colors,
        // and never dark (blue is always half).
        write(
            &fs,
            "/shader.glsl",
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) {\n\
             \x20   vec2 uv = pos / outputSize;\n\
             \x20   return vec4(uv.x, uv.y, 0.5, 1.0);\n\
             }\n",
        );
        for fixture in &fixtures {
            let node = fixture.node();
            nodes.push(format!("\"{node}\": {{ \"ref\": \"./{node}.json\" }}"));
            let (lamps, channel) = match fixture {
                Fix::Parts(..) => (1, "bus:control.out"),
                Fix::Grid(_, lamps, channel) => (*lamps, *channel),
            };
            write(
                &fs,
                &format!("/{node}.json"),
                &format!(
                    r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": {RENDER}, "height": {RENDER} }},
  "bindings": {{ "input": {{ "source": "bus:visual.out" }}, "output": {{ "target": "{channel}" }} }},
  "sampling": "direct",
  "diagnostic_mode": "off",
  "mapping": {{ "kind": "Map2d", "source": "{node}.map2d.json" }},
  "color_order": "rgb",
  "brightness": 1.0,
  "gamma_correction": false,
  "power": {{ "lamp_type": "ws2812b_5v", "budget_ma": 0 }}
}}"#
                ),
            );
            write(
                &fs,
                &format!("/{node}.map2d.json"),
                &format!(
                    r#"{{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, {lamps}.0, 1.0],
  "objects": [
    {{ "name": "strip", "shape": {{ "grid": {{ "origin": [0.5, 0.5], "cols": {lamps}, "rows": 1, "pitch": 1 }} }} }}
  ]
}}"#
                ),
            );
        }
        for out in &outputs {
            nodes.push(format!(
                "\"{}\": {{ \"ref\": \"./{}.json\" }}",
                out.node, out.node
            ));
            write(&fs, &format!("/{}.json", out.node), &out.json());
        }
        write(
            &fs,
            "/module.json",
            &format!(
                "{{ \"kind\": \"Module\", \"nodes\": {{ {} }} }}",
                nodes.join(", ")
            ),
        );

        let services = EngineServices::new(TreePath::parse("/parts.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        for fixture in fixtures {
            let Fix::Parts(node, mapping) = fixture else {
                continue;
            };
            let id = node_id(&rt, node);
            let runtime = FixtureNode::new(
                id,
                MappingConfig::path_points_vec(Vec::new(), 1.0),
                FixtureSamplingConfig::Direct,
                Revision::new(1),
            )
            .with_parts_mapping(
                FixturePartsSource {
                    mapping: *mapping,
                    // Every journey is drawn inside 100 × 100.
                    canvas: Some([0.0, 0.0, 100.0, 100.0]),
                    sample_diameter: 1.0,
                },
                RENDER,
                RENDER,
            );
            rt.engine_mut()
                .attach_runtime_node(id, Box::new(runtime), current_revision())
                .expect("attach the format-6 fixture");
        }
        Self { fs, rt, outputs }
    }

    /// Ticks before a buffer is read: the first frame establishes extents,
    /// and output identities settle one tick after registration.
    fn settle(&mut self) {
        self.tick(3);
    }

    fn tick(&mut self, frames: u32) {
        for _ in 0..frames {
            self.rt.tick(16).expect("tick");
        }
    }

    fn out_named(&self, name: &str) -> &Out {
        self.outputs
            .iter()
            .find(|out| out.name == Some(name))
            .unwrap_or_else(|| panic!("output {name:?}"))
    }

    /// A fixture's own control product, read directly (a control-product
    /// probe, as Studio's preview reads it): lamp k's samples, in the
    /// fixture's lamp order. Checked lit and pairwise distinct.
    fn control_product(&mut self, node: &str, lamps: u32) -> Vec<[u16; 3]> {
        let id = node_id(&self.rt, node);
        let (engine, registry) = self.rt.split_mut();
        let results = read_probe_results(
            engine,
            registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::ControlProduct(
                    ControlProductProbeRequest {
                        product: ControlProduct::new(id, 0, ControlExtent::new(1, lamps * 3)),
                        sample_format: WireChannelSampleFormat::U16,
                        geometry: RevisionGateRead::Always,
                    },
                )],
            },
        );
        let Some(ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            bytes,
            ..
        })) = results.first()
        else {
            panic!("{node}: a control preview, got {results:?}");
        };
        let colors: Vec<[u16; 3]> = bytes
            .chunks_exact(6)
            .map(|lamp| {
                let sample = |i: usize| u16::from_le_bytes([lamp[2 * i], lamp[2 * i + 1]]);
                [sample(0), sample(1), sample(2)]
            })
            .collect();
        assert_eq!(colors.len(), lamps as usize, "{node}: one color per lamp");
        for (k, lamp) in colors.iter().enumerate() {
            assert_ne!(*lamp, DARK, "{node}: lamp {k} is dark");
            assert!(
                colors[k + 1..].iter().all(|other| other != lamp),
                "{node}: lamp {k} is not distinguishable from a later one"
            );
        }
        colors
    }

    /// An output's published samples, one entry per RGB lamp.
    fn published(&self, node: &str) -> Vec<[u16; 3]> {
        let id = node_id(&self.rt, node);
        let Some(buffer) = self.rt.engine().runtime_output_sink_buffer_id(id) else {
            return Vec::new();
        };
        self.rt
            .engine()
            .runtime_buffers()
            .get(buffer)
            .expect("sink buffer")
            .value()
            .samples16()
            .map(|samples| {
                samples
                    .chunks_exact(3)
                    .map(|lamp| [lamp[0], lamp[1], lamp[2]])
                    .collect()
            })
            .unwrap_or_default()
    }

    /// How many lamps an output lights.
    fn lit(&self, node: &str) -> usize {
        self.published(node)
            .iter()
            .filter(|lamp| **lamp != DARK)
            .count()
    }

    fn status(&self, node: &str) -> Option<NodeRuntimeStatus> {
        let id = node_id(&self.rt, node);
        let entry = self.rt.engine().tree().get(id).expect("node");
        let NodeEntryState::Alive(runtime) = entry.state.value() else {
            panic!("{node} alive");
        };
        runtime.runtime_status()
    }

    /// Edit one slot of a file the way Studio's slot edit does (a registry
    /// mutation, then an apply), with no reload.
    fn edit(&mut self, file: &str, slot: &str, value: LpValue) {
        let shapes = self.rt.engine().slot_shapes().clone();
        let (engine, registry) = self.rt.split_mut();
        let result = registry
            .mutate(
                &self.fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file(file),
                    edit: SlotEdit::assign_value(SlotPath::parse(slot).expect("slot"), value),
                },
                current_revision(),
                &ParseCtx { shapes: &shapes },
            )
            .expect("edit");
        engine
            .apply_project_changes(&self.fs, registry, &result.changes)
            .expect("apply the edit");
    }
}

fn node_id(rt: &LoadedProjectRuntime, node: &str) -> NodeId {
    rt.tree()
        .lookup_sibling(rt.tree().root(), NodeName::parse(node).expect("name"))
        .unwrap_or_else(|| panic!("node {node}"))
}

fn write(fs: &LpFsMemory, path: &str, text: &str) {
    fs.write_file(path.as_path(), text.as_bytes())
        .unwrap_or_else(|error| panic!("write {path}: {error:?}"));
}
