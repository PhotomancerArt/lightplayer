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
use crate::firmware_age::FirmwareAge;
use crate::link::{LinkCommand, LinkEvent, ResetKind};
use crate::time::Millis;
use crate::wire::ClientFrame;

use super::activity_cell::{
    ActivityCtx, ActivityKind, ActivityOutcome, ActivityReducer, ActivityStep,
};

/// Identify's outcome line when another tab of this browser holds the port
/// ([`Event::LinkHeld`]).
pub const HELD_BY_ANOTHER_TAB: &str = "held by another tab";

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
    /// The window already holds the board's hello: settle on it, at the
    /// first tick, and ask the board for nothing more ([`Self::on_hello_in_hand`]).
    #[serde(default)]
    hello_in_hand: bool,
    /// A blank verdict that named no chip may reset the board, once, so its
    /// ROM banner is heard ([`Self::resetting_for_the_banner`]). Off unless
    /// the device asks for it: a board whose chip is already known, or one
    /// with no reset lines (a network link), is never reset for it.
    #[serde(default)]
    banner_reset_allowed: bool,
    /// That reset was sent in this run. One per run, whatever it heard.
    #[serde(default)]
    banner_reset_sent: bool,
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
            hello_in_hand: false,
            banner_reset_allowed: false,
            banner_reset_sent: false,
        }
    }

    /// Let a blank verdict that named no chip reset the board once, so the
    /// ROM's banner (`ESP-ROM:esp32c6-…`) names it.
    ///
    /// The banner prints once per boot, before anything else. A blank chip
    /// then floods "invalid header" until its watchdog resets it, so a port
    /// opened mid-loop (a phone's WebUSB grant, a page opened late) hears
    /// the flood and never the banner, and the install picker offered every
    /// board kind there is
    /// (docs/defects/2026-10-25-a-blank-board-opened-mid-boot-loop-offered-every-board.md).
    /// The reset is the same pulse the card's Reset verb sends: the board
    /// has no firmware, so it loses nothing, and the chip's own reset keeps
    /// its USB (it does not re-enumerate).
    pub fn with_banner_reset(mut self, allowed: bool) -> Self {
        self.banner_reset_allowed = allowed;
        self
    }

    /// An identify for a window that already holds the board's hello (a
    /// hello that came after identify settled without one,
    /// `Device::late_hello_reidentifies`): it asks for **no** hello, and
    /// settles on the one in hand at its first tick, asking what the board
    /// runs as any LightPlayer verdict does.
    ///
    /// Not asking is the point. A hello the board sends in answer is a new
    /// `hello_at` in the same window, and everything that is once per
    /// connection is keyed on it (`lpa-studio-core`'s `LoginWindow`: the
    /// access list read, the login, the account's board report, the restart
    /// watch; the network panel; the card's picture feed) — an asked hello
    /// repeated each of them on the same connection
    /// (docs/defects/2026-10-23-a-re-identify-reported-the-board-to-the-account-twice.md).
    pub fn on_hello_in_hand(started_at: Millis, ask_interval_ms: u64) -> Self {
        Self {
            settle_at: started_at,
            hello_in_hand: true,
            ..Self::new(started_at, started_at, ask_interval_ms)
        }
    }

    /// Commands to run at spawn: open the port if it is not open, otherwise
    /// start asking immediately.
    pub(crate) fn spawn_commands(&mut self, ctx: &mut ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        if self.hello_in_hand && ctx.evidence.has_hello() && ctx.evidence.presence.is_open() {
            return Vec::new();
        }
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

    /// Whether this blank verdict should first reset the board for its
    /// banner: allowed, not sent yet this run, and no chip heard.
    fn wants_banner(&self, ctx: &ActivityCtx<'_>) -> bool {
        self.banner_reset_allowed
            && !self.banner_reset_sent
            && ctx.evidence.detected_chip().is_none()
    }

    /// Reset the board so its ROM prints the banner, and keep listening:
    /// the reset starts a fresh window, the banner names the chip, and the
    /// boot loop's next "invalid header" settles the verdict with it. If
    /// the banner is lost anyway (a phone drops bytes), the next blank line
    /// settles without it: the reset is sent once per run.
    fn resetting_for_the_banner(&mut self, ctx: &ActivityCtx<'_>) -> ActivityStep {
        self.banner_reset_sent = true;
        match ctx.link {
            Some(link) => ActivityStep::Continue(vec![Command::Link {
                link,
                command: LinkCommand::RunReset(ResetKind::Normal),
            }]),
            None => self.settle_on_rom_verdict("blank or erased flash", ctx),
        }
    }

    /// A ROM verdict the deadline reached (the board went quiet after the
    /// banner reset) settles as one the line did: the port goes back.
    fn rom_verdict(&self, ctx: &ActivityCtx<'_>) -> Option<&'static str> {
        match ctx.evidence.classification {
            Classification::Blank => Some("blank or erased flash"),
            Classification::Bootloader => Some("waiting in ROM download mode"),
            _ => None,
        }
    }

    fn settle(&self, now: Millis, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let verdict = ctx.evidence.verdict_if_settled(now);
        let light_player = matches!(verdict, Classification::LightPlayer { .. });
        let outcome = match verdict {
            // A LightPlayer on ANOTHER wire version is still the happy
            // verdict; the outcome line says so in passing, and the fold
            // has already journaled the numbers.
            // The version leads; the wire proto speaks only when neither
            // side's version can be read.
            Classification::LightPlayer { hello } => ActivityOutcome::Succeeded {
                summary: match (
                    ctx.evidence.firmware_age().unwrap_or_default(),
                    ctx.evidence.wire_version(),
                ) {
                    (FirmwareAge::Older, _)
                    | (FirmwareAge::Unknown, Some(WireVersion::BoardOlder { .. })) => {
                        format!("{} (older firmware than Studio)", hello.label())
                    }
                    (FirmwareAge::Newer, _)
                    | (FirmwareAge::Unknown, Some(WireVersion::BoardNewer { .. })) => {
                        format!("{} (newer firmware than Studio)", hello.label())
                    }
                    (FirmwareAge::Different, _) => {
                        format!("{} (not this Studio's build)", hello.label())
                    }
                    (FirmwareAge::Current, _)
                    | (FirmwareAge::Unknown, Some(WireVersion::Match) | None) => hello.label(),
                },
            },
            Classification::Incompatible {
                reason: IncompatibleReason::NoHello,
            } => ActivityOutcome::Succeeded {
                summary: "speaks the framing but never said hello (pre-hello firmware)".to_string(),
            },
            Classification::OlderLightPlayer { .. } => ActivityOutcome::Succeeded {
                summary: "older LightPlayer firmware, from before this Studio's link".to_string(),
            },
            Classification::CoreOnly { version, state } => ActivityOutcome::Succeeded {
                summary: format!(
                    "LightPlayer {} running its core only ({})",
                    version.as_deref().unwrap_or("(version unknown)"),
                    state.describe()
                ),
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
        let commands = match light_player {
            true => self.ask_what_it_runs(ctx).into_iter().collect(),
            false => Vec::new(),
        };
        ActivityStep::Done { outcome, commands }
    }

    /// A LightPlayer that has not said what it runs is asked, once, as the
    /// identify ends. Firmware volunteers it in every heartbeat, but an
    /// embedder that sends none (the browser sim) only ever answers a
    /// `ListLoadedProjects`, and without one its card would say "not
    /// known" forever and never offer a project (the board card's feel
    /// gate, finding 2). The answer is folded like a heartbeat's report.
    fn ask_what_it_runs(&self, ctx: &ActivityCtx<'_>) -> Option<Command> {
        let link = ctx.link?;
        let unsaid = ctx.evidence.loaded_projects().is_none();
        (unsaid && ctx.evidence.presence.is_open()).then(|| Command::Link {
            link,
            command: LinkCommand::SendFrame(ClientFrame::list_loaded(self.next_request_id)),
        })
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
            // Another tab holds the port: there is nothing to wait for and
            // nothing to re-ask. The port was never opened (or the OS refused
            // it), so there is no port to hand back either. The device does
            // not retry this ending (see `Device::apply_step`).
            Event::LinkHeld { .. } => ActivityStep::done(ActivityOutcome::Failed {
                message: HELD_BY_ANOTHER_TAB.to_string(),
            }),
            Event::TimerFired { .. } => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                if now >= self.settle_at {
                    if self.banner_reset_sent
                        && let Some(summary) = self.rom_verdict(ctx)
                    {
                        return self.settle_on_rom_verdict(summary, ctx);
                    }
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
            | Event::BoardHeld { .. }
            | Event::LinkFreed { .. }
            | Event::ActivityMarker { .. }
            | Event::IdentityObserved { .. }
            | Event::GrantAnswered { .. } => ActivityStep::nothing(),
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
            //
            // Unless no banner named the chip: then the board is reset once
            // first, so the install picker can offer this chip's boards only
            // ([`Self::with_banner_reset`]).
            LinkEvent::Line(_) if !self.winding_down => match ctx.evidence.classification {
                Classification::Blank if self.wants_banner(ctx) => {
                    self.resetting_for_the_banner(ctx)
                }
                Classification::Blank => self.settle_on_rom_verdict("blank or erased flash", ctx),
                Classification::Bootloader => {
                    self.settle_on_rom_verdict("waiting in ROM download mode", ctx)
                }
                _ => ActivityStep::nothing(),
            },
            // A core-only board's manifest is its whole answer, the way a
            // hello is a running board's: it will send no hello to wait
            // for. The port stays open — its engine comes back over it.
            LinkEvent::UpdateFacts(_)
                if !self.winding_down
                    && matches!(ctx.evidence.classification, Classification::CoreOnly { .. }) =>
            {
                self.settle(self.settle_at, ctx)
            }
            // The banner reset could not run (no reset lines after all, a
            // port that refused the signals): the window it would have
            // cleared still holds the blank verdict, so settle on it now.
            LinkEvent::ResetOutcome { ok: false, .. }
                if self.banner_reset_sent && !self.winding_down =>
            {
                match self.rom_verdict(ctx) {
                    Some(summary) => self.settle_on_rom_verdict(summary, ctx),
                    None => ActivityStep::nothing(),
                }
            }
            // Boot lines (pre-verdict), errors and reset outcomes are
            // diagnosis: the fold has already recorded them and the
            // deadline decides. Update bytes are the update driver's.
            LinkEvent::Line(_)
            | LinkEvent::Error(_)
            | LinkEvent::ResetOutcome { .. }
            | LinkEvent::Passthrough { .. }
            | LinkEvent::WireNote(_)
            | LinkEvent::Update(_)
            | LinkEvent::UpdateFacts(_) => ActivityStep::nothing(),
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

    /// An embedder that sends no heartbeat (the browser sim) only says what
    /// it runs when asked: a LightPlayer whose hello came with nothing said
    /// about its projects is asked once, as the identify ends. One that
    /// already said (a heartbeat's report) is not asked again.
    #[test]
    fn a_light_player_that_has_not_said_what_it_runs_is_asked_as_identify_ends() {
        let config = RosterConfig::default();
        let hello = Event::Link {
            link: LinkId(1),
            event: LinkEvent::Frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    ..Default::default()
                },
            )),
        };
        let asks_what_it_runs = |step: &ActivityStep| match step {
            ActivityStep::Done { commands, .. } => commands.iter().any(|command| {
                matches!(
                    command,
                    Command::Link {
                        command: LinkCommand::SendFrame(ClientFrame {
                            body: crate::wire::ClientFrameBody::ListLoadedProjects,
                            ..
                        }),
                        ..
                    }
                )
            }),
            ActivityStep::Continue(_) => panic!("the hello settles it: {step:?}"),
        };

        let mut unsaid = Evidence::default();
        fold(&mut unsaid, Millis(0), opened(), &config);
        fold(&mut unsaid, Millis(100), hello.clone(), &config);
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);
        let step = with_ctx(&unsaid, &config, |ctx| {
            identify.handle(Millis(100), &Input::Event(hello.clone()), ctx)
        });
        assert!(asks_what_it_runs(&step), "{step:?}");

        let mut said = Evidence::default();
        fold(&mut said, Millis(0), opened(), &config);
        fold(
            &mut said,
            Millis(50),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Frame(ServerFrame::loaded_report(0, Vec::new())),
            },
            &config,
        );
        fold(&mut said, Millis(100), hello.clone(), &config);
        let mut identify = IdentifyActivity::new(Millis(0), Millis(5_000), 1_000);
        let step = with_ctx(&said, &config, |ctx| {
            identify.handle(Millis(100), &Input::Event(hello), ctx)
        });
        assert!(!asks_what_it_runs(&step), "it said already: {step:?}");
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

    /// The phone's case (2026-10-25): the port opened mid boot loop, so
    /// the first line is "invalid header" and no banner named the chip. The
    /// board is reset once, the banner it prints names the chip, and the
    /// loop's next line settles the blank verdict with the port handed back.
    #[test]
    fn a_blank_board_with_no_banner_is_reset_once_for_it() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identify =
            IdentifyActivity::new(Millis(0), Millis(5_000), 1_000).with_banner_reset(true);
        fold(&mut evidence, Millis(0), opened(), &config);

        let step = feed(
            &mut identify,
            &mut evidence,
            Millis(40),
            blank_line(),
            &config,
        );
        assert_eq!(
            step,
            ActivityStep::Continue(vec![Command::Link {
                link: LinkId(1),
                command: LinkCommand::RunReset(ResetKind::Normal),
            }]),
            "no chip heard: reset for the banner, don't settle"
        );

        let reset = LinkEvent::ResetOutcome {
            kind: ResetKind::Normal,
            ok: true,
        };
        feed(&mut identify, &mut evidence, Millis(150), reset, &config);
        feed(
            &mut identify,
            &mut evidence,
            Millis(160),
            opened_event(),
            &config,
        );
        let banner = LinkEvent::Line("ESP-ROM:esp32c6-20220919".to_string());
        let step = feed(&mut identify, &mut evidence, Millis(200), banner, &config);
        assert_eq!(step, ActivityStep::nothing(), "the banner is not a verdict");
        assert_eq!(evidence.detected_chip(), Some("esp32c6"));

        let step = feed(
            &mut identify,
            &mut evidence,
            Millis(220),
            blank_line(),
            &config,
        );
        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ref commands,
                } if summary.contains("blank")
                    && matches!(commands.as_slice(), [Command::Link { command: LinkCommand::Close, .. }])
            ),
            "{step:?}"
        );
    }

    /// Once per run: a banner lost anyway (a phone drops bytes) settles on
    /// the next blank line with no chip, and a reset the port refused
    /// settles at once.
    #[test]
    fn the_banner_reset_is_sent_once_and_never_waited_on() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identify =
            IdentifyActivity::new(Millis(0), Millis(5_000), 1_000).with_banner_reset(true);
        fold(&mut evidence, Millis(0), opened(), &config);
        feed(
            &mut identify,
            &mut evidence,
            Millis(40),
            blank_line(),
            &config,
        );
        let reset = LinkEvent::ResetOutcome {
            kind: ResetKind::Normal,
            ok: true,
        };
        feed(&mut identify, &mut evidence, Millis(150), reset, &config);
        let step = feed(
            &mut identify,
            &mut evidence,
            Millis(220),
            blank_line(),
            &config,
        );
        assert!(matches!(step, ActivityStep::Done { .. }), "{step:?}");
        assert_eq!(evidence.detected_chip(), None);

        let mut evidence = Evidence::default();
        let mut identify =
            IdentifyActivity::new(Millis(0), Millis(5_000), 1_000).with_banner_reset(true);
        fold(&mut evidence, Millis(0), opened(), &config);
        feed(
            &mut identify,
            &mut evidence,
            Millis(40),
            blank_line(),
            &config,
        );
        let refused = LinkEvent::ResetOutcome {
            kind: ResetKind::Normal,
            ok: false,
        };
        let step = feed(&mut identify, &mut evidence, Millis(60), refused, &config);
        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ..
                } if summary.contains("blank")
            ),
            "{step:?}"
        );
    }

    /// A banner already heard, or a device that did not allow it: no reset.
    #[test]
    fn a_chip_already_named_or_a_reset_not_allowed_settles_at_once() {
        let config = RosterConfig::default();
        let settles = |allowed: bool, banner: bool| {
            let mut evidence = Evidence::default();
            let mut identify =
                IdentifyActivity::new(Millis(0), Millis(5_000), 1_000).with_banner_reset(allowed);
            fold(&mut evidence, Millis(0), opened(), &config);
            if banner {
                let line = LinkEvent::Line("ESP-ROM:esp32c6-20220919".to_string());
                feed(&mut identify, &mut evidence, Millis(20), line, &config);
            }
            let step = feed(
                &mut identify,
                &mut evidence,
                Millis(40),
                blank_line(),
                &config,
            );
            matches!(step, ActivityStep::Done { .. })
        };
        assert!(settles(true, true), "the banner named it");
        assert!(settles(false, false), "the device did not allow a reset");
        assert!(!settles(true, false), "the premise: otherwise it resets");
    }

    fn feed(
        identify: &mut IdentifyActivity,
        evidence: &mut Evidence,
        now: Millis,
        event: LinkEvent,
        config: &RosterConfig,
    ) -> ActivityStep {
        let event = Event::Link {
            link: LinkId(1),
            event,
        };
        fold(evidence, now, event.clone(), config);
        with_ctx(evidence, config, |ctx| {
            identify.handle(now, &Input::Event(event), ctx)
        })
    }

    fn blank_line() -> LinkEvent {
        LinkEvent::Line("invalid header: 0xffffffff".to_string())
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
