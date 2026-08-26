//! The Tree (fixtures) and Outputs panels — derived slices over the
//! patch-surface DTOs, sharing the surface's ONE core-owned selection
//! (`ProjectEditorOp::PatchSelect`). The Tree's grain follows the VIEW
//! ([`TreeGrain`]): authored in Mapping, resolved in Patching.
//!
//! Rows, cells and free port space SELECT, everywhere and always — plain
//! clicks never write (D7). In the Patching view both panels are also
//! where an ARMED verb completes: a port click finishes a swap or lands
//! the armed assign, an object row finishes an assign the wire side
//! started (see `patch_verbs` on each panel, and
//! [`crate::app::editor_shell::patching::ArmedVerb`]).
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
    NodeId, PatchVerbKind, ProjectController, ProjectEditorOp, UiAction, UiArrangeTransform,
    UiPatchCell, UiPatchInstance, UiPatchSurface, UiPatchSurfaceFixture, UiPatchSurfaceModule,
    UiPatchSurfaceOutput, UiPatchTarget, UiSelection,
};
use lpc_mapping::{Map2dDoc, Map2dShape};

use crate::app::editor_shell::patching::{
    ArmedVerb, PatchingUi, complete_assign_on_object, is_armable,
};
use crate::app::patch::verb_ui::{
    dispatch_assign, dispatch_verb, free_runs, instance_target, port_next_free, port_window,
    segment_at_free_run,
};

/// The editor-canvas object palette (OBJECT_COLORS), indexed per fixture:
/// the one colour language — objects wear it everywhere, ports never do.
const OBJECT_COLORS: [&str; 6] = [
    "#5aa9e6", "#3fd68e", "#e4c065", "#c792ea", "#f0913b", "#64d8cb",
];

fn object_color(index: usize) -> &'static str {
    OBJECT_COLORS[index % OBJECT_COLORS.len()]
}

/// Which tree the Fixtures panel shows — **grain follows activity,
/// never dive state** (the `_features/patching.md` ruling, R5): the
/// Mapping view reads the AUTHORED tree (objects, repeat interiors —
/// what mapping edits), the Patching view the RESOLVED one (instances /
/// ranges with their wire chips — what patching assigns). The naming
/// pair matches `lpc_mapping::resolve`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeGrain {
    Authored,
    Resolved,
}

/// Dispatch a selection change to the core — the same op the patch
/// surface and (later) the arrange canvas dispatch: one selection.
fn select(on_action: &EventHandler<UiAction>, target: Option<UiPatchTarget>) {
    on_action.call(UiAction::from_op(
        lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
        ProjectEditorOp::PatchSelect {
            selection: UiSelection::from_option(target),
        },
    ));
}

fn is_selected(selection: &UiSelection, target: &UiPatchTarget) -> bool {
    selection.contains(target)
}

/// Fetch-on-demand: resolve unfetched map2d/patch bodies (a no-op once
/// cached). The demand signal is a panel being OPEN — or the Patching
/// center being mounted, whose verbs transform the patch bodies — the
/// lazy-loading principle, not an eager sweep.
pub(crate) fn prefetch_bodies(on_action: &EventHandler<UiAction>, surface: &UiPatchSurface) {
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
    selection: UiSelection,
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
    selection: UiSelection,
    /// Which tree this view reads (grain follows activity): the Mapping
    /// view passes `Authored`, the Patching view `Resolved`.
    grain: TreeGrain,
    /// Fixture map2d bodies by artifact — the authored grain's source for
    /// UNDIVED fixtures (a fixture whose body hasn't loaded shows its
    /// flat row until the panel's fetch-on-open lands it).
    #[props(default)]
    bodies: std::rc::Rc<std::collections::BTreeMap<lpa_studio_core::ArtifactLocation, String>>,
    /// The dive, when one is live: `(focused node, shared session)` — the
    /// focused fixture's row grows its OBJECT tree (the editor's old
    /// wiring-order rail, unified here; one tree, one selection).
    #[props(default)]
    dive: Option<(NodeId, Signal<MapEditorSession>)>,
    /// The Patching view passes true: an object row is a COUNTERPART an
    /// armed assign can complete against (§3's fixture-side completion).
    /// Unarmed — and everywhere else — a row click only selects.
    #[props(default)]
    patch_verbs: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    // The frame-scoped arm (armed in the Patching center, completed here or
    // in the Outputs dock); absent outside the workbench frame — panel
    // stories then simply have no arm to complete.
    let patching_ui = use_hook(try_consume_context::<PatchingUi>);
    let Some(surface) = surface else {
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "No fixtures on a wire yet — bind an output to a control bus and this panel fills in."
            }
        };
    };
    prefetch_bodies(&on_action, &surface);
    let fixtures = surface.fixtures.clone();
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
    // The object-click grammar for every row below: an armed assign
    // completes against an UNMAPPED object (the segment-first flow's second
    // click), and the row selects either way — mapped things always
    // plain-reselect (the ruling).
    let on_object = {
        let surface = surface.clone();
        let selection = selection.clone();
        let on_action = on_action;
        EventHandler::new(move |target: UiPatchTarget| {
            if patch_verbs {
                // The arm completes against a SINGLE selected end — a
                // multi-selection is not armable, and `single()` is how
                // that rule reads here.
                let single = selection.single().cloned();
                complete_assign_on_object(&on_action, &surface, &single, patching_ui, &target);
            }
            select(&on_action, Some(target));
        })
    };
    rsx! {
        // The old top summary line is now the dock's Finder-style footer
        // (D12) — see `panel_footer` in the workbench module.
        div { class: "tw:grid tw:content-start tw:gap-0.5 tw:text-sm",
            for (index, fixture) in orphans {
                FixtureRows {
                    key: "{fixture.node.0}",
                    dive_session: dive
                        .filter(|(node, _)| *node == fixture.node)
                        .map(|(_, session)| session),
                    grain,
                    body: fixture
                        .mapping_artifact
                        .as_ref()
                        .and_then(|artifact| bodies.get(artifact).cloned()),
                    fixture,
                    color: object_color(index),
                    indent: 0,
                    selection: selection.clone(),
                    on_object,
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
                        grain,
                        body: fixture
                            .mapping_artifact
                            .as_ref()
                            .and_then(|artifact| bodies.get(artifact).cloned()),
                        fixture,
                        color: object_color(index),
                        indent: module.depth + 1,
                        selection: selection.clone(),
                        on_object,
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

/// Flatten a document's shape tree in wiring order: each object's root
/// row, then its structural descendants (a repeat's inner item, at any
/// nesting) — the full depth the G1 feedback asked for. `selected`
/// judges the EXACT path, so a group and its inner item highlight
/// distinctly. Serves both authored renders: the dive (live session
/// selection) and the undived Mapping tree (derived patch-selection
/// highlight).
fn authored_rows(doc: &Map2dDoc, selected: &dyn Fn(&ShapePath) -> bool) -> Vec<DiveRow> {
    fn descend(
        rows: &mut Vec<DiveRow>,
        doc: &Map2dDoc,
        path: &ShapePath,
        depth: usize,
        object_index: usize,
        selected: &dyn Fn(&ShapePath) -> bool,
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
                selected: selected(&child_path),
                object_index,
            });
            descend(rows, doc, &child_path, depth + 1, object_index, selected);
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
            selected: selected(&path),
            object_index,
        });
        descend(&mut rows, doc, &path, 1, object_index, selected);
    }
    rows
}

