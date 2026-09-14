//! The Xtensa record/replay shape, and the two things every host of an
//! emitted module marshals: the architectural state through the exchange
//! area, and an entry's record for another engine.
//!
//! `lp-emu-jit`'s record became parameterised over the register count and the
//! first architectural number in XD6's pass, so the entry record, the outcome,
//! the memory-granule diff and the first-difference comparison are all the
//! ABI's and none of them is rewritten here. What Xtensa adds:
//!
//! - the file is **64 registers starting at 0**, not 31 starting at 1. There
//!   is no hardwired-zero register to skip, and the file recorded is the
//!   *physical* `AR[0..64]` rather than the window — a record of "a3" would
//!   mean different registers in two entries with different `WindowBase`;
//! - the window, loop and shift state ([`Window`]) is architectural in a way
//!   RV32 has no equivalent of. It is recorded beside the file because a
//!   replay that seeds `AR` and not `WindowBase` has not seeded the machine;
//! - [`marshal`]: the exchange area's head — the file and the eight extra
//!   words — written from a hart and read back into one. **One copy**, used
//!   by the classic's driver at every entry, exit and escape and by the
//!   round-trip harness, so the emitter's folded offsets and every host read
//!   the same bytes;
//! - [`case`]: an entry's record in the JSON `scripts/emu/jit-engine-check.mjs`
//!   replays — the same file the RV32 round-trip writes, generalised with a
//!   `layout` block (how many words the state is, where the protocol's fields
//!   sit) and a list of entries so one file can carry a whole recording.

use alloc::string::String;
use alloc::vec::Vec;

use lp_emu_jit::replay;

/// How many registers an Xtensa record carries: the physical `AR` file.
pub const RECORDED_REGS: usize = 64;

/// The architectural number of the first recorded register.
///
/// Zero: `AR[0]` is an ordinary register. RV32 records from 1 because `x0` is
/// hardwired and recording it would be recording a constant.
pub const RECORDED_FIRST: u8 = 0;

/// `AR[0]`..`AR[63]`, in physical order.
pub type Regs = replay::Regs<RECORDED_REGS, RECORDED_FIRST>;

/// What one entry into translated code produced.
pub type EntryOutcome = replay::EntryOutcome<RECORDED_REGS, RECORDED_FIRST>;

/// One entry into translated code, recorded.
pub type EntryRecord = replay::EntryRecord<RECORDED_REGS, RECORDED_FIRST>;

/// A whole run's worth of entries, in the order they happened.
pub type ReplayRecord = replay::ReplayRecord<RECORDED_REGS, RECORDED_FIRST>;

/// The architectural state past the register file that a stay can move.
///
/// The same words, in the same order, as the exchange area's extras
/// ([`crate::extra`]) minus the dirty mask, which is the emitter's bookkeeping
/// and not the machine's state. A replay seeds these and compares them.
///
/// `PS` is carried **whole** rather than as `CALLINC` alone: the exchange area
/// needs only the two bits a stay rotates by, but a record is an oracle and an
/// `INTLEVEL` or `EXCM` that moved inside a stay is exactly the kind of
/// divergence it exists to catch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// `WindowBase`, in units of four registers.
    pub window_base: u8,
    /// `WindowStart`, one bit per four-register frame.
    pub window_start: u16,
    /// `SAR`.
    pub sar: u32,
    /// `LBEG`.
    pub lbeg: u32,
    /// `LEND`.
    pub lend: u32,
    /// `LCOUNT`.
    pub lcount: u32,
    /// `PS`, whole.
    pub ps: u32,
}

