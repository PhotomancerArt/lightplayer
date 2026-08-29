//! The Patching view's center (R5): the ONE project canvas with
//! patch-selection highlighting, the #409 verb set as a keyboard grammar
//! (re-housed from the interim `/patch` page through
//! `app::patch::verb_ui`), and the selection pulse — the first consumer
//! of #411's `PatchPulseOp` (select pulses the live sim/hardware,
//! deselect and view-exit clear).
//!
//! Its bottom region is THE patch panel (pass 2, P4:
//! `app::patch::patch_panel`) — the verbs' visible home, the invitations,
//! and the keys row that replaced the help overlay. The toolbar above
//! keeps only history and status (D4).
//!
//! Same canvas, different furniture (the one-project-canvas ADR): no
//! dive here — the authored tree is Mapping's activity; this center
//! reads the resolved surface and writes patches through the verbs.
//!
//! It also owns the ARM GRAMMAR (pass 2, P3): linking is explicit — `a`
//! arms an assign, `s` a swap, and the next counterpart CLICK (in the
//! Outputs dock, the Tree, or on a sprite) completes it as one real,
//! undoable write. Plain clicks only ever select. `m` walks the next free
//! segment and keeps the arm, `[`/`]` and `-`/`=` nudge that window
//! (selection only), and Esc is a ladder: disarm, then deselect.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeId, PatchPulseOp, PatchPulseSubject, PatchVerbKind, PatchVerbWindow, ProjectController,
    ProjectEditorOp, ProjectEditorView, UiAction, UiPatchSurface, UiPatchTarget,
};

use super::arrange::{PackSlots, ProjectCanvasHost, refresh_pack_slots};
use super::toolbar::{StatusKind, ToolbarGroup, ToolbarItem, ToolbarStrip};
use super::{mapping_assets, prefetch_editor_meta};
use crate::app::patch::patch_panel::PatchPanel;
use crate::app::patch::verb_ui::{
    dispatch_assign, dispatch_verb, next_free_segment, port_window, resize_segment,
    selection_stride, shift_segment, target_is_unmapped,
};
use crate::app::workbench::panels::prefetch_bodies;

/// Which patch verb is ARMED, if any — the generalized swap arm (R3's
/// selection model v3: linking is explicit, plain clicks never write).
///
/// `Assign` carries NO payload on purpose: both ends resolve at COMPLETION
/// from the current selection plus the thing clicked. The selection moves
/// under a live arm (`m` advances to the next free segment and keeps it),
/// so an arm that captured its counterpart at arming time would go stale
/// on the second lap of the walk-up loop.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ArmedVerb {
    /// The next counterpart click links the selection to what it hits.
    Assign,
    /// The next port click swaps that port with this armed window.
    Swap(PatchVerbWindow),
}

impl ArmedVerb {
    /// The armed sentence the PANEL shows — the armed verb NAMES itself, so
    /// the user is never guessing which gesture the next click completes.
    pub(crate) fn banner(&self) -> &'static str {
        match self {
            Self::Assign => {
                "Assign armed — click the counterpart (an object, or a port / free segment) to link it (Esc cancels)"
            }
            Self::Swap(_) => "Swap armed — click the other port in the Outputs panel (Esc cancels)",
        }
    }
}

/// Cross-dock patching UI state, provided by the workbench frame: verbs arm
/// here (`a` / `s` in the center) and complete on a counterpart click in
/// the Outputs or Tree dock — the same two-sided gesture the interim page
/// carried inside one component, now spanning the frame. The context is
/// the precedent the page itself set (`HoveredPatchCell`).
#[derive(Clone, Copy)]
pub(crate) struct PatchingUi {
    pub armed: Signal<Option<ArmedVerb>>,
    /// The free-segment size the user nudged with `-`/`=`, kept across `m`
    /// (the ruling's "size override"). `None` = size every segment by the
    /// next unmapped object, the walk-up default.
    pub segment_size: Signal<Option<u32>>,
    /// A one-shot request to SUMMON the Outputs panel (the mobile fold's
    /// full-screen pick surface): the object-first invitation sets it when
    /// arming below the fold — the counterpart the user must now click
    /// lives in that panel, so the panel comes to them (G1 round 3, #6).
    /// The workbench consumes and resets it.
    pub summon_outputs: Signal<bool>,
}

