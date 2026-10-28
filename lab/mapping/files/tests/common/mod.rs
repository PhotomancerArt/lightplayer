//! What the journeys share: a small project built around one of
//! `lpc-mapping`'s format-6 journey documents, opened the way Studio's files
//! are; an editor laid out as the page lays it out; and reading where lamps
//! land and what the inspector says.

#![allow(dead_code, reason = "each journey uses only some of these")]

use lab_mapping_files::embedded::Embedded;
use lab_mapping_files::{MemFiles, Opened};
use lab_mapping_model::*;
use serde_json::{Value, json};

pub fn open(e: &Embedded) -> Opened {
    Opened::open(&e.files()).unwrap_or_else(|err| panic!("{}: {err}", e.key))
}

/// One of `lpc-mapping`'s format-6 journey documents
/// (`lp-core/lpc-mapping/tests/format6/<name>.map2d.json`).
pub fn format6_doc(name: &str) -> String {
    let path = format!(
        "{}/../../../lp-core/lpc-mapping/tests/format6/{name}.map2d.json",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A project at project format 12 around one mapping document: a fixture
/// (`fixture`) whose mapping is `map`, and one output named `output` (as the
/// document's start names it) with these ports, as (endpoint, count in
/// lamps; `None` takes the rest).
pub fn project_around(map: &str, output: &str, ports: &[(&str, Option<u32>)]) -> MemFiles {
    let ports: serde_json::Map<String, Value> = ports
        .iter()
        .enumerate()
        .map(|(k, (endpoint, count))| {
            let port = match count {
                Some(n) => json!({ "endpoint": endpoint, "count": n }),
                None => json!({ "endpoint": endpoint }),
            };
            (k.to_string(), port)
        })
        .collect();
    let files = [
        ("project.json", json!({ "format": 12, "name": "journey" })),
        (
            "module.json",
            json!({
                "kind": "Module",
                "nodes": {
                    "fixture": { "ref": "./fixture.json" },
                    "output": { "ref": "./output.json" },
                },
            }),
        ),
        (
            "fixture.json",
            json!({
                "kind": "Fixture",
                "mapping": { "kind": "Map2d", "source": "fixture.map2d.json" },
                "color_order": "rgb",
            }),
        ),
        (
            "output.json",
            json!({ "kind": "Output", "name": output, "ports": ports }),
        ),
    ];
    let mut out = MemFiles::new(files.map(|(path, v)| (path, v.to_string())));
    out.files
        .insert("fixture.map2d.json".into(), map.to_string());
    out
}

/// Open a project built in memory; it must open.
pub fn open_files(files: &MemFiles) -> Opened {
    Opened::open(files).unwrap_or_else(|err| panic!("{err}"))
}

/// The one fixture of a project built by [`project_around`].
pub fn fixture() -> NodeId {
    NodeId::new("fixture")
}

/// The one output of a project built by [`project_around`].
pub fn output() -> NodeId {
    NodeId::new("output")
}

/// A part of the fixture, by its id.
pub fn part(id: &str) -> PartRef {
    PartRef::parse(&fixture(), id).unwrap_or_else(|| panic!("a part id: {id}"))
}

/// A node of the fixture's tree, by its id (`circle1/r3`, `path1@2`).
pub fn node(id: &str) -> TreeRef {
    TreeRef::parse(&fixture(), id).unwrap_or_else(|| panic!("a node id: {id}"))
}

/// An editor on `p`, laid out the way the page does it.
pub fn editor_of(p: Project) -> Editor {
    let mut e = Editor::new(p);
    let placement = lab_layout::canvas_layout(&e.layout_input());
    e.set_placement(&placement);
    e.resize(Vec2::new(1200.0, 800.0));
    e
}

/// Where one lamp lands: (port key, pixel).
pub fn landing(p: &Project, segment: &TreeRef, lamp: u32) -> Option<(String, u32)> {
    p.effective_patch()
        .landing(segment, lamp)
        .map(|l| (l.port, l.pixel))
}

/// The port a segment starts on and its first and last pixels, in lamp
/// order (a reversed segment's first pixel is its highest).
pub fn span(p: &Project, segment: &TreeRef) -> Option<(String, u32, u32)> {
    let n = p.segment(segment)?.lamps.len() as u32;
    let first = landing(p, segment, 0)?;
    let last = landing(p, segment, n - 1)?;
    Some((first.0, first.1, last.1))
}

/// Every patching row the inspector shows for the selection.
pub fn rows(e: &Editor) -> Vec<UiItem> {
    e.view_inspector()
        .sections
        .into_iter()
        .flat_map(|s| s.items)
        .collect()
}

/// A choice row's options and what is chosen (the last with that label:
/// the segment's, after its part's).
pub fn choice(e: &Editor, label: &str) -> (Vec<String>, Option<usize>) {
    rows(e)
        .into_iter()
        .rev()
        .find_map(|i| match i {
            UiItem::PatchChoice {
                label: l,
                options,
                chosen,
                ..
            } if l == label => Some((options, chosen)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {label} choice"))
}

/// Choose the option of a choice row that starts with `words`.
pub fn choose(e: &mut Editor, field: PatchField, label: &str, words: &str) {
    let (options, _) = choice(e, label);
    let option = options
        .iter()
        .position(|o| o.starts_with(words))
        .unwrap_or_else(|| panic!("{label} has no {words}: {options:?}"));
    e.patch_input(PatchInput::Choose { field, option });
}

/// The hints the inspector shows.
pub fn hints(e: &Editor) -> Vec<String> {
    rows(e)
        .into_iter()
        .filter_map(|i| match i {
            UiItem::Hint(h) => Some(h),
            _ => None,
        })
        .collect()
}

/// Press and release a key.
pub fn key(e: &mut Editor, k: Key, mods: Mods) {
    e.key_down(k.clone(), mods);
    e.key_up(k, mods);
}

pub fn none() -> Mods {
    Mods::default()
}

pub fn shift() -> Mods {
    Mods {
        shift: true,
        ..Mods::default()
    }
}
