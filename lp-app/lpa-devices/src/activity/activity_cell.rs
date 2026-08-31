//! The supervision machinery: what an activity is, and the contract a
//! reducer implements.
//!
//! Supervision itself (cancel → grace → eviction → recovery) lives on
//! [`Device::handle`](crate::Device); this file holds the cell it supervises.

use serde::{Deserialize, Serialize};

use crate::event::{Command, EffectId, Input};
use crate::evidence::Evidence;
use crate::link::LinkId;
use crate::roster::RosterConfig;
use crate::time::Millis;

use super::erase::EraseActivity;
use super::flash::FlashActivity;
use super::identify::IdentifyActivity;
use super::push::PushActivity;

/// Which flow an activity is running. One per device at a time (invariant
/// I5): a gesture on a busy device gets a visible "busy with X — cancel it?",
/// never a silent queue behind a hang.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActivityKind {
    Identify,
    Flash,
    Push,
    Erase,
}

impl ActivityKind {
    /// Present-tense label for the card ("Identifying…").
    pub fn label(self) -> &'static str {
        match self {
            Self::Identify => "Identifying",
            Self::Flash => "Flashing firmware",
            Self::Push => "Sending the project",
            Self::Erase => "Erasing the flash",
        }
    }

    /// How long a cancelled activity of this kind gets to wind down before
    /// eviction. Flash is wide on purpose: esptool-js cannot abort a write
    /// cleanly, so the reducer holds a cancel through the write window with
    /// an honest label — a 2 s grace would evict it into a half-written
    /// image. Cancellation stays bounded (I2); the bound is just as wide as
    /// the physics.
    pub fn cancel_grace_ms(self, config: &RosterConfig) -> u64 {
        match self {
            Self::Identify => config.cancel_grace_ms,
            Self::Flash => config.flash_cancel_grace_ms,
            // A push cannot be torn out mid-write either — the device's
            // project dir was cleared before the first byte went down — so
            // the cancel waits out the conversation rather than leaving half
            // a project on the board.
            Self::Push => config.push_cancel_grace_ms,
            // An erase cannot be aborted mid-wipe either; the flash grace
            // is the same physics.
            Self::Erase => config.flash_cancel_grace_ms,
        }
    }
}

/// How an activity ended. Outcomes persist on the device entry, keyed by
/// identity, until a new activity supersedes them (invariant I4).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActivityOutcome {
    Succeeded {
        summary: String,
    },
    Failed {
        message: String,
    },
    /// The user cancelled and the activity wound down in time.
    Cancelled,
    /// The activity's deadline expired (invariant I1: no operation exists
    /// without a deadline).
    TimedOut,
    /// Something removed the ground under it — link lost, device forgotten,
    /// or a cancel grace that expired (invariant I2: every cancel is bounded).
    Interrupted {
        reason: String,
    },
}

impl ActivityOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }

    /// One-line description for the card banner and the journal.
    pub fn summary(&self) -> String {
        match self {
            Self::Succeeded { summary } => summary.clone(),
            Self::Failed { message } => message.clone(),
            Self::Cancelled => "cancelled".to_string(),
            Self::TimedOut => "timed out".to_string(),
            Self::Interrupted { reason } => format!("interrupted: {reason}"),
        }
    }
}

/// What a reducer did with one input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityStep {
    Continue(Vec<Command>),
    Done {
        outcome: ActivityOutcome,
        commands: Vec<Command>,
    },
}

impl ActivityStep {
    pub fn nothing() -> Self {
        Self::Continue(Vec::new())
    }

    pub fn done(outcome: ActivityOutcome) -> Self {
        Self::Done {
            outcome,
            commands: Vec::new(),
        }
    }
}

/// Read-only context a reducer gets alongside each input.
///
/// A deliberate deviation from the milestone's bare
/// `handle(&mut self, now, input)`: a sans-IO reducer cannot address a
/// transport or read the fold's conclusions out of thin air, and the
/// alternative (copying evidence into every reducer) would grow a second
/// place where device facts live — the exact disease being removed. Nothing
/// here is mutable.
pub struct ActivityCtx<'a> {
    /// The link this device is routed to, if any.
    pub link: Option<LinkId>,
    /// The fold's current conclusions. Folded BEFORE the input reaches the
    /// reducer, so a reducer always reads fresh evidence.
    pub evidence: &'a Evidence,
    pub config: &'a RosterConfig,
    /// The stamp a [`Command::RunEffect`](crate::Command::RunEffect) emitted
    /// from THIS step must wear. Minted by the device, which then records it
    /// on the cell — that pairing is what lets a marker from an evicted
    /// effect be recognized and dropped.
    pub effect_id: EffectId,
}

/// The one thing a flow implements.
pub trait ActivityReducer {
    fn kind(&self) -> ActivityKind;

    /// Handle one forwarded input. Cancellation arrives here as
    /// [`Action::CancelActivity`](crate::Action::CancelActivity): the
    /// reducer may wind down over several inputs, and supervision bounds how
    /// long it gets.
    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep;

    /// The next instant this reducer wants to be ticked at (a re-ask
    /// cadence, its own settle deadline). The device folds this into the one
    /// timer it keeps armed.
    fn next_deadline(&self) -> Option<Millis>;
}

