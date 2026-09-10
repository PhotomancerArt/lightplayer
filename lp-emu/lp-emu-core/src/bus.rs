//! `Bus`: what an instruction stream reads and writes.
//!
//! [`crate::Memory`] is the flat user-mode implementation (code + optional
//! shared + RAM regions, fixed bases); a SoC bus with MMIO-mapped
//! peripherals is another. Reads take `&mut self` because MMIO reads can
//! have side effects (FIFO pops, clear-on-read registers) — `Memory`'s own
//! reads happen to be pure, but the trait can't assume that of every
//! implementor. Errors carry no PC: the owner of the PC (the emulator, or
//! the privileged hart) attaches it at its error boundary, same as
//! [`crate::MemoryError`] does today.

use crate::memory::MemoryError;

/// What an instruction stream reads and writes.
///
/// Method names and return types mirror `Memory`'s inherent methods exactly,
/// so `impl Bus for Memory` is a mechanical forwarding impl and executor
/// bodies need no change beyond the parameter type.
pub trait Bus {
    fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError>;
    fn read_word(&mut self, address: u32) -> Result<i32, MemoryError>;
    fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError>;
    fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError>;
    fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError>;
    fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError>;
    fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError>;
    fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError>;

    /// Read up to three instruction bytes starting at `pc`, for a
    /// variable-length decoder.
    ///
    /// Xtensa instructions are 2 or 3 bytes and start at **any** alignment, so
    /// there is no word to return: the caller gets the bytes and the decoder's
    /// length rule says how many of them are the instruction. `Ok(n)` with
    /// `n < 3` means the mapping ran out — legal, and the decoder decides
    /// whether that truncates a real instruction.
    ///
    /// The default assembles the answer from [`Bus::read_u8`], so an RV32-only
    /// bus implements nothing and behaves correctly if something ever asks. A
    /// bus with a cheaper contiguous path (a flat region table, a cache line)
    /// overrides it. It is **not** a fetch-permission shortcut: an
    /// implementation must apply the same execute permission its
    /// [`Bus::fetch_instruction`] does — and note that the *default* reads
    /// through [`Bus::read_u8`], which is a **data** read. On a bus where data
    /// and fetch differ in permission or in side effects the default is wrong
    /// and the bus must override. Today only `lp-xt-emu`'s `Memory` overrides
    /// it, and only RV32 buses inherit it, where nothing calls it.
    ///
    /// The first byte classifies the address: if it cannot be fetched, that is
    /// the error. Later bytes that fall outside the mapping stop the count
    /// instead of failing.
    #[inline]
    fn fetch_bytes(&mut self, pc: u32, out: &mut [u8; 3]) -> Result<usize, MemoryError> {
        // The first byte must be readable; that classifies the address.
        out[0] = self.read_u8(pc)?;
        let mut got = 1;
        for i in 1..3u32 {
            match self.read_u8(pc.wrapping_add(i)) {
                Ok(b) => {
                    out[i as usize] = b;
                    got += 1;
                }
                Err(_) => break,
            }
        }
        Ok(got)
    }

    /// A hardware watchpoint slot (RISC-V trigger `mcontrol`, Xtensa
    /// `DBREAK`). The privileged layer mirrors its trigger CSRs here; a bus
    /// that honours it returns [`MemoryError::Watchpoint`] *instead of*
    /// performing the matching access. Default: ignored (user-mode
    /// `Memory`, which has no privileged state to trap from).
    #[inline(always)]
    fn set_watchpoint(&mut self, _slot: usize, _wp: Option<Watchpoint>) {}

    /// Tell the bus which instruction is about to issue accesses, and at
    /// what cycle.
    ///
    /// A bus log is worth having because it says *who* and *when*: `cyc=41288
    /// pc=0x42009a1c R4 TIMG0+0x068 rtccalicfg` answers "which status bit is
    /// it spinning on" in one line. Both halves have to arrive per
    /// instruction to be that line — a machine that set them once per
    /// scheduler slice would stamp a million accesses with the same value,
    /// and the unmapped-site dedup (keyed on `(pc, address)`) would collapse
    /// unrelated sites onto one.
    ///
    /// `cycle` is the count *before* this instruction is charged, which is
    /// the only reading that composes: two accesses one instruction apart
    /// differ by exactly that instruction's cost.
    ///
    /// Called before each instruction the privileged stepper executes. The
    /// default is empty and inlines away; only a bus that keeps a trace or a
    /// spin detector implements it.
    #[inline(always)]
    fn set_issuing(&mut self, _pc: u32, _cycle: u64) {}

