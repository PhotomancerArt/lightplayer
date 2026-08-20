//! The Patching view's center (R5): the ONE project canvas with
//! patch-selection highlighting, the #409 verb set as toolbar items +
//! keyboard grammar (re-housed from the interim `/patch` page through
//! `app::patch::verb_ui`), and the selection pulse — the first consumer
//! of #411's `PatchPulseOp` (select pulses the live sim/hardware,
//! deselect and view-exit clear).
//!
//! Same canvas, different furniture (the one-project-canvas ADR): no
//! dive here — the authored tree is Mapping's activity; this center
//! reads the resolved surface and writes patches through the verbs.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeId, PatchPulseOp, PatchPulseSubject, PatchVerbKind, PatchVerbWindow, ProjectController,
    ProjectEditorOp, ProjectEditorView, UiAction, UiPatchSurface, UiPatchTarget,
};

use super::arrange::{PackSlots, ProjectCanvasHost, refresh_pack_slots};
use super::toolbar::{StatusKind, ToolbarGroup, ToolbarItem, ToolbarStrip};
use super::{mapping_assets, prefetch_editor_meta};
use crate::app::patch::verb_ui::{dispatch_verb, port_window, selection_stride};
use crate::app::workbench::panels::prefetch_bodies;

/// Cross-dock patching UI state, provided by the workbench frame: the
/// swap verb arms here (`s` in the center) and completes on a port click
/// in the Outputs dock — the same two-sided gesture the interim page
/// carried inside one component, now spanning the frame. The context is
/// the precedent the page itself set (`HoveredPatchCell`).
#[derive(Clone, Copy)]
pub(crate) struct PatchingUi {
    pub armed_swap: Signal<Option<PatchVerbWindow>>,
}

/// Map the shared patch selection onto the pulse's subject vocabulary:
/// fixture-side targets pulse in fixture numbering (the controller maps
/// them through the placements), wire-side targets in wire numbering.
/// `Module` — and no selection — clear the pulse.
fn pulse_subject(
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
) -> Option<PatchPulseSubject> {
    match selection.as_ref()? {
        UiPatchTarget::Fixture { node } => Some(PatchPulseSubject::Fixture {
            node: *node,
            range: None,
        }),
        UiPatchTarget::Instance { node, path } => {
            let fixture = surface.fixtures.iter().find(|f| f.node == *node)?;
            let instance = fixture
                .instances
                .iter()
                .find(|instance| instance.path == *path)?;
            Some(PatchPulseSubject::Fixture {
                node: *node,
                range: Some((instance.start, instance.lamps)),
            })
        }
        UiPatchTarget::Range { node, start, count } => Some(PatchPulseSubject::Fixture {
            node: *node,
            range: count.map(|count| (*start, count)),
        }),
        UiPatchTarget::Cell { id } => {
            let node: u32 = id.split(':').next()?.parse().ok()?;
            let node = NodeId::new(node);
            let fixture = surface.fixtures.iter().find(|f| f.node == node)?;
            let cell = fixture.patch.cells.iter().find(|cell| cell.id == *id)?;
            Some(PatchPulseSubject::Fixture {
                node,
                range: Some((cell.source_start, cell.lamps)),
            })
        }
        UiPatchTarget::Port { node, port } => {
            let output = surface.outputs.iter().find(|output| output.node == *node)?;
            let port = output.bay.ports.iter().find(|p| p.key == *port)?;
            Some(PatchPulseSubject::Output {
                node: *node,
                range: Some((port.start, port.lamps)),
            })
        }
        UiPatchTarget::Output { node } => Some(PatchPulseSubject::Output {
            node: *node,
            range: None,
        }),
        UiPatchTarget::Module { .. } => None,
    }
}

fn send_pulse(on_action: &EventHandler<UiAction>, subject: Option<PatchPulseSubject>) {
    on_action.call(UiAction::from_op(
        ProjectController::NODE_ID,
        PatchPulseOp { subject },
    ));
}

fn select(on_action: &EventHandler<UiAction>, target: Option<UiPatchTarget>) {
    on_action.call(UiAction::from_op(
        lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
        ProjectEditorOp::PatchSelect { target },
    ));
}

