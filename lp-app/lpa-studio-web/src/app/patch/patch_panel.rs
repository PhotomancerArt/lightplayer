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
//! The strips (P5) paint the light languages IN the lamps. The OUTPUT strip
//! is the selected port's published bytes, decoded under A1's lamp type; the
//! OBJECT strip is the object's own lamps in OBJECT order — decoded from the
//! same published frames when it is mapped, and read from the controller's
//! [`UiPatchSurface::chase_preview`] when it is not (Q9: the unmapped chase
//! is computed ONCE, core-side, and the canvas sprites paint the very same
//! colors). Nothing here animates itself; the panel has no clock. Both
//! strips follow the spike's reactive rule (bulbs ≥ 7px/lamp, gradient
//! beyond) via the shared [`LampStrip`].
//!
//! Two grains live in the object section (Q8). A whole-FIXTURE selection is
//! not an object — it renders the FIXTURE CARD: fixture-grain facts, the
//! flow selector, `unmap all`, and no chase or transport. A fixture with no
//! sub-objects (the count-only strip — the scarf) IS its own object and
//! keeps the object treatment, with the flow selector added. And the grammar
//! itself is mode-gated (Q11): an AUTO-mapped fixture's objects show facts
//! and a strip only, because auto reflow would fight every transport verb.

use dioxus::prelude::*;
use lpa_studio_core::{
    ColorOrder, NodeId, PatchVerbKind, PatchVerbWindow, ProjectEditorOp, UiAction,
    UiControlProductPreview, UiPatchCell, UiPatchSurface, UiPatchSurfaceFixture, UiPatchTarget,
};

use crate::app::editor_shell::patching::{
    ArmedVerb, PatchingUi, arm_assign, arm_swap, assign_subject_target, is_armable,
};
use crate::app::node::lamp_view::{
    UNLIT_RGB, cell_frame, control_color_order_at_sample, control_rgb_at_sample,
    linear_unorm16_to_srgb8, wire_lamp_rgb,
};
use crate::app::patch::lamp_strip::{LampStrip, StripPresentation};
use crate::app::patch::verb_ui::{
    dispatch_assign, dispatch_verb, free_runs, instance_target, next_free_segment, port_next_free,
    resize_segment, segment_at_free_run, selection_stride, shift_segment, target_is_unmapped,
};
use crate::base::option_cards::{OptionCard, OptionCards};
use crate::base::{StudioIcon, StudioIconName};

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

/// The hotkey CHIP (G1 round 3, #5): a key on a button is a kbd badge,
/// visually a key-cap, never prose glued to the label ("assign a" read
/// like English). Shared by the transport buttons and the keys-row footer.
const KBD: &str = "tw:inline-block tw:rounded-[3px] tw:border tw:border-border-strong tw:bg-terminal tw:px-1 tw:font-mono tw:text-[9px] tw:leading-[13px] tw:text-dim-foreground";

/// A button label with its hotkey chip.
fn keyed(label: &str, key: &str) -> Element {
    rsx! {
        span { class: "tw:inline-flex tw:items-center tw:gap-1",
            span { "{label}" }
            span { class: "{KBD}", "{key}" }
        }
    }
}

/// Are engine frames flowing? The gate on every attention ANIMATION (the
/// armed pulse, the counterpart ring): live sessions breathe, stories and
/// frameless surfaces render the settled state — the same freeze rule the
/// chase preview keeps core-side.
pub(crate) fn surface_is_live(surface: &UiPatchSurface) -> bool {
    surface
        .outputs
        .iter()
        .any(|output| output_frame(surface, output.node).is_some())
}

/// Is the viewport at the mobile fold (the workbench's ≤820px breakpoint)?
/// Runtime-checked at gesture time; host test builds never fire gestures.
fn at_mobile_fold() -> bool {
    web_sys::window()
        .and_then(|window| window.inner_width().ok())
        .and_then(|width| width.as_f64())
        .is_some_and(|width| width <= 820.0)
}
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
    /// The owning FIXTURE's flow flag (P5b) — a fixture fact shown on the
    /// object section because that is where the user is looking when they
    /// discover an object cannot be unmapped.
    pub manual: bool,
    /// This object IS its whole fixture — the count-only strand (the scarf),
    /// which has no object table and so no fixture card of its own. It wears
    /// the flow selector here instead (Q8's exception).
    pub whole_fixture: bool,
    /// The fixture this object belongs to — the flow verbs' subject
    /// (they act on the fixture whatever grain the selection named).
    pub fixture: NodeId,
}

/// The FIXTURE CARD (Q8): what a whole-fixture selection shows instead of
/// pretending the fixture is one object.
///
/// Fixture-grain facts and fixture-grain verbs only — the counts, the flow
/// selector, and (in manual mode) `unmap all`. No chase, no transport: a
/// fixture with sub-objects has no single direction to show and no single
/// run to rotate, and the objects underneath it are one canvas click away.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FixtureCard {
    pub node: NodeId,
    pub name: String,
    /// The fixture's address path, or its label when it has none.
    pub context: String,
    pub lamps: u32,
    pub objects: usize,
    pub placed: usize,
    pub manual: bool,
}

/// The fixture card for this selection, or `None` when the selection is not
/// a whole fixture WITH sub-objects.
///
/// The exception is the ruling's: a fixture with no object table is one
/// strand (the scarf), which IS its own object — it keeps the object
/// treatment ([`object_view`]) and gains the flow row there instead.
pub(crate) fn fixture_card(
    surface: &UiPatchSurface,
    selection: Option<&UiPatchTarget>,
) -> Option<FixtureCard> {
    let UiPatchTarget::Fixture { node } = selection? else {
        return None;
    };
    let fixture = fixture_of(surface, *node)?;
    if fixture.instances.is_empty() {
        return None;
    }
    Some(FixtureCard {
        node: fixture.node,
        name: fixture.label.clone(),
        context: fixture
            .address
            .clone()
            .unwrap_or_else(|| fixture.label.clone()),
        lamps: fixture.patch.lamps,
        objects: fixture.instances.len(),
        placed: fixture
            .instances
            .iter()
            .filter(|instance| instance.placed)
            .count(),
        manual: fixture.manual_flow,
    })
}

