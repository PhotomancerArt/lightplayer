//! The integrated editor component: header + canvas + properties popover +
//! object rail over one session.
//!
//! Embeddable per the crate boundary: the host supplies the document (and a
//! `doc_epoch` bump to re-seed it) and receives committed documents via
//! `on_doc_change`; persistence stays outside. File ops render only when the
//! host provides [`EditorFileOps`] (the standalone page does; the fixture
//! face won't).

use dioxus::prelude::*;
use dioxus_icons::lucide::List;
use lpc_mapping::{Map2dDoc, bounds_of_points, resolve};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::map_tool::MapTool;
use crate::editor_core::shape_path::ShapePath;
use crate::view::editor_canvas::{CanvasDrag, EditorCanvas};
use crate::view::editor_header::EditorHeader;
use crate::view::object_list::ObjectList;
use crate::view::properties_popover::PropertiesPopover;
use crate::view::reference::ReferenceImage;

/// Host-provided file operations (the header renders new/open/save when
/// present; save is a data-URL download built from the live document).
#[derive(Clone, Copy, PartialEq)]
pub struct EditorFileOps {
    pub on_new: EventHandler<()>,
    pub on_open: EventHandler<()>,
}

/// Host-provided reference-image operations (page host only, like
/// [`EditorFileOps`]): the header renders "ref" pick/clear/opacity controls
/// when present. The host owns loading and persistence; the editor only
/// reports what the user asked for.
#[derive(Clone, Copy, PartialEq)]
pub struct ReferenceOps {
    /// Open the host's file dialog.
    pub on_pick: EventHandler<()>,
    /// Opacity change or clear (`None`).
    pub on_change: EventHandler<Option<ReferenceImage>>,
}

/// Canvas view toggles (editor defaults: wiring view on — this is the
/// editing surface, not the lit preview).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorViewOptions {
    pub numbers: bool,
    pub arrows: bool,
    /// Fill lamps from host-supplied live colors (`live_colors` prop);
    /// without a feed this falls back to the object palette.
    pub live: bool,
    pub fit_preview: bool,
    /// Show the host-supplied reference image (`reference` prop); moot
    /// without one.
    pub reference: bool,
}

