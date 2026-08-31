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
//! Shipped activities: [`identify::IdentifyActivity`] (round 1),
//! [`flash::FlashActivity`] (round 2's coarse-effect centerpiece) and
//! [`push::PushActivity`] (its second consumer). Pull is the remaining
//! round-2 variant of [`ActivityKind`] and `Reducer` (M4); the old
//! Setup/Provision orchestrators dissolved into the card ruling — Flash and
//! Push ARE the flows.

pub(crate) mod activity_cell;
pub mod flash;
pub mod identify;
pub mod push;

pub use activity_cell::{
    ActivityCell, ActivityCtx, ActivityKind, ActivityOutcome, ActivityProgress, ActivityReducer,
    ActivityStep, CancelPhase,
};
pub use flash::FlashActivity;
pub use identify::IdentifyActivity;
pub use push::PushActivity;
