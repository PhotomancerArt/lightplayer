//! The project-scoped PATCH SURFACE (D36, slice 2) — the full-page patching
//! home: output → port sidebar, per-output lamp canvases, and the two-sided
//! cell strips, all sharing ONE selection (core-owned,
//! `ProjectEditorOp::PatchSelect`) and the workspace's twin-hover context.
//!
//! Read-only in P5: clicking selects (canvas instance, bay cell, sidebar
//! port/output, fixture chip); nothing writes. P6 adds the verbs over this
//! exact selection model.
//!
//! The canvas is SVG with per-lamp hit targets (the editor-canvas idiom):
//! at patching scale (hundreds of lamps) the vdom cost is fine, and a lamp
//! click resolves through the output's placements to the run — and through
//! the fixture's instance table to the `/sector/2` grain. Colors come from
//! the output's published frame at sync cadence; the 60 Hz direct-DOM fill
//! is a later refinement, not a P5 requirement.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControlDisplayLayout, PatchVerbFixture, PatchVerbKind, PatchVerbOp, PatchVerbSubject,
    PatchVerbWindow, ProjectController, ProjectEditorOp, ProjectEditorView, UiAction,
    UiPatchSurface, UiPatchSurfaceFixture, UiPatchSurfaceOutput, UiPatchTarget,
};

use crate::app::node::HoveredPatchCell;

/// Every fixture's write-target facts, for the verb ops.
fn verb_fixtures(surface: &UiPatchSurface) -> Vec<PatchVerbFixture> {
    surface
        .fixtures
        .iter()
        .filter_map(|fixture| {
            Some(PatchVerbFixture {
                node: fixture.node,
                patch_artifact: fixture.patch_artifact.clone()?,
                mapping_artifact: fixture.mapping_artifact.clone(),
                lamp_count: fixture.patch.lamps,
            })
        })
        .collect()
}

/// The verb subject for the current selection: an instance path, a cell's
/// range (or its instance, when one covers it), or the whole fixture.
fn verb_subject(
    surface: &UiPatchSurface,
    selection: &UiPatchTarget,
) -> Option<(lpa_studio_core::NodeId, PatchVerbSubject, u32)> {
    match selection {
        UiPatchTarget::Instance { node, path } => {
            let fixture = surface.fixtures.iter().find(|f| f.node == *node)?;
            let stride = fixture
                .instances
                .iter()
                .find(|instance| instance.path == *path)
                .map(|instance| instance.stride)
                .unwrap_or(1);
            Some((
                *node,
                PatchVerbSubject {
                    path: Some(path.clone()),
                    range: None,
                },
                stride,
            ))
        }
        UiPatchTarget::Fixture { node } => Some((
            *node,
            PatchVerbSubject {
                path: None,
                range: None,
            },
            1,
        )),
        UiPatchTarget::Cell { id } => {
            // Cell ids are `node:output:source:wire` (the bay's format).
            let node: u32 = id.split(':').next()?.parse().ok()?;
            let node = lpa_studio_core::NodeId::new(node);
            let fixture = surface.fixtures.iter().find(|f| f.node == node)?;
            let cell = fixture.patch.cells.iter().find(|cell| cell.id == *id)?;
            // Prefer the instance covering the cell — path entries match by
            // path, and the instance's stride is the honest rotation step.
            if let Some(instance) = fixture.instances.iter().find(|instance| {
                cell.source_start >= instance.start
                    && cell.source_start < instance.start + instance.lamps
            }) {
                return Some((
                    node,
                    PatchVerbSubject {
                        path: Some(instance.path.clone()),
                        range: None,
                    },
                    instance.stride,
                ));
            }
            Some((
                node,
                PatchVerbSubject {
                    path: None,
                    range: Some((cell.source_start, Some(cell.lamps))),
                },
                1,
            ))
        }
        UiPatchTarget::Output { .. } | UiPatchTarget::Port { .. } => None,
    }
}

/// Dispatch one verb over the current selection.
fn dispatch_verb(
    on_action: &EventHandler<UiAction>,
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    verb: PatchVerbKind,
) {
    let subject = selection
        .as_ref()
        .and_then(|selection| verb_subject(surface, selection));
    let (subject_fixture, subject) = match (&verb, subject) {
        // Undo/redo and port verbs need no subject.
        (
            PatchVerbKind::Undo
            | PatchVerbKind::Redo
            | PatchVerbKind::SwapPorts { .. }
            | PatchVerbKind::ShiftPort { .. },
            resolved,
        ) => (
            resolved.map(|(node, _, _)| node),
            PatchVerbSubject::default(),
        ),
        (_, Some((node, subject, _))) => (Some(node), subject),
        (_, None) => return,
    };
    on_action.call(UiAction::from_op(
        ProjectController::NODE_ID,
        PatchVerbOp {
            subject_fixture,
            subject,
            fixtures: verb_fixtures(surface),
            assign_output_name: None,
            verb,
        },
    ));
}

