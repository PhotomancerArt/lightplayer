//! The Outputs panel — the Nodes view's right-dock slice over the
//! patch-surface DTOs, sharing the surface's ONE core-owned selection
//! (`ProjectEditorOp::PatchSelect`).
//!
//! Rows, cells and free port space SELECT, everywhere and always — plain
//! clicks never write (D7).
//!
//! Density rules (spike §3, ratified): a flat stack of slim section
//! headers (chevron · name · occupancy) with their port rows beneath —
//! each box opens INDEPENDENTLY, all open by default (R4-3: collapsing is
//! how a radiance-scale project gets its 10×32 back under control). Port
//! identity is text + position — port hues were cut.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeId, ProjectController, ProjectEditorOp, UiAction, UiPatchCell, UiPatchSurface,
    UiPatchSurfaceModule, UiPatchSurfaceOutput, UiPatchTarget, UiSelection,
};

use super::free_port_runs::{free_runs, segment_at_free_run};

/// Dispatch a selection change to the core — the surface's one
/// selection.
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

/// Fetch-on-demand: resolve unfetched map2d bodies (a no-op once cached).
/// The demand signal is a panel being OPEN — the lazy-loading principle,
/// not an eager sweep.
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
    }
}

/// `ux-row-edge` is the dense-row interaction light (see
/// [`row_edge_class`](crate::base::row_edge_class)): a spectrum left edge
/// and bloom on hover, not the full ring the buttons wear. It rides the
/// IDLE row only — the selected row wears the chosen-object ring (cool).
const ROW_IDLE: &str = "tw:flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-md tw:border tw:border-transparent tw:px-1.5 tw:py-1 ux-row-edge";
const ROW_SELECTED: &str = "tw:flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-md ux-sel-ring ux-sel-ring-cool tw:border tw:border-transparent tw:px-1.5 tw:py-1";
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

/// One module context row: the tree level above the outputs (G1),
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

/// Where a click landed on a port: the port row proper, or one of the FREE
/// runs in its bar (`(start, lamps)` in wire numbering). The port bar's
/// gaps are real elements, so "which free run" needs no geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PortClick {
    node: NodeId,
    port: u32,
    free_run: Option<(u32, u32)>,
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
    on_action: EventHandler<UiAction>,
) -> Element {
    // The CLOSED boxes, by output node id — closed is the exception, so
    // an output the panel has never seen reads as open by default and no
    // seeding pass is needed when the surface grows.
    let mut collapsed = use_signal(std::collections::BTreeSet::<lpa_studio_core::NodeId>::new);
    let Some(surface) = surface else {
        return rsx! {
            p { class: "tw:mt-3 tw:px-2 tw:text-center tw:text-xs tw:text-dim-foreground",
                "Nothing to patch yet. No output has a wire."
            }
        };
    };
    prefetch_bodies(&on_action, &surface);
    // The port-click grammar: a click selects, always (D7 — looking is
    // free, nothing writes). Free space names the SEGMENT of its run; the
    // row names its port.
    let on_port_click = move |click: PortClick| {
        let PortClick {
            node,
            port: port_key,
            free_run,
        } = click;
        let target = match free_run {
            Some(run) => segment_at_free_run(node, port_key, run),
            None => UiPatchTarget::Port {
                node,
                port: port_key,
            },
        };
        select(&on_action, Some(target));
    };
    // Module grouping: the tree above the outputs is context worth a slim
    // row, never an empty header.
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
                    on_port_click,
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
                        on_port_click,
                        on_action,
                    }
                }
            }
        }
    }
}

/// One output: a slim section-header ROW (chevron · name · occupancy) with
/// its port rows indented beneath — no card, no nested border, so the panel
/// reads flat (R4-3).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OutputBox(
    output: UiPatchSurfaceOutput,
    expanded: bool,
    /// Tree level under the module rows (0 = no enclosing module).
    indent: usize,
    selection: UiSelection,
    on_toggle: EventHandler<lpa_studio_core::NodeId>,
    /// A click on a port row or on free space in its bar — the panel
    /// supplies the grammar (select).
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
        "tw:flex tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-md ux-sel-ring ux-sel-ring-cool tw:border tw:border-transparent tw:px-1.5 tw:py-1"
    } else {
        "tw:flex tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-transparent tw:px-1.5 tw:py-1 ux-row-edge"
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
        "tw:flex tw:cursor-pointer tw:items-baseline tw:gap-1.5 tw:rounded ux-sel-ring ux-sel-ring-cool tw:border tw:border-transparent tw:px-1"
    } else {
        "tw:flex tw:cursor-pointer tw:items-baseline tw:gap-1.5 tw:rounded tw:border tw:border-transparent tw:px-1 ux-row-edge"
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
                        class: "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:rounded-[3px] tw:hover:bg-selection-bg",
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
                // The selected free segment, drawn over everything.
                if let Some((start , span)) = selected_segment {
                    div {
                        class: "tw:pointer-events-none tw:absolute tw:top-0 tw:h-full tw:rounded-[3px] ux-sel-ring ux-sel-ring-cool tw:border tw:border-transparent tw:bg-selection-bg",
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
        "tw:absolute tw:top-0 tw:flex tw:h-full tw:cursor-pointer tw:items-center tw:justify-center tw:overflow-hidden tw:rounded-[3px] ux-sel-ring ux-sel-ring-cool ux-sel-ring-inset tw:border tw:border-transparent tw:bg-card-raised"
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
