//! The "Reconnecting…" strip the project page shows while the editor's
//! board is riding out a link stall or reset (plan D13; see
//! [`lens_reconnect`](super::lens_reconnect)), or is away altogether and
//! expected back (see [`lens_hold`](super::lens_hold)). The words are
//! assembled here so the web shell only lays them out.

use crate::app::devices::LinkTrouble;

/// What the strip says.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLensReconnecting {
    /// "Reconnecting to Porch sign…"
    pub headline: String,
    /// One plain sentence: what happened, and that the editor stays.
    pub detail: String,
}

impl UiLensReconnecting {
    /// The strip for the board called `device_name`, in `trouble`.
    pub fn new(device_name: &str, trouble: LinkTrouble) -> Self {
        let detail = match trouble {
            LinkTrouble::Quiet => {
                "The board went quiet. The editor stays open and catches up when it answers."
            }
            LinkTrouble::Restarted => {
                "The connection to the board restarted. The editor stays open and catches up when the board says hello."
            }
        };
        Self {
            headline: format!("Reconnecting to {device_name}…"),
            detail: detail.to_string(),
        }
    }

    /// The strip for the board called `device_name`, whose link went away
    /// (a Bluetooth drop, a cable re-seated) and is expected back.
    pub fn link_lost(device_name: &str) -> Self {
        Self {
            headline: format!("Reconnecting to {device_name}…"),
            detail: "The connection dropped. You stay right here, and pick up where you left off when the board is back.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_names_the_board_and_says_the_editor_stays() {
        let strip = UiLensReconnecting::new("Porch sign", LinkTrouble::Quiet);
        assert_eq!(strip.headline, "Reconnecting to Porch sign…");
        assert!(strip.detail.contains("stays open"), "{}", strip.detail);
        let strip = UiLensReconnecting::new("Porch sign", LinkTrouble::Restarted);
        assert!(strip.detail.contains("says hello"), "{}", strip.detail);
        let strip = UiLensReconnecting::link_lost("Porch sign");
        assert_eq!(strip.headline, "Reconnecting to Porch sign…");
        assert!(strip.detail.contains("dropped"), "{}", strip.detail);
    }
}
