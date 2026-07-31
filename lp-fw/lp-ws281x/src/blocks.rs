//! How the RMT's memory blocks are shared out between the channels of one
//! direction group — the `blocks_per_channel` tunable.
//!
//! The RMT peripheral gives every channel exactly one block of RAM (48 words on
//! the ESP32-S3 and C6, 64 on the classic ESP32) and lets a channel *extend*
//! into the blocks of the channels above it by raising its `mem_size` field.
//! The extension is not a gift: block `k` has exactly one owner, so a channel
//! that absorbs its neighbour's block also takes the neighbour's ability to
//! transmit. On an ESP32-S3 (four TX channels, four TX blocks) the whole
//! configuration space is therefore
//!
//! ```text
//!   blocks:  [1,1,1,1]   4 channels x 24-word halves   threshold every 24 bits
//!            [2,0,2,0]   2 channels x 48-word halves   threshold every 48 bits
//!            [2,0,1,1]   3 channels, mixed
//!            [4,0,0,0]   1 channel  x 96-word halves
//!            [2,1,1,1]   INVALID — channel 1's block belongs to channel 0
//! ```
//!
//! [`BlockPlan`] is that table made explicit and checkable. It is a plain
//! `Copy` value so the same one can be handed to the driver (which uses it to
//! refuse [`configure`](crate::Ws281xDriver::configure) on a channel whose
//! block was absorbed) and to the chip backend (which uses it to size and bound
//! its RAM window), leaving one source of truth.
//!
//! # Choosing
//!
//! More blocks per channel is strictly easier on the interrupt handler and
//! strictly fewer outputs; see the interrupt-rate table in the crate README.
//! The driver itself is indifferent — the bit cursor makes any half size work.

/// Why a set of per-channel block counts is not a legal allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockPlanError {
    /// Channel `ch` asked for a block that a lower-numbered channel had already
    /// extended into. Give the absorbed channel `0` blocks instead.
    Overlap {
        /// The channel whose own block is not its own.
        ch: u8,
    },
    /// Channel `ch`'s window runs past the last block of the group.
    OutOfBlocks {
        /// The channel that asked for too much.
        ch: u8,
    },
    /// No channel was given any blocks at all.
    Empty,
}

/// A validated allocation of the group's `N` memory blocks to its `N` channels.
///
/// Channel `ch` owns the `blocks(ch)` blocks starting at block `ch`; a channel
/// with zero blocks is **unavailable** — either absorbed by a lower channel or
/// deliberately left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockPlan<const N: usize> {
    blocks: [u8; N],
}

impl<const N: usize> BlockPlan<N> {
    /// Validate a per-channel block count.
    ///
    /// ```
    /// use lp_ws281x::{BlockPlan, BlockPlanError};
    ///
    /// assert!(BlockPlan::<4>::new([1, 1, 1, 1]).is_ok());
    /// assert!(BlockPlan::<4>::new([2, 0, 1, 1]).is_ok());
    /// assert_eq!(
    ///     BlockPlan::<4>::new([2, 1, 1, 1]),
    ///     Err(BlockPlanError::Overlap { ch: 1 }),
    /// );
    /// assert_eq!(
    ///     BlockPlan::<4>::new([1, 1, 1, 2]),
    ///     Err(BlockPlanError::OutOfBlocks { ch: 3 }),
    /// );
    /// ```
    pub const fn new(blocks: [u8; N]) -> Result<Self, BlockPlanError> {
        let mut claimed = [false; N];
        let mut any = false;
        let mut ch = 0;
        while ch < N {
            let want = blocks[ch] as usize;
            if want == 0 {
                ch += 1;
                continue;
            }
            if claimed[ch] {
                return Err(BlockPlanError::Overlap { ch: ch as u8 });
            }
            if want > N - ch {
                return Err(BlockPlanError::OutOfBlocks { ch: ch as u8 });
            }
            let mut b = ch;
            while b < ch + want {
                claimed[b] = true;
                b += 1;
            }
            any = true;
            ch += 1;
        }
        if !any {
            return Err(BlockPlanError::Empty);
        }
        Ok(Self { blocks })
    }

    /// [`Self::new`], panicking on an invalid plan.
    ///
    /// Intended for `const`/`static` initialisers, where the panic is a
    /// compile-time error rather than a runtime one.
    pub const fn checked(blocks: [u8; N]) -> Self {
        match Self::new(blocks) {
            Ok(plan) => plan,
            Err(_) => panic!(
                "invalid RMT block plan: a channel extends into a block that another channel \
                 still claims, or past the end of the group"
            ),
        }
    }

    /// One block each — every channel available, the maximum output count and
    /// the tightest refill deadline. Always valid.
    pub const fn one_per_channel() -> Self {
        Self { blocks: [1; N] }
    }

    /// `blocks_each` blocks for every channel that can have them; the channels
    /// in between are marked unavailable.
    ///
    /// `uniform(2)` on a four-channel group is `[2, 0, 2, 0]`: two outputs with
    /// twice the RAM each.
    pub const fn uniform(blocks_each: u8) -> Result<Self, BlockPlanError> {
        let mut blocks = [0u8; N];
        let step = blocks_each as usize;
        if step == 0 {
            return Err(BlockPlanError::Empty);
        }
        let mut ch = 0;
        while ch + step <= N {
            blocks[ch] = blocks_each;
            ch += step;
        }
        Self::new(blocks)
    }

