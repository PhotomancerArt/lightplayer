//! The Remove-project activity: take what is on this board off it.
//!
//! The smallest of the coarse-effect activities, and shaped like
//! [`EraseActivity`](super::erase::EraseActivity): one
//! [`Command::RunEffect`], then an observation window that waits for the fold
//! to AGREE the board is empty before the card says so.
//!
//! It exists because of a dead end the bench walked into (G1, 2026-08-31): a
//! reflash preserves littlefs, so a board comes back auto-loading a project
//! from some previous life. The running face has no verbs for that, and
//! "factory reset" — the only escape that existed — throws the firmware away
//! too. Removing the project keeps the firmware and lands the card back on
//! the empty face, where the picker is.
//!
//! Three things the reducer knows, and nothing more:
//!
//! 1. The effect is running. WHICH dir it deletes is the board's own report,
//!    read inside the conversation — the model never names a path.
//! 2. When the effect ends well, ask the board what it has loaded now, so
//!    the empty face is drawn from EVIDENCE and not from our own optimism
//!    (I6). An `Empty` report settles it; silence still settles, honestly,
//!    because the delete itself was verified below the seam.
//! 3. A cancel during the conversation is HELD. The project is stopped and
//!    its dir is part-deleted by then; tearing out mid-delete would leave a
//!    board loading half a project. The push grace bounds the hold (I1/I2),
//!    and eviction is still the backstop.
//!
//! [`Command::RunEffect`]: crate::event::Command::RunEffect

use serde::{Deserialize, Serialize};

use crate::event::{Action, ActivityMarker, Command, EffectRequest, Event, Input};
use crate::identity::DeviceId;
use crate::link::{LinkCommand, LinkEvent};
use crate::time::Millis;
use crate::wire::ClientFrame;

use super::activity_cell::{
    ActivityCtx, ActivityKind, ActivityOutcome, ActivityReducer, ActivityStep,
};

/// Where the removal currently is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum RemovePhase {
    /// The coarse effect owns the wire; we absorb markers.
    Removing,
    /// The conversation is done. We asked the board what it has loaded, and
    /// give it this long to say — then settle anyway.
    Observing { deadline: Millis, summary: String },
}

/// The Remove-project reducer's own state. Everything it learns lives in the
/// fold.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemoveProjectActivity {
    device: DeviceId,
    phase: RemovePhase,
    /// Cancel arrived while the dir was being deleted: the wind-down waits
    /// for the effect to end rather than leaving half a project behind.
    cancel_after_effect: bool,
    /// Cancel wind-down in progress: the port was asked to close.
    winding_down: bool,
    next_request_id: u32,
}

impl RemoveProjectActivity {
    pub fn new(device: DeviceId) -> Self {
        Self {
            device,
            phase: RemovePhase::Removing,
            cancel_after_effect: false,
            winding_down: false,
            next_request_id: 1,
        }
    }

    /// The command that starts the coarse effect. Emitted by
    /// [`Device::spawn_remove_project`](crate::Device::spawn_remove_project)
    /// at spawn.
    pub(crate) fn spawn_commands(&self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        vec![Command::RunEffect {
            device: self.device,
            link,
            effect_id: ctx.effect_id,
            effect: EffectRequest::RemoveProject,
        }]
    }

    /// Start the wind-down: give the port back and wait for the close.
    fn wind_down(&mut self, ctx: &ActivityCtx<'_>) -> ActivityStep {
        self.winding_down = true;
        match ctx.link {
            Some(link) => ActivityStep::Continue(vec![Command::Link {
                link,
                command: LinkCommand::Close,
            }]),
            None => ActivityStep::done(ActivityOutcome::Cancelled),
        }
    }

    /// Ask the board what it is running now, so the fold re-observes the
    /// empty face rather than the reducer asserting it.
    ///
    /// Asked rather than waited for, exactly like the push's: the
    /// loaded-project fact also rides heartbeats, but a heartbeat period is
    /// five seconds on real firmware, and a card that says nothing for five
    /// seconds after a successful removal reads as a failure.
    fn ask_loaded(&mut self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        vec![Command::Link {
            link,
            command: LinkCommand::SendFrame(ClientFrame::list_loaded(request_id)),
        }]
    }

    /// Whether the board has now REPORTED an empty loaded list. `None` means
    /// it has not said — which is not "still running", and not "empty".
    fn reported_empty(ctx: &ActivityCtx<'_>) -> bool {
        ctx.evidence.loaded_projects().is_some_and(<[_]>::is_empty)
    }

    fn settled_summary(&self, summary: &str, ctx: &ActivityCtx<'_>) -> ActivityOutcome {
        match Self::reported_empty(ctx) {
            true => ActivityOutcome::Succeeded {
                summary: format!("{summary} — the board has nothing loaded"),
            },
            // Honest under-claim: the delete was verified over the wire and
            // the board simply has not reported since. Claiming the board is
            // empty is a claim no evidence supports yet.
            false => ActivityOutcome::Succeeded {
                summary: summary.to_string(),
            },
        }
    }

