//! The runtime band: the one mark a card wears that says it is not silicon.
//!
//! D38/D49, PD11. A real board's card has NO band — that is the point of
//! the whole "always a device" model: the card, the verbs and the fold are
//! the same for every runtime, and what differs is one 24 px row under the
//! header saying what is actually behind this device.
//!
//! ```text
//! ▶ Sim · Desktop · in this tab · GPU
//! ▶ Sim · XIAO ESP32-C6 · in this tab · CPU
//! ```
//!
//! Every clause is a fact somebody reported, and an absent one is dropped
//! rather than guessed:
//!
//! - **Sim** — the record's own kind (its `/device-sims/<uid>.json`
//!   sidecar). `Emu ·` joins it when an emulator backing exists.
//! - **the target** — what this sim ACTS AS, from the sidecar's `target`,
//!   resolved through the board catalog for its display name.
//! - **in this tab** — where the runtime lives. A sim is a worker in this
//!   browser tab, and closing the tab takes it with you; that is the one
//!   thing about a sim a person has to know without opening anything.
//! - **the tier** — the shader tier the worker GRANTED, never the one that
//!   was asked for (PD12, the fidelity-tiers ADR). Absent until a boot
//!   answered, which is honest: nothing has been granted yet.
//!
//! Joined at the app view like the card's feed, not folded into
//! `lpa-devices`: a band is a fact about the RUNTIME behind a device, and
//! the model deliberately does not know that a sim is a sim.

/// One card's runtime band, when the device has one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiRuntimeBand {
    /// The kind word: `Sim` today, `Emu` when a backing exists (D49's K1
    /// short forms).
    pub kind: &'static str,
    /// The target's display name — the board this runtime acts as, or
    /// `Desktop`.
    pub target: String,
    /// Where the runtime lives (`in this tab`).
    pub locality: &'static str,
    /// The granted shader tier (`GPU` / `CPU`), when a boot has answered.
    pub tier: Option<&'static str>,
}

impl UiRuntimeBand {
    /// A sim's band: the target it wears and the tier it was granted.
    pub fn sim(target: &str, granted_tier: Option<&str>) -> Self {
        Self {
            kind: "Sim",
            target: crate::app::roster::board_display_name(target),
            locality: "in this tab",
            tier: match granted_tier {
                Some("gpu") => Some("GPU"),
                Some("cpu") => Some("CPU"),
                _ => None,
            },
        }
    }

    /// The band as one line, for the renderer and for the text fallback.
    pub fn line(&self) -> String {
        let mut parts = vec![self.kind.to_string(), self.target.clone()];
        parts.push(self.locality.to_string());
        if let Some(tier) = self.tier {
            parts.push(tier.to_string());
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_names_the_target_the_place_and_the_granted_tier() {
        assert_eq!(
            UiRuntimeBand::sim("lightplayer/desktop", Some("gpu")).line(),
            "Sim · Desktop · in this tab · GPU"
        );
        assert_eq!(
            UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("cpu")).line(),
            "Sim · XIAO ESP32-C6 · in this tab · CPU"
        );
    }

    #[test]
    fn a_tier_nothing_granted_yet_is_dropped_not_guessed() {
        let band = UiRuntimeBand::sim("lightplayer/desktop", None);
        assert_eq!(band.tier, None);
        assert_eq!(band.line(), "Sim · Desktop · in this tab");
    }

    #[test]
    fn an_unknown_board_id_still_names_something() {
        // The catalog may not carry a board a record was made for; the
        // raw id is better than a blank clause.
        assert_eq!(
            UiRuntimeBand::sim("acme/future-board", None).line(),
            "Sim · acme/future-board · in this tab"
        );
    }
}
