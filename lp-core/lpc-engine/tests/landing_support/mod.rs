//! Where every fixture lamp of a project lands: load it the way the server
//! does, render a few frames, and read each output's published fragments,
//! cut into ports by the slicing its wires register
//! ([`EngineServices::planned_wire_slices_for_landing`]). Nothing here reads
//! a map2d or a patch document.
//!
//! Shared by `upgrade_landing` (every project against its format-11 golden)
//! and `creatable_projects_land` (every project the templates create).

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::NodeEntryState;
use lpc_engine::nodes::OutputFragment;
use lpc_engine::{ButtonService, EngineServices, ProjectLoader, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::{FixtureState, NodeDef, NodeDefState, NodeId, TreePath};
use lpfs::{LpFs, LpFsStd};

/// Ticks before placement is read: output names register on the first, the
/// fixtures' patches resolve against them on the next, and the plan settles
/// by the third.
pub const TICKS: usize = 4;

/// Where every fixture lamp of one project landed, keyed by the fixture's
/// node path (its definition file, from the project root).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLanding {
    pub project_format: u64,
    pub refused: Option<String>,
    pub fixtures: BTreeMap<String, FixtureLanding>,
}

/// One fixture's lamps: where each one lands (none = dark; more than one if
/// two runs place it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureLanding {
    pub lamps: u32,
    pub landings: Vec<Vec<Landing>>,
    pub dark: Vec<(u32, u32)>,
}

/// One lamp on one wire: the output node (its definition file), the port
/// key, and the first channel (sample) of the lamp within that port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Landing {
    pub output: String,
    pub port: u32,
    pub channel: u32,
}

pub fn at(output: &str, port: u32, channel: u32) -> Landing {
    Landing {
        output: String::from(output),
        port,
        channel,
    }
}

/// Load `project` the way the server does, render it, and read where every
/// fixture lamp landed.
pub fn project_landing(project: &Path) -> ProjectLanding {
    let fs = LpFsStd::new(project.to_path_buf());
    fs_landing(&fs, project_format(project), project)
}

/// [`project_landing`] for a project already in a filesystem: `label` names
/// it in failures.
pub fn fs_landing(fs: &dyn LpFs, project_format: u64, label: &Path) -> ProjectLanding {
    let project = label;
    let refused = |why: String| ProjectLanding {
        project_format,
        refused: Some(why),
        fixtures: BTreeMap::new(),
    };
    // The host's virtual buttons and radio, as a simulated board has: a
    // project with a radio node would otherwise fail every frame here and
    // nowhere else.
    let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
        HwRegistry::new(default_esp32c6_hardware_manifest()),
    )));
    let button_service: Rc<dyn ButtonService> = hardware.clone();
    let radio_service: Rc<dyn RadioService> = hardware;
    let mut services = EngineServices::new(TreePath::parse("/project.show").expect("root path"));
    services.set_button_service(Some(button_service));
    services.set_radio_service(Some(radio_service));
    let mut rt = match ProjectLoader::load_from_root(fs, services) {
        Ok(rt) => rt,
        Err(error) => return refused(format!("{error:?}")),
    };
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    // A failed frame does not stop the server, so it does not stop this: only
    // a project whose LAST frame still fails is refused — its output never
    // published a plan to read.
    let mut last = Ok(());
    for _ in 0..TICKS {
        last = rt.tick(16);
    }
    if let Err(error) = last {
        return refused(format!("every frame fails: {error:?}"));
    }
    read_landing(&rt, project_format, project)
}

/// Read the placement of a loaded, ticked project.
pub fn read_landing(
    rt: &LoadedProjectRuntime,
    project_format: u64,
    project: &Path,
) -> ProjectLanding {
    let engine = rt.engine();
    let registry = rt.registry();
    let index = engine.project_runtime_index();

    // Every fixture and output node, by runtime id, with its definition.
    let mut fixture_paths: BTreeMap<NodeId, String> = BTreeMap::new();
    let mut outputs = Vec::new();
    for (use_location, node) in registry.inventory().tree.nodes.iter() {
        let Some(id) = index.node_id(use_location) else {
            continue;
        };
        let Some(entry) = registry.def(&node.def_location) else {
            continue;
        };
        let NodeDefState::Loaded(def) = &entry.state else {
            continue;
        };
        let path = String::from(node.def_location.artifact.file_path().as_str());
        match def {
            NodeDef::Fixture(_) => {
                assert!(
                    !fixture_paths.values().any(|known| *known == path),
                    "{}: fixture definition {path} is used twice; its path does not \
                     name one fixture",
                    project.display()
                );
                fixture_paths.insert(id, path);
            }
            NodeDef::Output(config) => outputs.push((
                id,
                path,
                EngineServices::planned_wire_slices_for_landing(config),
            )),
            _ => {}
        }
    }

    let mut fixtures: BTreeMap<NodeId, FixtureLanding> = BTreeMap::new();
    for (id, path) in &fixture_paths {
        let lamps = fixture_lamps(rt, *id)
            .unwrap_or_else(|| panic!("{}: fixture {path} has no product", project.display()));
        fixtures.insert(
            *id,
            FixtureLanding {
                lamps,
                landings: vec![Vec::new(); lamps as usize],
                dark: Vec::new(),
            },
        );
    }

    for (id, output_path, wires) in &outputs {
        let fragments = output_fragments(rt, *id);
        assert_uncontested(project, output_path, fragments);
        for fragment in fragments {
            let fixture_id = fragment.product.node();
            let fixture = fixtures.get_mut(&fixture_id).unwrap_or_else(|| {
                panic!(
                    "{}: output {output_path} places a product of node {fixture_id:?}, \
                     which is not a fixture",
                    project.display()
                )
            });
            place_fragment(project, output_path, wires, fragment, fixture);
        }
    }

    let fixtures = fixtures
        .into_iter()
        .map(|(id, mut fixture)| {
            fixture.dark = dark_ranges(&fixture.landings);
            for lands in &mut fixture.landings {
                lands.sort();
            }
            (fixture_paths[&id].clone(), fixture)
        })
        .collect();
    ProjectLanding {
        project_format,
        refused: None,
        fixtures,
    }
}