    /// Side-band after an MMIO-class access: `true` when the bus's
    /// interrupt state may have changed (an MMIO store). Consumed by the
    /// privileged stepper after Store/System-class instructions only; the
    /// default is a constant the optimizer removes.
    #[inline(always)]
    fn take_sideband(&mut self) -> bool {
        false
    }

    /// Did a peripheral ask the machine to take over before the next
    /// instruction runs?
    ///
    /// The side-band above says "interrupt state may have changed", which
    /// the hart can answer by itself. This says "something changed that only
    /// the machine can act on", and the hart's answer is to end the slice.
    ///
    /// It exists for exactly one shape of thing, and the ESP32-C6's cache
    /// MMU is the first of it: a store that changes what an address
    /// *means*. The guest programs an MMU entry and reads through the
    /// window a few instructions later, in the same slice; a machine that
    /// refills the window at the next slice boundary serves it stale bytes.
    /// The C6's second-stage bootloader does exactly that, and read its own
    /// image header as zeros until this existed.
    ///
    /// Checked only after a store, and only when the store was to MMIO, so
    /// a bus that never sets it costs one already-loaded bool per store.
    fn take_yield(&mut self) -> bool {
        false
    }

    /// The CPU interrupt this bus's interrupt matrix asserts *right now* for
    /// the hart that is executing, or `None` for "nothing asserted".
    ///
    /// This is the other half of [`Bus::take_sideband`], and the two are a
    /// pair: when the side-band says an MMIO store may have changed interrupt
    /// state, the privileged stepper **replaces** its pending-interrupt input
    /// with this value and polls, so a store that raises a peripheral line is
    /// delivered before the next instruction retires — and a store that
    /// *lowers* one stops being pending in the same breath.
    ///
    /// Therefore: **a bus that ever returns `true` from `take_sideband` must
    /// implement this method.** A bus that never raises the side-band never
    /// has it called, which is why the default is a constant the optimizer
    /// removes rather than an `unimplemented!()`.
    #[inline(always)]
    fn pending_cpu_interrupt(&self) -> Option<u8> {
        None
    }

    /// Extra cycles this bus's memory system charged for the accesses made
    /// since the last call, and clear the total.
    ///
    /// The bus, not the hart, is where an access's *address* is known: the
    /// hart hands out a `pc` and the executors compute a load's address
    /// inside the instruction, so the only component that sees every address
    /// with its width is the one that serves it. A bus that models a cache
    /// or a peripheral bus (see [`crate::cycle_model::MemoryCost`])
    /// accumulates here and the privileged stepper drains it once per
    /// instruction, adding it to the cycle counter beside the instruction's
    /// own class cost.
    ///
    /// The default is a constant the optimizer removes, which is what keeps
    /// a grade with no memory-cost model exactly as fast, and exactly as
    /// counted, as it was before this existed.
    #[inline(always)]
    fn take_memory_cost(&mut self) -> u32 {
        0
    }

    // ---- what a pre-decoded block cache needs from a bus ------------------

    /// May a fetch be performed **ahead of time** and its result reused,
    /// with nothing observable changing?
    ///
    /// A [`crate::block::BlockCache`] decodes a run of instructions in one
    /// go and then executes them without fetching again. That is only exact
    /// when a fetch has no consequence beyond returning the word:
    ///
    /// - it must **charge nothing** ([`Bus::take_memory_cost`] must stay at
    ///   zero for fetches), because a decode-ahead fetch and the later
    ///   execution would otherwise charge the cycle twice, or not at all;
    /// - it must **trap nothing** — no execute-kind watchpoint may be armed,
    ///   or a block build would take a trap the guest has not reached yet.
    ///
    /// The default is **`false`**: a bus opts in, rather than being opted in
    /// by a trait default it never read. Getting this wrong is a silent
    /// mis-accounting, so the fail-safe direction is "no cache".
    ///
    /// It is read once per slice and again at every block boundary, so a bus
    /// may change its answer whenever it likes.
    #[inline(always)]
    fn fetch_is_pure(&self) -> bool {
        false
    }

