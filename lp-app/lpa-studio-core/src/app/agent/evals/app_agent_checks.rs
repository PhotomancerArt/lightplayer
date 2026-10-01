//! The stage-A checks: pure functions over a [`ProjectTree`] (plus, for
//! the two that need them, the run's node statuses and transcript).
//!
//! Each returns a [`CheckResult`] whose `reason` names what it saw, so a
//! failed eval reads as "the Output's endpoint is `ws281x:local:D10`", not
//! "check 3 failed". The checks are proved in both directions in this
//! file's tests and in `app_agent_eval_tests.rs`: the goldens pass, and the
//! negative fixtures fail the checks they should.
//!
//! Stage B (`lp-cli/tests/app_agent_emu_decode.rs`) is the ground truth for
//! "the LEDs light"; these judge the project the agent built.

use std::collections::BTreeMap;

use lpc_model::HwEndpointSpec;
use serde_json::Value;

use super::app_agent_project_tree::{ProjectTree, TreeNode};
use super::app_agent_scenario::{CheckId, Scenario};
use super::app_agent_transcript::{EvalStep, EvalTranscript};

/// The XIAO ESP32-C6's catalog board id (`lpc-hardware/boards/seeed/xiao-esp32-c6.json`).
pub(crate) const XIAO_C6_BOARD_ID: &str = "seeed/xiao-esp32-c6";

/// The endpoint every Sean scenario wants: XIAO D6 = GPIO16.
pub(crate) const D6_ENDPOINT: &str = "ws281x:local:D6";

/// One check's verdict.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct CheckResult {
    pub(crate) name: String,
    pub(crate) passed: bool,
    pub(crate) reason: String,
}

impl CheckResult {
    fn pass(check: CheckId, reason: impl Into<String>) -> Self {
        Self {
            name: check.name().to_string(),
            passed: true,
            reason: reason.into(),
        }
    }

    fn fail(check: CheckId, reason: impl Into<String>) -> Self {
        Self {
            name: check.name().to_string(),
            passed: false,
            reason: reason.into(),
        }
    }

    fn from(check: CheckId, result: Result<String, String>) -> Self {
        match result {
            Ok(reason) => Self::pass(check, reason),
            Err(reason) => Self::fail(check, reason),
        }
    }
}

/// A node's runtime status as the run left it (stage A reads these off
/// the project controller after the in-process server advanced).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct NodeStatusRow {
    /// The node's tree path (`/project/playlist/entry_2`).
    pub(crate) path: String,
    pub(crate) kind: String,
    /// The status word the editor shows (`Running`, `Error`, …).
    pub(crate) status: String,
    pub(crate) detail: Option<String>,
    /// `Running` (or an equivalent healthy state).
    pub(crate) ok: bool,
    /// A failure: error, init error, fault, failed load.
    pub(crate) failed: bool,
}

/// Everything a run hands the checks.
pub(crate) struct CheckInput<'a> {
    pub(crate) scenario: &'a Scenario,
    /// The project the run started from (`None` for a Blank start).
    pub(crate) start: Option<&'a ProjectTree>,
    /// The project the run left (saved bytes).
    pub(crate) project: &'a ProjectTree,
    /// Node statuses after the server advanced (`None`: not observed).
    pub(crate) statuses: Option<&'a [NodeStatusRow]>,
    /// Whether unsaved edits remain (`None`: not observed).
    pub(crate) unsaved: Option<bool>,
    pub(crate) transcript: &'a EvalTranscript,
}

/// Run every check the scenario names, in its order.
pub(crate) fn run_checks(input: &CheckInput<'_>) -> Vec<CheckResult> {
    input
        .scenario
        .checks
        .iter()
        .map(|check| run_check(*check, input))
        .collect()
}

