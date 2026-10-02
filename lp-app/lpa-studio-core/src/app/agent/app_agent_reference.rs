//! [`app_agent_reference`]: the project-model reference in the app agent's
//! static system prompt (plan P05, Q10).
//!
//! Knowing how a project fits together is the app agent's real job, and a
//! hand-written description would drift from the model the day a field
//! moves. So every fact here is generated from the product's own sources:
//!
//! - **how a project fits together** — the project the app itself
//!   generates for a XIAO ESP32-C6 (`generate_board_project`), verbatim;
//! - **node kinds** — each kind's writable fields and their JSON types,
//!   walked from the slot shape registry;
//! - **boards** — every catalog board's LED-capable pin labels and GPIOs
//!   (`lpa_boards`);
//! - **playlist cycle** — the cycle value's kinds and fields, from its shape;
//! - **catalog patterns** — slug, name and description (the embedded
//!   catalog);
//! - **worked example** — the golden edit script that builds Sean's project
//!   from Blank (PD9: it is known to work; the eval replays it).
//!
//! Static for a session (PD3). Snapshot-tested with the whole app prompt.

use std::fmt::Write as _;

use lpc_model::{
    NodeDef, NodeKind, SlotAccess, SlotRole, SlotShapeLookup, SlotShapeRegistry, SlotShapeView,
};

use crate::app::home::{CatalogBucket, embedded_examples, generate_board_project};
use crate::app::project::agent_slot_json::describe_type;

/// The board the "how a project fits together" example is generated for.
const EXAMPLE_BOARD: &str = "seeed/xiao-esp32-c6";

/// The kinds a lighting project is wired from, in the order a reader meets
/// them. (Shaders are the shader agent's; patterns arrive whole from the
/// catalog.)
const REFERENCE_KINDS: [NodeKind; 4] = [
    NodeKind::Clock,
    NodeKind::Playlist,
    NodeKind::Fixture,
    NodeKind::Output,
];

/// The worked example: the golden edit script for Sean's project (also the
/// eval's vocabulary oracle).
const WORKED_EXAMPLE: &str =
    include_str!("../../../tests/fixtures/app_agent/scripts/sean-250-d6.json");

/// The generated reference, as Markdown.
pub fn app_agent_reference() -> String {
    let mut p = String::new();
    project_shape_section(&mut p);
    node_kinds_section(&mut p);
    boards_section(&mut p);
    cycle_section(&mut p);
    catalog_section(&mut p);
    let _ = write!(
        p,
        "## Worked example\n\nThis one `edit_project` call builds a project for 250 LEDs \
         on D6 of a Seeed XIAO ESP32-C6 from a new, empty project, with three colourful \
         patterns on a 30-second cycle, and saves it:\n\n```json\n{WORKED_EXAMPLE}```\n"
    );
    p
}