/// Map the shared patch selection onto the pulse's subject vocabulary:
/// fixture-side targets pulse in fixture numbering (the controller maps
/// them through the placements), wire-side targets in wire numbering.
/// `Module` — and no selection — clear the pulse.
///
/// This resolves NUMBERS only. Which space each target counts in and which
/// light language it deserves are core's D9 matrix
/// ([`UiPatchTarget::pulse_language`]), applied by
/// [`UiPatchTarget::pulse_subject`] — so the UI cannot name a selection in
/// the wrong tongue.
fn pulse_subject(surface: &UiPatchSurface, target: &UiPatchTarget) -> Option<PatchPulseSubject> {
    let (node, range) = match target {
        UiPatchTarget::Fixture { node } => (*node, None),
        UiPatchTarget::Instance { node, path } => {
            let fixture = surface.fixtures.iter().find(|f| f.node == *node)?;
            let instance = fixture
                .instances
                .iter()
                .find(|instance| instance.path == *path)?;
            (*node, Some((instance.start, instance.lamps)))
        }
        UiPatchTarget::Range { node, start, count } => (*node, count.map(|count| (*start, count))),
        UiPatchTarget::Cell { id } => {
            let node: u32 = id.split(':').next()?.parse().ok()?;
            let node = NodeId::new(node);
            let fixture = surface.fixtures.iter().find(|f| f.node == node)?;
            let cell = fixture.patch.cells.iter().find(|cell| cell.id == *id)?;
            (node, Some((cell.source_start, cell.lamps)))
        }
        UiPatchTarget::Port { node, port } => {
            let output = surface.outputs.iter().find(|output| output.node == *node)?;
            let port = output.bay.ports.iter().find(|p| p.key == *port)?;
            (*node, Some((port.start, port.lamps)))
        }
        // A segment is already a wire window; its port is looked up so a
        // stale one pulses nothing, and the window is clipped to the port
        // rather than bleeding into its neighbour.
        UiPatchTarget::Segment {
            node,
            port,
            start,
            lamps,
        } => {
            let output = surface.outputs.iter().find(|output| output.node == *node)?;
            let port = output.bay.ports.iter().find(|p| p.key == *port)?;
            let first = (*start).max(port.start);
            let end = start
                .saturating_add(*lamps)
                .min(port.start.saturating_add(port.lamps));
            (*node, Some((first, end.saturating_sub(first))))
        }
        UiPatchTarget::Output { node } => (*node, None),
        // `pulse_subject` already returns None for these.
        UiPatchTarget::Module { .. } => return None,
    };
    target.pulse_subject(node, range)
}

fn send_pulse(on_action: &EventHandler<UiAction>, subjects: Vec<PatchPulseSubject>) {
    on_action.call(UiAction::from_op(
        ProjectController::NODE_ID,
        PatchPulseOp { subjects },
    ));
}

fn select(on_action: &EventHandler<UiAction>, target: Option<UiPatchTarget>) {
    on_action.call(UiAction::from_op(
        lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
        ProjectEditorOp::PatchSelect {
            selection: lpa_studio_core::UiSelection::from_option(target),
        },
    ));
}

/// What a Patching keydown asks for — the pure half of the grammar, so
/// the key → verb decision is unit-testable apart from the DOM plumbing
/// (the window listener in [`super::hotkeys`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatchKeyAction {
    /// The esc ladder: rung 1 disarms, rung 2 clears the selection (and
    /// with it the nudged segment size).
    Escape,
    Reverse,
    Rotate {
        steps: i32,
    },
    ArmAssign,
    ArmSwap,
    /// `m`: advance to the next free segment, KEEPING the arm.
    NextSegment,
    /// `[` / `]`: walk the free segment — selection only, never a write.
    ShiftSegment {
        delta: i32,
    },
    /// `-` / `=`: narrow/widen the free segment; creates the size
    /// override `m` then keeps.
    ResizeSegment {
        delta: i32,
    },
    Undo,
    Redo,
}

/// The Patching grammar's key table: `r` reverse · `;`/`'` rotate ∓/± ·
/// `a` arm assign · `s` arm swap · `m` next free segment · `[` `]` shift ·
/// `-` `=` resize · Escape ladder · ⌘Z/⌘⇧Z undo/redo. `key` is the DOM
/// `KeyboardEvent.key` string; ⌘⇧Z arrives as "Z", so the meta match is
/// case-insensitive (the Mapping center documents the same quirk).
pub(crate) fn patch_key_action(key: &str, meta: bool, shift: bool) -> Option<PatchKeyAction> {
    if meta {
        if key.eq_ignore_ascii_case("z") {
            return Some(if shift {
                PatchKeyAction::Redo
            } else {
                PatchKeyAction::Undo
            });
        }
        return None;
    }
    Some(match key {
        "Escape" => PatchKeyAction::Escape,
        "r" => PatchKeyAction::Reverse,
        ";" => PatchKeyAction::Rotate { steps: -1 },
        "'" => PatchKeyAction::Rotate { steps: 1 },
        "a" => PatchKeyAction::ArmAssign,
        "s" => PatchKeyAction::ArmSwap,
        "m" => PatchKeyAction::NextSegment,
        "[" => PatchKeyAction::ShiftSegment { delta: -1 },
        "]" => PatchKeyAction::ShiftSegment { delta: 1 },
        "-" => PatchKeyAction::ResizeSegment { delta: -1 },
        "=" => PatchKeyAction::ResizeSegment { delta: 1 },
        _ => return None,
    })
}

