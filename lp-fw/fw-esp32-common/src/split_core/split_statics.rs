//! The split core's statics that hold chip types: one per image, declared by
//! the chip (`static SPLIT: SplitStatics<Chip> = SplitStatics::new();`) and
//! reached through [`super::SplitChip::statics`], since Rust has no generic
//! statics.

use super::board_identity::SlicingCell;
use super::running_hook::RunningCell;
use super::split_chip::SplitChip;

pub struct SplitStatics<C: SplitChip> {
    /// The running engine's update session ([`super::running_hook`]).
    pub(super) running: RunningCell<C>,
    /// The core's hash in progress ([`super::board_identity`]).
    pub(super) slicing: SlicingCell<C>,
}

impl<C: SplitChip> SplitStatics<C> {
    pub const fn new() -> Self {
        Self {
            running: RunningCell::new(),
            slicing: SlicingCell::new(),
        }
    }
}

impl<C: SplitChip> Default for SplitStatics<C> {
    fn default() -> Self {
        Self::new()
    }
}
