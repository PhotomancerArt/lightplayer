//! Browser events → model input. Translation only.

use dioxus::html::geometry::WheelDelta;
use dioxus::html::input_data::MouseButton;
use dioxus::html::{Key as WebKey, Modifiers};
use dioxus::prelude::*;
use lab_mapping_model::{Button, Editor, Key, Mods, Vec2};

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
