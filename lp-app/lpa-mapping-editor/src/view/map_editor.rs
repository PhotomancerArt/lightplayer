//! The integrated editor component: header + canvas over one session.
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
use crate::view::editor_canvas::EditorCanvas;
use crate::view::editor_header::EditorHeader;

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
) -> Element {
    let mut session = use_signal(|| MapEditorSession::new(doc.clone()));
    let camera = use_signal(Camera::new);
    let view_opts = use_signal(EditorViewOptions::default);
    let viewport = use_signal(|| [1200.0f32, 800.0f32]);
    let mut fit_pending = use_signal(|| true);

    // Re-seed when the host bumps the epoch (render-time guarded write).
    let mut seen_epoch = use_signal(|| doc_epoch);
    if *seen_epoch.peek() != doc_epoch {
        seen_epoch.set(doc_epoch);
        session.write().set_doc(doc.clone());
        fit_pending.set(true);
    }

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

    rsx! {
        div { class: "lpme-editor",
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
                EditorCanvas { session, camera, view_opts, viewport }
                div { class: "lpme-hint",
                    "drag to pan · scroll to pan · ⌘/ctrl-scroll or pinch to zoom"
                }
            }
        }
    }
}