/// The selection's rotation stride (1 when it has none).
fn selection_stride(surface: &UiPatchSurface, selection: &Option<UiPatchTarget>) -> u32 {
    selection
        .as_ref()
        .and_then(|selection| verb_subject(surface, selection))
        .map(|(_, _, stride)| stride)
        .unwrap_or(1)
}

/// The next free wire lamp on a port (after its last occupied cell).
fn port_next_free(output: &UiPatchSurfaceOutput, port_key: u32) -> Option<u32> {
    let port = output.bay.ports.iter().find(|port| port.key == port_key)?;
    let last = port
        .cells
        .iter()
        .map(|cell| cell.wire_start + cell.lamps)
        .max()
        .unwrap_or(port.start);
    Some(last.max(port.start))
}

/// Dispatch a selection change to the core.
fn select(on_action: &EventHandler<UiAction>, target: Option<UiPatchTarget>) {
    // Editor ops route by editor TARGET id (a child of the project
    // controller); PatchSelect ignores the target, so the tree-level one
    // is the honest choice.
    on_action.call(UiAction::from_op(
        lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
        ProjectEditorOp::PatchSelect { target },
    ));
}

/// Is `target` the current selection?
fn is_selected(selection: &Option<UiPatchTarget>, target: &UiPatchTarget) -> bool {
    selection.as_ref() == Some(target)
}

