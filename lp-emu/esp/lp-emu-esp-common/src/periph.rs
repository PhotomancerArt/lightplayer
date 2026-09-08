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
use core::any::Any;
use lp_emu_core::sched::{Cycles, EventId, Scheduler};

use crate::host::HostSinks;
use crate::pins::Fabric;
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

/// What the chip's boot strap says when a reset lands: run the app, or
/// stay in the ROM's download console.
///
/// On the C6 the strap is GPIO9 at the reset edge, and the USB-Serial-JTAG
/// block can pull it from the host side: the download dance (DTR high, then
/// an RTS falling edge) resets the chip into the ROM console, the plain
/// dance (an RTS falling edge with DTR never high) resets it into the app.
/// The RWDT always resets into the app. Until M7 performs resets, the
/// machine only reports the strap in [`crate::periph::MachineRequest::Reset`]'s
/// outcome message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strap {
    /// Boot the application (GPIO9 high).
    App,
    /// Stay in the mask ROM's download console (GPIO9 low).
    Download,
}

impl core::fmt::Display for Strap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Strap::App => "app",
            Strap::Download => "download",
        })
    }
}

/// Something a peripheral needs the *machine* to do, because it cannot do
/// it itself: a reset. The bus holds at most one (the first wins) and the
/// machine takes it at the next slice boundary.
///
/// The producers are a watchdog whose stage action is a reset and the
/// USB-Serial-JTAG block's `chip_rst` path (a host's DTR/RTS dance). The
/// peripheral cannot reset the hart — it does not see the hart — and a
/// reset the emulator cannot yet perform (M7 owns the boot chain) is
/// reported as a run outcome instead, which is also the more useful answer:
/// "the RWDT expired at cycle N" is what a bring-up wants to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineRequest {
    Reset {
        /// Who asked: `"LP_WDT stage 0"`, `"USB_DEVICE chip_rst (serial)"`.
        source: &'static str,
        at: Cycles,
        /// What the chip would boot into. See [`Strap`].
        strap: Strap,
    },
}

/// How much of a register's behaviour is backed by evidence.
///
/// The validation system grades whole classes (`lp-emu-validate`'s
/// `Modeled < Documented < Measured`); this is the same ladder applied to
/// one register of one block, so a run can refuse to *touch* a register
/// whose behaviour is a guess. Every register is `Modeled` unless its
/// block says otherwise — a default that is honest about the accept
/// tables, where nothing was measured.
///
/// - `Modeled`: the behaviour is our reading of the PAC and the drivers,
///   and nothing on silicon has confirmed it.
/// - `Documented`: the behaviour is what a document states (the PAC's
///   description, a USB specification constant), unmeasured here.
/// - `Measured`: a committed silicon transcript shows the guest's
///   observable behaviour at this register agreeing with the model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegGrade {
    #[default]
    Modeled,
    Documented,
    Measured,
}

impl RegGrade {
    pub const fn as_str(self) -> &'static str {
        match self {
            RegGrade::Modeled => "modeled",
            RegGrade::Documented => "documented",
            RegGrade::Measured => "measured",
        }
    }

    /// `modeled` | `documented` | `measured`, as the CLI spells them.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "modeled" => Some(RegGrade::Modeled),
            "documented" => Some(RegGrade::Documented),
            "measured" => Some(RegGrade::Measured),
            _ => None,
        }
    }
}

impl core::fmt::Display for RegGrade {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A block's per-register grade table: register offset → [`RegGrade`],
/// `Modeled` for anything not listed.
///
/// Kept as a sorted list rather than a map: a block has a few dozen
/// registers and a handful of entries above `Modeled`, and a reviewer reads
/// the table in the block's file header — the same rows, in the same order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegGrades {
    /// Sorted by offset, one entry per offset.
    entries: Vec<(u32, RegGrade)>,
}

impl RegGrades {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Grade the register at `off` (word-aligned). A later call for the
    /// same offset replaces the earlier one.
    pub fn with_grade(mut self, off: u32, grade: RegGrade) -> Self {
        let off = off & !3;
        match self.entries.binary_search_by_key(&off, |(o, _)| *o) {
            Ok(i) => self.entries[i].1 = grade,
            Err(i) => self.entries.insert(i, (off, grade)),
        }
        self
    }

