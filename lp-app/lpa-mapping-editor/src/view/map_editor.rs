//! The integrated editor component: header + canvas + properties popover +
//! object rail over one session.
//!
//! Embeddable per the crate boundary: the host supplies the document (and a
//! `doc_epoch` bump to re-seed it) and receives committed documents via
//! `on_doc_change`; persistence stays outside. File ops render only when the
//! host provides [`EditorFileOps`] (the standalone page does; the fixture
//! face won't).

use dioxus::prelude::*;
use lpc_mapping::{Map2dDoc, bounds_of_points, resolve};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::map_tool::MapTool;
use crate::view::editor_canvas::{CanvasDrag, EditorCanvas};
use crate::view::editor_header::EditorHeader;
use crate::view::object_list::ObjectList;
use crate::view::properties_popover::PropertiesPopover;

/// Host-provided file operations (the header renders new/open/save when
/// present; save is a data-URL download built from the live document).
#[derive(Clone, Copy, PartialEq)]
pub struct EditorFileOps {
    pub on_new: EventHandler<()>,
    pub on_open: EventHandler<()>,
}

/// Canvas view toggles (editor defaults: wiring view on — this is the
/// editing surface, not the lit preview).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorViewOptions {
    pub numbers: bool,
    pub arrows: bool,
    pub universes: bool,
    pub fit_preview: bool,
}

