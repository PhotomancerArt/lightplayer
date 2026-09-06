//! The Identify activity: "what is on the other end of this link?"
//!
//! It mirrors the shipped hello-gate semantics
//! (`lpa-link/src/device_session/device_readiness.rs`) with one deliberate
//! difference: **no sticky verdicts.**
//!
//! What carries over:
//!
//! - Readiness is granted by exactly one thing — a hello (on any wire proto),
//!   either the unsolicited boot hello or the answer to our own hello
//!   request. A connect cannot assume the power to cause a boot, so asking
//!   is mandatory (`docs/defects/2026-08-21-hello-gate-assumes-fresh-boot.md`).
//! - Non-hello frames are **absorbed** as live-peer evidence and counted,
//!   never condemned on sight. Frames-but-no-hello is a verdict the
//!   *deadline* reaches, not the first frame.
//! - Boot lines are diagnosis, not readiness: blank flash, ROM download
//!   mode, and known replaceable firmware explain why no hello came.
//!
//! What changes: the verdict is not a state this reducer stores. The fold
//! owns classification and recomputes it from the current window, so a board
//! that boots noisily and *then* hellos ends up a LightPlayer instead of a
//! permanently blank chip, and a reset re-opens the question. This reducer
//! only decides *when* the question is settled and what the outcome line
//! says.

use serde::{Deserialize, Serialize};

use crate::event::{Action, Command, Event, Input};
use crate::evidence::{Classification, IncompatibleReason, WireVersion};
use crate::link::{LinkCommand, LinkEvent};
use crate::time::Millis;
use crate::wire::ClientFrame;

use super::activity_cell::{
    ActivityCtx, ActivityKind, ActivityOutcome, ActivityReducer, ActivityStep,
};

/// Identify's own state: a cadence and a settle time. Everything it learns
/// lives in the fold.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IdentifyActivity {
    /// When the question is answered one way or the other.
    settle_at: Millis,
    /// When to re-ask for a hello.
    next_ask_at: Millis,
    ask_interval_ms: u64,
    asks_sent: u32,
    next_request_id: u32,
    /// Cancel requested: the port was asked to close and we are waiting for
    /// it. Supervision bounds this wait.
    winding_down: bool,
}

impl IdentifyActivity {
    pub fn new(started_at: Millis, settle_at: Millis, ask_interval_ms: u64) -> Self {
        Self {
            settle_at,
            next_ask_at: started_at.plus_ms(ask_interval_ms),
            ask_interval_ms,
            asks_sent: 0,
            next_request_id: 1,
            winding_down: false,
        }
    }

    /// Commands to run at spawn: open the port if it is not open, otherwise
    /// start asking immediately.
    pub(crate) fn spawn_commands(&mut self, ctx: &mut ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        if ctx.evidence.presence.is_open() {
            return vec![self.ask(link, ctx)];
        }
        vec![Command::Link {
            link,
            command: LinkCommand::Open {
                baud: ctx.config.open_baud,
            },
        }]
    }

    /// How many hello requests have been sent in this run.
    pub fn asks_sent(&self) -> u32 {
        self.asks_sent
    }

    fn ask(&mut self, link: crate::link::LinkId, _ctx: &mut ActivityCtx<'_>) -> Command {
        // Request ids are the model's own counter. M3's adapter re-stamps
        // them with `lpa-client`'s allocator, which owns correlation.
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        self.asks_sent += 1;
        Command::Link {
            link,
            command: LinkCommand::SendFrame(ClientFrame::hello(request_id)),
        }
    }

    /// Settle immediately on a ROM-conclusive classification, handing the
    /// port back in the same step (see the `Line` arm for why waiting for
    /// the settle window loses the race against a boot-looping board).
    fn settle_on_rom_verdict(&self, summary: &str, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let commands = match ctx.link {
            Some(link) => vec![Command::Link {
                link,
                command: LinkCommand::Close,
            }],
            None => Vec::new(),
        };
        ActivityStep::Done {
            outcome: ActivityOutcome::Succeeded {
                summary: summary.to_string(),
            },
            commands,
        }
    }

