//! The candidate menu: what a HOLD (or a right-click) on an ambiguous spot
//! offers instead of guessing.
//!
//! A click on a crowded seam can honestly mean three objects. The click
//! itself picks one and the next click cycles — but cycling is a guessing
//! game when the stack is deep, so a ~420 ms hold (which is also the mobile
//! long-press) and the right-button menu both NAME the candidates and let
//! the user say which.
//!
//! This is an HTML overlay, not an SVG layer: it renders as a sibling of
//! the canvas svg inside `.lpme-canvas-wrap` (the two share a box, so the
//! canvas-relative press point places the menu directly), and it is gated
//! on fixture mode so a dived canvas — and every story capture — can never
//! grow one.

use dioxus::prelude::*;

use super::CanvasDrag;
use super::layers::fixtures::{FixtureEvent, FixturePick, FixtureSprite, ObjectHit};

/// How long a press must sit still before it becomes a menu. Long enough
/// that a normal click never trips it, short enough that a deliberate hold
/// does not feel broken — and the same gesture mobile browsers already
/// treat as a long-press.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "the hold timer only exists in the browser")
)]
pub(crate) const HOLD_MS: u32 = 420;

/// An open candidate menu: where it sits, what it offers, and the fixture
/// facts every row needs to dispatch the same pick a click would have.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CandidateMenu {
    /// Anchor in CANVAS-BOX pixels. `.lpme-canvas` is `inset: 0` inside
    /// `.lpme-canvas-wrap`, so the press point measured against the canvas
    /// is already wrap-relative — no camera math at render time, and the
    /// menu therefore cannot drift (it closes on pan/zoom anyway).
    pub(crate) at: [f32; 2],
    pub(crate) key: String,
    pub(crate) fixture: String,
    /// The sprite's object colour, for the row swatches.
    pub(crate) color: String,
    /// The lamp the press named — carried through so picking a row
    /// dispatches exactly what clicking that object would have.
    pub(crate) lamp: Option<u32>,
    pub(crate) rows: Vec<CandidateRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CandidateRow {
    /// Index into the sprite's object list.
    pub(crate) object: usize,
    pub(crate) label: String,
    pub(crate) lamps: u32,
    /// A near-miss: inside the slop ring, outside the body. Shown muted,
    /// because it is an offer, not something the pointer is actually on.
    pub(crate) near: bool,
}

/// Build the menu for one press. Rows keep PAINT ORDER — the same order
/// clicking again cycles through — so the menu reads as the stack the
/// cycle walks rather than as a second, differently-sorted list.
pub(crate) fn build(
    sprite: &FixtureSprite,
    candidates: &[ObjectHit],
    at: [f32; 2],
    lamp: Option<u32>,
) -> CandidateMenu {
    let rows = candidates
        .iter()
        .filter_map(|hit| {
            let object = sprite.objects.get(hit.index)?;
            Some(CandidateRow {
                object: hit.index,
                label: object.label.clone(),
                lamps: object.lamps.1,
                near: hit.near,
            })
        })
        .collect();
    CandidateMenu {
        at,
        key: sprite.key.clone(),
        fixture: sprite.label.clone(),
        color: sprite.color.clone(),
        lamp,
        rows,
    }
}

/// Arm the hold-to-menu timer for a press.
///
/// `generation` is this press's ticket: every cancellation path (a move
/// past the drag threshold, pointer-up, pointer-leave, a new press) bumps
/// `hold_gen`, and a fired timer holding a stale ticket does nothing. That
/// is the whole cancellation mechanism — a `TimeoutFuture` cannot be
/// cancelled once spawned, and a wrongly-fired menu is worse than a
/// timer that runs to completion and shuts up.
///
/// Firing CONSUMES the press (`drag` is cleared), so the pointer-up that
/// follows neither selects nor commits a move.
pub(crate) fn arm_hold_menu(
    generation: u64,
    hold_gen: Signal<u64>,
    drag: Signal<Option<CanvasDrag>>,
    menu: Signal<Option<CandidateMenu>>,
    payload: CandidateMenu,
) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut drag = drag;
        let mut menu = menu;
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(HOLD_MS).await;
            if *hold_gen.peek() != generation {
                return;
            }
            // Belt and braces: the ticket already covers every cancel
            // path, but the menu must never open over a press that is
            // gone or has become a drag.
            if !matches!(
                drag.peek().as_ref(),
                Some(CanvasDrag::FixturePress { moved: false, .. })
            ) {
                return;
            }
            // A browser whose long-press `contextmenu` beat this timer
            // already opened the menu; re-opening would only move it.
            if menu.peek().is_some() {
                return;
            }
            drag.set(None);
            menu.set(Some(payload));
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Host builds have no timer to hang this on (and no pointer to
        // hold): the menu is a browser-only affordance.
        let _ = (generation, hold_gen, drag, menu, payload);
    }
}

