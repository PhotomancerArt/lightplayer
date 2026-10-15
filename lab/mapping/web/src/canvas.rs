//! The canvas: an SVG drawn in screen pixels from the model, plus a
//! transparent layer on top that takes every pointer event (so coordinates
//! are always relative to the canvas, never to whatever SVG shape was hit).
//!
//! The visual language of selection (D17). Every outline is the thing's
//! AREA (`lab_mapping_model::area`) — the same shape a click hits:
//! - hover: a thin blue outline of what a click would select;
//! - selected: a solid light outline, with its name above it;
//! - the level you are in (the selection's parent): a dashed outline;
//! - everything outside that level: dimmed.
//!
//! Nodes (D42, D43): modules are faint dashed frames, fixtures solid
//! frames, outputs cards with a row per port. The patch is drawn as violet
//! edges — violet is the product's colour for what is bound — from each
//! object's first lamp to the pixel its span starts on (or, for a crowded
//! fixture, one bundle from its frame), with an arrowhead where the data
//! arrives. The curve is the model's (`Edge::curve`), so hovering finds
//! exactly what is drawn.
//!
//! The board (D16, D44): each port row shows its pixels as the simulated
//! board has them (`Editor::device_frame`), and every lamp on the canvas is
//! drawn in its own pixel's colour from the same frame, so the drawing and
//! the strip read as one. While they move on their own — a selection's
//! pattern, a drawing in progress — a clock ticks at ~30 fps and only the
//! pixel and lamp layers redraw.
//!
//! Inside each fixture its wiring is drawn too (`Project::data_path`): a
//! quiet dashed wire from each object's last lamp to the next one's first,
//! and an "in" where a port's data enters. Handles (`handle.rs`) are small
//! squares on the selection.

use dioxus::prelude::*;
use lab_mapping_model::{
    Camera, Component, ComponentId, DeviceFrame, Editor, Gesture, HandleKind, NodeId, NodeKind,
    Object, ObjectId, ObjectShape, PixelGrid, Project, Rect, SizeEdge, Target, Tool, Vec2,
    area::{FRAME_TITLE_PX, title_width},
    geom::{along_polyline, polyline_length},
    handle::HANDLE_PX,
    layout::{CARD_HEADER, CARD_ROW},
    object::objects_of,
};

use crate::input;

/// How often the board's clock ticks while its pixels move, in ms.
const TICK_MS: u32 = 33;