    fn settle(&self, now: Millis, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let outcome = match ctx.evidence.verdict_if_settled(now) {
            // A LightPlayer on ANOTHER wire version is still the happy
            // verdict; the outcome line says so in passing, and the fold
            // has already journaled the numbers.
            Classification::LightPlayer { hello } => ActivityOutcome::Succeeded {
                summary: match ctx.evidence.wire_version() {
                    Some(WireVersion::BoardOlder { .. }) => {
                        format!("{} (older firmware than Studio)", hello.label())
                    }
                    Some(WireVersion::BoardNewer { .. }) => {
                        format!("{} (newer firmware than Studio)", hello.label())
                    }
                    Some(WireVersion::Match) | None => hello.label(),
                },
            },
            Classification::Incompatible {
                reason: IncompatibleReason::NoHello,
            } => ActivityOutcome::Succeeded {
                summary: "speaks the framing but never said hello (pre-hello firmware)".to_string(),
            },
            Classification::Blank => ActivityOutcome::Succeeded {
                summary: "blank or erased flash".to_string(),
            },
            Classification::Bootloader => ActivityOutcome::Succeeded {
                summary: "waiting in ROM download mode".to_string(),
            },
            Classification::Foreign { label: Some(label) } => {
                ActivityOutcome::Succeeded { summary: label }
            }
            Classification::Foreign { label: None } => ActivityOutcome::Succeeded {
                summary: "unrecognized firmware".to_string(),
            },
            // Identification "succeeds" whenever it produces a verdict —
            // even an unwelcome one. Silence is the only failure.
            Classification::Quiet { .. } | Classification::Unknown => ActivityOutcome::Failed {
                message: "no response from the device".to_string(),
            },
        };
        ActivityStep::done(outcome)
    }
}

impl ActivityReducer for IdentifyActivity {
    fn kind(&self) -> ActivityKind {
        ActivityKind::Identify
    }

    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match input {
            Input::Action(Action::CancelActivity { .. }) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                self.winding_down = true;
                // Wind-down = give the port back. If the transport never
                // reports the close (a wedged port — the real-world case),
                // supervision evicts us when the grace expires.
                match ctx.link {
                    Some(link) => ActivityStep::Continue(vec![Command::Link {
                        link,
                        command: LinkCommand::Close,
                    }]),
                    None => ActivityStep::done(ActivityOutcome::Cancelled),
                }
            }
            Input::Action(_) => ActivityStep::nothing(),
            Input::Event(event) => self.handle_event(now, event, ctx),
        }
    }

    fn next_deadline(&self) -> Option<Millis> {
        if self.winding_down {
            // Nothing left to do on a tick; the cancel grace bounds us now.
            return None;
        }
        Some(self.settle_at.min(self.next_ask_at))
    }
}

