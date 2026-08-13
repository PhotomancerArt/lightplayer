//! The unified editor's center — the coordinator mounted in the
//! workbench Mapping view (unified-editor P3/P4, workbench-amended).
//!
//! The workbench chrome (#413) owns the docks, panels, and view tabs;
//! this module owns only the CENTER: the editor toolbar strip and the
//! [`arrange_canvas::ArrangeCanvas`]. The Fixtures/Outputs panels are the
//! editor's rails — they are grown in place, never forked.
//!
//! Mode note (Yona, 2026-08-12 gate rulings): mapping lands FIRST;
//! patching becomes its OWN workbench view later (R5), so there is no
//! mode segment here and the interim `/patch` page stays untouched until
//! then. Diving into a fixture is IN-PLACE (no separate screen): the
//! focused fixture's session mounts with the other fixtures dimmed
//! inside the same canvas, and the camera snaps to the fixture's frame —
//! the "snap viewport to fixture" solution.

pub mod arrange_canvas;
#[cfg(feature = "stories")]
pub(crate) mod editor_shell_stories;
pub(crate) mod mapping_session;

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lpa_studio_core::{
    ArtifactLocation, AssetEditOp, EditorMetaOp, EditorMetaVerb, NodeId, ProjectController,
    ProjectEditorOp, ProjectEditorView, UiAction, UiArrangeTransform, UiAssetEditor,
    UiEditJournalEvent, UiEditorMode, UiNodeChild, UiNodeFace, UiNodeView, UiPatchSurface,
    UiPatchTarget,
};

use arrange_canvas::{ArrangeCanvas, PackSlots, dive_context, refresh_pack_slots};
use mapping_session::MappingSessionHost;