/// The patching activity's toolbar: verb buttons with their keys printed
/// (every verb is a hotkey AND a visible control — walk-up-patching's
/// controls-surface rule), history, and the counts readout. Another item
/// list on the ONE strip, never another strip (the ADR's D1).
fn patch_toolbar(
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    armed: bool,
    help_open: bool,
) -> Vec<ToolbarGroup> {
    let has_subject = selection
        .as_ref()
        .and_then(|selection| crate::app::patch::verb_ui::verb_subject(surface, selection))
        .is_some();
    let port_selected = matches!(selection, Some(UiPatchTarget::Port { .. }));
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
            id: "patch-verbs",
            trailing: false,
            items: vec![
                verb(
                    "patch-reverse",
                    "r reverse",
                    "Reverse the selection's wire direction (r)",
                    false,
                    has_subject,
                ),
                verb(
                    "patch-rotate-back",
                    "; rotate",
                    "Rotate the selection one stride back (;)",
                    false,
                    has_subject,
                ),
                verb(
                    "patch-rotate-fwd",
                    "' rotate",
                    "Rotate the selection one stride forward (')",
                    false,
                    has_subject,
                ),
                verb(
                    "patch-swap",
                    "s swap",
                    "Arm a port swap from the selected port, then click the other port (s)",
                    armed,
                    port_selected || armed,
                ),
            ],
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
            items: vec![
                ToolbarItem::Status {
                    text: format!("{placed}/{instances} placed"),
                    kind: StatusKind::Mono,
                },
                verb(
                    "patch-help",
                    "?",
                    "Show the patching keys (?)",
                    help_open,
                    true,
                ),
            ],
        },
    ]
}