#[component]
pub fn Canvas(ed: Signal<Editor>) -> Element {
    let mut ed = ed;
    // Moves and releases come from the whole window, so a drag keeps going
    // when the pointer leaves the canvas (or the window) and finishes right
    // when it comes back.
    use_hook(move || input::follow_window(ed));
    // The board's clock. It only advances while the pixels move, so a
    // still board costs no redraws.
    let mut now = use_signal(|| 0.0_f64);
    // One frame for both layers that show it (the port rows and the
    // lamps). Canvas never reads it, so a tick redraws only those layers.
    let frame = use_memo(move || ed.read().device_frame(now()));
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(TICK_MS).await;
            if ed.peek().board_animates() {
                now.set(seconds());
            }
        }
    });
    let e = ed.read();
    let f = &e.project;
    let cam = e.camera;
    let context = e.context();
    let hover = e.hover_target();
    let open = e.open_fixtures();
    let mut frames: Vec<FrameView> = Vec::new();
    let mut cards: Vec<CardView> = Vec::new();
    // Ports the selection lands on, tinted on their cards.
    let mut tinted: Vec<Target> = Vec::new();
    for t in &e.selection {
        for p in f.ports_of(t) {
            if !tinted.contains(&p) {
                tinted.push(p);
            }
        }
    }
    let patch = f.effective_patch();
    let font = card_font(&cam);
    let mut pixel_rows: Vec<PixelRow> = Vec::new();
    for n in f.all_nodes() {
        let Some(world) = f.node_rect(&n.id) else {
            continue;
        };
        let rect = screen_rect(&cam, world);
        let t = Target::Node(n.id.clone());
        let mut dimmed = context
            .as_ref()
            .is_some_and(|c| !f.is_ancestor_or_self(c, &t) && !f.is_ancestor_or_self(&t, c));
        match &n.kind {
            NodeKind::Module { .. } | NodeKind::Fixture(_) => frames.push(FrameView {
                rect,
                name: n.name.clone(),
                title_w: title_width(&n.name, n.pinned.is_some()),
                pinned: n.pinned.is_some(),
                dimmed,
                module: matches!(n.kind, NodeKind::Module { .. }),
            }),
            NodeKind::Output(o) => {
                // An output an open fixture feeds stays lit.
                if open.iter().any(|fx| f.outputs_of(fx).contains(&n.id)) {
                    dimmed = false;
                }
                let rows = o
                    .ports
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let top = world.min.y + CARD_HEADER + i as f64 * CARD_ROW;
                        let row = screen_rect(
                            &cam,
                            Rect::new(
                                Vec2::new(world.min.x, top),
                                Vec2::new(world.max.x, top + CARD_ROW),
                            ),
                        );
                        let used = patch.used(&n.id, &p.key);
                        let pt = Target::Port {
                            output: n.id.clone(),
                            key: p.key.clone(),
                        };
                        // The name on top, the pixels below it, where the
                        // model puts them (they are selectable).
                        if let Some(grid) = f.pixel_grid(&n.id, &p.key) {
                            pixel_rows.push(PixelRow {
                                output: n.id.clone(),
                                key: p.key.clone(),
                                grid,
                            });
                        }
                        PortRow {
                            rect: row,
                            name: p.name.clone(),
                            fill: format!("{used}/{} px", p.pixels),
                            tinted: tinted.contains(&pt),
                        }
                    })
                    .collect();
                cards.push(CardView {
                    rect,
                    name: n.name.clone(),
                    rows,
                    dimmed,
                    pinned: n.pinned.is_some(),
                });
            }
        }
    }
    // A module's frame goes under the frames inside it.
    frames.sort_by_key(|fr| !fr.module);
    // Edges landing on the same port stack their labels.
    let mut landed: Vec<Target> = Vec::new();
    let edges: Vec<EdgeView> = e
        .edges()
        .into_iter()
        .map(|x| {
            let stack = landed.iter().filter(|t| **t == x.to).count() as f64;
            if x.labelled {
                landed.push(x.to.clone());
            }
            let mid = x.point_at(&cam, 0.5) + Vec2::new(0.0, stack * 12.0);
            let mut class = String::from("edge");
            if x.bundled {
                class.push_str(" edge-bundled");
            }
            if x.selected {
                class.push_str(" edge-selected");
            } else if !e.selection.is_empty() {
                class.push_str(" edge-quiet");
            }
            if x.hovered {
                class.push_str(" edge-hover");
            }
            EdgeView {
                d: x.path(&cam),
                arrow: x.arrow_path(&cam),
                label: x.labelled.then_some(x.label.clone()),
                mid,
                class,
            }
        })
        .collect();
    // The lamps a selected port or pixel carries, ringed; and the pixel of
    // each selected lamp, ringed on its port (D47).
    let carried: Vec<Vec2> = e
        .marked_lamps()
        .iter()
        .filter_map(|l| f.lamps_under(l).first().map(|p| cam.to_screen(*p)))
        .collect();
    let marked_pixels: Vec<Rect> = e
        .marked_pixels()
        .iter()
        .filter_map(|t| match t {
            Target::Pixel { output, key, index } => f.pixel_rect(output, key, *index),
            _ => None,
        })
        .map(|r| screen_rect(&cam, r))
        .collect();

    let placed = f.placed_objects();
    let objects: Vec<ObjectView> = placed
        .iter()
        .map(|(fid, o)| object_view(f, &cam, fid, o, &e.selection, context.as_ref()))
        .collect();
    // Every lamp, drawn by the board layer in its pixel's colour.
    let lamp_r = lamp_radius(&cam);
    let mut lamp_dots: Vec<LampDot> = Vec::new();
    for ((fid, o), view) in placed.iter().zip(&objects) {
        let bounds = f.node_rect(fid);
        for (i, l) in o.lamps.iter().enumerate() {
            lamp_dots.push(LampDot {
                object: o.id.clone(),
                index: i as u32,
                at: cam.to_screen(l.pos),
                inside: bounds.is_some_and(|b| b.contains(l.pos)),
                dimmed: view.class.contains("obj-dimmed"),
            });
        }
    }
    // The wiring inside each fixture; bright where you're working.
    let mut wires: Vec<WireView> = Vec::new();
    for n in f.fixtures() {
        let on = open.contains(&n.id) || e.selection.contains(&Target::Node(n.id.clone()));
        let dp = f.data_path(&n.id);
        for (a, b) in dp.links {
            wires.push(WireView::link(
                cam.to_screen(a),
                cam.to_screen(b),
                lamp_r,
                on,
            ));
        }
        for entry in dp.entries {
            wires.push(WireView::entry(
                cam.to_screen(entry.at),
                entry.along,
                lamp_r,
                on,
            ));
        }
    }
    // A draft goes into the fixture it starts in.
    let draft_bounds = e.draft_fixture().and_then(|id| f.node_rect(&id));
    // The points of a path being clicked out.
    let draft_points: Vec<Vec2> = e.path_draft.iter().map(|p| cam.to_screen(*p)).collect();
    // The selection's handles, the hovered or dragged one marked.
    let hovered_handle = e.hover_handle().map(|h| h.kind);
    let dragged_handle = e.dragged_handle().cloned();
    let handles: Vec<(Rect, &'static str)> = e
        .handles()
        .into_iter()
        .map(|h| {
            let on = Some(&h.kind) == hovered_handle.as_ref()
                || Some(&h.kind) == dragged_handle.as_ref();
            (h.rect(&cam), if on { "handle handle-on" } else { "handle" })
        })
        .collect();
    let draft: Vec<ObjectView> = e
        .draft()
        .map(|kind| {
            objects_of(&Component {
                id: ComponentId::new("draft"),
                kind,
            })
            .iter()
            .map(|o| draft_view(&cam, o, draft_bounds))
            .collect()
        })
        .unwrap_or_default();

    let mut outlines: Vec<Outline> = Vec::new();
    let mut add = |t: &Target, class: &'static str, label: bool| {
        if let Some(a) = f.area(&cam, t) {
            let top = a.top();
            outlines.push(Outline {
                path: a.outline_path(),
                stroke: a.stroke_width(),
                class,
                label: label.then(|| (f.label(t), top)),
            });
        }
    };
    if let Some(c) = &context {
        // A node's title already names it; a port's row too.
        add(
            c,
            "outline-context",
            !matches!(c, Target::Node(_) | Target::Port { .. }),
        );
    }
    for t in &e.selection {
        // A node's title already names it; a port's row too; a pixel is
        // named in the inspector.
        let label = !matches!(
            t,
            Target::Lamp { .. } | Target::Node(_) | Target::Port { .. } | Target::Pixel { .. }
        );
        add(t, "outline-selected", label);
    }
    // In the right-click list, the item under the pointer is outlined;
    // otherwise, what a click would select.
    let menu_hover = e
        .menu
        .as_ref()
        .and_then(|m| m.hover.and_then(|i| m.items.get(i)))
        .map(|i| i.target.clone());
    if let Some(h) = menu_hover.or(hover)
        && !e.selection.contains(&h)
    {
        add(&h, "outline-hover", false);
    }
    type MenuView = (
        Vec2,
        Vec<(String, String)>,
        Vec<(usize, String, Option<usize>)>,
    );
    let menu: Option<MenuView> = e.menu.as_ref().map(|m| {
        (
            m.at,
            m.actions
                .iter()
                .map(|a| (a.label.clone(), a.keys.clone()))
                .collect(),
            m.items
                .iter()
                .enumerate()
                .map(|(i, it)| (it.depth, it.label.clone(), m.hover.filter(|h| *h == i)))
                .collect(),
        )
    });
    let box_rect = e.selection_box().map(|r| screen_rect(&cam, r));
    let note = e.cursor_note().zip(e.pointer);
    let handle_cursor = dragged_handle
        .as_ref()
        .or(hovered_handle.as_ref())
        .map(|k| match k {
            HandleKind::PathPoint { .. } => "move",
            HandleKind::RingRadius { .. } => "ew-resize",
            HandleKind::FixtureSize { edge, .. } => match edge {
                SizeEdge::Corner => "nwse-resize",
                SizeEdge::Right => "ew-resize",
                SizeEdge::Bottom => "ns-resize",
            },
        });
    let cursor = match (&e.gesture, e.space_held, e.tool) {
        (Gesture::Panning { .. }, _, _) => "grabbing",
        (_, true, _) => "grab",
        _ if handle_cursor.is_some() => handle_cursor.unwrap_or_default(),
        (Gesture::Moving { owners, nodes, .. }, _, _) if owners.is_empty() && nodes.is_empty() => {
            "not-allowed"
        }
        (Gesture::Moving { .. }, _, _) => "move",
        (_, _, Tool::Path | Tool::Circle) => "crosshair",
        _ => "default",
    };
    let vw = e.viewport.x;
    let vh = e.viewport.y;
    drop(e);

    rsx! {
        div {
            class: "canvas",
            style: "cursor: {cursor}",
            onresize: move |evt| {
                if let Ok(size) = evt.get_content_box_size() {
                    ed.write().resize(Vec2::new(size.width, size.height));
                }
            },
            svg {
                class: "canvas-svg",
                width: "{vw}",
                height: "{vh}",
                rect { class: "outside", x: "0", y: "0", width: "{vw}", height: "{vh}" }
                for (i, fr) in frames.iter().enumerate() {
                    g {
                        key: "f{i}",
                        class: match (fr.module, fr.dimmed) {
                            (true, true) => "frame module frame-dimmed",
                            (true, false) => "frame module",
                            (false, true) => "frame frame-dimmed",
                            (false, false) => "frame",
                        },
                        rect {
                            class: "frame-box",
                            x: "{fr.rect.min.x}", y: "{fr.rect.min.y}",
                            width: "{fr.rect.width()}", height: "{fr.rect.height()}",
                            rx: if fr.module { "6" } else { "0" },
                        }
                        rect {
                            class: "frame-tab",
                            x: "{fr.rect.min.x}", y: "{fr.rect.min.y - FRAME_TITLE_PX - 2.0}",
                            width: "{fr.title_w}", height: "{FRAME_TITLE_PX}",
                            rx: "3",
                        }
                        text {
                            class: "frame-title",
                            x: "{fr.rect.min.x + 7.0}", y: "{fr.rect.min.y - 7.0}",
                            "{fr.name}"
                        }
                        if fr.pinned {
                            {pin_mark(Vec2::new(fr.rect.min.x + fr.title_w - 9.0, fr.rect.min.y - 11.0))}
                        }
                    }
                }
                for (i, c) in cards.iter().enumerate() {
                    g { key: "c{i}", class: if c.dimmed { "card card-dimmed" } else { "card" },
                        rect {
                            class: "card-box",
                            x: "{c.rect.min.x}", y: "{c.rect.min.y}",
                            width: "{c.rect.width()}", height: "{c.rect.height()}",
                            rx: "4",
                        }
                        if c.pinned {
                            {pin_mark(Vec2::new(c.rect.max.x - font * 1.2, c.rect.min.y + font * 1.2))}
                        }
                        if font > 4.5 {
                            text {
                                class: "card-title",
                                style: "font-size: {font * 1.1}px",
                                x: "{c.rect.min.x + font * 0.7}",
                                y: "{c.rect.min.y + font * 1.75}",
                                "{c.name}"
                            }
                        }
                        for (j, r) in c.rows.iter().enumerate() {
                            g { key: "{j}",
                                if r.tinted {
                                    rect {
                                        class: "port-tint",
                                        x: "{r.rect.min.x}", y: "{r.rect.min.y}",
                                        width: "{r.rect.width()}", height: "{r.rect.height()}",
                                    }
                                }
                                line {
                                    class: "port-sep",
                                    x1: "{r.rect.min.x}", y1: "{r.rect.min.y}",
                                    x2: "{r.rect.max.x}", y2: "{r.rect.min.y}",
                                }
                                if font > 4.5 {
                                    text {
                                        class: "port-name",
                                        style: "font-size: {font}px",
                                        x: "{r.rect.min.x + font * 0.7}",
                                        y: "{r.rect.min.y + r.rect.height() * 0.28}",
                                        "{r.name}"
                                    }
                                    text {
                                        class: "port-fill",
                                        style: "font-size: {font * 0.9}px",
                                        x: "{r.rect.max.x - font * 0.7}",
                                        y: "{r.rect.min.y + r.rect.height() * 0.28}",
                                        text_anchor: "end",
                                        "{r.fill}"
                                    }
                                }
                            }
                        }
                    }
                }
                BoardPixels { frame, rows: pixel_rows, cam }
                for (i, x) in edges.iter().enumerate() {
                    g { key: "e{i}", class: "{x.class}",
                        path { class: "edge-line", d: "{x.d}" }
                        path { class: "edge-arrow", d: "{x.arrow}" }
                        if let Some(label) = &x.label {
                            text {
                                class: "edge-label",
                                x: "{x.mid.x}", y: "{x.mid.y - 4.0}",
                                text_anchor: "middle",
                                "{label}"
                            }
                        }
                    }
                }
                for (i, w) in wires.iter().enumerate() {
                    g { key: "w{i}", class: if w.on { "wire wire-on" } else { "wire" },
                        path { class: "wire-line", d: "{w.line}" }
                        path { class: "wire-arrow", d: "{w.arrow}" }
                        if let Some(at) = w.label {
                            text { class: "wire-in", x: "{at.x}", y: "{at.y}", text_anchor: "middle", "in" }
                        }
                    }
                }
                for (i, o) in objects.iter().enumerate() {
                    g { key: "{i}", class: "{o.class}",
                        {stroke(o)}
                        {arrow(o)}
                        if let Some(p) = o.lamps.first() {
                            circle { class: "lamp-start", cx: "{p.0.x}", cy: "{p.0.y}", r: "{o.lamp_r + 3.0}" }
                        }
                    }
                }
                BoardLamps { frame, lamps: lamp_dots, r: lamp_r }
                for (i, o) in draft.iter().enumerate() {
                    g { key: "d{i}", class: "{o.class}",
                        {stroke(o)}
                        {arrow(o)}
                        for (j, (p, c)) in o.lamps.iter().enumerate() {
                            circle { key: "{j}", class: "{c}", cx: "{p.x}", cy: "{p.y}", r: "{o.lamp_r}" }
                        }
                    }
                }
                for (i, p) in draft_points.iter().enumerate() {
                    rect {
                        key: "dp{i}",
                        class: "draft-point",
                        x: "{p.x - HANDLE_PX}", y: "{p.y - HANDLE_PX}",
                        width: "{2.0 * HANDLE_PX}", height: "{2.0 * HANDLE_PX}",
                    }
                }
                for (i, p) in carried.iter().enumerate() {
                    circle { key: "k{i}", class: "lamp-ring-port", cx: "{p.x}", cy: "{p.y}", r: "{lamp_r + 2.5}" }
                }
                for (i, r) in marked_pixels.iter().enumerate() {
                    rect {
                        key: "mp{i}",
                        class: "px-mark",
                        x: "{r.min.x - 1.5}", y: "{r.min.y - 1.5}",
                        width: "{r.width() + 3.0}", height: "{r.height() + 3.0}",
                    }
                }
                for (i, o) in outlines.iter().enumerate() {
                    g { key: "o{i}",
                        if let Some(w) = o.stroke {
                            // A path's area: a band round its polyline. The
                            // band is one stroke; its edge is a wider stroke
                            // with the band masked out of it.
                            mask {
                                id: "om{i}",
                                mask_units: "userSpaceOnUse",
                                x: "0", y: "0", width: "{vw}", height: "{vh}",
                                rect { x: "0", y: "0", width: "{vw}", height: "{vh}", fill: "white" }
                                path { class: "outline-mask", d: "{o.path}", style: "stroke-width: {w}px" }
                            }
                            path { class: "{o.class}-band", d: "{o.path}", style: "stroke-width: {w}px" }
                            path {
                                class: "{o.class}-edge",
                                d: "{o.path}",
                                mask: "url(#om{i})",
                                style: "stroke-width: {w + 3.0}px",
                            }
                        } else {
                            path { class: "{o.class}", d: "{o.path}", fill_rule: "evenodd" }
                        }
                        if let Some((label, at)) = &o.label {
                            text { class: "{o.class}-label", x: "{at.x}", y: "{at.y - 5.0}", text_anchor: "middle", "{label}" }
                        }
                    }
                }
                for (i, (r, class)) in handles.iter().enumerate() {
                    rect {
                        key: "h{i}",
                        class: "{class}",
                        x: "{r.min.x}", y: "{r.min.y}",
                        width: "{r.width()}", height: "{r.height()}",
                    }
                }
                if let Some(r) = box_rect {
                    rect { class: "select-box", x: "{r.min.x}", y: "{r.min.y}", width: "{r.width()}", height: "{r.height()}" }
                }
            }
            div {
                class: "canvas-input",
                oncontextmenu: move |evt| evt.prevent_default(),
                onmousedown: move |evt| {
                    if let Some(b) = input::button(&evt) {
                        evt.prevent_default();
                        ed.write().pointer_down(input::point(&evt), b, input::mods(evt.modifiers()));
                    }
                },
                ondoubleclick: move |evt| {
                    if ed.write().double_click(input::point(&evt), input::mods(evt.modifiers())) {
                        evt.prevent_default();
                    }
                },
                onwheel: move |evt| {
                    evt.prevent_default();
                    let d = input::wheel_delta(&evt);
                    ed.write().wheel(input::wheel_point(&evt), d, input::mods(evt.modifiers()));
                },
            }
            if let Some((at, actions, items)) = menu {
                div {
                    class: "menu",
                    style: "{menu_place(at, vw, vh)}",
                    onmouseleave: move |_| ed.write().menu_hover(None),
                    for (i, (label, keys)) in actions.into_iter().enumerate() {
                        div {
                            key: "a{i}",
                            class: "menu-item menu-action",
                            onmousedown: move |evt| evt.prevent_default(),
                            onclick: move |_| ed.write().menu_act(i),
                            span { "{label}" }
                            kbd { "{keys}" }
                        }
                    }
                    if !items.is_empty() {
                        div { class: "menu-title", "Under the cursor" }
                    }
                    for (i, (depth, label, hovered)) in items.into_iter().enumerate() {
                        div {
                            key: "{i}",
                            class: if hovered.is_some() { "menu-item on" } else { "menu-item" },
                            style: "padding-left: {10 + depth * 14}px",
                            onmouseenter: move |_| ed.write().menu_hover(Some(i)),
                            onmousedown: move |evt| evt.prevent_default(),
                            onclick: move |evt| ed.write().menu_choose(i, evt.modifiers().shift()),
                            "{label}"
                        }
                    }
                    div { class: "menu-foot", kbd { "⇧" } " adds · " kbd { "Esc" } " closes" }
                }
            }
            if let Some((text, at)) = note {
                div { class: "cursor-note", style: "left: {at.x + 16.0}px; top: {at.y + 18.0}px", "{text}" }
            }
        }
    }
}