/// Where a cancel request has got to.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CancelPhase {
    Running,
    /// The reducer was asked to wind down at `since`. When the grace expires
    /// the cell is evicted and the device recovers — cancellation is bounded
    /// by removal, not by politeness.
    CancelRequested {
        since: Millis,
    },
}

/// Progress reported by a coarse effect (esptool-js flashing, in round 2).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActivityProgress {
    pub label: String,
    pub percent: Option<u8>,
}

/// One running activity, with its brackets and its deadline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActivityCell {
    pub kind: ActivityKind,
    pub started_at: Millis,
    /// Supervision backstop. The reducer normally settles itself first (it
    /// is handed each tick before supervision looks at the clock); this is
    /// the guarantee that it cannot run forever.
    pub deadline: Millis,
    pub cancel: CancelPhase,
    pub progress: Option<ActivityProgress>,
    /// The coarse effect this cell is currently waiting on, if any. Markers
    /// stamped with anything else came from an effect this cell does not own
    /// — a straggler from an evicted predecessor — and are dropped.
    #[serde(default)]
    pub current_effect: Option<EffectId>,
    reducer: Reducer,
}

impl ActivityCell {
    pub(crate) fn new(started_at: Millis, deadline: Millis, reducer: Reducer) -> Self {
        Self {
            kind: reducer.kind(),
            started_at,
            deadline,
            cancel: CancelPhase::Running,
            progress: None,
            current_effect: None,
            reducer,
        }
    }

    pub fn is_cancel_requested(&self) -> bool {
        matches!(self.cancel, CancelPhase::CancelRequested { .. })
    }

    pub(crate) fn handle(
        &mut self,
        now: Millis,
        input: &Input,
        ctx: &mut ActivityCtx<'_>,
    ) -> ActivityStep {
        self.reducer.handle(now, input, ctx)
    }

    /// Nearest instant this cell needs a tick at: its own deadline, its
    /// cancel grace expiry, or whatever cadence the reducer keeps.
    pub(crate) fn next_deadline(&self, grace_ms: u64) -> Millis {
        let mut soonest = self.deadline;
        if let CancelPhase::CancelRequested { since } = self.cancel {
            soonest = soonest.min(since.plus_ms(grace_ms));
        }
        if let Some(reducer_deadline) = self.reducer.next_deadline() {
            soonest = soonest.min(reducer_deadline);
        }
        soonest
    }

    /// Whether the cancel grace has run out at `now`.
    pub(crate) fn cancel_grace_expired(&self, now: Millis, grace_ms: u64) -> bool {
        match self.cancel {
            CancelPhase::Running => false,
            CancelPhase::CancelRequested { since } => now.since(since) >= grace_ms,
        }
    }
}

/// The reducer, as a closed enum rather than a `Box<dyn>`: it keeps
/// [`ActivityCell`] `Clone + PartialEq + Serialize`, which is what lets
/// fixtures assert on a whole roster and lets the journal be replayed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum Reducer {
    Identify(IdentifyActivity),
    Flash(FlashActivity),
    Push(PushActivity),
    Erase(EraseActivity),
}

impl ActivityReducer for Reducer {
    fn kind(&self) -> ActivityKind {
        match self {
            Self::Identify(reducer) => reducer.kind(),
            Self::Flash(reducer) => reducer.kind(),
            Self::Push(reducer) => reducer.kind(),
            Self::Erase(reducer) => reducer.kind(),
        }
    }

    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match self {
            Self::Identify(reducer) => reducer.handle(now, input, ctx),
            Self::Flash(reducer) => reducer.handle(now, input, ctx),
            Self::Push(reducer) => reducer.handle(now, input, ctx),
            Self::Erase(reducer) => reducer.handle(now, input, ctx),
        }
    }

    fn next_deadline(&self) -> Option<Millis> {
        match self {
            Self::Identify(reducer) => reducer.next_deadline(),
            Self::Flash(reducer) => reducer.next_deadline(),
            Self::Push(reducer) => reducer.next_deadline(),
            Self::Erase(reducer) => reducer.next_deadline(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_soonest_deadline_wins_and_cancel_grace_can_be_it() {
        let mut cell = ActivityCell::new(
            Millis(0),
            Millis(5_000),
            Reducer::Identify(IdentifyActivity::new(Millis(0), Millis(5_000), 1_000)),
        );

        // The reducer's 1 s re-ask cadence is nearer than the 5 s deadline.
        assert_eq!(cell.next_deadline(2_000), Millis(1_000));

        cell.cancel = CancelPhase::CancelRequested {
            since: Millis(1_200),
        };
        assert!(cell.is_cancel_requested());
        assert!(!cell.cancel_grace_expired(Millis(2_000), 2_000));
        assert!(cell.cancel_grace_expired(Millis(3_200), 2_000));
    }

    #[test]
    fn outcomes_describe_themselves_for_the_banner() {
        assert_eq!(
            ActivityOutcome::Succeeded {
                summary: "LightPlayer".to_string()
            }
            .summary(),
            "LightPlayer"
        );
        assert_eq!(ActivityOutcome::Cancelled.summary(), "cancelled");
        assert_eq!(
            ActivityOutcome::Interrupted {
                reason: "unplugged".to_string()
            }
            .summary(),
            "interrupted: unplugged"
        );
    }
}