/// The fixture card's facts, in the fact-card idiom.
pub(crate) fn fixture_facts(card: &FixtureCard) -> Vec<(String, String)> {
    vec![
        ("lamps".to_string(), card.lamps.to_string()),
        (
            "objects".to_string(),
            format!("{}/{} placed", card.placed, card.objects),
        ),
        ("flow".to_string(), flow_label(card.manual).to_string()),
    ]
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
        // A whole fixture is an OBJECT only when it has no sub-objects: one
        // count-only strand (the scarf), whose single patch entry IS the
        // fixture. A fixture with an object table gets the FIXTURE CARD
        // instead (Q8) — see [`fixture_card`].
        UiPatchTarget::Fixture { node } => {
            let fixture = fixture_of(surface, *node)?;
            if !fixture.instances.is_empty() {
                return None;
            }
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
        manual: fixture.manual_flow,
        whole_fixture: fixture.instances.is_empty(),
        fixture: fixture.node,
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
            // A fixture WITH objects names no single window (Q8): its runs
            // are its objects', and picking the first one to show would
            // claim a wire the card is not about.
            if matches!(target, UiPatchTarget::Fixture { .. }) && !fixture.instances.is_empty() {
                return None;
            }
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
///
/// `flow` is the FIXTURE's fact, not this object's — it belongs here because
/// it is the answer to "why did unmapping this put it back on the wire?"
/// (P5b), and this is the section the user is reading when they ask.
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
        ("flow".to_string(), flow_label(object.manual).to_string()),
    ]
}

/// What the fixture's flow flag is CALLED, everywhere it is shown.
pub(crate) fn flow_label(manual: bool) -> &'static str {
    if manual { "manual" } else { "auto-mapped" }
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

// -- the strips ----------------------------------------------------------------

/// The fixture an object-side target belongs to.
fn object_fixture<'a>(
    surface: &'a UiPatchSurface,
    target: &UiPatchTarget,
) -> Option<&'a UiPatchSurfaceFixture> {
    match target {
        UiPatchTarget::Fixture { node }
        | UiPatchTarget::Instance { node, .. }
        | UiPatchTarget::Range { node, .. } => fixture_of(surface, *node),
        UiPatchTarget::Cell { id } => cell_owner(surface, id).map(|(fixture, _)| fixture),
        _ => None,
    }
}

/// The OBJECT strip: the object's lamps in the OBJECT's own order.
///
/// Mapped, it is the published wire decoded back through the object's runs,
/// each against ITS OWN output's frame and end-first where the run is
/// reversed — which is how the engine's chase reaches this strip without the
/// client painting a thing, and why a reversed strand still reads
/// object-continuous here while running backwards on the wire.
///
/// Unmapped, there is no wire and therefore nothing published — so the
/// CONTROLLER paints the chase for it (Q9) and the strip reads
/// [`UiPatchSurface::chase_preview`] like any other decode. The canvas
/// sprites read the same colors, which is the whole point: one selection,
/// one chase, painted once. The preview arrives in the engine's 16-bit
/// linear space, so it goes through the same transfer a frame sample does.
///
/// Empty = nothing honest to draw (no frame yet); the host box's track shows
/// through rather than a field of invented black lamps.
fn object_strip_colors(surface: &UiPatchSurface, object: &ObjectView) -> Vec<[u8; 3]> {
    let lamps = object.lamps as usize;
    if lamps == 0 {
        return Vec::new();
    }
    if !object.mapped {
        return surface
            .chase_preview
            .as_ref()
            .filter(|preview| preview.node == object.fixture && preview.colors.len() == lamps)
            .map(|preview| preview.colors.iter().copied().map(srgb8).collect())
            .unwrap_or_default();
    }
    let Some(fixture) = object_fixture(surface, &object.target) else {
        return Vec::new();
    };
    let Some((range_start, _)) = object_source_range(fixture, &object.target) else {
        return Vec::new();
    };
    let mut colors = vec![UNLIT_RGB; lamps];
    let mut lit = false;
    for cell in cells_over(fixture, (range_start, object.lamps)) {
        let Some(frame) = cell_frame(surface, &cell.id) else {
            continue;
        };
        for index in 0..cell.lamps {
            let Some(slot) = cell
                .source_start
                .saturating_add(index)
                .checked_sub(range_start)
                .and_then(|offset| colors.get_mut(offset as usize))
            else {
                continue;
            };
            let wire = if cell.reversed {
                cell.wire_start
                    .saturating_add(cell.lamps.saturating_sub(1).saturating_sub(index))
            } else {
                cell.wire_start.saturating_add(index)
            };
            if let Some(rgb) = control_rgb_at_sample(frame, wire.saturating_mul(3)) {
                *slot = rgb;
                lit = true;
            }
        }
    }
    if lit { colors } else { Vec::new() }
}

/// One core-computed preview lamp, in the sRGB bytes a strip and a sprite
/// both paint with — the same transfer a published frame sample takes, so
/// the mapped chase and the unmapped one land in the same greys.
pub(crate) fn srgb8([r, g, b]: [u16; 3]) -> [u8; 3] {
    [
        linear_unorm16_to_srgb8(r),
        linear_unorm16_to_srgb8(g),
        linear_unorm16_to_srgb8(b),
    ]
}

/// The output's published frame, if one has arrived.
fn output_frame(surface: &UiPatchSurface, node: NodeId) -> Option<&UiControlProductPreview> {
    surface
        .outputs
        .iter()
        .find(|output| output.node == node)?
        .bay
        .frame
        .as_ref()
}

/// The OUTPUT strip: the port's WHOLE extent, in wire order.
///
/// Every lamp of the span, mapped or free — the free stretches are the point
/// of the walk-up flow, and the engine paints the selection's breath into
/// them before it publishes, so they carry real light. Lamps past the
/// published extent decode to nothing and draw as the unlit neutral.
fn output_strip_colors(
    frame: Option<&UiControlProductPreview>,
    span: (u32, u32),
    assumed: ColorOrder,
) -> Vec<[u8; 3]> {
    let Some(frame) = frame else {
        return Vec::new();
    };
    let (start, lamps) = span;
    (0..lamps)
        .map(|index| {
            wire_lamp_rgb(frame, start.saturating_add(index), assumed).unwrap_or(UNLIT_RGB)
        })
        .collect()
}

