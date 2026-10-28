//! The format-12 upgrade moved no lamp: every project's lamps land, at
//! project format 12, exactly where the live engine landed them at 11 (A6).
//!
//! Project format 12 moved every mapping onto the patcher. The "before"
//! shares no code with the upgrade step: the live engine at project format
//! 11 loaded every project in the repo (and every upgrade-corpus project),
//! rendered it, and recorded where each fixture lamp landed — the output's
//! own published fragments (the plan the output renders from), cut into
//! ports by the slicing the wires register
//! ([`EngineServices::planned_wire_slices_for_landing`]). Nothing here reads
//! a map2d or a patch document.
//!
//! Those answers are the landing goldens, one per project, under
//! `lp-app/lpa-upgrade/tests/landing/<project path>.landing.json`. They were
//! recorded at format 11 and are never re-blessed: the recorder was deleted
//! with the bump (mapping rebuild M3 P4), and the goldens stay as the
//! record. This test walks the goldens, not the tree: each golden's project
//! must exist, load at format 12 through the engine's format-6 path, and
//! land every lamp exactly where its golden says (output node, port key,
//! channel; dark stays dark). A project deleted later takes its golden with
//! it, and the PR says why; a project created after the bump has no
//! "before" and needs none.
//!
//! ```bash
//! cargo test -p lpc-engine --test upgrade_landing
//! ```

mod landing_support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use landing_support::{
    FixtureLanding, Landing, ProjectLanding, at, dark_ranges, project_format, project_landing,
};

/// Where the goldens live, relative to the workspace root.
const GOLDEN_ROOT: &str = "lp-app/lpa-upgrade/tests/landing";

/// Every project a golden names exists, is at the current project format,
/// and lands every lamp where the live engine landed it at format 11:
/// zero differences, lamp for lamp.
#[test]
fn every_golden_project_lands_every_lamp_where_it_did_at_format_11() {
    let workspace = workspace_dir();
    let mut failures = Vec::new();
    let mut totals = Totals::default();
    for (relative, golden) in goldens(&workspace) {
        let project = workspace.join(&relative);
        assert!(
            project.join("project.json").is_file(),
            "{relative}: its landing golden names a project that is not there. A project \
             deleted after the bump takes its golden with it, and the PR says why"
        );
        let format = project_format(&project);
        if format != u64::from(lpc_model::PROJECT_FORMAT_VERSION) {
            failures.push(format!(
                "{relative}: at project format {format}, not {}",
                lpc_model::PROJECT_FORMAT_VERSION
            ));
            continue;
        }
        let recorded = ProjectLanding::from_golden(&golden)
            .unwrap_or_else(|why| panic!("{relative}: unreadable golden: {why}"));
        let landing = project_landing(&project);
        totals.add(&relative, &recorded, &landing);
        if let Some(difference) = recorded.first_difference(&landing) {
            failures.push(format!("{relative}: {difference}"));
        }
    }

    eprintln!("{totals}");
    assert!(
        failures.is_empty(),
        "these projects' lamps no longer land where they did at format 11:\n{}",
        failures.join("\n")
    );
    assert!(
        totals.projects > 100,
        "only {} goldens were compared",
        totals.projects
    );
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

/// quad-wire-oracle's 64 lamps run across its four 16-lamp ports, one
/// 8 × 2 grid per port since it was split by hand for format 12 (it was one
/// 8 × 8 grid the ports cut).
///
/// From its `output.json` (ports 16, 16, 16, rest).
#[test]
fn quad_wire_oracle_lands_sixteen_lamps_per_port() {
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

// --- reading a golden ---------------------------------------------------------

impl ProjectLanding {
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

/// Totals over every golden compared, for the PR.
#[derive(Default)]
struct Totals {
    projects: usize,
    refused: Vec<String>,
    fixtures: usize,
    lamps: u64,
    dark: u64,
    differences: usize,
}

impl Totals {
    fn add(&mut self, relative: &str, recorded: &ProjectLanding, live: &ProjectLanding) {
        self.projects += 1;
        if let Some(why) = &live.refused {
            self.refused.push(format!("{relative}: {why}"));
        }
        self.fixtures += recorded.fixtures.len();
        for (path, fixture) in &recorded.fixtures {
            self.lamps += u64::from(fixture.lamps);
            self.dark += fixture
                .dark
                .iter()
                .map(|(_, count)| u64::from(*count))
                .sum::<u64>();
            let Some(other) = live.fixtures.get(path) else {
                self.differences += fixture.landings.len();
                continue;
            };
            self.differences += fixture
                .landings
                .iter()
                .zip(&other.landings)
                .filter(|(was, is)| was != is)
                .count();
            self.differences += fixture.landings.len().abs_diff(other.landings.len());
        }
    }
}

impl std::fmt::Display for Totals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "landing at format {}: {} projects, {} fixtures, {} lamps compared, {} dark, \
             {} refused as at format 11, {} lamps landing differently",
            lpc_model::PROJECT_FORMAT_VERSION,
            self.projects,
            self.fixtures,
            self.lamps,
            self.dark,
            self.refused.len(),
            self.differences
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

/// The index of the panel whose id is `id` among radiance-dome's parts:
/// each top-level part is a 5-way repeat holding one panel (format 6 keeps
/// today's object order, the repeat holding the object's id).
fn object_index(id: &str) -> u32 {
    let text = std::fs::read_to_string(
        workspace_dir().join("lab/mapping/projects/radiance-dome/dome.map2d.json"),
    )
    .expect("radiance map2d");
    let value: serde_json::Value = serde_json::from_str(&text).expect("radiance map2d parses");
    value["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .position(|part| part["kind"]["repeat"]["part"]["id"] == id)
        .unwrap_or_else(|| panic!("no panel {id}")) as u32
}

/// Every landing golden, keyed by the project path it names (from the
/// workspace root), with its text; sorted.
fn goldens(workspace: &Path) -> Vec<(String, String)> {
    let root = workspace.join(GOLDEN_ROOT);
    let mut files = Vec::new();
    collect_goldens(&root, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("under the golden root")
                .to_string_lossy()
                .replace('\\', "/");
            let project = relative
                .strip_suffix(".landing.json")
                .unwrap_or_else(|| panic!("{relative}: not a landing golden"))
                .to_string();
            let text = std::fs::read_to_string(&path).expect("golden");
            (project, text)
        })
        .collect()
}

fn collect_goldens(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_goldens(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}