/// The Mapping view's center: toolbar + arrange canvas.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EditorShellCenter(
    surface: Option<UiPatchSurface>,
    selection: Option<UiPatchTarget>,
    /// The full editor view — the canvas resolves fixture map2d bodies out
    /// of the snapshot's node views (the same bytes the face embeds hold).
    project_editor: ProjectEditorView,
    /// The workbench-owned dive state, shared with the Fixtures tree and
    /// the Props pane (R4): focused fixture, session, Props commit bumps.
    dive_focused: Signal<Option<NodeId>>,
    dive_session: Signal<lpa_mapping_editor::MapEditorSession>,
    dive_commits: Signal<u64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // The FOCUSED fixture — the DIVE (gate ruling: in-place, no separate
    // screen): its mapping session mounts in the center with every other
    // fixture rendered dimmed inside the same canvas, transformed into
    // the focused doc's space. Entered by double-click on the canvas or
    // the toolbar button; exited by the breadcrumb. Journal events stamp
    // every transition.
    let mut focused = dive_focused;
    let Some(surface) = surface else {
        return rsx! {
            div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:items-center tw:justify-center",
                p { class: "tw:m-0 tw:max-w-[360px] tw:text-center tw:text-xs tw:text-dim-foreground",
                    "No fixtures on a wire yet — bind an output to a control bus and the mapping editor fills in."
                }
            }
        };
    };
    prefetch_editor_meta(&on_action, &surface);
    prefetch_selected_body(&on_action, &surface, &selection);
    let (bodies, asset_editors) = mapping_assets(&project_editor);
    // Sticky auto-pack slots: refreshed only when the unarranged set
    // changes, so arranging one fixture never moves another.
    let mut pack_slots = use_signal(PackSlots::new);
    let refreshed = refresh_pack_slots(&surface, &bodies, &pack_slots.peek());
    if let Some(next) = refreshed {
        pack_slots.set(next);
    }
    let pack = pack_slots.read().clone();
    let focus_journal = move |on_action: &EventHandler<UiAction>,
                              event: UiEditJournalEvent,
                              node: Option<NodeId>,
                              mode: UiEditorMode| {
        on_action.call(UiAction::from_op(
            lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
            ProjectEditorOp::EditorJournal { event, node, mode },
        ));
    };
    let mut enter_focus = move |on_action: &EventHandler<UiAction>, node: NodeId| {
        if focused.peek().is_none() {
            focus_journal(
                on_action,
                UiEditJournalEvent::ModeSwitch,
                Some(node),
                UiEditorMode::Mapping,
            );
        } else if *focused.peek() != Some(node) {
            focus_journal(
                on_action,
                UiEditJournalEvent::NodeSwitch,
                Some(node),
                UiEditorMode::Mapping,
            );
        }
        focused.set(Some(node));
    };
    let mut exit_focus = move |on_action: &EventHandler<UiAction>| {
        if focused.peek().is_some() {
            focus_journal(
                on_action,
                UiEditJournalEvent::ModeSwitch,
                None,
                UiEditorMode::Arrange,
            );
        }
        focused.set(None);
    };
    // The focused fixture's face-editor DTO (fetch/refusal/apply wiring
    // rides it); a fixture that loses its editor (node removed) drops
    // focus honestly.
    let focused_editor: Option<(NodeId, String, UiAssetEditor)> = focused.read().and_then(|node| {
        let fixture = surface
            .fixtures
            .iter()
            .find(|fixture| fixture.node == node)?;
        let artifact = fixture.mapping_artifact.as_ref()?;
        let editor = asset_editors.get(artifact)?.clone();
        Some((node, fixture.label.clone(), editor))
    });
    let fixtures = surface.fixtures.len();
    let arranged = surface
        .fixtures
        .iter()
        .filter(|fixture| {
            fixture
                .arrange
                .as_ref()
                .is_some_and(|arrange| arrange.arranged)
        })
        .count();

    // The selected fixture's arrange facts, for the transform verbs.
    let selected: Option<(String, NodeId, UiArrangeTransform)> = match &selection {
        Some(
            UiPatchTarget::Fixture { node }
            | UiPatchTarget::Instance { node, .. }
            | UiPatchTarget::Range { node, .. },
        ) => surface
            .fixtures
            .iter()
            .find(|fixture| fixture.node == *node)
            .and_then(|fixture| {
                Some((
                    fixture.address.clone()?,
                    fixture.node,
                    fixture
                        .arrange
                        .clone()
                        .map(|arrange| arrange.transform)
                        .unwrap_or_default(),
                ))
            }),
        _ => None,
    };
    let arrange_op = arrange_dispatch(&surface);
    let arrange_verb = {
        let arrange_op = arrange_op.clone();
        move |on_action: &EventHandler<UiAction>, verb: EditorMetaVerb| {
            if let Some(op) = arrange_op(verb) {
                on_action.call(UiAction::from_op(ProjectController::NODE_ID, op));
            }
        }
    };
    let adjust = {
        let selected = selected.clone();
        let arrange_verb = arrange_verb.clone();
        move |on_action: &EventHandler<UiAction>, dr: f64, ds: f64| {
            let Some((key, node, transform)) = selected.clone() else {
                return;
            };
            let next = UiArrangeTransform {
                t: transform.t,
                r: transform.r + dr,
                s: (transform.s * if ds == 0.0 { 1.0 } else { ds }).clamp(0.05, 20.0),
            };
            arrange_verb(
                on_action,
                EditorMetaVerb::Set {
                    node_key: key,
                    node: Some(node),
                    transform: next,
                },
            );
        }
    };

    const TOOL: &str = "tw:cursor-pointer tw:rounded tw:border tw:border-border-strong tw:bg-card-subtle tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:opacity-40";

    rsx! {
        div {
            class: "tw:flex tw:min-h-0 tw:flex-1 tw:flex-col tw:outline-none",
            tabindex: 0,
            onkeydown: {
                let arrange_verb = arrange_verb.clone();
                move |evt: KeyboardEvent| {
                    // Mode-scoped ⌘Z (ratified): with a fixture focused the
                    // mapping session owns undo (the MapEditor's own
                    // handler); the arrange stack answers only in arrange.
                    if focused.peek().is_some() {
                        return;
                    }
                    let meta = evt.data().modifiers().meta() || evt.data().modifiers().ctrl();
                    // Shift+z arrives as "Z", so match case-insensitively.
                    let is_z = matches!(
                        evt.data().key(),
                        Key::Character(c) if c.eq_ignore_ascii_case("z")
                    );
                    if meta && is_z {
                        evt.prevent_default();
                        let verb = if evt.data().modifiers().shift() {
                            EditorMetaVerb::Redo
                        } else {
                            EditorMetaVerb::Undo
                        };
                        arrange_verb(&on_action, verb);
                    }
                }
            },
            // The editor toolbar. Focused: the breadcrumb back to the
            // arranged space (the MapEditor below brings its own tool
            // strip). Arrange: transform verbs for the selection, undo /
            // redo, and the reserved right-end slot for whatever
            // patching's home turns out to be.
            if let Some((_, label, _)) = &focused_editor {
                div { class: "tw:flex tw:min-h-[30px] tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border-subtle tw:bg-card-muted tw:px-2.5",
                    button {
                        class: "tw:cursor-pointer tw:border-none tw:bg-transparent tw:p-0 tw:text-xs tw:text-selection-border",
                        title: "Back to the arranged space",
                        onclick: move |_| exit_focus(&on_action),
                        "‹ Arrange"
                    }
                    span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                        "{label} · mapping"
                    }
                }
            } else {
            div { class: "tw:flex tw:min-h-[30px] tw:flex-none tw:items-center tw:gap-1.5 tw:border-b tw:border-border-subtle tw:bg-card-muted tw:px-2.5",
                span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                    "Arrange"
                }
                button {
                    class: "{TOOL}",
                    disabled: !selected_has_mapping(&surface, &selected),
                    title: "Edit the selected fixture's mapping (double-click on the canvas does too)",
                    onclick: {
                        let selected = selected.clone();
                        let surface_fixtures: Vec<(NodeId, bool)> = surface
                            .fixtures
                            .iter()
                            .map(|fixture| (fixture.node, fixture.mapping_artifact.is_some()))
                            .collect();
                        move |_| {
                            if let Some((_, node, _)) = &selected
                                && surface_fixtures
                                    .iter()
                                    .any(|(n, has)| n == node && *has)
                            {
                                enter_focus(&on_action, *node);
                            }
                        }
                    },
                    "edit mapping"
                }
                button {
                    class: "{TOOL}",
                    disabled: selected.is_none(),
                    title: "Rotate the selected fixture 15° counter-clockwise",
                    onclick: {
                        let adjust = adjust.clone();
                        move |_| adjust(&on_action, -15.0, 0.0)
                    },
                    "⟲ 15°"
                }
                button {
                    class: "{TOOL}",
                    disabled: selected.is_none(),
                    title: "Rotate the selected fixture 15° clockwise",
                    onclick: {
                        let adjust = adjust.clone();
                        move |_| adjust(&on_action, 15.0, 0.0)
                    },
                    "⟳ 15°"
                }
                button {
                    class: "{TOOL}",
                    disabled: selected.is_none(),
                    title: "Shrink the selected fixture",
                    onclick: {
                        let adjust = adjust.clone();
                        move |_| adjust(&on_action, 0.0, 1.0 / 1.15)
                    },
                    "−"
                }
                button {
                    class: "{TOOL}",
                    disabled: selected.is_none(),
                    title: "Grow the selected fixture",
                    onclick: {
                        let adjust = adjust.clone();
                        move |_| adjust(&on_action, 0.0, 1.15)
                    },
                    "+"
                }
                span { class: "tw:mx-1 tw:h-4 tw:w-px tw:bg-border-strong" }
                button {
                    class: "{TOOL}",
                    title: "Undo the last arrange edit (⌘Z)",
                    onclick: {
                        let arrange_verb = arrange_verb.clone();
                        move |_| arrange_verb(&on_action, EditorMetaVerb::Undo)
                    },
                    "↶"
                }
                button {
                    class: "{TOOL}",
                    title: "Redo the last undone arrange edit (⇧⌘Z)",
                    onclick: {
                        let arrange_verb = arrange_verb.clone();
                        move |_| arrange_verb(&on_action, EditorMetaVerb::Redo)
                    },
                    "↷"
                }
                span { class: "tw:ml-auto tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                    "{fixtures} fixtures · {arranged} arranged"
                }
            }
            }
            if let Some(error) = surface.editor_meta_error.clone() {
                div { class: "tw:flex-none tw:border-b tw:border-border-subtle tw:bg-status-attention-bg tw:px-2.5 tw:py-1 tw:text-[11px] tw:text-status-attention-foreground",
                    "editor.json refused: {error} — arranging is disabled so the file is never rewritten blind."
                }
            }
            div { class: "tw:relative tw:flex tw:min-h-0 tw:flex-1 tw:flex-col",
                if let Some((node, _, editor)) = focused_editor.clone() {
                    // The DIVE (gate ruling: no separate screen): the same
                    // asset-pipeline session (fetch → session → ApplyBody →
                    // SaveOverlay, refuse-don't-rewrite), with the OTHER
                    // fixtures rendered dimmed inside the editor's own
                    // canvas — transformed into the focused doc's space —
                    // and committed edits stamped into the correlation
                    // journal at this glue layer.
                    div { class: "tw:min-h-0 tw:flex-1 tw:overflow-hidden",
                        MappingSessionHost {
                            editor,
                            context: dive_context(&surface, &bodies, &pack, node),
                            external_session: dive_session,
                            commit_requests: dive_commits,
                            on_action: {
                                move |action: UiAction| {
                                    if action
                                        .op_as::<AssetEditOp>()
                                        .is_some_and(|op| matches!(op, AssetEditOp::ApplyBody { .. }))
                                    {
                                        focus_journal(
                                            &on_action,
                                            UiEditJournalEvent::Edit,
                                            Some(node),
                                            UiEditorMode::Mapping,
                                        );
                                    }
                                    on_action.call(action);
                                }
                            },
                        }
                    }
                } else if !surface.editor_meta_loaded {
                    div { class: "tw:flex tw:flex-1 tw:items-center tw:justify-center",
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Loading the arrangement…" }
                    }
                } else {
                    ArrangeCanvas {
                        surface: surface.clone(),
                        bodies,
                        selection: selection.clone(),
                        pack,
                        on_focus: move |node| enter_focus(&on_action, node),
                        on_action,
                    }
                }
            }
        }
    }
}