/// The patching activity's toolbar, SLIMMED (D4): history and the counts
/// readout. The verbs themselves moved into the panel's transport rows —
/// the controls now sit beside the thing they act on, which is what a
/// walk-up user reads. Every verb is still a hotkey (the view-scoped
/// window listener below — dock clicks no longer kill the keys).
fn patch_toolbar(surface: &UiPatchSurface) -> Vec<ToolbarGroup> {
    let verb = |id: &'static str, label: &str, title: &str, active: bool, enabled: bool| {
        ToolbarItem::Button {
            id,
            icon: None,
            label: Some(label.to_string()),
            title: title.to_string(),
            active,
            enabled,
        }
    };
    let placed: usize = surface
        .fixtures
        .iter()
        .flat_map(|fixture| &fixture.instances)
        .filter(|instance| instance.placed)
        .count();
    let instances: usize = surface
        .fixtures
        .iter()
        .map(|fixture| fixture.instances.len())
        .sum();
    vec![
        ToolbarGroup {
            id: "patch-label",
            trailing: false,
            items: vec![ToolbarItem::Status {
                text: "Patching".to_string(),
                kind: StatusKind::Label,
            }],
        },
        ToolbarGroup {
            id: "patch-history",
            trailing: false,
            items: vec![
                verb(
                    "patch-undo",
                    "undo",
                    "Undo the last patch edit (⌘Z)",
                    false,
                    true,
                ),
                verb("patch-redo", "redo", "Redo (⌘⇧Z)", false, true),
            ],
        },
        ToolbarGroup {
            id: "patch-status",
            trailing: true,
            items: vec![ToolbarItem::Status {
                text: format!("{placed}/{instances} placed"),
                kind: StatusKind::Mono,
            }],
        },
    ]
}