pub(crate) fn run_check(check: CheckId, input: &CheckInput<'_>) -> CheckResult {
    let scenario = input.scenario;
    match check {
        CheckId::OutputOnD6 => CheckResult::from(check, output_on(input.project, D6_ENDPOINT)),
        CheckId::TargetIsXiaoC6 => {
            CheckResult::from(check, target_is(input.project, XIAO_C6_BOARD_ID))
        }
        CheckId::StripOf => CheckResult::from(check, strip_of(input.project, scenario.leds)),
        CheckId::PlaylistCycles => CheckResult::from(
            check,
            playlist_cycles(
                input.project,
                scenario.playlist.min_entries,
                scenario.playlist.step_seconds,
                &scenario.playlist.colourful,
            ),
        ),
        CheckId::GraphWired => CheckResult::from(check, graph_wired(input.project)),
        CheckId::AllNodesOk => match input.statuses {
            Some(rows) => CheckResult::from(check, all_nodes_ok(rows)),
            None => CheckResult::fail(check, "node statuses were not observed"),
        },
        CheckId::Saved => match input.unsaved {
            Some(false) => CheckResult::pass(check, "no unsaved edits"),
            Some(true) => CheckResult::fail(check, "unsaved edits remain"),
            None => CheckResult::fail(check, "the save state was not observed"),
        },
        CheckId::AskedAboutBoard => {
            CheckResult::from(check, asked_about(input.transcript, "board"))
        }
        CheckId::NoDLabelBeforeBoard => {
            CheckResult::from(check, no_d_label_before_board(input.transcript))
        }
        CheckId::MinimalDiff => match input.start {
            Some(start) => CheckResult::from(check, minimal_strip_diff(start, input.project)),
            None => CheckResult::fail(check, "minimal_diff needs a starting project"),
        },
    }
}

/// Exactly one Output, and one of its ports drives `endpoint`.
pub(crate) fn output_on(project: &ProjectTree, endpoint: &str) -> Result<String, String> {
    let outputs = project.nodes_of_kind("Output");
    let [output] = outputs.as_slice() else {
        return Err(format!(
            "expected exactly one Output node, found {}",
            outputs.len()
        ));
    };
    let endpoints = port_endpoints(&output.def);
    if endpoints.is_empty() {
        return Err(format!("Output `{}` has no port endpoint", output.name));
    }
    for raw in &endpoints {
        match HwEndpointSpec::parse(raw.clone()) {
            Ok(spec) if spec.as_str() == endpoint => {
                return Ok(format!("Output `{}` drives {endpoint}", output.name));
            }
            Ok(_) => {}
            Err(error) => {
                return Err(format!("endpoint {raw:?} does not parse: {error:?}"));
            }
        }
    }
    Err(format!(
        "Output `{}` drives {endpoints:?}, not {endpoint}",
        output.name
    ))
}

/// The manifest's `target` is `board`.
pub(crate) fn target_is(project: &ProjectTree, board: &str) -> Result<String, String> {
    let manifest = project
        .manifest()
        .ok_or("project.json is missing or not JSON")?;
    match manifest.get("target").and_then(Value::as_str) {
        Some(target) if target == board => Ok(format!("target is {board}")),
        Some(target) => Err(format!("target is {target:?}, not {board}")),
        None => Err("project.json has no target".to_string()),
    }
}

/// Exactly one Fixture, mapping exactly `leds` lamps in strip order, and a
/// render size whose long side gives each lamp its own texel.
pub(crate) fn strip_of(project: &ProjectTree, leds: u32) -> Result<String, String> {
    let fixtures = project.nodes_of_kind("Fixture");
    let [fixture] = fixtures.as_slice() else {
        return Err(format!(
            "expected exactly one Fixture node, found {}",
            fixtures.len()
        ));
    };
    let positions = fixture_lamps(project, fixture)?;
    if positions.len() != leds as usize {
        return Err(format!(
            "the fixture maps {} lamps, not {leds}",
            positions.len()
        ));
    }
    if !in_strip_order(&positions) {
        return Err(format!(
            "the fixture's {leds} lamps are not in strip order along one axis"
        ));
    }
    let size = &fixture.def["render_size"];
    let (width, height) = (
        size["width"].as_u64().unwrap_or(0),
        size["height"].as_u64().unwrap_or(0),
    );
    if width.max(height) < u64::from(leds) {
        return Err(format!(
            "render size {width}×{height} gives {leds} lamps fewer than one texel each along the strip"
        ));
    }
    Ok(format!(
        "one fixture, {leds} lamps in strip order, render {width}×{height}"
    ))
}