    /// `--strict-bus` diagnostics: instructions in `[pc, pc + bytes)` are
    /// about to run from a **cached** block, so the bus will see no fetch for
    /// them.
    ///
    /// A bus that checks "was this code page written since the last
    /// `fence.i`?" does it on the fetch path, and a cached block has no fetch
    /// path — this is where it gets told instead. The default is empty and
    /// inlines away, which is what keeps the checker free on the default run.
    #[inline(always)]
    fn note_cached_execute(&mut self, _pc: u32, _bytes: u32) {}

    /// The guest retired a `fence.i`: every code page written up to here has
    /// been published, and a bus tracking "written but not fenced" clears its
    /// marks.
    ///
    /// The other end of the contract is the firmware's own `fence.i`, emitted
    /// by `lpvm_native::rt_jit::buffer::JitBuffer::from_code` after a JIT
    /// publish. The default is empty.
    #[inline(always)]
    fn note_fence_i(&mut self) {}

    // ---- what a translated core needs to peek at before it runs ----------
    //
    // A translated core (`lp-emu-jit`) executes many guest instructions
    // between two visits to this trait, so the three questions below have to
    // be answerable *without* consuming anything. Each one encodes a
    // correctness constraint that cost the M7 spike real debugging; the doc
    // comments are the record of why.

    /// Is a store side-band or a machine yield pending **right now**? A peek:
    /// nothing is consumed.
    ///
    /// Translated code must not be entered while one is pending, and must
    /// leave immediately after the access that raised one, so the interpreter
    /// observes it at exactly the store it always has — polling point (c) in
    /// this module's docs does not move because a block was translated.
    ///
    /// The default is a constant the optimizer removes.
    #[inline(always)]
    fn sideband_or_yield_pending(&self) -> bool {
        false
    }

    /// Is any **load** watchpoint armed?
    ///
    /// Translated code performs RAM loads the bus never sees, so a load
    /// watchpoint could not fire. It therefore refuses to run at all while
    /// one is armed, rather than running and missing the trap.
    ///
    /// The default is a constant the optimizer removes.
    #[inline(always)]
    fn load_watchpoints_armed(&self) -> bool {
        false
    }

    /// The **store** watchpoints, in the shape translated code can honour.
    ///
    /// Stores are the asymmetric case: esp-hal arms its stack guard for whole
    /// runs, so refusing on any armed store watchpoint would refuse the whole
    /// product run. One range can be honoured inline; more than one is a
    /// refusal.
    ///
    /// The default is [`StoreWatch::None`], which the optimizer folds.
    #[inline(always)]
    fn store_watch(&self) -> StoreWatch {
        StoreWatch::None
    }
}

/// The store watchpoints a translated core has to honour — see
/// [`Bus::store_watch`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StoreWatch {
    /// Nothing armed; stores need no check.
    #[default]
    None,
    /// Exactly one armed `[lo, hi)` range, which translated code can compare
    /// against inline.
    One { lo: u64, hi: u64 },
    /// More than one range: translated code refuses to run.
    Many,
}

