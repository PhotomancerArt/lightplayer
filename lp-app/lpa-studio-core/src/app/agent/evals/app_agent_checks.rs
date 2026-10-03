//! The project and board checks: pure functions over a [`ProjectTree`]
//! (plus, for the ones that need them, the run's node statuses and the
//! board's end state), and [`run_check`], which dispatches every
//! [`CheckSpec`] — the conversation's own checks live in
//! `app_agent_conversation_checks.rs`.
//!
//! Each returns a [`CheckResult`] whose `reason` names what it saw, so a
//! failed eval reads as "the Output's endpoint is `ws281x:local:D10`", not
//! "check 3 failed". The checks are proved in both directions in this
//! file's tests and in `app_agent_eval_tests.rs` / `app_agent_corpus_tests.rs`:
//! the goldens pass, and the negative fixtures fail the checks they should.
//!
//! Stage B (`lp-cli/tests/app_agent_emu_decode.rs`) is the ground truth for
//! "the LEDs light"; these judge the project the agent built.

use std::collections::BTreeMap;

use lpc_model::HwEndpointSpec;
use serde_json::Value;

use super::app_agent_check_spec::{CheckSpec, FirmwareIs};
use super::app_agent_conversation_checks as conversation;
use super::app_agent_project_tree::{ProjectTree, TreeNode};
use super::app_agent_scenario_seat::DeviceSummary;
use super::app_agent_transcript::EvalTranscript;

pub(crate) use super::app_agent_check_spec::XIAO_C6_BOARD_ID;

/// The endpoint every Sean scenario wants: XIAO D6 = GPIO16.
pub(crate) const D6_ENDPOINT: &str = "ws281x:local:D6";

/// One check's verdict.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct CheckResult {
    /// The check and its key parameter (`output_on(D6)`).
    pub(crate) name: String,
    /// The check's kind (`output_on`), for grouping.
    pub(crate) kind: String,
    pub(crate) passed: bool,
    pub(crate) reason: String,
}