/// A Playlist on Cycle with a step in range, and at least `min_entries`
/// entries that are catalog patterns from the `colourful` allowlist.
pub(crate) fn playlist_cycles(
    project: &ProjectTree,
    min_entries: usize,
    step_range: [f64; 2],
    colourful: &[String],
) -> Result<String, String> {
    let playlists = project.nodes_of_kind("Playlist");
    let playlist = match playlists.as_slice() {
        [one] => one,
        [] => return Err("no Playlist node".to_string()),
        many => return Err(format!("{} Playlist nodes; expected one", many.len())),
    };
    let cycle = &playlist.def["cycle"];
    if cycle.get("kind").and_then(Value::as_str) != Some("cycle") {
        return Err(format!(
            "the playlist does not cycle (cycle = {})",
            if cycle.is_null() {
                "unset".to_string()
            } else {
                cycle.to_string()
            }
        ));
    }
    let step = cycle["step_seconds"].as_f64().unwrap_or(0.0);
    if !(step_range[0]..=step_range[1]).contains(&step) {
        return Err(format!(
            "the cycle steps every {step} s, outside {}–{} s",
            step_range[0], step_range[1]
        ));
    }
    let def_file = playlist
        .file
        .clone()
        .unwrap_or_else(|| "module.json".into());
    let catalog = catalog_shader_slugs();
    let mut identified = Vec::new();
    let mut strangers = Vec::new();
    for (key, entry) in playlist.def["entries"].as_object().into_iter().flatten() {
        let Some(reference) = entry["node"]["ref"].as_str() else {
            strangers.push(format!("entry {key} (inline or unset node)"));
            continue;
        };
        let module = ProjectTree::resolve(&def_file, reference);
        match entry_pattern(project, &module, &catalog) {
            Some(slug) if colourful.iter().any(|allowed| *allowed == slug) => {
                identified.push(slug);
            }
            Some(slug) => {
                strangers.push(format!("entry {key} ({slug}, not on the colourful list)"))
            }
            None => strangers.push(format!("entry {key} (not a catalog pattern)")),
        }
    }
    if identified.len() < min_entries {
        return Err(format!(
            "{} colourful catalog entries ({identified:?}); need {min_entries}{}",
            identified.len(),
            if strangers.is_empty() {
                String::new()
            } else {
                format!("; also {}", strangers.join(", "))
            }
        ));
    }
    Ok(format!(
        "cycles every {step} s through {identified:?}{}",
        if strangers.is_empty() {
            String::new()
        } else {
            format!(" (plus {})", strangers.join(", "))
        }
    ))
}

/// The bus wiring `generate_board_project` builds: a clock, the playlist
/// reading `bus:time`, the fixture reading `bus:visual.out` and writing
/// `bus:control.out`, and the output reading `bus:control.out`.
pub(crate) fn graph_wired(project: &ProjectTree) -> Result<String, String> {
    let root_level = |kind: &str| -> Vec<TreeNode> {
        project
            .nodes_of_kind(kind)
            .into_iter()
            .filter(|node| !node.in_playlist)
            .collect()
    };
    let mut problems = Vec::new();
    if root_level("Clock").is_empty() {
        problems.push("no Clock".to_string());
    }
    for playlist in root_level("Playlist") {
        if binding(&playlist.def, "time", "source") != Some("bus:time") {
            problems.push(format!(
                "playlist `{}` does not read bus:time",
                playlist.name
            ));
        }
    }
    let fixtures = root_level("Fixture");
    if fixtures.is_empty() {
        problems.push("no Fixture".to_string());
    }
    for fixture in fixtures {
        if binding(&fixture.def, "input", "source") != Some("bus:visual.out") {
            problems.push(format!(
                "fixture `{}` does not read bus:visual.out",
                fixture.name
            ));
        }
        if binding(&fixture.def, "output", "target") != Some("bus:control.out") {
            problems.push(format!(
                "fixture `{}` does not write bus:control.out",
                fixture.name
            ));
        }
    }
    let outputs = root_level("Output");
    if outputs.is_empty() {
        problems.push("no Output".to_string());
    }
    for output in outputs {
        if binding(&output.def, "input", "source") != Some("bus:control.out") {
            problems.push(format!(
                "output `{}` does not read bus:control.out",
                output.name
            ));
        }
    }
    if problems.is_empty() {
        Ok("clock → playlist → fixture → output wired over the bus".to_string())
    } else {
        Err(problems.join("; "))
    }
}

