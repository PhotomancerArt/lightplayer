//! The Fixtures and Outputs panels — the project tree's "multi-node
//! custom faces" (spike round-2 ruling): derived slices over the #409
//! patch-surface DTOs, sharing the surface's ONE core-owned selection
//! (`ProjectEditorOp::PatchSelect`). Read-only in this pass: rows and
//! cells SELECT; the verbs stay on the interim `/patch` page until the
//! unified-editor plan re-houses them as the Mapping view's patching
//! mode.
//!
//! Density rules (spike §3, ratified): the Fixtures panel is
//! fixture → instance rows with text channel chips (port identity is
//! text + position — port hues were cut); the Outputs panel is a flat
//! stack of slim section headers (chevron · name · occupancy) with their
//! port rows beneath — each box opens INDEPENDENTLY, all open by default
//! (R4-3, replacing the one-at-a-time rule: the rail reads like the
//! Fixtures panel, and collapsing is how a radiance-scale project gets
//! its 10×32 back under control).

use dioxus::prelude::*;
use lpa_mapping_editor::{
    MapEditorSession, MapSelection, ObjectPropertiesPane, ShapePath,
    object_color as editor_object_color, shape_kind_label, structural_child,
    structural_child_count,
};
use lpa_studio_core::{
    NodeId, ProjectController, ProjectEditorOp, UiAction, UiPatchCell, UiPatchInstance,
    UiPatchSurface, UiPatchSurfaceFixture, UiPatchSurfaceModule, UiPatchSurfaceOutput,
    UiPatchTarget,
};
use lpc_mapping::{Map2dDoc, Map2dShape};

/// The editor-canvas object palette (OBJECT_COLORS), indexed per fixture:
/// the one colour language — objects wear it everywhere, ports never do.
const OBJECT_COLORS: [&str; 6] = [
    "#5aa9e6", "#3fd68e", "#e4c065", "#c792ea", "#f0913b", "#64d8cb",
];

fn object_color(index: usize) -> &'static str {
    OBJECT_COLORS[index % OBJECT_COLORS.len()]
}

/// Dispatch a selection change to the core — the same op the patch
/// surface and (later) the arrange canvas dispatch: one selection.
fn select(on_action: &EventHandler<UiAction>, target: Option<UiPatchTarget>) {
    on_action.call(UiAction::from_op(
        lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
        ProjectEditorOp::PatchSelect { target },
    ));
}

fn is_selected(selection: &Option<UiPatchTarget>, target: &UiPatchTarget) -> bool {
    selection.as_ref() == Some(target)
}

/// Fetch-on-open: resolve unfetched map2d/patch bodies exactly the way
/// the interim page does (a no-op once cached). The panel being OPEN is
/// the demand signal — the lazy-loading principle, not an eager sweep.
fn prefetch_bodies(on_action: &EventHandler<UiAction>, surface: &UiPatchSurface) {
    for fixture in &surface.fixtures {
        if !fixture.mapping_loaded
            && let Some(artifact) = fixture.mapping_artifact.clone()
        {
            on_action.call(UiAction::from_op(
                ProjectController::NODE_ID,
                lpa_studio_core::AssetContentFetchOp { artifact },
            ));
        }
        if !fixture.patch_loaded
            && let Some(artifact) = fixture.patch_artifact.clone()
        {
            on_action.call(UiAction::from_op(
                ProjectController::NODE_ID,
                lpa_studio_core::AssetContentFetchOp { artifact },
            ));
        }
    }
}

/// The cells covering one instance's slice of the fixture's channel
/// space — the instance's wire windows, chip material.
fn instance_cells<'a>(
    fixture: &'a UiPatchSurfaceFixture,
    instance: &UiPatchInstance,
) -> Vec<&'a UiPatchCell> {
    fixture
        .patch
        .cells
        .iter()
        .filter(|cell| {
            cell.source_start < instance.start + instance.lamps
                && cell.source_start + cell.lamps > instance.start
        })
        .collect()
}

/// One channel chip's text: port-named wire window (`IO18:1-30`),
/// 1-based like the spike; the output label backstops an unnamed pin,
/// and a cell with neither still shows its honest wire range.
fn chip_text(cell: &UiPatchCell) -> String {
    let port = if !cell.port_label.is_empty() {
        cell.port_label.as_str()
    } else {
        cell.output_label.as_str()
    };
    let range = format!("{}-{}", cell.wire_start + 1, cell.wire_start + cell.lamps);
    if port.is_empty() {
        range
    } else {
        format!("{port}:{range}")
    }
}