impl CheckResult {
    fn from(check: &CheckSpec, result: Result<String, String>) -> Self {
        let (passed, reason) = match result {
            Ok(reason) => (true, reason),
            Err(reason) => (false, reason),
        };
        Self {
            name: check.name(),
            kind: check.kind(),
            passed,
            reason,
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
    /// The checks to run, in report order.
    pub(crate) checks: &'a [CheckSpec],
    /// The project the run started from (`None` for a Blank or empty start).
    pub(crate) start: Option<&'a ProjectTree>,
    /// The project the run left (saved bytes, or what the board runs).
    pub(crate) project: &'a ProjectTree,
    /// Node statuses after the server advanced (`None`: not observed).
    pub(crate) statuses: Option<&'a [NodeStatusRow]>,
    /// Whether unsaved edits remain (`None`: not observed).
    pub(crate) unsaved: Option<bool>,
    pub(crate) transcript: &'a EvalTranscript,
    /// Model turns across the run.
    pub(crate) turns: u32,
    /// The board's end state (device seat).
    pub(crate) device: Option<&'a DeviceSummary>,
}

/// Run every check, in order.
pub(crate) fn run_checks(input: &CheckInput<'_>) -> Vec<CheckResult> {
    input
        .checks
        .iter()
        .map(|check| run_check(check, input))
        .collect()
}

/// One check's verdict.
pub(crate) fn run_check(check: &CheckSpec, input: &CheckInput<'_>) -> CheckResult {
    CheckResult::from(check, verdict(check, input))
}

fn verdict(check: &CheckSpec, input: &CheckInput<'_>) -> Result<String, String> {
    let project = input.project;
    let transcript = input.transcript;
    let start = || input.start.ok_or("needs a starting project".to_string());
    match check {
        CheckSpec::TargetIs { board } => target_is(project, board),
        CheckSpec::TargetIsXiaoC6 => target_is(project, XIAO_C6_BOARD_ID),
        CheckSpec::OutputOn { pin } => output_on(project, &format!("ws281x:local:{pin}")),
        CheckSpec::OutputOnD6 => output_on(project, D6_ENDPOINT),
        CheckSpec::StripOf { leds } => strip_of(project, *leds),
        CheckSpec::LampCount { leds } => lamp_count(project, *leds),
        CheckSpec::Playlist {
            min_entries,
            step_seconds,
            from,
        } => playlist_cycles(project, *min_entries, *step_seconds, from),
        CheckSpec::GraphWired => graph_wired(project),
        CheckSpec::AllNodesOk => match input.statuses {
            Some(rows) => all_nodes_ok(rows),
            None => Err("node statuses were not observed".to_string()),
        },
        CheckSpec::Saved => match input.unsaved {
            Some(false) => Ok("no unsaved edits".to_string()),
            Some(true) => Err("unsaved edits remain".to_string()),
            None => Err("the save state was not observed".to_string()),
        },
        CheckSpec::MinimalDiff { allow } => minimal_diff(start()?, project, allow),
        CheckSpec::Unchanged => unchanged(start()?, project),
        CheckSpec::Field {
            node,
            path,
            equals,
            between,
            default,
        } => field(
            project,
            node,
            path,
            equals.as_ref(),
            *between,
            default.as_ref(),
        ),
        CheckSpec::EntriesRemoved { patterns } => entries_removed(project, patterns),
        CheckSpec::EntriesKept { patterns } => entries_kept(start()?, project, patterns),
        CheckSpec::EntriesAdded { min, from } => entries_added(input.start, project, *min, from),
        CheckSpec::AnyOf { of } => {
            let verdicts: Vec<Result<String, String>> =
                of.iter().map(|check| verdict(check, input)).collect();
            match verdicts.iter().find_map(|v| v.as_ref().ok()) {
                Some(reason) => Ok(reason.clone()),
                None => Err(verdicts
                    .into_iter()
                    .filter_map(Result::err)
                    .collect::<Vec<_>>()
                    .join("; and ")),
            }
        }
        CheckSpec::Asked { topic } => conversation::asked_about(transcript, topic),
        CheckSpec::AskedAboutBoard => conversation::asked_about(transcript, "board"),
        CheckSpec::AskedBefore { topic, before } => {
            conversation::asked_before(transcript, topic, *before)
        }
        CheckSpec::NoDLabelBeforeBoard => conversation::no_d_label_before_board(transcript),
        CheckSpec::MaxQuestions { n, per_turn } => {
            conversation::max_questions(transcript, *n, *per_turn)
        }
        CheckSpec::MaxTurns { n } => match input.turns <= *n {
            true => Ok(format!("{} turns", input.turns)),
            false => Err(format!("{} turns, more than {n}", input.turns)),
        },
        CheckSpec::CardHanded { offer } => conversation::card_handed(transcript, offer),
        CheckSpec::Never { what } => conversation::never(transcript, what),
        CheckSpec::SaidAny { words, last } => conversation::said_any(transcript, words, *last),
        CheckSpec::SaidNone { words } => conversation::said_none(transcript, words),
        CheckSpec::BoardRunsProject => {
            board_runs_project(input.device.ok_or("no board was observed")?)
        }
        CheckSpec::BoardFirmware { is } => {
            board_firmware(input.device.ok_or("no board was observed")?, *is)
        }
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

/// The project's Fixtures map `leds` lamps between them, in any shape.
pub(crate) fn lamp_count(project: &ProjectTree, leds: u32) -> Result<String, String> {
    let fixtures = project.nodes_of_kind("Fixture");
    if fixtures.is_empty() {
        return Err("no Fixture node".to_string());
    }
    let mut total = 0;
    for fixture in &fixtures {
        total += fixture_lamps(project, fixture)?.len();
    }
    match total == leds as usize {
        true => Ok(format!("{} fixture(s) map {leds} lamps", fixtures.len())),
        false => Err(format!(
            "{} fixture(s) map {total} lamps, not {leds}",
            fixtures.len()
        )),
    }
}

/// A Playlist on Cycle with a step in range (when one is given), and at
/// least `min_entries` entries that are catalog patterns from the
/// `colourful` allowlist (any catalog pattern when it is empty).
pub(crate) fn playlist_cycles(
    project: &ProjectTree,
    min_entries: usize,
    step_range: Option<[f64; 2]>,
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
    if let Some([lo, hi]) = step_range
        && !(lo..=hi).contains(&step)
    {
        return Err(format!(
            "the cycle steps every {step} s, outside {lo}–{hi} s"
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
            Some(slug)
                if colourful.is_empty() || colourful.iter().any(|allowed| *allowed == slug) =>
            {
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

/// Starting from `start`, only what `allow` names changed, and something
/// did. `strip_size` (the default when `allow` is empty): the fixture's
/// render size and map body, and an output port's `count`.
/// `<Kind>.<path>`: that field of every node of that kind. `modules`:
/// anything under `modules/` (pattern folders). The manifest keeps its
/// name, target and format.
pub(crate) fn minimal_diff(
    start: &ProjectTree,
    end: &ProjectTree,
    allow: &[String],
) -> Result<String, String> {
    let strip_size = allow.is_empty() || allow.iter().any(|token| token == "strip_size");
    let modules = allow.iter().any(|token| token == "modules");
    let fields: Vec<(&str, &str)> = allow
        .iter()
        .filter_map(|token| token.split_once('.'))
        .collect();
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
    let allowed_body = match strip_size {
        true => [map_file(start), map_file(end)],
        false => [None, None],
    };
    let fixture = fixture_file(start).filter(|_| strip_size);
    let output = output_file(start).filter(|_| strip_size);

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
        if modules && path.starts_with("modules/") {
            changes.push(format!("{path} (pattern folder)"));
            continue;
        }
        let (Some(before), Some(after)) = (before, after) else {
            extra.push(format!(
                "{path} {}",
                if before.is_none() { "added" } else { "removed" }
            ));
            continue;
        };
        // Node defs compare by meaning: a save rewrites them canonically
        // (defaults spelled out), which is not the agent's change.
        let parse = |bytes: &Vec<u8>| canonical_json(bytes);
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
        for (kind, field) in &fields {
            for def in [&mut a, &mut b] {
                if def["kind"].as_str() == Some(kind) {
                    remove_path(def, field);
                }
            }
        }
        if a == b {
            changes.push(format!("{path} (allowed)"));
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
        "only what {} allows changed: {}",
        if allow.is_empty() {
            "strip_size".to_string()
        } else {
            allow.join(", ")
        },
        changes.join(", ")
    ))
}

/// The E2 rule: only the strip's size changed.
pub(crate) fn minimal_strip_diff(start: &ProjectTree, end: &ProjectTree) -> Result<String, String> {
    minimal_diff(start, end, &[])
}

/// The project's bytes, read by meaning, equal `start`'s. The manifest
/// compares by its name, target and format (a save stamps the rest).
pub(crate) fn unchanged(start: &ProjectTree, end: &ProjectTree) -> Result<String, String> {
    let manifest = |tree: &ProjectTree| {
        tree.manifest()
            .map(|v| (v["name"].clone(), v["target"].clone(), v["format"].clone()))
    };
    let mut paths: Vec<&String> = start.files.keys().chain(end.files.keys()).collect();
    paths.sort();
    paths.dedup();
    let changed: Vec<String> = paths
        .into_iter()
        .filter(|path| {
            if path.as_str() == "project.json" {
                return manifest(start) != manifest(end);
            }
            let (before, after) = (start.files.get(*path), end.files.get(*path));
            before != after
                && match (before, after) {
                    (Some(a), Some(b)) => {
                        canonical_json(a).is_none_or(|a| Some(a) != canonical_json(b))
                    }
                    _ => true,
                }
        })
        .map(
            |path| match (start.files.contains_key(path), end.files.contains_key(path)) {
                (false, _) => format!("{path} added"),
                (_, false) => format!("{path} removed"),
                _ => format!("{path} changed"),
            },
        )
        .collect();
    match changed.is_empty() {
        true => Ok("the project is unchanged".to_string()),
        false => Err(changed.join("; ")),
    }
}

/// The one root-level node of kind `node` has `path` (dotted) equal to
/// `equals`, or a number within `between`; `default` stands in for an
/// absent field.
pub(crate) fn field(
    project: &ProjectTree,
    node: &str,
    path: &str,
    equals: Option<&Value>,
    between: Option<[f64; 2]>,
    default: Option<&Value>,
) -> Result<String, String> {
    let nodes: Vec<TreeNode> = project
        .nodes_of_kind(node)
        .into_iter()
        .filter(|found| !found.in_playlist)
        .collect();
    let [found] = nodes.as_slice() else {
        return Err(format!(
            "expected exactly one root-level {node}, found {}",
            nodes.len()
        ));
    };
    let value = path
        .split('.')
        .try_fold(&found.def, |value, key| value.get(key))
        .or(default)
        .ok_or_else(|| format!("{node} `{}` has no {path}", found.name))?;
    if let Some(want) = equals {
        let same = match (value.as_f64(), want.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-6,
            _ => match (value.as_str(), want.as_str()) {
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => value == want,
            },
        };
        return match same {
            true => Ok(format!("{node}.{path} = {value}")),
            false => Err(format!("{node}.{path} is {value}, not {want}")),
        };
    }
    let [lo, hi] = between.ok_or("field names neither `equals` nor `between`")?;
    let number = value
        .as_f64()
        .ok_or_else(|| format!("{node}.{path} is {value}, not a number"))?;
    match (lo..=hi).contains(&number) {
        true => Ok(format!("{node}.{path} = {number}")),
        false => Err(format!("{node}.{path} is {number}, outside {lo}–{hi}")),
    }
}

/// No playlist entry plays any of `patterns`.
pub(crate) fn entries_removed(
    project: &ProjectTree,
    patterns: &[String],
) -> Result<String, String> {
    let playing = playlist_patterns(project);
    let still: Vec<&String> = patterns
        .iter()
        .filter(|pattern| playing.iter().any(|(slug, _)| slug == *pattern))
        .collect();
    match still.is_empty() {
        true => Ok(format!(
            "none of {patterns:?} plays; the playlist plays {:?}",
            slugs(&playing)
        )),
        false => Err(format!("{still:?} still in the playlist")),
    }
}

/// Every one of `patterns` still plays, its folder unchanged from `start`.
pub(crate) fn entries_kept(
    start: &ProjectTree,
    end: &ProjectTree,
    patterns: &[String],
) -> Result<String, String> {
    let before = playlist_patterns(start);
    let after = playlist_patterns(end);
    let mut problems = Vec::new();
    for pattern in patterns {
        let Some((_, dir)) = after.iter().find(|(slug, _)| slug == pattern) else {
            problems.push(format!("{pattern} no longer plays"));
            continue;
        };
        if let Some((_, start_dir)) = before.iter().find(|(slug, _)| slug == pattern) {
            let files = |tree: &ProjectTree, dir: &str| -> Vec<(String, Option<Value>)> {
                tree.files
                    .iter()
                    .filter_map(|(path, bytes)| {
                        path.strip_prefix(dir)
                            .map(|rest| (rest.to_string(), canonical_json(bytes)))
                    })
                    .collect()
            };
            if files(start, start_dir) != files(end, dir) {
                problems.push(format!("{pattern}'s folder {dir} changed"));
            }
        }
    }
    match problems.is_empty() {
        true => Ok(format!("{patterns:?} still play, untouched")),
        false => Err(problems.join("; ")),
    }
}

/// At least `min` playlist entries play a catalog pattern `start` did not
/// (from `from`, when it is not empty).
pub(crate) fn entries_added(
    start: Option<&ProjectTree>,
    end: &ProjectTree,
    min: usize,
    from: &[String],
) -> Result<String, String> {
    let mut before = start
        .map(playlist_patterns)
        .map(|p| slugs(&p))
        .unwrap_or_default();
    let mut added = Vec::new();
    for slug in slugs(&playlist_patterns(end)) {
        match before.iter().position(|had| *had == slug) {
            Some(at) => {
                before.remove(at);
            }
            None if from.is_empty() || from.contains(&slug) => added.push(slug),
            None => {}
        }
    }
    match added.len() >= min {
        true => Ok(format!("added {added:?}")),
        false => Err(format!(
            "added {} qualifying pattern(s) ({added:?}); need {min}{}",
            added.len(),
            match from.is_empty() {
                true => String::new(),
                false => format!(" from {from:?}"),
            }
        )),
    }
}

/// A board reports running a project.
pub(crate) fn board_runs_project(device: &DeviceSummary) -> Result<String, String> {
    match device.boards.iter().find(|board| board.running) {
        Some(board) => Ok(format!("the board is {}; {}", board.state, board.loaded)),
        None => Err(format!(
            "no board runs a project: boards {:?}, pending {:?}",
            device
                .boards
                .iter()
                .map(|board| format!("{} ({})", board.state, board.loaded))
                .collect::<Vec<_>>(),
            device.pending
        )),
    }
}

/// What firmware the board ended with.
pub(crate) fn board_firmware(device: &DeviceSummary, is: FirmwareIs) -> Result<String, String> {
    match is {
        FirmwareIs::Lightplayer => match device.boards.iter().find(|b| b.state == "Ready") {
            Some(_) => Ok("the board runs LightPlayer".to_string()),
            None => Err(format!(
                "no board runs LightPlayer: boards {:?}, pending {:?}",
                device
                    .boards
                    .iter()
                    .map(|board| board.state.clone())
                    .collect::<Vec<_>>(),
                device.pending
            )),
        },
        FirmwareIs::Flashed => match device.flashed.as_slice() {
            [] => Err("nothing was flashed".to_string()),
            boards => Ok(format!("flashed as {boards:?}")),
        },
        FirmwareIs::Untouched => match device.flashed.as_slice() {
            [] => Ok("nothing was flashed".to_string()),
            boards => Err(format!("flashed as {boards:?}")),
        },
    }
}

/// Every playlist entry's catalog pattern, with its module's folder
/// (`modules/spiral/`), in entry order.
fn playlist_patterns(project: &ProjectTree) -> Vec<(String, String)> {
    let catalog = catalog_shader_slugs();
    let mut out = Vec::new();
    for playlist in project.nodes_of_kind("Playlist") {
        let def_file = playlist
            .file
            .clone()
            .unwrap_or_else(|| "module.json".into());
        let mut entries: Vec<(&String, &Value)> = playlist.def["entries"]
            .as_object()
            .map(|entries| entries.iter().collect())
            .unwrap_or_default();
        entries.sort_by_key(|(key, _)| key.parse::<u64>().unwrap_or(u64::MAX));
        for (_, entry) in entries {
            let Some(reference) = entry["node"]["ref"].as_str() else {
                continue;
            };
            let module = ProjectTree::resolve(&def_file, reference);
            if let Some(slug) = entry_pattern(project, &module, &catalog) {
                let dir = match module.rfind('/') {
                    Some(at) => module[..=at].to_string(),
                    None => String::new(),
                };
                out.push((slug, dir));
            }
        }
    }
    out
}

fn slugs(patterns: &[(String, String)]) -> Vec<String> {
    patterns.iter().map(|(slug, _)| slug.clone()).collect()
}

/// Remove the dotted `path` from `value`, if it is there.
fn remove_path(value: &mut Value, path: &str) {
    match path.split_once('.') {
        None => {
            if let Some(map) = value.as_object_mut() {
                map.remove(path);
            }
        }
        Some((head, rest)) => {
            if let Some(child) = value.get_mut(head) {
                remove_path(child, rest);
            }
        }
    }
}

// --- helpers ---------------------------------------------------------------

/// A file's JSON, with a node def rewritten through the model's canonical
/// writer (so authored and saved forms of the same def compare equal).
fn canonical_json(bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?;
    let value: Value = serde_json::from_str(text).ok()?;
    if value.get("kind").is_none() {
        return Some(value);
    }
    let registry = lpc_model::SlotShapeRegistry::default();
    let canonical = lpc_model::NodeDef::read_json(&registry, text)
        .ok()?
        .write_json(&registry)
        .ok()?;
    serde_json::from_str(&canonical).ok()
}

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
    use crate::app::agent::evals::app_agent_scenario_seat::BoardRow;

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

    #[test]
    fn remove_path_walks_dots() {
        let mut value = json!({"power": {"budget_ma": 900, "lamp_type": "ws2812b"}, "a": 1});
        remove_path(&mut value, "power.budget_ma");
        remove_path(&mut value, "missing.deeper");
        assert_eq!(value, json!({"power": {"lamp_type": "ws2812b"}, "a": 1}));
    }

    #[test]
    fn the_board_checks_read_the_roster() {
        let running = DeviceSummary {
            boards: vec![BoardRow {
                state: "Ready".into(),
                loaded: "running \"Festival\"".into(),
                running: true,
            }],
            pending: Vec::new(),
            flashed: vec!["seeed/xiao-esp32-c6".into()],
            pushes: 1,
        };
        board_runs_project(&running).expect("running");
        board_firmware(&running, FirmwareIs::Lightplayer).expect("LightPlayer");
        board_firmware(&running, FirmwareIs::Flashed).expect("flashed");
        board_firmware(&running, FirmwareIs::Untouched).expect_err("it was flashed");
        let foreign = DeviceSummary {
            pending: vec!["needs firmware (WLED)".into()],
            ..DeviceSummary::default()
        };
        let reason = board_runs_project(&foreign).expect_err("nothing runs");
        assert!(reason.contains("WLED"), "{reason}");
        board_firmware(&foreign, FirmwareIs::Lightplayer).expect_err("WLED");
        board_firmware(&foreign, FirmwareIs::Untouched).expect("untouched");
    }

    #[test]
    fn any_of_passes_on_one_and_names_every_miss() {
        let tree = ProjectTree::default();
        let transcript = EvalTranscript::default();
        let checks = [CheckSpec::AnyOf {
            of: vec![
                CheckSpec::StripOf { leds: 60 },
                CheckSpec::MaxTurns { n: 3 },
            ],
        }];
        let input = CheckInput {
            checks: &checks,
            start: None,
            project: &tree,
            statuses: None,
            unsaved: None,
            transcript: &transcript,
            turns: 2,
            device: None,
        };
        let results = run_checks(&input);
        assert!(results[0].passed, "{results:?}");
        let input = CheckInput { turns: 9, ..input };
        let result = &run_checks(&input)[0];
        assert!(!result.passed);
        assert!(
            result.reason.contains("Fixture") && result.reason.contains("9 turns"),
            "{}",
            result.reason
        );
    }
}