/// No node failed, and at least one node runs. A dormant playlist entry
/// (not the playing one) reads `Pending`/`Created` and is neither.
pub(crate) fn all_nodes_ok(rows: &[NodeStatusRow]) -> Result<String, String> {
    let failed: Vec<String> = rows
        .iter()
        .filter(|row| row.failed)
        .map(|row| {
            format!(
                "{} ({}): {}{}",
                row.path,
                row.kind,
                row.status,
                row.detail
                    .as_deref()
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default()
            )
        })
        .collect();
    if !failed.is_empty() {
        return Err(failed.join("; "));
    }
    let running = rows.iter().filter(|row| row.ok).count();
    if running == 0 {
        return Err(format!("none of {} nodes is running", rows.len()));
    }
    Ok(format!(
        "{running} of {} nodes running, none failed",
        rows.len()
    ))
}

/// The run consumed a scripted reply about `topic`: the agent asked.
pub(crate) fn asked_about(transcript: &EvalTranscript, topic: &str) -> Result<String, String> {
    if transcript
        .steps
        .iter()
        .any(|step| matches!(step, EvalStep::ScriptedReply { about, .. } if about == topic))
    {
        Ok(format!("the agent asked about the {topic}"))
    } else {
        Err(format!("the agent never asked about the {topic}"))
    }
}

/// No tool call before the board reply wrote a `ws281x:local:D<n>` spec.
pub(crate) fn no_d_label_before_board(transcript: &EvalTranscript) -> Result<String, String> {
    for step in &transcript.steps {
        match step {
            EvalStep::ScriptedReply { about, .. } if about == "board" => {
                return Ok("no D-label endpoint was written before the board was known".into());
            }
            EvalStep::ToolCall { name, input } => {
                if let Some(spec) = d_label_spec(input) {
                    return Err(format!(
                        "`{name}` wrote {spec:?} before the user said which board it is"
                    ));
                }
            }
            _ => {}
        }
    }
    Ok("no D-label endpoint was written".into())
}

/// Starting from `start`, only the strip's size changed: the fixture's
/// render size and map body, and an output port's `count`. The manifest
/// keeps its name, target and format.
pub(crate) fn minimal_strip_diff(start: &ProjectTree, end: &ProjectTree) -> Result<String, String> {
    let fixture_file = |tree: &ProjectTree| {
        tree.nodes_of_kind("Fixture")
            .into_iter()
            .find_map(|node| node.file)
    };
    let map_file = |tree: &ProjectTree| -> Option<String> {
        let fixture = tree.nodes_of_kind("Fixture").into_iter().next()?;
        let file = fixture.file?;
        let source = fixture.def["mapping"]["source"].as_str()?;
        Some(ProjectTree::resolve(&file, source))
    };
    let output_file = |tree: &ProjectTree| {
        tree.nodes_of_kind("Output")
            .into_iter()
            .find_map(|node| node.file)
    };
    let allowed_body = [map_file(start), map_file(end)];
    let fixture = fixture_file(start);
    let output = output_file(start);

    let mut paths: Vec<&String> = start.files.keys().chain(end.files.keys()).collect();
    paths.sort();
    paths.dedup();
    let mut changes = Vec::new();
    let mut extra = Vec::new();
    for path in paths {
        let (before, after) = (start.files.get(path), end.files.get(path));
        if before == after {
            continue;
        }
        let path_opt = Some(path.clone());
        if allowed_body.contains(&path_opt) {
            changes.push(format!("{path} (strip body)"));
            continue;
        }
        let (Some(before), Some(after)) = (before, after) else {
            extra.push(format!(
                "{path} {}",
                if before.is_none() { "added" } else { "removed" }
            ));
            continue;
        };
        let parse = |bytes: &Vec<u8>| serde_json::from_slice::<Value>(bytes).ok();
        let (Some(mut a), Some(mut b)) = (parse(before), parse(after)) else {
            extra.push(format!("{path} changed"));
            continue;
        };
        if path == "project.json" {
            let keep = |v: &Value| (v["name"].clone(), v["target"].clone(), v["format"].clone());
            if keep(&a) != keep(&b) {
                extra.push("project.json name/target/format changed".to_string());
            }
            continue;
        }
        if fixture.as_ref() == Some(path) {
            strip_keys(&mut a, &["render_size"]);
            strip_keys(&mut b, &["render_size"]);
        }
        if output.as_ref() == Some(path) {
            strip_port_counts(&mut a);
            strip_port_counts(&mut b);
        }
        if a == b {
            changes.push(format!("{path} (size)"));
        } else {
            extra.push(format!("{path} changed beyond the strip size"));
        }
    }
    if !extra.is_empty() {
        return Err(extra.join("; "));
    }
    if changes.is_empty() {
        return Err("nothing changed".to_string());
    }
    Ok(format!(
        "only the strip size changed: {}",
        changes.join(", ")
    ))
}

