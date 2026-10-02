//! [`CommandPaletteDialog`]: the open palette — a query field over every
//! offer the view publishes, best match first.
//!
//! The rows are the offer tree's own ([`UiOfferTree::search`] ranks them in
//! core); this file only draws them and answers keys. Each row is an
//! [`ActionButton`] menu item, so the palette wears the same treatment as
//! every other surface: Undoable rows are tinted, Lasting rows arm before
//! they press, disabled rows say why. Beside the label sits the offer's
//! path, quietly — the id the app agent presses the same verb by.
//!
//! Keys stay in the query field the whole time the palette is open (Tab
//! is held, and the rows never take focus): ↑/↓ move the highlight over
//! the pressable rows, Enter presses it (a Lasting row arms first), Esc
//! closes. The grammar itself is [`PaletteCursor`], a plain value.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::{UiAction, UiOfferTree};

use super::palette_cursor::{PaletteCursor, PaletteKey, PaletteOutcome};
use crate::core::action::armed_confirm_button::ARMED_CONFIRM_WINDOW_MS;
use crate::core::{ActionButton, ActionButtonVariant};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CommandPaletteDialog(
    /// The view's offer tree, as published.
    offers: UiOfferTree,
    /// Dispatch a pressed offer's action, exactly as published.
    on_action: EventHandler<UiAction>,
    /// Close the palette (Esc, a click outside, or after a press).
    on_close: EventHandler<()>,
    /// Stories: pin the palette in its box instead of over the viewport,
    /// and leave focus where it is.
    #[props(default)]
    inline: bool,
    /// Stories: pre-type a query (a capture cannot type).
    #[props(default)]
    initial_query: String,
    /// Stories: start with the highlighted row armed, as one Enter on a
    /// Lasting row leaves it.
    #[props(default)]
    armed_preview: bool,
) -> Element {
    let mut query = use_signal(|| initial_query.clone());
    let mut cursor = use_signal(|| {
        let mut cursor = PaletteCursor::default();
        // Stories: one Enter, replayed at mount, arms a Lasting row. It
        // never stands down on its own, so a capture is deterministic.
        if armed_preview
            && cursor.key(PaletteKey::Enter, &offers.search(&initial_query))
                != PaletteOutcome::Armed
        {
            cursor = PaletteCursor::default();
        }
        cursor
    });
    let mut arm_generation = use_signal(|| 0u64);
    let list_id = use_hook(next_list_id);
    use_restore_focus(inline);

    let typed = query.read().clone();
    let rows = offers.search(&typed);
    let highlight = cursor.read().highlighted(&rows);
    let active_row = highlight.map(|at| row_id(&list_id, at));
    let tree_empty = offers.is_empty();

    let keys_offers = offers.clone();
    let keys_list_id = list_id.clone();
    let on_keydown = move |event: KeyboardEvent| {
        let key = match event.key() {
            Key::ArrowDown => PaletteKey::Down,
            Key::ArrowUp => PaletteKey::Up,
            Key::Enter => PaletteKey::Enter,
            Key::Escape => PaletteKey::Escape,
            // Focus stays in the field while the palette is open.
            Key::Tab => {
                event.prevent_default();
                return;
            }
            _ => return,
        };
        event.prevent_default();
        let typed = query.read().clone();
        let rows = keys_offers.search(&typed);
        let outcome = cursor.write().key(key, &rows);
        match outcome {
            PaletteOutcome::Stay => {
                if let Some(at) = cursor.read().highlighted(&rows) {
                    reveal_row(&row_id(&keys_list_id, at));
                }
            }
            PaletteOutcome::Armed => {
                // The arm stands down on its own, like the button's.
                let generation = *arm_generation.peek() + 1;
                arm_generation.set(generation);
                spawn(async move {
                    TimeoutFuture::new(ARMED_CONFIRM_WINDOW_MS).await;
                    if *arm_generation.peek() == generation {
                        cursor.write().disarm();
                    }
                });
            }
            PaletteOutcome::Press(action) => {
                on_action.call(action);
                on_close.call(());
            }
            PaletteOutcome::Close => on_close.call(()),
        }
    };

    let frame_class = if inline {
        INLINE_FRAME_CLASS
    } else {
        OVERLAY_CLASS
    };
    rsx! {
        div { class: frame_class,
            if !inline {
                // A click outside the panel closes it.
                div {
                    class: "tw:absolute tw:inset-0 tw:bg-black/50",
                    aria_hidden: "true",
                    onclick: move |_| on_close.call(()),
                }
            }
            div {
                class: PANEL_CLASS,
                role: "dialog",
                aria_modal: "true",
                aria_label: "Command palette",
                input {
                    class: INPUT_CLASS,
                    r#type: "text",
                    role: "combobox",
                    aria_label: "Find an action",
                    aria_expanded: "true",
                    aria_controls: "{list_id}",
                    aria_autocomplete: "list",
                    aria_activedescendant: active_row.clone().unwrap_or_default(),
                    placeholder: "Find an action…",
                    autocomplete: "off",
                    spellcheck: "false",
                    value: "{typed}",
                    onmounted: move |event| {
                        if !inline {
                            spawn(async move {
                                let _ = event.data().set_focus(true).await;
                            });
                        }
                    },
                    oninput: move |event| {
                        query.set(event.value());
                        cursor.write().reset();
                    },
                    onkeydown: on_keydown,
                }
                div {
                    id: "{list_id}",
                    class: LIST_CLASS,
                    role: "listbox",
                    aria_label: "Actions",
                    // The rows never take focus: the query field keeps it.
                    onmousedown: move |event| event.prevent_default(),
                    if tree_empty {
                        p { class: EMPTY_CLASS, "Nothing to press here yet." }
                    } else if rows.is_empty() {
                        p { class: EMPTY_CLASS, "Nothing here matches “{typed}”." }
                    }
                    for (at , offer) in rows.iter().enumerate() {
                        {
                            let highlighted = highlight == Some(at);
                            let armed = cursor.read().is_armed(&offer.path);
                            let hovered = (*offer).clone();
                            let published = offer.action.clone();
                            // The row draws with the offer's icon token, the
                            // one every surface draws it with; the press
                            // dispatches the action exactly as published.
                            let shown = offer.action.clone().with_icon(offer.icon.clone());
                            rsx! {
                                div {
                                    // The arm is part of the key: arming from
                                    // the keyboard remounts the button armed.
                                    key: "{offer.path}-{armed}",
                                    id: row_id(&list_id, at),
                                    class: row_class(highlighted, armed),
                                    role: "option",
                                    aria_selected: if highlighted { "true" } else { "false" },
                                    aria_disabled: if offer.is_enabled() { "false" } else { "true" },
                                    onmouseenter: move |_| cursor.write().hover(&hovered),
                                    // The disabled reason the button prints under
                                    // its label lines up with the label.
                                    div { class: "tw:min-w-0 tw:flex-1 tw:[&_p]:px-2 tw:[&_p]:pb-1.5",
                                        ActionButton {
                                            action: shown,
                                            running: false,
                                            variant: ActionButtonVariant::MenuItem,
                                            armed_preview: armed,
                                            on_action: move |_| {
                                                on_action.call(published.clone());
                                                on_close.call(());
                                            },
                                        }
                                    }
                                    // Clipped from the START: the verb at the
                                    // path's end is the part worth reading.
                                    span { class: PATH_CLASS, dir: "rtl", title: "{offer.path}",
                                        bdi { dir: "ltr", "{offer.path}" }
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    class: FOOTER_CLASS,
                    onmousedown: move |event| event.prevent_default(),
                    span { "↑↓ move" }
                    span { "↵ press" }
                    span { "esc close" }
                }
            }
        }
    }
}

/// Put focus back where it was when the palette opened (a field, the
/// GLSL editor) once it closes, so ⌘K then Esc costs the user nothing.
fn use_restore_focus(inline: bool) {
    let previous = use_hook(move || {
        if inline {
            return None;
        }
        use wasm_bindgen::JsCast as _;
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
            .map(std::rc::Rc::new)
    });
    use_drop(move || {
        if let Some(element) = previous {
            let _ = element.focus();
        }
    });
}

/// Scroll a row into view when the keys move onto it.
fn reveal_row(id: &str) {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
    else {
        return;
    };
    let options = web_sys::ScrollIntoViewOptions::new();
    options.set_block(web_sys::ScrollLogicalPosition::Nearest);
    element.scroll_into_view_with_scroll_into_view_options(&options);
}

fn row_id(list_id: &str, at: usize) -> String {
    format!("{list_id}-row-{at}")
}

/// A per-mount id, so two palettes on one page (the stories) cannot share
/// row ids.
fn next_list_id() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    format!(
        "ux-command-palette-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// One row: the button, then its path on the label's baseline. The
/// highlight is the menu's hover wash; an armed row wears the error tint
/// across its whole width, path included, not just behind the button.
fn row_class(highlighted: bool, armed: bool) -> &'static str {
    match (highlighted, armed) {
        (_, true) => {
            "tw:flex tw:min-w-0 tw:items-baseline tw:gap-3 tw:rounded tw:bg-status-error-bg tw:pr-2"
        }
        (true, false) => {
            "tw:flex tw:min-w-0 tw:items-baseline tw:gap-3 tw:rounded tw:bg-white/5 tw:pr-2"
        }
        (false, false) => "tw:flex tw:min-w-0 tw:items-baseline tw:gap-3 tw:rounded tw:pr-2",
    }
}

/// The viewport overlay: a dim backdrop, the panel high in the window (a
/// palette is read top-down, its field where the eye already is).
const OVERLAY_CLASS: &str =
    "tw:fixed tw:inset-0 tw:z-50 tw:flex tw:items-start tw:justify-center tw:px-4 tw:pt-[12vh]";

/// The stories' frame: the same panel, in flow.
const INLINE_FRAME_CLASS: &str = "tw:flex tw:justify-center tw:bg-black/50 tw:p-6";

const PANEL_CLASS: &str = "tw:relative tw:grid tw:w-full tw:max-w-xl tw:min-w-0 tw:overflow-hidden tw:rounded-lg tw:border tw:border-border-strong tw:bg-card-raised tw:shadow-2xl";

/// Preflight is not loaded, so the field resets its own UA chrome.
const INPUT_CLASS: &str = "tw:w-full tw:min-w-0 tw:appearance-none tw:border-0 tw:border-b tw:border-solid tw:border-border-subtle tw:bg-transparent tw:px-4 tw:py-3 tw:font-sans tw:text-base tw:text-strong-foreground tw:outline-none tw:placeholder:text-dim-foreground";

const LIST_CLASS: &str =
    "tw:grid tw:max-h-[min(60vh,420px)] tw:min-w-0 tw:gap-0.5 tw:overflow-y-auto tw:p-1.5";

const PATH_CLASS: &str =
    "tw:max-w-[45%] tw:flex-none tw:truncate tw:font-mono tw:text-[11px] tw:text-dim-foreground";

const EMPTY_CLASS: &str =
    "tw:m-0 tw:px-2.5 tw:py-6 tw:text-center tw:text-sm tw:text-muted-foreground";

const FOOTER_CLASS: &str = "tw:flex tw:gap-4 tw:border-0 tw:border-t tw:border-solid tw:border-border-subtle tw:px-4 tw:py-2 tw:text-xs tw:text-dim-foreground";
