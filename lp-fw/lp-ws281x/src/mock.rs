//! A host-side [`RmtHw`] that behaves like the transmitter, so the whole
//! driver can be tested without a chip.
//!
//! [`MockRmt`] models the parts of the peripheral the driver actually depends
//! on:
//!
//! * a per-channel RAM window of `ram_words` words,
//! * a read pointer that wraps within the window,
//! * **STOP-on-zero**: reading an all-zero word ends the transmission and
//!   raises `tx_end`,
//! * `tx_thr_event` when the read pointer crosses `tx_lim` (with `tx_lim ==
//!   ram_words` meaning the wrap back to word 0),
//! * a log of every word actually transmitted, which is the frame as the strip
//!   would have seen it.
//!
//! Time is measured in **words** — one word is one bit on the wire, ≈1.25 µs at
//! 800 kHz. [`MockRmt::advance`] is the clock; tests decide when interrupts get
//! serviced, which is how a late or lost refill is simulated.
//!
//! Enabled by the default `mock` feature; firmware depends on this crate with
//! `default-features = false`.

use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use crate::blocks::BlockPlan;
use crate::driver::Ws281xDriver;
use crate::hw::{InterruptFlags, RmtHw};
use crate::pulse::{PulseItem, STOP_WORD};

/// One simulated RMT TX channel.
#[derive(Debug)]
struct MockChannel {
    ram: Vec<Cell<u32>>,
    tx_lim: Cell<u16>,
    pos: Cell<usize>,
    running: Cell<bool>,
    emitted: RefCell<Vec<u32>>,
    starts: Cell<usize>,
    stops: Cell<usize>,
    writes: Cell<usize>,
}

impl MockChannel {
    fn new(ram_words: usize) -> Self {
        Self {
            ram: (0..ram_words).map(|_| Cell::new(0)).collect(),
            tx_lim: Cell::new(0),
            pos: Cell::new(0),
            running: Cell::new(false),
            emitted: RefCell::new(Vec::new()),
            starts: Cell::new(0),
            stops: Cell::new(0),
            writes: Cell::new(0),
        }
    }
}

/// A simulated RMT peripheral.
///
/// Not `Sync` — it is a single-threaded test double. The driver only ever needs
/// `&self` on the backend, so interior mutability is all that is required.
#[derive(Debug)]
pub struct MockRmt {
    channels: Vec<MockChannel>,
    pending: Cell<InterruptFlags>,
    writes_per_word: Cell<usize>,
}

impl MockRmt {
    /// `channels` identical channels of `ram_words` words each.
    pub fn new(channels: usize, ram_words: usize) -> Self {
        Self::with_ram_sizes(&alloc::vec![ram_words; channels])
    }

    /// One channel per entry, sized individually.
    pub fn with_ram_sizes(sizes: &[usize]) -> Self {
        Self {
            channels: sizes.iter().map(|&w| MockChannel::new(w)).collect(),
            pending: Cell::new(InterruptFlags::NONE),
            writes_per_word: Cell::new(0),
        }
    }

    /// The peripheral a [`BlockPlan`] describes: `N` channels, each with
    /// `blocks(ch) * block_words` words (unavailable channels get none).
    ///
    /// Building the mock and the driver from the same plan is exactly what a
    /// chip backend does, so the host tests exercise the real relationship
    /// rather than an assumed one.
    pub fn from_block_plan<const N: usize>(plan: &BlockPlan<N>, block_words: usize) -> Self {
        let sizes: Vec<usize> = (0..N)
            .map(|ch| plan.window_words(ch as u8, block_words))
            .collect();
        Self::with_ram_sizes(&sizes)
    }

    /// Make refills cost time: the transmitter advances one word for every
    /// `writes_per_word` RAM writes on that channel.
    ///
    /// The default, `0`, means a refill is instantaneous. A non-zero value
    /// models a handler that is racing the transmitter — which is exactly what
    /// the read-pointer-delta statistic measures.
    pub fn set_refill_cost(&self, writes_per_word: usize) {
        self.writes_per_word.set(writes_per_word);
    }

    fn channel(&self, ch: u8) -> &MockChannel {
        self.channels
            .get(ch as usize)
            .expect("mock: channel index out of range")
    }

