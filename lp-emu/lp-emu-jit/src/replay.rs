//! The identity harness's record and compare shapes.
//!
//! A translated module has to be proved byte-identical to the interpreter in
//! **every engine it will run in**, not just the one that emitted it. The
//! mechanism is a recording and a replay: an interpreter run records what each
//! entry into translated code was handed and what it produced, and the same
//! module is then replayed against that recording under wasmtime, under V8 and
//! under JavaScriptCore. A divergence names the entry it happened at.
//!
//! This module is the **shape only**. Nothing here produces a recording yet —
//! P3 does that — but the shape lands now so P3 does not invent it under time
//! pressure, and because one field of it was learned the hard way.
//!
//! # Why the memory-granule diff exists
//!
//! **A replay without it silently lies.** Translated code does not cover the
//! whole guest: the interpreter runs the blocks it refuses, *between* entries,
//! and those blocks write memory. A replay that reloads the initial memory
//! snapshot and then runs only the recorded entries is therefore executing the
//! translated code against memory the real run never had. The spike's first
//! replay did exactly that and diverged at entry 21 — not because the
//! translation was wrong, but because the harness was.
//!
//! So every entry carries the memory that changed since the previous one, and
//! the replay applies it before running the entry, standing in for the
//! interpreter. Diffing at [`GRANULE_BYTES`] granularity rather than
//! byte-by-byte is what keeps that affordable: it cost ~180 bytes per entry on
//! `render-rocaille`. Applying the diff is not part of what a replay times —
//! only the entry itself is.

use alloc::vec::Vec;

/// The unit the between-entries memory diff is taken in.
///
/// A whole granule is recorded when any byte in it changed. 64 bytes is the
/// spike's measured figure: fine enough that a single guest store does not drag
/// a page along with it, coarse enough that the per-granule header does not
/// dominate. It is a recording-format constant — a recorder and the replay that
/// reads it must agree on it — not a tuning knob.
pub const GRANULE_BYTES: usize = 64;

/// The number of architectural registers a record carries.
///
/// `x0` is not one of them: it reads as zero by definition, so recording it
/// would be recording a constant, and comparing it would be comparing the
/// decoder to itself.
pub const RECORDED_REGS: usize = 31;

/// `x1`..`x31`, in that order.
///
/// Indexed by *architectural* register number so that a caller cannot quietly
/// be off by one against the `[i32; 32]` file the interpreter keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Regs([i32; RECORDED_REGS]);

impl Regs {
    /// All zero.
    #[must_use]
    pub const fn zeroed() -> Self {
        Self([0; RECORDED_REGS])
    }

    /// Take `x1`..`x31` from the interpreter's 32-entry register file.
    #[must_use]
    pub fn from_file(file: &[i32; 32]) -> Self {
        let mut regs = [0i32; RECORDED_REGS];
        regs.copy_from_slice(&file[1..]);
        Self(regs)
    }

    /// Write `x1`..`x31` back into a 32-entry register file, leaving `x0` at
    /// zero.
    pub fn into_file(self, file: &mut [i32; 32]) {
        file[0] = 0;
        file[1..].copy_from_slice(&self.0);
    }

    /// The value of `x<n>`, for `n` in `1..=31`.
    ///
    /// # Panics
    ///
    /// If `n` is 0 or above 31 — both are bugs in the caller, not states a
    /// recording can be in.
    #[must_use]
    pub fn get(&self, n: u8) -> i32 {
        assert!(
            (1..=31).contains(&n),
            "x{n} is not an architectural register a record carries"
        );
        self.0[usize::from(n) - 1]
    }

    /// The first register whose value differs, as `(n, mine, theirs)`.
    #[must_use]
    pub fn first_difference(&self, other: &Self) -> Option<(u8, i32, i32)> {
        self.0
            .iter()
            .zip(other.0.iter())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| ((i + 1) as u8, *a, *b))
    }
}

impl Default for Regs {
    fn default() -> Self {
        Self::zeroed()
    }
}

/// One granule of guest memory that changed since the previous entry.
///
/// `offset` is a byte offset into the guest arena — not a guest address —
/// because that is what a replay host can apply without knowing the arena's
/// base. It is a whole multiple of [`GRANULE_BYTES`], and `bytes` is that long
/// except in the last granule of the arena.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Granule {
    pub offset: u32,
    pub bytes: Vec<u8>,
}