const CHIP: &str = "tw:whitespace-nowrap tw:rounded tw:border tw:border-border-strong tw:bg-card-muted tw:px-1 tw:font-mono tw:text-[9.5px] tw:text-subtle-foreground";
const ROW_IDLE: &str = "tw:flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-md tw:border tw:border-transparent tw:px-1.5 tw:py-1 tw:hover:bg-background-wash";
const ROW_SELECTED: &str = "tw:flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-md tw:border tw:border-selection-border tw:bg-selection-bg tw:px-1.5 tw:py-1";

/// Indent per tree level as an inline style — arbitrary depth (nested
/// modules, nested repeats) must never outrun a generated tailwind class.
fn indent_style(level: usize) -> String {
    format!("margin-left: {}px;", level * 14)
}

/// The modules worth a row: a module renders when its subtree (itself or
/// any deeper module before the walk pops back) encloses one of the
/// panel's items — a module row is context for something, never an empty
/// header.
fn module_encloses_items(
    modules: &[UiPatchSurfaceModule],
    index: usize,
    used: &std::collections::BTreeSet<NodeId>,
) -> bool {
    let base = modules[index].depth;
    used.contains(&modules[index].node)
        || modules[index + 1..]
            .iter()
            .take_while(|module| module.depth > base)
            .any(|module| used.contains(&module.node))
}

/// One module context row: the tree level above fixtures/outputs (G1),
/// selecting like every other surface row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModuleRow(
    module: UiPatchSurfaceModule,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let target = UiPatchTarget::Module { node: module.node };
    let row_class = if is_selected(&selection, &target) {
        ROW_SELECTED
    } else {
        ROW_IDLE
    };
    rsx! {
        div {
            class: "{row_class}",
            style: indent_style(module.depth),
            onclick: {
                let on_action = on_action;
                let target = target.clone();
                move |_| select(&on_action, Some(target.clone()))
            },
            span { class: "tw:flex-none tw:text-[10px] tw:text-dim-foreground", "▤" }
            span { class: "tw:truncate tw:font-medium tw:text-foreground", "{module.label}" }
            span { class: "tw:ml-auto tw:flex-none tw:font-mono tw:text-[9.5px] tw:text-dim-foreground",
                "module"
            }
        }
    }
}

/// The Fixtures panel: fixture → instance rows with mapped-state dots
/// and text channel chips. Selection routes through the shared patch
/// selection; the Outputs panel and the interim page light up in step.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FixturesPanel(
    surface: Option<UiPatchSurface>,
    selection: Option<UiPatchTarget>,
    /// The dive, when one is live: `(focused node, shared session)` — the
    /// focused fixture's row grows its OBJECT tree (the editor's old
    /// wiring-order rail, unified here; one tree, one selection).
    #[props(default)]
    dive: Option<(NodeId, Signal<MapEditorSession>)>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let Some(surface) = surface else {
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "No fixtures on a wire yet — bind an output to a control bus and this panel fills in."
            }
        };
    };
    prefetch_bodies(&on_action, &surface);
    let fixtures = surface.fixtures.clone();
    let lamps: u32 = fixtures.iter().map(|fixture| fixture.patch.lamps).sum();
    let instances: usize = fixtures.iter().map(|f| f.instances.len()).sum();
    // Fixture colour rides SURFACE order, so grouping under modules never
    // reshuffles the palette the canvas already wears.
    let indexed = |fixture: &UiPatchSurfaceFixture| {
        fixtures
            .iter()
            .position(|other| other.node == fixture.node)
            .unwrap_or_default()
    };
    let orphans: Vec<(usize, UiPatchSurfaceFixture)> = fixtures
        .iter()
        .filter(|fixture| fixture.module.is_none())
        .map(|fixture| (indexed(fixture), fixture.clone()))
        .collect();
    let used: std::collections::BTreeSet<NodeId> = fixtures
        .iter()
        .filter_map(|fixture| fixture.module)
        .collect();
    let groups: Vec<(UiPatchSurfaceModule, Vec<(usize, UiPatchSurfaceFixture)>)> = surface
        .modules
        .iter()
        .enumerate()
        .filter(|(index, _)| module_encloses_items(&surface.modules, *index, &used))
        .map(|(_, module)| {
            let members = fixtures
                .iter()
                .filter(|fixture| fixture.module == Some(module.node))
                .map(|fixture| (indexed(fixture), fixture.clone()))
                .collect();
            (module.clone(), members)
        })
        .collect();
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-0.5 tw:text-sm",
            p { class: "tw:m-0 tw:px-1.5 tw:pb-1.5 tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                "{fixtures.len()} fixtures · {lamps} lamps · {instances} instances"
            }
            for (index, fixture) in orphans {
                FixtureRows {
                    key: "{fixture.node.0}",
                    dive_session: dive
                        .filter(|(node, _)| *node == fixture.node)
                        .map(|(_, session)| session),
                    fixture,
                    color: object_color(index),
                    indent: 0,
                    selection: selection.clone(),
                    on_action,
                }
            }
            for (module, members) in groups {
                ModuleRow {
                    key: "m-{module.node.0}",
                    module: module.clone(),
                    selection: selection.clone(),
                    on_action,
                }
                for (index, fixture) in members {
                    FixtureRows {
                        key: "{fixture.node.0}",
                        dive_session: dive
                            .filter(|(node, _)| *node == fixture.node)
                            .map(|(_, session)| session),
                        fixture,
                        color: object_color(index),
                        indent: module.depth + 1,
                        selection: selection.clone(),
                        on_action,
                    }
                }
            }
        }
    }
}