/// Prebuild the arrange-op factory: `editor.json` artifact + the fixture
/// facts every write refreshes footprints through. `None` op = the
/// artifact is unknown (surface not settled), so verbs no-op honestly.
fn arrange_dispatch(
    surface: &UiPatchSurface,
) -> impl Fn(EditorMetaVerb) -> Option<EditorMetaOp> + Clone + 'static {
    let artifact = surface.editor_meta_artifact.clone();
    let fixtures: Vec<lpa_studio_core::EditorMetaFixture> = surface
        .fixtures
        .iter()
        .filter_map(|fixture| {
            Some(lpa_studio_core::EditorMetaFixture {
                node_key: fixture.address.clone()?,
                mapping_artifact: fixture.mapping_artifact.clone(),
            })
        })
        .collect();
    move |verb| {
        Some(EditorMetaOp {
            artifact: artifact.clone()?,
            fixtures: fixtures.clone(),
            verb,
        })
    }
}

/// Every fixture mapping editor the snapshot carries, keyed by artifact:
/// resolved body text (the canvas's geometry source) plus the face-editor
/// DTO whole (the focused mode's fetch/apply/refusal wiring). Same bytes
/// the face embeds edit; the canvas renders placeholders until they land.
type MappingAssets = (
    BTreeMap<ArtifactLocation, String>,
    BTreeMap<ArtifactLocation, UiAssetEditor>,
);