    /// The grade at `off`; `Modeled` when unlisted.
    pub fn grade(&self, off: u32) -> RegGrade {
        let off = off & !3;
        self.entries
            .binary_search_by_key(&off, |(o, _)| *o)
            .map(|i| self.entries[i].1)
            .unwrap_or_default()
    }

    /// Every listed `(offset, grade)`, ascending by offset.
    pub fn entries(&self) -> &[(u32, RegGrade)] {
        &self.entries
    }
}

/// A `Peripheral`'s view of the machine for the duration of one access.
///
/// Deliberately small: cycles, the PC that issued the access, the hart index
/// (PD6 — one hart today, the field is what makes a per-core register block
/// possible without reshaping this trait), the scheduler, the interrupt
/// source lines, the bus trace, the host byte streams, the chip's interrupt
/// matrix, and a slot for a [`MachineRequest`].
///
/// The matrix is here for one reason (the C6 plan's DD22): the register
/// blocks that *configure* it (`INTERRUPT_CORE0`, `PLIC_MX`) are ordinary
/// peripherals on the decode table, and the matrix is the single copy of
/// that configuration. Those peripherals are register **views** that write
/// into it through this handle and read back from it, so there is one state
/// and nothing to keep in sync. Every other peripheral ignores the field.
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
    /// The chip's interrupt matrix. Downcast with
    /// [`CpuIntMatrix::as_any_mut`] to the chip's concrete type.
    pub matrix: &'a mut dyn CpuIntMatrix,
    /// See [`MachineRequest`]. Set through [`BusCx::request`].
    pub request: &'a mut Option<MachineRequest>,
    /// The chip's signal fabric: where an output signal goes (plan DD34 e).
    /// The same seam as `matrix`, for pads: the GPIO block is a routing
    /// **view** that writes into it, an output peripheral drives its signal
    /// into it, and neither has to see the other. See [`crate::pins`].
    pub pins: &'a mut Fabric,
}

impl BusCx<'_> {
    /// Ask the machine for something the peripheral cannot do itself. The
    /// first request in a slice wins; a second is logged and dropped, since
    /// the machine stops at the first anyway.
    pub fn request(&mut self, request: MachineRequest) {
        if let Some(existing) = *self.request {
            log::warn!("BusCx: {request:?} dropped, the bus already holds {existing:?}");
            return;
        }
        *self.request = Some(request);
    }
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

/// Turns chip-wide interrupt **source** levels into "the CPU interrupt this
/// hart should take right now", or `None`.
///
/// The half of the interrupt path that is chip-specific: which source is
/// routed to which CPU interrupt, which are enabled, and what the priority
/// threshold is are all PLIC_MX / INTERRUPT_CORE0 questions, and this crate
/// holds no chip numbers. [`crate::bus::SocBus`] holds one of these and
/// answers `Bus::pending_cpu_interrupt` from it, which is what lets an MMIO
/// store that raises a line be delivered before the next instruction
/// retires.
///
/// It is asked on every side-band consumption, so it must be cheap and it
/// must be a **pure function of the levels and its own configuration** — no
/// scheduling, no logging per call.
///
/// Its configuration arrives as MMIO writes to chip-specific register
/// blocks, which reach it through [`BusCx::matrix`] and the two `as_any`
/// accessors: the chip's register-view peripherals downcast to the chip's
/// concrete matrix type. This crate never learns what that type is.
pub trait CpuIntMatrix: Send + 'static {
    /// The highest-priority CPU interrupt asserted for `hart`, or `None`.
    fn cpu_interrupt(&self, hart: usize, irq: &IrqLines) -> Option<u8>;

    /// The downcast seam for the chip's register views.
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Snapshot the matrix's configuration. Defaults to "no state", which is
    /// right for a matrix that is a pure routing table.
    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    fn load_state(&mut self, _bytes: &[u8]) {}
}