fn dive_rows(doc: &Map2dDoc, selection: &MapSelection) -> Vec<DiveRow> {
    authored_rows(doc, &|path| selection.contains(path))
}

/// The authored↔resolved selection bridge (`/sector/2`, D46): a resolved
/// instance path is an object-id segment plus integer instance steps, so
/// the authored row it names is DERIVED — the object whose sticky id
/// matches the first segment — never a second selection store. The
/// instance number stays implied (the authored tree has one row per
/// repeat interior, not per instance).
pub(crate) fn authored_object_for_instance_path(
    doc: &Map2dDoc,
    instance_path: &str,
) -> Option<usize> {
    let first = instance_path
        .split('/')
        .find(|segment| !segment.is_empty())?;
    doc.objects
        .iter()
        .position(|object| object.id.as_ref().is_some_and(|id| id.as_str() == first))
}

/// One fixture's row plus its instance (or range-grain) children.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureRows(
    fixture: UiPatchSurfaceFixture,
    color: &'static str,
    /// Tree level under the module rows (0 = no enclosing module).
    indent: usize,
    selection: UiSelection,
    /// Which tree this fixture's children render (grain follows
    /// activity — see [`TreeGrain`]).
    grain: TreeGrain,
    /// The fixture's map2d body, when loaded — the authored grain's
    /// undived source.
    #[props(default)]
    body: Option<String>,
    /// Present while this fixture is the dive: its object tree renders
    /// from (and selects through) the shared session.
    #[props(default)]
    dive_session: Option<Signal<MapEditorSession>>,
    /// An object row's click — the panel's grammar (select, and in the
    /// Patching view an armed assign's fixture-side completion).
    on_object: EventHandler<UiPatchTarget>,
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
    // The undived authored tree (Mapping grain): the loaded body, parsed;
    // a fixture with no loaded body keeps its flat row until the panel's
    // fetch-on-open lands it. Component memoization (props PartialEq)
    // bounds the re-parse to actual body changes.
    let authored_doc = (grain == TreeGrain::Authored && dive_session.is_none())
        .then(|| {
            body.as_deref()
                .and_then(|text| Map2dDoc::from_json(text).ok())
        })
        .flatten();
    rsx! {
        div {
            class: "{row_class}",
            style: indent_style(indent),
            onclick: {
                let target = fixture_target.clone();
                move |_| on_object.call(target.clone())
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
        if grain == TreeGrain::Authored && let Some(mut session) = dive_session {
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
        } else if grain == TreeGrain::Authored {
            // The UNDIVED authored tree (Mapping grain), CLICKABLE
            // (unified-selection P5 — tree parity): a row click selects
            // the object's instance targets through the ONE selection,
            // which ENTERS the fixture (scope derives) — the tree is an
            // alternate way in, no double-click needed. An id-less object
            // is not addressable (A1) and selects its fixture instead.
            // The highlight is the same D46 bridge, run backward.
            if let Some(doc) = authored_doc {
                {
                    let highlighted: std::collections::BTreeSet<usize> = selection
                        .targets()
                        .iter()
                        .filter_map(|target| match target {
                            UiPatchTarget::Instance { node, path } if *node == fixture.node => {
                                authored_object_for_instance_path(&doc, path)
                            }
                            _ => None,
                        })
                        .collect();
                    let mut rows = authored_rows(&doc, &|_| false);
                    for row in &mut rows {
                        if highlighted.contains(&row.object_index) && row.path.is_root() {
                            row.selected = true;
                        }
                    }
                    let doc = std::rc::Rc::new(doc);
                    rsx! {
                        for row in rows {
                            div {
                                key: "auth-{row.object_index}-{row.path.descent:?}",
                                class: if row.selected { "{ROW_SELECTED} tw:py-0.5" } else { "{ROW_IDLE} tw:py-0.5" },
                                style: indent_style(indent + 1 + row.depth),
                                onclick: {
                                    let doc = doc.clone();
                                    let instances = fixture.instances.clone();
                                    let node = fixture.node;
                                    let object_index = row.object_index;
                                    move |_| {
                                        use crate::app::editor_shell::selection as coord;
                                        let targets = coord::instance_targets_for_object(
                                            node,
                                            &doc,
                                            &instances,
                                            object_index,
                                        );
                                        let mut next = UiSelection::empty();
                                        if !targets.is_empty() {
                                            next.set_siblings(targets);
                                        } else if let Some(range) =
                                            coord::range_target_for_object(node, &doc, object_index)
                                        {
                                            // Id-less object (fyeah): the
                                            // Range grain addresses it.
                                            next.select_one(range);
                                        } else {
                                            next.select_one(UiPatchTarget::Fixture { node });
                                        }
                                        coord::dispatch_selection(&on_action, next);
                                    }
                                },
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
            }
        } else if range_grain {
            // The peach case (Resolved grain): no instance grain — one
            // honest row for the fixture's whole channel range, chips from
            // its cells.
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
                    // Keyed by START, not path: id-less display rows all
                    // carry the empty path, and start is unique per strand.
                    key: "{fixture.node.0}-{instance.start}",
                    cells: instance_cells(&fixture, &instance)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>(),
                    node: fixture.node,
                    instance,
                    indent: indent + 1,
                    selection: selection.clone(),
                    on_object,
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
    selection: UiSelection,
    /// An object row's click — see [`FixtureRows`].
    on_object: EventHandler<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // An id-less display row (empty path — the doc hasn't been through
    // ensure-ids) selects at RANGE grain: same lamps, same pulse, honest
    // about not being path-addressable (grain-robustness ruling, G1 R2).
    let target = instance_target(node, &instance);
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
                let target = target.clone();
                move |_| on_object.call(target.clone())
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

/// Where a click landed on a port: the port row proper, or one of the FREE
/// runs in its bar (`(start, lamps)` in wire numbering). The port bar's
/// gaps are real elements, so "which free run" needs no geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PortClick {
    pub node: NodeId,
    pub port: u32,
    pub free_run: Option<(u32, u32)>,
}

/// The Outputs panel: box → port → wire-window cells at dock density.
/// Every box carries its OWN open state, all open by default (R4-3), so
/// two boxes can be read side by side; the chevron header toggles just
/// its own. Rows and cells select through the shared selection.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OutputsPanel(
    surface: Option<UiPatchSurface>,
    selection: UiSelection,
    /// The Patching view passes true: a port click carries the walk-up
    /// grammar (an ARMED verb completes on it). Everywhere else — and
    /// unarmed here — a port click only ever selects: plain clicks never
    /// write (D7, the ruling).
    #[props(default)]
    patch_verbs: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    // The CLOSED boxes, by output node id — closed is the exception, so
    // an output the panel has never seen reads as open by default and no
    // seeding pass is needed when the surface grows.
    let mut collapsed = use_signal(std::collections::BTreeSet::<lpa_studio_core::NodeId>::new);
    // The frame-scoped arm (armed in the Patching center, completed here) —
    // absent when the panel renders outside the workbench frame (its own
    // stories), which simply means there is no arm to complete.
    let patching_ui = use_hook(try_consume_context::<PatchingUi>);
    let Some(surface) = surface else {
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "Nothing to patch yet — no output has published a wire."
            }
        };
    };
    prefetch_bodies(&on_action, &surface);
    // The single-target view of the one selection: the arm grammar and
    // the verb dispatches below are single-subject by ruling (a
    // multi-selection is not an armable end).
    let single = selection.single().cloned();
    // The counterpart glow (round 3, #5): while an assign is armed on a
    // fixture-side selection, the FREE RUNS are where the next click
    // belongs — they wear the attention ring. Animation only while frames
    // flow (story determinism, same gate as the panel).
    let free_attention = patch_verbs
        && patching_ui.is_some_and(|ui| {
            matches!(
                *ui.armed.read(),
                Some(crate::app::editor_shell::patching::ArmedVerb::Assign)
            )
        })
        && single
            .as_ref()
            .is_some_and(|target| is_armable(&surface, target))
        && !matches!(single, Some(UiPatchTarget::Segment { .. }))
        && crate::app::patch::patch_panel::surface_is_live(&surface);
    // The port-click grammar (P3's arm grammar): an ARMED verb completes
    // here — swap against the clicked port, assign the fixture-side
    // selection onto it — and everything else is selection. The #436
    // plain-click auto-assign is GONE (D7): looking is free in both
    // directions, and nothing writes without an arm.
    let on_port_click = {
        let surface = surface.clone();
        let selection = single.clone();
        let on_action = on_action;
        move |click: PortClick| {
            let PortClick {
                node,
                port: port_key,
                free_run,
            } = click;
            let Some(output) = surface.outputs.iter().find(|output| output.node == node) else {
                return;
            };
            if patch_verbs && let Some(ui) = patching_ui {
                let mut armed = ui.armed;
                let mut segment_size = ui.segment_size;
                let current = armed.peek().clone();
                match current {
                    // An armed swap completes on the next port click.
                    Some(ArmedVerb::Swap(a)) => {
                        armed.set(None);
                        if let Some(b) = port_window(output, port_key) {
                            dispatch_verb(
                                &on_action,
                                &surface,
                                &selection,
                                PatchVerbKind::SwapPorts { a, b },
                            );
                        }
                        return;
                    }
                    // An armed assign over a fixture-side selection: the
                    // clicked port (or the free run inside it) is the
                    // counterpart. The selection stays on the object —
                    // it is what the user is verifying.
                    Some(ArmedVerb::Assign) => {
                        armed.set(None);
                        // Still an armable end? (A selection that went
                        // mapped under the arm, or a wire-side one, is a
                        // nonsense pair — see below.)
                        if let Some(target) = selection
                            .as_ref()
                            .filter(|target| is_armable(&surface, target))
                        {
                            let lamp = match free_run {
                                Some((start, _)) => Some(start),
                                None => port_next_free(output, port_key),
                            };
                            // A whole-fixture selection means its next
                            // object still waiting for a wire — the same
                            // narrowing the sprite click does (P5b).
                            let subject = crate::app::editor_shell::patching::assign_subject_target(
                                &surface, target,
                            );
                            if let Some(lamp) = lamp
                                && dispatch_assign(&on_action, &surface, &subject, output, lamp)
                            {
                                // The override was fine-tuning for the
                                // segment the write just spent: the next
                                // one sizes itself off the next object
                                // again (the walk-up doc's improvement on
                                // lp2014's fixed chunk number).
                                segment_size.set(None);
                                return;
                            }
                        }
                        // A nonsense pair — a wire-side selection (segment
                        // meets port: both ends are the same side), or an
                        // object already on a wire — refuses: disarmed
                        // above, and the click falls through to plain
                        // selection. No tentative lane, no guessing.
                    }
                    None => {}
                }
            }
            // Plain click = select, always. Free space names a SEGMENT
            // auto-sized to the next unmapped object; the row names its
            // port.
            let target = match free_run {
                Some(run) => {
                    // A fresh click is a fresh window: the nudged size
                    // override belongs to the segment it was nudged on.
                    if let Some(mut size) = patching_ui.map(|ui| ui.segment_size) {
                        size.set(None);
                    }
                    segment_at_free_run(&surface, node, port_key, run, None)
                }
                None => UiPatchTarget::Port {
                    node,
                    port: port_key,
                },
            };
            select(&on_action, Some(target));
        }
    };
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
                    free_attention,
                    on_toggle,
                    on_port_click: {
                        let on_port_click = on_port_click.clone();
                        move |args| on_port_click(args)
                    },
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
                        free_attention,
                        on_toggle,
                        on_port_click: {
                            let on_port_click = on_port_click.clone();
                            move |args| on_port_click(args)
                        },
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
    /// Round 3: armed assign + fixture-side selection = the free runs glow.
    #[props(default)]
    free_attention: bool,
    selection: UiSelection,
    on_toggle: EventHandler<lpa_studio_core::NodeId>,
    /// A click on a port row or on free space in its bar — the panel
    /// supplies the grammar (select, or the Patching view's armed verbs).
    on_port_click: EventHandler<PortClick>,
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
                        free_attention,
                            key: "{output.node.0}-{port.key}",
                            node: output.node,
                            port,
                            selection: selection.clone(),
                            on_port_click,
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
    /// Round 3: the attention ring on this port's free runs.
    #[props(default)]
    free_attention: bool,
    selection: UiSelection,
    on_port_click: EventHandler<PortClick>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let target = UiPatchTarget::Port {
        node,
        port: port.key,
    };
    let used: u32 = port.cells.iter().map(|cell| cell.lamps).sum();
    let lamps = port.lamps.max(1);
    // Free space is a CLICK TARGET, not a hole: one transparent element per
    // free run, under the cells, so a click names the run it landed in
    // without measuring anything. The selected segment draws over them.
    let runs = free_runs(&port);
    let selected_segment = match selection.single() {
        Some(UiPatchTarget::Segment {
            node: seg_node,
            port: seg_port,
            start,
            lamps: seg_lamps,
        }) if *seg_node == node && *seg_port == port.key => Some((*start, *seg_lamps)),
        _ => None,
    };
    let span_style = |start: u32, span: u32| {
        let left = start.saturating_sub(port.start) as f32 / lamps as f32 * 100.0;
        let width = (span as f32 / lamps as f32 * 100.0).max(1.0);
        format!("left: {left}%; width: {width}%;")
    };
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
                    let key = port.key;
                    move |_| {
                        on_port_click
                            .call(PortClick {
                                node,
                                port: key,
                                free_run: None,
                            })
                    }
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
                // Free runs first: the cells paint over them, so a click
                // only ever reaches the gaps.
                for (start , span) in runs {
                    div {
                        key: "free-{start}",
                        class: if free_attention { "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:rounded-[3px] tw:hover:bg-selection-bg ux-arm-attention" } else { "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:rounded-[3px] tw:hover:bg-selection-bg" },
                        style: span_style(start, span),
                        title: "free — click to take a segment here",
                        onclick: {
                            let key = port.key;
                            move |_| {
                                on_port_click
                                    .call(PortClick {
                                        node,
                                        port: key,
                                        free_run: Some((start, span)),
                                    })
                            }
                        },
                    }
                }
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
                // The selected free segment: the window the arm completes
                // against, drawn over everything so the nudge keys have
                // something to move.
                if let Some((start , span)) = selected_segment {
                    div {
                        class: "tw:pointer-events-none tw:absolute tw:top-0 tw:h-full tw:rounded-[3px] tw:border tw:border-selection-border tw:bg-selection-bg",
                        style: span_style(start, span),
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
    selection: UiSelection,
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

/// The Props panel — the props STACK (B′ ruling, design record
/// `spikes/props-stack/index.html`): while a dive is live, the document
/// cards (deepest first, from the mapping editor) stack over the fixture's
/// PLACEMENT card, with the module chain as a muted context strip at the
/// bottom; at the arranged level the placement card stands alone for the
/// selected fixture. The arrange data lives HERE, in the shell — the
/// mapping-editor crate renders only authored document levels and stays
/// project-unaware.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PropsPanel(
    surface: Option<UiPatchSurface>,
    selection: UiSelection,
    dive_focused: Option<NodeId>,
    dive_session: Signal<MapEditorSession>,
    mut dive_commits: Signal<u64>,
    /// The Nodes view's route — the context strip's "Nodes ↗" link.
    workspace_href: String,
    on_action: EventHandler<UiAction>,
) -> Element {
    // Dived: the full stack. A dive whose fixture vanished from the
    // surface falls through to the arranged-level arms honestly.
    if let Some(node) = dive_focused
        && let Some(surface_now) = surface.as_ref()
        && let Some(fixture) = surface_now
            .fixtures
            .iter()
            .find(|fixture| fixture.node == node)
            .cloned()
    {
        let chain = fixture
            .module
            .map(|module| module_chain(&surface_now.modules, module))
            .unwrap_or_default();
        // With no document selection the placement card IS the top (only)
        // card — the fixture level is what esc's clear-rung leaves
        // selected. Never a "select an object" placeholder.
        let fixture_is_top = dive_session.read().selection.is_empty();
        return rsx! {
            div { class: "lpme-stack",
                ObjectPropertiesPane {
                    session: dive_session,
                    on_committed: move |()| {
                        let next = *dive_commits.peek() + 1;
                        dive_commits.set(next);
                    },
                }
                PlacementCard {
                    fixture,
                    surface: surface_now.clone(),
                    dived: true,
                    selected: fixture_is_top,
                    dive_focused,
                    dive_session,
                    on_action,
                }
            }
            ModuleContextStrip { chain, workspace_href }
        };
    }
    // A selected MODULE row reads out its context facts — a context level
    // (the stack's document/placement cards never grow module fields; the
    // real module UX lives on the Nodes view).
    if let (Some(surface), Some(UiPatchTarget::Module { node })) = (&surface, selection.single())
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
        let count = |n: usize, word: &str| {
            if n == 1 {
                format!("1 {word}")
            } else {
                format!("{n} {word}s")
            }
        };
        let facts = format!(
            "module · {} · {}",
            count(fixtures, "fixture"),
            count(outputs, "output")
        );
        let chain = module_chain(&surface.modules, module.node);
        return rsx! {
            div { class: "tw:grid tw:content-start tw:gap-1 tw:px-1.5 tw:text-sm",
                p { class: "tw:m-0 tw:font-medium tw:text-foreground", "{module.label}" }
                p { class: "tw:m-0 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground",
                    "{facts}"
                }
                p { class: "tw:m-0 tw:mt-2 tw:text-[11px] tw:text-dim-foreground",
                    "Select a fixture beneath it for placement, or dive in for object properties."
                }
            }
            ModuleContextStrip { chain, workspace_href }
        };
    }
    // WIRE-side selections (the Patching view's leaves): readout cards in
    // the same stack family, deepest first (B′) — the selected level is
    // the top card, its wire ancestors unwind beneath, the module chain
    // stays the muted strip. Cards are READOUTS in v1: the verbs act on
    // the tree/canvas/port selection, never on card fields.
    if let Some(surface_now) = surface.as_ref() {
        match selection.single() {
            Some(UiPatchTarget::Output { node }) => {
                if let Some(output) = surface_now
                    .outputs
                    .iter()
                    .find(|output| output.node == *node)
                {
                    let chain = output
                        .module
                        .map(|module| module_chain(&surface_now.modules, module))
                        .unwrap_or_default();
                    return rsx! {
                        div { class: "lpme-stack",
                            WireFactCard {
                                glyph: "▦",
                                name: output.display_name().to_string(),
                                kind: "output".to_string(),
                                facts: output_facts(output),
                                selected: true,
                            }
                        }
                        ModuleContextStrip { chain, workspace_href }
                    };
                }
            }
            Some(UiPatchTarget::Port { node, port }) => {
                if let Some(output) = surface_now
                    .outputs
                    .iter()
                    .find(|output| output.node == *node)
                    && let Some(port) = output.bay.ports.iter().find(|entry| entry.key == *port)
                {
                    let chain = output
                        .module
                        .map(|module| module_chain(&surface_now.modules, module))
                        .unwrap_or_default();
                    return rsx! {
                        div { class: "lpme-stack",
                            WireFactCard {
                                glyph: "⎓",
                                name: port_name(port),
                                kind: "port".to_string(),
                                facts: port_facts(port),
                                selected: true,
                            }
                            WireFactCard {
                                glyph: "▦",
                                name: output.display_name().to_string(),
                                kind: "output".to_string(),
                                facts: output_facts(output),
                                selected: false,
                            }
                        }
                        ModuleContextStrip { chain, workspace_href }
                    };
                }
            }
            Some(UiPatchTarget::Segment {
                node,
                port,
                start,
                lamps,
            }) => {
                if let Some(output) = surface_now
                    .outputs
                    .iter()
                    .find(|output| output.node == *node)
                    && let Some(port) = output.bay.ports.iter().find(|entry| entry.key == *port)
                {
                    let chain = output
                        .module
                        .map(|module| module_chain(&surface_now.modules, module))
                        .unwrap_or_default();
                    let mapped = segment_mapped_lamps(port, *start, *lamps);
                    return rsx! {
                        div { class: "lpme-stack",
                            WireFactCard {
                                glyph: "▬",
                                name: if mapped == 0 {
                                    "free segment".to_string()
                                } else {
                                    "segment".to_string()
                                },
                                kind: "segment".to_string(),
                                facts: segment_facts(*start, *lamps, mapped),
                                selected: true,
                            }
                            WireFactCard {
                                glyph: "⎓",
                                name: port_name(port),
                                kind: "port".to_string(),
                                facts: port_facts(port),
                                selected: false,
                            }
                        }
                        ModuleContextStrip { chain, workspace_href }
                    };
                }
            }
            Some(UiPatchTarget::Cell { id }) => {
                // The owning output/port carry the labelled cell copy —
                // the bay's, not the fixture's.
                let owning = surface_now.outputs.iter().find_map(|output| {
                    output
                        .bay
                        .ports
                        .iter()
                        .find_map(|port| {
                            port.cells
                                .iter()
                                .find(|cell| cell.id == *id)
                                .map(|cell| (port, cell))
                        })
                        .map(|(port, cell)| (output, port, cell))
                });
                if let Some((output, port, cell)) = owning {
                    let chain = output
                        .module
                        .map(|module| module_chain(&surface_now.modules, module))
                        .unwrap_or_default();
                    return rsx! {
                        div { class: "lpme-stack",
                            WireFactCard {
                                glyph: "▭",
                                name: cell.producer.clone(),
                                kind: "cell".to_string(),
                                facts: cell_facts(cell),
                                selected: true,
                            }
                            WireFactCard {
                                glyph: "⎓",
                                name: port_name(port),
                                kind: "port".to_string(),
                                facts: port_facts(port),
                                selected: false,
                            }
                        }
                        ModuleContextStrip { chain, workspace_href }
                    };
                }
            }
            _ => {}
        }
    }
    // A MULTI selection has no one placement to edit: the stack states the
    // count honestly instead of showing one member's card (the gesture
    // acts on the whole set; per-member edits come from selecting one).
    if selection.len() > 1 {
        let count = selection.len();
        let word = match selection.primary() {
            Some(UiPatchTarget::Fixture { .. }) => "fixtures",
            _ => "objects",
        };
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "{count} {word} selected — drag to move them together, or select one to edit its placement."
            }
        };
    }
    let fixture = match (&surface, selection.single()) {
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
            .find(|fixture| fixture.node == *node)
            .cloned(),
        _ => None,
    };
    let (Some(fixture), Some(surface)) = (fixture, surface) else {
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "Select a fixture on the canvas — its placement edits here; dive in (double-click) for object properties."
            }
        };
    };
    let chain = fixture
        .module
        .map(|module| module_chain(&surface.modules, module))
        .unwrap_or_default();
    rsx! {
        div { class: "lpme-stack",
            PlacementCard {
                fixture,
                surface,
                dived: false,
                selected: true,
                dive_focused,
                dive_session,
                on_action,
            }
        }
        ModuleContextStrip { chain, workspace_href }
    }
}

/// A wire-side readout card (output / port / cell) in the stack's card
/// family: head row + mono fact rows, no edit fields (v1 — the verbs act
/// on selections, not cards).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WireFactCard(
    glyph: &'static str,
    name: String,
    kind: String,
    facts: Vec<(String, String)>,
    selected: bool,
) -> Element {
    let card_class = if selected {
        "lpme-lvl lpme-lvl-sel"
    } else {
        "lpme-lvl"
    };
    rsx! {
        div { class: "{card_class}",
            div { class: "lpme-lvl-head",
                span { class: "tw:w-3 tw:flex-none tw:text-center tw:text-[10px] tw:text-dim-foreground",
                    "{glyph}"
                }
                span { class: "lpme-lvl-name", "{name}" }
                span { class: "lpme-lvl-kind", "{kind}" }
            }
            div { class: "lpme-lvl-body",
                for (label , value) in facts {
                    div { class: "tw:flex tw:items-baseline tw:gap-2 tw:px-0.5",
                        span { class: "tw:w-16 tw:flex-none tw:text-[10px] tw:text-dim-foreground",
                            "{label}"
                        }
                        span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[11px] tw:text-subtle-foreground",
                            "{value}"
                        }
                    }
                }
            }
        }
    }
}