/// One row of the dived fixture's shape tree, flattened for rendering:
/// path (the selection unit), descent depth, text, and whether it is the
/// exact selected node.
struct DiveRow {
    path: ShapePath,
    depth: usize,
    label: String,
    trail: String,
    selected: bool,
    object_index: usize,
}

/// The one-word text a tree row shows for a shape — the kind, with a
/// repeat carrying its instance count (`repeat ×5`).
fn shape_row_text(shape: &Map2dShape) -> String {
    match shape {
        Map2dShape::Repeat(repeat) => format!("repeat ×{}", repeat.count),
        other => shape_kind_label(other).to_string(),
    }
}

/// Flatten the dived document's shape tree in wiring order: each object's
/// root row, then its structural descendants (a repeat's inner item, at
/// any nesting) — the full depth the G1 feedback asked for. Selection is
/// the EXACT path, so a group and its inner item highlight distinctly.
fn dive_rows(doc: &Map2dDoc, selection: &MapSelection) -> Vec<DiveRow> {
    fn descend(
        rows: &mut Vec<DiveRow>,
        doc: &Map2dDoc,
        path: &ShapePath,
        depth: usize,
        object_index: usize,
        selection: &MapSelection,
    ) {
        let Some(shape) = path.resolve(doc) else {
            return;
        };
        for step in 0..structural_child_count(shape) {
            let child_path = path.child(step);
            let Some(child) = structural_child(shape, step) else {
                continue;
            };
            rows.push(DiveRow {
                path: child_path.clone(),
                depth,
                label: shape_row_text(child),
                trail: String::new(),
                selected: selection.contains(&child_path),
                object_index,
            });
            descend(rows, doc, &child_path, depth + 1, object_index, selection);
        }
    }
    let mut rows = Vec::new();
    for (object_index, object) in doc.objects.iter().enumerate() {
        let path = ShapePath::root(object_index);
        rows.push(DiveRow {
            path: path.clone(),
            depth: 0,
            label: object.name.clone(),
            trail: shape_row_text(&object.shape),
            selected: selection.contains(&path),
            object_index,
        });
        descend(&mut rows, doc, &path, 1, object_index, selection);
    }
    rows
}