// --- helpers ---------------------------------------------------------------

fn port_endpoints(output: &Value) -> Vec<String> {
    let ports = &output["ports"];
    let iter: Box<dyn Iterator<Item = &Value>> = match ports {
        Value::Object(map) => Box::new(map.values()),
        Value::Array(list) => Box::new(list.iter()),
        _ => Box::new(std::iter::empty()),
    };
    iter.filter_map(|port| port["endpoint"].as_str().map(str::to_string))
        .collect()
}

fn binding<'a>(def: &'a Value, name: &str, side: &str) -> Option<&'a str> {
    def["bindings"][name][side].as_str()
}

/// The fixture's lamp positions in wiring order, from its Map2d body or its
/// PathPoints paths.
fn fixture_lamps(project: &ProjectTree, fixture: &TreeNode) -> Result<Vec<[f32; 2]>, String> {
    let mapping = &fixture.def["mapping"];
    match mapping["kind"].as_str() {
        Some("Map2d") => {
            let source = mapping["source"]
                .as_str()
                .ok_or("the Map2d mapping names no source")?;
            let file = fixture.file.clone().unwrap_or_else(|| "module.json".into());
            let path = ProjectTree::resolve(&file, source);
            let text = project
                .text(&path)
                .ok_or_else(|| format!("the mapping body {path} is missing"))?;
            let doc = lpc_mapping::Map2dDoc::from_json(text)
                .map_err(|error| format!("{path} does not parse: {error:?}"))?;
            let resolved = lpc_mapping::resolve(&doc)
                .map_err(|error| format!("{path} does not resolve: {error:?}"))?;
            Ok(resolved.positions())
        }
        Some("PathPoints") => {
            let mut lamps = Vec::new();
            let paths = mapping["paths"]
                .as_object()
                .ok_or("PathPoints has no paths")?;
            let mut keys: Vec<(u64, &Value)> = paths
                .iter()
                .map(|(k, v)| (k.parse().unwrap_or(u64::MAX), v))
                .collect();
            keys.sort_by_key(|(k, _)| *k);
            for (_, path) in keys {
                let points = &path["points"];
                let list: Vec<&Value> = match points {
                    Value::Object(map) => {
                        let mut pts: Vec<(u64, &Value)> = map
                            .iter()
                            .map(|(k, v)| (k.parse().unwrap_or(u64::MAX), v))
                            .collect();
                        pts.sort_by_key(|(k, _)| *k);
                        pts.into_iter().map(|(_, v)| v).collect()
                    }
                    Value::Array(list) => list.iter().collect(),
                    _ => Vec::new(),
                };
                for point in list {
                    let xy = |i: usize| {
                        point
                            .get(i)
                            .or_else(|| point.get(if i == 0 { "x" } else { "y" }))
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0) as f32
                    };
                    lamps.push([xy(0), xy(1)]);
                }
            }
            Ok(lamps)
        }
        other => Err(format!("unsupported fixture mapping kind {other:?}")),
    }
}

/// Lamps advance monotonically along one axis (either direction).
fn in_strip_order(positions: &[[f32; 2]]) -> bool {
    if positions.len() < 2 {
        return true;
    }
    (0..2).any(|axis| {
        let pairs = positions.windows(2);
        let increasing = pairs.clone().all(|w| w[1][axis] > w[0][axis]);
        let decreasing = pairs.clone().all(|w| w[1][axis] < w[0][axis]);
        increasing || decreasing
    })
}