/// The default matrix: nothing is ever asserted.
///
/// What a bus has before a chip crate installs its own. A stub that returns
/// `None` is honest about having no routing where a stub that guessed would
/// not be.
#[derive(Copy, Clone, Debug, Default)]
pub struct NoCpuInterrupts;

impl CpuIntMatrix for NoCpuInterrupts {
    fn cpu_interrupt(&self, _hart: usize, _irq: &IrqLines) -> Option<u8> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// A stand-alone context for driving a [`Peripheral`] with no bus: the
/// test harness. Holds everything a [`BusCx`] borrows, so a unit test can
/// write a register, run the events that came due, and read the source
/// levels back, without building a machine.
pub struct Sandbox {
    pub now: Cycles,
    pub pc: u32,
    pub hart: usize,
    pub sched: Scheduler,
    pub irq: IrqLines,
    pub trace: Trace,
    pub host: HostSinks,
    pub matrix: Box<dyn CpuIntMatrix>,
    pub request: Option<MachineRequest>,
    pub pins: Fabric,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            now: 0,
            pc: 0,
            hart: 0,
            sched: Scheduler::new(),
            irq: IrqLines::new(),
            trace: Trace::disabled(),
            host: HostSinks::new(),
            matrix: Box::new(NoCpuInterrupts),
            request: None,
            pins: Fabric::new(),
        }
    }

    /// A sandbox whose matrix is the chip's.
    pub fn with_matrix(mut self, matrix: Box<dyn CpuIntMatrix>) -> Self {
        self.matrix = matrix;
        self
    }

    pub fn cx(&mut self) -> BusCx<'_> {
        BusCx {
            now: self.now,
            pc: self.pc,
            hart: self.hart,
            sched: &mut self.sched,
            irq: &mut self.irq,
            trace: &mut self.trace,
            host: &mut self.host,
            matrix: &mut *self.matrix,
            request: &mut self.request,
            pins: &mut self.pins,
        }
    }

    /// Read a register through the peripheral, at `self.now`.
    pub fn read(&mut self, p: &mut dyn Peripheral, off: u32) -> u32 {
        p.read(off, Width::Word, &mut self.cx())
    }

    /// Write a register through the peripheral, at `self.now`.
    pub fn write(&mut self, p: &mut dyn Peripheral, off: u32, value: u32) {
        p.write(off, Width::Word, value, &mut self.cx());
    }

    /// Move time to `now` and deliver every event of `p` that came due.
    ///
    /// The sandbox holds one peripheral's events, so every popped id is
    /// handed to `p` whatever peripheral index it encodes.
    pub fn run_to(&mut self, p: &mut dyn Peripheral, now: Cycles) {
        self.now = now;
        while let Some(id) = self.sched.pop_due(now) {
            p.on_event(id, &mut self.cx());
        }
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

    /// The bus has given this peripheral index `index`. Called once, from
    /// [`crate::bus::SocBus::add_peripheral`]; a peripheral that schedules
    /// events keeps it, because [`crate::bus::event_id`] packs it into every
    /// event tag and the bus routes the event back by it.
    fn attached(&mut self, _index: usize) {}

    /// The machine is about to run: guest time is zero and the schedule is
    /// empty. Called once per peripheral from
    /// [`crate::bus::SocBus::start_peripherals`], after every block is
    /// attached, with a full [`BusCx`] — the one place a peripheral can
    /// schedule something *before* the guest touches it. A UART polling a
    /// host source for bytes that may arrive before the guest has configured
    /// the block is the case; nothing else needs it and the default is a
    /// no-op.
    fn started(&mut self, _cx: &mut BusCx<'_>) {}

    /// A scheduled event came due. `id` is whatever this peripheral passed
    /// to [`Scheduler::schedule_at`]; the bus routes it back by the
    /// peripheral index encoded in the id (see [`crate::bus::event_id`]).
    fn on_event(&mut self, _id: EventId, _cx: &mut BusCx<'_>) {}

    /// The register's name at `off`, for the trace. Usually
    /// `self.names.name(off)` against a generated table.
    fn reg_name(&self, _off: u32) -> Option<&'static str> {
        None
    }

    /// `true` when reading the register at `off` has **no side effect** and
    /// its value can change only through a write or a scheduled event.
    ///
    /// Two claims, and a register needs both:
    ///
    /// 1. **The read changes nothing.** A FIFO whose read pops, a
    ///    clear-on-read status word, a register whose read arms a hardware
    ///    sequence: none of those qualify.
    /// 2. **The value is not a function of `cx.now`.** A free-running
    ///    counter fails this even though reading it is harmless — a guest
    ///    spinning on one is a *delay* loop, and a delay loop is not a fixed
    ///    point of machine state. `SYSTIMER` is the case: deliberately not
    ///    pure.
    ///
    /// Together they say a run of reads of this register, with nothing else
    /// happening, is a fixed point except for time — which is what
    /// [`lp_emu_core::Bus::take_pure_read`] reports and what lets the
    /// privileged stepper skip whole poll-loop iterations. Getting it wrong
    /// is a *correctness* bug, not a performance one, so the default is
    /// `false` for every register of every block and a block opts in
    /// register by register.
    ///
    /// `off` is the byte offset the bus passes to [`read`](Self::read), low
    /// bits intact; an implementation selects the register with `off & !3`.
    fn pure_read(&self, _off: u32) -> bool {
        false
    }

    /// How much evidence backs the register at `off`. See [`RegGrade`].
    ///
    /// `None` — the default — means this block **publishes no grade table**,
    /// and that is not the same as `Some(Modeled)`. A block with a table has
    /// been read register by register against a document, a driver and a
    /// transcript, and says where each one stands; a block without one has
    /// not been asked the question at all. `--strict-grade`
    /// ([`crate::bus::SocBus::set_strict_grade`]) is a claim about the
    /// registers somebody graded, so it applies to the first kind and passes
    /// over the second — otherwise `documented` would stop at the first MMIO
    /// access of any boot, on an accept table nobody ever said anything
    /// about, and the level would mean "did we finish the chip" rather than
    /// "is this register's behaviour backed by evidence".
    ///
    /// The report says which blocks were in scope, so an ungraded block is
    /// visible as an unanswered question rather than as a pass.
    ///
    /// The contract for an override: a block that publishes a table answers
    /// `Some` for **every** offset in its window — `RegGrades::grade` returns
    /// `Modeled` for anything unlisted, which is the right answer for a
    /// register the table's author considered and did not raise. Answering
    /// `None` for some offsets and `Some` for others would make the scope
    /// depend on which register a boot happened to touch first.
    fn reg_grade(&self, _off: u32) -> Option<RegGrade> {
        None
    }

    /// The downcast seam for a block the **machine** drives from outside the
    /// guest, through [`crate::bus::SocBus::with_peripheral`].
    ///
    /// Today there is one: `USB_DEVICE`, whose host transitions (attach,
    /// detach, open, close, the DTR/RTS lines) come from the control channel
    /// plan PD8 names, not from a guest store. The default is `None` — a
    /// block nothing outside the bus drives needs no downcast, and the trait
    /// stays a register interface for every other peripheral. The `'static`
    /// bound an `Any` would impose on `Self` is not on the trait: only the
    /// override that returns `Some(self)` pays it.
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        None
    }

    /// Snapshot this peripheral's state. The machine composes the
    /// per-peripheral blobs; the format is the peripheral's own business,
    /// and only it ever reads one back.
    fn save_state(&self) -> Vec<u8>;

    /// Restore from a blob produced by [`save_state`](Self::save_state).
    fn load_state(&mut self, bytes: &[u8]);

    /// The downcast seam for a machine that wants to *observe* a peripheral
    /// it built — the matrix's `as_any` precedent (plan DD22), read-only.
    /// `None` by default: most blocks have nothing to show beyond their
    /// registers, and a peripheral that does (the RMT's pulse and word logs)
    /// opts in with `Some(self)`. Nothing on the guest side can reach it.
    fn as_any(&self) -> Option<&dyn Any> {
        None
    }
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