    /// Blocks owned by `ch`; `0` for an unavailable or out-of-range channel.
    pub const fn blocks(&self, ch: u8) -> u8 {
        if (ch as usize) < N {
            self.blocks[ch as usize]
        } else {
            0
        }
    }

    /// Can `ch` transmit?
    pub const fn is_available(&self, ch: u8) -> bool {
        self.blocks(ch) != 0
    }

    /// Size of `ch`'s RAM window in words, given the chip's block size.
    ///
    /// This is what a backend reports from
    /// [`RmtHw::ram_words`](crate::RmtHw::ram_words).
    pub const fn window_words(&self, ch: u8, block_words: usize) -> usize {
        self.blocks(ch) as usize * block_words
    }

    /// Word offset of `ch`'s window within the whole RMT RAM of the group.
    ///
    /// A channel always starts at its own block, so this is just
    /// `ch * block_words` — named because the arithmetic is easy to get subtly
    /// wrong in a backend, and because `mem_raddr_ex` on the ESP32-S3 is
    /// expressed in exactly these absolute words.
    pub const fn window_start(&self, ch: u8, block_words: usize) -> usize {
        ch as usize * block_words
    }

    /// The per-channel counts, as given.
    pub const fn as_array(&self) -> &[u8; N] {
        &self.blocks
    }

    /// How many channels can transmit under this plan.
    pub const fn available_channels(&self) -> usize {
        let mut n = 0;
        let mut ch = 0;
        while ch < N {
            if self.blocks[ch] != 0 {
                n += 1;
            }
            ch += 1;
        }
        n
    }
}

impl<const N: usize> Default for BlockPlan<N> {
    fn default() -> Self {
        Self::one_per_channel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_per_channel_is_valid_for_every_group_size() {
        assert_eq!(BlockPlan::<1>::one_per_channel().available_channels(), 1);
        assert_eq!(BlockPlan::<8>::one_per_channel().available_channels(), 8);
        assert!(BlockPlan::<8>::new(*BlockPlan::<8>::one_per_channel().as_array()).is_ok());
    }

    #[test]
    fn extension_consumes_the_neighbours() {
        let plan = BlockPlan::<4>::new([2, 0, 2, 0]).unwrap();
        assert!(plan.is_available(0));
        assert!(!plan.is_available(1));
        assert!(plan.is_available(2));
        assert!(!plan.is_available(3));
        assert_eq!(plan.window_words(0, 48), 96);
        assert_eq!(plan.window_words(1, 48), 0);
        assert_eq!(plan.window_start(2, 48), 96);
        assert_eq!(plan.available_channels(), 2);
    }

    #[test]
    fn uniform_lays_out_the_obvious_plans() {
        assert_eq!(
            BlockPlan::<4>::uniform(1).unwrap().as_array(),
            &[1, 1, 1, 1]
        );
        assert_eq!(
            BlockPlan::<4>::uniform(2).unwrap().as_array(),
            &[2, 0, 2, 0]
        );
        assert_eq!(
            BlockPlan::<4>::uniform(4).unwrap().as_array(),
            &[4, 0, 0, 0]
        );
        assert_eq!(
            BlockPlan::<8>::uniform(3).unwrap().as_array(),
            &[3, 0, 0, 3, 0, 0, 0, 0]
        );
        // Nothing fits: not an allocation at all.
        assert_eq!(BlockPlan::<2>::uniform(3), Err(BlockPlanError::Empty));
        assert_eq!(BlockPlan::<4>::uniform(0), Err(BlockPlanError::Empty));
    }

    #[test]
    fn overlap_names_the_absorbed_channel() {
        assert_eq!(
            BlockPlan::<4>::new([2, 1, 1, 1]),
            Err(BlockPlanError::Overlap { ch: 1 })
        );
        assert_eq!(
            BlockPlan::<4>::new([3, 0, 1, 1]),
            Err(BlockPlanError::Overlap { ch: 2 })
        );
        // Absorbed channels must say 0 — then the same shape is fine.
        assert!(BlockPlan::<4>::new([3, 0, 0, 1]).is_ok());
    }

    #[test]
    fn a_window_may_not_run_off_the_end() {
        assert_eq!(
            BlockPlan::<4>::new([1, 1, 1, 2]),
            Err(BlockPlanError::OutOfBlocks { ch: 3 })
        );
        assert_eq!(
            BlockPlan::<4>::new([5, 0, 0, 0]),
            Err(BlockPlanError::OutOfBlocks { ch: 0 })
        );
        assert_eq!(
            BlockPlan::<4>::new([0, 0, 0, 0]),
            Err(BlockPlanError::Empty)
        );
    }

    #[test]
    fn out_of_range_channels_are_unavailable_rather_than_a_panic() {
        let plan = BlockPlan::<4>::one_per_channel();
        assert!(!plan.is_available(4));
        assert_eq!(plan.blocks(200), 0);
        assert_eq!(plan.window_words(9, 48), 0);
    }

    #[test]
    fn checked_accepts_what_new_accepts() {
        const PLAN: BlockPlan<4> = BlockPlan::checked([2, 0, 1, 1]);
        assert_eq!(PLAN.available_channels(), 3);
    }
}
