//! Format 11 → 12: a fixture's mapping and its patching are one file, map2d
//! format 6 (the mapping rebuild, M3; G1 2026-10-27: one file).
//!
//! The break: at format 12 a fixture's `*.map2d.json` is format 6 — parts,
//! segments and the patcher's starts and overrides — and patch files
//! (`*.patch.json`), the fixture's `patch` slot, auto-flow and hand-placed
//! `PathPoints` mappings are gone. A format-11 project's lamps land where
//! today's engine puts them (the frozen landing, `crate::v11`); this step
//! writes that down so the patcher lands every lamp in the same place.
//!
//! ## What this step does, per fixture in load order
//!
//! 1. **Refuses the whole project** if a segment does not land as one
//!    window on one port (a port boundary or a patch row cuts it, lean Q9),
//!    naming the project, the fixture, the object and where it is cut, with
//!    a remedy a person can do today. Today's engine refusing the project
//!    is refused the same way (the frozen landing says why).
//! 2. **Freezes its landing** (`crate::v11::freeze_patching`): the start
//!    where its first lamp lands, `off` on every dark segment, and an
//!    override on every segment that does not simply follow on.
//! 3. **Writes its map2d as format 6** (`crate::v11::freeze_document`),
//!    ids minted where objects have none (`crate::v11::freeze_ids`); a
//!    `PathPoints` fixture gets a new `<def stem>.map2d.json` of `points`
//!    parts, fitted so every lamp reaches the sampler where it does today
//!    (`crate::v11::freeze_fit`), and its `mapping` names it. A map2d
//!    already at format 6 passes through untouched.
//! 4. **Retires its patch**: the fixture's `patch` slot is removed, and so
//!    is the `*.patch.json` it named.
//!
//! Then every output a start or override names gets a name if it has none:
//! the lowest free number (`"1"`, `"2"`, …), project-unique, inserted after
//! its `kind`. `project.json`'s `format` bumps `11` → `12`.
//!
//! A map2d two fixtures share is written once when both freeze to the same
//! bytes; otherwise the later fixture (load order) gets a copy,
//! `<stem>-2.map2d.json`, and its `mapping` names the copy.
//!
//! The step is deterministic: the same files give the same bytes, run to
//! run and host to host (an over-the-air update runs it on a board's
//! project and Studio on the library copy, and the two must hash the same;
//! Q110). Nothing here reads a clock, a random number or a hash map's order.

use std::collections::{BTreeMap, BTreeSet};

use crate::json::JsonNode;
use crate::project_files::{PROJECT_MANIFEST, ProjectFiles};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;
use crate::v11::{
    ExactFit, FixtureLanding, FixtureLayout, FrozenPatching, Landing, OutputPorts, PartIds,
    ProjectLanding, SegmentLanding, SegmentSource, V11Mapping, V11Project, exact_fit,
    freeze_patching, landing, map2d6_document, mint_part_ids, object_ids_and_names,
    objects_to_name, points_document, read_json, refused, scalar, strided_objects,
};

const FROM: u32 = 11;
const TO: u32 = 12;

/// A `PathPoints` fixture's texture when its def gives none
/// (`FixtureDef`'s `default_render_size`).
const DEFAULT_RENDER_SIZE: (u32, u32) = (16, 16);

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    // All or nothing: a refusal anywhere leaves every file as it was.
    let mut working = files.clone();
    let mut working_report = report.clone();
    upgrade(&mut working, &mut working_report)?;
    *files = working;
    *report = working_report;
    Ok(())
}