/// A hardware watchpoint slot's configuration.
///
/// Shaped after the RISC-V `tdata1`/`tdata2` trigger pair (`mcontrol`); the
/// same shape covers Xtensa's `DBREAK` registers, so the privileged layer
/// for either architecture can drive it without a second type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watchpoint {
    /// NAPOT-encoded compare value as written to `tdata2` (bit pattern), or
    /// an exact address when `napot` is `false`.
    pub address: u32,
    pub napot: bool,
    pub on_store: bool,
    pub on_load: bool,
    pub on_execute: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryAccessKind;

    /// A bus that implements **only** the eight required methods, so
    /// everything it answers past them is the trait's own default. The
    /// mapping is `[base, base + data.len())`; anything else is an invalid
    /// access.
    struct TinyBus {
        base: u32,
        data: alloc::vec::Vec<u8>,
    }

    impl TinyBus {
        fn index(&self, address: u32) -> Option<usize> {
            let off = address.checked_sub(self.base)? as usize;
            (off < self.data.len()).then_some(off)
        }
        fn oob(address: u32, size: usize, kind: MemoryAccessKind) -> MemoryError {
            MemoryError::InvalidAccess {
                address,
                size,
                kind,
            }
        }
    }

    impl Bus for TinyBus {
        fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError> {
            self.read_word(address).map(|w| w as u32)
        }
        fn read_word(&mut self, address: u32) -> Result<i32, MemoryError> {
            let mut v = 0u32;
            for i in 0..4u32 {
                v |= u32::from(self.read_u8(address.wrapping_add(i))?) << (8 * i);
            }
            Ok(v as i32)
        }
        fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError> {
            let lo = u16::from(self.read_u8(address)?);
            let hi = u16::from(self.read_u8(address.wrapping_add(1))?);
            Ok((lo | (hi << 8)) as i16)
        }
        fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError> {
            self.read_u8(address).map(|b| b as i8)
        }
        fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError> {
            match self.index(address) {
                Some(i) => Ok(self.data[i]),
                None => Err(Self::oob(address, 1, MemoryAccessKind::Read)),
            }
        }
        fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError> {
            for i in 0..4u32 {
                self.write_byte(address.wrapping_add(i), (value >> (8 * i)) as i8)?;
            }
            Ok(())
        }
        fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError> {
            self.write_byte(address, value as i8)?;
            self.write_byte(address.wrapping_add(1), (value >> 8) as i8)
        }
        fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError> {
            match self.index(address) {
                Some(i) => {
                    self.data[i] = value as u8;
                    Ok(())
                }
                None => Err(Self::oob(address, 1, MemoryAccessKind::Write)),
            }
        }
    }

    /// The default `fetch_bytes` is exactly three `read_u8` calls that stop at
    /// the first failure — including at the very end of the mapping, where it
    /// returns `Ok(2)` and `Ok(1)` rather than an error.
    #[test]
    fn bus_fetch_bytes_default_matches_read_u8() {
        let base = 0x4000_0000u32;
        let mut bus = TinyBus {
            base,
            data: alloc::vec![0x11, 0x22, 0x33, 0x44, 0x55],
        };

        // Fully inside the mapping.
        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(base, &mut out), Ok(3));
        assert_eq!(out, [0x11, 0x22, 0x33]);

        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(base + 1, &mut out), Ok(3));
        assert_eq!(out, [0x22, 0x33, 0x44]);

        // Two bytes left: the third read fails and stops the count.
        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(base + 3, &mut out), Ok(2));
        assert_eq!(out[..2], [0x44, 0x55]);

        // One byte left.
        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(base + 4, &mut out), Ok(1));
        assert_eq!(out[0], 0x55);

        // The first byte classifies the address: unmapped is the error.
        let mut out = [0u8; 3];
        assert_eq!(
            bus.fetch_bytes(base + 5, &mut out),
            Err(MemoryError::InvalidAccess {
                address: base + 5,
                size: 1,
                kind: MemoryAccessKind::Read,
            })
        );

        // And the bytes it produced are the same bytes three `read_u8` calls
        // produce, address for address, everywhere it returned `Ok`.
        for pc in base..base + 5 {
            let mut out = [0u8; 3];
            let got = bus.fetch_bytes(pc, &mut out).expect("mapped");
            let mut want = [0u8; 3];
            let mut want_got = 0usize;
            for i in 0..3u32 {
                match bus.read_u8(pc.wrapping_add(i)) {
                    Ok(b) => {
                        want[i as usize] = b;
                        want_got += 1;
                    }
                    Err(_) => break,
                }
            }
            assert_eq!((got, &out[..got]), (want_got, &want[..want_got]));
        }
    }
}
