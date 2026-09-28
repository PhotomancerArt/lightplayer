//! The "Reconnecting…" strip the project page shows while the editor's
//! board is riding out a link stall or reset (plan D13; see
//! [`lens_reconnect`](super::lens_reconnect)). The words are assembled here
//! so the web shell only lays them out.

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
    }
}