fn upgrade(files: &mut ProjectFiles, report: &mut UpgradeReport) -> Result<(), UpgradeError> {
    let landed = landing(files)?;
    let project = V11Project::read(files)?;
    let project_name = project_name(files);
    refuse_cut_segments(&project_name, &landed)?;

    let ports: Vec<(&str, &[(u32, Option<u32>)])> = project
        .outputs
        .iter()
        .map(|output| (output.node.as_str(), output.ports.as_slice()))
        .collect();

    // 1–2. Every fixture's frozen mapping, before anything is written.
    let mut plans = Vec::with_capacity(project.fixtures.len());
    for (fixture, landing) in project.fixtures.iter().zip(&landed.fixtures) {
        plans.push(plan_fixture(
            files,
            &fixture.mapping,
            landing,
            &ports,
            report,
        )?);
    }

    // The outputs a start or override names, named if they are not.
    let named = name_outputs(files, &landed, &plans, report)?;
    let output_name = |node: &str| named.get(node).cloned().unwrap_or_default();

    // 3–4. Write.
    let mut written: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut retired: BTreeSet<String> = BTreeSet::new();
    for (landing, plan) in landed.fixtures.iter().zip(&plans) {
        let mut def_edit = DefEdit::default();
        match plan {
            FixturePlan::Untouched => {}
            FixturePlan::Map2d {
                file,
                old,
                ids,
                patching,
            } => {
                let bytes =
                    map2d6_document(file, old, ids, patching, &output_name)?.to_pretty_bytes();
                let target = match written.get(file) {
                    None => file.clone(),
                    Some(first) if *first == bytes => file.clone(),
                    Some(_) => {
                        let copy = free_sibling(files, &written, file, "-2", ".map2d.json");
                        def_edit.mapping_source = Some(file_name(&copy));
                        report.note(format!(
                            "{}: shares {} with an earlier fixture but is patched \
                             differently; it gets its own copy, {}",
                            bare(&landing.node),
                            bare(file),
                            bare(&copy)
                        ));
                        copy
                    }
                };
                written.insert(target.clone(), bytes.clone());
                put_file(files, report, &landing.node, &target, bytes);
            }
            FixturePlan::Points {
                file,
                fit,
                paths,
                patching,
            } => {
                let bytes = points_document(fit, paths, patching, &output_name).to_pretty_bytes();
                written.insert(file.clone(), bytes.clone());
                put_file(files, report, &landing.node, file, bytes);
                def_edit.mapping_source = Some(file_name(file));
                def_edit.becomes_map2d = true;
            }
        }
        if let Some(patch) = &landing.patch_file {
            def_edit.drop_patch = true;
            retired.insert(patch.clone());
        }
        edit_def(files, report, &landing.node, &def_edit)?;
    }
    for patch in &retired {
        if let Some(key) = key_of(files, patch) {
            files.remove(&key);
            report.record_removed(&key);
            report.note(format!(
                "{key}: removed; its patching now lives in the fixture's map2d (format 6)"
            ));
        }
    }
    bump_manifest(files, report)
}

/// What the step does to one fixture.
enum FixturePlan {
    /// No map2d to write: no mapping, or one already at format 6.
    Untouched,
    /// A format 1–5 map2d, rewritten in place.
    Map2d {
        file: String,
        old: JsonNode,
        ids: PartIds,
        patching: FrozenPatching,
    },
    /// Hand-placed lamps, moved into a new map2d.
    Points {
        file: String,
        fit: ExactFit,
        /// Lamps per `points` part, in `first_channel` order.
        paths: Vec<u32>,
        patching: FrozenPatching,
    },
}

