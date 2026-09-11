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
//! ▶ Emu · XIAO ESP32-C6 · in this tab · 0.5×
//! ```
//!
//! Every clause is a fact somebody reported, and an absent one is dropped
//! rather than guessed:
//!
//! - **Sim / Emu** — the record's own kind (its `/device-sims/<uid>.json`
//!   sidecar, D49's K1 short forms).
//! - **the target** — what this runtime ACTS AS, from the sidecar's
//!   `target`, resolved through the board catalog for its display name.
//! - **in this tab** — where the runtime lives. Both kinds are a worker in
//!   this browser tab, and closing the tab takes it with you; that is the
//!   one thing about a runtime a person has to know without opening
//!   anything.
//! - **the last clause** is the one thing the two kinds do not share:
//!   - a **sim** reports the shader tier the worker GRANTED, never the one
//!     that was asked for (PD12, the fidelity-tiers ADR);
//!   - an **emu** reports its SPEED against wall time (D8/D25) — it grants
//!     no tier, and the honest thing to put where the tier went is the
//!     number that actually varies.
//!
//!   Both are absent until something answered, which is honest: nothing has
//!   been granted and nothing has been measured yet.
//!
//! Joined at the app view like the card's feed, not folded into
//! `lpa-devices`: a band is a fact about the RUNTIME behind a device, and
//! the model deliberately does not know that a sim is a sim.
//!
//! # How the speed is written (G1's Q4)
//!
//! [`speed_word`] owns the rule, alone, so G2 can change one function:
//!
//! | dilation | reads |
//! |---|---|
//! | `None` (nothing measured yet) | the clause is dropped |
//! | at or above `0.1` | one decimal — `0.5×`, `1.0×` |
//! | below `0.1` | two decimals — `0.04×` |
//!
//! Two decimals below `0.1` because the measured range spans two orders of
//! magnitude — a hidden tab reads about `0.009` (G1) — and one decimal
//! would print `0.0×` there, which is both wrong and alarming. `×` is a
//! measurement of something the person can see (the board is slow), not a
//! judgement about it; the word that would alarm is "degraded".
//!
//! **Interim, reversible at G2**: G1's Q4 went to the gate unanswered and
//! the director ruled this as the working answer.

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
    /// A SIM's granted shader tier (`GPU` / `CPU`), when a boot has
    /// answered. Always `None` on an emu — it grants no tier.
    pub tier: Option<&'static str>,
    /// An EMU's speed against wall time (`0.5×`), when the page has
    /// measured one. Always `None` on a sim, and `None` on an emu whose
    /// first measuring window has not closed — an absent number is dropped,
    /// never printed as zero. [`speed_word`] is the format rule.
    pub speed: Option<String>,
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
            speed: None,
        }
    }

    /// An emu's band: the target it wears and how fast it is running
    /// (D25). `dilation` is the page's own measurement — guest seconds per
    /// wall second — and `None` before the first window closes.
    pub fn emu(target: &str, dilation: Option<f64>) -> Self {
        Self {
            kind: "Emu",
            target: crate::app::roster::board_display_name(target),
            locality: "in this tab",
            tier: None,
            speed: dilation.and_then(speed_word),
        }
    }

    /// The band as one line, for the renderer and for the text fallback.
    pub fn line(&self) -> String {
        let mut parts = vec![self.kind.to_string(), self.target.clone()];
        parts.push(self.locality.to_string());
        if let Some(tier) = self.tier {
            parts.push(tier.to_string());
        }
        if let Some(speed) = &self.speed {
            parts.push(speed.clone());
        }
        parts.join(" · ")
    }
}

/// How a dilation is written in the band (D25, G1's Q4) — the ONE place
/// the rule lives, so changing it is one function.
///
/// One decimal at or above `0.1`, two below it, so a tab nobody is looking
/// at reads `0.04×` rather than `0.0×`. A number that is not a number —
/// NaN, an infinity, a negative — is refused rather than printed: the page
/// reporting one would be a defect, and a band is not the place to find
/// out.
pub fn speed_word(dilation: f64) -> Option<String> {
    if !dilation.is_finite() || dilation < 0.0 {
        return None;
    }
    Some(match dilation >= 0.1 {
        true => format!("{dilation:.1}×"),
        false => format!("{dilation:.2}×"),
    })
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

    /// D25: the emu's band, with the word the gate is asked about.
    #[test]
    fn the_emu_band_names_the_target_the_place_and_the_speed() {
        assert_eq!(
            UiRuntimeBand::emu("seeed/xiao-esp32-c6", Some(0.5)).line(),
            "Emu · XIAO ESP32-C6 · in this tab · 0.5×"
        );
        let band = UiRuntimeBand::emu("seeed/xiao-esp32-c6", Some(0.45));
        assert_eq!(band.tier, None, "an emu grants no shader tier");
        assert_eq!(band.speed.as_deref(), Some("0.5×"));
    }

    /// Nothing measured yet is DROPPED, not printed as zero: an emu whose
    /// first window has not closed is not a board running at no speed.
    #[test]
    fn a_speed_nothing_measured_yet_is_dropped_not_guessed() {
        let band = UiRuntimeBand::emu("seeed/xiao-esp32-c6", None);
        assert_eq!(band.speed, None);
        assert_eq!(band.line(), "Emu · XIAO ESP32-C6 · in this tab");
    }

    /// The format rule (G1's Q4, the director's interim ruling): one
    /// decimal at or above 0.1, two below it — a hidden tab reads `0.04×`,
    /// never `0.0×`.
    #[test]
    fn the_speed_word_keeps_a_quiet_tabs_number_legible() {
        assert_eq!(speed_word(1.0).as_deref(), Some("1.0×"));
        assert_eq!(speed_word(0.45).as_deref(), Some("0.5×"));
        assert_eq!(speed_word(0.1).as_deref(), Some("0.1×"));
        assert_eq!(speed_word(0.099).as_deref(), Some("0.10×"));
        assert_eq!(speed_word(0.042).as_deref(), Some("0.04×"));
        assert_eq!(speed_word(0.009).as_deref(), Some("0.01×"));
        assert_eq!(speed_word(0.0).as_deref(), Some("0.00×"));
        for word in [0.009, 0.042].map(speed_word) {
            assert_ne!(word.as_deref(), Some("0.0×"), "never alarming and wrong");
        }
    }

    /// A page that reported a non-number is a defect, and the band is not
    /// where it should surface.
    #[test]
    fn a_speed_that_is_not_a_number_is_refused() {
        assert_eq!(speed_word(f64::NAN), None);
        assert_eq!(speed_word(f64::INFINITY), None);
        assert_eq!(speed_word(-1.0), None);
        assert_eq!(
            UiRuntimeBand::emu("lightplayer/desktop", Some(-1.0)).speed,
            None
        );
    }

    /// A sim never carries a speed and an emu never carries a tier: the
    /// last clause is the one place the two kinds differ, and neither can
    /// wear the other's.
    #[test]
    fn the_two_kinds_never_wear_each_others_last_clause() {
        let sim = UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("cpu"));
        let emu = UiRuntimeBand::emu("seeed/xiao-esp32-c6", Some(0.5));

        assert_eq!(sim.speed, None);
        assert_eq!(emu.tier, None);
        assert_eq!(sim.kind, "Sim");
        assert_eq!(emu.kind, "Emu");
        assert_eq!(sim.locality, emu.locality, "both live in this tab");
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