/// One frame's box on screen — a module or a fixture — with its title tab.
struct FrameView {
    rect: Rect,
    name: String,
    /// Matches the title rect in the frame's hit area (`area.rs`).
    title_w: f64,
    dimmed: bool,
    module: bool,
    /// Placed where it was dropped, not by the layout (D48).
    pinned: bool,
}

/// An output's card: its title, then one row per port.
struct CardView {
    rect: Rect,
    name: String,
    rows: Vec<PortRow>,
    dimmed: bool,
    pinned: bool,
}

/// The pinned mark: a small pin, its head at `at` (screen).
fn pin_mark(at: Vec2) -> Element {
    rsx! {
        g { class: "pin-mark",
            line { x1: "{at.x}", y1: "{at.y}", x2: "{at.x}", y2: "{at.y + 6.0}" }
            circle { cx: "{at.x}", cy: "{at.y}", r: "2.6" }
        }
    }
}

struct PortRow {
    rect: Rect,
    name: String,
    /// `50/60 px`.
    fill: String,
    /// The selection lands here.
    tinted: bool,
}

/// One patch edge: a cubic from what it leaves to the port row it reaches,
/// and the arrowhead where it lands.
struct EdgeView {
    d: String,
    arrow: String,
    /// Only for edges attached to a selected thing itself.
    label: Option<String>,
    mid: Vec2,
    class: String,
}

