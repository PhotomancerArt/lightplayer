//! THE patch panel (pass 2, P4) — the Patching view's bottom region:
//! OBJECT section over OUTPUT section, one selection, the counterpart
//! following.
//!
//! Design record: `spikes/patching-controls/index.html` §1 (states
//! `#armed` / `#paired` / `#derived` / `#objfirst`). The panel derives
//! EVERYTHING from the one core-owned selection plus the patch-surface
//! DTOs — it holds no selection state of its own, so the canvas, the
//! docks and the panel can never disagree about what is selected.
//!
//! Which section is PRIMARY (the blue edge) follows core's D9 matrix
//! ([`UiPatchTarget::pulse_space`]): a fixture-side selection lights the
//! object section, a wire-side one the output section. Reading the space
//! rather than re-deciding here keeps the panel's emphasis and the lamps'
//! light language answering to the same authority.
//!
//! The section WITHOUT a counterpart shows the invitation: the assign arm
//! (`a`) plus an inline picker that direct-assigns (pickers write — the
//! ratified mobile path). Everything else in the panel is either a
//! selection nudge or one of the existing verbs; the panel never opens a
//! second write path.
//!
//! Strips are PLACEHOLDERS here (containers + honest static geometry) —
//! P5 fills them with live lamps and the chase/breath languages.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeId, PatchVerbKind, PatchVerbWindow, ProjectEditorOp, UiAction, UiPatchCell, UiPatchSurface,
    UiPatchSurfaceFixture, UiPatchTarget,
};

use crate::app::editor_shell::patching::{ArmedVerb, PatchingUi, arm_assign, arm_swap};
use crate::app::patch::verb_ui::{
    dispatch_assign, dispatch_verb, free_runs, instance_target, next_free_segment, port_next_free,
    resize_segment, segment_at_free_run, selection_stride, shift_segment, target_is_unmapped,
};

/// Stepped controls are squared blocks (the panel-language convention) —
/// every transport button steps something discrete.
const STEP: &str = "tw:cursor-pointer tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-2 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:leading-4 tw:text-muted-foreground tw:hover:text-strong-foreground";
/// The same block, refusing: a verb with nothing to act on.
const STEP_OFF: &str = "tw:rounded-sm tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:leading-4 tw:text-dim-foreground tw:opacity-60";
/// The invitation's own block — the one action a walk-up user is meant to
/// find, so it wears the accent.
const STEP_ARM: &str = "tw:cursor-pointer tw:rounded-sm tw:border tw:border-accent-border tw:bg-card-raised tw:px-2 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:leading-4 tw:text-accent tw:hover:text-strong-foreground";
/// Armed: the same block, wearing the selection language the armed strip
/// and the pulsing lamps wear.
const STEP_ARMED: &str = "tw:cursor-pointer tw:rounded-sm tw:border tw:border-selection-border tw:bg-selection-bg tw:px-2 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:leading-4 tw:text-selection-border";
/// Mock-level room for the lamps−/+ mutation (plan: dashed, disabled, no
/// handler — the count edit belongs to a later pass).
const STEP_FUTURE: &str = "tw:rounded-sm tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:px-2 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:leading-4 tw:text-dim-foreground";
const PICKER: &str = "tw:min-w-0 tw:max-w-[210px] tw:cursor-pointer tw:rounded-sm tw:border tw:border-border-strong tw:bg-terminal tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:text-muted-foreground";
const SECTION: &str = "tw:grid tw:content-start tw:gap-1.5 tw:border-l-2 tw:border-l-transparent tw:px-2.5 tw:py-2 tw:max-[820px]:px-2 tw:max-[820px]:py-1.5";
const SECTION_PRIMARY: &str = "tw:grid tw:content-start tw:gap-1.5 tw:border-l-2 tw:border-l-selection-border tw:bg-selection-bg tw:px-2.5 tw:py-2 tw:max-[820px]:px-2 tw:max-[820px]:py-1.5";
const PROMPT: &str = "tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:px-2.5 tw:py-1.5 tw:text-[11.5px] tw:text-subtle-foreground";
const STRIP: &str =
    "tw:relative tw:h-5 tw:overflow-hidden tw:rounded tw:bg-track tw:max-[820px]:h-4";

/// Which half of the panel the selection itself is — the blue edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelSide {
    Object,
    Output,
}

/// The section the selection lives in, from core's D9 space matrix (a
/// module — or nothing — selects neither side).
pub(crate) fn primary_side(selection: Option<&UiPatchTarget>) -> Option<PanelSide> {
    match selection?.pulse_space()? {
        lpa_studio_core::PatchPulseSpace::Fixture => Some(PanelSide::Object),
        lpa_studio_core::PatchPulseSpace::Wire => Some(PanelSide::Output),
    }
}

/// The OBJECT half, derived from the selection: the thing the fixture-side
/// verbs act on, whether it was named directly or through the cell that
/// carries it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ObjectView {
    /// The verb subject — the selection itself for a fixture-side pick,
    /// the owner cell/instance when the panel derived it.
    pub target: UiPatchTarget,
    /// The object's own name (`sector 2`).
    pub name: String,
    /// Where it lives (`dome · /sector/2`) — the fixture path line.
    pub context: String,
    pub lamps: u32,
    pub mapped: bool,
    pub reversed: bool,
}

/// The OUTPUT half: the wire this selection is about — picked directly, or
/// derived from the object's own runs.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OutputView {
    pub node: NodeId,
    /// The output's display name.
    pub name: String,
    /// The port in play, when one is (the picker's value).
    pub port: Option<u32>,
    /// The port's board label (`IO18`), empty when the wire names no pin.
    pub pin: String,
    /// Wire lamps of the port (or the whole output when no port is named).
    pub used: u32,
    pub total: u32,
    /// First wire lamp / lamp count of the port's span, for the strip.
    pub span: (u32, u32),
    /// The window inside that span the selection is about.
    pub window: Option<(u32, u32)>,
    /// The window is FREE space — the nudges resize it and the arm can
    /// spend it. False = a mapped run (the shift verb moves it instead).
    pub free: bool,
    /// The window was derived from a fixture-side selection.
    pub derived: bool,
}