fn plan_fixture(
    files: &ProjectFiles,
    mapping: &V11Mapping,
    landing: &FixtureLanding,
    ports: &[OutputPorts<'_>],
    report: &mut UpgradeReport,
) -> Result<FixturePlan, UpgradeError> {
    let node = landing.node.as_str();
    if let (Some(patch), Some(reason)) = (&landing.patch_file, &landing.patch_unread) {
        report.warn(format!(
            "{}: today's engine cannot read this patch ({reason}) and auto-flows {} \
             instead; the upgrade keeps that, and removes the patch",
            bare(patch),
            bare(node)
        ));
    }
    let FixtureLayout::Segments(segments) = &landing.layout else {
        return Ok(FixturePlan::Untouched);
    };
    match mapping {
        V11Mapping::Unset => Ok(FixturePlan::Untouched),
        V11Mapping::Map2d { file } => {
            let old = read_json(files, file, node)?;
            let (authored_ids, names) = object_ids_and_names(file, &old)?;
            let ids = mint_part_ids(&objects_to_name(file, &old, &authored_ids, &names)?);
            let ids_by_segment: Vec<(String, &SegmentLanding)> = segments
                .iter()
                .map(|segment| match &segment.source {
                    SegmentSource::Object {
                        index, instances, ..
                    } => Ok((ids.segment_id(*index as usize, instances), segment)),
                    SegmentSource::PointList { .. } => {
                        Err(refused(file, "a map2d segment is not an object"))
                    }
                })
                .collect::<Result<_, _>>()?;
            let patching = freeze(node, &ids_by_segment, ports, report)?;
            for index in strided_objects(file, &old)? {
                report.note(format!(
                    "{}: object {index}'s `stride` (the Patch tab's rotation step) has no place \
                     in format 6 and is dropped; it never moved a lamp",
                    bare(file)
                ));
            }
            report.note(format!(
                "{}: map2d format 6 for {} ({})",
                bare(file),
                bare(node),
                summary(segments.len(), &patching)
            ));
            Ok(FixturePlan::Map2d {
                file: file.clone(),
                old,
                ids,
                patching,
            })
        }
        V11Mapping::PathPoints { .. } => {
            let def = read_json(files, node, node)?;
            let points = hand_placed(node, &def)?;
            let fit = exact_fit(
                &points.positions,
                points.sample_diameter,
                points.width,
                points.height,
            )
            .map_err(|reason| {
                refused(
                    node,
                    &format!(
                        "its hand-placed lamps can't be kept exactly where they are as a \
                         map2d: {reason}"
                    ),
                )
            })?;
            let ids_by_segment: Vec<(String, &SegmentLanding)> = segments
                .iter()
                .enumerate()
                .map(|(index, segment)| (format!("points{}", index + 1), segment))
                .collect();
            let patching = freeze(node, &ids_by_segment, ports, report)?;
            let file = free_sibling(files, &BTreeMap::new(), node, "", ".map2d.json");
            report.note(format!(
                "{}: its hand-placed lamps move into {} as `points` parts ({})",
                bare(node),
                bare(&file),
                summary(segments.len(), &patching)
            ));
            Ok(FixturePlan::Points {
                file,
                fit,
                paths: points.lamps_per_path,
                patching,
            })
        }
    }
}

fn freeze(
    node: &str,
    segments: &[(String, &SegmentLanding)],
    ports: &[OutputPorts<'_>],
    report: &mut UpgradeReport,
) -> Result<FrozenPatching, UpgradeError> {
    let patching = freeze_patching(segments, ports).map_err(|reason| refused(node, &reason))?;
    if patching.start.is_none() && !segments.is_empty() {
        let why = segments
            .iter()
            .find_map(|(_, segment)| match &segment.landing {
                Landing::Dark { why } => Some(why.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        report.warn(format!(
            "{}: none of its lamps reaches a wire today ({why}); it is written with no start, \
             so it still lights nothing",
            bare(node)
        ));
    }
    Ok(patching)
}

fn summary(segments: usize, patching: &FrozenPatching) -> String {
    let off = patching.overrides.iter().filter(|(_, o)| o.off).count();
    let start = match &patching.start {
        Some(start) => format!(
            "start {} port {} lamp {}",
            bare(&start.output),
            start.port,
            start.lamp
        ),
        None => String::from("no start"),
    };
    format!(
        "{segments} segment(s), {start}, {} override(s), {off} off",
        patching.overrides.len()
    )
}

/// A `PathPoints` mapping as written in its def.
struct HandPlaced {
    /// Every lamp, in `first_channel` order then key order.
    positions: Vec<[f32; 2]>,
    lamps_per_path: Vec<u32>,
    sample_diameter: f32,
    width: u32,
    height: u32,
}

fn hand_placed(node: &str, def: &JsonNode) -> Result<HandPlaced, UpgradeError> {
    let mapping = def
        .get("mapping")
        .ok_or_else(|| refused(node, "it has no mapping"))?;
    let sample_diameter = mapping
        .get("sample_diameter")
        .and_then(scalar::<f32>)
        .ok_or_else(|| refused(node, "its hand-placed lamps have no sample_diameter"))?;
    let (width, height) = match def.get("render_size") {
        None => DEFAULT_RENDER_SIZE,
        Some(size) => (
            size.get("width")
                .and_then(scalar::<u32>)
                .ok_or_else(|| refused(node, "its render_size has no width"))?,
            size.get("height")
                .and_then(scalar::<u32>)
                .ok_or_else(|| refused(node, "its render_size has no height"))?,
        ),
    };
    let mut paths: Vec<(u32, Vec<[f32; 2]>)> = Vec::new();
    for (key, spec) in mapping
        .get("paths")
        .and_then(JsonNode::object)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let first_channel = spec
            .get("first_channel")
            .and_then(scalar::<u32>)
            .ok_or_else(|| refused(node, &format!("path {key} has no first_channel")))?;
        let mut points: Vec<(u32, [f32; 2])> = Vec::new();
        for (index, point) in spec
            .get("points")
            .and_then(JsonNode::object)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let index: u32 = index.parse().map_err(|_| {
                refused(
                    node,
                    &format!("path {key}'s point key {index:?} is not a number"),
                )
            })?;
            let xy = match point {
                JsonNode::Array(items) if items.len() == 2 => {
                    match (scalar::<f32>(&items[0]), scalar::<f32>(&items[1])) {
                        (Some(x), Some(y)) => [x, y],
                        _ => return Err(refused(node, "a hand-placed lamp is not [x, y]")),
                    }
                }
                _ => return Err(refused(node, "a hand-placed lamp is not [x, y]")),
            };
            points.push((index, xy));
        }
        points.sort_by_key(|(index, _)| *index);
        // Lamp k of a path is channel first_channel + k only when its keys
        // run 0, 1, 2…: what the frozen landing counted.
        if points
            .iter()
            .enumerate()
            .any(|(k, (index, _))| *index != k as u32)
        {
            return Err(refused(
                node,
                &format!(
                    "path {key}'s lamps are not keyed 0, 1, 2…; the upgrade only moves \
                     hand-placed lamps that are"
                ),
            ));
        }
        if !points.is_empty() {
            paths.push((
                first_channel,
                points.into_iter().map(|(_, xy)| xy).collect(),
            ));
        }
    }
    paths.sort_by_key(|(first_channel, _)| *first_channel);
    Ok(HandPlaced {
        lamps_per_path: paths
            .iter()
            .map(|(_, points)| points.len() as u32)
            .collect(),
        positions: paths.into_iter().flat_map(|(_, points)| points).collect(),
        sample_diameter,
        width,
        height,
    })
}

/// Refuse a project with a segment that is not one window on one port.
fn refuse_cut_segments(project: &str, landed: &ProjectLanding) -> Result<(), UpgradeError> {
    for fixture in &landed.fixtures {
        let FixtureLayout::Segments(segments) = &fixture.layout else {
            continue;
        };
        for segment in segments {
            let Landing::Cut { why, pieces } = &segment.landing else {
                continue;
            };
            let (what, file) = match &segment.source {
                SegmentSource::Object {
                    index,
                    id,
                    name,
                    instances,
                } => {
                    let mut label = id.clone().unwrap_or_else(|| {
                        if name.is_empty() {
                            format!("object {index}")
                        } else {
                            name.clone()
                        }
                    });
                    if !instances.is_empty() {
                        let copies: Vec<String> = instances.iter().map(u32::to_string).collect();
                        label = format!("{label} (copy {})", copies.join("/"));
                    }
                    (
                        label,
                        fixture
                            .mapping_file
                            .clone()
                            .unwrap_or_else(|| fixture.node.clone()),
                    )
                }
                SegmentSource::PointList { key, .. } => {
                    (format!("path {key}"), fixture.node.clone())
                }
            };
            let mut cuts: BTreeSet<u32> = BTreeSet::new();
            for piece in pieces {
                for edge in [piece.first_lamp, piece.first_lamp + piece.lamps] {
                    let at = edge - segment.first_lamp;
                    if at > 0 && at < segment.lamps {
                        cuts.insert(at);
                    }
                }
            }
            let landed_where: Vec<String> = pieces
                .iter()
                .map(|piece| {
                    let first = piece.first_lamp - segment.first_lamp;
                    format!(
                        "lamps {first}–{} on port {} of {}",
                        first + piece.lamps - 1,
                        piece.port,
                        bare(&piece.output)
                    )
                })
                .collect();
            let cuts: Vec<String> = cuts.iter().map(u32::to_string).collect();
            let at = match cuts.as_slice() {
                [one] => format!("lamp {one}"),
                [first @ .., last] => format!("lamps {} and {last}", first.join(", ")),
                [] => String::from("each lamp that goes elsewhere"),
            };
            return Err(refused(
                &file,
                &format!(
                    "project {project:?}, fixture {}: `{what}` is cut — {why} ({}); the new \
                     mapping format lays each object whole on one port. Export the project, \
                     split `{what}` at {at} into separate objects in {}, and import it again",
                    bare(&fixture.node),
                    landed_where.join(", "),
                    bare(&file)
                ),
            ));
        }
    }
    Ok(())
}

/// Name every output a start or override names and has no name: the
/// lowest free number, project-unique, inserted after its `kind`. Returns
/// every output's name by its definition file.
fn name_outputs(
    files: &mut ProjectFiles,
    landed: &ProjectLanding,
    plans: &[FixturePlan],
    report: &mut UpgradeReport,
) -> Result<BTreeMap<String, String>, UpgradeError> {
    let mut wanted: BTreeSet<&str> = BTreeSet::new();
    for plan in plans {
        let patching = match plan {
            FixturePlan::Map2d { patching, .. } | FixturePlan::Points { patching, .. } => patching,
            FixturePlan::Untouched => continue,
        };
        if let Some(start) = &patching.start {
            wanted.insert(&start.output);
        }
        for (_, entry) in &patching.overrides {
            if let Some(output) = &entry.output {
                wanted.insert(output);
            }
        }
    }
    let mut taken: BTreeSet<String> = landed
        .outputs
        .iter()
        .filter_map(|output| output.name.clone())
        .collect();
    let mut names = BTreeMap::new();
    for output in &landed.outputs {
        let name = match &output.name {
            Some(name) => name.clone(),
            None if wanted.contains(output.node.as_str()) => {
                let name = (1u32..)
                    .map(|n| n.to_string())
                    .find(|name| !taken.contains(name))
                    .unwrap_or_default();
                taken.insert(name.clone());
                insert_output_name(files, report, &output.node, &name)?;
                name
            }
            None => continue,
        };
        names.insert(output.node.clone(), name);
    }
    Ok(names)
}

fn insert_output_name(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
    node: &str,
    name: &str,
) -> Result<(), UpgradeError> {
    edit_json(files, report, node, |def| {
        let members = def
            .object_mut()
            .ok_or_else(|| refused(node, "an output definition is not an object"))?;
        let at = members
            .iter()
            .position(|(key, _)| key == "kind")
            .map_or(0, |index| index + 1);
        members.insert(at, (String::from("name"), JsonNode::string(name)));
        Ok(())
    })?;
    report.note(format!(
        "{}: named {name:?}; a fixture's start or override names it",
        bare(node)
    ));
    Ok(())
}

/// What a fixture's def loses or names.
#[derive(Default)]
struct DefEdit {
    drop_patch: bool,
    /// The file name of the map2d its `mapping` names, when that changes:
    /// a copy beside the shared one, or a new file beside the def.
    mapping_source: Option<String>,
    /// A `PathPoints` mapping becomes a `Map2d` one (its source is then
    /// the file name alone: the new map2d sits beside the def).
    becomes_map2d: bool,
}

fn edit_def(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
    node: &str,
    edit: &DefEdit,
) -> Result<(), UpgradeError> {
    if !edit.drop_patch && edit.mapping_source.is_none() {
        return Ok(());
    }
    edit_json(files, report, node, |def| {
        if edit.drop_patch {
            def.remove("patch");
        }
        if let Some(source) = &edit.mapping_source {
            if edit.becomes_map2d {
                def.set(
                    "mapping",
                    JsonNode::Object(vec![
                        (String::from("kind"), JsonNode::string("Map2d")),
                        (String::from("source"), JsonNode::string(source)),
                    ]),
                );
            } else if let Some(mapping) = def.get_mut("mapping") {
                // The copy sits beside the shared file: the authored path
                // keeps its directories and changes its last name.
                let authored = mapping
                    .get("source")
                    .and_then(JsonNode::as_str)
                    .unwrap_or_default();
                let renamed = match authored.rsplit_once('/') {
                    Some((dir, _)) => format!("{dir}/{source}"),
                    None => source.clone(),
                };
                mapping.set("source", JsonNode::string(&renamed));
            }
        }
        Ok(())
    })
}

/// Read, edit and write back one JSON file, recording it when it changed.
fn edit_json(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
    path: &str,
    edit: impl FnOnce(&mut JsonNode) -> Result<(), UpgradeError>,
) -> Result<(), UpgradeError> {
    let key = key_of(files, path).ok_or_else(|| refused(path, "is not in the project"))?;
    let original = read_json(files, path, path)?;
    let mut edited = original.clone();
    edit(&mut edited)?;
    if edited != original {
        files.replace(&key, edited.to_pretty_bytes());
        report.record_changed(&key);
    }
    Ok(())
}

/// Write a map2d: over the file at `path`, or as a new file beside the
/// fixture's def.
fn put_file(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
    def: &str,
    path: &str,
    bytes: Vec<u8>,
) {
    match key_of(files, path) {
        Some(key) => {
            if files.get(&key) != Some(bytes.as_slice()) {
                files.replace(&key, bytes);
                report.record_changed(&key);
            }
        }
        None => {
            let rooted = key_of(files, def).is_some_and(|key| key.starts_with('/'));
            let key = if rooted {
                path.to_string()
            } else {
                bare(path).to_string()
            };
            files.insert(key.clone(), bytes);
            report.record_added(&key);
        }
    }
}

/// `project.json`'s `format`, 11 → 12.
fn bump_manifest(files: &mut ProjectFiles, report: &mut UpgradeReport) -> Result<(), UpgradeError> {
    let path = format!("/{PROJECT_MANIFEST}");
    edit_json(files, report, &path, |manifest| {
        if manifest.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
            manifest.set("format", JsonNode::u32(TO));
        }
        Ok(())
    })?;
    report.note(format!("{PROJECT_MANIFEST}: format {FROM} → {TO}"));
    Ok(())
}

/// The project's `name`, for refusals.
fn project_name(files: &ProjectFiles) -> String {
    read_json(files, &format!("/{PROJECT_MANIFEST}"), PROJECT_MANIFEST)
        .ok()
        .and_then(|manifest| manifest.get("name").and_then(JsonNode::as_str))
        .unwrap_or_else(|| String::from("(unnamed)"))
}

/// The key `path` is stored under (with or without its leading `/`).
fn key_of(files: &ProjectFiles, path: &str) -> Option<String> {
    let bare = bare(path);
    if files.contains(bare) {
        Some(bare.to_string())
    } else if files.contains(path) {
        Some(path.to_string())
    } else {
        None
    }
}

/// A file beside `beside` named `<its stem><suffix><extension>`, the first
/// of those (then `-2`, `-3`…) that is neither in the project nor written.
fn free_sibling(
    files: &ProjectFiles,
    written: &BTreeMap<String, Vec<u8>>,
    beside: &str,
    suffix: &str,
    extension: &str,
) -> String {
    let (dir, name) = beside.rsplit_once('/').unwrap_or(("", beside));
    let stem = name
        .strip_suffix(".map2d.json")
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name);
    let free = |path: &String| key_of(files, path).is_none() && !written.contains_key(path);
    let first = format!("{dir}/{stem}{suffix}{extension}");
    if free(&first) {
        return first;
    }
    (2u32..)
        .map(|n| format!("{dir}/{stem}-{n}{extension}"))
        .find(free)
        .unwrap_or(first)
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn bare(path: &str) -> &str {
    path.trim_start_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sibling_is_named_beside_its_file() {
        let files: ProjectFiles = [("dome/dome.map2d.json", b"{}".to_vec())]
            .into_iter()
            .collect();
        let none = BTreeMap::new();
        assert_eq!(
            free_sibling(&files, &none, "/dome/dome.map2d.json", "-2", ".map2d.json"),
            "/dome/dome-2.map2d.json"
        );
        assert_eq!(
            free_sibling(&files, &none, "/fixture.json", "", ".map2d.json"),
            "/fixture.map2d.json"
        );
        let taken: ProjectFiles = [("fixture.map2d.json", b"{}".to_vec())]
            .into_iter()
            .collect();
        assert_eq!(
            free_sibling(&taken, &none, "/fixture.json", "", ".map2d.json"),
            "/fixture-2.map2d.json"
        );
    }

    #[test]
    fn the_manifest_format_is_bumped() {
        let mut files: ProjectFiles = [(
            "project.json",
            b"{\n  \"format\": 11,\n  \"name\": \"x\"\n}".to_vec(),
        )]
        .into_iter()
        .collect();
        let mut report = UpgradeReport::new(FROM);
        bump_manifest(&mut files, &mut report).unwrap();
        assert_eq!(
            files.get("project.json"),
            Some(b"{\n  \"format\": 12,\n  \"name\": \"x\"\n}\n".as_slice())
        );
    }
}