/// Where one port's pixels go on its card: the model's grid, in world
/// units.
#[derive(Clone, PartialEq)]
struct PixelRow {
    output: NodeId,
    key: String,
    grid: PixelGrid,
}

/// Squares this big on screen get a hairline gap between them.
const GAP_PIXEL_PX: f64 = 4.0;

/// Every port's pixels, as the simulated board shows them now. Its own
/// component, so the clock's ticks redraw only this layer, not the canvas.
/// Drawn over the cards and never dimmed with them: the LEDs are the truth.
#[component]
fn BoardPixels(frame: Memo<DeviceFrame>, rows: Vec<PixelRow>, cam: Camera) -> Element {
    let frame = frame.read();
    let mut squares: Vec<(Rect, String)> = Vec::new();
    for row in &rows {
        let Some(pixels) = frame.port(&row.output, &row.key) else {
            continue;
        };
        let gap = if row.grid.side * cam.scale >= GAP_PIXEL_PX {
            1.0
        } else {
            0.0
        };
        for (i, px) in pixels.iter().enumerate() {
            let Some(world) = row.grid.rect(i as u32) else {
                continue;
            };
            let r = screen_rect(&cam, world);
            squares.push((
                Rect::new(r.min, r.max - Vec2::new(gap, gap)),
                format!("rgb({},{},{})", px.r, px.g, px.b),
            ));
        }
    }
    rsx! {
        g { class: "board-pixels",
            for (i, (r, fill)) in squares.into_iter().enumerate() {
                rect {
                    key: "{i}",
                    class: "px",
                    x: "{r.min.x}", y: "{r.min.y}",
                    width: "{r.width()}", height: "{r.height()}",
                    fill: "{fill}",
                }
            }
        }
    }
}

