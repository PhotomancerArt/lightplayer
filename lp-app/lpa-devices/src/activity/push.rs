//! The Push activity: put a project on this board.
//!
//! Shaped exactly like [`FlashActivity`](super::flash::FlashActivity), and
//! for the same reason: the work is a **coarse effect**. The reducer emits
//! one [`Command::RunEffect`] and then absorbs markers — it never touches the
//! wire while the effect owns it, and it never awaits (I7).
//!
//! What the effect does below the model is the `lpa-client` conversation the
//! studio already trusts (`project_deploy.rs`'s stop → write → load order,
//! `file_sync_ops.rs`'s chunking, then the package-hash verification
//! `open_library_project` performs). The reducer deliberately knows none of
//! it. It knows three things:
//!
//! 1. The effect is running (or was never able to start, which arrives as an
//!    immediate failure marker — a project the app could not prepare and a
//!    device that refused the write land on the SAME honest face).
//! 2. When the effect ends well, ask the board what it is running now, so
//!    the running face is drawn from EVIDENCE rather than from our own
//!    optimism (I6). The ask is best-effort: the push already succeeded, and
//!    a board that has not answered yet is not a failed push.
//! 3. A cancel during the write window is HELD. The conversation clears the
//!    device's project dir before it writes the replacement, so tearing out
//!    mid-write leaves half a project on the board. The card says
//!    "cancelling", the conversation finishes, and the wind-down happens
//!    instead of the observation. Supervision's push grace bounds the hold
//!    (I1/I2), and eviction is still the backstop.
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

/// Where the push currently is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum PushPhase {
    /// The coarse effect owns the wire; we absorb markers.
    Sending,
    /// The conversation is done. We asked the board what it has loaded, and
    /// give it this long to say — then settle anyway.
    Observing { deadline: Millis, summary: String },
}

/// The Push reducer's own state. Everything it learns lives in the fold.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PushActivity {
    device: DeviceId,
    phase: PushPhase,
    /// Cancel arrived during the write window: the device's project dir is
    /// already cleared, so the wind-down waits for the effect to end.
    cancel_after_effect: bool,
    /// Cancel wind-down in progress: the port was asked to close.
    winding_down: bool,
    next_request_id: u32,
}

impl PushActivity {
    pub fn new(device: DeviceId) -> Self {
        Self {
            device,
            phase: PushPhase::Sending,
            cancel_after_effect: false,
            winding_down: false,
            next_request_id: 1,
        }
    }