    fn handle_marker(
        &mut self,
        now: Millis,
        marker: &ActivityMarker,
        ctx: &mut ActivityCtx<'_>,
    ) -> ActivityStep {
        let ActivityMarker::Ended { outcome, .. } = marker else {
            // Progress lands on the cell (the device fold displays it);
            // Started brackets are the device's own.
            return ActivityStep::nothing();
        };
        match &self.phase {
            RemovePhase::Removing => {
                if self.cancel_after_effect {
                    return self.wind_down(ctx);
                }
                match outcome {
                    ActivityOutcome::Succeeded { summary } => {
                        self.phase = RemovePhase::Observing {
                            deadline: now.plus_ms(ctx.config.push_observe_ms),
                            summary: summary.clone(),
                        };
                        ActivityStep::Continue(self.ask_loaded(ctx))
                    }
                    other => ActivityStep::done(ActivityOutcome::Failed {
                        message: other.summary(),
                    }),
                }
            }
            // The conversation is over; a late marker changes nothing.
            RemovePhase::Observing { .. } => ActivityStep::nothing(),
        }
    }

    fn handle_timer(&mut self, now: Millis, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        if self.winding_down {
            return ActivityStep::nothing();
        }
        match self.phase.clone() {
            // The effect drives; supervision's deadline bounds it (I1).
            RemovePhase::Removing => ActivityStep::nothing(),
            RemovePhase::Observing { deadline, summary } => {
                if Self::reported_empty(ctx) || now >= deadline {
                    return ActivityStep::done(self.settled_summary(&summary, ctx));
                }
                ActivityStep::nothing()
            }
        }
    }

    fn handle_link_event(
        &mut self,
        now: Millis,
        event: &LinkEvent,
        ctx: &mut ActivityCtx<'_>,
    ) -> ActivityStep {
        match event {
            LinkEvent::Closed { .. } => {
                if self.winding_down {
                    return ActivityStep::done(ActivityOutcome::Cancelled);
                }
                ActivityStep::nothing()
            }
            LinkEvent::Frame(_) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                self.handle_timer(now, ctx)
            }
            LinkEvent::Opened { .. }
            | LinkEvent::Line(_)
            | LinkEvent::ResetOutcome { .. }
            | LinkEvent::Error(_) => ActivityStep::nothing(),
        }
    }
}