/// The Patching view's center: toolbar + the one canvas, verbs on keys.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PatchingShellCenter(
    surface: Option<UiPatchSurface>,
    selection: lpa_studio_core::UiSelection,
    /// The full editor view — the canvas resolves fixture map2d bodies out
    /// of the snapshot's node views, exactly like the Mapping center.
    project_editor: ProjectEditorView,
    /// The workbench-owned auto-pack slots, SHARED with the Mapping
    /// center (G1 round 1: one conceptual space, one slot truth).
    pack_slots: Signal<PackSlots>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let PatchingUi {
        mut armed,
        mut segment_size,
        summon_outputs: _,
    } = use_context::<PatchingUi>();
    // The pulse's echo guard: dispatch only when the mapped subject
    // actually changes (sweep-with-clear lives in the controller; this
    // just keeps renders from re-sending the same subject).
    let mut pulsed = use_signal(Vec::<PatchPulseSubject>::new);
    // View-exit clears the pulse: a highlight pointing at a selection the
    // user can no longer see is actively misleading (#411's own rule).
    use_drop({
        let on_action = on_action;
        move || send_pulse(&on_action, Vec::new())
    });
    let Some(surface) = surface else {
        return rsx! {
            div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:items-center tw:justify-center",
                p { class: "tw:m-0 tw:max-w-[360px] tw:text-center tw:text-xs tw:text-dim-foreground",
                    "Nothing to patch yet — bind an output to a control bus and the patching view fills in."
                }
            }
        };
    };
    prefetch_editor_meta(&on_action, &surface);
    // Both body kinds, all fixtures: the map2d bodies draw the sprites,
    // the patch bodies are what the verbs transform — without them every
    // edit blocks with "still loading" (the interim page's lesson).
    prefetch_bodies(&on_action, &surface);
    let (bodies, _) = mapping_assets(&project_editor);
    // Sticky auto-pack slots, the WORKBENCH-owned store both views share
    // (one conceptual space — and since G1 round 1, one signal, so the
    // views can never pack the same fixture two ways).
    let mut pack_slots = pack_slots;
    let refreshed = refresh_pack_slots(&surface, &bodies, &pack_slots.peek());
    if let Some(next) = refreshed {
        pack_slots.set(next);
    }
    let pack = pack_slots.read().clone();
    // The pulse union (P2): every selected target maps to a subject; a
    // multi-selection is breath-only by the sibling invariant, and the
    // controller merges same-output spans.
    let subjects: Vec<PatchPulseSubject> = selection
        .targets()
        .iter()
        .filter_map(|target| pulse_subject(&surface, target))
        .collect();
    if *pulsed.peek() != subjects {
        pulsed.set(subjects.clone());
        send_pulse(&on_action, subjects);
    }
    // The keyboard grammar rides a view-scoped WINDOW listener (see
    // `hotkeys.rs`): the walk-up loop's dock clicks — a port free-run, a
    // tree row — must never kill the keys, which is exactly what the old
    // center-div `onkeydown` + `tabindex` did the moment focus left the
    // div. The key → verb decision is `patch_key_action`, pure and
    // unit-tested; this closure is only the executor.
    {
        let surface = surface.clone();
        // The verbs and the arm are single-subject by ruling — a
        // multi-selection is not an armable end and has no one stride.
        let selection = selection.single().cloned();
        let on_action = on_action;
        super::hotkeys::use_window_keydown(move |event: web_sys::KeyboardEvent| {
            let meta = event.meta_key() || event.ctrl_key();
            let Some(action) = patch_key_action(&event.key(), meta, event.shift_key()) else {
                return;
            };
            match action {
                PatchKeyAction::Escape => {
                    if armed.peek().is_some() {
                        armed.set(None);
                    } else {
                        segment_size.set(None);
                        select(&on_action, None);
                    }
                }
                PatchKeyAction::Reverse => {
                    dispatch_verb(&on_action, &surface, &selection, PatchVerbKind::Reverse);
                }
                PatchKeyAction::Rotate { steps } => {
                    let stride = selection_stride(&surface, &selection);
                    dispatch_verb(
                        &on_action,
                        &surface,
                        &selection,
                        PatchVerbKind::Rotate { steps, stride },
                    );
                }
                PatchKeyAction::ArmAssign => arm_assign(&surface, &selection, &mut armed),
                PatchKeyAction::ArmSwap => arm_swap(&surface, &selection, &mut armed),
                PatchKeyAction::NextSegment => {
                    if let Some(next) =
                        next_free_segment(&surface, selection.as_ref(), *segment_size.peek())
                    {
                        select(&on_action, Some(next));
                    }
                }
                PatchKeyAction::ShiftSegment { delta } => {
                    if let Some(target) = selection.as_ref()
                        && let Some(next) = shift_segment(&surface, target, delta)
                    {
                        select(&on_action, Some(next));
                    }
                }
                PatchKeyAction::ResizeSegment { delta } => {
                    if let Some(target) = selection.as_ref()
                        && let Some(next) = resize_segment(&surface, target, delta)
                    {
                        // Resizing is what CREATES the override `m` then
                        // keeps.
                        if let UiPatchTarget::Segment { lamps, .. } = &next {
                            segment_size.set(Some(*lamps));
                        }
                        select(&on_action, Some(next));
                    }
                }
                PatchKeyAction::Undo => {
                    dispatch_verb(&on_action, &surface, &selection, PatchVerbKind::Undo);
                }
                PatchKeyAction::Redo => {
                    dispatch_verb(&on_action, &surface, &selection, PatchVerbKind::Redo);
                }
            }
        });
    }
    let armed_verb = armed.read().clone();
    let groups = patch_toolbar(&surface);
    let on_item = {
        let surface = surface.clone();
        let single = selection.single().cloned();
        let on_action = on_action;
        move |id: &'static str| match id {
            "patch-undo" => dispatch_verb(&on_action, &surface, &single, PatchVerbKind::Undo),
            "patch-redo" => dispatch_verb(&on_action, &surface, &single, PatchVerbKind::Redo),
            _ => {}
        }
    };
    rsx! {
        div {
            class: "tw:flex tw:min-h-0 tw:flex-1 tw:flex-col tw:outline-none",
            ToolbarStrip { groups, on_item }
            div { class: "tw:relative tw:flex tw:min-h-0 tw:flex-1 tw:flex-col",
                if !surface.editor_meta_loaded {
                    div { class: "tw:flex tw:flex-1 tw:items-center tw:justify-center",
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Loading the arrangement…" }
                    }
                } else {
                    // The one canvas, patching furniture: no dive, the
                    // shared pack slots, selection rings from the SAME
                    // patch selection the panels dispatch. The host
                    // carries the fit-reconcile stamp (`data-fit-viewport`)
                    // the story capture's ready-gate requires.
                    ProjectCanvasHost {
                        surface: surface.clone(),
                        bodies,
                        selection: selection.clone(),
                        pack,
                        // The guide invariant, default-on: every sprite
                        // glows with its live output colors (D2=b —
                        // patched vs unpatched at a glance).
                        live_sprites: true,
                        // A sprite completes an armed assign, like its row.
                        patch_verbs: true,
                        on_action,
                    }
                }
            }
            // THE panel (D8): always the center's bottom region, empty
            // states included — never a dock, never a popover. It is a
            // sibling consumer of the ONE selection above it.
            PatchPanel {
                surface: surface.clone(),
                selection: selection.clone(),
                armed: armed_verb,
                on_action,
            }
        }
    }
}

