//! THROWAWAY (M5 P1b) — never merge.
//!
//! Why the render loop's blocks are as short as they are, counted rather than
//! guessed: for every *execution* of a cached block, what ended it and how
//! many slots it actually ran. Weighted by entries, not by distinct blocks —
//! the question is what the guest spends its time on, not what the decoder
//! produced.
//!
//! The interesting split is **extendable** versus not: a block that ends at
//! an unconditional direct jump (`jal`, `c.j`, `c.jal`) could be continued
//! into its target by a decoder that followed it, and a block that ends at a
//! *not-taken* conditional branch could be continued into its fall-through
//! with a side exit. Both are superblock levers. A taken conditional branch,
//! a `jalr` (a return, or an indirect call) and a refused instruction are not.

extern crate alloc;

use core::sync::atomic::{AtomicU64, Ordering};

/// Why a block execution ended.
pub const KINDS: usize = 10;
pub const NAMES: [&str; KINDS] = [
    "branch not taken (fall-through)",
    "branch taken",
    "jal, rd=x0 (a plain direct jump)",
    "jal, rd!=x0 (a direct call)",
    "jalr (indirect call / return)",
    "c.j / c.jal (direct)",
    "c.beqz / c.bnez not taken",
    "c.beqz / c.bnez taken",
    "c.jr / c.jalr (indirect / return)",
    "no terminator: slot cap, a refused instruction, or a mid-block exit",
];

const MAXLEN: usize = 66;

static COUNT: [AtomicU64; KINDS] = [const { AtomicU64::new(0) }; KINDS];
static SLOTS: [AtomicU64; KINDS] = [const { AtomicU64::new(0) }; KINDS];
static LEN: [AtomicU64; MAXLEN] = [const { AtomicU64::new(0) }; MAXLEN];

/// Classify a block execution by its last-run instruction word.
///
/// `straight` is true when the hart left the block still on the decoder's
/// straight line — i.e. the last instruction did not transfer control.
/// `complete` is true when every slot in the block ran.
pub fn note(word: u32, straight: bool, complete: bool, ran: u32) {
    let kind = if !complete {
        9
    } else if (word & 0b11) != 0b11 {
        let q = word & 0b11;
        let f3 = (word >> 13) & 0b111;
        match (q, f3) {
            (0b01, 0b001) | (0b01, 0b101) => 5,
            (0b01, 0b110) | (0b01, 0b111) => {
                if straight {
                    6
                } else {
                    7
                }
            }
            (0b10, 0b100) => 8,
            _ => 9,
        }
    } else {
        match (word & 0x7f) as u8 {
            0x63 => {
                if straight {
                    0
                } else {
                    1
                }
            }
            0x6f => {
                if (word >> 7) & 0x1f == 0 {
                    2
                } else {
                    3
                }
            }
            0x67 => 4,
            _ => 9,
        }
    };
    COUNT[kind].fetch_add(1, Ordering::Relaxed);
    SLOTS[kind].fetch_add(u64::from(ran), Ordering::Relaxed);
    LEN[(ran as usize).min(MAXLEN - 1)].fetch_add(1, Ordering::Relaxed);
}

/// A report, for the C6 binary to print at exit.
#[must_use]
pub fn report() -> alloc::string::String {
    use alloc::format;
    use alloc::string::String;
    let total: u64 = COUNT.iter().map(|c| c.load(Ordering::Relaxed)).sum();
    let slots: u64 = SLOTS.iter().map(|c| c.load(Ordering::Relaxed)).sum();
    let mut s = String::new();
    s.push_str(&format!(
        "blockstats: {total} block executions, {slots} slots run, mean realised length {:.3}\n",
        slots as f64 / total.max(1) as f64
    ));
    for i in 0..KINDS {
        let c = COUNT[i].load(Ordering::Relaxed);
        let sl = SLOTS[i].load(Ordering::Relaxed);
        if c == 0 {
            continue;
        }
        s.push_str(&format!(
            "  {:>6.2}% of entries  {:>6.2}% of slots  mean {:>5.2}  {}\n",
            100.0 * c as f64 / total.max(1) as f64,
            100.0 * sl as f64 / slots.max(1) as f64,
            sl as f64 / c as f64,
            NAMES[i]
        ));
    }
    s.push_str("  length histogram (slots run : block executions)\n");
    for (n, c) in LEN.iter().enumerate() {
        let v = c.load(Ordering::Relaxed);
        if v == 0 {
            continue;
        }
        s.push_str(&format!("    {n:>3} : {v}\n"));
    }
    s
}
