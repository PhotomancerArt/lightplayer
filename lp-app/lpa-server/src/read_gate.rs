//! The ProjectRead memory gate: two numbers, set per chip by the embedder.
//!
//! A read on a device runs through infallible allocations, and an allocation
//! that fails there aborts and **resets** the board
//! (`docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`).
//! The gate refuses a read the heap cannot afford with a structured terminal
//! error instead (ADR `2026-08-28-project-reads-bounded-streamed-refusable`).
//!
//! It asks two questions, because a read needs two things:
//!
//! - **enough memory in total** ([`ReadGate::min_free_bytes`]) — a read's
//!   working set is a few hundred small allocations, 9–25 KB on the C6, that
//!   are all alive at once until its reply is written;
//! - **one block big enough for its largest single ask**
//!   ([`ReadGate::min_largest_block_bytes`]) — the biggest single allocation
//!   any measured read makes is 8 KB (the choker's mapping file during the
//!   first sync); the editor's repeating reads never ask for more than 2.5 KB.
//!
//! The rule for the numbers (Yona, 2026-09-27, plan
//! `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`, D1): total free ≥
//! the worst measured read working set on that chip + room for the link and
//! radio tasks meanwhile; largest block ≥ 2 × the largest single read ask.
//!
//! Until 2026-09-28 the gate asked only the second question, at 32 KiB
//! ([`PROJECT_READ_MIN_HEADROOM_BYTES`]). That was the wrong question on a
//! C6 with Bluetooth on: each shader edit leaves a few 2–4 KB objects in the
//! middle of the heap's big tail hole, so after a handful of edits the
//! largest block falls to ~19.5 KB (the second heap region, which the
//! Bluetooth controller shares) while ~90 KB is still free — and every read,
//! down to the device card's 2 KB one, was refused
//! (`docs/defects/2026-09-27-fragmented-heap-refuses-every-read.md`).
//!
//! [`PROJECT_READ_MIN_HEADROOM_BYTES`]: crate::PROJECT_READ_MIN_HEADROOM_BYTES

extern crate alloc;

use alloc::format;
use alloc::string::String;

/// The two floors a ProjectRead must clear to be served, installed per chip
/// with [`crate::LpServer::set_read_gate`].
///
/// Each floor is checked against its own source: total free from the
/// server's [`crate::MemoryStatsFn`], the largest block from its
/// [`crate::ReadHeadroomProbe`]. A floor whose source the embedder did not
/// install is not checked, and no gate at all (hosts, the browser) means a
/// read is never refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadGate {
    /// Refuse when total free heap is below this. `0` = not checked.
    pub min_free_bytes: u32,
    /// Refuse when the largest allocatable block is below this. `0` = not
    /// checked.
    pub min_largest_block_bytes: u32,
}

/// What the gate saw when it refused a read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadRefusal {
    /// The floors the read had to clear.
    pub gate: ReadGate,
    /// Total free heap at the time, when the embedder reports it.
    pub free_bytes: Option<u32>,
    /// The largest allocatable block at the time, when the embedder reports
    /// it.
    pub largest_block_bytes: Option<u32>,
}

impl ReadGate {
    /// Only the largest-block floor, at `bytes` — the gate's shape before the
    /// total-free floor existed.
    pub const fn largest_block_only(bytes: u32) -> Self {
        Self {
            min_free_bytes: 0,
            min_largest_block_bytes: bytes,
        }
    }

    /// Whether a read may start with `free_bytes` of heap free and a largest
    /// block of `largest_block_bytes` (either unknown = that floor passes).
    pub fn check(
        &self,
        free_bytes: Option<u32>,
        largest_block_bytes: Option<u32>,
    ) -> Result<(), ReadRefusal> {
        let short_of_free = free_bytes.is_some_and(|free| free < self.min_free_bytes);
        let short_of_block =
            largest_block_bytes.is_some_and(|largest| largest < self.min_largest_block_bytes);
        if short_of_free || short_of_block {
            Err(ReadRefusal {
                gate: *self,
                free_bytes,
                largest_block_bytes,
            })
        } else {
            Ok(())
        }
    }
}

impl ReadRefusal {
    /// The terminal `ProjectReadEvent::Error` text: what happened, and that
    /// it passes. A refusal is transient — the board is busy, not broken —
    /// and nothing about the query would change the answer (the old text
    /// advised narrowing it, which could not help: the editor's smallest read
    /// was refused as surely as its largest).
    pub fn message(&self) -> String {
        format!(
            "read refused: board memory busy (free {} B, largest block {} B; needs {} B free \
             and a {} B block); retry shortly",
            known(self.free_bytes),
            known(self.largest_block_bytes),
            self.gate.min_free_bytes,
            self.gate.min_largest_block_bytes,
        )
    }
}

fn known(bytes: Option<u32>) -> String {
    match bytes {
        Some(bytes) => format!("{bytes}"),
        None => String::from("?"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GATE: ReadGate = ReadGate {
        min_free_bytes: 40 * 1024,
        min_largest_block_bytes: 16 * 1024,
    };

    #[test]
    fn both_floors_clear_serves() {
        assert_eq!(GATE.check(Some(90_000), Some(19_480)), Ok(()));
    }

    #[test]
    fn a_small_largest_block_refuses() {
        assert!(GATE.check(Some(90_000), Some(16 * 1024 - 1)).is_err());
    }

    #[test]
    fn low_total_free_refuses_even_with_a_big_block() {
        assert!(
            GATE.check(Some(40 * 1024 - 1), Some(40 * 1024 - 1))
                .is_err()
        );
    }

    #[test]
    fn an_unknown_figure_passes_its_floor() {
        assert_eq!(GATE.check(None, Some(20_000)), Ok(()));
        assert_eq!(GATE.check(Some(50_000), None), Ok(()));
        assert_eq!(GATE.check(None, None), Ok(()));
    }

    #[test]
    fn largest_block_only_never_checks_free() {
        let gate = ReadGate::largest_block_only(32 * 1024);
        assert_eq!(gate.check(Some(0), Some(32 * 1024)), Ok(()));
        assert!(gate.check(Some(u32::MAX), Some(32 * 1024 - 1)).is_err());
    }

    #[test]
    fn the_message_says_busy_and_retry_with_the_numbers() {
        let refusal = GATE.check(Some(38_000), None).unwrap_err();
        assert_eq!(
            refusal.message(),
            "read refused: board memory busy (free 38000 B, largest block ? B; needs 40960 B \
             free and a 16384 B block); retry shortly"
        );
    }
}
