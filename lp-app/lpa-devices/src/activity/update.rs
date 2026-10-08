//! The Update activity: put firmware on this board over its own link (an
//! over-the-air update — over USB or Bluetooth, never esptool).
//!
//! One flow the card shows from the first byte to the new version, but the
//! link under it goes down and comes back up to three times: the board
//! resets into its new core (core-only), into its trial, and into the
//! running engine. So the activity runs in **legs**:
//!
//! 1. **Starting** — wait for an open link (knocking on a closed one), then
//!    the first leg.
//! 2. **Leg** — [`EffectRequest::Update`] runs over the device's open link.
//!    The effects layer holds one update driver per device across legs (it
//!    resumes from the board's manifest on every link), streams
//!    [`ActivityMarker::UpdateStage`] for the card's stage and percent, and
//!    ends the leg with [`ActivityMarker::Ended`]. A leg ends one of three
//!    ways, and the contract is exact:
//!    - an [`ActivityMarker::UpdateOutcome`] first, then `Ended` — the
//!      driver finished: the activity ends on that outcome;
//!    - `Ended` with [`ActivityOutcome::Interrupted`] and no outcome — the
//!      board reset or its link dropped: **not a failure**, the activity
//!      waits between legs;
//!    - `Ended` with anything else and no outcome — the leg could not run
//!      at all (the effects layer's own failure): the activity ends with
//!      that outcome as it is, rather than looping on a leg that cannot run.
//! 3. **Between legs** — the Flash ladder's reopen rung, shared
//!    (`reopen_rung`): reopen a closed serial port on its retry
//!    cadence (a native-USB C6 re-enumerates on every software reset, as
//!    after a flash), or leave a Bluetooth link to its provider's own
//!    reconnect loop — and open the new link that loop attaches, which
//!    arrives closed (opening a connected session is the model starting to
//!    listen, not a connect); ask an open, quiet one for a hello. The board is back
//!    when the link is open and the board has spoken since the leg ended —
//!    a hello, or its manifest on channel 3 (a core-only board sends no
//!    hello, only `M`). Then the next leg.
//!
//! ⚠️ What brings a leg back has to be NEWER than the leg. The observation
//! window survives a close, so the board's pre-reset hello (and manifest)
//! is still in the evidence while the link is down; reading it as the board
//! coming back would start the next leg on a dead link. "Since" is the
//! instant the leg ended — or, when the link reopened DURING the leg (a
//! Bluetooth reconnect can beat the leg's own end marker), anything heard
//! in that newer window.
//!
//! A gap is bounded ([`UPDATE_GAP_MS`]): one that runs out ends the activity
//! honestly — [`UpdateOutcomeFacts::BoardDidNotComeBack`]: the board keeps
//! its place, and reconnecting it finishes the update. The card stays on the
//! evidence's face with that sentence, never on "Offline" alone.
//!
//! **A link drop is not a failure here.** The activity survives a vanished
//! link (an unplug, a re-enumeration's new link generation) where every
//! other activity is evicted — see `Device::lose_link` — so the card keeps
//! "Updating… 40%" and the device stays busy rather than flipping to the
//! offline face.
//!
//! **Cancel** is offered only while nothing on the board has changed: before
//! the first stage, and while backing up. Once writing starts there is no
//! Cancel ([`Self::cancellable`]); a cancel is then refused rather than
//! held. A cancel while backing up ends the activity at once, and the
//! device abandons the running leg — which is how the effects layer learns
//! to drop the driver.
//!
//! All waiting is scheduled timers (I7); the reducer never reads a clock.

use serde::{Deserialize, Serialize};

use crate::event::{Action, ActivityMarker, Command, EffectRequest, Event, Input};
use crate::evidence::{Evidence, Presence};
use crate::identity::DeviceId;
use crate::time::Millis;

use super::activity_cell::{
    ActivityCtx, ActivityKind, ActivityOutcome, ActivityReducer, ActivityStep,
};
use super::reopen_rung::{self, ClosedPort};
use super::update_activity_view::UpdateActivityView;
use super::update_intent_facts::UpdateIntentFacts;
use super::update_outcome_facts::UpdateOutcomeFacts;
use super::update_stage_facts::UpdateStageFacts;

