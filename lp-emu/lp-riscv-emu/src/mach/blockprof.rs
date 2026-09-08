//! THROWAWAY (M5 P1): per-instruction counters for the block-cache
//! opportunity questions. Never merge.
//!
//! Answers, for one run:
//!  * M1 — retired instructions per 4 KiB page (post-processed into regions);
//!  * M3 — basic-block length under rule (a) "stores terminate" and rule (b)
//!    "stores do not";
//!  * M4 — how often each block start is entered;
//!  * M7 — the share of retired instructions in rule-(b) blocks with no
//!    `Load` and no `Store` slot (MD10's `has_mem` bypass).
//!
//! A block ends at a terminator **or at a slice boundary**, because the
//! machine re-enters the hart at an arbitrary pc there and a real cache would
//! have to look the block up again. Stores' target pages (SMC pressure) are
//! recorded by the bus, not here.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Where a block may end.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Term {
    /// Not a terminator under either rule.
    None,
    /// A plain store: terminates under rule (a) only.
    Store,
    /// Control transfer, `SYSTEM`, atomic or fence: terminates under both.
    Both,
}

const HIST: usize = 257;

pub struct BlockProf {
    pub instructions: u64,
    pub exec_pages: BTreeMap<u32, u64>,
    pub starts_a: BTreeMap<u32, u64>,
    pub starts_b: BTreeMap<u32, u64>,
    pub len_a: Vec<u64>,
    pub len_b: Vec<u64>,
    pub instr_nomem_b: u64,
    pub blocks_nomem_b: u64,
    pub loads: u64,
    pub stores: u64,
    pub amo: u64,
    pub system: u64,
    pub fence: u64,
    pub control: u64,
    pub slice_entries: u64,
    pub restarts: u64,

    cur_a: u32,
    cur_b: u32,
    live_a: bool,
    live_b: bool,
    b_has_mem: bool,
}

impl Default for BlockProf {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockProf {
    #[must_use]
    pub fn new() -> Self {
        Self {
            instructions: 0,
            exec_pages: BTreeMap::new(),
            starts_a: BTreeMap::new(),
            starts_b: BTreeMap::new(),
            len_a: alloc::vec![0; HIST],
            len_b: alloc::vec![0; HIST],
            instr_nomem_b: 0,
            blocks_nomem_b: 0,
            loads: 0,
            stores: 0,
            amo: 0,
            system: 0,
            fence: 0,
            control: 0,
            slice_entries: 0,
            restarts: 0,
            cur_a: 0,
            cur_b: 0,
            live_a: false,
            live_b: false,
            b_has_mem: false,
        }
    }

    fn close_a(&mut self) {
        if self.cur_a > 0 {
            let i = (self.cur_a as usize).min(HIST - 1);
            self.len_a[i] += 1;
            self.cur_a = 0;
        }
        self.live_a = false;
    }

    fn close_b(&mut self) {
        if self.cur_b > 0 {
            let i = (self.cur_b as usize).min(HIST - 1);
            self.len_b[i] += 1;
            if !self.b_has_mem {
                self.instr_nomem_b += u64::from(self.cur_b);
                self.blocks_nomem_b += 1;
            }
            self.cur_b = 0;
        }
        self.b_has_mem = false;
        self.live_b = false;
    }

    /// The machine re-entered the hart at an arbitrary pc (slice boundary,
    /// trap, `wfi` skip). Both blocks end here.
    pub fn restart(&mut self, slice: bool) {
        if slice {
            self.slice_entries += 1;
        } else {
            self.restarts += 1;
        }
        self.close_a();
        self.close_b();
    }

    /// One retired instruction at `pc`.
    pub fn retire(&mut self, pc: u32, term: Term, mem: bool) {
        self.instructions += 1;
        *self.exec_pages.entry(pc >> 12).or_insert(0) += 1;
        if !self.live_a {
            *self.starts_a.entry(pc).or_insert(0) += 1;
            self.live_a = true;
        }
        if !self.live_b {
            *self.starts_b.entry(pc).or_insert(0) += 1;
            self.live_b = true;
        }
        self.cur_a += 1;
        self.cur_b += 1;
        if mem {
            self.b_has_mem = true;
        }
        match term {
            Term::None => {}
            Term::Store => self.close_a(),
            Term::Both => {
                self.close_a();
                self.close_b();
            }
        }
    }

    /// Flush the open blocks so the histograms are complete.
    pub fn finish(&mut self) {
        self.close_a();
        self.close_b();
    }
}