/// The Patching view's center: toolbar + the one canvas, verbs on keys.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PatchingShellCenter(
    surface: Option<UiPatchSurface>,
    selection: Option<UiPatchTarget>,
    /// The full editor view — the canvas resolves fixture map2d bodies out
    /// of the snapshot's node views, exactly like the Mapping center.
    project_editor: ProjectEditorView,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut help_open = use_signal(|| false);
    let PatchingUi { mut armed_swap } = use_context::<PatchingUi>();
    // The pulse's echo guard: dispatch only when the mapped subject
    // actually changes (sweep-with-clear lives in the controller; this
    // just keeps renders from re-sending the same subject).
    let mut pulsed = use_signal(|| Option::<PatchPulseSubject>::None);
    // View-exit clears the pulse: a highlight pointing at a selection the
    // user can no longer see is actively misleading (#411's own rule).
    use_drop({
        let on_action = on_action;
        move || send_pulse(&on_action, None)
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
    // Sticky auto-pack slots, same policy as the Mapping center: the two
    // views must show the SAME arrangement (one conceptual space).
    let mut pack_slots = use_signal(PackSlots::new);
    let refreshed = refresh_pack_slots(&surface, &bodies, &pack_slots.peek());
    if let Some(next) = refreshed {
        pack_slots.set(next);
    }
    let pack = pack_slots.read().clone();
    let subject = pulse_subject(&surface, &selection);
    if *pulsed.peek() != subject {
        pulsed.set(subject.clone());
        send_pulse(&on_action, subject);
    }
    let armed = armed_swap.read().is_some();
    let groups = patch_toolbar(&surface, &selection, armed, *help_open.read());
    let on_item = {
        let surface = surface.clone();
        let selection = selection.clone();
        let on_action = on_action;
        move |id: &'static str| match id {
            "patch-reverse" => {
                dispatch_verb(&on_action, &surface, &selection, PatchVerbKind::Reverse)
            }
            "patch-rotate-back" => {
                let stride = selection_stride(&surface, &selection);
                dispatch_verb(
                    &on_action,
                    &surface,
                    &selection,
                    PatchVerbKind::Rotate { steps: -1, stride },
                );
            }
            "patch-rotate-fwd" => {
                let stride = selection_stride(&surface, &selection);
                dispatch_verb(
                    &on_action,
                    &surface,
                    &selection,
                    PatchVerbKind::Rotate { steps: 1, stride },
                );
            }
            "patch-swap" => arm_swap(&surface, &selection, &mut armed_swap),
            "patch-undo" => dispatch_verb(&on_action, &surface, &selection, PatchVerbKind::Undo),
            "patch-redo" => dispatch_verb(&on_action, &surface, &selection, PatchVerbKind::Redo),
            "patch-help" => {
                let open = *help_open.peek();
                help_open.set(!open);
            }
            _ => {}
        }
    };
    rsx! {
        div {
            class: "tw:flex tw:min-h-0 tw:flex-1 tw:flex-col tw:outline-none",
            // The keyboard grammar (the interim page's, verbatim):
            // r reverse · ;/' rotate ∓/± stride · s arm swap · Escape
            // ladder · ⌘Z/⌘⇧Z undo/redo · ? help.
            tabindex: 0,
            onkeydown: {
                let surface = surface.clone();
                let selection = selection.clone();
                let on_action = on_action;
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
                            "s" => arm_swap(&surface, &selection, &mut armed_swap),
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
            ToolbarStrip { groups, on_item }
            if armed {
                div { class: "tw:flex-none tw:border-b tw:border-border-subtle tw:bg-selection-bg tw:px-2.5 tw:py-1 tw:text-[11px] tw:text-selection-border",
                    "Swap armed — click the other port in the Outputs panel (Esc cancels)"
                }
            }
            if *help_open.read() {
                div { class: "tw:fixed tw:bottom-4 tw:right-4 tw:z-50 tw:rounded-lg tw:border tw:border-border-strong tw:bg-card-subtle tw:p-4 tw:text-xs tw:leading-relaxed tw:shadow-lg",
                    div { class: "tw:mb-1 tw:font-semibold", "Patch keys" }
                    div { "click port — assign selection · click cell — select" }
                    div { "r — reverse · ; / ' — rotate ∓/± stride" }
                    div { "s — arm swap (then click the other port)" }
                    div { "⌘Z / ⌘⇧Z — undo / redo · Esc — back out · ? — close" }
                }
            }
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
                        on_action,
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
        UiPatchBay, UiPatchInstance, UiPatchPort, UiPatchSurfaceFixture, UiPatchSurfaceOutput,
    };

    /// A two-instance fixture on a one-port output — enough shape for the
    /// selection→subject arms.
    fn mini_dome_like_surface() -> UiPatchSurface {
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: NodeId::new(2),
                label: "dome".to_string(),
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
    /// numbering, wire-side in WIRE numbering, and Module clears.
    #[test]
    fn selections_map_to_their_pulse_subjects() {
        let surface = mini_dome_like_surface();
        let node = surface.fixtures[0].node;
        let output = surface.outputs[0].node;

        assert_eq!(
            pulse_subject(&surface, &Some(UiPatchTarget::Fixture { node })),
            Some(PatchPulseSubject::Fixture { node, range: None })
        );
        let instance = &surface.fixtures[0].instances[1];
        assert_eq!(
            pulse_subject(
                &surface,
                &Some(UiPatchTarget::Instance {
                    node,
                    path: instance.path.clone(),
                })
            ),
            Some(PatchPulseSubject::Fixture {
                node,
                range: Some((instance.start, instance.lamps)),
            })
        );
        let port = &surface.outputs[0].bay.ports[0];
        assert_eq!(
            pulse_subject(
                &surface,
                &Some(UiPatchTarget::Port {
                    node: output,
                    port: port.key,
                })
            ),
            Some(PatchPulseSubject::Output {
                node: output,
                range: Some((port.start, port.lamps)),
            })
        );
        assert_eq!(
            pulse_subject(&surface, &Some(UiPatchTarget::Output { node: output })),
            Some(PatchPulseSubject::Output {
                node: output,
                range: None,
            })
        );
        assert_eq!(
            pulse_subject(
                &surface,
                &Some(UiPatchTarget::Module {
                    node: NodeId::new(1)
                })
            ),
            None,
            "a module selection clears the pulse"
        );
        assert_eq!(pulse_subject(&surface, &None), None);
    }
}

/// Arm the swap from the selected port (`s`, or the toolbar button); a
/// second call disarms — the key is a toggle, like the page's Escape rung.
fn arm_swap(
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    armed_swap: &mut Signal<Option<PatchVerbWindow>>,
) {
    if armed_swap.peek().is_some() {
        armed_swap.set(None);
        return;
    }
    if let Some(UiPatchTarget::Port { node, port }) = selection
        && let Some(output) = surface.outputs.iter().find(|output| output.node == *node)
        && let Some(window) = port_window(output, *port)
    {
        armed_swap.set(Some(window));
    }
}