impl IdentifyActivity {
    fn handle_event(
        &mut self,
        now: Millis,
        event: &Event,
        ctx: &mut ActivityCtx<'_>,
    ) -> ActivityStep {
        match event {
            Event::Link { event, .. } => self.handle_link_event(event, ctx),
            Event::TimerFired { .. } => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                if now >= self.settle_at {
                    return self.settle(now, ctx);
                }
                if now >= self.next_ask_at {
                    self.next_ask_at = now.plus_ms(self.ask_interval_ms);
                    if let Some(link) = ctx.link {
                        if ctx.evidence.presence.is_open() {
                            return ActivityStep::Continue(vec![self.ask(link, ctx)]);
                        }
                    }
                }
                ActivityStep::nothing()
            }
            // Link loss is supervision's business (it evicts and recovers),
            // not a verdict this reducer should invent; identity news and
            // the wire borrow are the fold's.
            Event::LinkAttached { .. }
            | Event::LinkDetached { .. }
            | Event::LinkBorrow { .. }
            | Event::ActivityMarker { .. }
            | Event::IdentityObserved { .. } => ActivityStep::nothing(),
        }
    }

    fn handle_link_event(&mut self, event: &LinkEvent, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match event {
            LinkEvent::Opened { .. } => match ctx.link {
                Some(link) if !self.winding_down => {
                    ActivityStep::Continue(vec![self.ask(link, ctx)])
                }
                _ => ActivityStep::nothing(),
            },
            LinkEvent::Closed { reason } => {
                if self.winding_down {
                    return ActivityStep::done(ActivityOutcome::Cancelled);
                }
                ActivityStep::done(ActivityOutcome::Interrupted {
                    reason: reason.clone(),
                })
            }
            // A hello — ours or the boot one — is the whole answer. Wrong
            // proto is also an answer. Everything else waits.
            LinkEvent::Frame(_) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                if ctx.evidence.has_hello() {
                    return self.settle(self.settle_at, ctx);
                }
                ActivityStep::nothing()
            }
            // A conclusive ROM verdict settles NOW, not at the deadline. An
            // erased native-USB chip boot-loops, flooding its signature
            // hundreds of times a second and watchdog-resetting (which
            // re-enumerates USB) every few seconds — waiting the settle
            // window out LOSES that race, ending every cycle Interrupted
            // (bench 2026-08-30: 49 adoption cycles in a row on a wiped
            // C6). The port goes back in the same step: a blank board has
            // nothing more to say, and listening only floods the journal.
            LinkEvent::Line(_) if !self.winding_down => match ctx.evidence.classification {
                Classification::Blank => self.settle_on_rom_verdict("blank or erased flash", ctx),
                Classification::Bootloader => {
                    self.settle_on_rom_verdict("waiting in ROM download mode", ctx)
                }
                _ => ActivityStep::nothing(),
            },
            // Boot lines (pre-verdict), errors and reset outcomes are
            // diagnosis: the fold has already recorded them and the
            // deadline decides.
            LinkEvent::Line(_) | LinkEvent::Error(_) | LinkEvent::ResetOutcome { .. } => {
                ActivityStep::nothing()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::Evidence;
    use crate::identity::{DeviceId, IdentityChain};
    use crate::link::{LinkId, LinkInfo};
    use crate::roster::RosterConfig;
    use crate::wire::{HelloFacts, ServerFrame};

    #[test]
    fn spawning_on_a_closed_port_opens_it_then_asks_on_open() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);

        let commands = with_ctx(&evidence, &config, |ctx| identify.spawn_commands(ctx));
        assert!(matches!(
            commands.as_slice(),
            [Command::Link {
                command: LinkCommand::Open { .. },
                ..
            }]
        ));
        assert_eq!(identify.asks_sent(), 0);

        fold(&mut evidence, Millis(50), opened(), &config);
        let step = with_ctx(&evidence, &config, |ctx| {
            identify.handle(Millis(50), &Input::link(LinkId(1), opened_event()), ctx)
        });

        assert!(matches!(
            step,
            ActivityStep::Continue(ref commands)
                if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::SendFrame(_), .. }])
        ));
        assert_eq!(identify.asks_sent(), 1);
    }

    #[test]
    fn a_heartbeat_first_then_the_hello_answer_settles_as_light_player() {
        // The shipped defect as a reducer test: the mid-stream attach.
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);
        fold(&mut evidence, Millis(0), opened(), &config);

        let heartbeat = Event::Link {
            link: LinkId(1),
            event: LinkEvent::Frame(ServerFrame::heartbeat(None)),
        };
        fold(&mut evidence, Millis(100), heartbeat.clone(), &config);
        let step = with_ctx(&evidence, &config, |ctx| {
            identify.handle(Millis(100), &Input::Event(heartbeat), ctx)
        });
        assert_eq!(step, ActivityStep::nothing(), "absorbed, not condemned");

        let hello = Event::Link {
            link: LinkId(1),
            event: LinkEvent::Frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    board_id: Some("dig-uno".to_string()),
                    ..Default::default()
                },
            )),
        };
        fold(&mut evidence, Millis(200), hello.clone(), &config);
        let step = with_ctx(&evidence, &config, |ctx| {
            identify.handle(Millis(200), &Input::Event(hello), ctx)
        });

        assert!(matches!(
            step,
            ActivityStep::Done {
                outcome: ActivityOutcome::Succeeded { ref summary },
                ..
            } if summary.contains("dig-uno")
        ));
    }

    #[test]
    fn silence_to_the_deadline_fails_but_boot_noise_is_a_verdict() {
        let config = RosterConfig::default();
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);

        let mut silent = Evidence::default();
        fold(&mut silent, Millis(0), opened(), &config);
        let step = with_ctx(&silent, &config, |ctx| {
            identify.handle(Millis(5_000), &Input::Event(timer()), ctx)
        });
        assert!(matches!(
            step,
            ActivityStep::Done {
                outcome: ActivityOutcome::Failed { .. },
                ..
            }
        ));

        let mut blank = Evidence::default();
        fold(&mut blank, Millis(0), opened(), &config);
        fold(
            &mut blank,
            Millis(10),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Line("invalid header: 0xffffffff".to_string()),
            },
            &config,
        );
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);
        let step = with_ctx(&blank, &config, |ctx| {
            identify.handle(Millis(5_000), &Input::Event(timer()), ctx)
        });
        assert!(matches!(
            step,
            ActivityStep::Done {
                outcome: ActivityOutcome::Succeeded { ref summary },
                ..
            } if summary.contains("blank")
        ));
    }

    #[test]
    fn cancel_asks_for_the_port_back_and_completes_when_it_closes() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        fold(&mut evidence, Millis(0), opened(), &config);
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);

        let step = with_ctx(&evidence, &config, |ctx| {
            identify.handle(
                Millis(500),
                &Input::Action(Action::CancelActivity {
                    device: DeviceId(1),
                }),
                ctx,
            )
        });
        assert!(matches!(
            step,
            ActivityStep::Continue(ref commands)
                if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::Close, .. }])
        ));
        assert!(
            identify.next_deadline().is_none(),
            "the grace bounds us now"
        );

        let step = with_ctx(&evidence, &config, |ctx| {
            identify.handle(
                Millis(600),
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

    #[test]
    fn the_re_ask_cadence_keeps_asking_until_the_settle_time() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        fold(&mut evidence, Millis(0), opened(), &config);
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);

        assert_eq!(identify.next_deadline(), Some(Millis(1_000)));
        for at in [1_000_u64, 2_000, 3_000] {
            let step = with_ctx(&evidence, &config, |ctx| {
                identify.handle(Millis(at), &Input::Event(timer()), ctx)
            });
            assert!(matches!(step, ActivityStep::Continue(ref c) if c.len() == 1));
        }
        assert_eq!(identify.asks_sent(), 3);
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
            effect_id: crate::event::EffectId(1),
        };
        body(&mut ctx)
    }

    fn fold(evidence: &mut Evidence, now: Millis, event: Event, config: &RosterConfig) {
        let mut identity = IdentityChain::default();
        evidence.fold(now, &event, &mut identity, config);
    }

    fn opened() -> Event {
        Event::Link {
            link: LinkId(1),
            event: opened_event(),
        }
    }

    fn opened_event() -> LinkEvent {
        LinkEvent::Opened {
            info: LinkInfo::default(),
        }
    }

    fn timer() -> Event {
        Event::TimerFired {
            timer: crate::time::TimerId {
                scope: crate::journal::Scope::Device(DeviceId(1)),
                seq: 1,
            },
        }
    }
}
