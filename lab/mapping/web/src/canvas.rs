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

use dioxus::prelude::*;
use lab_mapping_model::{
    Camera, Component, ComponentId, Editor, Fixture, Gesture, Object, ObjectShape, Rect, Target,
    Vec2, object::objects_of,
};

use crate::input;

#[component]
pub fn Canvas(ed: Signal<Editor>) -> Element {
    let mut ed = ed;
    let e = ed.read();
    let f = &e.fixture;
    let cam = e.camera;
    let context = e.context();
    let hover = e.hover_target();
    let bounds = screen_rect(&cam, f.bounds);

    let objects: Vec<ObjectView> = f
        .objects()
        .into_iter()
        .map(|o| object_view(f, &cam, &o, &e.selection, context.as_ref()))
        .collect();
    let draft: Vec<ObjectView> = e
        .draft()
        .map(|kind| {
            objects_of(&Component {
                id: ComponentId::new("draft"),
                kind,
            })
            .iter()
            .map(|o| draft_view(&cam, o, f.bounds))
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
        add(c, "outline-context", true);
    }
    for t in &e.selection {
        add(t, "outline-selected", !matches!(t, Target::Lamp { .. }));
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
                rect {
                    class: "bounds",
                    x: "{bounds.min.x}", y: "{bounds.min.y}",
                    width: "{bounds.width()}", height: "{bounds.height()}",
                }
                text { class: "bounds-label", x: "{bounds.min.x}", y: "{bounds.min.y - 8.0}",
                    "{bounds_caption(f_name(&ed))}"
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
                onmousemove: move |evt| ed.write().pointer_move(input::point(&evt), input::mods(evt.modifiers())),
                onmouseup: move |evt| ed.write().pointer_up(input::point(&evt), input::mods(evt.modifiers())),
                onmouseleave: move |_| ed.write().pointer_leave(),
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

fn f_name(ed: &Signal<Editor>) -> (String, f64, f64, usize) {
    let e = ed.read();
    let f = &e.fixture;
    (
        f.name.clone(),
        f.bounds.width(),
        f.bounds.height(),
        f.lamps_outside().len(),
    )
}

fn bounds_caption((name, w, h, outside): (String, f64, f64, usize)) -> String {
    let tail = match outside {
        0 => String::new(),
        1 => " · 1 lamp outside (unlit)".into(),
        n => format!(" · {n} lamps outside (unlit)"),
    };
    format!("{name} · {w} × {h}{tail}")
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
    f: &Fixture,
    cam: &Camera,
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
    let lamps = o
        .lamps
        .iter()
        .map(|l| {
            (
                cam.to_screen(l.pos),
                if f.bounds.contains(l.pos) {
                    "lamp"
                } else {
                    "lamp lamp-outside"
                },
            )
        })
        .collect();
    ObjectView {
        class,
        shape: shape_view(cam, &o.shape),
        lamps,
        lamp_r: lamp_radius(cam),
    }
}

fn draft_view(cam: &Camera, o: &Object, bounds: Rect) -> ObjectView {
    let lamps = o
        .lamps
        .iter()
        .map(|l| {
            (
                cam.to_screen(l.pos),
                if bounds.contains(l.pos) {
                    "lamp"
                } else {
                    "lamp lamp-outside"
                },
            )
        })
        .collect();
    ObjectView {
        class: "obj obj-draft",
        shape: shape_view(cam, &o.shape),
        lamps,
        lamp_r: lamp_radius(cam),
    }
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
