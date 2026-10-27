//! How plans are used this frame: replayed, checked, or rebuilt.

/// How [`super::PlanSlot`]s treat a frame whose key has not moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanMode {
    /// Replay the stored plan. The product's mode (release builds).
    Replay,
    /// Replay, and on every `every`th frame also rebuild the plan from
    /// scratch and compare it with the stored one. A difference is a missed
    /// invalidation: it is counted and logged once per slot, and panics when
    /// `strict`. Only exists in debug builds' behaviour: release firmware
    /// treats it as [`Self::Replay`] and links none of the comparison.
    Check {
        /// Compare on frames whose index is a multiple of this (1 = every
        /// frame).
        every: u32,
        /// Panic on a mismatch instead of logging it.
        strict: bool,
    },
    /// Never replay: build every frame, as if every key moved. The
    /// differential's reference mode (`tests/frame_plan_differential.rs`).
    RebuildEveryFrame,
}

impl PlanMode {
    /// The mode an engine starts in: checked every frame and strict in this
    /// crate's own tests, checked every 32nd frame elsewhere in debug builds
    /// (Studio dev, emulator debug images), replayed in release.
    pub const fn default_for_build() -> Self {
        if cfg!(test) {
            Self::Check {
                every: 1,
                strict: true,
            }
        } else if cfg!(debug_assertions) {
            Self::Check {
                every: 32,
                strict: false,
            }
        } else {
            Self::Replay
        }
    }

    /// Checked every frame, panicking on a mismatch: what a test that wants
    /// the checker's verdict sets.
    pub const fn strict_check() -> Self {
        Self::Check {
            every: 1,
            strict: true,
        }
    }
}

impl Default for PlanMode {
    fn default() -> Self {
        Self::default_for_build()
    }
}
