//! Editor-scoped keyboard shortcut model with OS-aware display.
//!
//! Deliberately small (GLSL editor UX plan, D4/QD-B): a [`Shortcut`] is the
//! platform's primary modifier (⌘ on Mac, Ctrl elsewhere) plus a key, and
//! [`Shortcut::display`] renders it in that platform's convention (`⌘↵` vs
//! `Ctrl+Enter`). There is no global command registry or palette, and no
//! event matching here — capture happens inside the CodeMirror keymap
//! (`Mod-` bindings in `vendor-src/codemirror/entry.mjs`), so this module
//! only needs platform detection and display.

use std::sync::OnceLock;

/// Apply the editor's current text to the running project (`Mod-Enter`).
pub const APPLY: Shortcut = Shortcut {
    alt: false,
    shift: false,
    key: Key::Enter,
};

/// Save the project overlay to disk (`Mod-s`, editor-focused).
pub const SAVE: Shortcut = Shortcut {
    alt: false,
    shift: false,
    key: Key::Char('s'),
};

/// Platform family, as far as shortcut display cares: Mac renders modifier
/// symbols (`⌘↵`), everything else spells them out (`Ctrl+Enter`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Mac,
    Other,
}

impl Platform {
    /// The runtime platform, read once from the browser's `navigator` and
    /// cached for the page's lifetime.
    pub fn detect() -> Self {
        static DETECTED: OnceLock<Platform> = OnceLock::new();
        *DETECTED.get_or_init(|| {
            let navigator = web_sys::window().map(|window| window.navigator());
            let platform = navigator
                .as_ref()
                .and_then(|navigator| navigator.platform().ok())
                .unwrap_or_default();
            let user_agent = navigator
                .as_ref()
                .and_then(|navigator| navigator.user_agent().ok())
                .unwrap_or_default();
            Self::from_navigator(&platform, &user_agent)
        })
    }

    /// Classify the `navigator.platform` / `navigator.userAgent` strings.
    /// Pure so it is testable off-browser. iOS/iPadOS count as Mac — external
    /// keyboards there use ⌘.
    fn from_navigator(platform: &str, user_agent: &str) -> Self {
        let apple = ["Mac", "iPhone", "iPad", "iPod"];
        if apple.iter().any(|marker| platform.contains(marker)) || user_agent.contains("Mac OS X") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}

/// The non-modifier key of a [`Shortcut`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Enter,
    Char(char),
}

/// One editor shortcut: the platform's primary modifier (⌘ on Mac, Ctrl
/// elsewhere) plus optional Alt/Shift and a key. Display-only today; the
/// actual key capture lives in the CodeMirror keymap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Shortcut {
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

impl Shortcut {
    /// Render for `platform`, in that platform's modifier order:
    /// symbol-joined with the primary modifier last on Mac (`⌥⇧⌘S`, Apple's
    /// convention), `+`-joined names with the primary first elsewhere
    /// (`Ctrl+Alt+Shift+S`).
    pub fn display(&self, platform: Platform) -> String {
        let mut pieces = Vec::with_capacity(4);
        match platform {
            Platform::Mac => {
                if self.alt {
                    pieces.push(Piece::Alt);
                }
                if self.shift {
                    pieces.push(Piece::Shift);
                }
                pieces.push(Piece::Primary);
            }
            Platform::Other => {
                pieces.push(Piece::Primary);
                if self.alt {
                    pieces.push(Piece::Alt);
                }
                if self.shift {
                    pieces.push(Piece::Shift);
                }
            }
        }
        let mut out = String::new();
        for piece in pieces {
            out.push_str(symbol(platform, piece));
            out.push_str(joiner(platform));
        }
        match self.key {
            Key::Enter => out.push_str(symbol(platform, Piece::Enter)),
            Key::Char(ch) => out.extend(ch.to_uppercase()),
        }
        out
    }
}

/// A displayable fragment of a shortcut, keyed into the one symbol table.
#[derive(Clone, Copy)]
enum Piece {
    Primary,
    Alt,
    Shift,
    Enter,
}

/// The symbol table: every platform-specific glyph/name in one place.
fn symbol(platform: Platform, piece: Piece) -> &'static str {
    match (platform, piece) {
        (Platform::Mac, Piece::Primary) => "⌘",
        (Platform::Mac, Piece::Alt) => "⌥",
        (Platform::Mac, Piece::Shift) => "⇧",
        (Platform::Mac, Piece::Enter) => "↵",
        (Platform::Other, Piece::Primary) => "Ctrl",
        (Platform::Other, Piece::Alt) => "Alt",
        (Platform::Other, Piece::Shift) => "Shift",
        (Platform::Other, Piece::Enter) => "Enter",
    }
}

fn joiner(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "",
        Platform::Other => "+",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_and_save_display_per_platform() {
        assert_eq!(APPLY.display(Platform::Mac), "⌘↵");
        assert_eq!(APPLY.display(Platform::Other), "Ctrl+Enter");
        assert_eq!(SAVE.display(Platform::Mac), "⌘S");
        assert_eq!(SAVE.display(Platform::Other), "Ctrl+S");
    }

    #[test]
    fn modifier_order_follows_each_platforms_convention() {
        let combo = Shortcut {
            alt: true,
            shift: true,
            key: Key::Char('k'),
        };
        // Mac: primary (⌘) last, per Apple's ⌥⇧⌘ order.
        assert_eq!(combo.display(Platform::Mac), "⌥⇧⌘K");
        // Elsewhere: Ctrl leads.
        assert_eq!(combo.display(Platform::Other), "Ctrl+Alt+Shift+K");
    }

    #[test]
    fn navigator_strings_classify_apple_platforms_as_mac() {
        assert_eq!(Platform::from_navigator("MacIntel", ""), Platform::Mac);
        assert_eq!(Platform::from_navigator("iPhone", ""), Platform::Mac);
        assert_eq!(
            Platform::from_navigator("", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"),
            Platform::Mac
        );
        assert_eq!(
            Platform::from_navigator("Win32", "Mozilla/5.0 (Windows NT 10.0)"),
            Platform::Other
        );
        assert_eq!(Platform::from_navigator("", ""), Platform::Other);
    }
}
