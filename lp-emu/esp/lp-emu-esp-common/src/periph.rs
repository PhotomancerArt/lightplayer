//! What a peripheral is, and the only things it is allowed to see.
//!
//! A [`Peripheral`] is a register window with behaviour. It gets the
//! register-aligned offset, the access [`Width`] and a [`BusCx`], and that
//! is the whole of its world: it never sees the hart's registers, never
//! sees another peripheral, and never reads a clock. It raises **interrupt
//! source levels** ([`IrqLines`]) and schedules **events**; turning a source
//! level into a CPU interrupt number is the chip's interrupt matrix, a layer
//! above (plan PD6 — `IrqLines` is chip-wide and per-source, the matrix is
//! per-hart).
//!
//! That narrowness is what makes a peripheral testable on its own and what
//! keeps this crate free of chip numbers.

use alloc::boxed::Box;
use alloc::vec::Vec;
use lp_emu_core::sched::{Cycles, EventId, Scheduler};

use crate::host::HostSinks;
use crate::trace::Trace;

/// The width of a single bus access.
///
/// Both widths happen against the same register on real firmware:
/// `esp-println` writes the USB-Serial-JTAG FIFO as a 32-bit word, while
/// the ROM's `uart_tx_one_char` writes UART0's FIFO as a byte. A peripheral
/// that assumed words would silently lose the ROM's output, so the width
/// travels with the access instead of being flattened.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Width {
    Byte,
    Half,
    Word,
}

impl Width {
    pub const fn bytes(self) -> u32 {
        match self {
            Width::Byte => 1,
            Width::Half => 2,
            Width::Word => 4,
        }
    }

    /// The value mask for this width, right-aligned (`0xff`, `0xffff`,
    /// `0xffff_ffff`).
    pub const fn mask(self) -> u32 {
        match self {
            Width::Byte => 0xff,
            Width::Half => 0xffff,
            Width::Word => 0xffff_ffff,
        }
    }

    /// The mask of the byte lane at `byte_in_word` (0..4), in word position.
    ///
    /// `Width::Half` at lane 2 gives `0xffff_0000`. Lanes past the end of
    /// the word are clamped, so a caller cannot produce a mask that reaches
    /// into the next register.
    pub const fn lane_mask(self, byte_in_word: u32) -> u32 {
        let shift = (byte_in_word & 3) * 8;
        let m = self.mask();
        // `m << shift` with the overflow bits dropped, in const-friendly form.
        ((m as u64) << shift) as u32
    }
}

/// A `Peripheral`'s view of the machine for the duration of one access.
///
/// Deliberately small: cycles, the PC that issued the access, the hart index
/// (PD6 — one hart today, the field is what makes a per-core register block
/// possible without reshaping this trait), the scheduler, the interrupt
/// source lines, the bus trace, and the host byte streams.
pub struct BusCx<'a> {
    /// Guest cycle count at the access.
    pub now: Cycles,
    /// The PC of the instruction performing the access. For the trace, for
    /// the spin detector, and for "who wrote this?" during bring-up.
    pub pc: u32,
    /// Which hart issued the access (PD6). Always 0 until a second one
    /// exists.
    pub hart: usize,
    pub sched: &'a mut Scheduler,
    pub irq: &'a mut IrqLines,
    pub trace: &'a mut Trace,
    pub host: &'a mut HostSinks,
}

/// Chip-wide interrupt **source** levels.
///
/// Level-triggered, one bit per source, deliberately not an edge queue: the
/// ESP32 interrupt matrix samples levels, and a peripheral that owns a
/// status bit owns the level that follows from it. Turning a set of levels
/// into a CPU interrupt number for a hart is the matrix's job.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IrqLines {
    /// 128 sources; the C6 declares 80-odd.
    words: [u64; 2],
    /// Set whenever a level actually changed, cleared by
    /// [`take_changed`](Self::take_changed). Lets the machine skip a matrix
    /// re-evaluation when nothing moved.
    changed: bool,
}

/// How many interrupt sources [`IrqLines`] can carry.
pub const IRQ_SOURCES: u16 = 128;

impl IrqLines {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set source `source`'s level. Sources at or past [`IRQ_SOURCES`] are
    /// ignored rather than panicking — a chip layer that asks for source 200
    /// has a bug, but an emulator that aborts mid-boot tells you less than
    /// one that keeps running with the line low.
    pub fn set_level(&mut self, source: u16, level: bool) {
        if source >= IRQ_SOURCES {
            log::warn!("IrqLines::set_level: source {source} is out of range, ignored");
            return;
        }
        let (w, b) = (usize::from(source >> 6), source & 63);
        let bit = 1u64 << b;
        let was = self.words[w] & bit != 0;
        if was == level {
            return;
        }
        if level {
            self.words[w] |= bit;
        } else {
            self.words[w] &= !bit;
        }
        self.changed = true;
    }

