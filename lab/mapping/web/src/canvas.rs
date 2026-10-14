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
//! edges — violet is the product's colour for what is bound — from a
//! fixture (or, inside one, each object) to the port row it lands on.

use dioxus::prelude::*;
use lab_mapping_model::{
    Camera, Component, ComponentId, Editor, Gesture, NodeId, NodeKind, Object, ObjectShape,
    Project, Rect, Target, Vec2,
    area::{FRAME_TITLE_PX, title_width},
    layout::{CARD_HEADER, CARD_ROW},
    object::objects_of,
};

use crate::input;

#[component]
pub fn Canvas(ed: Signal<Editor>) -> Element {
    let mut ed = ed;
    // Moves and releases come from the whole window, so a drag keeps going
    // when the pointer leaves the canvas (or the window) and finishes right
    // when it comes back.
    use_hook(move || input::follow_window(ed));
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
                title_w: title_width(&n.name),
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
                        PortRow {
                            rect: row,
                            name: p.name.clone(),
                            fill: format!("{used}/{} px", p.pixels),
                            ratio: (used as f64 / p.pixels.max(1) as f64).min(1.0),
                            tinted: tinted.contains(&pt),
                        }
                    })
                    .collect();
                cards.push(CardView {
                    rect,
                    name: n.name.clone(),
                    rows,
                    dimmed,
                });
            }
        }
    }
    // A module's frame goes under the frames inside it.
    frames.sort_by_key(|fr| !fr.module);
    let font = card_font(&cam);
    // Edges landing on the same port stack their labels.
    let mut landed: Vec<Target> = Vec::new();
    let edges: Vec<EdgeView> = e
        .edges()
        .into_iter()
        .map(|x| {
            let stack = landed.iter().filter(|t| **t == x.to).count() as f64;
            landed.push(x.to.clone());
            let a = cam.to_screen(x.from_at);
            let b = cam.to_screen(x.to_at);
            let dx = ((b.x - a.x).abs() * 0.5).max(30.0);
            let c1 = a + Vec2::new(dx, 0.0);
            let c2 = b - Vec2::new(dx, 0.0);
            let mid = (a + c1 * 3.0 + c2 * 3.0 + b) * 0.125 + Vec2::new(0.0, stack * 12.0);
            let class = match (x.selected, x.bundled, e.selection.is_empty()) {
                (true, true, _) => "edge edge-bundled edge-selected",
                (true, false, _) => "edge edge-selected",
                (false, true, true) => "edge edge-bundled",
                (false, false, true) => "edge",
                (false, true, false) => "edge edge-bundled edge-quiet",
                (false, false, false) => "edge edge-quiet",
            };
            EdgeView {
                d: format!(
                    "M{},{} C{},{} {},{} {},{}",
                    a.x, a.y, c1.x, c1.y, c2.x, c2.y, b.x, b.y
                ),
                label: x.label,
                mid,
                class,
            }
        })
        .collect();
    // The lamps a selected port carries.
    let mut carried: Vec<Vec2> = Vec::new();
    for t in &e.selection {
        if let Target::Port { output, key } = t {
            for (o, i) in f.lamps_on_port(output, key) {
                if let Some(p) = f
                    .lamps_under(&Target::Lamp {
                        object: o,
                        index: i,
                    })
                    .first()
                {
                    carried.push(cam.to_screen(*p));
                }
            }
        }
    }

    let objects: Vec<ObjectView> = f
        .placed_objects()
        .into_iter()
        .map(|(fid, o)| object_view(f, &cam, &fid, &o, &e.selection, context.as_ref()))
        .collect();
    // A draft goes into the fixture it was started in.
    let draft_bounds = match &e.gesture {
        Gesture::Drawing { start_world } => {
            f.fixture_at(*start_world).and_then(|id| f.node_rect(&id))
        }
        _ => None,
    };
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
                class,
                label: label.then(|| (f.label(t), top)),
            });
        }
    };
    if let Some(c) = &context {
        // A node's title already names it.
        add(c, "outline-context", !matches!(c, Target::Node(_)));
    }
    for t in &e.selection {
        // A node's title already names it; a port's row too.
        let label = !matches!(
            t,
            Target::Lamp { .. } | Target::Node(_) | Target::Port { .. }
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
    let menu: Option<(Vec2, Vec<(usize, String, Option<usize>)>)> = e.menu.as_ref().map(|m| {
        (
            m.at,
            m.items
                .iter()
                .enumerate()
                .map(|(i, it)| (it.depth, it.label.clone(), m.hover.filter(|h| *h == i)))
                .collect(),
        )
    });
    let box_rect = e.selection_box().map(|r| screen_rect(&cam, r));
    let note = e.cursor_note().zip(e.pointer);
    let cursor = match (&e.gesture, e.space_held, e.tool) {
        (Gesture::Panning { .. }, _, _) => "grabbing",
        (_, true, _) => "grab",
        (Gesture::Moving { .. }, _, _) => "move",
        (_, _, lab_mapping_model::Tool::Line | lab_mapping_model::Tool::Circle) => "crosshair",
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
                                        y: "{r.rect.min.y + r.rect.height() * 0.55}",
                                        "{r.name}"
                                    }
                                    text {
                                        class: "port-fill",
                                        style: "font-size: {font * 0.9}px",
                                        x: "{r.rect.max.x - font * 0.7}",
                                        y: "{r.rect.min.y + r.rect.height() * 0.55}",
                                        text_anchor: "end",
                                        "{r.fill}"
                                    }
                                }
                                rect {
                                    class: "port-bar-bg",
                                    x: "{r.rect.min.x + font * 0.7}",
                                    y: "{r.rect.max.y - r.rect.height() * 0.22}",
                                    width: "{(r.rect.width() - font * 1.4).max(0.0)}",
                                    height: "{(r.rect.height() * 0.09).max(1.0)}",
                                }
                                rect {
                                    class: "port-bar",
                                    x: "{r.rect.min.x + font * 0.7}",
                                    y: "{r.rect.max.y - r.rect.height() * 0.22}",
                                    width: "{(r.rect.width() - font * 1.4).max(0.0) * r.ratio}",
                                    height: "{(r.rect.height() * 0.09).max(1.0)}",
                                }
                            }
                        }
                    }
                }
                for (i, x) in edges.iter().enumerate() {
                    g { key: "e{i}", class: "{x.class}",
                        path { class: "edge-line", d: "{x.d}" }
                        text {
                            class: "edge-label",
                            x: "{x.mid.x}", y: "{x.mid.y - 4.0}",
                            text_anchor: "middle",
                            "{x.label}"
                        }
                    }
                }
                for (i, o) in objects.iter().chain(draft.iter()).enumerate() {
                    g { key: "{i}", class: "{o.class}",
                        {stroke(o)}
                        {arrow(o)}
                        for (j, (p, c)) in o.lamps.iter().enumerate() {
                            circle { key: "{j}", class: "{c}", cx: "{p.x}", cy: "{p.y}", r: "{o.lamp_r}" }
                        }
                        if let Some(p) = o.lamps.first() {
                            circle { class: "lamp-start", cx: "{p.0.x}", cy: "{p.0.y}", r: "{o.lamp_r + 3.0}" }
                        }
                    }
                }
                for (i, p) in carried.iter().enumerate() {
                    circle { key: "k{i}", class: "lamp-ring-port", cx: "{p.x}", cy: "{p.y}", r: "{lamp_radius(&cam) + 2.5}" }
                }
                for (i, o) in outlines.iter().enumerate() {
                    g { key: "o{i}",
                        path { class: "{o.class}", d: "{o.path}", fill_rule: "evenodd" }
                        if let Some((label, at)) = &o.label {
                            text { class: "{o.class}-label", x: "{at.x}", y: "{at.y - 5.0}", text_anchor: "middle", "{label}" }
                        }
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
                onwheel: move |evt| {
                    evt.prevent_default();
                    let d = input::wheel_delta(&evt);
                    ed.write().wheel(input::wheel_point(&evt), d, input::mods(evt.modifiers()));
                },
            }
            if let Some((at, items)) = menu {
                div {
                    class: "menu",
                    style: "{menu_place(at, vw, vh)}",
                    onmouseleave: move |_| ed.write().menu_hover(None),
                    div { class: "menu-title", "Under the cursor" }
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
}

/// An output's card: its title, then one row per port.
struct CardView {
    rect: Rect,
    name: String,
    rows: Vec<PortRow>,
    dimmed: bool,
}

struct PortRow {
    rect: Rect,
    name: String,
    /// `50/60 px`.
    fill: String,
    /// Used ÷ pixels, for the bar.
    ratio: f64,
    /// The selection lands here.
    tinted: bool,
}

/// One patch edge: a cubic from what it leaves to the port row it reaches.
struct EdgeView {
    d: String,
    label: String,
    mid: Vec2,
    class: &'static str,
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
    Segment(Vec2, Vec2),
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
        ObjectShape::Segment { from, to } => {
            ShapeView::Segment(cam.to_screen(*from), cam.to_screen(*to))
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
    match o.shape {
        ShapeView::Segment(a, b) => {
            rsx! { line { class: "stroke", x1: "{a.x}", y1: "{a.y}", x2: "{b.x}", y2: "{b.y}" } }
        }
        ShapeView::Ring(c, r) => {
            rsx! { circle { class: "stroke", cx: "{c.x}", cy: "{c.y}", r: "{r}" } }
        }
    }
}

/// A small arrowhead showing which way the data runs: halfway along a line,
/// just after lamp 0 on a ring.
fn arrow(o: &ObjectView) -> Element {
    let (at, dir) = match (&o.shape, o.lamps.as_slice()) {
        (ShapeView::Segment(a, b), _) => (a.lerp(*b, 0.5), *b - *a),
        (ShapeView::Ring(..), [first, second, ..]) => {
            (first.0.lerp(second.0, 0.5), second.0 - first.0)
        }
        _ => return rsx! {},
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