/// One lamp on the canvas, for the board layer to colour.
#[derive(Clone, PartialEq)]
struct LampDot {
    object: ObjectId,
    index: u32,
    at: Vec2,
    /// Inside its fixture's box: only these ever light (D4).
    inside: bool,
    /// Outside the level you're working in.
    dimmed: bool,
}

/// Every lamp, in its pixel's colour in the same frame the port rows show.
/// A lamp that lights nothing (unpatched, or outside its box) is a dim
/// hollow dot.
#[component]
fn BoardLamps(frame: Memo<DeviceFrame>, lamps: Vec<LampDot>, r: f64) -> Element {
    let frame = frame.read();
    let dots: Vec<(Vec2, Option<String>, bool)> = lamps
        .iter()
        .map(|l| {
            let fill = l
                .inside
                .then(|| frame.lamp(&l.object, l.index))
                .flatten()
                .map(|px| format!("rgb({},{},{})", px.r, px.g, px.b));
            (l.at, fill, l.dimmed)
        })
        .collect();
    rsx! {
        g { class: "board-lamps",
            for (i, (at, fill, dimmed)) in dots.into_iter().enumerate() {
                if let Some(fill) = fill {
                    circle {
                        key: "{i}",
                        class: if dimmed { "lamp-lit lamp-dimmed" } else { "lamp-lit" },
                        cx: "{at.x}", cy: "{at.y}", r: "{r}",
                        fill: "{fill}",
                    }
                } else {
                    circle {
                        key: "{i}",
                        class: if dimmed { "lamp-dark lamp-dimmed" } else { "lamp-dark" },
                        cx: "{at.x}", cy: "{at.y}", r: "{r}",
                    }
                }
            }
        }
    }
}