fn port_name(port: &lpa_studio_core::UiPatchPort) -> String {
    if port.pin_label.is_empty() {
        format!("port {}", port.key)
    } else {
        port.pin_label.clone()
    }
}

fn output_facts(output: &UiPatchSurfaceOutput) -> Vec<(String, String)> {
    let used: u32 = output
        .bay
        .ports
        .iter()
        .flat_map(|port| port.cells.iter())
        .map(|cell| cell.lamps)
        .sum();
    let total: u32 = output.bay.ports.iter().map(|port| port.lamps).sum();
    let mut facts = vec![
        ("ports".to_string(), output.bay.ports.len().to_string()),
        ("lamps".to_string(), format!("{used}/{total} used")),
    ];
    if output.name.is_none() {
        if let Some((_, name)) = &output.name_assign {
            facts.push((
                "name".to_string(),
                format!("unnamed — first assign names it \"{name}\""),
            ));
        }
    }
    if output.bay.contested_lamps > 0 {
        facts.push((
            "contested".to_string(),
            format!("{} lamps", output.bay.contested_lamps),
        ));
    }
    facts
}

fn port_facts(port: &lpa_studio_core::UiPatchPort) -> Vec<(String, String)> {
    let used: u32 = port.cells.iter().map(|cell| cell.lamps).sum();
    // 1-based spans, the chips' own convention.
    let mut facts = vec![
        (
            "wire".to_string(),
            format!("{}-{}", port.start + 1, port.start + port.lamps),
        ),
        ("lamps".to_string(), format!("{used}/{} used", port.lamps)),
        ("cells".to_string(), port.cells.len().to_string()),
    ];
    let next_free = port
        .cells
        .iter()
        .map(|cell| cell.wire_start + cell.lamps)
        .max()
        .unwrap_or(port.start)
        .max(port.start);
    if next_free < port.start + port.lamps {
        facts.push(("next free".to_string(), format!("lamp {}", next_free + 1)));
    }
    facts
}

