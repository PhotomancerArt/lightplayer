//! The unified editor's center — the coordinator mounted in the
//! workbench Mapping view.
//!
//! The workbench chrome (#413) owns the docks, panels, and view tabs;
//! this module owns only the CENTER: the ONE data-driven toolbar row and
//! the ONE crate canvas ([`arrange::ProjectCanvasHost`]) carrying both
//! activities. The Fixtures/Outputs panels are the editor's rails — they
//! are grown in place, never forked.
//!
//! The DIVE is layer state, not component identity (the one-project-canvas
//! plan): double-clicking a fixture makes its lamps editable IN PLACE —
//! same canvas, same camera, no chrome swap. The asset pipeline
//! ([`mapping_session::MappingAssetPipeline`]) seeds the workbench-owned
//! session and applies commits; the toolbar morphs between the fixture
//! and mapping item lists; esc walks the editor ladder and, at its end,
//! leaves the dive. Patching becomes its own workbench view later (R5) by
//! mounting the same canvas with different furniture.

pub(crate) mod arrange;
#[cfg(feature = "stories")]
pub(crate) mod editor_shell_stories;
pub(crate) mod mapping_session;
pub(crate) mod patching;
pub(crate) mod toolbar;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use dioxus::prelude::*;
use lpa_mapping_editor::{
    DocOpen, EditorKeyOutcome, EditorViewOptions, Map2dDoc, MapTool, handle_editor_key,
};
use lpa_studio_core::{
    ArtifactLocation, AssetEditOp, EditorMetaOp, EditorMetaVerb, NodeId, ProjectController,
    ProjectEditorOp, ProjectEditorView, UiAction, UiArrangeTransform, UiAssetEditor,
    UiEditJournalEvent, UiEditorMode, UiNodeChild, UiNodeFace, UiNodeView, UiPatchSurface,
    UiPatchTarget,
};

use arrange::{DiveHost, PackSlots, ProjectCanvasHost, refresh_pack_slots};
use mapping_session::{
    DiveAssetState, MappingAssetPipeline, click_element, save_overlay_action, trigger_download,
};
use toolbar::{StatusKind, ToolbarGroup, ToolbarIcon, ToolbarItem, ToolbarStrip};

/// Monotonic ids for the hidden upload inputs (one per mounted center).
static NEXT_UPLOAD_INPUT_ID: AtomicU64 = AtomicU64::new(0);