/// The app's own first project for a board, file by file (the pattern's
/// shader files left out — they are the pattern, not the wiring).
fn project_shape_section(p: &mut String) {
    p.push_str("## How a project fits together\n\n");
    let Ok(project) = generate_board_project(EXAMPLE_BOARD, Some("Example")) else {
        return;
    };
    let _ = writeln!(
        p,
        "This is the project the app itself creates for a {EXAMPLE_BOARD} board: a clock, \
         a playlist playing one pattern, a fixture (the LED layout: a one-row strip of \
         {} lamps in its `.map2d.json`, rendered at {}×8 and sampled `direct`), and an \
         output sending the fixture's colours to pin {}. Nodes talk over named buses: the \
         clock publishes `bus:time`, the playlist reads it and publishes `bus:visual.out`, \
         the fixture turns that into `bus:control.out`, and the output sends it to the \
         wire. A strip of N LEDs is the same fixture with N lamps and render width N.\n",
        crate::app::home::DEFAULT_STRIP_PIXELS,
        crate::app::home::DEFAULT_STRIP_PIXELS,
        project.endpoint
    );
    for (path, bytes) in &project.files {
        if path.starts_with("effect/") {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let _ = write!(p, "`{path}`:\n```json\n{}\n```\n", text.trim_end());
    }
    p.push('\n');
}

/// Each reference kind's writable fields and their JSON types.
fn node_kinds_section(p: &mut String) {
    p.push_str(
        "## Node kinds\n\nThe fields you can `set` on each kind, with the JSON they take \
         (`set` a group of fields with an object; a field marked optional is made present \
         by setting it).\n\n",
    );
    let registry = SlotShapeRegistry::default();
    for kind in REFERENCE_KINDS {
        let Some(root) = registry.get_shape(NodeDef::default_for_kind(kind).shape_id()) else {
            continue;
        };
        let _ = writeln!(p, "### {kind:?}");
        let Some(len) = root.record_fields_len() else {
            continue;
        };
        for field in (0..len).filter_map(|index| root.record_field(index)) {
            if !authorable(&registry, field) {
                continue;
            }
            let described = describe_shape(&registry, field.shape(), 0);
            if described == "{}" {
                continue; // a group with nothing an author writes
            }
            let _ = writeln!(p, "- `{}`: {described}", field.name_str());
        }
        p.push('\n');
    }
}

/// A shape the way the model writes its JSON.
fn describe_shape(registry: &SlotShapeRegistry, shape: SlotShapeView<'_>, depth: usize) -> String {
    let Some(shape) = resolve(registry, shape) else {
        return "?".to_string();
    };
    if let Some(value) = shape.value_shape() {
        return describe_type(&value.ty_owned());
    }
    if let Some(some) = shape.option_some() {
        return format!("optional {}", describe_shape(registry, some, depth));
    }
    if let Some(value) = shape.map_value() {
        let key = match shape.map_key() {
            Some(lpc_model::SlotMapKeyShape::String) | None => "name",
            Some(_) => "number",
        };
        if depth >= 2 {
            return format!("map of {key} → …");
        }
        return format!(
            "map of {key} → {}",
            describe_shape(registry, value, depth + 1)
        );
    }
    if shape.is_enum() {
        let variants: Vec<String> = (0..32)
            .map_while(|index| shape.enum_variant(index))
            .map(|variant| variant.name_str().to_string())
            .collect();
        return format!("one of `{{\"kind\": …}}`: {}", variants.join(", "));
    }
    if let Some(len) = shape.record_fields_len() {
        if depth >= 2 {
            return "{…}".to_string();
        }
        let fields: Vec<String> = (0..len)
            .filter_map(|index| shape.record_field(index))
            .filter(|field| authorable(registry, *field))
            .map(|field| {
                format!(
                    "{}: {}",
                    field.name_str(),
                    describe_shape(registry, field.shape(), depth + 1)
                )
            })
            .collect();
        return format!("{{{}}}", fields.join(", "));
    }
    "?".to_string()
}

/// A field an author writes: writable, not a debug override, not a runtime
/// product (those are wired through `bindings`, never set), and not an
/// empty group.
fn authorable(
    registry: &SlotShapeRegistry,
    field: lpc_model::slot::SlotFieldShapeView<'_>,
) -> bool {
    if !field.is_writable() || field.role() == SlotRole::Debug {
        return false;
    }
    let Some(shape) = resolve(registry, field.shape()) else {
        return false;
    };
    if let Some(value) = shape.value_shape()
        && matches!(value.ty_owned(), lpc_model::LpType::Product(_))
    {
        return false;
    }
    shape.record_fields_len() != Some(0)
}

fn resolve<'s>(
    registry: &'s SlotShapeRegistry,
    mut shape: SlotShapeView<'s>,
) -> Option<SlotShapeView<'s>> {
    for _ in 0..32 {
        if let Some(id) = shape.ref_id() {
            shape = registry.get_shape(id)?;
        } else if let Some(projected) = shape.custom_shape() {
            shape = projected;
        } else {
            return Some(shape);
        }
    }
    None
}

