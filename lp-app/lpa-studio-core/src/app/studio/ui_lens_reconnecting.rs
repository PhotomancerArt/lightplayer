//! The "Reconnecting…" curtain the project page shows while the editor's
//! board is riding out a link stall or reset (plan D13; see
//! [`lens_reconnect`](super::lens_reconnect)), or is away altogether and
//! expected back (see [`lens_hold`](super::lens_hold)). The words are
//! assembled here so the web shell only lays them out.

use crate::app::devices::LinkTrouble;

/// What the curtain says.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLensReconnecting {
    /// "Reconnecting…"
    pub headline: String,
    /// One plain sentence: what happened to which board, and that it is
    /// being retried.
    pub detail: String,
}

impl UiLensReconnecting {
    /// The curtain for the board called `device_name`, in `trouble`.
    pub fn new(device_name: &str, trouble: LinkTrouble) -> Self {
        let detail = match trouble {
            LinkTrouble::Quiet => {
                format!("{device_name} stopped responding. Attempting to reconnect.")
            }
            LinkTrouble::Restarted => {
                format!("The connection to {device_name} was reset. Attempting to reconnect.")
            }
        };
        Self::with_detail(detail)
    }

    /// The curtain for the board called `device_name`, whose link went away
    /// (a Bluetooth drop, a cable re-seated) and is expected back.
    pub fn link_lost(device_name: &str) -> Self {
        Self::with_detail(format!(
            "Lost the connection to {device_name}. Attempting to reconnect."
        ))
    }

    /// The curtain while the board is being updated: its link drops and
    /// comes back as the board resets into its new firmware, and that is
    /// the update, not a lost connection — so the detail is the update's
    /// own line ("Updating over Bluetooth… 40%").
    pub fn updating(update_line: &str) -> Self {
        Self::with_detail(update_line.to_string())
    }

    fn with_detail(detail: String) -> Self {
        Self {
            headline: "Reconnecting…".to_string(),
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curtain_says_reconnecting_and_names_the_board() {
        for strip in [
            UiLensReconnecting::new("Porch sign", LinkTrouble::Quiet),
            UiLensReconnecting::new("Porch sign", LinkTrouble::Restarted),
            UiLensReconnecting::link_lost("Porch sign"),
        ] {
            assert_eq!(strip.headline, "Reconnecting…");
            assert!(strip.detail.contains("Porch sign"), "{}", strip.detail);
            assert!(
                strip.detail.ends_with("Attempting to reconnect."),
                "{}",
                strip.detail
            );
        }
    }

    #[test]
    fn while_the_board_updates_the_curtain_names_the_update() {
        let strip = UiLensReconnecting::updating("Updating over Bluetooth… 40%");
        assert_eq!(strip.headline, "Reconnecting…");
        assert_eq!(strip.detail, "Updating over Bluetooth… 40%");
    }
}