/// Arm the swap from the selected port (`s`, or the panel's swap block); a
/// second call disarms — the key is a toggle, like the page's Escape rung.
pub(crate) fn arm_swap(
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    armed: &mut Signal<Option<ArmedVerb>>,
) {
    if armed.peek().is_some() {
        armed.set(None);
        return;
    }
    if let Some(UiPatchTarget::Port { node, port }) = selection
        && let Some(output) = surface.outputs.iter().find(|output| output.node == *node)
        && let Some(window) = port_window(output, *port)
    {
        armed.set(Some(ArmedVerb::Swap(window)));
    }
}

/// Can this selection start an assign? An UNMAPPED object on a MANUAL
/// fixture (whatever grain it was named at), or a free SEGMENT — the two
/// ends of the one link. A mapped thing arms nothing: there is no link to
/// make, and the ruling says it plain-reselects.
///
/// The mode gate is Q11's: an auto-mapped fixture flows its own unnamed
/// lamps onto the wire, so there is no link to arm there either — `a` on one
/// of its objects does nothing rather than arming a gesture the next click
/// could not complete. A whole FIXTURE never arms (Q8: the card is not an
/// object); the canvas now names objects directly, so nothing needs it to.
pub(crate) fn is_armable(surface: &UiPatchSurface, target: &UiPatchTarget) -> bool {
    match target {
        // A `Segment` only ever names FREE space (a mapped run selects as
        // its `Cell`, which speaks the fixture's language instead).
        UiPatchTarget::Segment { .. } => true,
        // The fixture card is a card, not an object — except for the scarf,
        // the count-only strand that has no objects to be a card ABOUT.
        UiPatchTarget::Fixture { node } => {
            fixture_is_manual(surface, *node)
                && surface
                    .fixtures
                    .iter()
                    .any(|fixture| fixture.node == *node && fixture.instances.is_empty())
                && target_is_unmapped(surface, target)
        }
        other => {
            target_fixture(other).is_some_and(|node| fixture_is_manual(surface, node))
                && target_is_unmapped(surface, other)
        }
    }
}

/// The fixture a fixture-side target names, when it names one.
fn target_fixture(target: &UiPatchTarget) -> Option<NodeId> {
    match target {
        UiPatchTarget::Fixture { node }
        | UiPatchTarget::Instance { node, .. }
        | UiPatchTarget::Range { node, .. } => Some(*node),
        // Cell ids are `node:output:source:wire` (the bay's format).
        UiPatchTarget::Cell { id } => Some(NodeId::new(id.split(':').next()?.parse().ok()?)),
        _ => None,
    }
}

/// Does this fixture's patch declare MANUAL flow (P5b)? Unknown fixtures are
/// not manual: manual is a claim a document makes.
fn fixture_is_manual(surface: &UiPatchSurface, node: NodeId) -> bool {
    surface
        .fixtures
        .iter()
        .any(|fixture| fixture.node == node && fixture.manual_flow)
}

/// Arm the ASSIGN (`a`, or the panel's invitation button): a toggle like
/// `s`, refused when the selection has nothing to link.
pub(crate) fn arm_assign(
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    armed: &mut Signal<Option<ArmedVerb>>,
) {
    if armed.peek().is_some() {
        armed.set(None);
        return;
    }
    if selection
        .as_ref()
        .is_some_and(|target| is_armable(surface, target))
    {
        armed.set(Some(ArmedVerb::Assign));
    }
}

/// What a FIXTURE-grain subject actually means to the assign arm.
///
/// Since Q10 the canvas names OBJECTS directly (a sprite click resolves its
/// lamp to the span that owns it), so this narrowing no longer runs from the
/// canvas. What still reaches it: the panel's pickers, the Outputs panel's
/// port-click completion, and the scarf — a fixture with no object table,
/// which is one strand and patches at the range grain its document can
/// actually hold.
///
/// A fixture WITH objects is a card, not a subject (Q8), and `is_armable`
/// refuses it before this is ever asked; the next-unmapped narrowing here
/// stays as the honest answer for any caller that hands one over anyway.
/// Every other target passes through unchanged.
pub(crate) fn assign_subject_target(
    surface: &UiPatchSurface,
    target: &UiPatchTarget,
) -> UiPatchTarget {
    let UiPatchTarget::Fixture { node } = target else {
        return target.clone();
    };
    let Some(fixture) = surface.fixtures.iter().find(|entry| entry.node == *node) else {
        return target.clone();
    };
    match fixture.instances.iter().find(|instance| !instance.placed) {
        Some(instance) => crate::app::patch::verb_ui::instance_target(*node, instance),
        None if fixture.instances.is_empty() => UiPatchTarget::Range {
            node: *node,
            start: 0,
            count: None,
        },
        None => target.clone(),
    }
}

