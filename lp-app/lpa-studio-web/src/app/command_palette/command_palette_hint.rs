//! [`CommandPaletteHint`]: the site chrome's small "⌘K" (or "Ctrl+K")
//! chip — the palette's way of being found, and a way to open it without
//! the keyboard.

use dioxus::prelude::*;

use crate::base::Platform;
use crate::base::keyboard::PALETTE;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CommandPaletteHint(
    /// Open the palette.
    on_open: EventHandler<()>,
    /// Stories: draw for this platform instead of the browser's.
    #[props(default)]
    platform: Option<Platform>,
) -> Element {
    let platform = platform.unwrap_or_else(Platform::detect);
    let chord = PALETTE.display(platform);
    rsx! {
        button {
            class: HINT_CLASS,
            r#type: "button",
            title: "Find an action ({chord})",
            aria_label: "Open the command palette ({chord})",
            aria_keyshortcuts: "{aria_chord(platform)}",
            onclick: move |_| on_open.call(()),
            kbd { class: KBD_CLASS, "{chord}" }
        }
    }
}

/// The chord as `aria-keyshortcuts` spells it: `Meta+K` / `Control+K`.
fn aria_chord(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "Meta+K",
        Platform::Other => "Control+K",
    }
}

/// A quiet chip: no fill, a hairline border, the text brightening on
/// hover. Preflight is not loaded, so it resets the UA button chrome.
const HINT_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:rounded tw:border tw:border-solid tw:border-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:text-subtle-foreground tw:transition-colors tw:hover:border-border-strong tw:hover:text-strong-foreground ux-focus-ring";

const KBD_CLASS: &str =
    "tw:font-sans tw:text-[11px] tw:font-semibold tw:leading-4 tw:tracking-wide";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_names_the_chord_both_ways() {
        assert_eq!(PALETTE.display(Platform::Mac), "⌘K");
        assert_eq!(aria_chord(Platform::Mac), "Meta+K");
        assert_eq!(PALETTE.display(Platform::Other), "Ctrl+K");
        assert_eq!(aria_chord(Platform::Other), "Control+K");
    }
}