/// The Mapping view's center: the morphing toolbar + the one canvas.
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
    let mut focused = dive_focused;
    // Dive-scoped UI state: the asset pipeline's verdict, the view
    // toggles, the fit request, and upload plumbing. Held here (not in the
    // canvas host) so the toolbar and keyboard read the same signals.
    let dive_state = use_signal(DiveAssetState::default);
    let view_opts = use_signal(EditorViewOptions::default);
    let fit_pending = use_signal(|| true);
    let mut upload_error = use_signal(|| None::<String>);
    let upload_input_id = use_hook(|| {
        format!(
            "lpme-upload-{}",
            NEXT_UPLOAD_INPUT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
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
    // Sticky auto-pack slots: grown only when a never-slotted fixture
    // appears, and never re-packed — arranging one fixture must not move
    // another, and an undone arrange returns to the retained slot.
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
    let enter_focus = move |on_action: &EventHandler<UiAction>, node: NodeId| {
        enter_dive(on_action, focused, node)
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

    // One commit path for every session writer: canvas gestures, editor
    // keys, and the Props pane all bump the same counter, and the pipeline
    // applies the session's document once per bump.
    let mut dive_commits_signal = dive_commits;
    let on_committed = EventHandler::new(move |()| {
        let now = *dive_commits_signal.peek();
        dive_commits_signal.set(now + 1);
    });

    let dive_ready = *dive_state.read() == DiveAssetState::Ready;
    let dive_refusal = match &*dive_state.read() {
        DiveAssetState::Refused(refusal) => Some(refusal.clone()),
        _ => None,
    };
    let dived = focused_editor.is_some();

    // The toolbar: one strip, per-activity item lists (D1 — the morph is
    // a data swap).
    let groups = if let Some((_, label, editor)) = &focused_editor {
        let session_read = dive_session.read();
        dive_toolbar(
            label,
            &session_read.tool,
            *view_opts.read(),
            editor,
            dive_ready,
            upload_error.read().as_deref(),
        )
    } else {
        fixture_toolbar(
            selected_has_mapping(&surface, &selected),
            selected.is_some(),
            fixtures,
            arranged,
        )
    };
    let on_toolbar_item = {
        let adjust = adjust.clone();
        let arrange_verb = arrange_verb.clone();
        let selected = selected.clone();
        let focused_editor = focused_editor.clone();
        let surface_fixtures: Vec<(NodeId, bool)> = surface
            .fixtures
            .iter()
            .map(|fixture| (fixture.node, fixture.mapping_artifact.is_some()))
            .collect();
        let upload_input_id = upload_input_id.clone();
        let mut dive_session = dive_session;
        move |id: &'static str| match id {
            "dive.exit" => exit_focus(&on_action),
            "arrange.edit-mapping" => {
                if let Some((_, node, _)) = &selected
                    && surface_fixtures.iter().any(|(n, has)| n == node && *has)
                {
                    enter_focus(&on_action, *node);
                }
            }
            "arrange.rot-ccw" => adjust(&on_action, -15.0, 0.0),
            "arrange.rot-cw" => adjust(&on_action, 15.0, 0.0),
            "arrange.shrink" => adjust(&on_action, 0.0, 1.0 / 1.15),
            "arrange.grow" => adjust(&on_action, 0.0, 1.15),
            "arrange.undo" => arrange_verb(&on_action, EditorMetaVerb::Undo),
            "arrange.redo" => arrange_verb(&on_action, EditorMetaVerb::Redo),
            "tool.select" => dive_session.write().tool = MapTool::Select,
            "tool.grid" => dive_session.write().tool = MapTool::Grid,
            "tool.ring" => dive_session.write().tool = MapTool::Ring,
            "tool.path" => dive_session.write().tool = MapTool::path(),
            "view.numbers" => {
                let current = view_opts.peek().numbers;
                let mut view_opts = view_opts;
                view_opts.write().numbers = !current;
            }
            "view.arrows" => {
                let current = view_opts.peek().arrows;
                let mut view_opts = view_opts;
                view_opts.write().arrows = !current;
            }
            "view.live" => {
                let current = view_opts.peek().live;
                let mut view_opts = view_opts;
                view_opts.write().live = !current;
            }
            "view.fit-preview" => {
                let current = view_opts.peek().fit_preview;
                let mut view_opts = view_opts;
                view_opts.write().fit_preview = !current;
            }
            "file.upload" => click_element(&upload_input_id),
            "file.download" => {
                if let Some((_, _, editor)) = &focused_editor
                    && let Some(text) = editor.content.as_ref().and_then(|content| content.text())
                {
                    let pretty = Map2dDoc::from_json(text)
                        .map(|doc| doc.to_json_pretty())
                        .unwrap_or_else(|_| text.to_string());
                    let href = format!(
                        "data:application/json;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(pretty.as_bytes())
                    );
                    trigger_download(&editor.source, &href);
                }
            }
            "save.revert" => {
                if let Some((_, _, editor)) = &focused_editor {
                    on_action.call(editor.revert_action());
                }
            }
            "save.save" => on_action.call(save_overlay_action()),
            _ => {}
        }
    };

    // Journal stamping rides the pipeline's action stream: every ApplyBody
    // is an Edit on the focused node.
    let pipeline_action = focused_editor.as_ref().map(|(node, _, _)| {
        let node = *node;
        EventHandler::new(move |action: UiAction| {
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
        })
    });

    let upload_editor = focused_editor.as_ref().map(|(_, _, editor)| editor.clone());

    rsx! {
        div {
            class: "tw:flex tw:min-h-0 tw:flex-1 tw:flex-col tw:outline-none",
            tabindex: 0,
            onkeydown: {
                let arrange_verb = arrange_verb.clone();
                move |evt: KeyboardEvent| {
                    // Mode-scoped keys (ratified): dived, the session owns
                    // the whole editor grammar — and esc's last rung exits
                    // the dive (Q4). Not dived, the arrange byte stack
                    // answers ⌘Z alone.
                    if focused.peek().is_some() {
                        if dived && dive_ready {
                            let outcome = handle_editor_key(
                                dive_session,
                                view_opts,
                                fit_pending,
                                on_committed,
                                &evt,
                            );
                            if outcome == EditorKeyOutcome::ExitDive {
                                exit_focus(&on_action);
                            }
                        } else if matches!(evt.data().key(), Key::Escape) {
                            // A refused or still-loading dive: esc still
                            // leaves.
                            exit_focus(&on_action);
                        }
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
            ToolbarStrip { groups, on_item: on_toolbar_item }
            if let Some(error) = surface.editor_meta_error.clone() {
                div { class: "tw:flex-none tw:border-b tw:border-border-subtle tw:bg-status-attention-bg tw:px-2.5 tw:py-1 tw:text-[11px] tw:text-status-attention-foreground",
                    "editor.json refused: {error} — arranging is disabled so the file is never rewritten blind."
                }
            }
            if let Some(refusal) = &dive_refusal {
                // Refuse-don't-rewrite, in place of editability: the
                // fixture stays a visible sprite, tools stay disabled, and
                // the stored file is never touched.
                div { class: "tw:flex-none tw:border-b tw:border-border-subtle tw:bg-status-attention-bg tw:px-2.5 tw:py-1 tw:text-[11px] tw:text-status-attention-foreground",
                    "{refusal.message} — this mapping is not being edited, and the stored file has been left exactly as it is."
                }
            }
            div { class: "tw:relative tw:flex tw:min-h-0 tw:flex-1 tw:flex-col",
                // The non-visual asset pipeline: fetch → seed the shared
                // session (echo-suppressed) → apply on commit bumps.
                if let Some((_, _, editor)) = focused_editor.clone() {
                    MappingAssetPipeline {
                        editor,
                        session: dive_session,
                        commit_requests: dive_commits,
                        state: dive_state,
                        on_action: pipeline_action,
                    }
                }
                if let Some(editor) = upload_editor {
                    input {
                        id: "{upload_input_id}",
                        class: "lpme-hidden-input",
                        r#type: "file",
                        accept: ".json,application/json",
                        onchange: move |evt| {
                            let file = evt.files().first().cloned();
                            let editor = editor.clone();
                            async move {
                                let Some(file) = file else { return };
                                let name = file.name();
                                match file.read_string().await {
                                    Ok(text) => match DocOpen::parse(&name, &text) {
                                        DocOpen::Ready(parsed) => {
                                            upload_error.set(None);
                                            on_action.call(editor.apply_action(&parsed.to_json()));
                                        }
                                        // A file this build cannot read is
                                        // never applied to the fixture.
                                        DocOpen::Refused(refusal) => {
                                            upload_error.set(Some(refusal.message));
                                        }
                                    },
                                    Err(error) => {
                                        upload_error.set(Some(format!("could not read {name}: {error}")));
                                    }
                                }
                            }
                        },
                    }
                }
                if !surface.editor_meta_loaded && !dived {
                    div { class: "tw:flex tw:flex-1 tw:items-center tw:justify-center",
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Loading the arrangement…" }
                    }
                } else {
                    ProjectCanvasHost {
                        surface: surface.clone(),
                        bodies,
                        selection: selection.clone(),
                        pack,
                        dive: focused_editor.as_ref().map(|(node, _, _)| DiveHost {
                            node: *node,
                            session: dive_session,
                            editable: dive_ready,
                        }),
                        view_opts,
                        fit_pending,
                        on_committed,
                        on_focus: move |node| enter_focus(&on_action, node),
                        on_action,
                    }
                }
            }
        }
    }
}

/// The fixture activity's toolbar: breadcrumb root, arrange verbs for the
/// selection, history, and the counts readout.
fn fixture_toolbar(
    can_edit_mapping: bool,
    has_selection: bool,
    fixtures: usize,
    arranged: usize,
) -> Vec<ToolbarGroup> {
    vec![
        ToolbarGroup {
            id: "breadcrumb",
            trailing: false,
            items: vec![ToolbarItem::Status {
                text: "Project".to_string(),
                kind: StatusKind::Label,
            }],
        },
        ToolbarGroup {
            id: "arrange",
            trailing: false,
            items: vec![
                ToolbarItem::Button {
                    id: "arrange.edit-mapping",
                    icon: None,
                    label: Some("edit mapping".to_string()),
                    title:
                        "Edit the selected fixture's mapping (double-click on the canvas does too)"
                            .to_string(),
                    active: false,
                    enabled: can_edit_mapping,
                },
                ToolbarItem::Button {
                    id: "arrange.rot-ccw",
                    icon: None,
                    label: Some("⟲ 15°".to_string()),
                    title: "Rotate the selected fixture 15° counter-clockwise".to_string(),
                    active: false,
                    enabled: has_selection,
                },
                ToolbarItem::Button {
                    id: "arrange.rot-cw",
                    icon: None,
                    label: Some("⟳ 15°".to_string()),
                    title: "Rotate the selected fixture 15° clockwise".to_string(),
                    active: false,
                    enabled: has_selection,
                },
                ToolbarItem::Button {
                    id: "arrange.shrink",
                    icon: None,
                    label: Some("−".to_string()),
                    title: "Shrink the selected fixture".to_string(),
                    active: false,
                    enabled: has_selection,
                },
                ToolbarItem::Button {
                    id: "arrange.grow",
                    icon: None,
                    label: Some("+".to_string()),
                    title: "Grow the selected fixture".to_string(),
                    active: false,
                    enabled: has_selection,
                },
            ],
        },
        ToolbarGroup {
            id: "history",
            trailing: false,
            items: vec![
                ToolbarItem::Button {
                    id: "arrange.undo",
                    icon: None,
                    label: Some("↶".to_string()),
                    title: "Undo the last arrange edit (⌘Z)".to_string(),
                    active: false,
                    enabled: true,
                },
                ToolbarItem::Button {
                    id: "arrange.redo",
                    icon: None,
                    label: Some("↷".to_string()),
                    title: "Redo the last undone arrange edit (⇧⌘Z)".to_string(),
                    active: false,
                    enabled: true,
                },
            ],
        },
        ToolbarGroup {
            id: "counts",
            trailing: true,
            items: vec![ToolbarItem::Status {
                text: format!("{fixtures} fixtures · {arranged} arranged"),
                kind: StatusKind::Mono,
            }],
        },
    ]
}

/// The dive's toolbar: breadcrumb (root exits), tools, view toggles, and
/// the save cluster — ONE chrome row total.
fn dive_toolbar(
    label: &str,
    tool: &MapTool,
    opts: EditorViewOptions,
    editor: &UiAssetEditor,
    editable: bool,
    upload_error: Option<&str>,
) -> Vec<ToolbarGroup> {
    let dirty = editor.content.as_ref().is_some_and(|content| content.dirty);
    let has_content = editor
        .content
        .as_ref()
        .is_some_and(|content| content.text().is_some());
    let mut save_items = Vec::new();
    if let Some(failure) = &editor.failure {
        save_items.push(ToolbarItem::Status {
            text: failure.clone(),
            kind: StatusKind::Attention,
        });
    }
    if let Some(failure) = upload_error {
        save_items.push(ToolbarItem::Status {
            text: failure.to_string(),
            kind: StatusKind::Attention,
        });
    }
    if editor.in_flight {
        save_items.push(ToolbarItem::Status {
            text: "applying…".to_string(),
            kind: StatusKind::Mono,
        });
    }
    save_items.extend([
        ToolbarItem::Button {
            id: "file.upload",
            icon: Some(ToolbarIcon::Upload),
            label: None,
            title: "load a .map2d.json from disk (applies to this fixture)".to_string(),
            active: false,
            enabled: editable,
        },
        ToolbarItem::Button {
            id: "file.download",
            icon: Some(ToolbarIcon::Download),
            label: None,
            title: "download the current mapping document".to_string(),
            active: false,
            enabled: has_content,
        },
        ToolbarItem::Status {
            text: if dirty { "Unsaved" } else { "Saved" }.to_string(),
            kind: StatusKind::Mono,
        },
        ToolbarItem::Button {
            id: "save.revert",
            icon: Some(ToolbarIcon::Revert),
            label: Some("Revert".to_string()),
            title: "discard the applied edit and return to the saved file".to_string(),
            active: false,
            enabled: dirty,
        },
        ToolbarItem::Button {
            id: "save.save",
            icon: Some(ToolbarIcon::Save),
            label: Some("Save".to_string()),
            title: "save the project (writes the applied mapping to disk)".to_string(),
            active: false,
            enabled: dirty,
        },
    ]);
    vec![
        ToolbarGroup {
            id: "breadcrumb",
            trailing: false,
            items: vec![
                ToolbarItem::Link {
                    id: "dive.exit",
                    label: "‹ Project".to_string(),
                    title: "Leave the dive — back to the project canvas (esc)".to_string(),
                },
                ToolbarItem::Status {
                    text: format!("▸ {label}"),
                    kind: StatusKind::Label,
                },
            ],
        },
        ToolbarGroup {
            id: "tools",
            trailing: false,
            items: vec![
                ToolbarItem::Button {
                    id: "tool.select",
                    icon: Some(ToolbarIcon::Select),
                    label: None,
                    title: "select (V)".to_string(),
                    active: matches!(tool, MapTool::Select),
                    enabled: editable,
                },
                ToolbarItem::Button {
                    id: "tool.grid",
                    icon: Some(ToolbarIcon::Grid),
                    label: None,
                    title: "grid tool (G): click to drop a default grid".to_string(),
                    active: matches!(tool, MapTool::Grid),
                    enabled: editable,
                },
                ToolbarItem::Button {
                    id: "tool.ring",
                    icon: Some(ToolbarIcon::Ring),
                    label: None,
                    title: "ring tool (R): click to drop a default ring".to_string(),
                    active: matches!(tool, MapTool::Ring),
                    enabled: editable,
                },
                ToolbarItem::Button {
                    id: "tool.path",
                    icon: Some(ToolbarIcon::Path),
                    label: None,
                    title: "path tool (P): click vertices, Enter finishes".to_string(),
                    active: matches!(tool, MapTool::Path { .. }),
                    enabled: editable,
                },
            ],
        },
        ToolbarGroup {
            id: "views",
            trailing: false,
            items: vec![
                ToolbarItem::Button {
                    id: "view.numbers",
                    icon: Some(ToolbarIcon::Numbers),
                    label: None,
                    title: "wiring numbers (N)".to_string(),
                    active: opts.numbers,
                    enabled: editable,
                },
                ToolbarItem::Button {
                    id: "view.arrows",
                    icon: Some(ToolbarIcon::Arrows),
                    label: None,
                    title: "wiring arrows (A)".to_string(),
                    active: opts.arrows,
                    enabled: editable,
                },
                ToolbarItem::Button {
                    id: "view.live",
                    icon: None,
                    label: Some("live".to_string()),
                    title: "live output colors (L)".to_string(),
                    active: opts.live,
                    enabled: editable,
                },
                ToolbarItem::Button {
                    id: "view.fit-preview",
                    icon: Some(ToolbarIcon::FitPreview),
                    label: None,
                    title: "texture-frame preview — how the doc fits shader space (F)".to_string(),
                    active: opts.fit_preview,
                    enabled: editable,
                },
            ],
        },
        ToolbarGroup {
            id: "save",
            trailing: true,
            items: save_items,
        },
    ]
}

/// Enter the dive on `node`, stamping the edit journal exactly like the
/// toolbar path — ONE truth for the transition, shared by the center's
/// "edit mapping"/double-click and the Props placement card's action.
pub(crate) fn enter_dive(
    on_action: &EventHandler<UiAction>,
    mut dive_focused: Signal<Option<NodeId>>,
    node: NodeId,
) {
    let event = if dive_focused.peek().is_none() {
        Some(UiEditJournalEvent::ModeSwitch)
    } else if *dive_focused.peek() != Some(node) {
        Some(UiEditJournalEvent::NodeSwitch)
    } else {
        None
    };
    if let Some(event) = event {
        on_action.call(UiAction::from_op(
            lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
            ProjectEditorOp::EditorJournal {
                event,
                node: Some(node),
                mode: UiEditorMode::Mapping,
            },
        ));
    }
    dive_focused.set(Some(node));
}

/// Prebuild the arrange-op factory: `editor.json` artifact + the fixture
/// facts every write refreshes footprints through. `None` op = the
/// artifact is unknown (surface not settled), so verbs no-op honestly.
pub(crate) fn arrange_dispatch(
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
/// DTO whole (the dive's fetch/apply/refusal wiring). Same bytes the face
/// embeds edited; the canvas renders placeholders until they land.
type MappingAssets = (
    BTreeMap<ArtifactLocation, String>,
    BTreeMap<ArtifactLocation, UiAssetEditor>,
);

pub(crate) fn mapping_assets(project_editor: &ProjectEditorView) -> MappingAssets {
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
