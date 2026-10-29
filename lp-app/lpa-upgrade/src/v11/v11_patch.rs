//! A format-11 fixture's patch document (`*.patch.json`, formats 1–3) and
//! where it places the fixture's lamps, in lamps on a named or the default
//! output.
//!
//! Frozen from `lp-core/lpc-mapping/src/patch.rs` at origin/main b9f862fb2:
//! the three formats as `PatchDoc::from_json` reads them (keyed format 1,
//! rows at 2 and 3, `flow` at 3), and `resolve_patch` with the context the
//! engine's fixture passes (no allowed-output set, no default-output name):
//! one window per object path (a later duplicate is dropped), rows before
//! reflow, every lamp no row claims flowing on after the highest anchored
//! end on the default output — or nowhere, under `flow: manual`.

use super::v11_files::{refused, scalar};
use super::v11_segments::{SegmentSource, V11Segment, check_object_id};
use crate::json::JsonNode;
use crate::upgrade_error::UpgradeError;

/// A read patch document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct V11PatchDoc {
    format: u32,
    /// `flow: manual` (format 3): lamps no row places are on no wire.
    pub(crate) manual: bool,
    pub(crate) entries: Vec<PatchEntry>,
}

/// One row: this fixture source goes at that wire lamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatchEntry {
    source: PatchSource,
    /// The output's authored name; `None` = the default output.
    output: Option<String>,
    lamp: u32,
    reversed: bool,
    offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatchSource {
    /// `/id/k/…` with an optional node-relative sub-span.
    Path {
        id: String,
        instances: Vec<u32>,
        range: Option<PatchRange>,
    },
    /// Fixture-relative lamps.
    Range(PatchRange),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PatchRange {
    start: u32,
    /// Absent: to the end.
    count: Option<u32>,
}

/// One resolved placement, in lamps (`PatchedRange`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatchedRun {
    /// First fixture lamp.
    pub(crate) start: u32,
    pub(crate) count: u32,
    /// First wire lamp of the run's window on its output.
    pub(crate) lamp: u32,
    pub(crate) reversed: bool,
    /// Rotation within the window, in lamps.
    pub(crate) offset: u32,
    /// The named output, or `None` for the default output.
    pub(crate) output: Option<String>,
}

impl PatchedRun {
    pub(crate) fn end(&self) -> u32 {
        self.start.saturating_add(self.count)
    }

    pub(crate) fn lamp_end(&self) -> u32 {
        self.lamp.saturating_add(self.count)
    }
}

/// Read a patch document. Anything `PatchDoc::from_json` would not read —
/// including a format above 3 — is refused: today the engine ignores such a
/// file and auto-flows the fixture, which is not something an upgrade should
/// write down on its own.
pub(crate) fn read_patch(file: &str, doc: &JsonNode) -> Result<V11PatchDoc, UpgradeError> {
    let unreadable = |reason: &str| {
        refused(
            file,
            &format!(
                "the engine cannot read this patch ({reason}) and auto-flows the fixture \
                 instead; fix the patch or remove it, then upgrade"
            ),
        )
    };
    let format = doc
        .get("format")
        .and_then(scalar::<u32>)
        .ok_or_else(|| unreadable("it has no format"))?;
    match format {
        1 => read_v1(doc).map_err(|reason| unreadable(&reason)),
        2 | 3 => read_rows(doc, format).map_err(|reason| unreadable(&reason)),
        _ => Err(unreadable(&format!(
            "format {format}; this build reads patch formats 1 to 3"
        ))),
    }
}

fn read_v1(doc: &JsonNode) -> Result<V11PatchDoc, String> {
    let entries = match doc.get("entries") {
        None => Vec::new(),
        Some(JsonNode::Array(entries)) => entries
            .iter()
            .map(read_v1_entry)
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(String::from("\"entries\" is not an array")),
    };
    Ok(V11PatchDoc {
        format: 1,
        manual: false,
        entries,
    })
}

