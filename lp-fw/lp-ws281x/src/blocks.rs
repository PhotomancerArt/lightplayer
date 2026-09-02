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
//!
//! Since the board manifest became the sole authority on channel count (ADR
//! `2026-07-31-lp-ws281x-multi-channel-driver-adoption`), the choice is not an
//! authoring question at all: a chip backend computes the plan **once at
//! driver init** from the number of declared channels
//! ([`BlockPlan::for_channels`]) and publishes it through a
//! [`SharedBlockPlan`], giving a one-strip board the whole buffer and a
//! many-strip board the even split — no cargo feature, no rebuild.

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

    /// The plan for `channels` declared channels: each gets `blocks_each`
    /// consecutive blocks, laid out back to back from slot 0, and any
    /// remainder blocks at the top of the group stay idle.
    ///
    /// This is the manifest-driven form of [`Self::uniform`]: where `uniform`
    /// fills the whole group at a given width, `for_channels` allocates
    /// **exactly** the declared count — computing the width is the chip
    /// backend's job (`floor(usable_blocks / channels)`, plus any per-chip
    /// cap such as the classic's 9-bit `tx_lim`).
    ///
    /// ```
    /// use lp_ws281x::{BlockPlan, BlockPlanError};
    ///
    /// // Three declared channels on an eight-block chip: floor(8/3) = 2 each,
    /// // slots 0/2/4, blocks 6 and 7 idle.
    /// assert_eq!(
    ///     BlockPlan::<8>::for_channels(3, 2).unwrap().as_array(),
    ///     &[2, 0, 2, 0, 2, 0, 0, 0]
    /// );
    /// // One declared channel absorbing the whole group.
    /// assert_eq!(
    ///     BlockPlan::<4>::for_channels(1, 4).unwrap().as_array(),
    ///     &[4, 0, 0, 0]
    /// );
    /// // More than fits is an error, not a truncation.
    /// assert_eq!(
    ///     BlockPlan::<4>::for_channels(3, 2),
    ///     Err(BlockPlanError::OutOfBlocks { ch: 4 })
    /// );
    /// ```
    pub const fn for_channels(channels: usize, blocks_each: u8) -> Result<Self, BlockPlanError> {
        if channels == 0 || blocks_each == 0 {
            return Err(BlockPlanError::Empty);
        }
        let step = blocks_each as usize;
        if channels * step > N {
            // Name the first channel that would not fit, in slot terms.
            return Err(BlockPlanError::OutOfBlocks {
                ch: (N / step * step) as u8,
            });
        }
        let mut blocks = [0u8; N];
        let mut ch = 0;
        while ch < channels {
            blocks[ch * step] = blocks_each;
            ch += 1;
        }
        Self::new(blocks)
    }

    /// The slot backing declared channel `index` — the `index`-th channel the
    /// plan leaves available, in slot order — or `None` when the plan has no
    /// slot for it.
    ///
    /// This is the manifest-index → RMT-slot mapping: manifest resource
    /// `/rmt/ws281xK` drives `nth_available(K)`. Under a
    /// [`Self::for_channels`] plan this is `K * blocks_each`, but counting
    /// available slots keeps it honest under a hand-written plan too.
    pub const fn nth_available(&self, index: usize) -> Option<u8> {
        let mut seen = 0;
        let mut ch = 0;
        while ch < N {
            if self.blocks[ch] != 0 {
                if seen == index {
                    return Some(ch as u8);
                }
                seen += 1;
            }
            ch += 1;
        }
        None
    }

    /// The declared channel index a slot backs — the inverse of
    /// [`Self::nth_available`] — or `None` for an absorbed, idle or
    /// out-of-range slot.
    pub const fn index_of_slot(&self, slot: u8) -> Option<usize> {
        if slot as usize >= N || self.blocks[slot as usize] == 0 {
            return None;
        }
        let mut index = 0;
        let mut ch = 0;
        while ch < slot as usize {
            if self.blocks[ch] != 0 {
                index += 1;
            }
            ch += 1;
        }
        Some(index)
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

/// Why [`SharedBlockPlan::init`] refused a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedPlanError {
    /// A different plan was already published. The stored plan wins —
    /// windows must never change after init (see [`crate::RmtHw::ram_words`]).
    AlreadyInit,
}