/// Lamps of a segment window that some cell already claims — a free
/// segment's window should be zero, and saying so honestly beats implying a
/// clear run over an occupied one.
fn segment_mapped_lamps(port: &lpa_studio_core::UiPatchPort, start: u32, lamps: u32) -> u32 {
    let end = start.saturating_add(lamps);
    port.cells
        .iter()
        .map(|cell| {
            let cell_end = cell.wire_start.saturating_add(cell.lamps);
            end.min(cell_end).saturating_sub(start.max(cell.wire_start))
        })
        .sum()
}

fn segment_facts(start: u32, lamps: u32, mapped: u32) -> Vec<(String, String)> {
    // 1-based spans, the chips' own convention.
    let mut facts = vec![
        (
            "wire".to_string(),
            format!("{}-{}", start + 1, start.saturating_add(lamps)),
        ),
        ("lamps".to_string(), lamps.to_string()),
    ];
    if mapped > 0 {
        facts.push(("mapped".to_string(), format!("{mapped} lamps taken")));
    }
    facts
}

fn cell_facts(cell: &UiPatchCell) -> Vec<(String, String)> {
    let mut facts = vec![
        ("wire".to_string(), chip_text(cell)),
        (
            "source".to_string(),
            format!(
                "{}-{}",
                cell.source_start + 1,
                cell.source_start + cell.lamps
            ),
        ),
        ("lamps".to_string(), cell.lamps.to_string()),
    ];
    if cell.reversed {
        facts.push(("direction".to_string(), "reversed".to_string()));
    }
    if cell.contested {
        facts.push(("contested".to_string(), "yes — lamps are dark".to_string()));
    }
    facts
}