fn mapping_assets(project_editor: &ProjectEditorView) -> MappingAssets {
    let mut assets = MappingAssets::default();
    fn face(assets: &mut MappingAssets, face: &Option<UiNodeFace>) {
        if let Some(UiNodeFace::Fixture(fixture)) = face
            && let Some(editor) = &fixture.mapping_editor
        {
            if let Some(text) = editor.content.as_ref().and_then(|content| content.text()) {
                assets.0.insert(editor.artifact.clone(), text.to_string());
            }
            assets.1.insert(editor.artifact.clone(), editor.clone());
        }
    }
    fn walk_children(assets: &mut MappingAssets, children: &[UiNodeChild]) {
        for child in children {
            face(assets, &child.face);
            walk_children(assets, &child.children);
        }
    }
    fn walk_nodes(assets: &mut MappingAssets, nodes: &[UiNodeView]) {
        for node in nodes {
            face(assets, &node.face);
            walk_children(assets, &node.children);
        }
    }
    walk_nodes(&mut assets, &project_editor.nodes);
    assets
}

/// Does the current selection name a fixture whose mapping can be edited?
fn selected_has_mapping(
    surface: &UiPatchSurface,
    selected: &Option<(String, NodeId, UiArrangeTransform)>,
) -> bool {
    selected.as_ref().is_some_and(|(_, node, _)| {
        surface
            .fixtures
            .iter()
            .any(|fixture| fixture.node == *node && fixture.mapping_artifact.is_some())
    })
}

/// Lazy loading on SELECTION (P5): selecting a fixture whose body has not
/// landed dispatches the fetch, flag-driven — the canvas swaps its
/// placeholder for real geometry when the snapshot catches up.
fn prefetch_selected_body(
    on_action: &EventHandler<UiAction>,
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
) {
    let node = match selection {
        Some(
            UiPatchTarget::Fixture { node }
            | UiPatchTarget::Instance { node, .. }
            | UiPatchTarget::Range { node, .. },
        ) => *node,
        _ => return,
    };
    let Some(fixture) = surface.fixtures.iter().find(|fixture| fixture.node == node) else {
        return;
    };
    if !fixture.mapping_loaded
        && let Some(artifact) = fixture.mapping_artifact.clone()
    {
        on_action.call(UiAction::from_op(
            ProjectController::NODE_ID,
            lpa_studio_core::AssetContentFetchOp { artifact },
        ));
    }
}

/// Flag-driven prefetch (the #409 lesson: never hand-code a fetch a flag
/// doesn't ask for): while the surface says editor.json has not settled,
/// dispatch the fetch. Absence settles the flag too, so this quiesces
/// after one round trip.
fn prefetch_editor_meta(on_action: &EventHandler<UiAction>, surface: &UiPatchSurface) {
    if !surface.editor_meta_loaded
        && let Some(artifact) = surface.editor_meta_artifact.clone()
    {
        on_action.call(UiAction::from_op(
            ProjectController::NODE_ID,
            lpa_studio_core::EditorMetaFetchOp { artifact },
        ));
    }
}