fn fixture_of<'a>(surface: &'a UiPatchSurface, node: NodeId) -> Option<&'a UiPatchSurfaceFixture> {
    surface.fixtures.iter().find(|fixture| fixture.node == node)
}

/// The fixture (and cell) a bay cell id belongs to.
fn cell_owner<'a>(
    surface: &'a UiPatchSurface,
    id: &str,
) -> Option<(&'a UiPatchSurfaceFixture, &'a UiPatchCell)> {
    surface.fixtures.iter().find_map(|fixture| {
        fixture
            .patch
            .cells
            .iter()
            .find(|cell| cell.id == id)
            .map(|cell| (fixture, cell))
    })
}

/// An object's lamp range in its fixture's OWN numbering — what the cells
/// overlapping it are looked up by.
fn object_source_range(
    fixture: &UiPatchSurfaceFixture,
    target: &UiPatchTarget,
) -> Option<(u32, u32)> {
    match target {
        UiPatchTarget::Fixture { .. } => Some((0, fixture.patch.lamps)),
        UiPatchTarget::Instance { path, .. } => fixture
            .instances
            .iter()
            .find(|instance| instance.path == *path)
            .map(|instance| (instance.start, instance.lamps)),
        UiPatchTarget::Range { start, count, .. } => Some((
            *start,
            count.unwrap_or_else(|| fixture.patch.lamps.saturating_sub(*start)),
        )),
        UiPatchTarget::Cell { id } => fixture
            .patch
            .cells
            .iter()
            .find(|cell| cell.id == *id)
            .map(|cell| (cell.source_start, cell.lamps)),
        _ => None,
    }
}

/// The cells covering a source range — an object's wire runs.
fn cells_over<'a>(fixture: &'a UiPatchSurfaceFixture, range: (u32, u32)) -> Vec<&'a UiPatchCell> {
    let (start, lamps) = range;
    let end = start.saturating_add(lamps);
    fixture
        .patch
        .cells
        .iter()
        .filter(|cell| cell.source_start < end && cell.source_start + cell.lamps > start)
        .collect()
}

/// What the object section shows for this selection, or `None` when the
/// selection names no object (a free segment, a port, a module, nothing).
pub(crate) fn object_view(
    surface: &UiPatchSurface,
    selection: Option<&UiPatchTarget>,
) -> Option<ObjectView> {
    let target = selection?;
    let (fixture, name, context, lamps) = match target {
        UiPatchTarget::Fixture { node } => {
            let fixture = fixture_of(surface, *node)?;
            (
                fixture,
                fixture.label.clone(),
                "whole fixture".to_string(),
                fixture.patch.lamps,
            )
        }
        UiPatchTarget::Instance { node, path } => {
            let fixture = fixture_of(surface, *node)?;
            let instance = fixture
                .instances
                .iter()
                .find(|instance| instance.path == *path)?;
            (
                fixture,
                instance.label.clone(),
                format!("{} · {}", fixture.label, instance.path),
                instance.lamps,
            )
        }
        UiPatchTarget::Range { node, start, count } => {
            let fixture = fixture_of(surface, *node)?;
            let lamps = count.unwrap_or_else(|| fixture.patch.lamps.saturating_sub(*start));
            (
                fixture,
                format!("lamps {}-{}", start + 1, start.saturating_add(lamps)),
                fixture.label.clone(),
                lamps,
            )
        }
        // A mapped run selects as its cell (D9) — the panel shows the
        // OWNER object, which is what the verbs act on anyway.
        UiPatchTarget::Cell { id } => {
            let (fixture, cell) = cell_owner(surface, id)?;
            let instance = fixture.instances.iter().find(|instance| {
                cell.source_start >= instance.start
                    && cell.source_start < instance.start + instance.lamps
            });
            let (name, context) = match instance {
                Some(instance) => (
                    instance.label.clone(),
                    format!("{} · {}", fixture.label, instance.path),
                ),
                None => (fixture.label.clone(), "whole fixture".to_string()),
            };
            (fixture, name, context, cell.lamps)
        }
        _ => return None,
    };
    let range = object_source_range(fixture, target).unwrap_or((0, lamps));
    let cells = cells_over(fixture, range);
    Some(ObjectView {
        mapped: !target_is_unmapped(surface, target),
        reversed: cells.iter().any(|cell| cell.reversed),
        target: target.clone(),
        name,
        context,
        lamps,
    })
}

/// One output+port lookup, with the port's own facts.
fn port_view(
    surface: &UiPatchSurface,
    node: NodeId,
    port_key: u32,
    window: Option<(u32, u32)>,
    free: bool,
    derived: bool,
) -> Option<OutputView> {
    let output = surface.outputs.iter().find(|output| output.node == node)?;
    let port = output.bay.ports.iter().find(|port| port.key == port_key)?;
    Some(OutputView {
        node,
        name: output.display_name().to_string(),
        port: Some(port_key),
        pin: port.pin_label.clone(),
        used: port.cells.iter().map(|cell| cell.lamps).sum(),
        total: port.lamps,
        span: (port.start, port.lamps),
        window,
        free,
        derived,
    })
}

