//! The playlist's switch sequence: hold the frame, unload, load, fade.
//!
//! Only one entry is ever loaded (multi-pattern vision D8), so a switch from
//! `old` to `new` runs in four steps (plan P4):
//!
//! 1. **Capture.** The frame the switch is decided on still renders `old`
//!    (or the fade that was running) exactly as the frame before, and the
//!    playlist keeps a copy of that output: the held frame
//!    ([`super::playlist_held_frame`]).
//! 2. **Request.** The playlist asks for `{ unload: old, load: new }`
//!    ([`crate::node::ResidencyRequest`]); the engine applies it at the
//!    top of the next tick, unload first.
//! 3. **Hold.** The playlist shows the held frame until `new` renders for
//!    real. A freshly loaded shader renders black for a frame while its
//!    first compile waits for the compile window, so the playlist asks the
//!    producer ([`crate::node::RenderNode::visual_readiness`]) instead of
//!    counting frames.
//! 4. **Fade.** From the first real frame, the output fades from the held
//!    frame to `new` over the LEAVING entry's `fade_after` (or the
//!    playlist's `default_fade`). When the fade ends the held frame is
//!    freed.
//!
//! A load or compile failure of `new` marks it failed and moves to the next
//! candidate, still holding (plan PD9). The one dark case is boot: when the
//! idle entry itself fails before anything rendered there is no frame to
//! hold, so the output stays cleared until a working entry renders.
//!
//! This file is the state; `playlist_node.rs` drives it.

use lpc_model::VisualProduct;

/// A switch in flight: holding, then fading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PlaylistSwitch {
    /// Fade length in playlist seconds (the leaving entry's `fade_after`, or
    /// `default_fade`). `0` cuts on the incoming entry's first real frame.
    pub fade: f32,
    /// Playlist time of the incoming entry's first real frame; `None` while
    /// the playlist is still holding.
    pub fade_start: Option<f32>,
}

impl PlaylistSwitch {
    /// A switch that has just been decided: holding.
    pub(super) fn holding(fade: f32) -> Self {
        Self {
            fade,
            fade_start: None,
        }
    }

    /// The fade's progress at `time`: `None` while holding, then `0 → 1`.
    /// At `>= 1` the switch is over.
    pub(super) fn alpha(&self, time: f32) -> Option<f32> {
        let start = self.fade_start?;
        if self.fade <= 0.0 {
            return Some(1.0);
        }
        Some(clamp01((time - start) / self.fade))
    }
}

/// What this frame's render shows, planned by the playlist's `produce` so
/// every render call of the frame (sample stream, texture, readiness)
/// agrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PlaylistFramePlan {
    /// Nothing rendered yet and nothing held: cleared output (boot, or
    /// every entry failed before anything rendered).
    Clear,
    /// One entry, live.
    Live {
        product: VisualProduct,
        /// The switch's first frame: keep a copy of what is shown.
        capture: bool,
        /// This is the current entry and it has not rendered for real since
        /// it loaded: ask whether this frame did.
        probe: bool,
    },
    /// The held frame.
    Held {
        /// The incoming entry, sampled for its readiness (its answer is not
        /// shown). `None` when it is not loaded yet.
        probe: Option<VisualProduct>,
    },
    /// The held frame faded toward the incoming entry.
    Blend {
        product: VisualProduct,
        alpha: f32,
        /// A new switch was decided mid-fade: the blend shown this frame
        /// becomes the held frame.
        capture: bool,
    },
}

impl PlaylistFramePlan {
    /// The live product this frame renders or probes, if any.
    pub(super) fn product(&self) -> Option<VisualProduct> {
        match *self {
            Self::Clear => None,
            Self::Live { product, .. } | Self::Blend { product, .. } => Some(product),
            Self::Held { probe } => probe,
        }
    }
}

pub(super) fn clamp01(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value >= 1.0 {
        1.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_holding_switch_has_no_alpha() {
        assert_eq!(PlaylistSwitch::holding(0.5).alpha(10.0), None);
    }

    #[test]
    fn the_fade_runs_from_the_first_real_frame() {
        let mut switch = PlaylistSwitch::holding(0.5);
        switch.fade_start = Some(2.0);

        assert_eq!(switch.alpha(2.0), Some(0.0));
        assert_eq!(switch.alpha(2.25), Some(0.5));
        assert_eq!(switch.alpha(3.0), Some(1.0));
    }

    #[test]
    fn a_zero_fade_cuts() {
        let mut switch = PlaylistSwitch::holding(0.0);
        switch.fade_start = Some(2.0);
        assert_eq!(switch.alpha(2.0), Some(1.0));
    }
}