/// One stretch of a fixture's wiring, on screen: a dashed wire from one
/// object's last lamp to the next one's first, or the short lead where a
/// port's data comes in.
struct WireView {
    line: String,
    arrow: String,
    /// Where "in" goes, for an entry.
    label: Option<Vec2>,
    /// The fixture you're working in: drawn brighter.
    on: bool,
}

impl WireView {
    fn link(a: Vec2, b: Vec2, lamp_r: f64, on: bool) -> Self {
        let d = b - a;
        let len = d.length().max(1e-9);
        let u = d * (1.0 / len);
        // Stop short of the lamps, so the wire reads as between them.
        let from = a + u * (lamp_r + 1.5).min(len / 3.0);
        let tip = b - u * (lamp_r + 1.5).min(len / 3.0);
        WireView {
            line: format!("M{},{} L{},{}", from.x, from.y, tip.x, tip.y),
            arrow: arrowhead(tip, u),
            label: None,
            on,
        }
    }

    fn entry(at: Vec2, along: Vec2, lamp_r: f64, on: bool) -> Self {
        let tip = at - along * (lamp_r + 1.5);
        let tail = at - along * (lamp_r + 16.0);
        let label = at - along * (lamp_r + 23.0) + Vec2::new(0.0, 3.5);
        WireView {
            line: format!("M{},{} L{},{}", tail.x, tail.y, tip.x, tip.y),
            arrow: arrowhead(tip, along),
            label: Some(label),
            on,
        }
    }
}

