//! The backend seam: everything chip-specific a driver needs, and nothing else.
//!
//! An [`RmtHw`] implementation is pure register glue — it never decides *what*
//! to write or *when*. All sequencing (which half to refill, where the guard
//! goes, when to flip the threshold, when a frame is done) lives in
//! [`crate::driver`], so it is tested once on the host and reused verbatim by
//! every chip backend.
//!
//! The trait takes `&self` throughout: the driver is shared between thread
//! context and the interrupt handler, and a backend's state is memory-mapped
//! registers, not owned data.

/// Snapshot of the RMT interrupt causes, one bit per channel.
///
/// Backends read the peripheral's raw/status register, clear the bits they
/// report, and return them here — so the driver sees a consistent picture and
/// no cause can be lost between the read and the clear.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InterruptFlags {
    /// `tx_thr_event`: the transmitter passed the configured threshold, so the
    /// half it just left is free to refill.
    pub threshold: u32,
    /// `tx_end`: the transmission finished — either it ran off the end of the
    /// data or it hit a STOP word.
    pub end: u32,
    /// `tx_err`: the transmitter under-ran or otherwise faulted.
    pub error: u32,
}

impl InterruptFlags {
    /// No pending causes.
    pub const NONE: Self = Self {
        threshold: 0,
        end: 0,
        error: 0,
    };

    /// True when nothing at all is flagged.
    pub const fn is_empty(&self) -> bool {
        self.threshold == 0 && self.end == 0 && self.error == 0
    }

    /// Bitwise union — useful for backends that coalesce several reads.
    pub const fn merged(self, other: Self) -> Self {
        Self {
            threshold: self.threshold | other.threshold,
            end: self.end | other.end,
            error: self.error | other.error,
        }
    }

    /// Threshold flagged for `ch`?
    pub const fn threshold_for(&self, ch: u8) -> bool {
        bit(self.threshold, ch)
    }

    /// End flagged for `ch`?
    pub const fn end_for(&self, ch: u8) -> bool {
        bit(self.end, ch)
    }

    /// Error flagged for `ch`?
    pub const fn error_for(&self, ch: u8) -> bool {
        bit(self.error, ch)
    }

    /// Set the threshold bit for `ch` (backend/mock helper).
    pub fn set_threshold(&mut self, ch: u8) {
        self.threshold |= mask(ch);
    }

    /// Clear the threshold bit for `ch` (test-hook helper).
    pub fn clear_threshold(&mut self, ch: u8) {
        self.threshold &= !mask(ch);
    }

    /// Set the end bit for `ch` (backend/mock helper).
    pub fn set_end(&mut self, ch: u8) {
        self.end |= mask(ch);
    }

    /// Set the error bit for `ch` (backend/mock helper).
    pub fn set_error(&mut self, ch: u8) {
        self.error |= mask(ch);
    }
}

const fn mask(ch: u8) -> u32 {
    if ch < 32 {
        1u32 << ch
    } else {
        0
    }
}

const fn bit(bits: u32, ch: u8) -> bool {
    bits & mask(ch) != 0
}

/// A contiguous raw view of one channel's RMT RAM window, for backends whose
/// RAM is plain memory-mapped words.
///
/// Returned by [`RmtHw::ram_window`] so the refill path can hoist the window
/// base and bound out of its per-word loop: measured on the classic ESP32,
/// the per-word block-plan lookup and double bounds check behind
/// [`RmtHw::write_ram`] cost ~4 CPU cycles per word against a ~12-cycle APB
/// store — real money at four coincident refills against an 80 µs deadline.
///
/// # Contract (on the backend returning `Some`)
///
/// * `base` addresses exactly [`RmtHw::ram_words`]`(ch)` writable `u32`
///   words — the same window, the same size — and stays valid for the
///   driver's lifetime (block plans are fixed at init).
/// * Stores through it must be equivalent to [`RmtHw::write_ram`] calls: the
///   driver performs volatile word stores and never reads back.
///
/// The driver never holds one across calls; it is re-fetched per fill.
#[derive(Debug, Clone, Copy)]
pub struct RamWindow {
    /// First word of the channel's window.
    pub base: *mut u32,
    /// Words in the window (equal to [`RmtHw::ram_words`] for the channel).
    pub words: usize,
}

/// The chip-specific half of the driver.
///
/// # Implementing a backend
///
/// Every method addresses one RMT **TX channel** by its driver-local index
/// `ch`, which the backend maps to whatever the chip calls it. Word indices
/// passed to [`write_ram`](RmtHw::write_ram) are relative to the channel's own
/// RAM window (`0..ram_words(ch)`), so the core never needs to know about
/// per-chip block sizes (48 words on S3/C6, 64 on the classic ESP32) or how
/// many blocks a channel was given.
///
/// Implementations must be sound to call from an interrupt handler and must not
/// block.
pub trait RmtHw {
    /// Number of RMT RAM words usable by `ch` (block size × blocks assigned).
    ///
    /// `0` means the channel is **unavailable** — absorbed by a neighbour's
    /// window, left out of the block plan, or the plan has not been published
    /// yet — and [`crate::Ws281xDriver::configure`] refuses it. A non-zero
    /// count must be stable for the lifetime of the driver, at least 4, and
    /// even — the driver splits it into two equal ping-pong halves. A plan
    /// fixed at driver init (before any channel is configured) satisfies
    /// this; windows must never change after init.
    fn ram_words(&self, ch: u8) -> usize;

    /// Write one word into `ch`'s RAM window. Backends do a volatile store.
    fn write_ram(&self, ch: u8, word_idx: usize, value: u32);

    /// A raw contiguous view of `ch`'s RAM window, or `None` when the backend
    /// cannot (or need not) offer one.
    ///
    /// The default is `None`, which keeps every existing backend and the mock
    /// on the per-word [`Self::write_ram`] path — same writes, same modelled
    /// costs, same assertions. A memory-mapped backend that returns `Some`
    /// lets the refill loop hoist the window base and bound once per fill;
    /// see [`RamWindow`] for the contract and the measured why. Both paths
    /// write identical words in identical order — this is a cost lever, not
    /// a semantic fork.
    fn ram_window(&self, ch: u8) -> Option<RamWindow> {
        let _ = ch;
        None
    }

    /// Set `ch`'s threshold (`tx_lim`) to `words`.
    ///
    /// The driver alternates between "half the window" and "the whole window",
    /// so the threshold event marks each crossing between the two halves.
    fn set_tx_threshold(&self, ch: u8, words: u16);

    /// Current hardware read position within `ch`'s RAM window
    /// (`mem_raddr_ex`), in words, `0..ram_words(ch)`.
    fn read_pos(&self, ch: u8) -> u16;

    /// Begin transmitting `ch` from word 0.
    ///
    /// This is the complete start sequence, including resetting the memory read
    /// pointer and applying the pending configuration — the driver has already
    /// filled RAM and set the threshold when this is called.
    fn start_tx(&self, ch: u8);

    /// Stop `ch` immediately.
    ///
    /// Must tolerate being called on an already-idle channel:
    /// [`crate::Ws281xDriver::abort`] runs unconditionally on the cleanup path.
    fn stop_tx(&self, ch: u8);

    /// Read **and clear** the pending interrupt causes for all channels.
    fn take_interrupts(&self) -> InterruptFlags;
}