impl Default for EditorViewOptions {
    fn default() -> Self {
        Self {
            numbers: true,
            arrows: true,
            live: false,
            fit_preview: false,
            reference: true,
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
    /// Stories: descend into the (single) initial selection after seeding,
    /// so scoped-state captures are deterministic.
    #[props(default = false)]
    initial_descend: bool,
    #[props(default)] initial_draft: Vec<[f32; 2]>,
    #[props(default)] initial_view: Option<EditorViewOptions>,
    /// Host-owned view options (the fixture face's toggle bar drives the
    /// editor with the same buttons as display mode). When set, the header
    /// hides its own view-toggle cluster — the host chrome owns it.
    #[props(default)]
    shared_view: Option<Signal<EditorViewOptions>>,
    /// Live lamp colors indexed by wiring index, fed per frame by the host
    /// (the fixture face's control preview). Empty = no live feed.
    #[props(default)]
    live_colors: Vec<[u8; 3]>,
    /// Bump to request a zoom-to-fit without re-seeding the doc (the face
    /// bumps it on full-page expand/collapse — the viewport changes size
    /// and the user expects a re-fit).
    #[props(default)]
    refit_epoch: u64,
    /// Host-owned reference image for tracing, rendered under the authored
    /// geometry. Editor-side state only — never part of the document.
    #[props(default)]
    reference: Option<ReferenceImage>,
    /// Reference pick/clear/opacity controls render when present (page
    /// host); the fixture face omits it and loses nothing.
    #[props(default)]
    reference_ops: Option<ReferenceOps>,
    /// Neighbour fixtures rendered dimmed under the document (see
    /// [`crate::ContextFixture`]) — the dive's "others still visible".
    #[props(default)]
    context: Vec<crate::view::context_layer::ContextFixture>,
) -> Element {
    let mut session = use_signal(|| {
        let mut session = MapEditorSession::new(doc.clone());
        for index in &initial_selection {
            session.selection.insert_path(ShapePath::root(*index));
        }
        if initial_descend {
            session.descend();
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
    let local_view = use_signal(move || initial_view.unwrap_or_default());
    let view_opts = shared_view.unwrap_or(local_view);
    // Measured canvas size (CSS px), fed by the canvas's mount/resize
    // handlers. `None` until the first measurement — fit waits for it, so
    // the embedded editor fits its real box, not a guessed window.
    let viewport = use_signal(|| None::<[f32; 2]>);
    let mut fit_pending = use_signal(|| initial_camera.is_none());
    let drag = use_signal(|| None::<CanvasDrag>);
    // The wiring-order rail: closed by default (it hides the canvas), and
    // docked as a right-side pane that shrinks the view rather than
    // floating over it (M5 gate direction).
    let mut rail_open = use_signal(|| false);

    // The live feed crosses into the canvas as a SIGNAL so a 60Hz color
    // frame re-renders only this thin component, never the 1500-node lamp
    // tree (P2 of the view/edit-split plan; the canvas applies colors as
    // direct DOM writes). Keep-last-good lives here: an apply round-trip
    // empties the host feed for a frame or two, and falling back to the
    // palette would read as live mode dropping out.
    let mut live_feed = use_signal(Vec::<[u8; 3]>::new);
    if !live_colors.is_empty() && *live_feed.peek() != live_colors {
        live_feed.set(live_colors.clone());
    }

    // Re-seed when the host bumps the epoch (render-time guarded write).
    let mut seen_epoch = use_signal(|| doc_epoch);
    if *seen_epoch.peek() != doc_epoch {
        seen_epoch.set(doc_epoch);
        session.write().set_doc(doc.clone());
        fit_pending.set(true);
    }

    // Host-requested re-fit (expand/collapse): the canvas box is about to
    // change size, so drop the stale measurement — the fit effect waits
    // for the fresh one instead of fitting against the old box.
    let mut seen_refit = use_signal(|| refit_epoch);
    if *seen_refit.peek() != refit_epoch {
        seen_refit.set(refit_epoch);
        let mut viewport = viewport;
        viewport.set(None);
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
            // Reactive on BOTH: a pending fit waits for the first real
            // viewport measurement and re-runs when it lands.
            let viewport_now = viewport();
            if fit_pending_effect()
                && let Some([width, height]) = viewport_now
            {
                // Frame the authored canvas when there is one — the display
                // mode renders exactly that region, so the view ⇄ edit flip
                // (and explicit fit) land on the same framing. Docs without
                // a canvas fall back to the resolved lamp bounds.
                let bounds = {
                    let session_read = session.read();
                    let doc = session_read.doc();
                    doc.canvas_bounds().or_else(|| {
                        resolve(doc)
                            .ok()
                            .and_then(|resolved| bounds_of_points(&resolved.positions()))
                    })
                };
                if let Some(bounds) = bounds {
                    // Match the display-mode framing (`ux-map-inset`, 5.5%
                    // inset) so flipping view ⇄ edit keeps the fixture at
                    // the same on-screen size: pad by 5.5% of the limiting
                    // viewport dimension.
                    let width_limited =
                        bounds.width / bounds.height.max(1e-6) >= width / height.max(1.0);
                    let padding = 0.055 * if width_limited { width } else { height };
                    camera.write().fit(bounds, width, height, padding);
                }
                fit_pending_effect.set(false);
            }
        });
    }

    // The hint teaches the group grammar exactly when it applies (G1
    // feedback: double-click descend is undiscoverable without a prompt):
    // a selected group invites entering; a descended selection explains
    // write-through and the way out. Tool hints otherwise.
    let tool_hint = {
        let session_read = session.read();
        let selected_group = matches!(session_read.tool, MapTool::Select)
            && session_read.selection.single().is_some_and(|path| {
                path.resolve(session_read.doc()).is_some_and(|shape| {
                    crate::editor_core::shape_path::structural_child_count(shape) > 0
                })
            });
        let descended = matches!(session_read.tool, MapTool::Select)
            && session_read
                .selection
                .single()
                .is_some_and(|path| !path.is_root());
        match session_read.tool {
            MapTool::Select if selected_group => {
                "double-click enters the group — edit its sub-object with every instance live · esc leaves"
            }
            MapTool::Select if descended => {
                "editing the sub-object — every instance follows · esc leaves the group"
            }
            MapTool::Select => {
                "click selects · ⇧-click adds · drag empty space for marquee · corners resize · ⌘Z undo"
            }
            MapTool::Grid => "click to drop a default grid — size it in the properties popover",
            MapTool::Ring => "click to drop a default ring — tune it in the properties popover",
            MapTool::Path { .. } => {
                "click to place lamps · ⏎ or double-click finishes · esc backs out one point"
            }
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
                view_opts,
                external_view: shared_view.is_some(),
                fit_pending,
                file_ops,
                scene_menu,
                on_doc_change,
                reference: reference.clone(),
                reference_ops,
            }
            div { class: "lpme-body",
                div { class: "lpme-canvas-wrap",
                    EditorCanvas {
                        session,
                        camera,
                        view_opts,
                        viewport,
                        drag,
                        live_feed,
                        on_committed,
                        reference,
                        context: context.clone(),
                    }
                    PropertiesPopover { session, camera, viewport, drag, on_committed }
                    div { class: "lpme-hint", "{tool_hint}" }
                    ZoomFloat { camera, viewport, fit_pending }
                    div { class: "lpme-rail-float",
                        button {
                            class: if rail_open() { "lpme-btn lpme-btn-on" } else { "lpme-btn" },
                            title: "wiring order",
                            onclick: move |_| {
                                let now = *rail_open.peek();
                                rail_open.set(!now);
                                // The dock resizes the canvas: drop the
                                // stale measurement and re-fit so the
                                // content stays in view (same dance as
                                // full-page expand).
                                let mut viewport = viewport;
                                viewport.set(None);
                                fit_pending.set(true);
                            },
                            List { size: 13 }
                        }
                    }
                    HelpFloat {}
                }
                if rail_open() {
                    ObjectList {
                        session,
                        on_committed,
                        on_focus: move |index: usize| {
                            focus_object_in_view(session, camera, viewport, index);
                        },
                    }
                }
            }
        }
    }
}

/// Center the camera on one object's lamps (rail row click).
fn focus_object_in_view(
    session: Signal<MapEditorSession>,
    mut camera: Signal<Camera>,
    viewport: Signal<Option<[f32; 2]>>,
    object_index: usize,
) {
    let Some([width, height]) = *viewport.peek() else {
        return;
    };
    let bounds = {
        let session_read = session.read();
        let doc = session_read.doc();
        resolve(doc).ok().and_then(|resolved| {
            let points: Vec<[f32; 2]> = resolved
                .lamps
                .iter()
                .filter(|lamp| lamp.object as usize == object_index)
                .map(|lamp| lamp.pos)
                .collect();
            bounds_of_points(&points)
        })
    };
    if let Some(bounds) = bounds {
        camera.set(focus_camera(bounds, width, height));
    }
}

/// The camera that frames `bounds` for a rail-row focus: generous 20%
/// padding, capped well below max zoom so a small object doesn't slam the
/// view to 8× — capped focus stays centered on the object.
fn focus_camera(bounds: lpc_mapping::Bounds2d, width: f32, height: f32) -> Camera {
    const FOCUS_MAX_SCALE: f32 = 3.0;
    let mut cam = Camera::new();
    cam.fit(bounds, width, height, 0.20 * width.min(height));
    if cam.scale > FOCUS_MAX_SCALE {
        let center = [
            bounds.min_x + bounds.width / 2.0,
            bounds.min_y + bounds.height / 2.0,
        ];
        cam.scale = FOCUS_MAX_SCALE;
        cam.x = width / 2.0 - center[0] * FOCUS_MAX_SCALE;
        cam.y = height / 2.0 - center[1] * FOCUS_MAX_SCALE;
    }
    cam
}

/// Floating "?" toggling a compact keyboard-shortcut reference, next to
/// the zoom float (the M5 round-3 ask: every tool has a key, and the keys
/// deserve a home).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HelpFloat() -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        div { class: "lpme-help-float",
            button {
                class: if open() { "lpme-btn lpme-btn-on" } else { "lpme-btn" },
                title: "keyboard shortcuts",
                onclick: move |_| {
                    let now = *open.peek();
                    open.set(!now);
                },
                "?"
            }
        }
        if open() {
            div { class: "lpme-help-panel",
                div { class: "lpme-help-title", "keyboard" }
                for (keys, what) in [
                    ("V / G / R / P", "select · grid · ring · path tool"),
                    ("N / A / L", "numbers · arrows · live"),
                    ("F", "texture-frame preview"),
                    ("B", "reference image"),
                    ("0", "zoom to fit"),
                    ("⌘ + scroll", "zoom at cursor"),
                    ("right-drag / scroll", "pan"),
                    ("⌘Z / ⇧⌘Z", "undo · redo"),
                    ("⌘A", "select all"),
                    ("⌫", "delete selection"),
                    ("⏎", "finish path"),
                    ("dbl-click", "enter a group (edit its sub-object)"),
                    ("esc", "back out · leave group · clear · select tool"),
                ] {
                    div { class: "lpme-help-row",
                        span { class: "lpme-help-keys", "{keys}" }
                        span { class: "lpme-help-what", "{what}" }
                    }
                }
            }
        }
    }
}