/// Every lamp of one placed fragment, onto the wire its output sample is
/// cut into. Reversal first, then rotation within the window — the order the
/// output applies them (`reverse_lamps`, then `rotate_lamps`'s
/// `rotate_right`). A lamp whose sample no wire carries lands nowhere.
pub fn place_fragment(
    project: &Path,
    output_path: &str,
    wires: &[(u32, u32, Option<u32>)],
    fragment: &OutputFragment,
    fixture: &mut FixtureLanding,
) {
    let window = fragment.len_samples / 3;
    let first_source = fragment.source_offset_samples / 3;
    assert!(
        fragment.len_samples % 3 == 0
            && fragment.offset_samples % 3 == 0
            && fragment.source_offset_samples % 3 == 0
            && fragment.rotation_samples % 3 == 0,
        "{}: output {output_path} has a fragment off a lamp boundary: {fragment:?}",
        project.display()
    );
    assert!(
        first_source + window <= fixture.lamps,
        "{}: output {output_path} has a fragment past its product's end (its lamps \
         would stay dark): {fragment:?}",
        project.display()
    );
    let rotation = if window == 0 {
        0
    } else {
        (fragment.rotation_samples / 3) % window
    };
    for j in 0..window {
        let flipped = if fragment.reversed { window - 1 - j } else { j };
        let slot = (flipped + rotation) % window;
        let sample = fragment.offset_samples + slot * 3;
        let wire = wires
            .iter()
            .find(|(_, start, len)| sample >= *start && len.is_none_or(|len| sample < start + len));
        if let Some((port, start, _)) = wire {
            fixture.landings[(first_source + j) as usize].push(Landing {
                output: String::from(output_path),
                port: *port,
                channel: sample - start,
            });
        }
    }
}

/// The output zeroes a range two fragments claim; the goldens do not model
/// it, so a project that has one stops the recorder.
pub fn assert_uncontested(project: &Path, output_path: &str, fragments: &[OutputFragment]) {
    let mut claimed: Vec<(u32, u32)> = fragments
        .iter()
        .filter(|fragment| fragment.len_samples > 0)
        .map(|fragment| (fragment.offset_samples, fragment.end_samples()))
        .collect();
    claimed.sort_unstable();
    for pair in claimed.windows(2) {
        assert!(
            pair[1].0 >= pair[0].1,
            "{}: output {output_path} has contested samples {:?} and {:?}; the landing \
             goldens do not model a contested range",
            project.display(),
            pair[0],
            pair[1]
        );
    }
}

/// The fragments an output published on its last rendered frame.
pub fn output_fragments(rt: &LoadedProjectRuntime, id: NodeId) -> &[OutputFragment] {
    let entry = rt.engine().tree().get(id).expect("output entry");
    match entry.state.value() {
        NodeEntryState::Alive(node) => node.runtime_output_fragments(),
        _ => &[],
    }
}

/// A fixture's lamp count: its published control product's extent.
pub fn fixture_lamps(rt: &LoadedProjectRuntime, id: NodeId) -> Option<u32> {
    let entry = rt.engine().tree().get(id)?;
    let NodeEntryState::Alive(node) = entry.state.value() else {
        return None;
    };
    let state = node
        .runtime_state_slots()?
        .as_any()
        .downcast_ref::<FixtureState>()?;
    Some(state.output.value().preferred_extent().sample_count() / 3)
}

/// `[first lamp, count]` for every run of lamps that landed nowhere.
pub fn dark_ranges(landings: &[Vec<Landing>]) -> Vec<(u32, u32)> {
    let mut dark: Vec<(u32, u32)> = Vec::new();
    for (lamp, lands) in landings.iter().enumerate() {
        if !lands.is_empty() {
            continue;
        }
        let lamp = lamp as u32;
        match dark.last_mut() {
            Some((first, count)) if *first + *count == lamp => *count += 1,
            _ => dark.push((lamp, 1)),
        }
    }
    dark
}

impl FixtureLanding {
    /// Where `lamp` lands, sorted.
    pub fn lands(&self, lamp: u32) -> Vec<Landing> {
        self.landings[lamp as usize].clone()
    }
}

impl ProjectLanding {
    pub fn fixture(&self, path: &str) -> &FixtureLanding {
        self.fixtures
            .get(path)
            .unwrap_or_else(|| panic!("no fixture {path}; have {:?}", self.fixtures.keys()))
    }
}

/// The `format` of `project`'s manifest.
pub fn project_format(project: &Path) -> u64 {
    let text = std::fs::read_to_string(project.join("project.json")).expect("project.json");
    let value: serde_json::Value = serde_json::from_str(&text).expect("project.json parses");
    value["format"].as_u64().expect("project.json has a format")
}