    /// Run `ch`'s transmitter for up to `words` words.
    ///
    /// Returns the number actually transmitted, which is shorter when the
    /// transmitter hits a STOP word (or was not running).
    pub fn advance(&self, ch: u8, words: usize) -> usize {
        let c = self.channel(ch);
        let len = c.ram.len();
        if len == 0 {
            // A channel a `BlockPlan` marked unavailable: no RAM, so nothing to
            // transmit. It can never be started, but `advance_all` still visits
            // it.
            return 0;
        }
        let mut done = 0;
        while done < words && c.running.get() {
            let word = c.ram[c.pos.get()].get();
            if word == STOP_WORD {
                c.running.set(false);
                self.flag(|f| f.set_end(ch));
                break;
            }
            c.emitted.borrow_mut().push(word);
            c.pos.set((c.pos.get() + 1) % len);
            done += 1;

            // `tx_lim == ram_words` arms the wrap back to word 0.
            let boundary = (c.tx_lim.get() as usize) % len;
            if c.pos.get() == boundary {
                self.flag(|f| f.set_threshold(ch));
            }
        }
        done
    }

    /// Advance every running channel by `words`.
    pub fn advance_all(&self, words: usize) {
        for ch in 0..self.channels.len() {
            self.advance(ch as u8, words);
        }
    }

    /// Is `ch` transmitting?
    pub fn is_running(&self, ch: u8) -> bool {
        self.channel(ch).running.get()
    }

    /// Is any channel transmitting?
    pub fn any_running(&self) -> bool {
        self.channels.iter().any(|c| c.running.get())
    }

    /// Is an interrupt cause waiting to be taken?
    pub fn has_pending(&self) -> bool {
        !self.pending.get().is_empty()
    }

    /// The currently pending causes, without clearing them.
    pub fn peek_interrupts(&self) -> InterruptFlags {
        self.pending.get()
    }

    /// Throw away all pending `tx_thr_event` bits — simulates a threshold
    /// interrupt that was lost (masked out, or preempted past its deadline).
    pub fn drop_threshold_interrupts(&self) {
        let mut f = self.pending.get();
        f.threshold = 0;
        self.pending.set(f);
    }

    /// As [`Self::drop_threshold_interrupts`], but for one channel only.
    pub fn drop_threshold_interrupt(&self, ch: u8) {
        let mut f = self.pending.get();
        f.threshold &= !(1u32 << ch);
        self.pending.set(f);
    }

    /// Inject interrupt causes, e.g. a `tx_err` the model does not produce on
    /// its own.
    pub fn raise(&self, flags: InterruptFlags) {
        self.pending.set(self.pending.get().merged(flags));
    }

    /// `ch`'s read pointer, as a `usize`.
    pub fn read_pos_words(&self, ch: u8) -> usize {
        self.channel(ch).pos.get()
    }

    /// Every word `ch` has transmitted since the last [`Self::clear_emitted`].
    pub fn emitted(&self, ch: u8) -> Vec<u32> {
        self.channel(ch).emitted.borrow().clone()
    }

    /// [`Self::emitted`], decoded into level/duration pairs.
    pub fn emitted_items(&self, ch: u8) -> Vec<PulseItem> {
        self.emitted(ch)
            .into_iter()
            .filter_map(PulseItem::decode)
            .collect()
    }

    /// Forget `ch`'s transmitted-word log.
    pub fn clear_emitted(&self, ch: u8) {
        self.channel(ch).emitted.borrow_mut().clear();
    }

    /// A copy of `ch`'s RAM window.
    pub fn ram(&self, ch: u8) -> Vec<u32> {
        self.channel(ch).ram.iter().map(Cell::get).collect()
    }

    /// `ch`'s current `tx_lim`.
    pub fn tx_lim(&self, ch: u8) -> u16 {
        self.channel(ch).tx_lim.get()
    }

    /// How many times `ch` has been started.
    pub fn start_count(&self, ch: u8) -> usize {
        self.channel(ch).starts.get()
    }

    /// How many times `ch` has been explicitly stopped.
    pub fn stop_count(&self, ch: u8) -> usize {
        self.channel(ch).stops.get()
    }