/// How long one gap between legs (or the wait for the first open link) may
/// last before the activity ends honestly.
///
/// Sized for the slowest way back measured, with room to spare:
/// - a native-USB C6 re-enumerates on every software reset — the same
///   physics the Flash ladder waits out, whose whole classic climb is three
///   8 s rungs (`RosterConfig::flash_rung_ms`, bench G1 2026-08-31) plus the
///   2.5 s park settle; a gap is never shorter than that climb (a test
///   holds it);
/// - a core-only board verifies its image before its USB comes back, and a
///   core on trial boots its engine before it says hello (seconds each);
/// - Bluefy reconnects a held device with no gesture, 803 ms after a reboot
///   on the desk (G1 Run F), but a phone that locked or walked out of range
///   takes as long as the person does to come back to it.
pub const UPDATE_GAP_MS: u64 = 90_000;

/// Supervision backstop for a whole update (I1). Bluetooth is the slow
/// link: at the spike's ~22 KB/s a backup read-back (~1.8 MB, ~81 s), the
/// core (~1.1 MB, ~52 s) and the engine (~1.8 MB, ~81 s) take under four
/// minutes, plus up to three gaps of [`UPDATE_GAP_MS`]. Thirty minutes is
/// several times that; a leg is driven by the effect, so this backstop is
/// what bounds one that never ends.
pub const UPDATE_DEADLINE_MS: u64 = 30 * 60_000;

/// The honest failure when the first leg never got an open link.
const LINK_NEVER_OPENED: &str = "not updated: the board's link never opened. Nothing was changed.";

/// The label before the first stage arrives.
const STARTING_LABEL: &str = "Updating firmware…";

/// Where the update currently is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum UpdatePhase {
    /// Spawned on a link that is not open yet; knocking until it is.
    Starting {
        deadline: Millis,
        next_poke_at: Millis,
    },
    /// A leg runs: the effects layer drives the update over the open link.
    Leg { started_at: Millis },
    /// The leg ended with the link (a reset or a drop); waiting for the
    /// board to come back.
    BetweenLegs {
        /// When the leg ended (or its link vanished): only the board
        /// speaking at or after this is the board back.
        since: Millis,
        /// When the leg began: a window that opened after this is a newer
        /// link session than the leg's.
        leg_started_at: Millis,
        deadline: Millis,
        next_poke_at: Millis,
    },
}

/// The Update reducer's own state. Everything it learns about the board
/// lives in the fold; this holds only what the card shows across a gap.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UpdateActivity {
    device: DeviceId,
    intent: UpdateIntentFacts,
    /// The link reconnects by itself (Bluetooth, the LAN): the gap only waits, and
    /// never sends an open that would fight the provider's own loop.
    reconnects_itself: bool,
    phase: UpdatePhase,
    /// The last stage the driver reported, and its progress — kept across
    /// gaps, so the card still says "Updating… 40%" while reconnecting.
    stage: Option<UpdateStageFacts>,
    done: u32,
    total: u32,
    /// How the driver said it ended.
    outcome: Option<UpdateOutcomeFacts>,
    /// Legs started, for the journal and the tests.
    legs: u32,
    next_request_id: u32,
}

impl UpdateActivity {
    pub fn new(device: DeviceId, intent: UpdateIntentFacts, reconnects_itself: bool) -> Self {
        Self {
            device,
            intent,
            reconnects_itself,
            phase: UpdatePhase::Starting {
                deadline: Millis(0),
                next_poke_at: Millis(0),
            },
            stage: None,
            done: 0,
            total: 0,
            outcome: None,
            legs: 0,
            next_request_id: 1,
        }
    }

    /// The commands at spawn: the first leg on an open link, else a knock.
    /// Emitted by `Device::spawn_update`.
    pub(crate) fn spawn_commands(&mut self, now: Millis, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        if ctx.evidence.presence.is_open() {
            return self.start_leg(now, ctx);
        }
        self.phase = UpdatePhase::Starting {
            deadline: now.plus_ms(UPDATE_GAP_MS),
            next_poke_at: now.plus_ms(ctx.config.flash_reopen_retry_ms),
        };
        let closed = self.closed_port(ctx.evidence);
        reopen_rung::knock(ctx, &mut self.next_request_id, closed)
    }