fn read_v1_entry(entry: &JsonNode) -> Result<PatchEntry, String> {
    let range = entry.get("range").ok_or("an entry has no range")?;
    let at = entry.get("at").ok_or("an entry has no at")?;
    let start = range
        .get("start")
        .and_then(scalar::<u32>)
        .ok_or("a range has no start")?;
    let count = optional_u32(range.get("count"), "a range's count")?;
    let lamp = at
        .get("channel")
        .and_then(scalar::<u32>)
        .ok_or("an anchor has no channel")?;
    let output = match at.get("output") {
        None => None,
        Some(name) => Some(name.as_str().ok_or("an anchor's output is not a string")?),
    };
    let reversed = match entry.get("reversed") {
        None => false,
        Some(flag) => scalar::<bool>(flag).ok_or("reversed is not a boolean")?,
    };
    let offset = optional_u32(entry.get("offset"), "an offset")?.unwrap_or(0);
    Ok(PatchEntry {
        source: PatchSource::Range(PatchRange { start, count }),
        output,
        lamp,
        reversed,
        offset,
    })
}

fn read_rows(doc: &JsonNode, format: u32) -> Result<V11PatchDoc, String> {
    let manual = match (doc.get("flow"), format) {
        (None, _) => false,
        (Some(flow), 3) => match flow.as_str().as_deref() {
            Some("auto") => false,
            Some("manual") => true,
            Some(other) => return Err(format!("unknown flow {other:?}")),
            None => return Err(String::from("\"flow\" must be \"auto\" or \"manual\"")),
        },
        (Some(_), _) => return Err(String::from("\"flow\" is a format-3 field")),
    };
    let outputs: Vec<String> = match doc.get("outputs") {
        None => Vec::new(),
        Some(JsonNode::Array(names)) => names
            .iter()
            .map(|name| name.as_str().ok_or("outputs table entries must be strings"))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(String::from("\"outputs\" must be an array of names")),
    };
    let entries = match doc.get("entries") {
        None => Vec::new(),
        Some(JsonNode::Array(rows)) => rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                read_row(row, &outputs).map_err(|reason| format!("entry row {index}: {reason}"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(String::from("\"entries\" must be an array of rows")),
    };
    Ok(V11PatchDoc {
        format,
        manual,
        entries,
    })
}

/// One `[from, out#, lamp, flags?, offset?]` row.
fn read_row(row: &JsonNode, outputs: &[String]) -> Result<PatchEntry, String> {
    let JsonNode::Array(columns) = row else {
        return Err(String::from("a row is not an array"));
    };
    if columns.len() < 3 || columns.len() > 5 {
        return Err(String::from("rows carry 3 to 5 columns"));
    }
    let source = read_source(&columns[0])?;
    let out_index = scalar::<i64>(&columns[1]).ok_or("out# must be an integer")?;
    let output = if out_index == -1 {
        None
    } else {
        Some(
            usize::try_from(out_index)
                .ok()
                .and_then(|index| outputs.get(index))
                .ok_or("out# indexes past the outputs table")?
                .clone(),
        )
    };
    let lamp = scalar::<u32>(&columns[2]).ok_or("lamp must be a non-negative integer")?;
    let reversed = match columns.get(3) {
        None => false,
        Some(flags) => match flags.as_str().as_deref() {
            Some("") => false,
            Some("r") => true,
            Some(other) => return Err(format!("unknown flags {other:?}")),
            None => return Err(String::from("flags must be a string")),
        },
    };
    let offset = match columns.get(4) {
        None => 0,
        Some(offset) => scalar::<u32>(offset).ok_or("offset must be a non-negative integer")?,
    };
    Ok(PatchEntry {
        source,
        output,
        lamp,
        reversed,
        offset,
    })
}

/// The `from` column: a path string, `[path, start, count?]`, or
/// `[start, count?]`.
fn read_source(from: &JsonNode) -> Result<PatchSource, String> {
    if let Some(text) = from.as_str() {
        let (id, instances) = read_object_path(&text)?;
        return Ok(PatchSource::Path {
            id,
            instances,
            range: None,
        });
    }
    let JsonNode::Array(parts) = from else {
        return Err(String::from("from must be a path string or an array"));
    };
    match parts.first() {
        Some(first) if first.as_str().is_some() => {
            let (id, instances) = read_object_path(&first.as_str().unwrap_or_default())?;
            let range = read_range(&parts[1..])
                .ok_or("a path sub-span is [path, start] or [path, start, count]")?;
            Ok(PatchSource::Path {
                id,
                instances,
                range: Some(range),
            })
        }
        Some(first) if first.is_number() => Ok(PatchSource::Range(
            read_range(parts).ok_or("a range source is [start] or [start, count]")?,
        )),
        _ => Err(String::from(
            "from must be a path string, [path, start, count?], or [start, count?]",
        )),
    }
}

fn read_range(parts: &[JsonNode]) -> Option<PatchRange> {
    match parts {
        [start] => Some(PatchRange {
            start: scalar(start)?,
            count: None,
        }),
        [start, count] => Some(PatchRange {
            start: scalar(start)?,
            count: Some(scalar(count)?),
        }),
        _ => None,
    }
}

/// `/id/k/…` (`MapObjectPath::parse`).
fn read_object_path(text: &str) -> Result<(String, Vec<u32>), String> {
    let rest = text
        .strip_prefix('/')
        .ok_or_else(|| format!("invalid object path {text:?}: must start with '/'"))?;
    let mut segments = rest.split('/');
    let id = segments.next().unwrap_or_default();
    check_object_id(id).map_err(|reason| format!("invalid object path {text:?}: {reason}"))?;
    let mut instances = Vec::new();
    for segment in segments {
        let instance: u32 = segment.parse().map_err(|_| {
            format!("invalid object path {text:?}: instance segments must be integers")
        })?;
        if segment != instance.to_string() {
            return Err(format!(
                "invalid object path {text:?}: instance segments must be canonical integers"
            ));
        }
        instances.push(instance);
    }
    Ok((id.to_string(), instances))
}

fn optional_u32(node: Option<&JsonNode>, what: &str) -> Result<Option<u32>, String> {
    match node {
        None => Ok(None),
        Some(node) => scalar::<u32>(node)
            .map(Some)
            .ok_or_else(|| format!("{what} is not a count")),
    }
}

/// `resolve_patch` with the fixture's context: the runs, in authoring order
/// then reflow; or the kernel's refusal, in words.
///
/// A later row addressing an object path an earlier row addressed is
/// dropped (the kernel's per-entry degrade); its lamps reflow.
pub(crate) fn resolve_patch(
    lamps: u32,
    segments: &[V11Segment],
    doc: &V11PatchDoc,
) -> Result<Vec<PatchedRun>, String> {
    let mut anchored: Vec<PatchedRun> = Vec::with_capacity(doc.entries.len());
    for (index, entry) in doc.entries.iter().enumerate() {
        if let PatchSource::Path { id, instances, .. } = &entry.source
            && doc.entries[..index].iter().any(|earlier| {
                matches!(&earlier.source, PatchSource::Path { id: other_id, instances: other, .. }
                    if other_id == id && other == instances)
            })
        {
            continue;
        }
        let run = resolve_entry(index, entry, doc.format, lamps, segments)?;
        for (other_index, other) in anchored.iter().enumerate() {
            if let Some(lamp) = overlap_start(run.start, run.end(), other.start, other.end()) {
                return Err(format!(
                    "patch entries {other_index} and {index} both place fixture lamp {lamp}"
                ));
            }
            if run.output == other.output
                && let Some(lamp) =
                    overlap_start(run.lamp, run.lamp_end(), other.lamp, other.lamp_end())
            {
                return Err(format!(
                    "patch entries {other_index} and {index} both land on wire lamp {lamp}"
                ));
            }
        }
        anchored.push(run);
    }

    let mut resolved = anchored.clone();
    if doc.manual {
        return Ok(resolved);
    }

    let mut claimed = anchored;
    claimed.sort_unstable_by_key(|run| run.start);
    let mut cursor = resolved
        .iter()
        .filter(|run| run.output.is_none())
        .map(PatchedRun::lamp_end)
        .max()
        .unwrap_or(0);
    let mut lamp = 0u32;
    let sentinel = PatchedRun {
        start: lamps,
        count: 0,
        lamp: 0,
        reversed: false,
        offset: 0,
        output: None,
    };
    for run in claimed.iter().chain(core::iter::once(&sentinel)) {
        if run.start > lamp {
            let count = run.start - lamp;
            resolved.push(PatchedRun {
                start: lamp,
                count,
                lamp: cursor,
                reversed: false,
                offset: 0,
                output: None,
            });
            cursor = cursor.saturating_add(count);
        }
        lamp = lamp.max(run.end());
    }
    Ok(resolved)
}

/// One row's fixture-relative run, lowered and validated (`resolve_entry`).
fn resolve_entry(
    index: usize,
    entry: &PatchEntry,
    format: u32,
    lamps: u32,
    segments: &[V11Segment],
) -> Result<PatchedRun, String> {
    if format == 1 {
        if let Some(output) = &entry.output {
            return Err(format!(
                "patch entry {index}: names output {output:?}, which format 1 cannot express"
            ));
        }
        if entry.offset != 0 {
            return Err(format!(
                "patch entry {index}: carries a rotation offset, which format 1 cannot express"
            ));
        }
    }
    let (start, count) = match &entry.source {
        PatchSource::Range(range) => clamp_range(index, range, 0, lamps, "the fixture")?,
        PatchSource::Path {
            id,
            instances,
            range,
        } => {
            // The addressed node's lamps: every segment of the object whose
            // copy path extends the addressed one.
            let matching: Vec<&V11Segment> = segments
                .iter()
                .filter(|segment| match &segment.source {
                    SegmentSource::Object {
                        id: Some(segment_id),
                        instances: segment_instances,
                        ..
                    } => {
                        segment_id == id
                            && segment_instances.len() >= instances.len()
                            && segment_instances[..instances.len()] == instances[..]
                    }
                    _ => false,
                })
                .collect();
            let Some(first) = matching.first() else {
                return Err(format!(
                    "patch entry {index}: path names no object or instance in the mapping"
                ));
            };
            let node_start = first.first_lamp;
            let node_count: u32 = matching.iter().map(|segment| segment.lamps).sum();
            match range {
                None => (node_start, node_count),
                Some(range) => {
                    clamp_range(index, range, node_start, node_count, "the addressed node")?
                }
            }
        }
    };
    Ok(PatchedRun {
        start,
        count,
        lamp: entry.lamp,
        reversed: entry.reversed,
        offset: if count > 0 { entry.offset % count } else { 0 },
        output: entry.output.clone(),
    })
}

fn clamp_range(
    index: usize,
    range: &PatchRange,
    base: u32,
    extent: u32,
    what: &str,
) -> Result<(u32, u32), String> {
    if range.start >= extent {
        return Err(format!(
            "patch entry {index}: starts at lamp {} but {what} has {extent}",
            range.start
        ));
    }
    let remaining = extent - range.start;
    let count = range.count.unwrap_or(remaining);
    if count == 0 {
        return Err(format!("patch entry {index}: places zero lamps"));
    }
    if count > remaining {
        return Err(format!(
            "patch entry {index}: places {count} lamps from lamp {}, past {what}'s {extent}",
            range.start
        ));
    }
    Ok((base + range.start, count))
}

fn overlap_start(start: u32, end: u32, other_start: u32, other_end: u32) -> Option<u32> {
    let first = start.max(other_start);
    (first < end.min(other_end)).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> V11PatchDoc {
        read_patch("f.patch.json", &JsonNode::parse(text).unwrap()).unwrap()
    }

    fn object(id: &str, first_lamp: u32, lamps: u32, copy: Option<u32>) -> V11Segment {
        V11Segment {
            source: SegmentSource::Object {
                index: 0,
                id: Some(id.to_string()),
                name: String::new(),
                instances: copy.into_iter().collect(),
            },
            first_lamp,
            lamps,
        }
    }

    fn placed(runs: &[PatchedRun]) -> Vec<(u32, u32, u32, bool, u32)> {
        runs.iter()
            .map(|run| (run.start, run.count, run.lamp, run.reversed, run.offset))
            .collect()
    }

    /// Rows place first; the lamps no row claims flow on after the highest
    /// anchored end on the default output, in lamp order.
    #[test]
    fn rows_place_first_and_the_rest_flow_on_after_them() {
        let runs = resolve_patch(
            10,
            &[],
            &doc(r#"{"format":1,"entries":[{"range":{"start":2,"count":3},"at":{"channel":20}}]}"#),
        )
        .unwrap();
        assert_eq!(
            placed(&runs),
            vec![
                (2, 3, 20, false, 0),
                (0, 2, 23, false, 0),
                (5, 5, 25, false, 0)
            ]
        );
    }

    /// A named output's row neither consumes nor extends the default
    /// output's cursor.
    #[test]
    fn a_named_outputs_row_does_not_move_the_default_cursor() {
        let runs = resolve_patch(
            6,
            &[object("a", 0, 3, None)],
            &doc(r#"{"format":2,"outputs":["far"],"entries":[["/a",0,40]]}"#),
        )
        .unwrap();
        assert_eq!(
            placed(&runs),
            vec![(0, 3, 40, false, 0), (3, 3, 0, false, 0)]
        );
    }

    /// Manual flow places the rows and nothing else.
    #[test]
    fn manual_flow_places_only_its_rows() {
        let runs = resolve_patch(
            6,
            &[object("a", 0, 3, None), object("b", 3, 3, None)],
            &doc(r#"{"format":3,"flow":"manual","entries":[["/b",-1,0]]}"#),
        )
        .unwrap();
        assert_eq!(placed(&runs), vec![(3, 3, 0, false, 0)]);
    }

    /// A rotation is kept modulo the run's length.
    #[test]
    fn a_rotation_is_kept_modulo_the_runs_length() {
        let runs = resolve_patch(
            5,
            &[object("a", 0, 5, None)],
            &doc(r#"{"format":2,"entries":[["/a",-1,0,"r",7]]}"#),
        )
        .unwrap();
        assert_eq!(placed(&runs), vec![(0, 5, 0, true, 2)]);
    }

    /// A path with no copy index addresses every copy of a repeat, as one
    /// run.
    #[test]
    fn a_repeat_path_without_a_copy_addresses_every_copy() {
        let segments = [object("r", 0, 4, Some(0)), object("r", 4, 4, Some(1))];
        let runs = resolve_patch(
            8,
            &segments,
            &doc(r#"{"format":2,"entries":[["/r",-1,10],["/r/1",-1,0]]}"#),
        );
        // `/r` and `/r/1` are different paths, so both place — and claim
        // lamps 4..8 twice, which the kernel refuses.
        assert!(runs.unwrap_err().contains("both place fixture lamp 4"));
        let runs = resolve_patch(
            8,
            &segments,
            &doc(r#"{"format":2,"entries":[["/r",-1,10]]}"#),
        )
        .unwrap();
        assert_eq!(placed(&runs), vec![(0, 8, 10, false, 0)]);
    }

    /// A later row addressing the same path is dropped; its lamps reflow.
    #[test]
    fn a_duplicate_path_is_dropped_and_its_lamps_reflow() {
        let runs = resolve_patch(
            3,
            &[object("a", 0, 3, None)],
            &doc(r#"{"format":2,"entries":[["/a",-1,5],["/a",-1,0]]}"#),
        )
        .unwrap();
        assert_eq!(placed(&runs), vec![(0, 3, 5, false, 0)]);
    }

    /// A patch at a format this build does not read is refused, by name.
    #[test]
    fn a_patch_above_format_3_is_refused() {
        let error = read_patch(
            "f.patch.json",
            &JsonNode::parse(r#"{"format":4,"entries":[]}"#).unwrap(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("format 4"), "{error}");
    }
}