/// What the output section shows for this selection: the wire the user
/// picked, or the one the selected object already sits on. `None` = the
/// section has no counterpart and invites instead.
pub(crate) fn output_view(
    surface: &UiPatchSurface,
    selection: Option<&UiPatchTarget>,
) -> Option<OutputView> {
    match selection? {
        UiPatchTarget::Output { node } => {
            let output = surface.outputs.iter().find(|output| output.node == *node)?;
            let used: u32 = output
                .bay
                .ports
                .iter()
                .flat_map(|port| port.cells.iter())
                .map(|cell| cell.lamps)
                .sum();
            let total: u32 = output.bay.ports.iter().map(|port| port.lamps).sum();
            let start = output.bay.ports.first().map_or(0, |port| port.start);
            Some(OutputView {
                node: *node,
                name: output.display_name().to_string(),
                port: None,
                pin: String::new(),
                used,
                total,
                span: (start, total),
                window: None,
                free: false,
                derived: false,
            })
        }
        UiPatchTarget::Port { node, port } => port_view(surface, *node, *port, None, false, false),
        UiPatchTarget::Segment {
            node,
            port,
            start,
            lamps,
        } => {
            // A `Segment` only ever names free space (a mapped run is its
            // cell) — but a run can land under it between snapshots, so the
            // freeness is READ from the port rather than assumed.
            let output = surface.outputs.iter().find(|output| output.node == *node)?;
            let entry = output.bay.ports.iter().find(|entry| entry.key == *port)?;
            let end = start.saturating_add(*lamps);
            let taken = entry
                .cells
                .iter()
                .any(|cell| cell.wire_start < end && cell.wire_start + cell.lamps > *start);
            port_view(surface, *node, *port, Some((*start, *lamps)), !taken, false)
        }
        // Fixture-side: the object's own runs name the wire (the spike's
        // "derived from object" window).
        target => {
            let node = match target {
                UiPatchTarget::Fixture { node }
                | UiPatchTarget::Instance { node, .. }
                | UiPatchTarget::Range { node, .. } => *node,
                UiPatchTarget::Cell { id } => cell_owner(surface, id)?.0.node,
                _ => return None,
            };
            let fixture = fixture_of(surface, node)?;
            let range = object_source_range(fixture, target)?;
            let ids: Vec<&str> = cells_over(fixture, range)
                .into_iter()
                .map(|cell| cell.id.as_str())
                .collect();
            if ids.is_empty() {
                return None;
            }
            // The object's window on the FIRST port its runs reach: the
            // panel shows one wire at a time, and a split object's other
            // pieces stay honest in the Outputs dock.
            let (output, port, first) = surface.outputs.iter().find_map(|output| {
                output.bay.ports.iter().find_map(|port| {
                    port.cells
                        .iter()
                        .find(|cell| ids.contains(&cell.id.as_str()))
                        .map(|cell| (output, port, cell))
                })
            })?;
            let mut start = first.wire_start;
            let mut end = first.wire_start.saturating_add(first.lamps);
            for cell in port
                .cells
                .iter()
                .filter(|cell| ids.contains(&cell.id.as_str()))
            {
                start = start.min(cell.wire_start);
                end = end.max(cell.wire_start.saturating_add(cell.lamps));
            }
            port_view(
                surface,
                output.node,
                port.key,
                Some((start, end.saturating_sub(start))),
                false,
                true,
            )
        }
    }
}

/// The object section's facts, in the fact-card idiom (label + mono value).
pub(crate) fn object_facts(object: &ObjectView) -> Vec<(String, String)> {
    vec![
        ("lamps".to_string(), object.lamps.to_string()),
        (
            "wire".to_string(),
            if object.mapped {
                if object.reversed {
                    "mapped · reversed".to_string()
                } else {
                    "mapped · forward".to_string()
                }
            } else {
                "unmapped".to_string()
            },
        ),
    ]
}

/// The output section's facts: the pin and its occupancy, plus the window
/// the selection is about (1-based spans, the chips' own convention).
pub(crate) fn output_facts(output: &OutputView) -> Vec<(String, String)> {
    let mut facts = Vec::new();
    if !output.pin.is_empty() {
        facts.push(("pin".to_string(), output.pin.clone()));
    }
    facts.push((
        "lamps".to_string(),
        format!("{}/{} used", output.used, output.total),
    ));
    if let Some((start, lamps)) = output.window {
        facts.push((
            "window".to_string(),
            format!(
                "{}-{}{}",
                start + 1,
                start.saturating_add(lamps),
                if output.derived { " · derived" } else { "" }
            ),
        ));
    }
    facts
}

/// Every object still waiting for a wire, in surface order — the object
/// side's inline picker (direct-assign).
pub(crate) fn unmapped_objects(surface: &UiPatchSurface) -> Vec<(UiPatchTarget, String)> {
    let mut rows = Vec::new();
    for fixture in &surface.fixtures {
        if fixture.instances.is_empty() {
            if fixture.patch.cells.is_empty() && fixture.patch.lamps > 0 {
                rows.push((
                    UiPatchTarget::Fixture { node: fixture.node },
                    format!("{} · {}", fixture.label, fixture.patch.lamps),
                ));
            }
            continue;
        }
        for instance in fixture.instances.iter().filter(|entry| !entry.placed) {
            rows.push((
                instance_target(fixture.node, instance),
                format!("{} · {}", instance.label, instance.lamps),
            ));
        }
    }
    rows
}

/// Every port on the surface as picker rows — the output side's inline
/// picker. Value keys are `node:port`, parsed back by [`parse_port_key`].
pub(crate) fn port_options(surface: &UiPatchSurface) -> Vec<(String, String)> {
    surface
        .outputs
        .iter()
        .flat_map(|output| {
            output.bay.ports.iter().map(move |port| {
                let pin = if port.pin_label.is_empty() {
                    format!("port {}", port.key)
                } else {
                    format!("{} · port {}", port.pin_label, port.key)
                };
                (
                    format!("{}:{}", output.node.0, port.key),
                    format!("{} · {pin}", output.display_name()),
                )
            })
        })
        .collect()
}

/// `node:port` back into its parts.
pub(crate) fn parse_port_key(value: &str) -> Option<(NodeId, u32)> {
    let (node, port) = value.split_once(':')?;
    Some((NodeId::new(node.parse().ok()?), port.parse().ok()?))
}

fn select(on_action: &EventHandler<UiAction>, target: Option<UiPatchTarget>) {
    on_action.call(UiAction::from_op(
        lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
        ProjectEditorOp::PatchSelect { target },
    ));
}