/// The arm grammar's FIXTURE-SIDE completion, shared by every surface a
/// user can click an object on (the Tree's rows, the canvas's sprites).
///
/// An armed assign with a free segment selected completes here: the clicked
/// object takes the segment (one write, one undo step, through the same
/// verb path every other gesture uses). Anything else — a mapped object, a
/// fixture-side selection, no arm at all — only DISARMS: nonsense pairs
/// refuse rather than guess (§2), and mapped things always plain-reselect.
///
/// The caller selects the clicked target either way: after a completion the
/// object is what the user is looking at (the spike's transition), and
/// without one this was just a plain click.
pub(crate) fn complete_assign_on_object(
    on_action: &EventHandler<UiAction>,
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    ui: Option<PatchingUi>,
    target: &UiPatchTarget,
) {
    let Some(ui) = ui else {
        return;
    };
    let mut armed = ui.armed;
    if *armed.peek() != Some(ArmedVerb::Assign) {
        return;
    }
    // The arm is spent by the click, completed or not.
    armed.set(None);
    let Some(UiPatchTarget::Segment { node, start, .. }) = selection else {
        return;
    };
    // The same precondition the arm itself has: an unmapped object on a
    // manual fixture. A fixture ROW (Q8's card) and an auto-mapped object
    // are nonsense counterparts — they refuse, spending the arm.
    if !is_armable(surface, target) {
        return;
    }
    let Some(output) = surface.outputs.iter().find(|output| output.node == *node) else {
        return;
    };
    let subject = assign_subject_target(surface, target);
    if dispatch_assign(on_action, surface, &subject, output, *start) {
        // The nudged size was fine-tuning for the segment this write just
        // spent; the next one sizes itself off the next object again.
        let mut segment_size = ui.segment_size;
        segment_size.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{
        UiPatchBay, UiPatchInstance, UiPatchPort, UiPatchSurfaceFixture, UiPatchSurfaceOutput,
    };

    /// A two-instance fixture on a one-port output — enough shape for the
    /// selection→subject arms.
    fn small_dome_like_surface() -> UiPatchSurface {
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: NodeId::new(2),
                label: "dome".to_string(),
                // MANUAL: the mode the walk-up grammar lives in (Q11).
                manual_flow: true,
                instances: vec![
                    UiPatchInstance {
                        path: "/sector/1".to_string(),
                        label: "sector 1".to_string(),
                        start: 0,
                        lamps: 30,
                        stride: 1,
                        placed: true,
                    },
                    UiPatchInstance {
                        path: "/sector/2".to_string(),
                        label: "sector 2".to_string(),
                        start: 30,
                        lamps: 30,
                        stride: 1,
                        placed: true,
                    },
                ],
                ..Default::default()
            }],
            outputs: vec![UiPatchSurfaceOutput {
                node: NodeId::new(10),
                label: "out_a".to_string(),
                bay: UiPatchBay {
                    ports: vec![UiPatchPort {
                        key: 0,
                        pin_label: "IO18".to_string(),
                        start: 0,
                        lamps: 39,
                        cells: Vec::new(),
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Selection → pulse subject: fixture-side targets pulse in FIXTURE
    /// numbering, wire-side in WIRE numbering, and Module clears. Each
    /// subject carries the LANGUAGE core's matrix gave its target — the
    /// object chases, the fixture card and the wire breathe (D9, round 3).
    #[test]
    fn selections_map_to_their_pulse_subjects() {
        use lpa_studio_core::{PatchPulseLamps, PatchPulseLanguage};

        let surface = small_dome_like_surface();
        let node = surface.fixtures[0].node;
        let output = surface.outputs[0].node;

        assert_eq!(
            pulse_subject(&surface, &UiPatchTarget::Fixture { node }),
            Some(PatchPulseSubject {
                lamps: PatchPulseLamps::Fixture { node, range: None },
                language: PatchPulseLanguage::Breath,
            }),
            "a fixture is a bag of objects — no direction to claim"
        );
        let instance = &surface.fixtures[0].instances[1];
        assert_eq!(
            pulse_subject(
                &surface,
                &(UiPatchTarget::Instance {
                    node,
                    path: instance.path.clone(),
                })
            ),
            Some(PatchPulseSubject {
                lamps: PatchPulseLamps::Fixture {
                    node,
                    range: Some((instance.start, instance.lamps)),
                },
                language: PatchPulseLanguage::Chase,
            })
        );
        let port = &surface.outputs[0].bay.ports[0];
        assert_eq!(
            pulse_subject(
                &surface,
                &(UiPatchTarget::Port {
                    node: output,
                    port: port.key,
                })
            ),
            Some(PatchPulseSubject {
                lamps: PatchPulseLamps::Output {
                    node: output,
                    range: Some((port.start, port.lamps)),
                },
                language: PatchPulseLanguage::Breath,
            })
        );
        assert_eq!(
            pulse_subject(&surface, &UiPatchTarget::Output { node: output }),
            Some(PatchPulseSubject {
                lamps: PatchPulseLamps::Output {
                    node: output,
                    range: None,
                },
                language: PatchPulseLanguage::Breath,
            })
        );
        assert_eq!(
            pulse_subject(
                &surface,
                &(UiPatchTarget::Module {
                    node: NodeId::new(1)
                })
            ),
            None,
            "a module selection clears the pulse"
        );
    }

    /// A free SEGMENT is already wire-space: it pulses its own window, and
    /// the window is CLIPPED to the port it was drawn on rather than
    /// bleeding into the next one. A segment on a port that has gone away
    /// pulses nothing at all.
    #[test]
    fn a_segment_pulses_its_window_clipped_to_its_port() {
        use lpa_studio_core::{PatchPulseLamps, PatchPulseLanguage};

        let breathes = |node, range| {
            Some(PatchPulseSubject {
                lamps: PatchPulseLamps::Output { node, range },
                language: PatchPulseLanguage::Breath,
            })
        };
        let surface = small_dome_like_surface();
        let output = surface.outputs[0].node;
        let port = &surface.outputs[0].bay.ports[0];
        assert_eq!(
            (port.start, port.lamps),
            (0, 39),
            "the fixture's one port spans the whole wire"
        );

        assert_eq!(
            pulse_subject(
                &surface,
                &(UiPatchTarget::Segment {
                    node: output,
                    port: port.key,
                    start: 12,
                    lamps: 8,
                })
            ),
            breathes(output, Some((12, 8))),
            "a window inside the port passes straight through, in wire numbering"
        );

        assert_eq!(
            pulse_subject(
                &surface,
                &(UiPatchTarget::Segment {
                    node: output,
                    port: port.key,
                    start: 30,
                    lamps: 100,
                })
            ),
            breathes(output, Some((30, 9))),
            "an oversized window stops at the port's end"
        );

        assert_eq!(
            pulse_subject(
                &surface,
                &(UiPatchTarget::Segment {
                    node: output,
                    port: 7,
                    start: 0,
                    lamps: 4,
                })
            ),
            None,
            "a segment on a port the surface no longer has pulses nothing"
        );
    }

    /// The assign arm's precondition (§2): an UNMAPPED object or a free
    /// SEGMENT arms; a mapped object, a port, a module — nothing to link —
    /// do not. Mapped things always plain-reselect, so they must never put
    /// the surface into an armed state the next click would spend.
    #[test]
    fn only_the_two_ends_of_a_link_can_arm_assign() {
        let mut surface = small_dome_like_surface();
        // Sector 2 has no run yet; sector 1 does.
        surface.fixtures[0].instances[1].placed = false;
        let node = surface.fixtures[0].node;
        let output = surface.outputs[0].node;

        assert!(is_armable(
            &surface,
            &UiPatchTarget::Instance {
                node,
                path: "/sector/2".to_string(),
            }
        ));
        assert!(
            !is_armable(
                &surface,
                &UiPatchTarget::Instance {
                    node,
                    path: "/sector/1".to_string(),
                }
            ),
            "a mapped object plain-reselects — it arms nothing"
        );
        assert!(is_armable(
            &surface,
            &UiPatchTarget::Segment {
                node: output,
                port: 0,
                start: 0,
                lamps: 30,
            }
        ));
        assert!(
            !is_armable(
                &surface,
                &UiPatchTarget::Port {
                    node: output,
                    port: 0
                }
            ),
            "a whole port is the SWAP arm's subject, not the assign arm's"
        );
        assert!(!is_armable(
            &surface,
            &UiPatchTarget::Module {
                node: NodeId::new(1),
            }
        ));

        // Q8: a fixture WITH objects is a CARD, not an object — it arms
        // nothing even while one of its objects is waiting for a wire. The
        // canvas names objects directly now (Q10), so nothing needs it to.
        assert!(!is_armable(&surface, &UiPatchTarget::Fixture { node }));

        // Q11's mode gate: the same unmapped object on an AUTO-mapped
        // fixture arms nothing. Its unnamed lamps flow onto the wire by
        // themselves, so there is no link for the next click to complete.
        let mut auto = surface.clone();
        auto.fixtures[0].manual_flow = false;
        assert!(!is_armable(
            &auto,
            &UiPatchTarget::Instance {
                node,
                path: "/sector/2".to_string(),
            }
        ));
        assert!(
            is_armable(
                &auto,
                &UiPatchTarget::Segment {
                    node: output,
                    port: 0,
                    start: 0,
                    lamps: 30,
                }
            ),
            "the WIRE side is port-side: an auto fixture nearby changes nothing"
        );

        // The scarf (Q8's exception): a fixture with NO object table is its
        // own object, so it arms like one — while it is manual and unmapped.
        let mut scarf = surface.clone();
        scarf.fixtures[0].instances.clear();
        assert!(is_armable(&scarf, &UiPatchTarget::Fixture { node }));
        scarf.fixtures[0].manual_flow = false;
        assert!(!is_armable(&scarf, &UiPatchTarget::Fixture { node }));

        // Every instance placed: the fixture row has nothing left to link.
        surface.fixtures[0].instances[1].placed = true;
        assert!(!is_armable(&surface, &UiPatchTarget::Fixture { node }));
    }

    /// A fixture-grain click (a sprite, a Tree fixture row) means the
    /// object the free segment was SIZED for — its first one still waiting
    /// for a wire (P5b). Without this the canvas could only ever offer a
    /// whole-fixture subject, which the assign verb refuses.
    #[test]
    fn a_fixture_click_assigns_the_next_object_waiting_for_a_wire() {
        let mut surface = small_dome_like_surface();
        surface.fixtures[0].instances[1].placed = false;
        let node = surface.fixtures[0].node;

        assert_eq!(
            assign_subject_target(&surface, &UiPatchTarget::Fixture { node }),
            UiPatchTarget::Instance {
                node,
                path: "/sector/2".to_string(),
            },
            "sector 1 is already placed; sector 2 is what the click offers"
        );
        // A named object passes straight through — the click said which.
        let named = UiPatchTarget::Instance {
            node,
            path: "/sector/1".to_string(),
        };
        assert_eq!(assign_subject_target(&surface, &named), named);

        // A fixture with no object table is ONE strand: the whole thing, at
        // the range grain its document can actually hold (the scarf).
        surface.fixtures[0].instances.clear();
        assert_eq!(
            assign_subject_target(&surface, &UiPatchTarget::Fixture { node }),
            UiPatchTarget::Range {
                node,
                start: 0,
                count: None,
            }
        );
    }

    /// The banner names the armed verb — a walk-up user must never have to
    /// guess which gesture the next click completes.
    #[test]
    fn the_banner_names_the_armed_verb() {
        assert!(ArmedVerb::Assign.banner().starts_with("Assign armed"));
        assert!(
            ArmedVerb::Swap(PatchVerbWindow {
                output_name: None,
                start: 0,
                lamps: 30,
            })
            .banner()
            .starts_with("Swap armed")
        );
    }

    /// The key table, pure: every verb key routes, unknown keys don't, and
    /// meta gates the undo pair. ⌘⇧Z arrives as "Z" from the DOM, so the
    /// meta match must be case-insensitive (the old `onkeydown` matched
    /// "z" exactly and shipped a dead redo hotkey).
    #[test]
    fn patch_keys_route_to_their_actions() {
        use PatchKeyAction as A;
        let plain = |key| patch_key_action(key, false, false);
        assert_eq!(plain("Escape"), Some(A::Escape));
        assert_eq!(plain("r"), Some(A::Reverse));
        assert_eq!(plain(";"), Some(A::Rotate { steps: -1 }));
        assert_eq!(plain("'"), Some(A::Rotate { steps: 1 }));
        assert_eq!(plain("a"), Some(A::ArmAssign));
        assert_eq!(plain("s"), Some(A::ArmSwap));
        assert_eq!(plain("m"), Some(A::NextSegment));
        assert_eq!(plain("["), Some(A::ShiftSegment { delta: -1 }));
        assert_eq!(plain("]"), Some(A::ShiftSegment { delta: 1 }));
        assert_eq!(plain("-"), Some(A::ResizeSegment { delta: -1 }));
        assert_eq!(plain("="), Some(A::ResizeSegment { delta: 1 }));
        assert_eq!(plain("q"), None);
        assert_eq!(plain("z"), None, "bare z is not a verb");

        assert_eq!(patch_key_action("z", true, false), Some(A::Undo));
        assert_eq!(patch_key_action("Z", true, true), Some(A::Redo));
        assert_eq!(
            patch_key_action("r", true, false),
            None,
            "meta+verb is the browser's, not ours"
        );
    }
}
