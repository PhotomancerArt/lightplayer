//! Activities: one supervised reducer per flow.
//!
//! **Existence is imperative, state is reduced.** A user action spawns the
//! activity; the spawn and its end are journaled brackets that the device
//! fold consumes, so "busy with X" participates in derived state without a
//! parallel store. Between the brackets, activity state moves only by
//! forwarded inputs.
//!
//! Reducers are sans-IO: events in, commands out, never an `.await`. That is
//! what makes eviction safe — there is no half-finished future holding
//! controller state — and what makes every flow testable by event script.
//!
//! M1 ships one activity, [`identify::IdentifyActivity`]. Setup, Flash,
//! Provision, Push and Pull are round-2 variants of [`ActivityKind`] and
//! `Reducer`.

pub(crate) mod activity_cell;
pub mod identify;

pub use activity_cell::{
    ActivityCell, ActivityCtx, ActivityKind, ActivityOutcome, ActivityProgress, ActivityReducer,
    ActivityStep, CancelPhase,
};
pub use identify::IdentifyActivity;