/// THE patch panel: the Patching center's bottom region (D8 — always
/// present, empty states included).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PatchPanel(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
    /// The frame's armed verb, read in the center and passed down so the
    /// panel stays plain data (and stories can pose an armed state).
    #[props(default)]
    armed: Option<ArmedVerb>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let object = object_view(&surface, selection.as_ref());
    let output = output_view(&surface, selection.as_ref());
    let primary = primary_side(selection.as_ref());
    rsx! {
        // Capped and self-scrolling: the panel is the center's bottom
        // region, and it must never squeeze the canvas above it to nothing
        // (the canvas re-fits on every resize — starving it would churn the
        // fit reconciliation the story capture gates on).
        div { class: "tw:flex tw:max-h-[45%] tw:flex-none tw:flex-col tw:overflow-y-auto tw:border-t tw:border-border-subtle tw:bg-card-subtle",
            // The armed state, absorbing P3's standalone banner: the panel
            // is where the walk-up user is looking, so the arm names itself
            // here (and the invitation buttons below echo it).
            if let Some(verb) = armed.as_ref() {
                div { class: "tw:flex-none tw:border-b tw:border-border-subtle tw:bg-selection-bg tw:px-2.5 tw:py-1 tw:text-[11px] tw:text-selection-border",
                    "{verb.banner()}"
                }
            }
            ObjectPane {
                surface: surface.clone(),
                selection: selection.clone(),
                object,
                armed: armed.clone(),
                primary: primary == Some(PanelSide::Object),
                on_action,
            }
            OutputPane {
                surface: surface.clone(),
                selection: selection.clone(),
                output,
                armed,
                primary: primary == Some(PanelSide::Output),
                on_action,
            }
            // The keys row REPLACES the help overlay: one line, always
            // visible, in the panel the gesture happens in.
            div { class: "tw:flex tw:flex-none tw:flex-wrap tw:gap-x-3.5 tw:gap-y-0.5 tw:border-t tw:border-border-subtle tw:bg-card-muted tw:px-2.5 tw:py-1 tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                span {
                    b { class: "tw:text-subtle-foreground", "a" }
                    " assign"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "m" }
                    " next free"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "[ ]" }
                    " shift"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "- =" }
                    " narrow / widen"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "r" }
                    " flip"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "; '" }
                    " rotate"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "s" }
                    " swap"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "⌘Z" }
                    " undo"
                }
                span {
                    b { class: "tw:text-subtle-foreground", "esc" }
                    " disarm, then deselect"
                }
            }
        }
    }
}

/// One section head: label · name · context · facts · × (the × belongs to
/// the section that IS the selection — deselecting from a derived
/// counterpart would be a different gesture than it looks).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SectionHead(
    label: &'static str,
    name: String,
    context: String,
    facts: Vec<(String, String)>,
    deselect: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let ui = use_hook(try_consume_context::<PatchingUi>);
    rsx! {
        // Below the fold the head WRAPS rather than hiding facts: the
        // walk-up phone still needs the lamp count it is about to commit.
        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-2 tw:gap-y-0.5",
            span { class: "tw:w-12 tw:flex-none tw:font-mono tw:text-[9.5px] tw:uppercase tw:tracking-[0.13em] tw:text-dim-foreground",
                "{label}"
            }
            span { class: "tw:truncate tw:text-[12.5px] tw:font-medium tw:text-strong-foreground", "{name}" }
            span { class: "tw:truncate tw:font-mono tw:text-[10px] tw:text-dim-foreground", "{context}" }
            span { class: "tw:ml-auto tw:flex tw:flex-none tw:items-baseline tw:gap-2",
                for (fact_label , value) in facts {
                    span { class: "tw:flex tw:items-baseline tw:gap-1",
                        span { class: "tw:text-[10px] tw:text-dim-foreground", "{fact_label}" }
                        span { class: "tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground", "{value}" }
                    }
                }
            }
            if deselect {
                button {
                    class: "tw:ml-1 tw:flex-none tw:cursor-pointer tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-1 tw:text-[11px] tw:leading-4 tw:text-dim-foreground tw:hover:text-strong-foreground",
                    title: "Deselect (esc)",
                    onclick: move |_| {
                        // Same rung as esc's clear: the size override
                        // belongs to the selection it was nudged on.
                        if let Some(ui) = ui {
                            let mut size = ui.segment_size;
                            size.set(None);
                        }
                        select(&on_action, None);
                    },
                    "×"
                }
            }
        }
    }
}