/// Every catalog board: its id, its name, its LED-capable pin labels.
fn boards_section(p: &mut String) {
    p.push_str(
        "## Boards and pins\n\nAn output port's `endpoint` is `ws281x:local:<pin label>`, \
         where the label is the board's own silkscreen label. A label means a different \
         pin, or nothing, on a different board. The project's `target` names its board \
         (`set_target`). Boards, with the labels an LED strip can be wired to:\n\n",
    );
    for board in lpa_boards::all_boards() {
        let wires: Vec<String> = board
            .output_wires()
            .map(|(label, gpio)| format!("{label} (GPIO{gpio})"))
            .collect();
        if wires.is_empty() {
            continue;
        }
        let _ = writeln!(
            p,
            "- `{}` — {} ({}): {}{}",
            board.board_id,
            board.display_name,
            board.soc,
            wires.join(", "),
            board
                .default_led_wire()
                .map(|wire| format!("; default {wire}"))
                .unwrap_or_default()
        );
    }
    p.push('\n');
}

/// The playlist's `cycle` value: its kinds and fields, from its shape.
fn cycle_section(p: &mut String) {
    let registry = SlotShapeRegistry::default();
    let Some(root) = registry.get_shape(NodeDef::default_for_kind(NodeKind::Playlist).shape_id())
    else {
        return;
    };
    let Some(cycle) = (0..root.record_fields_len().unwrap_or(0))
        .filter_map(|index| root.record_field(index))
        .find(|field| field.name_str() == "cycle")
    else {
        return;
    };
    let _ = write!(
        p,
        "## Playlist cycle\n\nA playlist plays one entry at a time. Its `cycle` is {}. \
         `step_seconds` is how long each entry plays before the next; `fade_seconds` is \
         the crossfade between them. Without a `cycle` the playlist holds on one entry.\n\n",
        describe_shape(&registry, cycle.shape(), 0)
    );
}

/// The catalog's patterns: what `import_pattern` can bring in.
fn catalog_section(p: &mut String) {
    p.push_str(
        "## Catalog patterns\n\n`import_pattern` copies one into the project (into a \
         playlist with `in_playlist`):\n\n",
    );
    for example in embedded_examples()
        .iter()
        .filter(|example| example.bucket == CatalogBucket::Patterns)
    {
        let _ = writeln!(
            p,
            "- `{}` — {}: {}",
            example.slug, example.name, example.description
        );
    }
    p.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole app system prompt (doctrine + this reference) is
    /// snapshot-tested like the shader agent's; regenerate with
    /// `LPA_AGENT_UPDATE_SNAPSHOTS=1 cargo test -p lpa-studio-core app_agent_system_prompt`.
    #[test]
    fn app_agent_system_prompt_matches_its_snapshot() {
        let prompt = lpa_agent::build_app_system_prompt(&app_agent_reference());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/app/agent/app_agent_system_prompt_snapshot.md");
        if std::env::var_os("LPA_AGENT_UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &prompt).expect("write the snapshot");
        }
        let snapshot = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            prompt == snapshot,
            "the app system prompt changed; review it and regenerate with \
             LPA_AGENT_UPDATE_SNAPSHOTS=1 ({})",
            path.display()
        );
    }

    /// Room for a 128k-context open model (D13): the static prompt stays
    /// under ~12k tokens (≈ 4 chars a token).
    #[test]
    fn the_app_system_prompt_fits_a_small_context() {
        let prompt = lpa_agent::build_app_system_prompt(&app_agent_reference());
        assert!(prompt.len() / 4 < 12_000, "≈{} tokens", prompt.len() / 4);
    }

    #[test]
    fn the_reference_names_what_sean_needs() {
        let reference = app_agent_reference();
        for needle in [
            "`seeed/xiao-esp32-c6`",
            "D6 (GPIO16)",
            "`palette-waves`",
            "step_seconds",
            "`render_size`",
            "\"edit_project\"",
        ] {
            let found = reference.contains(needle)
                || (needle == "\"edit_project\"" && reference.contains("edit_project"));
            assert!(found, "the reference lacks {needle}");
        }
    }
}