/// One fixture's row plus its instance (or range-grain) children.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureRows(
    fixture: UiPatchSurfaceFixture,
    color: &'static str,
    /// Tree level under the module rows (0 = no enclosing module).
    indent: usize,
    selection: Option<UiPatchTarget>,
    /// Present while this fixture is the dive: its object tree renders
    /// from (and selects through) the shared session.
    #[props(default)]
    dive_session: Option<Signal<MapEditorSession>>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let fixture_target = UiPatchTarget::Fixture { node: fixture.node };
    let row_class = if is_selected(&selection, &fixture_target) {
        ROW_SELECTED
    } else {
        ROW_IDLE
    };
    let range_grain = fixture.instances.is_empty();
    let kind = if range_grain { "range" } else { "2d map" };
    rsx! {
        div {
            class: "{row_class}",
            style: indent_style(indent),
            onclick: {
                let on_action = on_action;
                let target = fixture_target.clone();
                move |_| select(&on_action, Some(target.clone()))
            },
            span {
                class: "tw:h-2 tw:w-2 tw:flex-none tw:rounded-[3px]",
                style: "background: {color};",
            }
            span { class: "tw:truncate tw:font-medium tw:text-foreground", "{fixture.label}" }
            span { class: "tw:ml-auto tw:flex-none tw:font-mono tw:text-[9.5px] tw:text-dim-foreground",
                "{fixture.patch.lamps} · {kind}"
            }
        }
        if let Some(mut session) = dive_session {
            // The DIVE's object tree (one tree, one selection): the edited
            // document's FULL shape tree in wiring order — objects, and
            // inside each the structural descent (a repeat's inner item at
            // any nesting) — selecting exact ShapePaths through the
            // session the canvas and Props pane share.
            {
                let session_read = session.read();
                let rows = dive_rows(session_read.doc(), &session_read.selection);
                drop(session_read);
                rsx! {
                    for row in rows {
                        div {
                            key: "obj-{row.object_index}-{row.path.descent:?}",
                            class: if row.selected { "{ROW_SELECTED} tw:py-0.5" } else { "{ROW_IDLE} tw:py-0.5" },
                            style: indent_style(indent + 1 + row.depth),
                            onclick: {
                                let path = row.path.clone();
                                move |_| {
                                    session.write().selection.select_only_path(path.clone());
                                }
                            },
                            // Root objects wear the filled object-colour
                            // square; descendants an outlined one — same
                            // colour language, group vs member.
                            if row.path.is_root() {
                                span {
                                    class: "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-[2px]",
                                    style: "background: {editor_object_color(row.object_index)};",
                                }
                            } else {
                                span {
                                    class: "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-[2px] tw:border",
                                    style: "border-color: {editor_object_color(row.object_index)};",
                                }
                            }
                            span { class: "tw:truncate tw:text-[12px] tw:text-muted-foreground", "{row.label}" }
                            span { class: "tw:ml-auto tw:flex-none tw:font-mono tw:text-[9.5px] tw:text-dim-foreground",
                                "{row.trail}"
                            }
                        }
                    }
                }
            }
        } else if range_grain {
            // The peach case: no instance grain — one honest row for the
            // fixture's whole channel range, chips from its cells.
            div {
                class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1 tw:px-1.5 tw:py-0.5",
                style: indent_style(indent + 1),
                span { class: "tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground",
                    "0..{fixture.patch.lamps}"
                }
                for cell in fixture.patch.cells.clone() {
                    span { class: "{CHIP} tw:text-status-attention-foreground", "{chip_text(&cell)}" }
                }
            }
        } else {
            for instance in fixture.instances.clone() {
                InstanceRow {
                    key: "{fixture.node.0}-{instance.path}",
                    cells: instance_cells(&fixture, &instance)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>(),
                    node: fixture.node,
                    instance,
                    indent: indent + 1,
                    selection: selection.clone(),
                    on_action,
                }
            }
        }
    }
}

/// One instance row: mapped dot · label · channel chips.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn InstanceRow(
    node: lpa_studio_core::NodeId,
    instance: UiPatchInstance,
    cells: Vec<UiPatchCell>,
    /// Tree level under the fixture row.
    indent: usize,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let target = UiPatchTarget::Instance {
        node,
        path: instance.path.clone(),
    };
    let row_class = if is_selected(&selection, &target) {
        ROW_SELECTED
    } else {
        ROW_IDLE
    };
    // The DTO's derived fact (P2), not a chip-list inference — the two
    // agree today, but `placed` is the one the core owns.
    let mapped = instance.placed;
    let dot = if mapped {
        "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-good-foreground"
    } else {
        "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:border tw:border-dim-foreground"
    };
    let title = format!(
        "{} · lamps {}–{} · stride {}",
        instance.path,
        instance.start,
        instance.start + instance.lamps.saturating_sub(1),
        instance.stride,
    );
    rsx! {
        div {
            class: "{row_class} tw:py-0.5",
            style: indent_style(indent),
            title: "{title}",
            onclick: {
                let on_action = on_action;
                let target = target.clone();
                move |_| select(&on_action, Some(target.clone()))
            },
            span { class: "{dot}" }
            span { class: "tw:truncate tw:text-[12px] tw:text-muted-foreground", "{instance.label}" }
            span { class: "tw:ml-auto tw:flex tw:flex-none tw:gap-0.5",
                for cell in cells {
                    span { class: "{CHIP}",
                        "{chip_text(&cell)}"
                        if cell.reversed {
                            span { class: "tw:text-status-warning-foreground", " ‹rev" }
                        }
                    }
                }
            }
        }
    }
}