/// The OBJECT section: the selected (or derived) object, its strip, and the
/// fixture-side transport — or the invitation when a free segment is
/// waiting for one.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ObjectPane(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
    object: Option<ObjectView>,
    armed: Option<ArmedVerb>,
    primary: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let ui = use_hook(try_consume_context::<PatchingUi>);
    let class = if primary { SECTION_PRIMARY } else { SECTION };
    let is_armed = matches!(armed, Some(ArmedVerb::Assign));
    // The invitation belongs to the object side when the WIRE side holds a
    // free segment: that is the pairing this panel can still make.
    let free_segment = match (&selection, &object) {
        (Some(UiPatchTarget::Segment { node, start, .. }), None) => Some((*node, *start)),
        _ => None,
    };
    // A port or output selection has no object either — but nothing to
    // invite, so it says what WOULD name one.
    let wire_hint = free_segment.is_none() && selection.is_some();
    rsx! {
        section { class: "{class}",
            match object {
                Some(object) => {
                    let facts = object_facts(&object);
                    let target = Some(object.target.clone());
                    let stride = selection_stride(&surface, &target);
                    let mapped = object.mapped;
                    let verb = {
                        let surface = surface.clone();
                        let target = target.clone();
                        move |kind: PatchVerbKind| {
                            dispatch_verb(&on_action, &surface, &target, kind);
                        }
                    };
                    rsx! {
                        SectionHead {
                            label: "object",
                            name: object.name.clone(),
                            context: object.context.clone(),
                            facts,
                            deselect: primary,
                            on_action,
                        }
                        // Strip container — P5 paints the chase in the
                        // lamps here; today it carries the object's honest
                        // extent and nothing it does not know.
                        div { class: "{STRIP}", "data-patch-strip": "object",
                            div { class: "tw:absolute tw:inset-y-[3px] tw:left-[2px] tw:right-[2px] tw:rounded-[3px] tw:bg-border-muted" }
                        }
                        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5",
                            button {
                                class: if mapped { STEP } else { STEP_OFF },
                                disabled: !mapped,
                                title: "Rotate one stride back (;)",
                                onclick: {
                                    let verb = verb.clone();
                                    move |_| verb(PatchVerbKind::Rotate { steps: -1, stride })
                                },
                                "‹ ;"
                            }
                            button {
                                class: if mapped { STEP } else { STEP_OFF },
                                disabled: !mapped,
                                title: "Rotate one stride forward (')",
                                onclick: {
                                    let verb = verb.clone();
                                    move |_| verb(PatchVerbKind::Rotate { steps: 1, stride })
                                },
                                "› '"
                            }
                            button {
                                class: if mapped { STEP } else { STEP_OFF },
                                disabled: !mapped,
                                title: "Reverse the wire direction (r)",
                                onclick: {
                                    let verb = verb.clone();
                                    move |_| verb(PatchVerbKind::Reverse)
                                },
                                "flip r"
                            }
                            button {
                                class: if mapped { STEP } else { STEP_OFF },
                                disabled: !mapped,
                                title: "Take this object off the wire",
                                onclick: {
                                    let verb = verb.clone();
                                    move |_| verb(PatchVerbKind::Clear)
                                },
                                "unmap"
                            }
                            span { class: "tw:w-2" }
                            // Mock-level room only (plan): the lamp-count
                            // edit is a mapping write, not a patch verb.
                            button {
                                class: "{STEP_FUTURE}",
                                disabled: true,
                                title: "future — the count is authored in Mapping",
                                "lamps −"
                            }
                            button {
                                class: "{STEP_FUTURE}",
                                disabled: true,
                                title: "future — the count is authored in Mapping",
                                "lamps +"
                            }
                        }
                    }
                }
                None => {
                    match free_segment {
                        // The invitation (#armed / segment-first): the arm,
                        // and a picker that writes the same link.
                        Some((node, start)) => {
                            let options = unmapped_objects(&surface);
                            let picker_targets: Vec<UiPatchTarget> = options
                                .iter()
                                .map(|(target, _)| target.clone())
                                .collect();
                            rsx! {
                                SectionHead {
                                    label: "object",
                                    name: "—".to_string(),
                                    context: String::new(),
                                    facts: Vec::new(),
                                    deselect: false,
                                    on_action,
                                }
                                div { class: "{PROMPT}",
                                    span { "Not mapped to any object." }
                                    button {
                                        class: if is_armed { STEP_ARMED } else { STEP_ARM },
                                        title: "Arm the assign — the next object click links it (a)",
                                        onclick: {
                                            let surface = surface.clone();
                                            let selection = selection.clone();
                                            move |_| {
                                                if let Some(ui) = ui {
                                                    let mut armed = ui.armed;
                                                    arm_assign(&surface, &selection, &mut armed);
                                                }
                                            }
                                        },
                                        if is_armed {
                                            "armed — click the object"
                                        } else {
                                            "assign a"
                                        }
                                    }
                                    select {
                                        class: "{PICKER}",
                                        value: "",
                                        onchange: {
                                            let surface = surface.clone();
                                            move |event: FormEvent| {
                                                let Ok(index) = event.value().parse::<usize>() else {
                                                    return;
                                                };
                                                let Some(target) = picker_targets.get(index) else {
                                                    return;
                                                };
                                                let Some(output) = surface
                                                    .outputs
                                                    .iter()
                                                    .find(|output| output.node == node)
                                                else {
                                                    return;
                                                };
                                                // Pickers write (ratified): the same
                                                // one-step verb the armed click
                                                // dispatches.
                                                if dispatch_assign(
                                                        &on_action,
                                                        &surface,
                                                        target,
                                                        output,
                                                        start,
                                                    ) && let Some(ui) = ui
                                                {
                                                    let mut armed = ui.armed;
                                                    let mut size = ui.segment_size;
                                                    armed.set(None);
                                                    size.set(None);
                                                }
                                            }
                                        },
                                        option { value: "", "or pick…" }
                                        for (index , (_ , label)) in options.iter().enumerate() {
                                            option { key: "{index}", value: "{index}", "{label}" }
                                        }
                                    }
                                }
                            }
                        }
                        None => rsx! {
                            SectionHead {
                                label: "object",
                                name: "—".to_string(),
                                context: String::new(),
                                facts: Vec::new(),
                                deselect: false,
                                on_action,
                            }
                            div { class: "{PROMPT} tw:opacity-70",
                                if wire_hint {
                                    "No object on this wire — click free port space to take a segment."
                                } else {
                                    "Nothing selected — click an object, or free space on a port."
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

/// The OUTPUT section: the wire in play, its strip and window, the wire-side
/// transport — or the invitation when an unmapped object is waiting.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OutputPane(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
    output: Option<OutputView>,
    armed: Option<ArmedVerb>,
    primary: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let ui = use_hook(try_consume_context::<PatchingUi>);
    let class = if primary { SECTION_PRIMARY } else { SECTION };
    let is_armed = matches!(armed, Some(ArmedVerb::Assign));
    let swap_armed = matches!(armed, Some(ArmedVerb::Swap(_)));
    // The object side holds an UNMAPPED object: this section invites.
    let unmapped_object = match (&selection, &output) {
        (Some(target), None) if target_is_unmapped(&surface, target) => Some(target.clone()),
        _ => None,
    };
    let ports = port_options(&surface);
    rsx! {
        section { class: "{class}",
            match output {
                Some(output) => {
                    let facts = output_facts(&output);
                    let (span_start, span_lamps) = output.span;
                    let lamps = span_lamps.max(1);
                    let window_style = output
                        .window
                        .map(|(start, count)| {
                            let left = start.saturating_sub(span_start) as f32 / lamps as f32
                                * 100.0;
                            let width = (count as f32 / lamps as f32 * 100.0).max(1.0);
                            format!("left: {left}%; width: {width}%;")
                        });
                    let free = output.free;
                    let selected_port = output
                        .port
                        .map(|key| format!("{}:{key}", output.node.0))
                        .unwrap_or_default();
                    let object_target = object_view(&surface, selection.as_ref())
                        .map(|object| object.target);
                    let shift_window = output
                        .window
                        .filter(|_| !free)
                        .map(|(start, count)| PatchVerbWindow {
                            output_name: surface
                                .outputs
                                .iter()
                                .find(|entry| entry.node == output.node)
                                .and_then(|entry| entry.name.clone()),
                            start,
                            lamps: count,
                        });
                    let nudge = {
                        let surface = surface.clone();
                        let selection = selection.clone();
                        let shift_window = shift_window.clone();
                        move |delta: i32| {
                            match (&shift_window, selection.as_ref()) {
                                // A MAPPED window moves with the verb (a
                                // real, undoable write).
                                (Some(window), _) => {
                                    dispatch_verb(
                                        &on_action,
                                        &surface,
                                        &selection,
                                        PatchVerbKind::ShiftPort {
                                            window: window.clone(),
                                            delta,
                                        },
                                    );
                                }
                                // A FREE window is selection only — a window
                                // is not a patch until the arm completes.
                                (None, Some(target)) => {
                                    if let Some(next) = shift_segment(&surface, target, delta) {
                                        select(&on_action, Some(next));
                                    }
                                }
                                (None, None) => {}
                            }
                        }
                    };
                    let resize = {
                        let surface = surface.clone();
                        let selection = selection.clone();
                        move |delta: i32| {
                            if let Some(target) = selection.as_ref()
                                && let Some(next) = resize_segment(&surface, target, delta)
                            {
                                if let (UiPatchTarget::Segment { lamps, .. }, Some(ui)) = (
                                    &next,
                                    ui,
                                ) {
                                    // Resizing CREATES the override `m`
                                    // keeps (the key grammar's rule).
                                    let mut size = ui.segment_size;
                                    size.set(Some(*lamps));
                                }
                                select(&on_action, Some(next));
                            }
                        }
                    };
                    rsx! {
                        SectionHead {
                            label: "output",
                            name: output.name.clone(),
                            context: output.pin.clone(),
                            facts,
                            deselect: primary,
                            on_action,
                        }
                        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5",
                            // The port picker writes on pick when there is
                            // an object to move; otherwise it walks the
                            // selection to that port's first free segment.
                            select {
                                class: "{PICKER}",
                                value: "{selected_port}",
                                onchange: {
                                    let surface = surface.clone();
                                    let object_target = object_target.clone();
                                    move |event: FormEvent| {
                                        let Some((node, key)) = parse_port_key(&event.value()) else {
                                            return;
                                        };
                                        let Some(entry) = surface
                                            .outputs
                                            .iter()
                                            .find(|entry| entry.node == node)
                                        else {
                                            return;
                                        };
                                        match object_target.as_ref() {
                                            Some(target) => {
                                                if let Some(lamp) = port_next_free(entry, key) {
                                                    dispatch_assign(
                                                        &on_action,
                                                        &surface,
                                                        target,
                                                        entry,
                                                        lamp,
                                                    );
                                                }
                                            }
                                            None => {
                                                let run = entry
                                                    .bay
                                                    .ports
                                                    .iter()
                                                    .find(|port| port.key == key)
                                                    .and_then(|port| free_runs(port).into_iter().next());
                                                let target = match run {
                                                    Some(run) => {
                                                        segment_at_free_run(&surface, node, key, run, None)
                                                    }
                                                    None => UiPatchTarget::Port { node, port: key },
                                                };
                                                select(&on_action, Some(target));
                                            }
                                        }
                                    }
                                },
                                if selected_port.is_empty() {
                                    option { value: "", "pick a port…" }
                                }
                                for (value , label) in ports.iter().cloned() {
                                    option { key: "{value}", value: "{value}", "{label}" }
                                }
                            }
                        }
                        // Strip container — P5 decodes the port's live
                        // lamps here. Today it shows the honest geometry:
                        // the window inside the port's span.
                        div { class: "{STRIP}", "data-patch-strip": "output",
                            if let Some(style) = window_style {
                                div {
                                    class: "tw:absolute tw:inset-y-0 tw:rounded-[3px] tw:border tw:border-selection-border tw:bg-selection-bg",
                                    style: "{style}",
                                }
                            }
                        }
                        div { class: "tw:flex tw:flex-wrap tw:items-baseline tw:gap-2 tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                            span { "wire {span_start + 1}" }
                            // A1, stated where it is assumed: the strip's
                            // values decode under the CURRENT fixture's
                            // lamp type. P5 names the layout it used.
                            span { class: "tw:text-status-warning-foreground",
                                "decoded under this fixture's lamp type"
                            }
                            span { class: "tw:ml-auto", "wire {span_start + span_lamps}" }
                        }
                        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5",
                            button {
                                class: "{STEP}",
                                title: if free { "Walk the window back ten lamps ([)" } else { "Shift the run back ten lamps" },
                                onclick: {
                                    let nudge = nudge.clone();
                                    move |_| nudge(-10)
                                },
                                "‹‹ [×10"
                            }
                            button {
                                class: "{STEP}",
                                title: if free { "Walk the window back one lamp ([)" } else { "Shift the run back one lamp" },
                                onclick: {
                                    let nudge = nudge.clone();
                                    move |_| nudge(-1)
                                },
                                "‹ ["
                            }
                            button {
                                class: "{STEP}",
                                title: if free { "Walk the window on one lamp (])" } else { "Shift the run on one lamp" },
                                onclick: {
                                    let nudge = nudge.clone();
                                    move |_| nudge(1)
                                },
                                "] ›"
                            }
                            button {
                                class: "{STEP}",
                                title: if free { "Walk the window on ten lamps (])" } else { "Shift the run on ten lamps" },
                                onclick: {
                                    let nudge = nudge.clone();
                                    move |_| nudge(10)
                                },
                                "]×10 ››"
                            }
                            if free {
                                span { class: "tw:w-2" }
                                button {
                                    class: "{STEP}",
                                    title: "Narrow the segment one lamp (-)",
                                    onclick: {
                                        let resize = resize.clone();
                                        move |_| resize(-1)
                                    },
                                    "narrow −"
                                }
                                button {
                                    class: "{STEP}",
                                    title: "Widen the segment one lamp (=)",
                                    onclick: {
                                        let resize = resize.clone();
                                        move |_| resize(1)
                                    },
                                    "widen ="
                                }
                            }
                            if matches!(selection, Some(UiPatchTarget::Port { .. })) || swap_armed {
                                span { class: "tw:w-2" }
                                button {
                                    class: if swap_armed { STEP_ARMED } else { STEP },
                                    title: "Arm a port swap, then click the other port (s)",
                                    onclick: {
                                        let surface = surface.clone();
                                        let selection = selection.clone();
                                        move |_| {
                                            if let Some(ui) = ui {
                                                let mut armed = ui.armed;
                                                arm_swap(&surface, &selection, &mut armed);
                                            }
                                        }
                                    },
                                    if swap_armed {
                                        "armed — click a port"
                                    } else {
                                        "swap s"
                                    }
                                }
                            }
                            span { class: "tw:w-2" }
                            button {
                                class: "{STEP_ARM}",
                                title: "Select the next free segment, keeping the arm (m)",
                                onclick: {
                                    let surface = surface.clone();
                                    let selection = selection.clone();
                                    move |_| {
                                        let size = ui.and_then(|ui| *ui.segment_size.peek());
                                        if let Some(next) = next_free_segment(
                                            &surface,
                                            selection.as_ref(),
                                            size,
                                        ) {
                                            select(&on_action, Some(next));
                                        }
                                    }
                                },
                                "next free m"
                            }
                        }
                    }
                }
                None => {
                    match unmapped_object {
                        // The invitation (#objfirst): arm, or pick the port
                        // the object lands on.
                        Some(target) => rsx! {
                            SectionHead {
                                label: "output",
                                name: "—".to_string(),
                                context: String::new(),
                                facts: Vec::new(),
                                deselect: false,
                                on_action,
                            }
                            div { class: "{PROMPT}",
                                span { "Not on any port segment." }
                                button {
                                    class: if is_armed { STEP_ARMED } else { STEP_ARM },
                                    title: "Arm the assign — the next port click links it (a)",
                                    onclick: {
                                        let surface = surface.clone();
                                        let selection = selection.clone();
                                        move |_| {
                                            if let Some(ui) = ui {
                                                let mut armed = ui.armed;
                                                arm_assign(&surface, &selection, &mut armed);
                                            }
                                        }
                                    },
                                    if is_armed {
                                        "armed — click a port"
                                    } else {
                                        "assign a"
                                    }
                                }
                                select {
                                    class: "{PICKER}",
                                    value: "",
                                    onchange: {
                                        let surface = surface.clone();
                                        move |event: FormEvent| {
                                            let Some((node, key)) = parse_port_key(&event.value()) else {
                                                return;
                                            };
                                            let Some(entry) = surface
                                                .outputs
                                                .iter()
                                                .find(|entry| entry.node == node)
                                            else {
                                                return;
                                            };
                                            // The object lands at the port's next
                                            // free lamp — the spike's picker
                                            // transition.
                                            if let Some(lamp) = port_next_free(entry, key)
                                                && dispatch_assign(
                                                    &on_action,
                                                    &surface,
                                                    &target,
                                                    entry,
                                                    lamp,
                                                )
                                                && let Some(ui) = ui
                                            {
                                                let mut armed = ui.armed;
                                                armed.set(None);
                                            }
                                        }
                                    },
                                    option { value: "", "or pick…" }
                                    for (value , label) in ports.iter().cloned() {
                                        option { key: "{value}", value: "{value}", "{label} · next free" }
                                    }
                                }
                            }
                        },
                        None => rsx! {
                            SectionHead {
                                label: "output",
                                name: "—".to_string(),
                                context: String::new(),
                                facts: Vec::new(),
                                deselect: false,
                                on_action,
                            }
                            div { class: "{PROMPT} tw:opacity-70", "No port segment selected." }
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{
        UiFixturePatch, UiPatchBay, UiPatchInstance, UiPatchPort, UiPatchSurfaceFixture,
        UiPatchSurfaceOutput,
    };

    fn dome_node() -> NodeId {
        NodeId::new(2)
    }

    fn output_node() -> NodeId {
        NodeId::new(10)
    }

    fn cell(
        id: &str,
        source_start: u32,
        wire_start: u32,
        lamps: u32,
        reversed: bool,
    ) -> UiPatchCell {
        UiPatchCell {
            id: id.to_string(),
            producer: "dome".to_string(),
            source_start,
            lamps,
            wire_start,
            reversed,
            port_label: "IO18".to_string(),
            output_label: "out_a".to_string(),
            ..Default::default()
        }
    }

    fn instance(path: &str, label: &str, start: u32, lamps: u32, placed: bool) -> UiPatchInstance {
        UiPatchInstance {
            path: path.to_string(),
            label: label.to_string(),
            start,
            lamps,
            stride: 1,
            placed,
        }
    }

    /// Sector 1 mapped to the front of one 60-lamp port, sector 2 still
    /// waiting — the shape both flows walk.
    fn half_patched_surface() -> UiPatchSurface {
        let mapped = cell("2:0", 0, 0, 30, true);
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: dome_node(),
                label: "dome".to_string(),
                patch: UiFixturePatch {
                    lamps: 60,
                    cells: vec![mapped.clone()],
                    ..Default::default()
                },
                instances: vec![
                    instance("/sector/1", "sector 1", 0, 30, true),
                    instance("/sector/2", "sector 2", 30, 30, false),
                ],
                ..Default::default()
            }],
            outputs: vec![UiPatchSurfaceOutput {
                node: output_node(),
                label: "out_a".to_string(),
                name: Some("1".to_string()),
                bay: UiPatchBay {
                    ports: vec![UiPatchPort {
                        key: 0,
                        pin_label: "IO18".to_string(),
                        start: 0,
                        lamps: 60,
                        cells: vec![mapped],
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// The blue edge follows core's space matrix — never a second answer.
    #[test]
    fn the_primary_side_follows_the_selections_space() {
        assert_eq!(
            primary_side(Some(&UiPatchTarget::Instance {
                node: dome_node(),
                path: "/sector/2".to_string(),
            })),
            Some(PanelSide::Object)
        );
        assert_eq!(
            primary_side(Some(&UiPatchTarget::Cell {
                id: "2:0".to_string()
            })),
            Some(PanelSide::Object),
            "a mapped run speaks the fixture's language (D9)"
        );
        assert_eq!(
            primary_side(Some(&UiPatchTarget::Segment {
                node: output_node(),
                port: 0,
                start: 30,
                lamps: 30,
            })),
            Some(PanelSide::Output)
        );
        assert_eq!(
            primary_side(Some(&UiPatchTarget::Module {
                node: NodeId::new(1)
            })),
            None,
            "a module is a level above both sections"
        );
        assert_eq!(primary_side(None), None);
    }

    /// `#objfirst`: an unmapped object fills the object section and leaves
    /// the output section with nothing to show — which is what makes it
    /// invite.
    #[test]
    fn an_unmapped_object_fills_one_section_and_invites_in_the_other() {
        let surface = half_patched_surface();
        let selection = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/2".to_string(),
        };
        let object = object_view(&surface, Some(&selection)).expect("the object section fills");
        assert_eq!(object.name, "sector 2");
        assert_eq!(object.context, "dome · /sector/2");
        assert_eq!(object.lamps, 30);
        assert!(!object.mapped);
        assert_eq!(
            object_facts(&object),
            vec![
                ("lamps".to_string(), "30".to_string()),
                ("wire".to_string(), "unmapped".to_string()),
            ]
        );
        assert_eq!(
            output_view(&surface, Some(&selection)),
            None,
            "an unmapped object has no wire to derive"
        );
        assert!(target_is_unmapped(&surface, &selection), "so it invites");
    }

    /// `#derived` / `#paired`: a mapped object derives its window, and the
    /// panel says the window is derived rather than picked.
    #[test]
    fn a_mapped_object_derives_its_window() {
        let surface = half_patched_surface();
        let selection = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/1".to_string(),
        };
        let object = object_view(&surface, Some(&selection)).expect("object");
        assert!(object.mapped);
        assert!(object.reversed, "the run is laid end-first");
        assert_eq!(
            object_facts(&object)[1],
            ("wire".to_string(), "mapped · reversed".to_string())
        );

        let output = output_view(&surface, Some(&selection)).expect("derived window");
        assert_eq!(output.node, output_node());
        assert_eq!(output.port, Some(0));
        assert_eq!(output.window, Some((0, 30)));
        assert!(output.derived);
        assert!(!output.free, "a derived window is a mapped run");
        assert_eq!(
            output_facts(&output),
            vec![
                ("pin".to_string(), "IO18".to_string()),
                ("lamps".to_string(), "30/60 used".to_string()),
                ("window".to_string(), "1-30 · derived".to_string()),
            ]
        );
    }

    /// A mapped run selected as its CELL shows the owner object above its
    /// own window — both sections, one selection.
    #[test]
    fn a_cell_selection_shows_its_owner_and_its_window() {
        let surface = half_patched_surface();
        let selection = UiPatchTarget::Cell {
            id: "2:0".to_string(),
        };
        let object = object_view(&surface, Some(&selection)).expect("owner object");
        assert_eq!(object.name, "sector 1");
        assert!(object.mapped);
        assert_eq!(
            object.target, selection,
            "the verbs keep acting on the cell the user clicked"
        );
        let output = output_view(&surface, Some(&selection)).expect("its window");
        assert_eq!(output.window, Some((0, 30)));
    }

    /// `#armed`: a free segment fills the output section as FREE space (the
    /// nudges resize it) and leaves the object section inviting.
    #[test]
    fn a_free_segment_is_the_output_sections_window() {
        let surface = half_patched_surface();
        let free = UiPatchTarget::Segment {
            node: output_node(),
            port: 0,
            start: 30,
            lamps: 30,
        };
        assert_eq!(object_view(&surface, Some(&free)), None);
        let output = output_view(&surface, Some(&free)).expect("the window");
        assert_eq!(output.window, Some((30, 30)));
        assert!(output.free);
        assert!(!output.derived);
        assert_eq!(output.span, (0, 60), "the strip spans the whole port");

        // A run that landed under the window since the click: the section
        // reads the port rather than trusting the target's word.
        let taken = UiPatchTarget::Segment {
            node: output_node(),
            port: 0,
            start: 0,
            lamps: 30,
        };
        assert!(!output_view(&surface, Some(&taken)).expect("window").free);
    }

    /// A whole port/output selection fills the section with its facts and no
    /// window — nothing derived, nothing invented.
    #[test]
    fn ports_and_outputs_fill_the_section_without_a_window() {
        let surface = half_patched_surface();
        let port = output_view(
            &surface,
            Some(&UiPatchTarget::Port {
                node: output_node(),
                port: 0,
            }),
        )
        .expect("port");
        assert_eq!(port.window, None);
        assert_eq!(port.used, 30);
        assert_eq!(port.total, 60);

        let output = output_view(
            &surface,
            Some(&UiPatchTarget::Output {
                node: output_node(),
            }),
        )
        .expect("output");
        assert_eq!(output.port, None);
        assert_eq!(output.name, "1", "the authored name wins the label");
        assert_eq!(output.window, None);
    }

    /// The pickers' rows: unmapped objects on the object side, every port on
    /// the output side (round-tripping its value key).
    #[test]
    fn the_pickers_list_the_two_sides_options() {
        let surface = half_patched_surface();
        let objects = unmapped_objects(&surface);
        assert_eq!(objects.len(), 1, "only sector 2 still wants a wire");
        assert_eq!(
            objects[0].0,
            UiPatchTarget::Instance {
                node: dome_node(),
                path: "/sector/2".to_string(),
            }
        );
        assert_eq!(objects[0].1, "sector 2 · 30");

        let ports = port_options(&surface);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].0, "10:0");
        assert_eq!(ports[0].1, "1 · IO18 · port 0");
        assert_eq!(parse_port_key(&ports[0].0), Some((output_node(), 0)));
        assert_eq!(parse_port_key("nonsense"), None);
    }

    /// A range-grain fixture (no instances, no runs) is one pickable object;
    /// once it has a run it drops out of the invitation's list.
    #[test]
    fn a_range_grain_fixture_is_one_picker_row() {
        let mut peach = half_patched_surface();
        peach.fixtures[0].instances.clear();
        peach.fixtures[0].patch.cells.clear();
        let objects = unmapped_objects(&peach);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].0, UiPatchTarget::Fixture { node: dome_node() });
        assert_eq!(objects[0].1, "dome · 60");

        let mut placed = half_patched_surface();
        placed.fixtures[0].instances.clear();
        assert!(unmapped_objects(&placed).is_empty());
    }
}