/// The fixture's PLACEMENT card: `editor.json` arrange data as editable
/// fields (x/y/rotation/scale — each commit is ONE `EditorMetaVerb::Set`,
/// one arrange undo step; no mid-typing preview, the controller path has
/// no uncommitted lane) plus the "edit mapping" action. Wears the same
/// `lpme-lvl` card language as the document cards above it (one pane, one
/// family). While dived with a document selection it is a muted context
/// card whose header click pops selection back to the fixture level —
/// the same state esc's clear-rung produces.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PlacementCard(
    fixture: UiPatchSurfaceFixture,
    surface: UiPatchSurface,
    /// Whether the pane is dived into this fixture.
    dived: bool,
    /// Whether this card is the stack's top/selected card.
    selected: bool,
    dive_focused: Option<NodeId>,
    mut dive_session: Signal<MapEditorSession>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let arrange = fixture.arrange.clone().unwrap_or_default();
    let transform = arrange.transform;
    let node = fixture.node;
    let node_key = fixture.address.clone();
    let can_edit_mapping = fixture.mapping_artifact.is_some();
    let range_grain = fixture.instances.is_empty();
    let meta = if range_grain {
        format!("{} lamps · range", fixture.patch.lamps)
    } else {
        format!(
            "{} lamps · {} instances",
            fixture.patch.lamps,
            fixture.instances.len()
        )
    };
    let dispatch = crate::app::editor_shell::arrange_dispatch(&surface);
    let commit = EventHandler::new(move |transform: UiArrangeTransform| {
        let Some(node_key) = node_key.clone() else {
            return;
        };
        if let Some(op) = dispatch(lpa_studio_core::EditorMetaVerb::Set {
            node_key,
            node: Some(node),
            transform,
        }) {
            on_action.call(UiAction::from_op(ProjectController::NODE_ID, op));
        }
    });
    let card_class = if selected {
        "lpme-lvl lpme-lvl-sel"
    } else {
        "lpme-lvl"
    };
    rsx! {
        div { class: "{card_class}",
            div {
                class: "lpme-lvl-head",
                title: if dived { "select the fixture level" } else { "" },
                onclick: move |_| {
                    // Dived: ASCEND the one selection — the fixture becomes
                    // the selection at project level (the dive closes,
                    // scope being derived). Not dived, the fixture already
                    // IS the selection.
                    if dived {
                        dive_session.write().selection.clear();
                        let mut next = lpa_studio_core::UiSelection::empty();
                        next.select_one(UiPatchTarget::Fixture { node });
                        crate::app::editor_shell::selection::dispatch_selection(&on_action, next);
                    }
                },
                span { class: "tw:w-3 tw:flex-none tw:text-center tw:text-[10px] tw:text-dim-foreground",
                    "⬡"
                }
                span { class: "lpme-lvl-name", "{fixture.label}" }
                span { class: "lpme-lvl-kind", "{meta}" }
            }
            div { class: "lpme-lvl-body",
                if arrange.arranged {
                    PlacementField {
                        label: "x",
                        shown: format!("{:.1}", transform.t[0]),
                        on_commit: move |v: f64| {
                            let mut t = transform;
                            t.t[0] = v;
                            commit.call(t);
                        },
                    }
                    PlacementField {
                        label: "y",
                        shown: format!("{:.1}", transform.t[1]),
                        on_commit: move |v: f64| {
                            let mut t = transform;
                            t.t[1] = v;
                            commit.call(t);
                        },
                    }
                    PlacementField {
                        label: "rotation",
                        shown: format!("{:.0}", transform.r),
                        on_commit: move |v: f64| {
                            let mut t = transform;
                            t.r = v;
                            commit.call(t);
                        },
                    }
                    PlacementField {
                        label: "scale",
                        shown: format!("{:.2}", transform.s),
                        on_commit: move |v: f64| {
                            let mut t = transform;
                            // Same bound the toolbar verbs enforce.
                            t.s = v.clamp(0.05, 20.0);
                            commit.call(t);
                        },
                    }
                } else {
                    div { class: "lpme-pop-meta", "unarranged — drag it on the canvas to place it" }
                }
                if !dived {
                    div { class: "lpme-pop-actions",
                        button {
                            class: "lpme-btn",
                            title: "Edit this fixture's mapping (double-click on the canvas does too)",
                            disabled: !can_edit_mapping,
                            onclick: {
                                let surface = surface.clone();
                                move |_| {
                                    crate::app::editor_shell::enter_dive(&on_action, &surface, node);
                                }
                            },
                            "edit mapping"
                        }
                    }
                }
            }
        }
    }
}