    /// The card's label: the stage's, or the update's own before one.
    pub fn label(&self) -> &'static str {
        self.stage.map_or(STARTING_LABEL, UpdateStageFacts::label)
    }

    /// The stage's percent, once the driver reported a piece's length.
    pub fn percent(&self) -> Option<u8> {
        self.stage?;
        if self.total == 0 {
            return None;
        }
        let percent = u64::from(self.done.min(self.total)) * 100 / u64::from(self.total);
        u8::try_from(percent).ok()
    }

    /// Whether a cancel can still leave the board untouched: before the
    /// first stage, or while backing up.
    pub fn cancellable(&self) -> bool {
        self.stage.is_none_or(UpdateStageFacts::allows_cancel)
    }

    /// How the driver said it ended, or the model's own ending — what the
    /// device leaves on the evidence when the activity ends.
    pub fn outcome(&self) -> Option<UpdateOutcomeFacts> {
        self.outcome
    }

    /// Legs started so far.
    pub fn legs(&self) -> u32 {
        self.legs
    }

    /// The typed view the card's words are made of.
    pub fn view(&self) -> UpdateActivityView {
        UpdateActivityView {
            intent: self.intent.clone(),
            stage: self.stage,
            done: self.done,
            total: self.total,
            outcome: self.outcome,
            between_legs: matches!(self.phase, UpdatePhase::BetweenLegs { .. }),
        }
    }

    /// What the gap's knock does with a closed link.
    ///
    /// A serial port is reopened on the knock's cadence. A Bluetooth link
    /// that dropped is left to the provider's own reconnect loop — an open
    /// on it would be a connect fighting that loop. But the loop's success
    /// is a NEW link (OTA M7 P12): the departure sweep detached the dropped
    /// one, and the reconnect's sweep attached this one, closed, through the
    /// roster's re-attach, which spawns an Identify to open it — a no-op on
    /// a device this activity holds. So a closed link attached AFTER the gap
    /// began is the provider's reconnect, its session already connected, and
    /// opening it is the model starting to listen. (The dropped link's own
    /// close is never after the gap began: the leg ends on it.)
    fn closed_port(&self, evidence: &Evidence) -> ClosedPort {
        if !self.reconnects_itself {
            return ClosedPort::Reopen;
        }
        let gap_began = match self.phase {
            UpdatePhase::BetweenLegs { since, .. } => since,
            UpdatePhase::Starting { .. } | UpdatePhase::Leg { .. } => return ClosedPort::Wait,
        };
        match evidence.presence {
            Presence::Present { since, .. } if since > gap_began => ClosedPort::Reopen,
            _ => ClosedPort::Wait,
        }
    }

    fn start_leg(&mut self, now: Millis, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        self.phase = UpdatePhase::Leg { started_at: now };
        self.legs += 1;
        vec![Command::RunEffect {
            device: self.device,
            link,
            effect_id: ctx.effect_id,
            effect: EffectRequest::Update {
                intent: self.intent.clone(),
            },
        }]
    }

    /// End on `outcome`, keeping it for the evidence.
    fn finish(&mut self, outcome: UpdateOutcomeFacts) -> ActivityStep {
        self.outcome = Some(outcome);
        ActivityStep::done(outcome.activity_outcome())
    }

    /// The leg is over with its link: wait for the board — or, if it is
    /// already back (the link reopened during the leg), go on at once.
    fn between_legs(&mut self, now: Millis, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let leg_started_at = match self.phase {
            UpdatePhase::Leg { started_at } => started_at,
            _ => now,
        };
        self.phase = UpdatePhase::BetweenLegs {
            since: now,
            leg_started_at,
            deadline: now.plus_ms(UPDATE_GAP_MS),
            next_poke_at: now.plus_ms(ctx.config.flash_reopen_retry_ms),
        };
        if self.board_is_back(ctx.evidence) {
            return ActivityStep::Continue(self.start_leg(now, ctx));
        }
        let closed = self.closed_port(ctx.evidence);
        ActivityStep::Continue(reopen_rung::knock(ctx, &mut self.next_request_id, closed))
    }

    /// Whether the board came back after the leg that ended: the link is
    /// open, and the board spoke — a hello, or its manifest on channel 3 —
    /// since the leg ended, or in a window that opened after the leg began.
    fn board_is_back(&self, evidence: &Evidence) -> bool {
        let UpdatePhase::BetweenLegs {
            since,
            leg_started_at,
            ..
        } = self.phase
        else {
            return false;
        };
        if !evidence.presence.is_open() {
            return false;
        }
        let spoke_since = reopen_rung::hello_heard_since(evidence, since)
            || evidence
                .update_facts_heard_at()
                .is_some_and(|heard_at| heard_at >= since);
        let spoke_in_a_newer_window = evidence
            .window_started_at()
            .is_some_and(|started| started > leg_started_at)
            && (evidence.has_hello() || evidence.update_facts().is_some());
        spoke_since || spoke_in_a_newer_window
    }

    fn handle_marker(
        &mut self,
        now: Millis,
        marker: &ActivityMarker,
        ctx: &ActivityCtx<'_>,
    ) -> ActivityStep {
        match marker {
            ActivityMarker::UpdateStage { stage, done, total } => {
                self.stage = Some(*stage);
                self.done = *done;
                self.total = *total;
                ActivityStep::nothing()
            }
            ActivityMarker::UpdateOutcome(outcome) => match self.phase {
                // The leg's end follows; that is where the activity ends.
                UpdatePhase::Leg { .. } => {
                    self.outcome = Some(*outcome);
                    ActivityStep::nothing()
                }
                // The link went down first (a detach beats the effect's own
                // markers), but the driver did finish: that is the end.
                UpdatePhase::BetweenLegs { .. } | UpdatePhase::Starting { .. } => {
                    self.finish(*outcome)
                }
            },
            ActivityMarker::Ended { outcome, .. } => {
                if !matches!(self.phase, UpdatePhase::Leg { .. }) {
                    // A leg whose link already vanished: its end changes
                    // nothing — the gap is already being waited out.
                    return ActivityStep::nothing();
                }
                if let Some(finished) = self.outcome {
                    return self.finish(finished);
                }
                match outcome {
                    ActivityOutcome::Interrupted { .. } => self.between_legs(now, ctx),
                    // The leg could not run at all: end on what the effects
                    // layer said rather than loop on it.
                    other => ActivityStep::done(other.clone()),
                }
            }
            ActivityMarker::Started { .. }
            | ActivityMarker::Progress { .. }
            | ActivityMarker::LayoutVerdict { .. } => ActivityStep::nothing(),
        }
    }

    fn handle_timer(&mut self, now: Millis, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let closed = self.closed_port(ctx.evidence);
        match self.phase.clone() {
            UpdatePhase::Starting {
                deadline,
                next_poke_at,
            } => {
                if ctx.evidence.presence.is_open() {
                    return ActivityStep::Continue(self.start_leg(now, ctx));
                }
                if now >= deadline {
                    return ActivityStep::done(ActivityOutcome::Failed {
                        message: LINK_NEVER_OPENED.to_string(),
                    });
                }
                let mut next_poke_at = next_poke_at;
                let commands = reopen_rung::knock_when_due(
                    now,
                    &mut next_poke_at,
                    ctx,
                    &mut self.next_request_id,
                    closed,
                );
                self.phase = UpdatePhase::Starting {
                    deadline,
                    next_poke_at,
                };
                ActivityStep::Continue(commands.unwrap_or_default())
            }
            // The effect drives; supervision's backstop bounds it (I1).
            UpdatePhase::Leg { .. } => ActivityStep::nothing(),
            UpdatePhase::BetweenLegs {
                since,
                leg_started_at,
                deadline,
                next_poke_at,
            } => {
                if self.board_is_back(ctx.evidence) {
                    return ActivityStep::Continue(self.start_leg(now, ctx));
                }
                if now >= deadline {
                    return self.finish(UpdateOutcomeFacts::BoardDidNotComeBack);
                }
                let mut next_poke_at = next_poke_at;
                let commands = reopen_rung::knock_when_due(
                    now,
                    &mut next_poke_at,
                    ctx,
                    &mut self.next_request_id,
                    closed,
                );
                self.phase = UpdatePhase::BetweenLegs {
                    since,
                    leg_started_at,
                    deadline,
                    next_poke_at,
                };
                ActivityStep::Continue(commands.unwrap_or_default())
            }
        }
    }

    /// Something the board or the link did: it may be the link opening for
    /// the first leg, or the board coming back for the next.
    fn on_link_news(&mut self, now: Millis, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let ready = match self.phase {
            UpdatePhase::Starting { .. } => ctx.evidence.presence.is_open(),
            UpdatePhase::BetweenLegs { .. } => self.board_is_back(ctx.evidence),
            UpdatePhase::Leg { .. } => false,
        };
        match ready {
            true => ActivityStep::Continue(self.start_leg(now, ctx)),
            false => ActivityStep::nothing(),
        }
    }
}

