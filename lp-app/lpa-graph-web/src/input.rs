//! Browser events → model input. Translation only.

use dioxus::dioxus_core::{Runtime, current_scope_id};
use dioxus::html::geometry::WheelDelta;
use dioxus::html::input_data::MouseButton;
use dioxus::html::{Key as WebKey, Modifiers};
use dioxus::prelude::*;
use lab_mapping_model::{Button, GraphInput, Key, Mods, Vec2};

use crate::graph_editor::GraphEditor;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

pub fn mods(m: Modifiers) -> Mods {
    Mods {
        shift: m.shift(),
        alt: m.alt(),
        command: m.meta(),
        ctrl: m.ctrl(),
    }
}

/// A mouse event's point relative to the canvas. Measured from the
/// canvas's own box, whatever element the event landed on (a control inside
/// a face may take events one day, Q1).
pub fn client_point(evt: &MouseEvent) -> Option<Vec2> {
    let p = evt.client_coordinates();
    relative(p.x, p.y).map(|(at, _)| at)
}

pub fn wheel_client_point(evt: &WheelEvent) -> Option<Vec2> {
    let p = evt.client_coordinates();
    relative(p.x, p.y).map(|(at, _)| at)
}

pub fn button(evt: &MouseEvent) -> Option<Button> {
    match evt.trigger_button() {
        Some(MouseButton::Primary) => Some(Button::Primary),
        Some(MouseButton::Auxiliary) => Some(Button::Middle),
        Some(MouseButton::Secondary) => Some(Button::Secondary),
        _ => None,
    }
}

pub fn wheel_delta(evt: &WheelEvent) -> Vec2 {
    let v = evt.delta().strip_units();
    let k = match evt.delta() {
        WheelDelta::Pixels(_) => 1.0,
        WheelDelta::Lines(_) => 16.0,
        WheelDelta::Pages(_) => 400.0,
    };
    Vec2::new(v.x * k, v.y * k)
}

fn key(evt: &KeyboardEvent) -> Option<Key> {
    Some(match evt.key() {
        WebKey::Escape => Key::Escape,
        WebKey::Enter => Key::Enter,
        WebKey::Backspace | WebKey::Delete => Key::Delete,
        WebKey::ArrowLeft => Key::Left,
        WebKey::ArrowRight => Key::Right,
        WebKey::ArrowUp => Key::Up,
        WebKey::ArrowDown => Key::Down,
        WebKey::Character(s) if s == " " => Key::Space,
        WebKey::Character(s) => Key::Char(s.chars().next()?),
        _ => return None,
    })
}

pub fn key_down(ed: &GraphEditor, evt: KeyboardEvent) {
    let mods = mods(evt.modifiers());
    match key(&evt) {
        Some(key) => {
            if ed.input(GraphInput::KeyDown { key, mods }) {
                evt.prevent_default();
            }
        }
        // ⌘, ⌥, ⇧ on their own: the hint bar changes while they are held.
        None => {
            ed.input(GraphInput::Modifiers(mods));
        }
    }
}

pub fn key_up(ed: &GraphEditor, evt: KeyboardEvent) {
    let mods = mods(evt.modifiers());
    match key(&evt) {
        Some(key) => ed.input(GraphInput::KeyUp { key, mods }),
        None => ed.input(GraphInput::Modifiers(mods)),
    };
}

/// The window's `mousemove` and `mouseup` listeners for one canvas. They
/// are taken off the window when this is dropped: the canvas is mounted
/// again when the size class changes, and a listener left behind would run
/// in a scope that is gone.
pub struct WindowListeners {
    window: web_sys::Window,
    on_move: Closure<dyn FnMut(web_sys::MouseEvent)>,
    on_up: Closure<dyn FnMut(web_sys::MouseEvent)>,
}

impl Drop for WindowListeners {
    fn drop(&mut self) {
        let _ = self.window.remove_event_listener_with_callback(
            "mousemove",
            self.on_move.as_ref().unchecked_ref(),
        );
        let _ = self
            .window
            .remove_event_listener_with_callback("mouseup", self.on_up.as_ref().unchecked_ref());
    }
}

/// Listen for `mousemove` and `mouseup` on the window while the canvas is
/// mounted. A drag follows the pointer anywhere; a release outside the
/// window is noticed on the first move back (no button held).
pub fn follow_window(ed: GraphEditor) -> Option<WindowListeners> {
    let window = web_sys::window()?;
    let runtime = Runtime::current();
    let scope = current_scope_id();

    let on_move = {
        let runtime = runtime.clone();
        let ed = ed.clone();
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
            let Some((at, inside)) = canvas_point(&evt) else {
                return;
            };
            let m = window_mods(&evt);
            runtime.in_scope(scope, || {
                let dragging = ed.peek().dragging();
                if dragging && evt.buttons() == 0 {
                    // Released somewhere we could not hear it.
                    ed.input(GraphInput::PointerMove { at, mods: m });
                    ed.input(GraphInput::PointerUp { at, mods: m });
                } else if dragging || inside {
                    ed.input(GraphInput::PointerMove { at, mods: m });
                    let edge = crate::edge_layer::hit(&ed.peek(), at);
                    ed.input(GraphInput::HoveredEdge(edge));
                } else if ed.peek().pointer.is_some() {
                    ed.input(GraphInput::PointerLeave);
                }
            });
        })
    };
    let on_up = {
        let ed = ed.clone();
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
            let Some((at, _)) = canvas_point(&evt) else {
                return;
            };
            let m = window_mods(&evt);
            runtime.in_scope(scope, || {
                if ed.peek().dragging() {
                    ed.input(GraphInput::PointerUp { at, mods: m });
                }
            });
        })
    };
    let _ = window.add_event_listener_with_callback("mousemove", on_move.as_ref().unchecked_ref());
    let _ = window.add_event_listener_with_callback("mouseup", on_up.as_ref().unchecked_ref());
    Some(WindowListeners {
        window,
        on_move,
        on_up,
    })
}

/// The event's point relative to the canvas, and whether it is over it.
fn canvas_point(evt: &web_sys::MouseEvent) -> Option<(Vec2, bool)> {
    relative(evt.client_x() as f64, evt.client_y() as f64)
}

/// A point in the window, relative to the canvas, and whether it is over
/// it.
fn relative(client_x: f64, client_y: f64) -> Option<(Vec2, bool)> {
    let el = web_sys::window()?
        .document()?
        .query_selector(".graph .canvas")
        .ok()??;
    let r = el.get_bounding_client_rect();
    let x = client_x - r.left();
    let y = client_y - r.top();
    let inside = x >= 0.0 && y >= 0.0 && x < r.width() && y < r.height();
    Some((Vec2::new(x, y), inside))
}

fn window_mods(evt: &web_sys::MouseEvent) -> Mods {
    Mods {
        shift: evt.shift_key(),
        alt: evt.alt_key(),
        command: evt.meta_key(),
        ctrl: evt.ctrl_key(),
    }
}