/// One placement field row: number input committing on CHANGE only (there
/// is no uncommitted-preview lane through the controller). While the user
/// types, a local draft signal owns the rendered value — live frames
/// re-render this panel constantly, and a value bound straight to the DTO
/// would clobber in-progress typing every frame. The draft drops on
/// change/blur, so canonical values (canvas drags, undo) show through
/// whenever the field is not being edited.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PlacementField(label: &'static str, shown: String, on_commit: EventHandler<f64>) -> Element {
    let mut draft = use_signal(|| None::<String>);
    let value = draft.read().clone().unwrap_or(shown);
    rsx! {
        div { class: "lpme-field",
            label { "{label}" }
            input {
                r#type: "number",
                value: "{value}",
                oninput: move |evt| draft.set(Some(evt.value())),
                onchange: move |evt| {
                    if let Ok(parsed) = evt.value().parse::<f64>()
                        && parsed.is_finite()
                    {
                        on_commit.call(parsed);
                    }
                    draft.set(None);
                },
                onblur: move |_| draft.set(None),
            }
        }
    }
}

/// The module chain enclosing `leaf` (root-first): the surface's modules
/// are a flattened preorder walk, so the chain is the nearest entry at
/// each strictly smaller depth walking backward from the leaf.
fn module_chain(modules: &[UiPatchSurfaceModule], leaf: NodeId) -> Vec<UiPatchSurfaceModule> {
    let Some(mut index) = modules.iter().position(|module| module.node == leaf) else {
        return Vec::new();
    };
    let mut chain = vec![modules[index].clone()];
    let mut depth = modules[index].depth;
    while depth > 0 && index > 0 {
        index -= 1;
        if modules[index].depth < depth {
            depth = modules[index].depth;
            chain.push(modules[index].clone());
        }
    }
    chain.reverse();
    chain
}