/// A [`BlockPlan`] published once at driver init and read from then on —
/// including from the interrupt handler.
///
/// The chip backends live in `static`s (they are shared with the ISR), so
/// their plan cannot arrive through a constructor at boot; it has to be
/// written into place. This is that slot: interior-mutable, `Sync`, and
/// **fail-closed** — until [`Self::init`] succeeds every channel reads as
/// unavailable with a zero-word window, so nothing can transmit under a plan
/// that was never chosen.
///
/// # Set once, then frozen
///
/// [`crate::RmtHw::ram_words`] must be stable for the driver's lifetime: the
/// driver caches window sizes at [`configure`](crate::Ws281xDriver::configure)
/// time and the refill handler does modular arithmetic on them, so a window
/// that moved mid-frame would corrupt the transmission. `init` therefore
/// accepts the first plan, tolerates an identical re-init (a harness
/// re-creating its channel), and refuses a different one.
///
/// Publication is release/acquire on the `set` flag; in practice the plan is
/// written during single-threaded boot, before the RMT interrupt is installed
/// and before any channel is configured.
pub struct SharedBlockPlan<const N: usize> {
    blocks: [core::sync::atomic::AtomicU8; N],
    set: core::sync::atomic::AtomicBool,
}

impl<const N: usize> SharedBlockPlan<N> {
    /// An empty slot: every channel unavailable until [`Self::init`].
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            blocks: [const { core::sync::atomic::AtomicU8::new(0) }; N],
            set: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Publish `plan`. First caller wins; an identical re-init is a no-op
    /// `Ok`; a different plan is refused.
    pub fn init(&self, plan: BlockPlan<N>) -> Result<(), SharedPlanError> {
        use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
        if self.set.load(Acquire) {
            return if self.get().as_ref() == Some(&plan) {
                Ok(())
            } else {
                Err(SharedPlanError::AlreadyInit)
            };
        }
        let mut ch = 0;
        while ch < N {
            self.blocks[ch].store(plan.blocks[ch], Relaxed);
            ch += 1;
        }
        self.set.store(true, Release);
        Ok(())
    }

    /// The published plan, or `None` before [`Self::init`].
    pub fn get(&self) -> Option<BlockPlan<N>> {
        use core::sync::atomic::Ordering::{Acquire, Relaxed};
        if !self.set.load(Acquire) {
            return None;
        }
        let mut blocks = [0u8; N];
        let mut ch = 0;
        while ch < N {
            blocks[ch] = self.blocks[ch].load(Relaxed);
            ch += 1;
        }
        Some(BlockPlan { blocks })
    }

    /// [`BlockPlan::blocks`], fail-closed: `0` before init.
    ///
    /// `always`-inlined (with the two helpers below): backends call these from
    /// the interrupt path, and an outlined copy in flash is a cache miss on the
    /// refill deadline — measured on the ESP32-C6 image at `opt-level = "z"`.
    #[inline(always)]
    pub fn blocks(&self, ch: u8) -> u8 {
        use core::sync::atomic::Ordering::{Acquire, Relaxed};
        if !self.set.load(Acquire) || ch as usize >= N {
            return 0;
        }
        self.blocks[ch as usize].load(Relaxed)
    }

    /// [`BlockPlan::is_available`], fail-closed: `false` before init.
    pub fn is_available(&self, ch: u8) -> bool {
        self.blocks(ch) != 0
    }

    /// [`BlockPlan::window_words`], fail-closed: `0` before init.
    #[inline(always)]
    pub fn window_words(&self, ch: u8, block_words: usize) -> usize {
        self.blocks(ch) as usize * block_words
    }

    /// [`BlockPlan::window_start`] — needs no plan, kept here so backends can
    /// speak to one type.
    #[inline(always)]
    pub fn window_start(&self, ch: u8, block_words: usize) -> usize {
        ch as usize * block_words
    }

    /// [`BlockPlan::available_channels`], fail-closed: `0` before init.
    pub fn available_channels(&self) -> usize {
        match self.get() {
            Some(plan) => plan.available_channels(),
            None => 0,
        }
    }

    /// [`BlockPlan::nth_available`], fail-closed: `None` before init.
    pub fn nth_available(&self, index: usize) -> Option<u8> {
        self.get()?.nth_available(index)
    }