/// The first field of [`Window`] that differs, named, for a report that says
/// *what* diverged rather than that something did.
#[must_use]
pub fn first_window_difference(mine: &Window, theirs: &Window) -> Option<(&'static str, u32, u32)> {
    let pairs: [(&'static str, u32, u32); 7] = [
        (
            "WindowBase",
            u32::from(mine.window_base),
            u32::from(theirs.window_base),
        ),
        (
            "WindowStart",
            u32::from(mine.window_start),
            u32::from(theirs.window_start),
        ),
        ("SAR", mine.sar, theirs.sar),
        ("LBEG", mine.lbeg, theirs.lbeg),
        ("LEND", mine.lend, theirs.lend),
        ("LCOUNT", mine.lcount, theirs.lcount),
        ("PS", mine.ps, theirs.ps),
    ];
    pairs
        .into_iter()
        .find(|(_, a, b)| a != b)
        .map(|(name, a, b)| (name, a, b))
}

/// The exchange area's head, as the emitted module reads and writes it.
pub mod marshal {
    use lp_xt_emu::cpu::{Cpu, NUM_AR};

    use crate::{LAYOUT, extra};

    /// How many `i32` words the head is: the file and the eight extras.
    pub const WORDS: usize = NUM_AR + crate::EXTRA_WORDS as usize;

    fn put(x: &mut [u8], at: u64, v: u32) {
        x[at as usize..][..4].copy_from_slice(&v.to_le_bytes());
    }

    fn get(x: &[u8], at: u64) -> u32 {
        u32::from_le_bytes(x[at as usize..][..4].try_into().expect("four bytes"))
    }

    /// The hart's file, window, shift and loop state → the exchange area.
    ///
    /// The dirty mask is left alone: it is the emitter's, written at an exit.
    pub fn write_state(x: &mut [u8], cpu: &Cpu, lbeg: u32, lend: u32, lcount: u32) {
        for (i, &r) in cpu.ar.iter().enumerate() {
            put(x, LAYOUT.reg(i as u32), r);
        }
        put(x, extra(extra::WINDOW_BASE), u32::from(cpu.window_base));
        put(x, extra(extra::WINDOW_START), u32::from(cpu.window_start));
        put(x, extra(extra::SAR), cpu.sar);
        put(x, extra(extra::LBEG), lbeg);
        put(x, extra(extra::LEND), lend);
        put(x, extra(extra::LCOUNT), lcount);
        put(x, extra(extra::PS_CALLINC), u32::from(cpu.ps_callinc & 3));
    }

    /// The exchange area → the hart's file, window and shift state; the loop
    /// registers come back as `(LBEG, LEND, LCOUNT)` for the caller's `SrFile`.
    pub fn read_state(x: &[u8], cpu: &mut Cpu) -> (u32, u32, u32) {
        for (i, r) in cpu.ar.iter_mut().enumerate() {
            *r = get(x, LAYOUT.reg(i as u32));
        }
        cpu.window_base = (get(x, extra(extra::WINDOW_BASE)) & 0xF) as u8;
        cpu.window_start = get(x, extra(extra::WINDOW_START)) as u16;
        cpu.sar = get(x, extra(extra::SAR));
        cpu.ps_callinc = (get(x, extra(extra::PS_CALLINC)) & 3) as u8;
        (
            get(x, extra(extra::LBEG)),
            get(x, extra(extra::LEND)),
            get(x, extra(extra::LCOUNT)),
        )
    }

    /// The words a polling point can observe from inside a stay, exchange
    /// area → the hart: `PS.CALLINC` (interrupt entry saves `PS` whole) and
    /// the loop registers (the handler's `save_context` reads them). The
    /// emitter keeps exactly these current in the exchange area at every
    /// point they change.
    pub fn read_polled_state(x: &[u8], cpu: &mut Cpu) -> (u32, u32, u32) {
        cpu.ps_callinc = (get(x, extra(extra::PS_CALLINC)) & 3) as u8;
        (
            get(x, extra(extra::LBEG)),
            get(x, extra(extra::LEND)),
            get(x, extra(extra::LCOUNT)),
        )
    }

    /// The dirty mask the last exit wrote.
    #[must_use]
    pub fn dirty(x: &[u8]) -> u32 {
        get(x, extra(extra::DIRTY))
    }

    /// The whole head as words, for a record.
    #[must_use]
    pub fn words(x: &[u8]) -> alloc::vec::Vec<i32> {
        (0..WORDS as u32)
            .map(|i| get(x, LAYOUT.reg(i)) as i32)
            .collect()
    }
}

/// An entry's record, in the JSON `scripts/emu/jit-engine-check.mjs` replays.
pub mod case {
    use alloc::string::String;
    use alloc::vec::Vec;

    use super::marshal::WORDS;
    use crate::LAYOUT;

    /// The 64-bit FNV-1a the JS side computes, so the two agree by
    /// construction. Fed incrementally so a hash over several ranges is the
    /// hash of their concatenation.
    #[derive(Clone, Copy, Debug)]
    pub struct Fnv(u64);

    impl Default for Fnv {
        fn default() -> Self {
            Self(0xcbf2_9ce4_8422_2325)
        }
    }

    impl Fnv {
        pub fn update(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 ^= u64::from(b);
                self.0 = self.0.wrapping_mul(0x100_0000_01b3);
            }
        }

        #[must_use]
        pub fn finish(self) -> u64 {
            self.0
        }
    }

    /// Plain base64, so the case file needs no dependency on either side.
    #[must_use]
    pub fn base64(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            out.push(A[(n >> 18) as usize & 63] as char);
            out.push(A[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                A[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                A[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    /// The granules of `[before, after)` that differ, as `(offset + base,
    /// bytes)`, at [`lp_emu_jit::replay::GRANULE_BYTES`] granularity.
    #[must_use]
    pub fn granules(before: &[u8], after: &[u8], base: u32) -> Vec<(u32, Vec<u8>)> {
        let g = lp_emu_jit::replay::GRANULE_BYTES;
        before
            .chunks(g)
            .zip(after.chunks(g))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (_, b))| (base + (i * g) as u32, b.to_vec()))
            .collect()
    }

    /// One import call's answer, in call order.
    #[derive(Clone, Debug)]
    pub enum Call {
        Load(i64),
        Store(i64),
        Poll(i64),
        /// The escape hatch: what the interpreter left — the pc, the
        /// counters, the status, the exchange head's [`WORDS`], and the
        /// guest memory it wrote.
        Step {
            pc: u32,
            cycle: u64,
            instret: u64,
            status: u32,
            words: Vec<i32>,
            mem: Vec<(u32, Vec<u8>)>,
        },
    }

    /// One entry into translated code.
    #[derive(Clone, Debug)]
    pub struct Entry {
        pub entry: u32,
        pub cycle: u64,
        pub instret: u64,
        pub end: u64,
        pub watch_lo: u64,
        pub watch_hi: u64,
        /// The exchange head as the entry was handed it.
        pub words_in: Vec<i32>,
        /// Guest memory the interpreter changed since the previous entry.
        pub delta: Vec<(u32, Vec<u8>)>,
        pub calls: Vec<Call>,
        pub exit_pc: u32,
        pub flags: i32,
        pub cycle_out: u64,
        pub instret_out: u64,
        /// The exchange head as the entry left it.
        pub words_out: Vec<i32>,
    }

    fn granule_json(g: &[(u32, Vec<u8>)]) -> String {
        let parts: Vec<String> = g
            .iter()
            .map(|(at, b)| alloc::format!(r#"{{"at":{at},"b":"{}"}}"#, base64(b)))
            .collect();
        parts.join(",")
    }

    fn words_json(w: &[i32]) -> String {
        let parts: Vec<String> = w.iter().map(|v| alloc::format!("{v}")).collect();
        parts.join(",")
    }

    fn call_json(c: &Call) -> String {
        match c {
            Call::Load(v) => alloc::format!(r#"{{"kind":"load","ret":"{v}"}}"#),
            Call::Store(v) => alloc::format!(r#"{{"kind":"store","ret":"{v}"}}"#),
            Call::Poll(v) => alloc::format!(r#"{{"kind":"poll","ret":"{v}"}}"#),
            Call::Step {
                pc,
                cycle,
                instret,
                status,
                words,
                mem,
            } => alloc::format!(
                r#"{{"kind":"step","pc":{pc},"cycle":"{cycle}","instret":"{instret}","status":{status},"regs":[{}],"mem":[{}]}}"#,
                words_json(words),
                granule_json(mem)
            ),
        }
    }

    /// The whole case file: the layout block, the initial memory as ranges
    /// of the imported memory, every entry, and the final memory hash.
    ///
    /// `module` names the module file beside the JSON; `ranges` are
    /// `(offset, bytes)` in the imported memory as the first entry found
    /// them, hashed in order; `final_ranges` the same offsets as the last
    /// entry left them.
    #[must_use]
    pub fn json(
        name: &str,
        pages: u64,
        exchange: u32,
        module: &str,
        ranges: &[(u32, &[u8])],
        entries: &[Entry],
        final_ranges: &[(u32, &[u8])],
    ) -> String {
        let mut initial = Fnv::default();
        for (_, b) in ranges {
            initial.update(b);
        }
        let mut last = Fnv::default();
        for (_, b) in final_ranges {
            last.update(b);
        }
        let ranges_json: Vec<String> = ranges
            .iter()
            .map(|(at, b)| alloc::format!(r#"{{"at":{at},"b":"{}"}}"#, base64(b)))
            .collect();
        let entries_json: Vec<String> = entries
            .iter()
            .map(|e| {
                let calls: Vec<String> = e.calls.iter().map(call_json).collect();
                alloc::format!(
                    r#"{{"entry":{},"cycle":"{}","instret":"{}","end":"{}","watchLo":"{}","watchHi":"{}","regsIn":[{}],"delta":[{}],"calls":[{}],"expect":{{"pc":{},"flags":{},"cycle":"{}","instret":"{}","regs":[{}]}}}}"#,
                    e.entry,
                    e.cycle,
                    e.instret,
                    e.end,
                    e.watch_lo,
                    e.watch_hi,
                    words_json(&e.words_in),
                    granule_json(&e.delta),
                    calls.join(","),
                    e.exit_pc,
                    e.flags,
                    e.cycle_out,
                    e.instret_out,
                    words_json(&e.words_out),
                )
            })
            .collect();
        alloc::format!(
            r#"{{"case":"{name}","pages":{pages},"exchange":{exchange},"module":"{module}",
"layout":{{"words":{WORDS},"compareFrom":0,"cycle":{},"instret":{},"flags":{},"status":{}}},
"memoryFnv":"{}","ranges":[{}],
"entries":[{}],
"expect":{{"memoryFnv":"{}"}}}}"#,
            LAYOUT.cycle(),
            LAYOUT.instret(),
            LAYOUT.flags(),
            LAYOUT.status(),
            initial.finish(),
            ranges_json.join(","),
            entries_json.join(",\n"),
            last.finish(),
        )
    }
}

/// Kept so the `String`/`Vec` imports above are used by the module's own
/// signature surface; the JSON writer is in [`case`].
#[allow(dead_code)]
fn _uses(_: String, _: Vec<u8>) {}