/// Everything the interpreter wrote to guest memory between two entries.
///
/// Empty is the common case and the cheap one; see the module docs for why it
/// is not the only case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryDelta {
    pub granules: Vec<Granule>,
}

impl MemoryDelta {
    /// Apply the delta to a replay host's copy of the guest arena.
    ///
    /// A granule that does not fit is a corrupt recording rather than a
    /// divergence, so this reports it as an error the caller must not treat as
    /// a mismatch.
    pub fn apply(&self, arena: &mut [u8]) -> Result<(), DeltaError> {
        for granule in &self.granules {
            let at = granule.offset as usize;
            let end = at
                .checked_add(granule.bytes.len())
                .ok_or(DeltaError::OutOfRange {
                    offset: granule.offset,
                    len: granule.bytes.len(),
                    arena: arena.len(),
                })?;
            if end > arena.len() {
                return Err(DeltaError::OutOfRange {
                    offset: granule.offset,
                    len: granule.bytes.len(),
                    arena: arena.len(),
                });
            }
            arena[at..end].copy_from_slice(&granule.bytes);
        }
        Ok(())
    }

    /// The bytes this delta carries, excluding per-granule bookkeeping. The
    /// number the spike reported as ~180 per entry.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.granules.iter().map(|g| g.bytes.len()).sum()
    }
}

/// A recording that cannot be applied to the arena it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaError {
    OutOfRange {
        offset: u32,
        len: usize,
        arena: usize,
    },
}

/// What a stay in translated code produced.
///
/// The same shape whether it came from the interpreter's recording or from the
/// engine under test, so [`compare`] has nothing to translate between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryOutcome {
    /// Where control left translated code.
    pub exit_pc: u32,
    /// The cycle counter at the exit. Absolute, not a delta: peripherals read
    /// `cx.now` mid-block, so the counter is an observable and its absolute
    /// value is what an oracle compares (JD17).
    pub cycle: u64,
    /// Instructions retired during the stay. A delta, because that is what the
    /// `stopped after` line's retired figure accumulates.
    pub retired: u32,
    /// Did the stay end after a store? The machine resamples external state
    /// after one, so getting this wrong moves an interrupt.
    pub after_store: bool,
    /// `x1`..`x31` at the exit.
    pub regs: Regs,
}

/// One entry into translated code, recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryRecord {
    /// Where translated code was entered.
    pub entry_pc: u32,
    /// The cycle counter at the entry.
    pub cycle_in: u64,
    /// `x1`..`x31` at the entry — what a replay seeds the engine with.
    pub regs_in: Regs,
    /// What the interpreter's run produced. A replay must reproduce it
    /// exactly.
    pub outcome: EntryOutcome,
    /// Guest memory the interpreter changed between the previous entry's exit
    /// and this entry — see the module docs. Applied *before* the entry runs,
    /// and never counted in a timing.
    pub memory_delta: MemoryDelta,
}

impl EntryRecord {
    /// Cycles charged during the stay.
    #[must_use]
    pub fn cycles(&self) -> u64 {
        self.outcome.cycle.saturating_sub(self.cycle_in)
    }
}

/// A whole run's worth of entries, in the order they happened.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayRecord {
    pub entries: Vec<EntryRecord>,
}

impl ReplayRecord {
    /// The mean bytes of memory delta per entry — the figure that says whether
    /// the diff is affordable on a given image. The spike measured ~180 on
    /// `render-rocaille`.
    #[must_use]
    pub fn mean_delta_bytes(&self) -> f64 {
        if self.entries.is_empty() {
            return 0.0;
        }
        let total: usize = self.entries.iter().map(|e| e.memory_delta.byte_len()).sum();
        total as f64 / self.entries.len() as f64
    }
}

/// The first way a replayed entry differed from its recording.
///
/// One variant per architectural output, because "entry 21 diverged" is not
/// actionable and "entry 21's `a3` is 0x40 low" is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Divergence {
    ExitPc {
        recorded: u32,
        replayed: u32,
    },
    Cycle {
        recorded: u64,
        replayed: u64,
    },
    Retired {
        recorded: u32,
        replayed: u32,
    },
    AfterStore {
        recorded: bool,
        replayed: bool,
    },
    Register {
        reg: u8,
        recorded: i32,
        replayed: i32,
    },
}