    /// The command that starts the coarse effect. Emitted by
    /// [`Device::spawn_push`](crate::Device::spawn_push) at spawn.
    pub(crate) fn spawn_commands(&self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        vec![Command::RunEffect {
            device: self.device,
            link,
            effect_id: ctx.effect_id,
            effect: EffectRequest::Push,
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

    /// Ask the board what it is running now, so the fold re-observes rather
    /// than the reducer assuming.
    ///
    /// Asked rather than waited for: the loaded-project fact also rides
    /// heartbeats, but a heartbeat period is five seconds on real firmware,
    /// and a card that says nothing for five seconds after a successful push
    /// reads as a failure.
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

    /// The board reported a loaded project: the push is provably on it.
    fn settled_summary(&self, summary: &str, ctx: &ActivityCtx<'_>) -> ActivityOutcome {
        match ctx
            .evidence
            .loaded_projects()
            .and_then(|loaded| loaded.first())
        {
            Some(project) => ActivityOutcome::Succeeded {
                summary: format!("{summary} — the board is running {}", project.label()),
            },
            // Honest under-claim: the write and the hash check both passed,
            // and the board simply has not reported yet. Saying it is
            // running would be a claim no evidence supports.
            None => ActivityOutcome::Succeeded {
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
            PushPhase::Sending => {
                if self.cancel_after_effect {
                    return self.wind_down(ctx);
                }
                match outcome {
                    ActivityOutcome::Succeeded { summary } => {
                        self.phase = PushPhase::Observing {
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
            PushPhase::Observing { .. } => ActivityStep::nothing(),
        }
    }

    fn handle_timer(&mut self, now: Millis, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        if self.winding_down {
            return ActivityStep::nothing();
        }
        match self.phase.clone() {
            // The effect drives; supervision's deadline bounds it (I1).
            PushPhase::Sending => ActivityStep::nothing(),
            PushPhase::Observing { deadline, summary } => {
                // A NON-EMPTY report, or the deadline. An empty one is
                // usually the board's last word from before the push — the
                // observation window is not reset by a push, because the
                // port never closed — so treating it as an answer would
                // settle on stale evidence.
                let reported = ctx
                    .evidence
                    .loaded_projects()
                    .is_some_and(|loaded| !loaded.is_empty());
                if reported || now >= deadline {
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

impl ActivityReducer for PushActivity {
    fn kind(&self) -> ActivityKind {
        ActivityKind::Push
    }

    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match input {
            Input::Action(Action::CancelActivity { .. }) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                match self.phase {
                    // The project dir on the device is already cleared:
                    // stopping now would leave half a project there. Hold
                    // the cancel; the push grace bounds the hold.
                    PushPhase::Sending => {
                        self.cancel_after_effect = true;
                        ActivityStep::nothing()
                    }
                    // The bytes are down and verified; the observation can
                    // stop politely right now.
                    PushPhase::Observing { .. } => self.wind_down(ctx),
                }
            }
            Input::Action(_) => ActivityStep::nothing(),
            Input::Event(event) => match event {
                Event::ActivityMarker { marker, .. } => self.handle_marker(now, marker, ctx),
                Event::TimerFired { .. } => self.handle_timer(now, ctx),
                Event::Link { event, .. } => self.handle_link_event(now, event, ctx),
                Event::IdentityObserved { .. }
                | Event::LinkAttached { .. }
                | Event::LinkDetached { .. } => ActivityStep::nothing(),
            },
        }
    }

    fn next_deadline(&self) -> Option<Millis> {
        if self.winding_down {
            return None;
        }
        match &self.phase {
            PushPhase::Sending => None,
            PushPhase::Observing { deadline, .. } => Some(*deadline),
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

    fn push() -> PushActivity {
        PushActivity::new(DeviceId(1))
    }

    fn ended(outcome: ActivityOutcome) -> Input {
        Input::Event(Event::ActivityMarker {
            device: DeviceId(1),
            effect: Some(EffectId(1)),
            marker: ActivityMarker::Ended {
                kind: ActivityKind::Push,
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

    fn opened(evidence: &mut Evidence, config: &RosterConfig) {
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
    }

    fn heartbeat_reports(
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
        opened(&mut evidence, &config);
        let activity = push();

        let commands = with_ctx(&evidence, &config, |ctx| activity.spawn_commands(ctx));

        assert!(
            matches!(
                commands.as_slice(),
                [Command::RunEffect {
                    effect: EffectRequest::Push,
                    effect_id: EffectId(1),
                    ..
                }]
            ),
            "{commands:?}"
        );
        assert_eq!(activity.next_deadline(), None, "the effect drives");
    }

    #[test]
    fn a_successful_push_asks_the_board_and_settles_on_what_it_reports() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, &config);
        let mut activity = push();

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(1_000),
                &ended(ActivityOutcome::Succeeded {
                    summary: "sent porch-sign".to_string(),
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

        heartbeat_reports(
            &mut evidence,
            Millis(1_100),
            &config,
            vec![LoadedProjectFacts::new("/projects/demo")],
        );
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(1_100), &timer(), ctx)
        });

        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ..
                } if summary.contains("sent porch-sign") && summary.contains("demo")
            ),
            "{step:?}"
        );
    }

    /// The board never reports back. The push still SUCCEEDED — the write
    /// and the hash check both passed — so the outcome says exactly that
    /// and claims nothing about what is running.
    #[test]
    fn a_silent_board_still_leaves_the_push_honestly_successful() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, &config);
        let mut activity = push();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(0),
                &ended(ActivityOutcome::Succeeded {
                    summary: "sent porch-sign".to_string(),
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
                } if summary == "sent porch-sign"
            ),
            "{step:?}"
        );
    }

    /// A project the app could not prepare and a device that refused the
    /// write arrive the same way, and both land on the problem face.
    #[test]
    fn a_failed_effect_ends_the_activity_with_the_effects_message() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, &config);
        let mut activity = push();

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(500),
                &ended(ActivityOutcome::Failed {
                    message: "pushed project hash mismatch".to_string(),
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
                } if message.contains("hash mismatch")
            ),
            "{step:?}"
        );
    }

    #[test]
    fn cancel_during_the_write_is_held_then_honoured_when_the_effect_ends() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, &config);
        let mut activity = push();

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
            "the device's project dir is already cleared"
        );

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(5_000),
                &ended(ActivityOutcome::Succeeded {
                    summary: "sent".to_string(),
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
