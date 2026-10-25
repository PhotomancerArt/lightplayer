//! Whether this page can open a WebSocket to a board on the person's own
//! network, as the browser decides it: a platform fact the web reports once
//! ([`crate::StudioController::set_lan_reach`]), the way it reports what
//! Bluetooth answered. Core never sniffs the browser.
//!
//! | page | browser | [`LanReach`] |
//! |---|---|---|
//! | `http:` | any | [`LanReach::Direct`] |
//! | `https:` | Chromium | [`LanReach::AfterPrompt`] (its Local Network check asks first) |
//! | `https:` | any other | [`LanReach::Blocked`] (mixed content: a `ws://` socket never opens) |
//!
//! Only the choices Studio makes by itself obey [`LanReach::Blocked`]: which
//! road Connect takes first (`best_link.rs`) and whether a session through
//! lightplayer.app moves onto Wi‑Fi (`lan_move.rs`). "Connect over Wi‑Fi"
//! pressed by hand still tries, and says plainly why it could not.

/// What this page's browser lets a socket to the LAN do.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LanReach {
    /// A socket to a board on the LAN opens with no question (an `http:`
    /// page). The default, so a page that never reports (the host tests, a
    /// dev page) behaves as before.
    #[default]
    Direct,
    /// The browser asks the person first (Chrome's Local Network check on
    /// an `https:` page): worth trying, and the person may say no.
    AfterPrompt,
    /// The browser never opens one (an `https:` page outside Chromium):
    /// nothing automatic tries.
    Blocked,
}

impl LanReach {
    /// The reach of a page served over `https` (or not), in a Chromium
    /// browser (or not). The web edge reads the two facts and decides
    /// nothing.
    pub fn of_page(https: bool, chromium: bool) -> Self {
        match (https, chromium) {
            (false, _) => Self::Direct,
            (true, true) => Self::AfterPrompt,
            (true, false) => Self::Blocked,
        }
    }

    /// Whether Studio may try the LAN by itself.
    pub fn tries_by_itself(self) -> bool {
        self != Self::Blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_http_page_reaches_the_lan_in_any_browser() {
        assert_eq!(LanReach::of_page(false, true), LanReach::Direct);
        assert_eq!(LanReach::of_page(false, false), LanReach::Direct);
    }

    #[test]
    fn an_https_page_reaches_it_after_the_prompt_in_chromium_and_never_elsewhere() {
        assert_eq!(LanReach::of_page(true, true), LanReach::AfterPrompt);
        assert_eq!(LanReach::of_page(true, false), LanReach::Blocked);
        assert!(LanReach::AfterPrompt.tries_by_itself());
        assert!(!LanReach::Blocked.tries_by_itself());
    }

    #[test]
    fn a_page_that_never_reports_is_direct() {
        assert_eq!(LanReach::default(), LanReach::Direct);
    }
}