/// Floating zoom control, bottom-right of the canvas pane (the M5 gate
/// direction: zoom is canvas furniture, not toolbar chrome). The percent
/// readout doubles as "zoom to fit". Anchors button zooms on the measured
/// viewport center.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ZoomFloat(
    camera: Signal<Camera>,
    viewport: Signal<Option<[f32; 2]>>,
    fit_pending: Signal<bool>,
) -> Element {
    let mut camera = camera;
    let mut fit_pending = fit_pending;
    let percent = (camera().scale * 100.0).round() as u32;
    let center = move || {
        viewport.peek().map_or([600.0, 400.0], |[width, height]| {
            [width / 2.0, height / 2.0]
        })
    };
    rsx! {
        div { class: "lpme-zoom-float",
            button {
                class: "lpme-btn",
                title: "zoom out",
                onclick: move |_| camera.write().zoom_at(center(), 0.8),
                "−"
            }
            button {
                class: "lpme-btn lpme-zoom-pct",
                title: "zoom to fit (0)",
                onclick: move |_| fit_pending.set(true),
                "{percent}%"
            }
            button {
                class: "lpme-btn",
                title: "zoom in",
                onclick: move |_| camera.write().zoom_at(center(), 1.25),
                "+"
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
                "l" => {
                    let current = view_opts.peek().live;
                    view_opts.write().live = !current;
                }
                "f" => {
                    let current = view_opts.peek().fit_preview;
                    view_opts.write().fit_preview = !current;
                }
                "b" => {
                    let current = view_opts.peek().reference;
                    view_opts.write().reference = !current;
                }
                "0" => fit_pending.set(true),
                _ => {}
            }
        }
        Key::Escape => {
            let mut s = session.write();
            // The ladder (D6 + the selection/tree ADR): back out one path
            // vertex, then drop a vertex sub-selection, then ASCEND out of
            // a descended group, then clear selection, then select tool.
            if s.path_backout() {
                return;
            }
            if s.selection.vertex.is_some() {
                s.selection.vertex = None;
                return;
            }
            if s.ascend() {
                return;
            }
            if !s.selection.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::Bounds2d;

    #[test]
    fn focus_camera_frames_large_bounds_with_padding() {
        let bounds = Bounds2d {
            min_x: 100.0,
            min_y: 100.0,
            width: 800.0,
            height: 200.0,
        };
        let cam = focus_camera(bounds, 600.0, 400.0);
        // Width-limited: usable 600 - 2*80 padding = 440 over 800 wide.
        assert!(
            (cam.scale - 440.0 / 800.0).abs() < 1e-3,
            "scale {}",
            cam.scale
        );
        // Bounds center lands on the viewport center.
        let center = cam.doc_to_view([500.0, 200.0]);
        assert!((center[0] - 300.0).abs() < 1e-2);
        assert!((center[1] - 200.0).abs() < 1e-2);
    }

    #[test]
    fn focus_camera_caps_zoom_on_tiny_objects_and_stays_centered() {
        let bounds = Bounds2d {
            min_x: 40.0,
            min_y: 40.0,
            width: 10.0,
            height: 10.0,
        };
        let cam = focus_camera(bounds, 600.0, 400.0);
        assert!((cam.scale - 3.0).abs() < 1e-6, "capped, got {}", cam.scale);
        let center = cam.doc_to_view([45.0, 45.0]);
        assert!((center[0] - 300.0).abs() < 1e-2);
        assert!((center[1] - 200.0).abs() < 1e-2);
    }
}