/// The next object still waiting for a wire, with its fixture — the one that
/// sizes a free segment, and so (D6) the one whose lamp type the free
/// stretches are read under.
///
/// MANUAL fixtures only (Q11): an auto-mapped fixture's unnamed lamps flow
/// onto the wire by themselves, so nothing there is waiting to be placed and
/// a segment sized by one of its objects would be sized for a link the user
/// is never asked to make.
fn next_unmapped_object(surface: &UiPatchSurface) -> Option<(&UiPatchSurfaceFixture, String)> {
    for fixture in surface.fixtures.iter().filter(|entry| entry.manual_flow) {
        if fixture.instances.is_empty() {
            if fixture.patch.cells.is_empty() && fixture.patch.lamps > 0 {
                return Some((fixture, fixture.label.clone()));
            }
            continue;
        }
        if let Some(instance) = fixture.instances.iter().find(|entry| !entry.placed) {
            return Some((fixture, instance.label.clone()));
        }
    }
    None
}

/// A fixture's lamp type, learned from the wire it already drives.
///
/// The layout a frame carries declares a colour order per PLACED run, so a
/// fixture with a run anywhere states its own lamp type; one with no run at
/// all has never told anybody, and the panel says so rather than guessing in
/// silence.
///
/// Each run is asked of ITS OWN output's frame ([`cell_frame`]): a fixture
/// driving two boxes has two wires, and a run's wire lamps mean nothing in
/// the other one's layout.
fn fixture_color_order(
    surface: &UiPatchSurface,
    fixture: &UiPatchSurfaceFixture,
) -> Option<ColorOrder> {
    fixture.patch.cells.iter().find_map(|cell| {
        control_color_order_at_sample(
            cell_frame(surface, &cell.id)?,
            cell.wire_start.saturating_mul(3),
        )
    })
}

/// A1, said out loud: which lamp type the output strip decoded under, and
/// where that assumption came from.
///
/// The wire's own layout answers for every stretch a producer sits on. The
/// FREE stretches — the ones a walk-up user is about to spend — have no
/// declared order at all, so the strip reads them under the lamp type of the
/// object that would land there (D6). That is a deliberate lp2014-style
/// assumption, which is exactly why the panel prints it.
fn decode_line(
    surface: &UiPatchSurface,
    output: &OutputView,
    object: Option<&ObjectView>,
    frame: Option<&UiControlProductPreview>,
) -> (ColorOrder, String) {
    let Some(frame) = frame else {
        return (ColorOrder::Rgb, "no signal on this wire yet".to_string());
    };
    // A mapped window: the owner object's own lamp type, as published.
    if let Some(object) = object.filter(|object| object.mapped)
        && let Some(order) = output
            .window
            .and_then(|(start, _)| control_color_order_at_sample(frame, start.saturating_mul(3)))
            .or_else(|| {
                object_fixture(surface, &object.target)
                    .and_then(|fixture| fixture_color_order(surface, fixture))
            })
    {
        return (order, decoded_as(order, &object.name));
    }
    // Free space: the object that sized the segment (D6).
    if let Some((fixture, name)) = next_unmapped_object(surface)
        && let Some(order) = fixture_color_order(surface, fixture)
    {
        return (order, decoded_as(order, &name));
    }
    // Nothing on this surface has stated a lamp type yet: fall back to what
    // this wire's other runs use, and name the fallback as a fallback.
    let order = frame
        .sample_layout
        .spans
        .iter()
        .find_map(|span| control_color_order_at_sample(frame, span.start))
        .unwrap_or(ColorOrder::Rgb);
    (order, decoded_as(order, "assumed"))
}

fn decoded_as(order: ColorOrder, source: &str) -> String {
    format!("decoded as {} — {source}", color_order_label(order))
}

fn color_order_label(order: ColorOrder) -> &'static str {
    match order {
        ColorOrder::Rgb => "RGB",
        ColorOrder::Grb => "GRB",
        ColorOrder::Rbg => "RBG",
        ColorOrder::Gbr => "GBR",
        ColorOrder::Brg => "BRG",
        ColorOrder::Bgr => "BGR",
    }
}

/// Every object still waiting for a wire, in surface order — the object
/// side's inline picker (direct-assign).
///
/// MANUAL fixtures only (Q11), for the same reason [`next_unmapped_object`]
/// filters: offering to patch an auto-mapped fixture's object would be
/// offering a link its own flow flag makes meaningless.
pub(crate) fn unmapped_objects(surface: &UiPatchSurface) -> Vec<(UiPatchTarget, String)> {
    let mut rows = Vec::new();
    for fixture in surface.fixtures.iter().filter(|entry| entry.manual_flow) {
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
                // Occupancy IN the option (round 3, #6): a destination you
                // cannot judge is not a choice.
                (
                    format!("{}:{}", output.node.0, port.key),
                    format!(
                        "{} · {pin} · {}",
                        output.display_name(),
                        port_occupancy(port)
                    ),
                )
            })
        })
        .collect()
}

/// One terse occupancy phrase: where the free space is, or that none is.
fn port_occupancy(port: &lpa_studio_core::UiPatchPort) -> String {
    let used: u32 = port.cells.iter().map(|cell| cell.lamps).sum();
    let free = port.lamps.saturating_sub(used);
    if free == 0 {
        "full".to_string()
    } else if let Some((start, _)) = free_runs(port).into_iter().next() {
        format!("{free} free @ {}", start + 1)
    } else {
        format!("{free} free")
    }
}