impl ActivityReducer for UpdateActivity {
    fn kind(&self) -> ActivityKind {
        ActivityKind::Update
    }

    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match input {
            // Refused (nothing happens) once writing started; the device
            // never marks such a cancel as requested either.
            Input::Action(Action::CancelActivity { .. }) => match self.cancellable() {
                true => ActivityStep::done(ActivityOutcome::Cancelled),
                false => ActivityStep::nothing(),
            },
            Input::Action(_) => ActivityStep::nothing(),
            Input::Event(event) => match event {
                Event::ActivityMarker { marker, .. } => self.handle_marker(now, marker, ctx),
                Event::TimerFired { .. } => self.handle_timer(now, ctx),
                // The link vanished under a leg: nothing more can come from
                // it, so the gap starts now rather than waiting on an end
                // marker that may never be sent.
                Event::LinkDetached { .. } => {
                    if let UpdatePhase::Leg { started_at } = self.phase {
                        self.phase = UpdatePhase::BetweenLegs {
                            since: now,
                            leg_started_at: started_at,
                            deadline: now.plus_ms(UPDATE_GAP_MS),
                            next_poke_at: now.plus_ms(ctx.config.flash_reopen_retry_ms),
                        };
                    }
                    ActivityStep::nothing()
                }
                Event::Link { .. } | Event::LinkAttached { .. } => self.on_link_news(now, ctx),
                Event::IdentityObserved { .. }
                | Event::GrantAnswered { .. }
                | Event::LinkBorrow { .. } => ActivityStep::nothing(),
            },
        }
    }

    fn next_deadline(&self) -> Option<Millis> {
        match &self.phase {
            UpdatePhase::Starting {
                deadline,
                next_poke_at,
            }
            | UpdatePhase::BetweenLegs {
                deadline,
                next_poke_at,
                ..
            } => Some((*deadline).min(*next_poke_at)),
            UpdatePhase::Leg { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EffectId;
    use crate::identity::IdentityChain;
    use crate::link::{LinkCommand, LinkEvent, LinkId, LinkInfo};
    use crate::roster::RosterConfig;
    use crate::update_facts::{UpdateBoardState, UpdateFacts};

    /// The gap is never shorter than the Flash ladder's whole classic climb:
    /// the same re-enumeration, plus a core-only boot and Bluetooth.
    #[test]
    fn a_gap_is_never_shorter_than_the_flash_ladder() {
        let config = RosterConfig::default();
        assert!(UPDATE_GAP_MS >= 3 * config.flash_rung_ms);
        assert!(UPDATE_DEADLINE_MS > 3 * UPDATE_GAP_MS);
    }

    #[test]
    fn spawn_on_an_open_link_runs_the_first_leg() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = update(UpdateIntentFacts::Auto);
        let commands = with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(10), ctx)
        });
        assert!(matches!(
            commands.as_slice(),
            [Command::RunEffect {
                effect: EffectRequest::Update {
                    intent: UpdateIntentFacts::Auto
                },
                ..
            }]
        ));
        assert_eq!(activity.legs(), 1);
        assert_eq!(activity.next_deadline(), None, "the leg's effect drives");
    }

    #[test]
    fn spawn_on_a_closed_link_knocks_then_runs_the_leg_when_it_opens() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut activity = update(UpdateIntentFacts::Reinstall);
        let commands = with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(0), ctx)
        });
        assert!(matches!(
            commands.as_slice(),
            [Command::Link {
                command: LinkCommand::Open { .. },
                ..
            }]
        ));
        opened(&mut evidence, Millis(300), &config);
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(300),
                &link(LinkEvent::Opened {
                    info: LinkInfo::default(),
                }),
                ctx,
            )
        });
        assert!(
            matches!(step, ActivityStep::Continue(ref commands)
                if matches!(commands.as_slice(), [Command::RunEffect { .. }])),
            "{step:?}"
        );
    }

    #[test]
    fn a_bluetooth_gap_only_waits_for_the_provider_to_reconnect() {
        let config = RosterConfig::default();
        let evidence = Evidence::default();
        let mut activity = UpdateActivity::new(DeviceId(1), UpdateIntentFacts::Auto, true);
        let commands = with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(0), ctx)
        });
        assert!(
            commands.is_empty(),
            "no open fights the provider: {commands:?}"
        );
    }

    /// OTA M7 P12: the board's reset dropped its Bluetooth link mid-update;
    /// the provider reconnected and the sweep attached a new, closed link.
    /// The gap's next knock opens it (nothing else will while the activity
    /// holds the device), and the next leg starts when the board speaks.
    #[test]
    fn a_bluetooth_link_attached_between_legs_is_opened() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = UpdateActivity::new(DeviceId(1), UpdateIntentFacts::Auto, true);
        with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(10), ctx)
        });
        // The reset: the link vanishes under the leg.
        let mut evidence = Evidence::default();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(20),
                &Input::Event(Event::LinkDetached { link: LinkId(1) }),
                ctx,
            )
        });
        let attached = Event::LinkAttached {
            link: LinkId(2),
            info: LinkInfo::default(),
        };
        evidence.fold(
            Millis(900),
            &attached,
            &mut IdentityChain::default(),
            &config,
        );

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(2_000), &timer(), ctx)
        });

        assert!(
            matches!(step, ActivityStep::Continue(ref commands)
            if matches!(commands.as_slice(), [Command::Link {
                command: LinkCommand::Open { .. },
                ..
            }])),
            "the reconnected link is opened: {step:?}"
        );
    }

    /// …but the link that DROPPED is left to the provider's loop: its close
    /// is the gap's start, never after it, so the knock opens nothing.
    #[test]
    fn a_dropped_bluetooth_link_is_left_to_the_providers_loop() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = UpdateActivity::new(DeviceId(1), UpdateIntentFacts::Auto, true);
        with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(10), ctx)
        });
        // The drop: the link closes, then the leg ends on it.
        fold(
            &mut evidence,
            Millis(20),
            LinkEvent::Closed {
                reason: "bluetooth link lost: the board reset".to_string(),
            },
            &config,
        );
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(20),
                &ended(ActivityOutcome::Interrupted {
                    reason: "the board's link closed".to_string(),
                }),
                ctx,
            )
        });

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(2_000), &timer(), ctx)
        });

        assert!(
            matches!(step, ActivityStep::Continue(ref commands) if commands.is_empty()),
            "no open fights the provider's reconnect: {step:?}"
        );
    }

    #[test]
    fn stages_become_the_label_and_the_percent() {
        let mut activity = update(UpdateIntentFacts::Auto);
        assert_eq!(activity.label(), STARTING_LABEL);
        assert_eq!(activity.percent(), None);
        activity.stage = Some(UpdateStageFacts::Updating);
        activity.done = 400;
        activity.total = 1_000;
        assert_eq!(activity.label(), "Updating firmware…");
        assert_eq!(activity.percent(), Some(40));
    }

    #[test]
    fn an_outcome_before_the_end_ends_on_the_outcome() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = update(UpdateIntentFacts::Auto);
        with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(0), ctx)
        });
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(10),
                &marker(ActivityMarker::UpdateOutcome(UpdateOutcomeFacts::NeedsUsb)),
                ctx,
            )
        });
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(20),
                &ended(ActivityOutcome::Interrupted {
                    reason: "link closed".to_string(),
                }),
                ctx,
            )
        });
        assert!(matches!(
            step,
            ActivityStep::Done { outcome: ActivityOutcome::Failed { ref message }, .. }
                if message == UpdateOutcomeFacts::NeedsUsb.describe()
        ));
        assert_eq!(activity.outcome(), Some(UpdateOutcomeFacts::NeedsUsb));
    }

    /// The effects layer that cannot run a leg at all ends it with a plain
    /// failure: the activity ends there instead of looping on it.
    #[test]
    fn a_leg_that_could_not_run_ends_the_activity_rather_than_looping() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = update(UpdateIntentFacts::Auto);
        with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(0), ctx)
        });
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(5),
                &ended(ActivityOutcome::Failed {
                    message: "no update host".to_string(),
                }),
                ctx,
            )
        });
        assert!(matches!(
            step,
            ActivityStep::Done { outcome: ActivityOutcome::Failed { ref message }, .. }
                if message == "no update host"
        ));
        assert_eq!(activity.outcome(), None, "no typed outcome was reported");
    }

    /// A Bluetooth reconnect can beat the leg's own end marker: the link
    /// reopened and the board said `M` while the leg still ran. That newer
    /// window is the board back, so the next leg starts with no wait.
    #[test]
    fn a_link_that_reopened_during_the_leg_starts_the_next_leg_at_once() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = update(UpdateIntentFacts::Auto);
        with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(10), ctx)
        });
        opened(&mut evidence, Millis(500), &config);
        fold(
            &mut evidence,
            Millis(600),
            LinkEvent::UpdateFacts(UpdateFacts {
                state: UpdateBoardState::OnTrial,
                ..UpdateFacts::default()
            }),
            &config,
        );
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(700),
                &ended(ActivityOutcome::Interrupted {
                    reason: "link closed".to_string(),
                }),
                ctx,
            )
        });
        assert!(
            matches!(step, ActivityStep::Continue(ref commands)
                if matches!(commands.as_slice(), [Command::RunEffect { .. }])),
            "{step:?}"
        );
        assert_eq!(activity.legs(), 2);
    }

    /// The pre-reset hello in a window that survived the close is not the
    /// board coming back.
    #[test]
    fn a_hello_older_than_the_leg_never_starts_the_next_one() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        hello(&mut evidence, Millis(5), &config);
        let mut activity = update(UpdateIntentFacts::Auto);
        with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(10), ctx)
        });
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(1_000),
                &ended(ActivityOutcome::Interrupted {
                    reason: "reset".to_string(),
                }),
                ctx,
            )
        });
        assert!(
            matches!(step, ActivityStep::Continue(ref commands)
                if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::SendFrame(_), .. }])),
            "an open, quiet link is asked for a hello: {step:?}"
        );
        assert!(activity.view().between_legs);

        hello(&mut evidence, Millis(2_000), &config);
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(2_000), &timer(), ctx)
        });
        assert!(
            matches!(step, ActivityStep::Continue(ref commands)
                if matches!(commands.as_slice(), [Command::RunEffect { .. }])),
            "{step:?}"
        );
    }

    fn update(intent: UpdateIntentFacts) -> UpdateActivity {
        UpdateActivity::new(DeviceId(1), intent, false)
    }

    fn with_ctx<T>(
        evidence: &Evidence,
        config: &RosterConfig,
        body: impl FnOnce(&mut ActivityCtx<'_>) -> T,
    ) -> T {
        let mut ctx = ActivityCtx {
            link: Some(LinkId(1)),
            evidence,
            config,
            effect_id: EffectId(1),
        };
        body(&mut ctx)
    }

    fn link(event: LinkEvent) -> Input {
        Input::link(LinkId(1), event)
    }

    fn marker(marker: ActivityMarker) -> Input {
        Input::Event(Event::ActivityMarker {
            device: DeviceId(1),
            effect: Some(EffectId(1)),
            marker,
        })
    }

    fn ended(outcome: ActivityOutcome) -> Input {
        marker(ActivityMarker::Ended {
            kind: ActivityKind::Update,
            outcome,
        })
    }

    fn timer() -> Input {
        Input::Event(Event::TimerFired {
            timer: crate::time::TimerId {
                scope: crate::journal::Scope::Device(DeviceId(1)),
                seq: 1,
            },
        })
    }

    fn fold(evidence: &mut Evidence, now: Millis, event: LinkEvent, config: &RosterConfig) {
        let mut identity = IdentityChain::default();
        evidence.fold(
            now,
            &Event::Link {
                link: LinkId(1),
                event,
            },
            &mut identity,
            config,
        );
    }

    fn opened(evidence: &mut Evidence, now: Millis, config: &RosterConfig) {
        fold(
            evidence,
            now,
            LinkEvent::Opened {
                info: LinkInfo::default(),
            },
            config,
        );
    }

    fn hello(evidence: &mut Evidence, now: Millis, config: &RosterConfig) {
        fold(
            evidence,
            now,
            LinkEvent::Frame(crate::wire::ServerFrame::hello(
                1,
                crate::wire::HelloFacts {
                    proto: config.expected_proto,
                    ..Default::default()
                },
            )),
            config,
        );
    }
}