/// Every catalog pattern's shader source, keyed by the source bytes.
fn catalog_shader_slugs() -> BTreeMap<Vec<u8>, String> {
    let mut out = BTreeMap::new();
    for example in crate::app::home::embedded_examples() {
        for (path, bytes) in example.files {
            if path.ends_with(".glsl") {
                out.insert(bytes.to_vec(), example.slug.to_string());
            }
        }
    }
    out
}

/// Which catalog pattern the module at `module` (a def path) vendors,
/// recognised by its shader source bytes.
fn entry_pattern(
    project: &ProjectTree,
    module: &str,
    catalog: &BTreeMap<Vec<u8>, String>,
) -> Option<String> {
    let dir = match module.rfind('/') {
        Some(at) => format!("{}/", &module[..at]),
        None => String::new(),
    };
    project
        .files
        .iter()
        .filter(|(path, _)| path.starts_with(&dir) && path.ends_with(".glsl"))
        .find_map(|(_, bytes)| catalog.get(bytes).cloned())
}

/// A `ws281x:local:D<n>` spec anywhere in a tool input.
fn d_label_spec(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let at = text.find("ws281x:local:D")?;
            let rest = &text[at + "ws281x:local:D".len()..];
            rest.chars().next().filter(char::is_ascii_digit).map(|_| {
                text[at..]
                    .split(|c: char| c == '"' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
        }
        Value::Array(items) => items.iter().find_map(d_label_spec),
        Value::Object(map) => map.values().find_map(d_label_spec),
        _ => None,
    }
}

fn strip_keys(value: &mut Value, keys: &[&str]) {
    if let Some(map) = value.as_object_mut() {
        for key in keys {
            map.remove(*key);
        }
    }
}

fn strip_port_counts(output: &mut Value) {
    match &mut output["ports"] {
        Value::Object(map) => {
            for port in map.values_mut() {
                strip_keys(port, &["count"]);
            }
        }
        Value::Array(list) => {
            for port in list {
                strip_keys(port, &["count"]);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_d_label_written_before_the_board_reply_fails() {
        let before = EvalTranscript {
            steps: vec![
                EvalStep::ToolCall {
                    name: "edit_project".into(),
                    input: json!({"edits":[{"set":{"value":"ws281x:local:D6"}}]}),
                },
                EvalStep::ScriptedReply {
                    about: "board".into(),
                    text: "XIAO".into(),
                },
            ],
        };
        let reason = no_d_label_before_board(&before).expect_err("guessed the board");
        assert!(reason.contains("ws281x:local:D6"), "{reason}");

        let after = EvalTranscript {
            steps: before.steps.iter().rev().cloned().collect(),
        };
        no_d_label_before_board(&after).expect("asked first");
        asked_about(&after, "board").expect("asked");
        asked_about(&EvalTranscript::default(), "board").expect_err("never asked");
    }

    #[test]
    fn gpio_specs_are_not_d_labels() {
        assert_eq!(d_label_spec(&json!("ws281x:local:GPIO16")), None);
        assert_eq!(d_label_spec(&json!("ws281x:local:Data")), None);
        assert_eq!(
            d_label_spec(&json!({"a":["x", "ws281x:local:D10"]})),
            Some("ws281x:local:D10".into())
        );
    }

    #[test]
    fn strip_order_is_monotone_along_an_axis() {
        assert!(in_strip_order(&[[0.0, 1.0], [1.0, 1.0], [2.0, 1.0]]));
        assert!(in_strip_order(&[[0.0, 3.0], [0.0, 2.0], [0.0, 1.0]]));
        assert!(!in_strip_order(&[[0.0, 0.0], [2.0, 0.0], [1.0, 0.0]]));
    }

    #[test]
    fn all_nodes_ok_names_the_failure() {
        let row = |path: &str, ok, failed| NodeStatusRow {
            path: path.into(),
            kind: "Output".into(),
            status: if failed { "Error" } else { "Running" }.into(),
            detail: failed.then(|| "endpoint ws281x:local:D6 did not open".into()),
            ok,
            failed,
        };
        all_nodes_ok(&[row("/a", true, false)]).expect("ok");
        let reason = all_nodes_ok(&[row("/a", true, false), row("/out", false, true)])
            .expect_err("a failed node");
        assert!(
            reason.contains("/out") && reason.contains("did not open"),
            "{reason}"
        );
        all_nodes_ok(&[]).expect_err("nothing runs");
    }
}
