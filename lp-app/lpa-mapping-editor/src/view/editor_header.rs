//! The editor header: title/status, tools, corpus scenes, file ops, view
//! toggles — the pinned home for editor chrome (M3 gate direction). Zoom
//! lives in the canvas pane's floating control, not here (M5 gate
//! direction).

use base64::Engine as _;
use dioxus::prelude::*;
use dioxus_icons::lucide::{
    CircleDashed, Grid3x3, Hash, Layers, MousePointer, Route, Scan, Spline,
};
use lpc_mapping::{Map2dDoc, corpus, resolve};

use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::map_tool::MapTool;
use crate::view::map_editor::{EditorFileOps, EditorViewOptions};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EditorHeader(
    session: Signal<MapEditorSession>,
    view_opts: Signal<EditorViewOptions>,
    /// The host chrome owns the view toggles (fixture-face embed): hide the
    /// header's own cluster.
    #[props(default = false)]
    external_view: bool,
    fit_pending: Signal<bool>,
    #[props(default)] file_ops: Option<EditorFileOps>,
    #[props(default = false)] scene_menu: bool,
    #[props(default)] on_doc_change: Option<EventHandler<String>>,
) -> Element {
    let opts = view_opts();
    let (lamp_count, universe_count, dirty, doc_json) = {
        let session_read = session.read();
        let resolved = resolve(session_read.doc()).ok();
        (
            resolved.as_ref().map_or(0, |r| r.lamps.len()),
            resolved.as_ref().map_or(0, |r| r.universe_count()),
            session_read.is_dirty(),
            session_read.doc().to_json_pretty(),
        )
    };
    let save_href = format!(
        "data:application/json;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(doc_json.as_bytes())
    );

    let toggle_class = |on: bool| {
        if on {
            "lpme-btn lpme-btn-on"
        } else {
            "lpme-btn"
        }
    };
    let mut load_scene = move |doc: Map2dDoc| {
        session.write().set_doc(doc);
        fit_pending.set(true);
        if let Some(handler) = &on_doc_change {
            handler.call(session.read().doc().to_json());
        }
    };

    rsx! {
        header { class: "lpme-header",
            span { class: "lpme-title", "mapping" }
            span { class: "lpme-status",
                "{lamp_count} lamps · {universe_count} u"
                if dirty {
                    span { class: "lpme-dirty", title: "unsaved changes", " ●" }
                }
            }
            span { class: "lpme-sep" }
            button {
                class: toggle_class(matches!(session.read().tool, MapTool::Select)),
                title: "select (V)",
                onclick: move |_| session.write().tool = MapTool::Select,
                MousePointer { size: 13 }
            }
            button {
                class: toggle_class(matches!(session.read().tool, MapTool::Grid)),
                title: "grid tool (G): click to drop a default grid",
                onclick: move |_| session.write().tool = MapTool::Grid,
                Grid3x3 { size: 13 }
            }
            button {
                class: toggle_class(matches!(session.read().tool, MapTool::Ring)),
                title: "ring tool (R): click to drop a default ring",
                onclick: move |_| session.write().tool = MapTool::Ring,
                CircleDashed { size: 13 }
            }
            button {
                class: toggle_class(matches!(session.read().tool, MapTool::Path { .. })),
                title: "path tool (P): click vertices, Enter finishes",
                onclick: move |_| session.write().tool = MapTool::path(),
                Spline { size: 13 }
            }
            div { class: "lpme-spacer" }
            if scene_menu {
                select {
                    class: "lpme-select",
                    title: "load a corpus scene",
                    onchange: move |evt| {
                        match evt.value().as_str() {
                            "button" => load_scene(corpus::basic_button()),
                            "ears" => load_scene(corpus::cat_ears()),
                            "panel" => load_scene(corpus::panel_16x16()),
                            "fyeah" => load_scene(corpus::fyeah()),
                            _ => {}
                        }
                    },
                    option { value: "", selected: true, disabled: true, "scenes…" }
                    option { value: "button", "button (rings)" }
                    option { value: "ears", "cat ears" }
                    option { value: "panel", "panel 16×16" }
                    option { value: "fyeah", "fyeah sign" }
                }
            }
            if let Some(ops) = file_ops {
                button {
                    class: "lpme-btn",
                    title: "new document",
                    onclick: move |_| ops.on_new.call(()),
                    "new"
                }
                button {
                    class: "lpme-btn",
                    title: "open a .map2d.json",
                    onclick: move |_| ops.on_open.call(()),
                    "open"
                }
                a {
                    class: "lpme-btn",
                    title: "download the document",
                    href: "{save_href}",
                    download: "fixture.map2d.json",
                    "save"
                }
            }
            if !external_view {
                span { class: "lpme-sep" }
                button {
                    class: toggle_class(opts.numbers),
                    title: "wiring numbers (N)",
                    onclick: move |_| view_opts.write().numbers = !opts.numbers,
                    Hash { size: 13 }
                }
                button {
                    class: toggle_class(opts.arrows),
                    title: "wiring arrows (A)",
                    onclick: move |_| view_opts.write().arrows = !opts.arrows,
                    Route { size: 13 }
                }
                button {
                    class: toggle_class(opts.universes),
                    title: "universe colors, 170 lamps each (U)",
                    onclick: move |_| view_opts.write().universes = !opts.universes,
                    Layers { size: 13 }
                }
                button {
                    class: toggle_class(opts.fit_preview),
                    title: "texture-frame preview — how the doc fits shader space (F)",
                    onclick: move |_| view_opts.write().fit_preview = !opts.fit_preview,
                    Scan { size: 13 }
                }
            }
        }
    }
}
