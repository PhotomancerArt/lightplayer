//! [`CommandPalette`]: the palette's mount at the web root — the ⌘K
//! listener, and the dialog while it is open.
//!
//! It mounts at the root, beside the routes, not inside `StudioShell`: the
//! shell's offers context only exists on the shell's pages, and ⌘K must
//! open the palette on Home too. So the root hands it the view's tree
//! directly. Whether it is open is web chrome, like a popover's open state:
//! the root owns that signal so the site chrome's hint button can open it.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiOfferTree};

use super::command_palette_dialog::CommandPaletteDialog;
use crate::app::editor_shell::hotkeys::use_window_shortcut;
use crate::base::Platform;
use crate::base::keyboard::PALETTE;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CommandPalette(
    /// The view's offer tree (`UiStudioView::offers`).
    offers: UiOfferTree,
    /// Whether the palette is open; ⌘K toggles it, Esc / a click outside /
    /// a press closes it.
    open: Signal<bool>,
    /// The root's action dispatch, the one every button uses.
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut open = open;
    // ⌘K (Ctrl+K off Mac) from anywhere, a text field or the GLSL editor
    // included: the listener runs in the capture phase with no editable
    // guard, and takes the chord from the browser (Chrome's own Ctrl+K
    // focuses its search box).
    use_window_shortcut(move |event: web_sys::KeyboardEvent| {
        if PALETTE.matches(
            Platform::detect(),
            &event.key(),
            event.meta_key(),
            event.ctrl_key(),
            event.alt_key(),
            event.shift_key(),
        ) {
            event.prevent_default();
            let was_open = *open.peek();
            open.set(!was_open);
        }
    });

    rsx! {
        if open() {
            CommandPaletteDialog {
                offers,
                on_action,
                on_close: move |_| open.set(false),
            }
        }
    }
}