/// A small arrowhead with its tip at `tip`, pointing along `u`.
fn arrowhead(tip: Vec2, u: Vec2) -> String {
    let n = Vec2::new(-u.y, u.x);
    let l = tip - u * 5.0 + n * 2.6;
    let r = tip - u * 5.0 - n * 2.6;
    format!("M{},{} L{},{} L{},{} Z", tip.x, tip.y, l.x, l.y, r.x, r.y)
}

/// Seconds since the page started: the board's clock.
fn seconds() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now() / 1000.0)
}

/// Card text is sized in world units, so it zooms with the card.
fn card_font(cam: &Camera) -> f64 {
    3.9 * cam.scale
}

struct ObjectView {
    class: &'static str,
    shape: ShapeView,
    lamps: Vec<(Vec2, &'static str)>,
    lamp_r: f64,
}

enum ShapeView {
    Polyline(Vec<Vec2>),
    Ring(Vec2, f64),
}

/// Where the right-click list opens: beside the cursor, flipped inward when
/// it would run off the canvas.
fn menu_place(at: Vec2, vw: f64, vh: f64) -> String {
    const W: f64 = 200.0;
    const H: f64 = 220.0;
    let x = if at.x + 4.0 + W > vw {
        (at.x - 4.0 - W).max(0.0)
    } else {
        at.x + 4.0
    };
    let y = if at.y + 4.0 + H > vh {
        (at.y - 4.0 - H).max(0.0)
    } else {
        at.y + 4.0
    };
    format!("left: {x}px; top: {y}px")
}

/// A selected, hovered or context outline: the thing's area as a path.
struct Outline {
    path: String,
    /// A path's area is its polyline stroked this wide (`Area::stroke_width`).
    stroke: Option<f64>,
    class: &'static str,
    /// Its name, and the top of its area to put it at.
    label: Option<(String, Vec2)>,
}

fn object_view(
    f: &Project,
    cam: &Camera,
    fixture: &NodeId,
    o: &Object,
    selection: &[Target],
    context: Option<&Target>,
) -> ObjectView {
    let target = f.object_target(&o.id);
    let chain = f.chain(&target);
    let selected = chain.iter().any(|t| selection.contains(t));
    // Dim what lies outside the level you are working in.
    let dimmed = context.is_some_and(|c| !chain.contains(c));
    let class = match (selected, dimmed) {
        (true, _) => "obj obj-selected",
        (false, true) => "obj obj-dimmed",
        _ => "obj",
    };
    let bounds = f.node_rect(fixture);
    // The lamps themselves are drawn by the board layer; these place the
    // start ring and the direction arrow.
    let lamps = lamp_views(cam, o, bounds);
    ObjectView {
        class,
        shape: shape_view(cam, &o.shape),
        lamps,
        lamp_r: lamp_radius(cam),
    }
}

fn draft_view(cam: &Camera, o: &Object, bounds: Option<Rect>) -> ObjectView {
    ObjectView {
        class: "obj obj-draft",
        shape: shape_view(cam, &o.shape),
        lamps: lamp_views(cam, o, bounds),
        lamp_r: lamp_radius(cam),
    }
}

/// Lamps on screen; one outside its own fixture's box is drawn hollow.
fn lamp_views(cam: &Camera, o: &Object, bounds: Option<Rect>) -> Vec<(Vec2, &'static str)> {
    o.lamps
        .iter()
        .map(|l| {
            let inside = bounds.is_some_and(|b| b.contains(l.pos));
            (
                cam.to_screen(l.pos),
                if inside { "lamp" } else { "lamp lamp-outside" },
            )
        })
        .collect()
}

fn shape_view(cam: &Camera, s: &ObjectShape) -> ShapeView {
    match s {
        ObjectShape::Polyline { points } => {
            ShapeView::Polyline(points.iter().map(|p| cam.to_screen(*p)).collect())
        }
        ObjectShape::Ring { center, radius } => {
            ShapeView::Ring(cam.to_screen(*center), radius * cam.scale)
        }
    }
}

/// Lamps grow with zoom, within limits, so they stay readable.
fn lamp_radius(cam: &Camera) -> f64 {
    (cam.scale * 0.55).clamp(1.8, 6.0)
}

fn stroke(o: &ObjectView) -> Element {
    match &o.shape {
        ShapeView::Polyline(points) => {
            let d: String = points
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}{},{} ", if i == 0 { "M" } else { "L" }, p.x, p.y))
                .collect();
            rsx! { path { class: "stroke", d: "{d}" } }
        }
        ShapeView::Ring(c, r) => {
            rsx! { circle { class: "stroke", cx: "{c.x}", cy: "{c.y}", r: "{r}" } }
        }
    }
}

/// A small arrowhead showing which way the data runs: halfway along a
/// path, just after the top on a ring.
fn arrow(o: &ObjectView) -> Element {
    let (at, dir) = match &o.shape {
        ShapeView::Polyline(points) => {
            match along_polyline(points, polyline_length(points) / 2.0) {
                Some(p) => p,
                None => return rsx! {},
            }
        }
        ShapeView::Ring(..) => match o.lamps.as_slice() {
            [first, second, ..] => (first.0.lerp(second.0, 0.5), second.0 - first.0),
            _ => return rsx! {},
        },
    };
    let len = dir.length();
    if len < 1e-6 {
        return rsx! {};
    }
    let d = dir * (1.0 / len);
    let n = Vec2::new(-d.y, d.x);
    let tip = at + d * 6.0;
    let l = at - d * 4.0 + n * 4.5;
    let r = at - d * 4.0 - n * 4.5;
    rsx! {
        path { class: "arrow", d: "M{tip.x},{tip.y} L{l.x},{l.y} L{r.x},{r.y} Z" }
    }
}

fn screen_rect(cam: &Camera, r: Rect) -> Rect {
    Rect::from_corners(cam.to_screen(r.min), cam.to_screen(r.max))
}