/// The same choices as [`port_options`], as EXPLAINING CARDS (round 3, #6):
/// title = the port, blurb = its occupancy, so picking a destination is an
/// informed act rather than a name lottery.
pub(crate) fn port_cards(surface: &UiPatchSurface) -> Vec<OptionCard> {
    surface
        .outputs
        .iter()
        .flat_map(|output| {
            output.bay.ports.iter().map(move |port| {
                let pin = if port.pin_label.is_empty() {
                    format!("port {}", port.key)
                } else {
                    port.pin_label.clone()
                };
                OptionCard {
                    id: format!("{}:{}", output.node.0, port.key),
                    icon: StudioIconName::Usb,
                    title: format!("{} · {pin}", output.display_name()),
                    blurb: port_occupancy(port),
                }
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
    let card = fixture_card(&surface, selection.as_ref());
    let output = output_view(&surface, selection.as_ref());
    let primary = primary_side(selection.as_ref());
    // The attention language (round 3): while an assign is armed, the
    // COUNTERPART section — where the next click belongs — wears the ring.
    // Animations only run while frames flow (story determinism).
    let live = surface_is_live(&surface);
    let armed_assign = matches!(armed, Some(ArmedVerb::Assign));
    let attention_object = live && armed_assign && primary == Some(PanelSide::Output);
    let attention_output = live && armed_assign && primary == Some(PanelSide::Object);
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
                card,
                armed: armed.clone(),
                primary: primary == Some(PanelSide::Object),
                attention: attention_object,
                animate: live,
                on_action,
            }
            OutputPane {
                surface: surface.clone(),
                selection: selection.clone(),
                output,
                armed,
                primary: primary == Some(PanelSide::Output),
                attention: attention_output,
                animate: live,
                on_action,
            }
            // The keys row REPLACES the help overlay: one line, always
            // visible, in the panel the gesture happens in.
            div { class: "tw:flex tw:flex-none tw:flex-wrap tw:gap-x-3.5 tw:gap-y-0.5 tw:border-t tw:border-border-subtle tw:bg-card-muted tw:px-2.5 tw:py-1 tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                span {
                    span { class: "{KBD}", "a" }
                    " assign"
                }
                span {
                    span { class: "{KBD}", "m" }
                    " next free"
                }
                span {
                    span { class: "{KBD}", "[ ]" }
                    " shift"
                }
                span {
                    span { class: "{KBD}", "- =" }
                    " narrow / widen"
                }
                span {
                    span { class: "{KBD}", "r" }
                    " flip"
                }
                span {
                    span { class: "{KBD}", "; '" }
                    " rotate"
                }
                span {
                    span { class: "{KBD}", "s" }
                    " swap"
                }
                span {
                    span { class: "{KBD}", "⌘Z" }
                    " undo"
                }
                span {
                    span { class: "{KBD}", "esc" }
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
    /// A whole-fixture selection (Q8) — the FIXTURE CARD takes the section
    /// instead of an object; the two are mutually exclusive by construction.
    #[props(default)]
    card: Option<FixtureCard>,
    armed: Option<ArmedVerb>,
    primary: bool,
    /// The arm's counterpart ring: an armed assign points HERE next.
    #[props(default)]
    attention: bool,
    /// Frames are flowing — attention animations may run.
    #[props(default)]
    animate: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let ui = use_hook(try_consume_context::<PatchingUi>);
    // Nothing here animates itself any more (Q9): every picture the panel
    // paints — published bytes or the controller's unmapped-chase preview —
    // arrives as data on the surface, so the panel keeps no clock at all.
    let base = if primary { SECTION_PRIMARY } else { SECTION };
    let class = if attention {
        format!("{base} ux-arm-attention")
    } else {
        base.to_string()
    };
    let is_armed = matches!(armed, Some(ArmedVerb::Assign));
    let arm_class = if is_armed {
        if animate {
            format!("{STEP_ARMED} ux-arm-pulse")
        } else {
            STEP_ARMED.to_string()
        }
    } else {
        STEP_ARM.to_string()
    };
    // The invitation belongs to the object side when the WIRE side holds a
    // free segment: that is the pairing this panel can still make.
    let free_segment = match (&selection, &object) {
        (Some(UiPatchTarget::Segment { node, start, .. }), None) => Some((*node, *start)),
        _ => None,
    };
    // A port or output selection has no object either — but nothing to
    // invite, so it says what WOULD name one.
    let wire_hint = free_segment.is_none() && selection.is_some();
    if let Some(card) = card {
        return rsx! {
            section { class: "{class}",
                FixtureCardPane {
                    surface: surface.clone(),
                    card,
                    primary,
                    on_action,
                }
            }
        };
    }
    rsx! {
        section { class: "{class}",
            match object {
                Some(object) => {
                    let facts = object_facts(&object);
                    let colors = object_strip_colors(&surface, &object);
                    let target = Some(object.target.clone());
                    let stride = selection_stride(&surface, &target);
                    let mapped = object.mapped;
                    // Q11: an AUTO-mapped fixture reflows its own lamps, so
                    // every transport verb here would be fought by the next
                    // resolve. Its objects get the LEAN panel — facts and a
                    // strip — and the selector that unlocks the grammar
                    // lives on the fixture card, one click away.
                    let manual = object.manual;
                    // The scarf (Q8's exception): a fixture with no object
                    // table IS its own object, so it has no card to carry
                    // the flow selector — it wears one here instead.
                    let scarf = object.whole_fixture;
                    let fixture = object.fixture;
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
                        // The object's own lamps, in object order: the
                        // published chase when it is on a wire, and the
                        // controller's core-computed preview when it is not
                        // (Q9 — the very colors the canvas sprites paint).
                        // Nothing to decode yet leaves the track showing —
                        // an honest "no signal", not a field of black lamps.
                        div { class: "{STRIP}", "data-patch-strip": "object",
                            LampStrip { colors }
                        }
                        if manual {
                            div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5",
                                button {
                                    class: if mapped { STEP } else { STEP_OFF },
                                    disabled: !mapped,
                                    title: "Rotate one stride back (;)",
                                    onclick: {
                                        let verb = verb.clone();
                                        move |_| verb(PatchVerbKind::Rotate { steps: -1, stride })
                                    },
                                    {keyed("‹", ";")}
                                }
                                button {
                                    class: if mapped { STEP } else { STEP_OFF },
                                    disabled: !mapped,
                                    title: "Rotate one stride forward (')",
                                    onclick: {
                                        let verb = verb.clone();
                                        move |_| verb(PatchVerbKind::Rotate { steps: 1, stride })
                                    },
                                    {keyed("›", "'")}
                                }
                                button {
                                    class: if mapped { STEP } else { STEP_OFF },
                                    disabled: !mapped,
                                    title: "Reverse the wire direction (r)",
                                    onclick: {
                                        let verb = verb.clone();
                                        move |_| verb(PatchVerbKind::Reverse)
                                    },
                                    {keyed("flip", "r")}
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
                        } else {
                            // The LEAN panel (Q11): no transport, no
                            // invitation, and one line saying why — plus
                            // where the grammar is unlocked.
                            div { class: "tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                                "auto-mapped — this fixture places its own objects. Select the fixture to change that."
                            }
                        }
                        // The scarf carries the fixture row itself: it has no
                        // card of its own (Q8's exception).
                        if scarf {
                            div { class: "tw:grid tw:gap-1.5",
                                FlowSelector {
                                    surface: surface.clone(),
                                    node: fixture,
                                    manual,
                                    on_action,
                                }
                                if manual {
                                    div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5",
                                        UnmapAllButton {
                                            surface: surface.clone(),
                                            node: fixture,
                                            on_action,
                                        }
                                    }
                                }
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
                                        class: "{arm_class}",
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
                                        // The label never changes (round 3):
                                        // armed = the button pulses and the
                                        // counterpart section wears the ring.
                                        {keyed("assign", "a")}
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
                                                // dispatches, narrowed the same way.
                                                let subject = assign_subject_target(
                                                    &surface,
                                                    target,
                                                );
                                                if dispatch_assign(
                                                        &on_action,
                                                        &surface,
                                                        &subject,
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
                                        // The bound value IS the placeholder
                                        // (the picker snaps back after each
                                        // pick); `selected` mirrors that so
                                        // the mount order can't pick another
                                        // option (see select_mirror_lint).
                                        option { value: "", selected: true, "or pick…" }
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

/// THE FIXTURE CARD (Q8): what a whole-fixture selection shows.
///
/// Fixture-grain facts, the flow selector, and — in manual mode — the one
/// gesture that takes the whole thing back off the wire. No chase strip and
/// no object transport: a fixture with sub-objects has no single direction
/// to show and no single run to rotate, and its objects are one canvas click
/// away now that clicks name objects (Q10).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureCardPane(
    surface: UiPatchSurface,
    card: FixtureCard,
    primary: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let facts = fixture_facts(&card);
    let unplaced = card.objects.saturating_sub(card.placed);
    rsx! {
        SectionHead {
            label: "fixture",
            name: card.name.clone(),
            context: card.context.clone(),
            facts,
            deselect: primary,
            on_action,
        }
        FlowSelector {
            surface: surface.clone(),
            node: card.node,
            manual: card.manual,
            on_action,
        }
        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5",
            if card.manual {
                UnmapAllButton { surface: surface.clone(), node: card.node, on_action }
            }
            span { class: "tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                if card.manual && unplaced > 0 {
                    "{unplaced} still to patch — click one on the canvas."
                } else if card.manual {
                    "every object is on a wire."
                } else {
                    "objects flow onto the wire by themselves."
                }
            }
        }
    }
}

/// Q7: the flow control as an EXPLAINING selector, not a bare toggle.
///
/// Two cards, both always visible, each saying what picking it means — a
/// toggle named one state and left the user to guess the other. Picking
/// dispatches the ordinary `SetFlow` verb, so it is one undoable step
/// through the same write path as everything else in the panel.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FlowSelector(
    surface: UiPatchSurface,
    node: NodeId,
    manual: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let options = vec![
        OptionCard::new(
            FLOW_AUTO,
            StudioIconName::MapArrows,
            "auto-mapped",
            "objects place themselves along the wire — just works",
        ),
        OptionCard::new(
            FLOW_MANUAL,
            StudioIconName::Edited,
            "manual",
            "only what you patch lights up — unmapped stays dark",
        ),
    ];
    rsx! {
        OptionCards {
            label: "mapping".to_string(),
            options,
            selected: if manual { FLOW_MANUAL.to_string() } else { FLOW_AUTO.to_string() },
            on_pick: move |id: String| {
                // The flow verb acts on the FIXTURE, whatever grain the
                // selection named.
                dispatch_verb(
                    &on_action,
                    &surface,
                    &Some(UiPatchTarget::Fixture { node }),
                    PatchVerbKind::SetFlow { manual: id == FLOW_MANUAL },
                );
            },
        }
    }
}

/// The flow selector's option ids — the values [`flow_label`] names.
pub(crate) const FLOW_AUTO: &str = "auto";
pub(crate) const FLOW_MANUAL: &str = "manual";

/// Take every object of one fixture off the wire (undoable, like every other
/// verb — so it is safe to try).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UnmapAllButton(
    surface: UiPatchSurface,
    node: NodeId,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        button {
            class: "{STEP}",
            title: "Take every object of this fixture off the wire (undoable)",
            onclick: move |_| {
                dispatch_verb(
                    &on_action,
                    &surface,
                    &Some(UiPatchTarget::Fixture { node }),
                    PatchVerbKind::UnmapAll,
                );
            },
            span { class: "tw:mr-1 tw:inline-flex tw:items-center tw:align-[-1px]", aria_hidden: "true",
                StudioIcon { name: StudioIconName::UnboundValue, size: 10 }
            }
            "unmap all"
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
    /// The arm's counterpart ring: an armed assign points HERE next.
    #[props(default)]
    attention: bool,
    /// Frames are flowing — attention animations may run.
    #[props(default)]
    animate: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let ui = use_hook(try_consume_context::<PatchingUi>);
    // The strip reports which presentation its measured box chose, so the
    // fact line can name it (the spike's mode chip) — and so a walk-up gate
    // can see where the 7px threshold actually falls.
    let presentation = use_signal(StripPresentation::default);
    let base = if primary { SECTION_PRIMARY } else { SECTION };
    let class = if attention {
        format!("{base} ux-arm-attention")
    } else {
        base.to_string()
    };
    let is_armed = matches!(armed, Some(ArmedVerb::Assign));
    let swap_armed = matches!(armed, Some(ArmedVerb::Swap(_)));
    let arm_class = if is_armed {
        if animate {
            format!("{STEP_ARMED} ux-arm-pulse")
        } else {
            STEP_ARMED.to_string()
        }
    } else {
        STEP_ARM.to_string()
    };
    let swap_class = if swap_armed {
        if animate {
            format!("{STEP_ARMED} ux-arm-pulse")
        } else {
            STEP_ARMED.to_string()
        }
    } else {
        STEP.to_string()
    };
    // The object side holds a LINKABLE object: this section invites. The
    // test is the arm's own (Q11) — an auto-mapped fixture's object has no
    // link to offer, and a fixture CARD is not an object at all (Q8), so
    // neither gets an invitation it could not complete.
    let unmapped_object = match (&selection, &output) {
        (Some(target), None) if is_armable(&surface, target) => Some(target.clone()),
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
                    let object = object_view(&surface, selection.as_ref());
                    let frame = output_frame(&surface, output.node);
                    let (assumed, decode_note) = decode_line(
                        &surface,
                        &output,
                        object.as_ref(),
                        frame,
                    );
                    let colors = output_strip_colors(frame, output.span, assumed);
                    let has_signal = !colors.is_empty();
                    let object_target = object.map(|object| object.target);
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
                                                let subject = assign_subject_target(
                                                    &surface,
                                                    target,
                                                );
                                                if let Some(lamp) = port_next_free(entry, key) {
                                                    dispatch_assign(
                                                        &on_action,
                                                        &surface,
                                                        &subject,
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
                                // `selected` mirrors the bound value onto
                                // each option: the select's own `value` is
                                // applied before the options mount, so it
                                // alone cannot restore the selection (see
                                // select_mirror_lint).
                                if selected_port.is_empty() {
                                    option { value: "", selected: true, "pick a port…" }
                                }
                                for (value , label) in ports.iter().cloned() {
                                    option {
                                        key: "{value}",
                                        value: "{value}",
                                        selected: value == selected_port,
                                        "{label}"
                                    }
                                }
                            }
                        }
                        // The port's WHOLE extent as published, with the
                        // selection's window marked over it — the window is
                        // a DOM overlay so it survives whichever
                        // presentation the strip below it chose.
                        div { class: "{STRIP}", "data-patch-strip": "output",
                            LampStrip { colors, presentation }
                            if let Some(style) = window_style {
                                div {
                                    class: "tw:absolute tw:inset-y-0 tw:rounded-[3px] tw:border tw:border-selection-border tw:bg-selection-bg",
                                    style: "{style}",
                                }
                            }
                        }
                        div { class: "tw:flex tw:flex-wrap tw:items-baseline tw:gap-2 tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                            span { "wire {span_start + 1}" }
                            // A1, stated where it is assumed: the free
                            // stretches of this wire are read under a lamp
                            // type nobody declared, so the panel names the
                            // one it used and where it got it.
                            span { class: "tw:text-status-warning-foreground", "{decode_note}" }
                            if has_signal {
                                span { "{presentation().label()}" }
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
                                {keyed("‹‹ ×10", "[")}
                            }
                            button {
                                class: "{STEP}",
                                title: if free { "Walk the window back one lamp ([)" } else { "Shift the run back one lamp" },
                                onclick: {
                                    let nudge = nudge.clone();
                                    move |_| nudge(-1)
                                },
                                {keyed("‹", "[")}
                            }
                            button {
                                class: "{STEP}",
                                title: if free { "Walk the window on one lamp (])" } else { "Shift the run on one lamp" },
                                onclick: {
                                    let nudge = nudge.clone();
                                    move |_| nudge(1)
                                },
                                {keyed("›", "]")}
                            }
                            button {
                                class: "{STEP}",
                                title: if free { "Walk the window on ten lamps (])" } else { "Shift the run on ten lamps" },
                                onclick: {
                                    let nudge = nudge.clone();
                                    move |_| nudge(10)
                                },
                                {keyed("×10 ››", "]")}
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
                                    {keyed("narrow", "-")}
                                }
                                button {
                                    class: "{STEP}",
                                    title: "Widen the segment one lamp (=)",
                                    onclick: {
                                        let resize = resize.clone();
                                        move |_| resize(1)
                                    },
                                    {keyed("widen", "=")}
                                }
                            }
                            if matches!(selection, Some(UiPatchTarget::Port { .. })) || swap_armed {
                                span { class: "tw:w-2" }
                                button {
                                    class: "{swap_class}",
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
                                    {keyed("swap", "s")}
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
                                {keyed("next free", "m")}
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
                                    class: "{arm_class}",
                                    title: "Arm the assign — the next port click links it (a)",
                                    onclick: {
                                        let surface = surface.clone();
                                        let selection = selection.clone();
                                        move |_| {
                                            if let Some(ui) = ui {
                                                let mut armed = ui.armed;
                                                arm_assign(&surface, &selection, &mut armed);
                                                // At the mobile fold the ports
                                                // live in the Outputs panel —
                                                // bring it up (round 3, #6);
                                                // picking there completes and
                                                // dismisses it.
                                                if armed.peek().is_some() && at_mobile_fold() {
                                                    let mut summon = ui.summon_outputs;
                                                    summon.set(true);
                                                }
                                            }
                                        }
                                    },
                                    // The label never changes (round 3): armed
                                    // = pulse + the counterpart ring.
                                    {keyed("assign", "a")}
                                }
                            }
                            // The destinations as EXPLAINING CARDS (#6):
                            // occupancy on every option; picking writes (the
                            // ratified picker rule) at the port's next free
                            // lamp. At the fold the summoned Outputs panel is
                            // the picker instead.
                            div { class: "tw:max-[820px]:hidden",
                                OptionCards {
                                    label: "or pick a port".to_string(),
                                    options: port_cards(&surface),
                                    on_pick: {
                                        let surface = surface.clone();
                                        move |value: String| {
                                            let Some((node, key)) = parse_port_key(&value) else {
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
                                            let subject = assign_subject_target(&surface, &target);
                                            if let Some(lamp) = port_next_free(entry, key)
                                                && dispatch_assign(
                                                    &on_action,
                                                    &surface,
                                                    &subject,
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
        ControlExtent, ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan,
        UiControlSampleFormat, UiFixturePatch, UiPatchBay, UiPatchChasePreview, UiPatchInstance,
        UiPatchPort, UiPatchSurfaceFixture, UiPatchSurfaceOutput,
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
    /// waiting — the shape both flows walk. MANUAL, because that is the
    /// mode the walk-up grammar exists in (Q11): auto-mapped fixtures place
    /// their own objects and get the lean panel instead.
    fn half_patched_surface() -> UiPatchSurface {
        let mapped = cell("2:0", 0, 0, 30, true);
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: dome_node(),
                label: "dome".to_string(),
                address: Some("/rig.module/dome.fixture".to_string()),
                manual_flow: true,
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
                ("flow".to_string(), "manual".to_string()),
            ]
        );
        assert_eq!(
            output_view(&surface, Some(&selection)),
            None,
            "an unmapped object has no wire to derive"
        );
        assert!(target_is_unmapped(&surface, &selection), "so it invites");
    }

    /// The fixture's flow flag reaches the object section (P5b): it is a
    /// FIXTURE fact carried on every object of that fixture, whatever grain
    /// the selection named, because it is the answer to "why did unmapping
    /// this put it straight back on the wire?".
    #[test]
    fn the_object_section_carries_its_fixtures_flow_flag() {
        let mut surface = half_patched_surface();
        let selection = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/2".to_string(),
        };
        let object = object_view(&surface, Some(&selection)).expect("object");
        assert!(object.manual);
        assert_eq!(object.fixture, dome_node(), "the flow verbs' subject");
        assert_eq!(
            object_facts(&object)[2],
            ("flow".to_string(), "manual".to_string())
        );

        // A document with no flag is AUTO, and the fact says so.
        surface.fixtures[0].manual_flow = false;
        let object = object_view(&surface, Some(&selection)).expect("object");
        assert!(!object.manual);
        assert_eq!(
            object_facts(&object)[2],
            ("flow".to_string(), "auto-mapped".to_string())
        );
        surface.fixtures[0].manual_flow = true;
        // The same flag on a mapped sibling — it belongs to the fixture, not
        // to whichever object happens to be selected.
        let mapped = object_view(
            &surface,
            Some(&UiPatchTarget::Cell {
                id: "2:0".to_string(),
            }),
        )
        .expect("object");
        assert!(mapped.mapped && mapped.manual);
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
    ///
    /// A port row carries its OCCUPANCY (round 3, #6) — where the free space
    /// is and how much of it — because a destination the user cannot judge is
    /// not a choice. The 1-based lamp in the phrase is the chips' convention,
    /// so "30 free @ 31" is the tail of a 60-lamp port whose first half is
    /// spoken for.
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
        assert_eq!(ports[0].1, "1 · IO18 · port 0 · 30 free @ 31");
        assert_eq!(parse_port_key(&ports[0].0), Some((output_node(), 0)));
        assert_eq!(parse_port_key("nonsense"), None);
    }

    /// A published wire whose lamp `n` is saturated in channel `n % 3`, with
    /// `declared` lamps carrying `order` and the rest declared by nobody —
    /// the shape of a half-patched port.
    fn wire_frame(lamps: u32, declared: u32, order: ColorOrder) -> UiControlProductPreview {
        let mut bytes = Vec::with_capacity(lamps as usize * 6);
        for lamp in 0..lamps {
            let mut rgb = [0_u16; 3];
            rgb[(lamp % 3) as usize] = 65535;
            for sample in rgb {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        UiControlProductPreview {
            revision: 1,
            extent: ControlExtent::new(1, lamps * 3),
            sample_format: UiControlSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: vec![ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: declared * 3,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: declared,
                        color_order: order,
                    },
                }],
            },
            display_layout: None,
            bytes: bytes.into(),
        }
    }

    /// The same surface, live: sector 1's 30 lamps are declared GRB, the
    /// free half of the port is declared by nobody.
    fn live_surface() -> UiPatchSurface {
        let mut surface = half_patched_surface();
        let frame = wire_frame(60, 30, ColorOrder::Grb);
        surface.fixtures[0].patch.frame = Some(frame.clone());
        surface.outputs[0].bay.frame = Some(frame);
        surface
    }

    /// A chase the CONTROLLER painted (Q9): the strip renders the preview's
    /// colors through the ordinary linear -> sRGB transfer and invents
    /// nothing. The very same `chase_preview` reaches the canvas sprites, so
    /// this assertion is what pins the two views to one picture.
    #[test]
    fn the_object_strip_paints_the_core_computed_chase() {
        let mut surface = half_patched_surface();
        let selection = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/2".to_string(),
        };
        let object = object_view(&surface, Some(&selection)).expect("object");

        // No preview on the surface: nothing honest to draw, and the strip
        // does NOT fall back to a chase of its own (the P5 behaviour Q9
        // deleted).
        assert!(
            object_strip_colors(&surface, &object).is_empty(),
            "the panel never computes a chase itself any more"
        );

        surface.chase_preview = Some(UiPatchChasePreview {
            node: dome_node(),
            start: 30,
            colors: (0..30_u16).map(|lamp| [lamp * 2000, 0, 65535]).collect(),
            phase: 0.25,
        });
        let colors = object_strip_colors(&surface, &object);
        assert_eq!(colors.len(), 30, "the object's own lamps, all of them");
        assert_eq!(colors[0], srgb8([0, 0, 65535]));
        assert_eq!(colors[29], srgb8([29 * 2000, 0, 65535]));

        // A preview for ANOTHER fixture is not this object's picture.
        surface.chase_preview.as_mut().expect("preview").node = NodeId::new(99);
        assert!(object_strip_colors(&surface, &object).is_empty());
    }

    /// A MAPPED object's strip is its own wire, decoded back through its
    /// runs — which is how the ENGINE's chase reaches this strip without the
    /// client painting anything. A reversed run reads object-continuous.
    #[test]
    fn the_object_strip_reads_a_mapped_objects_own_wire() {
        let surface = live_surface();
        let selection = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/1".to_string(),
        };
        let object = object_view(&surface, Some(&selection)).expect("object");
        assert!(object.mapped && object.reversed);
        let colors = object_strip_colors(&surface, &object);
        assert_eq!(colors.len(), 30);
        // The run is laid end-first, so object lamp 0 is wire lamp 29 —
        // saturated in channel 29 % 3 = 2, read under GRB as blue.
        assert_eq!(colors[0], [0, 0, 255]);
        // Object lamp 1 is wire 28 (channel 1) — GRB reads that as red.
        assert_eq!(colors[1], [255, 0, 0]);
        // And with no frame there is nothing honest to draw at all.
        let dark = object_strip_colors(&half_patched_surface(), &object);
        assert!(dark.is_empty(), "no frame, no invented lamps");
    }

    /// A fixture driving TWO boxes decodes each run against the wire that
    /// run landed on. `UiFixturePatch::frame` carries only the FIRST
    /// output's — enough for the bay's own face, and a lie on a panel
    /// showing the object the user just selected (the mini dome, whose
    /// sectors are split across both of its boxes, reads wrong without
    /// this).
    #[test]
    fn a_split_fixture_reads_each_run_from_its_own_output() {
        let mut surface = live_surface();
        // A second box, all-red, carrying sector 2's run at wire 0.
        let second = output_node().0 + 1;
        let mut red = wire_frame(60, 30, ColorOrder::Rgb);
        red.bytes = std::iter::repeat_n([65535_u16, 0, 0], 60)
            .flatten()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<u8>>()
            .into();
        let run = cell("2:1", 30, 0, 30, false);
        surface.fixtures[0].patch.cells.push(run.clone());
        surface.fixtures[0].instances[1].placed = true;
        surface.outputs.push(UiPatchSurfaceOutput {
            node: NodeId::new(second),
            label: "out_b".to_string(),
            name: Some("Box 2".to_string()),
            bay: UiPatchBay {
                ports: vec![UiPatchPort {
                    key: 0,
                    pin_label: "IO14".to_string(),
                    start: 0,
                    lamps: 60,
                    cells: vec![run],
                }],
                frame: Some(red),
                ..Default::default()
            },
            ..Default::default()
        });

        let selection = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/2".to_string(),
        };
        let object = object_view(&surface, Some(&selection)).expect("object");
        assert!(object.mapped, "sector 2 now sits on the second box");
        let colors = object_strip_colors(&surface, &object);
        assert!(
            colors.iter().all(|color| *color == [255, 0, 0]),
            "every lamp reads the SECOND box's wire, not the first's: {colors:?}"
        );
    }

    /// The output strip is the WHOLE port, free lamps included — the free
    /// stretch is the thing the walk-up flow is about, and the engine paints
    /// its breath into the published bytes before publishing.
    #[test]
    fn the_output_strip_covers_the_whole_port_extent() {
        let surface = live_surface();
        let free = UiPatchTarget::Segment {
            node: output_node(),
            port: 0,
            start: 30,
            lamps: 30,
        };
        let output = output_view(&surface, Some(&free)).expect("window");
        let frame = output_frame(&surface, output.node);
        let colors = output_strip_colors(frame, output.span, ColorOrder::Rgb);
        assert_eq!(colors.len(), 60, "every lamp of the port, mapped or not");
        // Wire 0 is declared GRB (channel 0 saturated → green); wire 30 is
        // declared by nobody, so the assumption decides (channel 0 → red).
        assert_eq!(colors[0], [0, 255, 0]);
        assert_eq!(colors[30], [255, 0, 0]);
        assert_eq!(
            output_strip_colors(None, output.span, ColorOrder::Rgb),
            Vec::<[u8; 3]>::new(),
            "no frame = no strip, not a black one"
        );
    }

    /// A1 out loud: the mapped window names its owner's lamp type, a free
    /// segment names the object that would land there (D6), and a wire with
    /// no frame says so instead of decoding nothing into a claim.
    #[test]
    fn the_decode_line_names_the_lamp_type_and_its_source() {
        let surface = live_surface();
        let mapped = UiPatchTarget::Instance {
            node: dome_node(),
            path: "/sector/1".to_string(),
        };
        let output = output_view(&surface, Some(&mapped)).expect("derived window");
        let object = object_view(&surface, Some(&mapped));
        let frame = output_frame(&surface, output.node);
        assert_eq!(
            decode_line(&surface, &output, object.as_ref(), frame),
            (ColorOrder::Grb, "decoded as GRB — sector 1".to_string())
        );

        let free = UiPatchTarget::Segment {
            node: output_node(),
            port: 0,
            start: 30,
            lamps: 30,
        };
        let output = output_view(&surface, Some(&free)).expect("window");
        assert_eq!(
            decode_line(&surface, &output, None, frame),
            (ColorOrder::Grb, "decoded as GRB — sector 2".to_string()),
            "the free stretch reads under the object about to take it"
        );

        assert_eq!(
            decode_line(&surface, &output, None, None),
            (ColorOrder::Rgb, "no signal on this wire yet".to_string())
        );
    }

    /// Q8: a whole-fixture selection is a CARD, not an object — fixture-grain
    /// facts and verbs, no chase, no window to derive.
    #[test]
    fn a_fixture_selection_is_a_card_not_an_object() {
        let surface = half_patched_surface();
        let selection = UiPatchTarget::Fixture { node: dome_node() };

        let card = fixture_card(&surface, Some(&selection)).expect("the card fills");
        assert_eq!(card.name, "dome");
        assert_eq!(card.context, "/rig.module/dome.fixture");
        assert_eq!((card.lamps, card.objects, card.placed), (60, 2, 1));
        assert!(card.manual);
        assert_eq!(
            fixture_facts(&card),
            vec![
                ("lamps".to_string(), "60".to_string()),
                ("objects".to_string(), "1/2 placed".to_string()),
                ("flow".to_string(), "manual".to_string()),
            ]
        );

        assert_eq!(
            object_view(&surface, Some(&selection)),
            None,
            "the fixture is not an object — the card takes the section"
        );
        assert_eq!(
            output_view(&surface, Some(&selection)),
            None,
            "and it names no single wire window"
        );

        // The scarf (Q8's exception): no object table, so the fixture IS
        // its own object and keeps the object treatment.
        let mut scarf = half_patched_surface();
        scarf.fixtures[0].instances.clear();
        assert_eq!(fixture_card(&scarf, Some(&selection)), None);
        let object = object_view(&scarf, Some(&selection)).expect("the strand is an object");
        assert!(object.whole_fixture, "so it wears the flow selector itself");
        assert_eq!(object.lamps, 60);
    }

    /// The flow selector's two option ids are the two states
    /// [`flow_label`] names — one vocabulary, so a pick cannot mean
    /// something the fact line does not say.
    #[test]
    fn the_flow_selectors_ids_are_the_flow_states() {
        assert_eq!(flow_label(true), FLOW_MANUAL);
        assert_ne!(flow_label(false), FLOW_MANUAL);
        assert_eq!(FLOW_AUTO, "auto");
    }

    /// Q11: an AUTO fixture's objects are not offered a link — not by the
    /// pickers, not by the sizing rule that draws free segments.
    #[test]
    fn an_auto_fixtures_objects_are_never_offered_a_link() {
        use crate::app::patch::verb_ui::next_unmapped_lamps;

        let mut surface = half_patched_surface();
        assert_eq!(
            unmapped_objects(&surface).len(),
            1,
            "sector 2, while manual"
        );
        assert_eq!(next_unmapped_lamps(&surface), Some(30));

        surface.fixtures[0].manual_flow = false;
        assert!(
            unmapped_objects(&surface).is_empty(),
            "auto-mapped objects place themselves — the picker offers none"
        );
        assert_eq!(
            next_unmapped_lamps(&surface),
            None,
            "and none of them sizes a free segment"
        );
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