impl Default for EditorViewOptions {
    fn default() -> Self {
        Self {
            numbers: true,
            arrows: true,
            universes: false,
            fit_preview: false,
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapEditor(
    /// Bump to re-seed the session from `doc` (open/new/external change).
    doc_epoch: u64,
    doc: Map2dDoc,
    #[props(default)] on_doc_change: Option<EventHandler<String>>,
    #[props(default)] file_ops: Option<EditorFileOps>,
    #[props(default = false)] scene_menu: bool,
    /// Deterministic mount state for stories: `[x, y, scale]` camera (fit
    /// runs when absent), preselected objects, an in-progress path draft,
    /// and view-option overrides.
    #[props(default)]
    initial_camera: Option<[f32; 3]>,
    #[props(default)] initial_selection: Vec<usize>,
    #[props(default)] initial_draft: Vec<[f32; 2]>,
    #[props(default)] initial_view: Option<EditorViewOptions>,
) -> Element {
    let mut session = use_signal(|| {
        let mut session = MapEditorSession::new(doc.clone());
        for index in &initial_selection {
            session.selection.objects.insert(*index);
        }
        if !initial_draft.is_empty() {
            session.tool = MapTool::Path {
                draft: initial_draft.clone(),
            };
        }
        session
    });
    let camera = use_signal(|| {
        initial_camera
            .map(|[x, y, scale]| Camera { x, y, scale })
            .unwrap_or_default()
    });
    let view_opts = use_signal(move || initial_view.unwrap_or_default());
    let viewport = use_signal(|| [1200.0f32, 800.0f32]);
    let mut fit_pending = use_signal(|| initial_camera.is_none());
    let drag = use_signal(|| None::<CanvasDrag>);

    // Re-seed when the host bumps the epoch (render-time guarded write).
    let mut seen_epoch = use_signal(|| doc_epoch);
    if *seen_epoch.peek() != doc_epoch {
        seen_epoch.set(doc_epoch);
        session.write().set_doc(doc.clone());
        fit_pending.set(true);
    }

    // One notifier for every committed change (canvas drags, popover edits,
    // keyboard ops): hand the host the serialized doc.
    let on_committed = EventHandler::new(move |()| {
        if let Some(handler) = &on_doc_change {
            handler.call(session.read().doc().to_json());
        }
    });

    // Fit runs as an effect: it writes the camera after render, whenever
    // requested (initial mount, epoch change, header "fit").
    {
        let mut camera = camera;
        let mut fit_pending_effect = fit_pending;
        use_effect(move || {
            if fit_pending_effect() {
                let bounds = {
                    let session_read = session.read();
                    let doc = session_read.doc();
                    resolve(doc)
                        .ok()
                        .and_then(|resolved| bounds_of_points(&resolved.positions()))
                        .or_else(|| doc.canvas_bounds())
                };
                if let Some(bounds) = bounds {
                    let [width, height] = *viewport.peek();
                    camera.write().fit(bounds, width, height, 60.0);
                }
                fit_pending_effect.set(false);
            }
        });
    }

    let tool_hint = match session.read().tool {
        MapTool::Select => {
            "click selects · ⇧-click adds · drag empty space for marquee · corners resize · ⌘Z undo"
        }
        MapTool::Grid => "click to drop a default grid — size it in the properties popover",
        MapTool::Ring => "click to drop a default ring — tune it in the properties popover",
        MapTool::Path { .. } => {
            "click to place lamps · ⏎ or double-click finishes · esc backs out one point"
        }
    };

    rsx! {
        div {
            class: "lpme-editor",
            tabindex: "0",
            onkeydown: move |evt| {
                handle_key(session, view_opts, fit_pending, on_committed, &evt);
            },
            EditorHeader {
                session,
                camera,
                view_opts,
                fit_pending,
                file_ops,
                scene_menu,
                on_doc_change,
            }
            div { class: "lpme-canvas-wrap",
                EditorCanvas {
                    session,
                    camera,
                    view_opts,
                    viewport,
                    drag,
                    on_committed,
                }
                ObjectList { session, on_committed }
                PropertiesPopover { session, camera, viewport, drag, on_committed }
                div { class: "lpme-hint", "{tool_hint}" }
            }
        }
    }
}

fn handle_key(
    mut session: Signal<MapEditorSession>,
    mut view_opts: Signal<EditorViewOptions>,
    mut fit_pending: Signal<bool>,
    on_committed: EventHandler<()>,
    evt: &Event<KeyboardData>,
) {
    let modifiers = evt.data().modifiers();
    let command = modifiers.meta() || modifiers.ctrl();
    match evt.data().key() {
        Key::Character(text) => {
            let key = text.to_lowercase();
            if command {
                match key.as_str() {
                    "z" => {
                        evt.prevent_default();
                        if modifiers.shift() {
                            session.write().redo();
                        } else {
                            session.write().undo();
                        }
                        on_committed.call(());
                    }
                    "a" => {
                        evt.prevent_default();
                        session.write().select_all();
                    }
                    _ => {}
                }
                return;
            }
            match key.as_str() {
                "v" => session.write().tool = MapTool::Select,
                "g" => session.write().tool = MapTool::Grid,
                "r" => session.write().tool = MapTool::Ring,
                "p" => session.write().tool = MapTool::path(),
                "n" => {
                    let current = view_opts.peek().numbers;
                    view_opts.write().numbers = !current;
                }
                "a" => {
                    let current = view_opts.peek().arrows;
                    view_opts.write().arrows = !current;
                }
                "u" => {
                    let current = view_opts.peek().universes;
                    view_opts.write().universes = !current;
                }
                "f" => {
                    let current = view_opts.peek().fit_preview;
                    view_opts.write().fit_preview = !current;
                }
                "0" => fit_pending.set(true),
                _ => {}
            }
        }
        Key::Escape => {
            let mut s = session.write();
            // D6: never discard work wholesale — back out one path vertex,
            // then clear selection, then fall back to the select tool.
            if s.path_backout() {
                return;
            }
            if !s.selection.is_empty() || s.selection.vertex.is_some() {
                s.selection.clear();
            } else {
                s.tool = MapTool::Select;
            }
        }
        Key::Enter => {
            if matches!(session.peek().tool, MapTool::Path { .. })
                && session.write().path_finish().is_some()
            {
                on_committed.call(());
            }
        }
        Key::Backspace | Key::Delete => {
            let had_selection = !session.peek().selection.is_empty();
            if had_selection {
                evt.prevent_default();
                session.write().delete_selection();
                on_committed.call(());
            }
        }
        _ => {}
    }
}