impl ActivityReducer for RemoveProjectActivity {
    fn kind(&self) -> ActivityKind {
        ActivityKind::RemoveProject
    }

    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match input {
            Input::Action(Action::CancelActivity { .. }) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                match self.phase {
                    // The dir is already part-deleted: stopping now would
                    // leave a board loading half a project. Hold the cancel;
                    // the push grace bounds the hold.
                    RemovePhase::Removing => {
                        self.cancel_after_effect = true;
                        ActivityStep::nothing()
                    }
                    // The dir is gone; the observation can stop politely.
                    RemovePhase::Observing { .. } => self.wind_down(ctx),
                }
            }
            Input::Action(_) => ActivityStep::nothing(),
            Input::Event(event) => match event {
                Event::ActivityMarker { marker, .. } => self.handle_marker(now, marker, ctx),
                Event::TimerFired { .. } => self.handle_timer(now, ctx),
                Event::Link { event, .. } => self.handle_link_event(now, event, ctx),
                Event::IdentityObserved { .. }
                | Event::LinkAttached { .. }
                | Event::LinkDetached { .. }
                | Event::LinkBorrow { .. } => ActivityStep::nothing(),
            },
        }
    }

    fn next_deadline(&self) -> Option<Millis> {
        if self.winding_down {
            return None;
        }
        match &self.phase {
            RemovePhase::Removing => None,
            RemovePhase::Observing { deadline, .. } => Some(*deadline),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EffectId;
    use crate::evidence::Evidence;
    use crate::identity::IdentityChain;
    use crate::link::LinkId;
    use crate::roster::RosterConfig;
    use crate::wire::{HelloFacts, LoadedProjectFacts, ServerFrame};

    fn removal() -> RemoveProjectActivity {
        RemoveProjectActivity::new(DeviceId(1))
    }

    fn ended(outcome: ActivityOutcome) -> Input {
        Input::Event(Event::ActivityMarker {
            device: DeviceId(1),
            effect: Some(EffectId(1)),
            marker: ActivityMarker::Ended {
                kind: ActivityKind::RemoveProject,
                outcome,
            },
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

    fn fold(evidence: &mut Evidence, now: Millis, event: Event, config: &RosterConfig) {
        let mut identity = IdentityChain::default();
        evidence.fold(now, &event, &mut identity, config);
    }

    /// A board that has said hello and reported one loaded project — the
    /// state the Remove verb is offered from.
    fn running(evidence: &mut Evidence, config: &RosterConfig) {
        fold(
            evidence,
            Millis(0),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Opened {
                    info: crate::link::LinkInfo::default(),
                },
            },
            config,
        );
        fold(
            evidence,
            Millis(1),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Frame(ServerFrame::hello(
                    1,
                    HelloFacts {
                        proto: config.expected_proto,
                        ..Default::default()
                    },
                )),
            },
            config,
        );
        reports(
            evidence,
            Millis(2),
            config,
            vec![LoadedProjectFacts::new("/projects/zook-dome")],
        );
    }

    fn reports(
        evidence: &mut Evidence,
        now: Millis,
        config: &RosterConfig,
        loaded: Vec<LoadedProjectFacts>,
    ) {
        fold(
            evidence,
            now,
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Frame(ServerFrame::heartbeat_with_loaded(None, loaded)),
            },
            config,
        );
    }

    #[test]
    fn spawn_emits_the_coarse_effect_stamped_with_the_steps_generation() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        running(&mut evidence, &config);
        let activity = removal();

        let commands = with_ctx(&evidence, &config, |ctx| activity.spawn_commands(ctx));

        assert!(
            matches!(
                commands.as_slice(),
                [Command::RunEffect {
                    effect: EffectRequest::RemoveProject,
                    effect_id: EffectId(1),
                    ..
                }]
            ),
            "{commands:?}"
        );
        assert_eq!(activity.next_deadline(), None, "the effect drives");
    }

    /// The walk: the conversation succeeds, the board is asked, and the
    /// activity settles on the EMPTY report — never on its own optimism.
    #[test]
    fn a_successful_removal_asks_the_board_and_settles_on_the_empty_report() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        running(&mut evidence, &config);
        let mut activity = removal();

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(1_000),
                &ended(ActivityOutcome::Succeeded {
                    summary: "removed zook-dome".to_string(),
                }),
                ctx,
            )
        });
        assert!(
            matches!(
                step,
                ActivityStep::Continue(ref commands)
                    if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::SendFrame(_), .. }])
            ),
            "{step:?}"
        );

        reports(&mut evidence, Millis(1_100), &config, Vec::new());
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(1_100), &timer(), ctx)
        });

        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ..
                } if summary.contains("zook-dome") && summary.contains("nothing loaded")
            ),
            "{step:?}"
        );
    }

    /// The board never reports back. The removal still SUCCEEDED — the
    /// delete was verified over the wire — so the outcome says exactly that
    /// and claims nothing about what the board holds now.
    #[test]
    fn a_silent_board_still_leaves_the_removal_honestly_successful() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        running(&mut evidence, &config);
        let mut activity = removal();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(0),
                &ended(ActivityOutcome::Succeeded {
                    summary: "removed zook-dome".to_string(),
                }),
                ctx,
            )
        });

        let at = Millis(config.push_observe_ms + 1);
        let step = with_ctx(&evidence, &config, |ctx| activity.handle(at, &timer(), ctx));

        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ..
                } if summary == "removed zook-dome"
            ),
            "{step:?}"
        );
    }

    #[test]
    fn a_failed_conversation_ends_the_activity_with_the_effects_message() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        running(&mut evidence, &config);
        let mut activity = removal();

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(500),
                &ended(ActivityOutcome::Failed {
                    message: "the board refused to delete /projects/zook-dome".to_string(),
                }),
                ctx,
            )
        });

        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Failed { ref message },
                    ..
                } if message.contains("refused to delete")
            ),
            "{step:?}"
        );
    }

    #[test]
    fn cancel_during_the_delete_is_held_then_honoured_when_the_effect_ends() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        running(&mut evidence, &config);
        let mut activity = removal();

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(100),
                &Input::Action(Action::CancelActivity {
                    device: DeviceId(1),
                }),
                ctx,
            )
        });
        assert_eq!(
            step,
            ActivityStep::nothing(),
            "the dir is already part-deleted"
        );

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(5_000),
                &ended(ActivityOutcome::Succeeded {
                    summary: "removed".to_string(),
                }),
                ctx,
            )
        });
        assert!(
            matches!(
                step,
                ActivityStep::Continue(ref commands)
                    if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::Close, .. }])
            ),
            "{step:?}"
        );

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(5_100),
                &Input::link(
                    LinkId(1),
                    LinkEvent::Closed {
                        reason: "cancelled".to_string(),
                    },
                ),
                ctx,
            )
        });
        assert!(matches!(
            step,
            ActivityStep::Done {
                outcome: ActivityOutcome::Cancelled,
                ..
            }
        ));
    }
}