/// The Outputs panel: box → port → wire-window cells at dock density.
/// Every box carries its OWN open state, all open by default (R4-3), so
/// two boxes can be read side by side; the chevron header toggles just
/// its own. Rows and cells select through the shared selection.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OutputsPanel(
    surface: Option<UiPatchSurface>,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // The CLOSED boxes, by output node id — closed is the exception, so
    // an output the panel has never seen reads as open by default and no
    // seeding pass is needed when the surface grows.
    let mut collapsed = use_signal(std::collections::BTreeSet::<lpa_studio_core::NodeId>::new);
    let Some(surface) = surface else {
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "Nothing to patch yet — no output has published a wire."
            }
        };
    };
    prefetch_bodies(&on_action, &surface);
    // Same module grouping as the Fixtures panel: the tree above the
    // outputs is context worth a slim row, never an empty header.
    let outputs = surface.outputs.clone();
    let orphans: Vec<UiPatchSurfaceOutput> = outputs
        .iter()
        .filter(|output| output.module.is_none())
        .cloned()
        .collect();
    let used: std::collections::BTreeSet<NodeId> =
        outputs.iter().filter_map(|output| output.module).collect();
    let groups: Vec<(UiPatchSurfaceModule, Vec<UiPatchSurfaceOutput>)> = surface
        .modules
        .iter()
        .enumerate()
        .filter(|(index, _)| module_encloses_items(&surface.modules, *index, &used))
        .map(|(_, module)| {
            let members = outputs
                .iter()
                .filter(|output| output.module == Some(module.node))
                .cloned()
                .collect();
            (module.clone(), members)
        })
        .collect();
    let on_toggle = move |node| {
        collapsed.with_mut(|closed| {
            if !closed.remove(&node) {
                closed.insert(node);
            }
        });
    };
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-0.5",
            for output in orphans {
                OutputBox {
                    key: "{output.node.0}",
                    expanded: !collapsed.read().contains(&output.node),
                    output,
                    indent: 0,
                    selection: selection.clone(),
                    on_toggle,
                    on_action,
                }
            }
            for (module, members) in groups {
                ModuleRow {
                    key: "m-{module.node.0}",
                    module: module.clone(),
                    selection: selection.clone(),
                    on_action,
                }
                for output in members {
                    OutputBox {
                        key: "{output.node.0}",
                        expanded: !collapsed.read().contains(&output.node),
                        output,
                        indent: module.depth + 1,
                        selection: selection.clone(),
                        on_toggle,
                        on_action,
                    }
                }
            }
        }
    }
}