    /// [`BlockPlan::index_of_slot`], fail-closed: `None` before init.
    pub fn index_of_slot(&self, slot: u8) -> Option<usize> {
        self.get()?.index_of_slot(slot)
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

    // --- for_channels: the manifest-count → plan arithmetic ------------------

    #[test]
    fn for_channels_lays_out_every_shipped_shape() {
        // C6, 2 declared channels: one block each.
        assert_eq!(
            BlockPlan::<4>::for_channels(2, 1).unwrap().as_array(),
            &[1, 1, 0, 0]
        );
        // C6, 1 declared channel: the whole group, RX blocks absorbed.
        assert_eq!(
            BlockPlan::<4>::for_channels(1, 4).unwrap().as_array(),
            &[4, 0, 0, 0]
        );
        // Classic, 4 declared channels: today's [2,0,2,0,2,0,2,0].
        assert_eq!(
            BlockPlan::<8>::for_channels(4, 2).unwrap().as_array(),
            &[2, 0, 2, 0, 2, 0, 2, 0]
        );
        // S3, 4 declared channels: one block each.
        assert_eq!(
            BlockPlan::<4>::for_channels(4, 1).unwrap().as_array(),
            &[1, 1, 1, 1]
        );
        // S3, 1 declared channel: the whole TX group.
        assert_eq!(
            BlockPlan::<4>::for_channels(1, 4).unwrap().as_array(),
            &[4, 0, 0, 0]
        );
    }

    #[test]
    fn for_channels_windows_and_stride() {
        // Classic, 2 declared at the 4-block cap: 256-word windows at slots 0/4.
        let plan = BlockPlan::<8>::for_channels(2, 4).unwrap();
        assert_eq!(plan.as_array(), &[4, 0, 0, 0, 4, 0, 0, 0]);
        assert_eq!(plan.window_words(0, 64), 256);
        assert_eq!(plan.window_words(4, 64), 256);
        assert_eq!(plan.window_start(4, 64), 256);
        assert_eq!(plan.available_channels(), 2);
        for absorbed in [1u8, 2, 3, 5, 6, 7] {
            assert!(!plan.is_available(absorbed), "slot {absorbed}");
            assert_eq!(plan.window_words(absorbed, 64), 0);
        }
    }

    #[test]
    fn for_channels_leaves_odd_remainder_blocks_idle() {
        // 3 declared on 8 blocks: floor(8/3) = 2 each, blocks 6 and 7 idle —
        // idle, not granted to a fourth channel the manifest never declared.
        let plan = BlockPlan::<8>::for_channels(3, 2).unwrap();
        assert_eq!(plan.as_array(), &[2, 0, 2, 0, 2, 0, 0, 0]);
        assert_eq!(plan.available_channels(), 3);
        assert!(!plan.is_available(6));
        assert!(!plan.is_available(7));
    }

    #[test]
    fn for_channels_rejects_what_cannot_fit() {
        assert_eq!(
            BlockPlan::<4>::for_channels(0, 1),
            Err(BlockPlanError::Empty)
        );
        assert_eq!(
            BlockPlan::<4>::for_channels(2, 0),
            Err(BlockPlanError::Empty)
        );
        assert_eq!(
            BlockPlan::<4>::for_channels(5, 1),
            Err(BlockPlanError::OutOfBlocks { ch: 4 })
        );
        assert_eq!(
            BlockPlan::<4>::for_channels(3, 2),
            Err(BlockPlanError::OutOfBlocks { ch: 4 })
        );
    }

    // --- slot mapping --------------------------------------------------------

    #[test]
    fn nth_available_maps_declared_index_to_slot() {
        let plan = BlockPlan::<8>::for_channels(4, 2).unwrap();
        assert_eq!(plan.nth_available(0), Some(0));
        assert_eq!(plan.nth_available(1), Some(2));
        assert_eq!(plan.nth_available(2), Some(4));
        assert_eq!(plan.nth_available(3), Some(6));
        assert_eq!(plan.nth_available(4), None);

        let wide = BlockPlan::<4>::for_channels(1, 4).unwrap();
        assert_eq!(wide.nth_available(0), Some(0));
        assert_eq!(wide.nth_available(1), None);
    }

    #[test]
    fn index_of_slot_is_the_inverse_of_nth_available() {
        let plan = BlockPlan::<8>::for_channels(3, 2).unwrap();
        for index in 0..3 {
            let slot = plan.nth_available(index).unwrap();
            assert_eq!(plan.index_of_slot(slot), Some(index));
        }
        for absorbed in [1u8, 3, 5, 6, 7, 200] {
            assert_eq!(plan.index_of_slot(absorbed), None, "slot {absorbed}");
        }
    }

    // --- SharedBlockPlan -----------------------------------------------------

    #[test]
    fn shared_plan_fails_closed_before_init() {
        let shared: SharedBlockPlan<4> = SharedBlockPlan::new();
        assert_eq!(shared.get(), None);
        assert_eq!(shared.blocks(0), 0);
        assert!(!shared.is_available(0));
        assert_eq!(shared.window_words(0, 48), 0);
        assert_eq!(shared.available_channels(), 0);
        assert_eq!(shared.nth_available(0), None);
        assert_eq!(shared.index_of_slot(0), None);
    }

    #[test]
    fn shared_plan_publishes_and_freezes() {
        let shared: SharedBlockPlan<4> = SharedBlockPlan::new();
        let plan = BlockPlan::<4>::for_channels(1, 4).unwrap();
        shared.init(plan).unwrap();

        assert_eq!(shared.get(), Some(plan));
        assert_eq!(shared.window_words(0, 48), 192);
        assert!(!shared.is_available(1));
        assert_eq!(shared.nth_available(0), Some(0));

        // Identical re-init: a no-op Ok (a harness re-creating its channel).
        shared.init(plan).unwrap();
        // A different plan: refused, and the stored plan is untouched.
        let other = BlockPlan::<4>::for_channels(2, 1).unwrap();
        assert_eq!(shared.init(other), Err(SharedPlanError::AlreadyInit));
        assert_eq!(shared.get(), Some(plan));
    }
}
