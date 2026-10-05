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
//! [`flash::FlashActivity`] (round 2's coarse-effect centerpiece),
//! [`push::PushActivity`] (its second consumer), the two always-actions
//! [`erase::EraseActivity`] (Factory reset) and
//! [`remove_project::RemoveProjectActivity`], and
//! [`update::UpdateActivity`] — an over-the-air update run in legs across
//! the board's resets, the one activity that outlives its link. Flash and
//! Update wait for a reset board the same way: `reopen_rung` is that
//! shared rung. Pull is the remaining round-2 variant of [`ActivityKind`]
//! and `Reducer` (M4); the old Setup/Provision orchestrators dissolved into
//! the card ruling — Flash and Push ARE the flows.

pub(crate) mod activity_cell;
pub mod erase;
pub mod flash;
pub mod flash_step;
pub mod identify;
pub mod layout_verdict;
pub mod push;
pub mod remove_project;
pub(crate) mod reopen_rung;
pub mod update;
pub mod update_activity_view;
pub mod update_intent_facts;
pub mod update_outcome_facts;
pub mod update_stage_facts;

pub use activity_cell::{
    ActivityCell, ActivityCtx, ActivityKind, ActivityOutcome, ActivityProgress, ActivityReducer,
    ActivityStep, CancelPhase,
};
pub use erase::EraseActivity;
pub use flash::FlashActivity;
pub use flash_step::FlashStep;
pub use identify::IdentifyActivity;
pub use layout_verdict::{FlashLayoutView, LayoutVerdict};
pub use push::PushActivity;
pub use remove_project::RemoveProjectActivity;
pub use update::{UPDATE_DEADLINE_MS, UPDATE_GAP_MS, UpdateActivity};
pub use update_activity_view::UpdateActivityView;
pub use update_intent_facts::UpdateIntentFacts;
pub use update_outcome_facts::UpdateOutcomeFacts;
pub use update_stage_facts::UpdateStageFacts;