    pub fn level(&self, source: u16) -> bool {
        if source >= IRQ_SOURCES {
            return false;
        }
        self.words[usize::from(source >> 6)] & (1u64 << (source & 63)) != 0
    }

    /// Raw level bitmap, low sources first. The matrix reads this.
    pub fn raw(&self) -> [u64; 2] {
        self.words
    }

    pub fn any(&self) -> bool {
        self.words[0] != 0 || self.words[1] != 0
    }

    /// `true` if any level changed since the last call, and clears the flag.
    pub fn take_changed(&mut self) -> bool {
        core::mem::replace(&mut self.changed, false)
    }

    /// Drop every level. Machine reset.
    pub fn clear(&mut self) {
        self.words = [0; 2];
        self.changed = true;
    }
}

/// A register window with behaviour.
///
/// Offsets are **relative to the peripheral's base** and register-aligned:
/// `off` is the word-aligned register address, and the access's byte lane
/// within that word travels in `width` plus the low bits of the original
/// address, which the bus has already folded into `off`'s low two bits.
/// (Concretely: the bus passes `off` with its low bits intact and the
/// peripheral uses `off & !3` to select a register and `off & 3` to select
/// the lane; [`crate::regfile::RegFile`] does exactly that, and most
/// peripherals are a `RegFile` with a few live registers.)
pub trait Peripheral {
    /// The block's instance name as it appears in the trace: `UART0`,
    /// `SYSTIMER`. Instance, not type — `UART1` shares UART0's register
    /// layout but is a different block in the log.
    fn name(&self) -> &'static str;

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32;

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>);

    /// A scheduled event came due. `id` is whatever this peripheral passed
    /// to [`Scheduler::schedule_at`]; the bus routes it back by the
    /// peripheral index encoded in the id (see [`crate::bus::event_id`]).
    fn on_event(&mut self, _id: EventId, _cx: &mut BusCx<'_>) {}

    /// The register's name at `off`, for the trace. Usually
    /// `self.names.name(off)` against a generated table.
    fn reg_name(&self, _off: u32) -> Option<&'static str> {
        None
    }

    /// Snapshot this peripheral's state. The machine composes the
    /// per-peripheral blobs; the format is the peripheral's own business,
    /// and only it ever reads one back.
    fn save_state(&self) -> Vec<u8>;

    /// Restore from a blob produced by [`save_state`](Self::save_state).
    fn load_state(&mut self, bytes: &[u8]);
}

/// A boxed peripheral, as the bus stores them.
pub type BoxedPeripheral = Box<dyn Peripheral>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_bytes_and_masks() {
        assert_eq!(Width::Byte.bytes(), 1);
        assert_eq!(Width::Half.bytes(), 2);
        assert_eq!(Width::Word.bytes(), 4);
        assert_eq!(Width::Byte.mask(), 0xff);
        assert_eq!(Width::Half.mask(), 0xffff);
        assert_eq!(Width::Word.mask(), 0xffff_ffff);
    }

    #[test]
    fn lane_masks_do_not_reach_past_the_word() {
        assert_eq!(Width::Byte.lane_mask(0), 0x0000_00ff);
        assert_eq!(Width::Byte.lane_mask(3), 0xff00_0000);
        assert_eq!(Width::Half.lane_mask(2), 0xffff_0000);
        assert_eq!(Width::Word.lane_mask(0), 0xffff_ffff);
        // A half-word at lane 3 would run off the end; the mask stops.
        assert_eq!(Width::Half.lane_mask(3), 0xff00_0000);
    }

    #[test]
    fn irq_levels_set_read_and_report_change() {
        let mut irq = IrqLines::new();
        assert!(!irq.any());
        assert!(!irq.take_changed());

        irq.set_level(31, true);
        assert!(irq.level(31));
        assert!(irq.any());
        assert!(irq.take_changed());
        assert!(!irq.take_changed());

        // Setting the same level again is not a change.
        irq.set_level(31, true);
        assert!(!irq.take_changed());

        irq.set_level(31, false);
        assert!(!irq.level(31));
        assert!(irq.take_changed());
    }

    #[test]
    fn irq_reaches_the_high_word() {
        let mut irq = IrqLines::new();
        irq.set_level(127, true);
        assert!(irq.level(127));
        assert_eq!(irq.raw(), [0, 1 << 63]);
    }

    #[test]
    fn irq_out_of_range_is_ignored_not_fatal() {
        let mut irq = IrqLines::new();
        irq.set_level(IRQ_SOURCES, true);
        assert!(!irq.any());
        assert!(!irq.level(IRQ_SOURCES));
    }
}