/// A divergence, and where in the run it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// Index into [`ReplayRecord::entries`].
    pub entry: usize,
    /// The guest pc that entry was entered at, so a reader can go straight to
    /// the disassembly.
    pub entry_pc: u32,
    pub divergence: Divergence,
}

/// Check one replayed entry against its recording.
///
/// Reports the *first* difference in a fixed order — exit pc, cycle, retired,
/// after-store, then registers in number order — so that two runs of the same
/// divergence report the same thing.
pub fn compare(recorded: &EntryOutcome, replayed: &EntryOutcome) -> Result<(), Divergence> {
    if recorded.exit_pc != replayed.exit_pc {
        return Err(Divergence::ExitPc {
            recorded: recorded.exit_pc,
            replayed: replayed.exit_pc,
        });
    }
    if recorded.cycle != replayed.cycle {
        return Err(Divergence::Cycle {
            recorded: recorded.cycle,
            replayed: replayed.cycle,
        });
    }
    if recorded.retired != replayed.retired {
        return Err(Divergence::Retired {
            recorded: recorded.retired,
            replayed: replayed.retired,
        });
    }
    if recorded.after_store != replayed.after_store {
        return Err(Divergence::AfterStore {
            recorded: recorded.after_store,
            replayed: replayed.after_store,
        });
    }
    if let Some((reg, rec, rep)) = recorded.regs.first_difference(&replayed.regs) {
        return Err(Divergence::Register {
            reg,
            recorded: rec,
            replayed: rep,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn outcome() -> EntryOutcome {
        EntryOutcome {
            exit_pc: 0x4208_0500,
            cycle: 1_000,
            retired: 12,
            after_store: false,
            regs: Regs::zeroed(),
        }
    }

    #[test]
    fn a_register_index_is_the_architectural_number() {
        let mut file = [0i32; 32];
        file[0] = 99; // x0 is a constant; a record must not carry it.
        file[1] = 11;
        file[31] = 31;
        let regs = Regs::from_file(&file);
        assert_eq!(regs.get(1), 11);
        assert_eq!(regs.get(31), 31);

        let mut back = [7i32; 32];
        regs.into_file(&mut back);
        assert_eq!(back[0], 0, "x0 is restored as zero, not as what was there");
        assert_eq!(back[1], 11);
        assert_eq!(back[31], 31);
    }

    #[test]
    fn compare_reports_the_first_difference_in_a_fixed_order() {
        assert_eq!(compare(&outcome(), &outcome()), Ok(()));

        let mut theirs = outcome();
        theirs.exit_pc = 0x4208_0600;
        theirs.retired = 13;
        assert_eq!(
            compare(&outcome(), &theirs),
            Err(Divergence::ExitPc {
                recorded: 0x4208_0500,
                replayed: 0x4208_0600,
            }),
            "the exit pc is reported before the retired count, always"
        );

        let mut file = [0i32; 32];
        file[13] = 0x40;
        let mut theirs = outcome();
        theirs.regs = Regs::from_file(&file);
        assert_eq!(
            compare(&outcome(), &theirs),
            Err(Divergence::Register {
                reg: 13,
                recorded: 0,
                replayed: 0x40,
            })
        );
    }

    #[test]
    fn a_delta_stands_in_for_the_interpreters_writes_between_entries() {
        let mut arena = vec![0u8; 4 * GRANULE_BYTES];
        let delta = MemoryDelta {
            granules: vec![Granule {
                offset: (2 * GRANULE_BYTES) as u32,
                bytes: vec![0xab; GRANULE_BYTES],
            }],
        };
        assert_eq!(delta.byte_len(), GRANULE_BYTES);
        delta.apply(&mut arena).expect("in range");
        assert_eq!(arena[2 * GRANULE_BYTES], 0xab);
        assert_eq!(arena[2 * GRANULE_BYTES - 1], 0);
        assert_eq!(arena[3 * GRANULE_BYTES], 0);
    }

    #[test]
    fn a_granule_past_the_arena_is_a_corrupt_recording_not_a_divergence() {
        let mut arena = vec![0u8; GRANULE_BYTES];
        let delta = MemoryDelta {
            granules: vec![Granule {
                offset: GRANULE_BYTES as u32,
                bytes: vec![0; 1],
            }],
        };
        assert_eq!(
            delta.apply(&mut arena),
            Err(DeltaError::OutOfRange {
                offset: GRANULE_BYTES as u32,
                len: 1,
                arena: GRANULE_BYTES,
            })
        );
    }
}
