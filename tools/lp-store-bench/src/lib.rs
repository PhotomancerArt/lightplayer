//! The storage testbed: race on-device store candidates through simulated
//! power cuts on `lp-nor-sim`.
//!
//! A [`Candidate`] (format, mount) yields a [`CandidateStore`]; a [`Workload`]
//! is steps of puts and prefix deletes ending in a commit, built from corpora;
//! the oracle ([`oracle`]) judges every cut (each document old or new, the
//! store still writes); the drivers sweep cut points exhaustively, double cut,
//! walk at random, or measure fault-free; the [`Scoreboard`] is JSONL.
//!
//! Host tooling for the spike `lp2025/2026-10-07-1858-lpfs-fit-spike`; nothing
//! here is linked into firmware. Async stores are driven with a null-waker
//! `block_on` here only (a test edge, per AGENTS.md "Sans-IO core").

mod board_files;
mod candidate;
mod candidate_report;
pub mod candidates;
mod corpus;
pub mod cut_case;
pub mod driver_double_cut;
pub mod driver_endurance;
pub mod driver_exhaustive;
pub mod driver_fill;
pub mod driver_full_flash;
pub mod driver_fuzz;
pub mod driver_long;
pub mod driver_measure;
#[cfg(feature = "mutants")]
pub mod driver_mutants;
pub mod driver_random;
pub mod gc_tally;
pub mod oracle;
pub mod overnight;
mod panic_quiet;
pub mod refusal_check;
pub mod report;
mod reproducer;
mod scoreboard;
mod store_error;
pub mod workload;
mod workload_step;

pub use board_files::board_files;
pub use candidate::{Candidate, CandidateConfig, CandidateStore};
pub use candidate_report::CandidateReport;
pub use corpus::{Corpus, CorpusDoc};
pub use panic_quiet::catch_quiet;
pub use reproducer::{Reproducer, replay};
pub use scoreboard::{Scoreboard, read_scoreboard};
pub use store_error::StoreError;
pub use workload::{CorpusSet, Workload, WorkloadKind, WorkloadSpec};
pub use workload_step::{Op, Step};