    fn flag(&self, f: impl FnOnce(&mut InterruptFlags)) {
        let mut flags = self.pending.get();
        f(&mut flags);
        self.pending.set(flags);
    }
}

impl RmtHw for MockRmt {
    fn ram_words(&self, ch: u8) -> usize {
        self.channel(ch).ram.len()
    }

    fn write_ram(&self, ch: u8, word_idx: usize, value: u32) {
        let c = self.channel(ch);
        assert!(
            word_idx < c.ram.len(),
            "mock: write_ram({ch}, {word_idx}) outside the {}-word window",
            c.ram.len()
        );
        c.ram[word_idx].set(value);

        let cost = self.writes_per_word.get();
        if cost > 0 {
            c.writes.set(c.writes.get() + 1);
            if c.writes.get() % cost == 0 {
                self.advance(ch, 1);
            }
        }
    }

    fn set_tx_threshold(&self, ch: u8, words: u16) {
        self.channel(ch).tx_lim.set(words);
    }

    fn read_pos(&self, ch: u8) -> u16 {
        self.channel(ch).pos.get() as u16
    }

    fn start_tx(&self, ch: u8) {
        let c = self.channel(ch);
        c.pos.set(0);
        c.running.set(true);
        c.starts.set(c.starts.get() + 1);
    }

    fn stop_tx(&self, ch: u8) {
        let c = self.channel(ch);
        if c.running.get() {
            c.running.set(false);
            c.stops.set(c.stops.get() + 1);
        }
    }

    fn take_interrupts(&self) -> InterruptFlags {
        self.pending.replace(InterruptFlags::NONE)
    }
}

/// A scripted clock for [`MockRmt`]: advance, service interrupts, repeat.
///
/// The defaults model a healthy system — the transmitter is stepped one word at
/// a time and the handler runs one word after the cause appears.
#[derive(Debug, Clone, Copy)]
pub struct Pump {
    /// Words the transmitter advances per poll.
    pub step: usize,
    /// Words that pass between a cause appearing and the handler running, i.e.
    /// interrupt entry latency.
    ///
    /// Keep this ≥ 1 for realistic runs: the threshold event marks the boundary
    /// between halves, so with zero latency the read pointer sits exactly on the
    /// guard slot and the driver (correctly) declines to plant a guard.
    pub isr_latency: usize,
    /// Index of a threshold interrupt to throw away instead of servicing
    /// (0 = the first one of the run).
    pub drop_threshold: Option<usize>,
    /// Abort the run after this many transmitted words.
    pub max_words: usize,
}

impl Default for Pump {
    fn default() -> Self {
        Self {
            step: 1,
            isr_latency: 1,
            drop_threshold: None,
            max_words: 1 << 20,
        }
    }
}

impl Pump {
    /// Run until no channel is transmitting (or [`Self::max_words`] is hit),
    /// returning the number of words the clock advanced.
    pub fn run<const N: usize>(&self, driver: &Ws281xDriver<MockRmt, N>) -> usize {
        let mock = driver.hw();
        let mut words = 0;
        let mut thresholds_seen = 0;

        while mock.any_running() && words < self.max_words {
            mock.advance_all(self.step);
            words += self.step;
            if !mock.has_pending() {
                continue;
            }

            let had_threshold = mock.peek_interrupts().threshold != 0;
            if self.isr_latency > 0 {
                mock.advance_all(self.isr_latency);
                words += self.isr_latency;
            }

            if had_threshold {
                let index = thresholds_seen;
                thresholds_seen += 1;
                if self.drop_threshold == Some(index) {
                    mock.drop_threshold_interrupts();
                    if !mock.has_pending() {
                        continue;
                    }
                }
            }
            driver.on_interrupt();
        }

        // The transmitter can stop *during* a refill (see
        // [`MockRmt::set_refill_cost`]), which leaves a `tx_end` behind after
        // the loop has already seen every channel go idle. Hardware would
        // re-enter the handler; so does this.
        for _ in 0..4 {
            if !mock.has_pending() {
                break;
            }
            driver.on_interrupt();
        }
        words
    }
}