/// The module chain as a muted one-line CONTEXT strip under the stack —
/// these levels are homed on the Nodes view (the "Nodes ↗" link), never
/// edited here (scope ruling: a node level rendered as a card would have
/// to be the real node card re-housed whole — a someday-unification).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModuleContextStrip(chain: Vec<UiPatchSurfaceModule>, workspace_href: String) -> Element {
    if chain.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "tw:mt-2 tw:flex tw:items-center tw:gap-1 tw:overflow-hidden tw:whitespace-nowrap tw:px-1 tw:text-[10.5px] tw:text-dim-foreground",
            span { class: "tw:flex-none", "▤" }
            for (slot, module) in chain.into_iter().enumerate() {
                if slot > 0 {
                    span { class: "tw:flex-none tw:text-[9px]", "▸" }
                }
                span { class: "tw:truncate tw:text-subtle-foreground", "{module.label}" }
            }
            a {
                class: "tw:ml-auto tw:flex-none tw:text-[10px] tw:text-selection-border tw:no-underline tw:opacity-75 tw:hover:opacity-100",
                href: "{workspace_href}",
                title: "modules are edited on the Nodes view",
                "Nodes ↗"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_chain_walks_ancestors_by_depth() {
        // Preorder: root(0) > rig(1) > wing(2), then a SIBLING branch
        // spare(1) — the chain from `wing` must skip `spare`.
        let module = |id: u32, label: &str, depth: usize| UiPatchSurfaceModule {
            node: NodeId::new(id),
            label: label.to_string(),
            address: None,
            depth,
        };
        let modules = vec![
            module(1, "root", 0),
            module(2, "rig", 1),
            module(3, "wing", 2),
            module(4, "spare", 1),
        ];
        let names = |leaf: u32| {
            module_chain(&modules, NodeId::new(leaf))
                .into_iter()
                .map(|module| module.label)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(3), ["root", "rig", "wing"]);
        assert_eq!(names(4), ["root", "spare"]);
        assert_eq!(names(1), ["root"]);
        assert!(names(99).is_empty(), "unknown leaf yields no chain");
    }

    /// The D46 bridge: a resolved instance path (`/sector/2`) names the
    /// authored object by its STICKY ID segment — never by name, never by
    /// position — and nested instance steps don't confuse it.
    #[test]
    fn instance_paths_derive_their_authored_object() {
        use lpc_mapping::{Map2dObject, Map2dObjectId, Map2dShape, PathAlign, PathShape};
        let object = |name: &str, id: Option<&str>| Map2dObject {
            name: name.to_string(),
            id: id.map(|id| Map2dObjectId::new(id).expect("test id")),
            stride: None,
            shape: Map2dShape::Path(PathShape {
                points: vec![[0.0, 0.0], [1.0, 0.0]],
                count: 10,
                gaps: Vec::new(),
                reversed: false,
                align: PathAlign::On,
            }),
        };
        let mut doc = Map2dDoc::new();
        doc.objects.push(object("Renamed Sector", Some("sector")));
        doc.objects.push(object("door", Some("door")));
        doc.objects.push(object("no id yet", None));

        assert_eq!(
            authored_object_for_instance_path(&doc, "/sector/2"),
            Some(0)
        );
        assert_eq!(authored_object_for_instance_path(&doc, "/door"), Some(1));
        assert_eq!(
            authored_object_for_instance_path(&doc, "/panels/3/2"),
            None,
            "unknown id segment derives nothing"
        );
        assert_eq!(
            authored_object_for_instance_path(&doc, ""),
            None,
            "an empty path derives nothing"
        );
        assert_eq!(
            authored_object_for_instance_path(&doc, "/no id yet"),
            None,
            "names never stand in for ids"
        );
    }
}
