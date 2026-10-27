//! Where every lamp lands today: the landing goldens the format-12 upgrade
//! is proven against.
//!
//! Project format 12 moves every mapping onto the patcher, and the proof
//! that no lamp moved needs a "before" that shares no code with the upgrade
//! step. This file is that "before": the live engine at project format 11
//! loads every project in the repo, renders it, and reads where each
//! fixture lamp landed — the output's own published fragments (the plan the
//! output renders from), cut into ports by the slicing the wires register
//! ([`EngineServices::planned_wire_slices_for_landing`]). Nothing here reads
//! a map2d or a patch document.
//!
//! The answer is committed as one golden per project under
//! `lp-app/lpa-upgrade/tests/landing/<project path>.landing.json`, recorded
//! before the bump and never re-blessed: a golden is replaced only by
//! deleting it by hand and saying why in the PR.
//!
//! ```bash
//! cargo test -p lpc-engine --test upgrade_landing                       # check
//! LP_LANDING_RECORD=1 cargo test -p lpc-engine --test upgrade_landing   # record missing goldens
//! ```
//!
//! Until the bump a project with no golden is listed on stderr, not failed,
//! so a project added to `main` before it does not block anyone; the bump
//! records the missing ones first.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::NodeEntryState;
use lpc_engine::nodes::OutputFragment;
use lpc_engine::{ButtonService, EngineServices, ProjectLoader, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::{FixtureState, NodeDef, NodeDefState, NodeId, TreePath};
use lpfs::LpFsStd;

/// Where the project directories live, relative to the workspace root.
const PROJECT_ROOTS: [&str; 5] = [
    "catalog",
    "projects",
    "lab/mapping/projects",
    "lp-fw/fw-browser/www",
    "lp-app/lpa-studio-core/tests/fixtures",
];

/// The upgrade corpus: only each version's `_expected/` projects (the chain's
/// output, at the current format) are in the set.
const CORPUS_ROOT: &str = "lp-app/lpa-upgrade/tests/corpus";

/// Where the goldens live, relative to the workspace root.
const GOLDEN_ROOT: &str = "lp-app/lpa-upgrade/tests/landing";

/// Ticks before placement is read: output names register on the first, the
/// fixtures' patches resolve against them on the next, and the plan settles
/// by the third.
const TICKS: usize = 4;

/// The environment variable that turns recording on.
const RECORD_ENV: &str = "LP_LANDING_RECORD";

/// The engine lands every lamp of every recorded project as its golden says
/// (and this branch records every project in the repo).
#[test]
fn the_engine_lands_every_lamp_as_its_golden_says() {
    let workspace = workspace_dir();
    let recording = std::env::var_os(RECORD_ENV).is_some();
    let recorded_at = if recording {
        git_head(&workspace)
    } else {
        String::new()
    };

    let mut failures = Vec::new();
    let mut missing = Vec::new();
    let mut totals = Totals::default();
    for project in project_dirs(&workspace) {
        let relative = relative_path(&workspace, &project);
        let landing = project_landing(&project);
        totals.add(&relative, &landing);
        let golden = golden_path(&workspace, &relative);
        match std::fs::read_to_string(&golden) {
            Ok(text) => {
                let recorded = ProjectLanding::from_golden(&text)
                    .unwrap_or_else(|why| panic!("{}: unreadable golden: {why}", golden.display()));
                if let Some(difference) = recorded.first_difference(&landing) {
                    failures.push(format!("{relative}: {difference}"));
                }
            }
            Err(_) if recording => {
                let text = landing.to_golden(&recorded_at);
                match record_golden(&golden, &text) {
                    Recorded::Written => eprintln!("recorded {}", golden.display()),
                    Recorded::AlreadyThere => {
                        panic!("{}: appeared while recording", golden.display())
                    }
                }
            }
            Err(_) => missing.push(relative),
        }
    }

    eprintln!("{totals}");
    for relative in &missing {
        eprintln!("no landing golden yet: {relative} (record with {RECORD_ENV}=1)");
    }
    assert!(
        failures.is_empty(),
        "the engine no longer lands these projects' lamps where their goldens say:\n{}",
        failures.join("\n")
    );
}

/// Recording never overwrites a golden.
#[test]
fn recording_never_overwrites_a_golden() {
    let dir = std::env::temp_dir().join(format!(
        "upgrade-landing-never-overwrites-{}",
        std::process::id()
    ));
    let path = dir.join("nested/project.landing.json");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(record_golden(&path, "first\n"), Recorded::Written);
    assert_eq!(record_golden(&path, "second\n"), Recorded::AlreadyThere);
    assert_eq!(
        std::fs::read_to_string(&path).expect("golden"),
        "first\n",
        "the second recording left the first golden alone"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// small-dome's band-c copy 0 starts at port 4, lamp 119 of output "1",
/// rotated 40: its first lamp (fixture lamp 2380) sits 40 lamps into the
/// window, and its 80th wraps to the window's start.
///
/// From `dome.patch.json`: `["/band-c/0", 0, 1071, "", 40]` — wire lamp 1071
/// of output "1" (`out_a.json`, ports of 238 lamps: port 4 starts at 952).
#[test]
fn small_dome_band_c_copy_0_starts_at_port_4_lamp_119_rotated_40() {
    let landing = repo_landing("catalog/projects/small-dome");
    let dome = landing.fixture("/dome/dome.json");
    assert_eq!(
        dome.lamps, 5950,
        "fifty 119-lamp panels; the door is its own fixture"
    );
    assert_eq!(
        dome.lands(2380),
        vec![at("/out_a.json", 4, (119 + 40) * 3)],
        "band-c copy 0's first lamp, 40 lamps into its window"
    );
    assert_eq!(
        dome.lands(2380 + 78),
        vec![at("/out_a.json", 4, (119 + 118) * 3)],
        "the 79th lamp ends the window"
    );
    assert_eq!(
        dome.lands(2380 + 79),
        vec![at("/out_a.json", 4, 119 * 3)],
        "the 80th lamp wraps to the window's start"
    );
}

/// small-dome's band-b copy 2 lands reversed at wire lamp 595 of output
/// "Box 2", and the door's 360 lamps take output "1"'s port-12 tail,
/// rotated one leg.
///
/// From `dome.patch.json` `["/band-b/2", 1, 595, "r"]` (fixture lamps
/// 2023..2142) and `doors.patch.json`.
#[test]
fn small_dome_band_b_copy_2_is_reversed_on_box_2() {
    let landing = repo_landing("catalog/projects/small-dome");
    let dome = landing.fixture("/dome/dome.json");
    // Box 2's ports are 238 lamps too: wire lamp 595 is port 2, lamp 119.
    assert_eq!(
        dome.lands(2023),
        vec![at("/out_b.json", 2, (119 + 118) * 3)],
        "the copy's first lamp is the window's last"
    );
    assert_eq!(
        dome.lands(2023 + 118),
        vec![at("/out_b.json", 2, 119 * 3)],
        "its last lamp is the window's first"
    );
    let doors = landing.fixture("/doors/doors.json");
    assert_eq!(doors.lamps, 360);
    assert_eq!(
        doors.dark,
        Vec::<(u32, u32)>::new(),
        "every door lamp lands"
    );
    // Wire lamp 2975 = port 12's lamp 119 (12 × 238 = 2856); rotated 180.
    assert_eq!(
        doors.lands(0),
        vec![at("/out_a.json", 12, (119 + 180) * 3)],
        "the door's first lamp, one leg into its window"
    );
}

/// Peach's two body legs land at wire lamps 0 and 34, the second reversed,
/// with the leaf's 12 lamps between them (wire lamps 22..34): one strand of
/// 56.
///
/// From `body/peach_body.patch.json` (ranges `[0, 22)` at 0 and `[22, 44)`
/// at 34 reversed) and `leaf/peach_leaf.patch.json` (`[0, 12)` at 22).
#[test]
fn peach_body_legs_land_at_0_and_34_the_second_reversed() {
    for project in ["catalog/projects/peach-1d", "catalog/projects/peach-2d"] {
        let landing = repo_landing(project);
        let body = landing.fixture("/body/peach_body.json");
        let leaf = landing.fixture("/leaf/peach_leaf.json");
        assert_eq!((body.lamps, leaf.lamps), (44, 12), "{project}");
        assert_eq!(body.lands(0), vec![at("/output.json", 0, 0)], "{project}");
        assert_eq!(
            body.lands(21),
            vec![at("/output.json", 0, 21 * 3)],
            "{project}"
        );
        assert_eq!(
            body.lands(22),
            vec![at("/output.json", 0, 55 * 3)],
            "{project}: the second leg starts at the strand's far end"
        );
        assert_eq!(
            body.lands(43),
            vec![at("/output.json", 0, 34 * 3)],
            "{project}: and runs back to wire lamp 34"
        );
        assert_eq!(
            leaf.lands(0),
            vec![at("/output.json", 0, 22 * 3)],
            "{project}"
        );
        assert_eq!(
            leaf.lands(11),
            vec![at("/output.json", 0, 33 * 3)],
            "{project}"
        );
        assert!(body.dark.is_empty() && leaf.dark.is_empty(), "{project}");
    }
}

/// eaves-wired is patched by hand (`flow: manual`): the front run lands
/// reversed on port 0 (D10 feeds it from its far end) and the side run starts
/// port 1; nothing is dark.
///
/// From `fixture.patch.json`: `["/front", -1, 0, "r"]`, `["/side", -1, 60]`,
/// output ports `0` (60 lamps) and `1` (the rest).
#[test]
fn eaves_wired_front_is_reversed_on_port_0_and_side_starts_port_1() {
    let landing = repo_landing("lab/mapping/projects/eaves-wired");
    let fixture = landing.fixture("/fixture.json");
    assert_eq!(fixture.lamps, 110);
    assert_eq!(fixture.lands(0), vec![at("/output.json", 0, 59 * 3)]);
    assert_eq!(fixture.lands(59), vec![at("/output.json", 0, 0)]);
    assert_eq!(fixture.lands(60), vec![at("/output.json", 1, 0)]);
    assert_eq!(fixture.lands(109), vec![at("/output.json", 1, 49 * 3)]);
    assert!(fixture.dark.is_empty());
}

/// radiance-dome's 275 rows name every one of its 275 panel copies, so no
/// lamp is dark; output "10" is named by no row and carries nothing; the
/// generator's two quirks land where its README says.
///
/// From `dome.patch.json` (275 rows, `flow: manual`) and the README:
/// `/r6-p04/2` reversed on output "5" port 10, `/r7-p05/3` rotated 27 on
/// output "7" port 12.
#[test]
fn radiance_dome_lands_every_panel_and_nothing_on_output_10() {
    let landing = repo_landing("lab/mapping/projects/radiance-dome");
    let dome = landing.fixture("/dome.json");
    assert_eq!(dome.lamps, 22_000);
    assert!(dome.dark.is_empty(), "every lamp is named by a row");
    assert!(
        dome.landings
            .iter()
            .flatten()
            .all(|landing| landing.output != "/out_10.json"),
        "no row names output 10"
    );
    // Copy k of map2d object o is fixture lamps (o·5 + k)·80 .. +80: every
    // object is a 5-way repeat of an 80-lamp panel.
    let panel = |object: u32, copy: u32| (object * 5 + copy) * 80;
    let reversed = panel(object_index("r6-p04"), 2);
    assert_eq!(dome.lands(reversed), vec![at("/out_5.json", 10, 79 * 3)]);
    assert_eq!(dome.lands(reversed + 79), vec![at("/out_5.json", 10, 0)]);
    let rotated = panel(object_index("r7-p05"), 3);
    assert_eq!(dome.lands(rotated), vec![at("/out_7.json", 12, 27 * 3)]);
    assert_eq!(dome.lands(rotated + 53), vec![at("/out_7.json", 12, 0)]);
}

/// quad-wire-oracle's one 64-lamp grid runs across its four 16-lamp ports:
/// the one segment in the repo a port boundary cuts.
///
/// From its `output.json` (ports 16, 16, 16, rest) and one grid of 8 × 8.
#[test]
fn quad_wire_oracle_grid_runs_across_four_ports() {
    let landing = repo_landing("projects/test/quad-wire-oracle");
    let (_, fixture) = landing
        .fixtures
        .iter()
        .next()
        .expect("quad-wire-oracle has a fixture");
    assert_eq!(fixture.lamps, 64);
    for port in 0..4u32 {
        let first = port * 16;
        let land = fixture.lands(first);
        assert_eq!(land.len(), 1, "lamp {first} lands once");
        assert_eq!((land[0].port, land[0].channel), (port, 0), "lamp {first}");
    }
}

// --- the recorder -----------------------------------------------------------

/// Where every fixture lamp of one project landed, keyed by the fixture's
/// node path (its definition file, from the project root).
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectLanding {
    project_format: u64,
    refused: Option<String>,
    fixtures: BTreeMap<String, FixtureLanding>,
}

/// One fixture's lamps: where each one lands (none = dark; more than one if
/// two runs place it).
#[derive(Clone, Debug, PartialEq, Eq)]
struct FixtureLanding {
    lamps: u32,
    landings: Vec<Vec<Landing>>,
    dark: Vec<(u32, u32)>,
}

/// One lamp on one wire: the output node (its definition file), the port
/// key, and the first channel (sample) of the lamp within that port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Landing {
    output: String,
    port: u32,
    channel: u32,
}

fn at(output: &str, port: u32, channel: u32) -> Landing {
    Landing {
        output: String::from(output),
        port,
        channel,
    }
}

/// Load `project` the way the server does, render it, and read where every
/// fixture lamp landed.
fn project_landing(project: &Path) -> ProjectLanding {
    let project_format = project_format(project);
    let refused = |why: String| ProjectLanding {
        project_format,
        refused: Some(why),
        fixtures: BTreeMap::new(),
    };
    let fs = LpFsStd::new(project.to_path_buf());
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
    let mut rt = match ProjectLoader::load_from_root(&fs, services) {
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
fn read_landing(rt: &LoadedProjectRuntime, project_format: u64, project: &Path) -> ProjectLanding {
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
fn place_fragment(
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
fn assert_uncontested(project: &Path, output_path: &str, fragments: &[OutputFragment]) {
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
fn output_fragments(rt: &LoadedProjectRuntime, id: NodeId) -> &[OutputFragment] {
    let entry = rt.engine().tree().get(id).expect("output entry");
    match entry.state.value() {
        NodeEntryState::Alive(node) => node.runtime_output_fragments(),
        _ => &[],
    }
}

/// A fixture's lamp count: its published control product's extent.
fn fixture_lamps(rt: &LoadedProjectRuntime, id: NodeId) -> Option<u32> {
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
fn dark_ranges(landings: &[Vec<Landing>]) -> Vec<(u32, u32)> {
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
    fn lands(&self, lamp: u32) -> Vec<Landing> {
        self.landings[lamp as usize].clone()
    }

    /// The landings as runs: `count` consecutive fixture lamps landing at
    /// channels `first, first + step, …` of one port. Canonical whatever
    /// order the fragments came in: every `(lamp, landing)` pair sorted by
    /// output, port and lamp, merged greedily, then ordered by first lamp.
    fn runs(&self) -> Vec<Run> {
        let mut pairs: Vec<(&Landing, u32)> = self
            .landings
            .iter()
            .enumerate()
            .flat_map(|(lamp, lands)| lands.iter().map(move |land| (land, lamp as u32)))
            .collect();
        pairs.sort_by(|a, b| {
            (&a.0.output, a.0.port, a.1, a.0.channel).cmp(&(
                &b.0.output,
                b.0.port,
                b.1,
                b.0.channel,
            ))
        });
        let mut runs: Vec<Run> = Vec::new();
        for (land, lamp) in pairs {
            if let Some(run) = runs.last_mut()
                && run.output == land.output
                && run.port == land.port
                && run.first + run.count == lamp
            {
                let next = i64::from(run.channel) + i64::from(run.step) * i64::from(run.count);
                let step = i64::from(land.channel) - i64::from(run.channel);
                if run.count == 1 && (step == 3 || step == -3) {
                    run.step = step as i32;
                    run.count += 1;
                    continue;
                }
                if run.count > 1 && next == i64::from(land.channel) {
                    run.count += 1;
                    continue;
                }
            }
            runs.push(Run {
                first: lamp,
                count: 1,
                output: land.output.clone(),
                port: land.port,
                channel: land.channel,
                step: 3,
            });
        }
        runs.sort_by(|a, b| {
            (a.first, &a.output, a.port, a.channel).cmp(&(b.first, &b.output, b.port, b.channel))
        });
        runs
    }
}

/// One golden run.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    first: u32,
    count: u32,
    output: String,
    port: u32,
    channel: u32,
    step: i32,
}

impl ProjectLanding {
    fn fixture(&self, path: &str) -> &FixtureLanding {
        self.fixtures
            .get(path)
            .unwrap_or_else(|| panic!("no fixture {path}; have {:?}", self.fixtures.keys()))
    }

    /// The golden's text: keys sorted, one run per line, so a diff reads.
    fn to_golden(&self, recorded_at: &str) -> String {
        let mut text = String::from("{\n");
        let _ = writeln!(text, "  \"recorded_at\": {},", json_string(recorded_at));
        let _ = write!(text, "  \"project_format\": {}", self.project_format);
        if let Some(why) = &self.refused {
            let _ = write!(text, ",\n  \"refused\": {}", json_string(why));
        }
        text.push_str(",\n  \"fixtures\": {");
        for (index, (path, fixture)) in self.fixtures.iter().enumerate() {
            text.push_str(if index == 0 { "\n" } else { ",\n" });
            let _ = writeln!(text, "    {}: {{", json_string(path));
            let _ = writeln!(text, "      \"lamps\": {},", fixture.lamps);
            text.push_str("      \"runs\": [");
            for (index, run) in fixture.runs().iter().enumerate() {
                text.push_str(if index == 0 { "\n" } else { ",\n" });
                let _ = write!(
                    text,
                    "        [{}, {}, {}, {}, {}, {}]",
                    run.first,
                    run.count,
                    json_string(&run.output),
                    json_string(&run.port.to_string()),
                    run.channel,
                    run.step
                );
            }
            text.push_str(if fixture.runs().is_empty() {
                "],\n"
            } else {
                "\n      ],\n"
            });
            text.push_str("      \"dark\": [");
            for (index, (first, count)) in fixture.dark.iter().enumerate() {
                text.push_str(if index == 0 { "" } else { ", " });
                let _ = write!(text, "[{first}, {count}]");
            }
            text.push_str("]\n    }");
        }
        text.push_str(if self.fixtures.is_empty() {
            "}\n}\n"
        } else {
            "\n  }\n}\n"
        });
        text
    }

    /// Read a golden back into lamp-by-lamp landings.
    fn from_golden(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let project_format = value["project_format"]
            .as_u64()
            .ok_or("no project_format")?;
        let refused = value
            .get("refused")
            .map(|why| {
                why.as_str()
                    .map(String::from)
                    .ok_or("refused is not a string")
            })
            .transpose()?;
        let mut fixtures = BTreeMap::new();
        for (path, fixture) in value["fixtures"].as_object().ok_or("no fixtures")? {
            let lamps = fixture["lamps"].as_u64().ok_or("no lamps")? as u32;
            let mut landings = vec![Vec::new(); lamps as usize];
            for run in fixture["runs"].as_array().ok_or("no runs")? {
                let run = run.as_array().ok_or("a run is not an array")?;
                let number = |index: usize| run.get(index).and_then(serde_json::Value::as_i64);
                let (Some(first), Some(count), Some(channel), Some(step)) =
                    (number(0), number(1), number(4), number(5))
                else {
                    return Err(format!("bad run {run:?}"));
                };
                let output = run[2].as_str().ok_or("run output")?;
                let port: u32 = run[3]
                    .as_str()
                    .and_then(|key| key.parse().ok())
                    .ok_or("run port")?;
                for k in 0..count {
                    let lamp = (first + k) as usize;
                    let lands: &mut Vec<Landing> = landings
                        .get_mut(lamp)
                        .ok_or("a run past the fixture's lamps")?;
                    lands.push(Landing {
                        output: String::from(output),
                        port,
                        channel: (channel + step * k) as u32,
                    });
                }
            }
            for lands in &mut landings {
                lands.sort();
            }
            let dark = dark_ranges(&landings);
            let recorded_dark: Vec<(u32, u32)> = fixture["dark"]
                .as_array()
                .ok_or("no dark")?
                .iter()
                .map(|range| {
                    (
                        range[0].as_u64().unwrap_or(u64::MAX) as u32,
                        range[1].as_u64().unwrap_or(u64::MAX) as u32,
                    )
                })
                .collect();
            if dark != recorded_dark {
                return Err(format!(
                    "{path}: its dark ranges {recorded_dark:?} disagree with its runs ({dark:?})"
                ));
            }
            fixtures.insert(
                path.clone(),
                FixtureLanding {
                    lamps,
                    landings,
                    dark,
                },
            );
        }
        Ok(Self {
            project_format,
            refused,
            fixtures,
        })
    }

    /// The first way `live` disagrees with this recorded landing, in words.
    fn first_difference(&self, live: &Self) -> Option<String> {
        if self.refused != live.refused {
            return Some(format!(
                "refused: recorded {:?}, now {:?}",
                self.refused, live.refused
            ));
        }
        let recorded: Vec<&String> = self.fixtures.keys().collect();
        let now: Vec<&String> = live.fixtures.keys().collect();
        if recorded != now {
            return Some(format!("fixtures: recorded {recorded:?}, now {now:?}"));
        }
        for (path, fixture) in &self.fixtures {
            let other = &live.fixtures[path];
            if fixture.lamps != other.lamps {
                return Some(format!(
                    "{path}: recorded {} lamps, now {}",
                    fixture.lamps, other.lamps
                ));
            }
            for (lamp, (was, is)) in fixture.landings.iter().zip(&other.landings).enumerate() {
                if was != is {
                    return Some(format!("{path} lamp {lamp}: recorded {was:?}, now {is:?}"));
                }
            }
        }
        None
    }
}

/// Write `text` to `path` unless a golden is already there.
fn record_golden(path: &Path, text: &str) -> Recorded {
    if path.exists() {
        return Recorded::AlreadyThere;
    }
    std::fs::create_dir_all(path.parent().expect("a golden has a directory"))
        .expect("create the golden's directory");
    std::fs::write(path, text).expect("write the golden");
    Recorded::Written
}

#[derive(Debug, PartialEq, Eq)]
enum Recorded {
    Written,
    AlreadyThere,
}

/// Totals over every project, for the PR.
#[derive(Default)]
struct Totals {
    projects: usize,
    refused: Vec<String>,
    fixtures: usize,
    lamps: u64,
    dark: u64,
}

impl Totals {
    fn add(&mut self, relative: &str, landing: &ProjectLanding) {
        self.projects += 1;
        if let Some(why) = &landing.refused {
            self.refused.push(format!("{relative}: {why}"));
        }
        self.fixtures += landing.fixtures.len();
        for fixture in landing.fixtures.values() {
            self.lamps += u64::from(fixture.lamps);
            self.dark += fixture
                .dark
                .iter()
                .map(|(_, count)| u64::from(*count))
                .sum::<u64>();
        }
    }
}

impl std::fmt::Display for Totals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "landing: {} projects, {} fixtures, {} lamps, {} dark, {} refused",
            self.projects,
            self.fixtures,
            self.lamps,
            self.dark,
            self.refused.len()
        )?;
        for refused in &self.refused {
            write!(f, "\n  refused {refused}")?;
        }
        Ok(())
    }
}

// --- helpers ----------------------------------------------------------------

/// The landing of one project under the workspace, live.
fn repo_landing(relative: &str) -> ProjectLanding {
    project_landing(&workspace_dir().join(relative))
}

/// The index of the map2d object whose id is `id`, in radiance-dome's map.
fn object_index(id: &str) -> u32 {
    let text = std::fs::read_to_string(
        workspace_dir().join("lab/mapping/projects/radiance-dome/dome.map2d.json"),
    )
    .expect("radiance map2d");
    let value: serde_json::Value = serde_json::from_str(&text).expect("radiance map2d parses");
    value["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .position(|object| object["id"] == id)
        .unwrap_or_else(|| panic!("no object {id}")) as u32
}

/// Every project directory in the set, sorted.
fn project_dirs(workspace: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in PROJECT_ROOTS {
        collect_projects(&workspace.join(root), &mut dirs);
    }
    let corpus = workspace.join(CORPUS_ROOT);
    for version in read_dir_sorted(&corpus) {
        let expected = version.join("_expected");
        for project in read_dir_sorted(&expected) {
            if project.join("project.json").is_file() {
                dirs.push(project);
            }
        }
    }
    dirs.sort();
    dirs
}

fn collect_projects(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.join("project.json").is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    for child in read_dir_sorted(dir) {
        let name = child
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        collect_projects(&child, out);
    }
}

fn read_dir_sorted(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();
    children
}

fn project_format(project: &Path) -> u64 {
    let text = std::fs::read_to_string(project.join("project.json")).expect("project.json");
    let value: serde_json::Value = serde_json::from_str(&text).expect("project.json parses");
    value["format"].as_u64().expect("project.json has a format")
}

fn golden_path(workspace: &Path, relative: &str) -> PathBuf {
    workspace
        .join(GOLDEN_ROOT)
        .join(format!("{relative}.landing.json"))
}

fn relative_path(workspace: &Path, project: &Path) -> String {
    project
        .strip_prefix(workspace)
        .expect("projects live under the workspace")
        .to_string_lossy()
        .replace('\\', "/")
}

fn git_head(workspace: &Path) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(workspace)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|| String::from("unknown"))
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).expect("a string serializes")
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}