/// The full-page surface: sidebar | per-output wires + fixture rows.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PatchSurfacePage(view: ProjectEditorView, on_action: EventHandler<UiAction>) -> Element {
    // The surface provides its own twin-hover context: it renders outside
    // the workspace, and its bay strips + canvases hover as one.
    use_context_provider(|| HoveredPatchCell(Signal::new(None)));
    let selection = view.patch_selection.clone();
    // Resolve unfetched map2d AND patch bodies — the same fetch op the
    // editor tab dispatches on mount (a no-op once the body is cached, so
    // re-renders cost nothing). The map2d body fills the instance chips;
    // the patch body is what the verbs transform, so without this fetch
    // every edit blocks with "still loading".
    if let Some(surface) = view.patch_surface.as_ref() {
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
    let Some(surface) = view.patch_surface.clone() else {
        return rsx! {
            section { class: "tw:grid tw:gap-3 tw:content-start",
                h2 { class: "tw:text-lg tw:font-semibold", "Patch" }
                p { class: "tw:text-sm tw:opacity-70",
                    "Nothing to patch yet: no output has published a wire. \
                     Bind an output to a control bus and the surface fills in."
                }
            }
        };
    };
    let mut help_open = use_signal(|| false);
    // The swap verb's second target: `s` arms it from the selected port,
    // the next port click completes the exchange.
    let mut armed_swap = use_signal(|| Option::<PatchVerbWindow>::None);
    let port_window = |output: &UiPatchSurfaceOutput, key: u32| {
        output
            .bay
            .ports
            .iter()
            .find(|port| port.key == key)
            .map(|port| PatchVerbWindow {
                output_name: output.name.clone(),
                start: port.start,
                lamps: port.lamps,
            })
    };
    let on_port_click = {
        let surface = surface.clone();
        let selection = selection.clone();
        let on_action = on_action;
        move |(output_index, port_key): (usize, u32)| {
            let Some(output) = surface.outputs.get(output_index) else {
                return;
            };
            // An armed swap completes on the next port click.
            let armed = armed_swap.peek().clone();
            if let Some(a) = armed {
                if let Some(b) = port_window(output, port_key) {
                    armed_swap.set(None);
                    dispatch_verb(
                        &on_action,
                        &surface,
                        &selection,
                        PatchVerbKind::SwapPorts { a, b },
                    );
                }
                return;
            }
            // With a fixture-side selection, a port click ASSIGNS it there
            // (spike §5's grammar); otherwise it selects the port.
            let has_subject = selection
                .as_ref()
                .and_then(|selection| verb_subject(&surface, selection))
                .is_some();
            if has_subject {
                let Some(lamp) = port_next_free(output, port_key) else {
                    return;
                };
                let mut op_ready = PatchVerbKind::Assign {
                    output_name: output.name.clone(),
                    lamp,
                };
                // An unnamed output gets its numeric default alongside the
                // write (D39) — the DTO prebuilt it.
                if output.name.is_none()
                    && let Some((_, name)) = output.name_assign.clone()
                {
                    op_ready = PatchVerbKind::Assign {
                        output_name: Some(name),
                        lamp,
                    };
                }
                let assign_name = output.name_assign.clone();
                let subject = selection
                    .as_ref()
                    .and_then(|selection| verb_subject(&surface, selection));
                if let Some((node, subject, _)) = subject {
                    on_action.call(UiAction::from_op(
                        ProjectController::NODE_ID,
                        PatchVerbOp {
                            subject_fixture: Some(node),
                            subject,
                            fixtures: verb_fixtures(&surface),
                            assign_output_name: assign_name,
                            verb: op_ready,
                        },
                    ));
                }
            } else {
                select(
                    &on_action,
                    Some(UiPatchTarget::Port {
                        node: output.node,
                        port: port_key,
                    }),
                );
            }
        }
    };
    rsx! {
        section {
            class: "tw:grid tw:grid-cols-[minmax(200px,260px)_minmax(0,1fr)] tw:gap-3.5 tw:content-start tw:max-[880px]:grid-cols-1",
            // The keyboard grammar (map-editor root-onkeydown precedent):
            // r reverse · ;/' rotate ∓/± stride · s arm swap · Escape
            // ladder · ⌘Z/⌘⇧Z undo/redo · ? help.
            tabindex: 0,
            onkeydown: {
                let on_action = on_action;
                let surface = surface.clone();
                let selection = selection.clone();
                move |event: KeyboardEvent| {
                    let meta = event.modifiers().meta() || event.modifiers().ctrl();
                    match event.key() {
                        Key::Escape => {
                            // The ladder: drop the armed swap first, then
                            // the selection.
                            if armed_swap.peek().is_some() {
                                armed_swap.set(None);
                            } else {
                                select(&on_action, None);
                            }
                        }
                        Key::Character(key) => match key.as_str() {
                            "r" => dispatch_verb(
                                &on_action,
                                &surface,
                                &selection,
                                PatchVerbKind::Reverse,
                            ),
                            ";" => {
                                let stride = selection_stride(&surface, &selection);
                                dispatch_verb(
                                    &on_action,
                                    &surface,
                                    &selection,
                                    PatchVerbKind::Rotate { steps: -1, stride },
                                );
                            }
                            "'" => {
                                let stride = selection_stride(&surface, &selection);
                                dispatch_verb(
                                    &on_action,
                                    &surface,
                                    &selection,
                                    PatchVerbKind::Rotate { steps: 1, stride },
                                );
                            }
                            "s" => {
                                // Arm the swap from the selected port.
                                if let Some(UiPatchTarget::Port { node, port }) = &selection
                                    && let Some(output) =
                                        surface.outputs.iter().find(|output| output.node == *node)
                                    && let Some(window) = port_window(output, *port)
                                {
                                    armed_swap.set(Some(window));
                                }
                            }
                            "z" if meta => {
                                let verb = if event.modifiers().shift() {
                                    PatchVerbKind::Redo
                                } else {
                                    PatchVerbKind::Undo
                                };
                                dispatch_verb(&on_action, &surface, &selection, verb);
                            }
                            "?" => {
                                let open = *help_open.peek();
                                help_open.set(!open);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            },
            if *help_open.read() {
                div { class: "tw:fixed tw:bottom-4 tw:right-4 tw:z-50 tw:rounded-lg tw:border tw:border-white/20 tw:bg-black/90 tw:p-4 tw:text-xs tw:leading-relaxed",
                    div { class: "tw:font-semibold tw:mb-1", "Patch keys" }
                    div { "click port — assign selection · click cell — select" }
                    div { "r — reverse · ; / ' — rotate ∓/± stride" }
                    div { "s — arm swap (then click the other port)" }
                    div { "⌘Z / ⌘⇧Z — undo / redo · Esc — back out · ? — close" }
                }
            }
            if armed_swap.read().is_some() {
                div { class: "tw:fixed tw:top-4 tw:right-4 tw:z-50 tw:rounded tw:border tw:border-white/30 tw:bg-black/80 tw:px-3 tw:py-1.5 tw:text-xs",
                    "Swap armed — click the other port (Esc cancels)"
                }
            }
            PatchSidebar {
                surface: surface.clone(),
                selection: selection.clone(),
                on_action,
                on_port_click,
            }
            div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
                for output in surface.outputs.clone() {
                    PatchOutputSection {
                        key: "{output.node.0}",
                        output,
                        fixtures: surface.fixtures.clone(),
                        selection: selection.clone(),
                        on_action,
                    }
                }
                for fixture in surface.fixtures.clone() {
                    PatchFixtureRow {
                        key: "fixture-{fixture.node.0}",
                        fixture,
                        selection: selection.clone(),
                        on_action,
                    }
                }
            }
        }
    }
}

/// The output → port tree (D29v): rows select; status chips ride along.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchSidebar(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
    on_port_click: EventHandler<(usize, u32)>,
) -> Element {
    rsx! {
        nav { class: "tw:grid tw:content-start tw:gap-1 tw:text-sm",
            h2 { class: "tw:text-base tw:font-semibold tw:mb-1", "Patch" }
            for (output_index, output) in surface.outputs.clone().into_iter().enumerate() {
                {
                    let target = UiPatchTarget::Output { node: output.node };
                    let selected = is_selected(&selection, &target);
                    let row_class = if selected {
                        "tw:flex tw:items-center tw:gap-2 tw:rounded tw:px-2 tw:py-1 tw:cursor-pointer tw:bg-white/10"
                    } else {
                        "tw:flex tw:items-center tw:gap-2 tw:rounded tw:px-2 tw:py-1 tw:cursor-pointer hover:tw:bg-white/5"
                    };
                    let contested = output.bay.contested_lamps;
                    let gaps = output.bay.gap_lamps;
                    let name = output.display_name().to_string();
                    let on_action = on_action;
                    rsx! {
                        div {
                            class: "{row_class}",
                            onclick: move |_| select(&on_action, Some(target.clone())),
                            span { class: "tw:font-medium", "{name}" }
                            if contested > 0 {
                                span { class: "tw:text-xs tw:text-red-400", "{contested} contested" }
                            }
                            if gaps > 0 {
                                span { class: "tw:text-xs tw:opacity-60", "{gaps} dark" }
                            }
                        }
                        for port in output.bay.ports.clone() {
                            {
                                let target = UiPatchTarget::Port { node: output.node, port: port.key };
                                let selected = is_selected(&selection, &target);
                                let row_class = if selected {
                                    "tw:flex tw:items-baseline tw:gap-2 tw:rounded tw:pl-6 tw:pr-2 tw:py-0.5 tw:cursor-pointer tw:bg-white/10"
                                } else {
                                    "tw:flex tw:items-baseline tw:gap-2 tw:rounded tw:pl-6 tw:pr-2 tw:py-0.5 tw:cursor-pointer hover:tw:bg-white/5"
                                };
                                let _ = target;
                                rsx! {
                                    div {
                                        class: "{row_class}",
                                        onclick: move |_| on_port_click.call((output_index, port.key)),
                                        span { "port {port.key}" }
                                        span { class: "tw:text-xs tw:opacity-60", "{port.pin_label}" }
                                        span { class: "tw:text-xs tw:opacity-60", "{port.lamps} lamps" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One output's wire: the SVG lamp canvas plus its port strips of cells.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchOutputSection(
    output: UiPatchSurfaceOutput,
    fixtures: Vec<UiPatchSurfaceFixture>,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let name = output.display_name().to_string();
    rsx! {
        section { class: "tw:grid tw:gap-2 tw:rounded-lg tw:border tw:border-white/10 tw:p-3",
            header { class: "tw:flex tw:items-baseline tw:gap-2",
                h3 { class: "tw:font-semibold", "{name}" }
                span { class: "tw:text-xs tw:opacity-60", "{output.label}" }
            }
            PatchWireCanvas { output: output.clone(), fixtures, selection: selection.clone(), on_action }
            for port in output.bay.ports.clone() {
                div { class: "tw:grid tw:gap-1",
                    div { class: "tw:text-xs tw:opacity-60", "port {port.key} · {port.pin_label}" }
                    div { class: "tw:relative tw:flex tw:h-6 tw:w-full tw:overflow-hidden tw:rounded tw:bg-white/5",
                        for cell in port.cells.clone() {
                            {
                                let width = if port.lamps > 0 {
                                    (cell.lamps as f32 / port.lamps as f32 * 100.0).max(1.0)
                                } else {
                                    0.0
                                };
                                let left = if port.lamps > 0 {
                                    (cell.wire_start.saturating_sub(port.start)) as f32
                                        / port.lamps as f32
                                        * 100.0
                                } else {
                                    0.0
                                };
                                let target = UiPatchTarget::Cell { id: cell.id.clone() };
                                let selected = is_selected(&selection, &target);
                                let cell_class = if cell.contested {
                                    "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:border tw:border-red-500 tw:bg-red-500/40"
                                } else if selected {
                                    "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:border tw:border-white tw:bg-white/30"
                                } else {
                                    "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:border tw:border-white/30 tw:bg-white/15 hover:tw:bg-white/25"
                                };
                                let title = format!(
                                    "{} {}–{}{}",
                                    cell.producer,
                                    cell.source_start,
                                    cell.source_end().unwrap_or(cell.source_start),
                                    if cell.reversed { " ◀" } else { "" },
                                );
                                let on_action = on_action;
                                let hover = use_context::<HoveredPatchCell>();
                                let cell_id = cell.id.clone();
                                let cell_id_out = cell.id.clone();
                                rsx! {
                                    div {
                                        class: "{cell_class}",
                                        style: "left: {left}%; width: {width}%;",
                                        title: "{title}",
                                        onclick: move |_| select(&on_action, Some(target.clone())),
                                        onmouseenter: {
                                            let mut hover = hover;
                                            move |_| hover.0.set(Some(cell_id.clone()))
                                        },
                                        onmouseleave: {
                                            let mut hover = hover;
                                            move |_| {
                                                if hover.0.peek().as_deref() == Some(cell_id_out.as_str()) {
                                                    hover.0.set(None);
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The output's lamps as SVG dots, colored from its published frame, with
/// per-lamp instance hit targets resolved through placements + instances.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchWireCanvas(
    output: UiPatchSurfaceOutput,
    fixtures: Vec<UiPatchSurfaceFixture>,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let Some(frame) = output.bay.frame.clone() else {
        return rsx! {
            div { class: "tw:text-xs tw:opacity-50", "no frame yet" }
        };
    };
    let Some(ControlDisplayLayout::Layout2d(layout)) = frame.display_layout.as_deref().cloned()
    else {
        return rsx! {
            div { class: "tw:text-xs tw:opacity-50", "no geometry for this wire" }
        };
    };
    // Wire lamp → owning cell (for hit-testing and selection highlight).
    let cells: Vec<(u32, u32, String)> = output
        .bay
        .ports
        .iter()
        .flat_map(|port| port.cells.iter())
        .map(|cell| (cell.wire_start, cell.lamps, cell.id.clone()))
        .collect();
    // Selected instance's wire lamps, when the selection is an instance of
    // a fixture with runs on this wire: highlight rings ride the canvas.
    let selected_cell = match &selection {
        Some(UiPatchTarget::Cell { id }) => Some(id.clone()),
        _ => None,
    };
    let bytes = frame.bytes.clone();
    rsx! {
        svg {
            class: "tw:w-full tw:rounded tw:bg-black/40",
            view_box: "0 0 1 1",
            preserve_aspect_ratio: "xMidYMid meet",
            style: "aspect-ratio: 1;",
            for lamp in layout.lamps.clone() {
                {
                    let cell = cells
                        .iter()
                        .find(|(start, lamps, _)| {
                            let lamp_index = lamp.sample_start / 3;
                            lamp_index >= *start && lamp_index < start + lamps
                        })
                        .map(|(_, _, id)| id.clone());
                    let rgb = lamp_rgb(&bytes, lamp.sample_start);
                    let fill = format!("rgb({},{},{})", rgb[0], rgb[1], rgb[2]);
                    let selected = cell.is_some() && cell == selected_cell;
                    let stroke = if selected { "white" } else { "none" };
                    let on_action = on_action;
                    rsx! {
                        circle {
                            cx: "{lamp.center[0]}",
                            cy: "{lamp.center[1]}",
                            r: if selected { "0.012" } else { "0.008" },
                            fill: "{fill}",
                            stroke: "{stroke}",
                            stroke_width: "0.003",
                            onclick: move |_| {
                                if let Some(id) = cell.clone() {
                                    select(&on_action, Some(UiPatchTarget::Cell { id }));
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// One fixture's row: its instance chips (the `/sector/2` grain) or, for a
/// range-grain fixture (the peach), its runs — plus the whole-fixture chip.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatchFixtureRow(
    fixture: UiPatchSurfaceFixture,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let fixture_target = UiPatchTarget::Fixture { node: fixture.node };
    let fixture_selected = is_selected(&selection, &fixture_target);
    let name_class = if fixture_selected {
        "tw:font-semibold tw:cursor-pointer tw:underline"
    } else {
        "tw:font-semibold tw:cursor-pointer hover:tw:underline"
    };
    rsx! {
        section { class: "tw:grid tw:gap-2 tw:rounded-lg tw:border tw:border-white/10 tw:p-3",
            header { class: "tw:flex tw:items-baseline tw:gap-2",
                {
                    let on_action = on_action;
                    let target = fixture_target.clone();
                    rsx! {
                        h3 {
                            class: "{name_class}",
                            onclick: move |_| select(&on_action, Some(target.clone())),
                            "{fixture.label}"
                        }
                    }
                }
                span { class: "tw:text-xs tw:opacity-60", "{fixture.patch.lamps} lamps" }
                if fixture.instances.is_empty() {
                    span { class: "tw:text-xs tw:opacity-60", "range grain" }
                }
            }
            if !fixture.instances.is_empty() {
                div { class: "tw:flex tw:flex-wrap tw:gap-1.5",
                    for instance in fixture.instances.clone() {
                        {
                            let target = UiPatchTarget::Instance {
                                node: fixture.node,
                                path: instance.path.clone(),
                            };
                            let selected = is_selected(&selection, &target);
                            let chip_class = if selected {
                                "tw:rounded tw:border tw:border-white tw:bg-white/20 tw:px-2 tw:py-0.5 tw:text-xs tw:cursor-pointer"
                            } else {
                                "tw:rounded tw:border tw:border-white/25 tw:px-2 tw:py-0.5 tw:text-xs tw:cursor-pointer hover:tw:bg-white/10"
                            };
                            let title = format!(
                                "{} · lamps {}–{} · stride {}",
                                instance.path,
                                instance.start,
                                instance.start + instance.lamps.saturating_sub(1),
                                instance.stride,
                            );
                            let on_action = on_action;
                            rsx! {
                                span {
                                    class: "{chip_class}",
                                    title: "{title}",
                                    onclick: move |_| select(&on_action, Some(target.clone())),
                                    "{instance.label}"
                                }
                            }
                        }
                    }
                }
            }
            // The fixture's own runs, laid along ITS channel space — the
            // fixture side of the two-sided bay, with the same twin ids.
            div { class: "tw:relative tw:flex tw:h-5 tw:w-full tw:overflow-hidden tw:rounded tw:bg-white/5",
                for cell in fixture.patch.cells.clone() {
                    {
                        let lamps = fixture.patch.lamps.max(1);
                        let width = (cell.lamps as f32 / lamps as f32 * 100.0).max(1.0);
                        let left = cell.source_start as f32 / lamps as f32 * 100.0;
                        let target = UiPatchTarget::Cell { id: cell.id.clone() };
                        let selected = is_selected(&selection, &target);
                        let cell_class = if cell.contested {
                            "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:border tw:border-red-500 tw:bg-red-500/40"
                        } else if selected {
                            "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:border tw:border-white tw:bg-white/30"
                        } else {
                            "tw:absolute tw:top-0 tw:h-full tw:cursor-pointer tw:border tw:border-white/30 tw:bg-white/15 hover:tw:bg-white/25"
                        };
                        let title = format!(
                            "→ {} lamp {}{}",
                            cell.output_label,
                            cell.wire_start,
                            if cell.reversed { " ◀" } else { "" },
                        );
                        let on_action = on_action;
                        rsx! {
                            div {
                                class: "{cell_class}",
                                style: "left: {left}%; width: {width}%;",
                                title: "{title}",
                                onclick: move |_| select(&on_action, Some(target.clone())),
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One lamp's RGB out of the frame bytes (u16 LE samples → 8-bit).
fn lamp_rgb(bytes: &[u8], sample_start: u32) -> [u8; 3] {
    let base = sample_start as usize * 2;
    let mut rgb = [0u8; 3];
    for (channel, value) in rgb.iter_mut().enumerate() {
        let offset = base + channel * 2;
        if offset + 1 < bytes.len() {
            *value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]).to_be_bytes()[0];
        }
    }
    rgb
}
