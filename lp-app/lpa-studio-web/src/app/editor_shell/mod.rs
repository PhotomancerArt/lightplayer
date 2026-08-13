//! The unified editor's center — the coordinator mounted in the
//! workbench Mapping view (unified-editor P3/P4, workbench-amended).
//!
//! The workbench chrome (#413) owns the docks, panels, and view tabs;
//! this module owns only the CENTER: the editor toolbar strip and the
//! [`arrange_canvas::ArrangeCanvas`]. The Fixtures/Outputs panels are the
//! editor's rails — they are grown in place, never forked.
//!
//! Mode note (Yona, 2026-08-12 mid-run steer): mapping lands FIRST and
//! patching's home is decided after it is played with — so there is no
//! mapping|patching mode segment here yet, and the interim `/patch` page
//! stays untouched. The toolbar keeps the slot the segment (or whatever
//! wins) will occupy.

pub mod arrange_canvas;
#[cfg(feature = "stories")]
pub(crate) mod editor_shell_stories;

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lpa_studio_core::{
    ArtifactLocation, EditorMetaOp, EditorMetaVerb, NodeId, ProjectController, ProjectEditorView,
    UiAction, UiArrangeTransform, UiNodeChild, UiNodeFace, UiNodeView, UiPatchSurface,
    UiPatchTarget,
};

use arrange_canvas::ArrangeCanvas;

/// The Mapping view's center: toolbar + arrange canvas.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EditorShellCenter(
    surface: Option<UiPatchSurface>,
    selection: Option<UiPatchTarget>,
    /// The full editor view — the canvas resolves fixture map2d bodies out
    /// of the snapshot's node views (the same bytes the face embeds hold).
    project_editor: ProjectEditorView,
    on_action: EventHandler<UiAction>,
) -> Element {
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
    let bodies = mapping_bodies(&project_editor);
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
            // The editor toolbar: arrange verbs for the selection, undo /
            // redo, and the reserved right-end slot for whatever patching's
            // home turns out to be. Mapping tools join in P5.
            div { class: "tw:flex tw:min-h-[30px] tw:flex-none tw:items-center tw:gap-1.5 tw:border-b tw:border-border-subtle tw:bg-card-muted tw:px-2.5",
                span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                    "Arrange"
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
            if let Some(error) = surface.editor_meta_error.clone() {
                div { class: "tw:flex-none tw:border-b tw:border-border-subtle tw:bg-status-attention-bg tw:px-2.5 tw:py-1 tw:text-[11px] tw:text-status-attention-foreground",
                    "editor.json refused: {error} — arranging is disabled so the file is never rewritten blind."
                }
            }
            div { class: "tw:relative tw:flex tw:min-h-0 tw:flex-1",
                if !surface.editor_meta_loaded {
                    div { class: "tw:flex tw:flex-1 tw:items-center tw:justify-center",
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Loading the arrangement…" }
                    }
                } else {
                    ArrangeCanvas {
                        surface: surface.clone(),
                        bodies,
                        selection: selection.clone(),
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

/// Every fixture map2d body the snapshot carries, keyed by artifact — the
/// same bytes the face embeds edit (fetched by the panels' prefetch or the
/// face's own mount; the canvas renders placeholders until they land).
fn mapping_bodies(project_editor: &ProjectEditorView) -> BTreeMap<ArtifactLocation, String> {
    let mut bodies = BTreeMap::new();
    fn face(bodies: &mut BTreeMap<ArtifactLocation, String>, face: &Option<UiNodeFace>) {
        if let Some(UiNodeFace::Fixture(fixture)) = face
            && let Some(editor) = &fixture.mapping_editor
            && let Some(text) = editor.content.as_ref().and_then(|content| content.text())
        {
            bodies.insert(editor.artifact.clone(), text.to_string());
        }
    }
    fn walk_children(bodies: &mut BTreeMap<ArtifactLocation, String>, children: &[UiNodeChild]) {
        for child in children {
            face(bodies, &child.face);
            walk_children(bodies, &child.children);
        }
    }
    fn walk_nodes(bodies: &mut BTreeMap<ArtifactLocation, String>, nodes: &[UiNodeView]) {
        for node in nodes {
            face(bodies, &node.face);
            walk_children(bodies, &node.children);
        }
    }
    walk_nodes(&mut bodies, &project_editor.nodes);
    bodies
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
