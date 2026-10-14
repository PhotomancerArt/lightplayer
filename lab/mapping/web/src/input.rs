//! Browser events → model input. Translation only.

use dioxus::dioxus_core::{Runtime, current_scope_id};
use dioxus::html::geometry::WheelDelta;
use dioxus::html::input_data::MouseButton;
use dioxus::html::{Key as WebKey, Modifiers};
use dioxus::prelude::*;
use lab_mapping_model::{Button, Editor, Key, Mods, Vec2};
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

pub fn point(evt: &MouseEvent) -> Vec2 {
    let p = evt.element_coordinates();
    Vec2::new(p.x, p.y)
}

pub fn wheel_point(evt: &WheelEvent) -> Vec2 {
    let p = evt.element_coordinates();
    Vec2::new(p.x, p.y)
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

pub fn key_down(ed: &mut Signal<Editor>, evt: KeyboardEvent) {
    let m = mods(evt.modifiers());
    match key(&evt) {
        Some(k) => {
            if ed.write().key_down(k, m) {
                evt.prevent_default();
            }
        }
        // ⌘, ⌥, ⇧ on their own: the hint bar changes while they are held.
        None => ed.write().modifiers(m),
    }
}

pub fn key_up(ed: &mut Signal<Editor>, evt: KeyboardEvent) {
    let m = mods(evt.modifiers());
    match key(&evt) {
        Some(k) => ed.write().key_up(k, m),
        None => ed.write().modifiers(m),
    }
}

/// Listen for `mousemove` and `mouseup` on the window, once, for the page's
/// life. A drag follows the pointer anywhere; a release outside the window
/// is noticed on the first move back (no button held).
pub fn follow_window(ed: Signal<Editor>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let runtime = Runtime::current();
    let scope = current_scope_id();

    let on_move = {
        let runtime = runtime.clone();
        let mut ed = ed;
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
            let Some((at, inside)) = canvas_point(&evt) else {
                return;
            };
            let m = window_mods(&evt);
            runtime.in_scope(scope, || {
                let dragging = ed.peek().dragging();
                if dragging && evt.buttons() == 0 {
                    // Released somewhere we could not hear it.
                    let mut e = ed.write();
                    e.pointer_move(at, m);
                    e.pointer_up(at, m);
                } else if dragging || inside {
                    ed.write().pointer_move(at, m);
                } else if ed.peek().pointer.is_some() {
                    ed.write().pointer_leave();
                }
            });
        })
    };
    let on_up = {
        let mut ed = ed;
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
            let Some((at, _)) = canvas_point(&evt) else {
                return;
            };
            let m = window_mods(&evt);
            runtime.in_scope(scope, || {
                if ed.peek().dragging() {
                    ed.write().pointer_up(at, m);
                }
            });
        })
    };
    let _ = window.add_event_listener_with_callback("mousemove", on_move.as_ref().unchecked_ref());
    let _ = window.add_event_listener_with_callback("mouseup", on_up.as_ref().unchecked_ref());
    on_move.forget();
    on_up.forget();
}

/// The event's point relative to the canvas, and whether it is over it.
fn canvas_point(evt: &web_sys::MouseEvent) -> Option<(Vec2, bool)> {
    let el = web_sys::window()?
        .document()?
        .query_selector(".canvas-input")
        .ok()??;
    let r = el.get_bounding_client_rect();
    let x = evt.client_x() as f64 - r.left();
    let y = evt.client_y() as f64 - r.top();
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