/// One output: a slim section-header ROW (chevron · name · occupancy) with
/// its port rows indented beneath — no card, no nested border, so the panel
/// reads as flat as the Fixtures panel beside it (R4-3).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OutputBox(
    output: UiPatchSurfaceOutput,
    expanded: bool,
    /// Tree level under the module rows (0 = no enclosing module).
    indent: usize,
    selection: Option<UiPatchTarget>,
    on_toggle: EventHandler<lpa_studio_core::NodeId>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let name = output.display_name().to_string();
    let used: u32 = output
        .bay
        .ports
        .iter()
        .flat_map(|port| port.cells.iter())
        .map(|cell| cell.lamps)
        .sum();
    let total: u32 = output.bay.ports.iter().map(|port| port.lamps).sum();
    let occupancy = if total > 0 {
        (f64::from(used) / f64::from(total) * 100.0).round()
    } else {
        0.0
    };
    let contested = output.bay.contested_lamps;
    let chevron = if expanded { "▾" } else { "▸" };
    let output_target = UiPatchTarget::Output { node: output.node };
    let header_class = if is_selected(&selection, &output_target) {
        "tw:flex tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-selection-border tw:bg-selection-bg tw:px-1.5 tw:py-1"
    } else {
        "tw:flex tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-transparent tw:px-1.5 tw:py-1 tw:hover:bg-background-wash"
    };
    rsx! {
        section { class: "tw:grid tw:content-start", style: indent_style(indent),
            header {
                class: "{header_class}",
                onclick: {
                    let on_action = on_action;
                    let target = output_target.clone();
                    let node = output.node;
                    move |_| {
                        // Toggling is also selecting: one gesture, both
                        // meanings — the row is the output.
                        on_toggle.call(node);
                        select(&on_action, Some(target.clone()));
                    }
                },
                span { class: "tw:text-[10px] tw:text-dim-foreground", "{chevron}" }
                span { class: "tw:truncate tw:text-[12.5px] tw:font-medium tw:text-foreground", "{name}" }
                if contested > 0 {
                    span { class: "tw:flex-none tw:font-mono tw:text-[9.5px] tw:text-status-error-foreground",
                        "{contested} contested"
                    }
                }
                span { class: "tw:ml-auto tw:flex tw:flex-none tw:items-center tw:gap-1.5" ,
                    span { class: "tw:h-[5px] tw:w-11 tw:overflow-hidden tw:rounded-full tw:bg-border-muted",
                        span {
                            class: "tw:block tw:h-full tw:bg-heading/60",
                            style: "width: {occupancy}%;",
                        }
                    }
                    span { class: "tw:font-mono tw:text-[9.5px] tw:text-dim-foreground", "{used}/{total}" }
                }
            }
            if expanded {
                // Indented under the header rather than boxed inside it —
                // the port rows belong to the row above, and one hairline
                // rail is enough to say so.
                div { class: "tw:ml-2.5 tw:grid tw:gap-1 tw:border-l tw:border-border-subtle tw:py-1 tw:pl-2",
                    for port in output.bay.ports.clone() {
                        PortRow {
                            key: "{output.node.0}-{port.key}",
                            node: output.node,
                            port,
                            selection: selection.clone(),
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// One port: the one-line `pin · used/total` row plus its cell bar.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PortRow(
    node: lpa_studio_core::NodeId,
    port: lpa_studio_core::UiPatchPort,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let target = UiPatchTarget::Port {
        node,
        port: port.key,
    };
    let used: u32 = port.cells.iter().map(|cell| cell.lamps).sum();
    let line_class = if is_selected(&selection, &target) {
        "tw:flex tw:cursor-pointer tw:items-baseline tw:gap-1.5 tw:rounded tw:border tw:border-selection-border tw:bg-selection-bg tw:px-1"
    } else {
        "tw:flex tw:cursor-pointer tw:items-baseline tw:gap-1.5 tw:rounded tw:border tw:border-transparent tw:px-1 tw:hover:bg-background-wash"
    };
    rsx! {
        div {
            div {
                class: "{line_class}",
                onclick: {
                    let on_action = on_action;
                    let target = target.clone();
                    move |_| select(&on_action, Some(target.clone()))
                },
                span { class: "tw:font-mono tw:text-[10.5px] tw:font-semibold tw:text-strong-foreground",
                    "{port.pin_label}"
                }
                span { class: "tw:font-mono tw:text-[9.5px] tw:text-dim-foreground", "port {port.key}" }
                span { class: "tw:ml-auto tw:font-mono tw:text-[9.5px] tw:text-dim-foreground",
                    "{used}/{port.lamps}"
                }
            }
            div { class: "tw:relative tw:mt-0.5 tw:h-4 tw:overflow-hidden tw:rounded tw:bg-track",
                for cell in port.cells.clone() {
                    PortCell {
                        key: "{cell.id}",
                        cell,
                        port_start: port.start,
                        port_lamps: port.lamps,
                        selection: selection.clone(),
                        on_action,
                    }
                }
            }
        }
    }
}

/// One wire-window cell in a port bar: neutral, path-labelled, absolute-
/// positioned by its wire window (the interim page's geometry, dock-fit).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PortCell(
    cell: UiPatchCell,
    port_start: u32,
    port_lamps: u32,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let lamps = port_lamps.max(1);
    let width = (cell.lamps as f32 / lamps as f32 * 100.0).max(1.0);
    let left = (cell.wire_start.saturating_sub(port_start)) as f32 / lamps as f32 * 100.0;
    let target = UiPatchTarget::Cell {
        id: cell.id.clone(),
    };
    let class = if cell.contested {
        "tw:absolute tw:top-0 tw:flex tw:h-full tw:cursor-pointer tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-[3px] tw:border tw:border-status-error-border tw:bg-status-error-bg"
    } else if is_selected(&selection, &target) {
        "tw:absolute tw:top-0 tw:flex tw:h-full tw:cursor-pointer tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-[3px] tw:border tw:border-selection-border tw:bg-selection-bg"
    } else {
        "tw:absolute tw:top-0 tw:flex tw:h-full tw:cursor-pointer tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-[3px] tw:border tw:border-border-strong tw:bg-card-raised tw:hover:border-dim-foreground"
    };
    let label = format!("{}{}", cell.producer, if cell.reversed { " ‹" } else { "" });
    let title = format!(
        "{} {}–{}{} → wire {}",
        cell.producer,
        cell.source_start,
        cell.source_end().unwrap_or(cell.source_start),
        if cell.reversed { " ◀" } else { "" },
        cell.wire_start,
    );
    rsx! {
        div {
            class: "{class}",
            style: "left: {left}%; width: {width}%;",
            title: "{title}",
            onclick: {
                let on_action = on_action;
                move |_| select(&on_action, Some(target.clone()))
            },
            span { class: "tw:truncate tw:font-mono tw:text-[8.5px] tw:text-subtle-foreground", "{label}" }
        }
    }
}

/// The Props panel (R4, Figma prior art): while a dive is live, the
/// selected OBJECT's fields — the floating popover re-housed, editing the
/// shared session and committing through the session host's pipeline.
/// At the arranged level it reads out the selected fixture's placement.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PropsPanel(
    surface: Option<UiPatchSurface>,
    selection: Option<UiPatchTarget>,
    dive_focused: Signal<Option<NodeId>>,
    dive_session: Signal<MapEditorSession>,
    mut dive_commits: Signal<u64>,
) -> Element {
    if dive_focused.read().is_some() {
        return rsx! {
            ObjectPropertiesPane {
                session: dive_session,
                on_committed: move |()| {
                    let next = *dive_commits.peek() + 1;
                    dive_commits.set(next);
                },
            }
        };
    }
    // A selected MODULE row reads out its context facts — the level the
    // Props stack will grow from.
    if let (Some(surface), Some(UiPatchTarget::Module { node })) = (&surface, &selection)
        && let Some(module) = surface.modules.iter().find(|module| module.node == *node)
    {
        let fixtures = surface
            .fixtures
            .iter()
            .filter(|fixture| fixture.module == Some(module.node))
            .count();
        let outputs = surface
            .outputs
            .iter()
            .filter(|output| output.module == Some(module.node))
            .count();
        return rsx! {
            div { class: "tw:grid tw:content-start tw:gap-1 tw:px-1.5 tw:text-sm",
                p { class: "tw:m-0 tw:font-medium tw:text-foreground", "{module.label}" }
                p { class: "tw:m-0 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground",
                    "module · {fixtures} fixtures · {outputs} outputs"
                }
                p { class: "tw:m-0 tw:mt-2 tw:text-[11px] tw:text-dim-foreground",
                    "Select a fixture beneath it for placement, or dive in for object properties."
                }
            }
        };
    }
    let fixture = match (&surface, &selection) {
        (
            Some(surface),
            Some(
                UiPatchTarget::Fixture { node }
                | UiPatchTarget::Instance { node, .. }
                | UiPatchTarget::Range { node, .. },
            ),
        ) => surface
            .fixtures
            .iter()
            .find(|fixture| fixture.node == *node),
        _ => None,
    };
    let Some(fixture) = fixture else {
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "Select a fixture on the canvas — its placement reads out here; dive in (double-click) for object properties."
            }
        };
    };
    let arrange = fixture.arrange.clone().unwrap_or_default();
    let transform = arrange.transform;
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-1 tw:px-1.5 tw:text-sm",
            p { class: "tw:m-0 tw:font-medium tw:text-foreground", "{fixture.label}" }
            p { class: "tw:m-0 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground",
                "{fixture.patch.lamps} lamps · {fixture.instances.len()} instances"
            }
            p { class: "tw:m-0 tw:mt-1 tw:font-mono tw:text-[10.5px] tw:text-dim-foreground",
                if arrange.arranged {
                    "placed at {transform.t[0]:.1}, {transform.t[1]:.1} · {transform.r:.0}° · ×{transform.s:.2}"
                } else {
                    "unarranged — drag it on the canvas to place it"
                }
            }
            p { class: "tw:m-0 tw:mt-2 tw:text-[11px] tw:text-dim-foreground",
                "Double-click the fixture to dive into its mapping."
            }
        }
    }
}