/// The overlay: a click-away scrim under a small list of candidates.
///
/// The scrim is what makes "click away closes" and "a wheel closes" true
/// without the canvas having to know the menu exists — while it is up, it
/// is the thing under the pointer.
pub(crate) fn menu_overlay(
    open: CandidateMenu,
    menu: Signal<Option<CandidateMenu>>,
    on_fixture: EventHandler<FixtureEvent>,
) -> Element {
    let mut menu = menu;
    let CandidateMenu {
        at,
        key,
        fixture,
        color,
        lamp,
        rows,
    } = open;
    rsx! {
        div {
            class: "lpme-cmenu-scrim",
            onpointerdown: move |evt| {
                evt.stop_propagation();
                menu.set(None);
            },
            // Suppress the browser menu, but do NOT treat this as a
            // dismissal: a mobile long-press synthesizes `contextmenu`
            // ~80ms AFTER the hold already opened the menu, and by then
            // the scrim is what sits under the finger. Closing here would
            // make the menu unreachable on touch. Right-clicking the
            // scrim still closes it — through the pointerdown above.
            oncontextmenu: move |evt| evt.prevent_default(),
            onwheel: move |evt| {
                evt.prevent_default();
                menu.set(None);
            },
        }
        div {
            class: "lpme-cmenu",
            style: "left: {at[0]}px; top: {at[1]}px;",
            // A wheel over the list is a zoom that never arrives; close
            // rather than swallow it silently.
            onwheel: move |evt| {
                evt.prevent_default();
                menu.set(None);
            },
            div { class: "lpme-cmenu-head", "{fixture}" }
            for row in rows.iter() {
                button {
                    key: "{row.object}",
                    r#type: "button",
                    class: if row.near { "lpme-cmenu-row lpme-cmenu-row-near" } else { "lpme-cmenu-row" },
                    // The near marker is TEXT as well as colour: "close
                    // to" is a fact about the pointer, not decoration.
                    title: if row.near { "within the click slop" } else { "under the pointer" },
                    onclick: {
                        let key = key.clone();
                        let object = row.object;
                        move |_| {
                            on_fixture.call(FixtureEvent::Select(Some(FixturePick {
                                key: key.clone(),
                                lamp,
                                object: Some(object),
                            })));
                            menu.set(None);
                        }
                    },
                    span { class: "lpme-cmenu-swatch", style: "background: {color};" }
                    span { class: "lpme-cmenu-name", "{row.label}" }
                    span { class: "lpme-cmenu-lamps",
                        if row.near { "~{row.lamps}" } else { "{row.lamps}" }
                    }
                }
            }
        }
    }
}

/// A document-level `keydown` listener that closes the menu on Escape,
/// alive only while the menu is open.
///
/// Deliberately NOT a focused overlay: taking focus to hear a key would
/// steal it from the shell's `tabindex` container, and the editor's whole
/// keyboard grammar hangs off that. Listening at the document costs one
/// closure and breaks nothing.
#[cfg(target_arch = "wasm32")]
pub(crate) struct MenuEscListener {
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::KeyboardEvent)>,
}

#[cfg(target_arch = "wasm32")]
impl MenuEscListener {
    pub(crate) fn install(close: impl Fn() + 'static) -> Option<Self> {
        use wasm_bindgen::JsCast;
        let callback = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |evt: web_sys::KeyboardEvent| {
                if evt.key() == "Escape" {
                    close();
                }
            },
        );
        web_sys::window()?
            .document()?
            .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())
            .ok()?;
        Some(Self { callback })
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for MenuEscListener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            let _ = document.remove_event_listener_with_callback(
                "keydown",
                self.callback.as_ref().unchecked_ref(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_core::placement::Placement;
    use crate::view::canvas::layers::fixtures::{FixtureBody, SpriteObject};

    fn sprite() -> FixtureSprite {
        FixtureSprite {
            key: "dome".to_string(),
            label: "Dome".to_string(),
            color: "#8ab".to_string(),
            placement: Placement::IDENTITY,
            bounds: [0.0, 0.0, 10.0, 10.0],
            body: FixtureBody::Placeholder { lamps: 10 },
            arranged: true,
            selected: false,
            selected_range: None,
            objects: vec![
                SpriteObject {
                    label: "door".to_string(),
                    hull: Vec::new(),
                    outline: Vec::new(),
                    cells: Vec::new(),
                    lamps: (0, 12),
                    selected: false,
                },
                SpriteObject {
                    label: "sector 3".to_string(),
                    hull: Vec::new(),
                    outline: Vec::new(),
                    cells: Vec::new(),
                    lamps: (12, 30),
                    selected: false,
                },
            ],
        }
    }

    /// Rows name the objects, keep paint order, and carry the near flag
    /// the muted style reads.
    #[test]
    fn build_names_every_candidate_in_paint_order() {
        let candidates = [
            ObjectHit {
                index: 0,
                near: false,
            },
            ObjectHit {
                index: 1,
                near: true,
            },
        ];
        let menu = build(&sprite(), &candidates, [40.0, 12.0], Some(7));
        assert_eq!(menu.key, "dome");
        assert_eq!(menu.fixture, "Dome");
        assert_eq!(menu.lamp, Some(7));
        assert_eq!(menu.at, [40.0, 12.0]);
        let named: Vec<(usize, &str, u32, bool)> = menu
            .rows
            .iter()
            .map(|row| (row.object, row.label.as_str(), row.lamps, row.near))
            .collect();
        assert_eq!(
            named,
            vec![(0, "door", 12, false), (1, "sector 3", 30, true)]
        );
    }

    /// A candidate index the sprite no longer lists cannot become a row
    /// that dispatches a pick at nothing.
    #[test]
    fn build_drops_indexes_the_sprite_does_not_have() {
        let candidates = [ObjectHit {
            index: 9,
            near: false,
        }];
        assert!(
            build(&sprite(), &candidates, [0.0, 0.0], None)
                .rows
                .is_empty()
        );
    }
}
