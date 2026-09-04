//! The Flash activity: put LightPlayer firmware on this board.
//!
//! The write itself is a **coarse effect**: the reducer emits
//! [`Command::RunEffect`] and the effects layer runs esptool with exclusive
//! ownership of the wire, streaming progress back as
//! [`ActivityMarker::Progress`](crate::event::ActivityMarker::Progress)
//! events and ending with an
//! [`ActivityMarker::Ended`](crate::event::ActivityMarker::Ended). The
//! reducer never sees the wire during the write — it absorbs markers.
//!
//! What the reducer DOES own is the part the old system got wrong: the
//! **post-flash reconnect ladder**. A USB-Serial-JTAG chip (the C6)
//! re-enumerates on the flasher's closing hard reset, so the port under the
//! link dies exactly when the flash succeeds. The ladder is:
//!
//! 1. **Reopen**, on a retry cadence — session adoption in the platform
//!    layer re-derives the handle for a re-enumerated port, so an open that
//!    fails now succeeds a moment later. Each open window also *asks* for a
//!    hello: a board that booted while the port was down never volunteers
//!    one again.
//! 2. Still quiet? [`ResetKind::Normal`] — the ordinary DTR/RTS reboot.
//! 3. Still quiet? [`ResetKind::BothThenDrop`] — the CH34x sequence that
//!    works where `Normal` does not (M1's bench-proven fallback).
//! 4. Still quiet? **Fail honestly**, with the V3/CH340 guidance: on those
//!    bridges a replug kills the browser's grant, and Reconnect is the way
//!    back.
//!
//! The hello is **fold evidence**, never a sticky gate: the reducer settles
//! the moment `ctx.evidence.has_hello()` turns true, whichever rung caused
//! it. All waiting is scheduled timers (I7 — a reducer never awaits).
//!
//! Once the hello proves the app protocol is up, one more coarse effect
//! writes the picked board's runtime manifest to `/hardware.json`
//! (board-selection D4, effective next boot). Its failure degrades honestly:
//! the flash stands, the summary says the pin map stayed default.
//!
//! Cancellation is honest about physics: esptool-js cannot abort a write
//! cleanly, so a cancel during the write window is *held* — the card says
//! "cancelling", the write finishes, and the wind-down happens instead of
//! the ladder. Supervision still bounds everything (I1/I2): the flash
//! deadline and the flash cancel grace are wide because the write window is,
//! and eviction remains the backstop.
//!
//! [`Command::RunEffect`]: crate::event::Command::RunEffect

use serde::{Deserialize, Serialize};

use crate::event::{Action, ActivityMarker, Command, EffectRequest, Event, Input};
use crate::identity::DeviceId;
use crate::link::{LinkCommand, LinkEvent, ResetKind};
use crate::time::Millis;
use crate::wire::ClientFrame;

use super::activity_cell::{
    ActivityCtx, ActivityKind, ActivityOutcome, ActivityReducer, ActivityStep,
};

/// How long a parked port gets to re-enumerate before esptool takes it.
const PARK_SETTLE_MS: u64 = 2_500;

/// The honest failure copy when every rung of the ladder stayed quiet.
const LADDER_EXHAUSTED: &str = "firmware was written, but the board never answered. If it is \
     on a CH340/V3 bridge, unplug it, plug it back in, and use Reconnect — a replug loses the \
     browser's permission for the port.";

/// Where the flash currently is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum FlashPhase {
    /// Driving the chip into its ROM downloader before the effect (native
    /// USB only): a blank native-USB chip boot-loops and re-enumerates
    /// every few seconds, cutting esptool mid-connect — the downloader
    /// waits stably with USB up (bench, G1 2026-08-31: two straight
    /// mid-SYNC losses on the C6). Best effort: the reset outcome OR the
    /// deadline starts the effect either way — parking is an odds-improver,
    /// never a gate.
    Parking {
        deadline: Millis,
        /// Set once the downloader dance answered: entering the downloader
        /// RE-ENUMERATES a native-USB port, and the OS needs a beat to
        /// enumerate the replacement — handing esptool the wire before
        /// that finishes gives it the dead generation (bench, G1
        /// 2026-08-31: a parked flash still died at setSignals in 5.8s).
        /// The effect starts when this settle instant passes.
        settle_at: Option<Millis>,
    },
    /// The coarse effect owns the wire; we absorb markers.
    Writing,
    /// The effect succeeded; the ladder is bringing the board back.
    Reconnecting {
        rung: ReconnectRung,
        /// When this rung gives up and the next one fires.
        rung_deadline: Millis,
        /// Next instant to retry the open / re-ask the hello.
        next_poke_at: Millis,
    },
    /// Hello heard; the board-manifest effect is writing `/hardware.json`.
    Stamping { deadline: Millis },
}

/// The escalation order. Each rung's *name* is the reset it fires on entry;
/// the first rung fires no reset — the flasher's own closing hard reset
/// already rebooted the board.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum ReconnectRung {
    Reopen,
    NormalReset,
    BothThenDrop,
}

/// The Flash reducer's own state. Everything it learns lives in the fold.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FlashActivity {
    device: DeviceId,
    board_id: String,
    build_id: String,
    park_first: bool,
    /// Dead-port retries left. Enumeration jitter after the park has more
    /// faces than one settle constant covers (bench: esptool died at
    /// setSignals ~8s AFTER a clean park) — one in-activity retry
    /// re-settles and hands the wire over again; the chip is already
    /// parked, so the retry costs seconds and no dance.
    open_retries_left: u8,
    phase: FlashPhase,
    /// Cancel arrived during the write window: esptool cannot stop cleanly,
    /// so the wind-down happens when the effect ends instead of the ladder.
    cancel_after_effect: bool,
    /// Cancel wind-down in progress: the port was asked to close.
    winding_down: bool,
    next_request_id: u32,
}

impl FlashActivity {
    pub fn new(device: DeviceId, board_id: String, build_id: String, park_first: bool) -> Self {
        Self {
            device,
            board_id,
            build_id,
            park_first,
            open_retries_left: 1,
            phase: FlashPhase::Writing,
            cancel_after_effect: false,
            winding_down: false,
            next_request_id: 1,
        }
    }

    /// The command that starts the coarse effect. Emitted by
    /// [`Device::spawn_flash`](crate::Device::spawn_flash) at spawn.
    pub(crate) fn spawn_commands(&mut self, now: Millis, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        if self.park_first {
            self.phase = FlashPhase::Parking {
                deadline: now.plus_ms(ctx.config.flash_rung_ms),
                settle_at: None,
            };
            return vec![Command::Link {
                link,
                command: LinkCommand::RunReset(ResetKind::UsbJtagDownload),
            }];
        }
        self.start_effect(ctx)
    }

    /// The command that hands the wire to the flasher. From spawn directly
    /// (UART bridges), or once parking resolves (native USB).
    fn start_effect(&mut self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        self.phase = FlashPhase::Writing;
        vec![Command::RunEffect {
            device: self.device,
            link,
            effect_id: ctx.effect_id,
            effect: EffectRequest::Flash {
                build_id: self.build_id.clone(),
                board_id: self.board_id.clone(),
            },
        }]
    }

    fn ask_hello(&mut self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        vec![Command::Link {
            link,
            command: LinkCommand::SendFrame(ClientFrame::hello(request_id)),
        }]
    }

    fn open_port(&self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        vec![Command::Link {
            link,
            command: LinkCommand::Open {
                baud: ctx.config.open_baud,
            },
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

    /// The hello arrived: move to the board-manifest stamp.
    ///
    /// A hello on another wire version is NOT a failed flash (ruled
    /// 2026-09-04): the image wrote and the board boots it. The fold has
    /// journaled the version and put the notice in the terminal; failing
    /// here would tell the user the flash broke when it did not.
    fn on_hello(&mut self, now: Millis, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let Some(link) = ctx.link else {
            // The hello proves the board is alive, but the link vanished
            // under us in the same instant; the stamp cannot run.
            return ActivityStep::done(self.success_without_stamp(
                "firmware installed; the board manifest was not written (port lost) — \
                 the compiled-in default pin map stands",
            ));
        };
        // The stamp's OWN budget, not a ladder rung: the write goes to a
        // board that may still be formatting its littlefs, and the seam
        // below waits for it to answer a cheap request before writing at
        // all. Reusing the 8 s rung gave up ~30 s before the classic was
        // ever going to be ready (G1 bench, 2026-08-31).
        self.phase = FlashPhase::Stamping {
            deadline: now.plus_ms(ctx.config.stamp_deadline_ms),
        };
        ActivityStep::Continue(vec![Command::RunEffect {
            device: self.device,
            link,
            effect_id: ctx.effect_id,
            effect: EffectRequest::WriteBoardManifest {
                board_id: self.board_id.clone(),
            },
        }])
    }

    fn success(&self, ctx: &ActivityCtx<'_>) -> ActivityOutcome {
        let label = ctx
            .evidence
            .classification
            .hello()
            .map(|hello| hello.label())
            .unwrap_or_else(|| "LightPlayer".to_string());
        ActivityOutcome::Succeeded {
            summary: format!("firmware installed — {label}"),
        }
    }

    fn success_without_stamp(&self, summary: &str) -> ActivityOutcome {
        ActivityOutcome::Succeeded {
            summary: summary.to_string(),
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
            FlashPhase::Parking { .. } => ActivityStep::nothing(),
            FlashPhase::Writing => {
                if self.cancel_after_effect {
                    // The write window is over; honour the held cancel now,
                    // whatever the effect reported.
                    return self.wind_down(ctx);
                }
                match outcome {
                    ActivityOutcome::Failed { message }
                        if self.open_retries_left > 0
                            && (message.contains("Failed to open serial port")
                                || message.contains("port is closed")) =>
                    {
                        // The dead-port signature: esptool grabbed a stale
                        // generation. The chip is parked (or parking is
                        // moot); re-settle and hand the wire over again.
                        self.open_retries_left -= 1;
                        self.phase = FlashPhase::Parking {
                            deadline: now.plus_ms(ctx.config.flash_rung_ms),
                            settle_at: Some(now.plus_ms(PARK_SETTLE_MS)),
                        };
                        ActivityStep::nothing()
                    }
                    ActivityOutcome::Succeeded { .. } => {
                        // The flasher's closing hard reset just rebooted the
                        // board (and on a C6, re-enumerated its port). Climb.
                        self.phase = FlashPhase::Reconnecting {
                            rung: ReconnectRung::Reopen,
                            rung_deadline: now.plus_ms(ctx.config.flash_rung_ms),
                            next_poke_at: now.plus_ms(ctx.config.flash_reopen_retry_ms),
                        };
                        ActivityStep::Continue(self.open_port(ctx))
                    }
                    other => ActivityStep::done(ActivityOutcome::Failed {
                        message: format!("flash failed: {}", other.summary()),
                    }),
                }
            }
            FlashPhase::Reconnecting { .. } => ActivityStep::nothing(),
            FlashPhase::Stamping { .. } => match outcome {
                ActivityOutcome::Succeeded { .. } => ActivityStep::done(self.success(ctx)),
                // The flash stands; only the pin-map stamp failed. Degrade
                // honestly rather than calling the whole thing a failure.
                other => ActivityStep::done(self.success_without_stamp(&format!(
                    "firmware installed; writing the board manifest failed ({}) — the \
                     compiled-in default pin map stands",
                    other.summary()
                ))),
            },
        }
    }

    fn handle_timer(&mut self, now: Millis, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        if self.winding_down {
            return ActivityStep::nothing();
        }
        match self.phase.clone() {
            FlashPhase::Parking {
                deadline,
                settle_at,
            } => {
                if let Some(settle_at) = settle_at {
                    if now >= settle_at {
                        // Parked AND enumerated: esptool gets a still,
                        // live port.
                        return ActivityStep::Continue(self.start_effect(ctx));
                    }
                } else if now >= deadline {
                    // Parking is an odds-improver, never a gate: if the
                    // downloader dance went quiet, hand esptool the wire
                    // and let it fight its own fight.
                    return ActivityStep::Continue(self.start_effect(ctx));
                }
                ActivityStep::nothing()
            }
            FlashPhase::Writing => ActivityStep::nothing(),
            FlashPhase::Reconnecting {
                rung,
                rung_deadline,
                next_poke_at,
            } => {
                if ctx.evidence.has_hello() {
                    return self.on_hello(now, ctx);
                }
                if now >= rung_deadline {
                    return self.escalate(now, rung, ctx);
                }
                if now >= next_poke_at {
                    let commands = match ctx.evidence.presence.is_open() {
                        // Open, quiet: the boot hello may already be gone —
                        // ask for one (a connect cannot assume the power to
                        // cause a boot).
                        true => self.ask_hello(ctx),
                        // Closed: keep knocking. Session adoption makes a
                        // re-enumerated port answer one of these knocks.
                        false => self.open_port(ctx),
                    };
                    self.phase = FlashPhase::Reconnecting {
                        rung,
                        rung_deadline,
                        next_poke_at: now.plus_ms(ctx.config.flash_reopen_retry_ms),
                    };
                    return ActivityStep::Continue(commands);
                }
                ActivityStep::nothing()
            }
            FlashPhase::Stamping { deadline } => {
                if now >= deadline {
                    // The stamp effect went quiet. The flash stands.
                    return ActivityStep::done(self.success_without_stamp(
                        "firmware installed; writing the board manifest timed out — the \
                         compiled-in default pin map stands",
                    ));
                }
                ActivityStep::nothing()
            }
        }
    }

    fn escalate(
        &mut self,
        now: Millis,
        rung: ReconnectRung,
        ctx: &mut ActivityCtx<'_>,
    ) -> ActivityStep {
        let next = match rung {
            ReconnectRung::Reopen => ReconnectRung::NormalReset,
            ReconnectRung::NormalReset => ReconnectRung::BothThenDrop,
            ReconnectRung::BothThenDrop => {
                return ActivityStep::done(ActivityOutcome::Failed {
                    message: LADDER_EXHAUSTED.to_string(),
                });
            }
        };
        let commands = match (ctx.link, ctx.evidence.presence.is_open()) {
            (Some(link), true) => {
                let kind = match next {
                    ReconnectRung::NormalReset => ResetKind::Normal,
                    ReconnectRung::BothThenDrop => ResetKind::BothThenDrop,
                    ReconnectRung::Reopen => unreachable!("escalation never re-enters Reopen"),
                };
                vec![Command::Link {
                    link,
                    command: LinkCommand::RunReset(kind),
                }]
            }
            // The port never even opened; a reset has nothing to drive.
            // Keep knocking — the rung deadline still moves the ladder on.
            _ => self.open_port(ctx),
        };
        self.phase = FlashPhase::Reconnecting {
            rung: next,
            rung_deadline: now.plus_ms(ctx.config.flash_rung_ms),
            next_poke_at: now.plus_ms(ctx.config.flash_reopen_retry_ms),
        };
        ActivityStep::Continue(commands)
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
                // During the write the effect owns the port, and during the
                // ladder the retry cadence reopens; a close is not fatal.
                ActivityStep::nothing()
            }
            LinkEvent::Frame(_) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                if matches!(self.phase, FlashPhase::Reconnecting { .. }) && ctx.evidence.has_hello()
                {
                    return self.on_hello(now, ctx);
                }
                ActivityStep::nothing()
            }
            // Opens, boot lines, reset outcomes and errors are diagnosis the
            // fold already recorded; the timers decide.
            LinkEvent::ResetOutcome { .. }
                if matches!(self.phase, FlashPhase::Parking { .. }) && !self.winding_down =>
            {
                // Parked (or the dance failed — either way the answer is
                // in). Not the wire yet: the dance re-enumerated the port,
                // so wait out the OS's enumeration before the flasher
                // takes it.
                if let FlashPhase::Parking { deadline, .. } = self.phase {
                    self.phase = FlashPhase::Parking {
                        deadline,
                        settle_at: Some(now.plus_ms(PARK_SETTLE_MS)),
                    };
                }
                ActivityStep::nothing()
            }
            LinkEvent::Opened { .. }
            | LinkEvent::Line(_)
            | LinkEvent::ResetOutcome { .. }
            | LinkEvent::Error(_) => ActivityStep::nothing(),
        }
    }
}

impl ActivityReducer for FlashActivity {
    fn kind(&self) -> ActivityKind {
        ActivityKind::Flash
    }

    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match input {
            Input::Action(Action::CancelActivity { .. }) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                match self.phase {
                    // Nothing owns the wire during parking; stop politely.
                    FlashPhase::Parking { .. } => self.wind_down(ctx),
                    // esptool-js cannot abort a write cleanly: hold the
                    // cancel through the write window (the card says
                    // "cancelling") and wind down when the effect ends.
                    // Supervision's flash grace bounds the hold.
                    FlashPhase::Writing => {
                        self.cancel_after_effect = true;
                        ActivityStep::nothing()
                    }
                    // The write is done; the ladder and the stamp can stop
                    // politely right now.
                    FlashPhase::Reconnecting { .. } | FlashPhase::Stamping { .. } => {
                        self.wind_down(ctx)
                    }
                }
            }
            Input::Action(_) => ActivityStep::nothing(),
            Input::Event(event) => match event {
                Event::ActivityMarker { marker, .. } => self.handle_marker(now, marker, ctx),
                Event::TimerFired { .. } => self.handle_timer(now, ctx),
                Event::Link { event, .. } => self.handle_link_event(now, event, ctx),
                // Identity learned by the preflight is fold business; link
                // loss is supervision's (it evicts and recovers); the wire
                // borrow is the fold's too (it pauses freshness).
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
            FlashPhase::Parking {
                deadline,
                settle_at,
            } => Some(settle_at.map_or(*deadline, |settle| settle.min(*deadline))),
            // The effect drives; the supervision backstop bounds it (I1).
            FlashPhase::Writing => None,
            FlashPhase::Reconnecting {
                rung_deadline,
                next_poke_at,
                ..
            } => Some((*rung_deadline).min(*next_poke_at)),
            FlashPhase::Stamping { deadline } => Some(*deadline),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::Evidence;
    use crate::identity::IdentityChain;
    use crate::link::{LinkId, LinkInfo};
    use crate::roster::RosterConfig;
    use crate::wire::{HelloFacts, ServerFrame};

    #[test]
    fn parking_holds_the_effect_until_the_downloader_answers() {
        // Native USB: the downloader dance first, the flasher second — a
        // boot-looping chip cuts esptool mid-connect otherwise (bench,
        // G1 2026-08-31).
        let mut activity = FlashActivity::new(
            DeviceId(1),
            "seeed-xiao-esp32c6".to_string(),
            "esp32c6-4mb".to_string(),
            true,
        );
        let evidence = Evidence::default();
        let config = RosterConfig::default();
        let commands = with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(0), ctx)
        });
        assert!(
            commands.iter().any(|command| matches!(
                command,
                Command::Link {
                    command: LinkCommand::RunReset(ResetKind::UsbJtagDownload),
                    ..
                }
            )),
            "spawn parks first: {commands:?}"
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::RunEffect { .. })),
            "the effect waits for the downloader"
        );

        // The dance answers — but the port just re-enumerated, so the
        // flasher waits out the settle rather than grabbing a dead
        // generation.
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(100),
                &Input::Event(Event::Link {
                    link: LinkId(1),
                    event: LinkEvent::ResetOutcome {
                        kind: ResetKind::UsbJtagDownload,
                        ok: true,
                    },
                }),
                ctx,
            )
        });
        assert!(
            matches!(step, ActivityStep::Continue(ref commands) if commands.is_empty())
                || matches!(step, ActivityStep::Continue(_)) == false,
            "the effect does NOT start at the outcome: {step:?}"
        );
        assert!(
            activity
                .next_deadline()
                .is_some_and(|at| at <= Millis(2_600)),
            "the settle instant is armed: {:?}",
            activity.next_deadline()
        );

        // The settle passes — NOW esptool takes the wire.
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(2_700),
                &Input::Event(Event::TimerFired {
                    timer: crate::time::TimerId {
                        scope: crate::journal::Scope::Device(DeviceId(1)),
                        seq: 1,
                    },
                }),
                ctx,
            )
        });
        let ActivityStep::Continue(commands) = step else {
            panic!("the settle hands over the wire: {step:?}");
        };
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::RunEffect { .. })),
            "the effect starts once parked AND enumerated: {commands:?}"
        );
    }

    fn flash() -> FlashActivity {
        FlashActivity::new(
            DeviceId(1),
            "seeed-xiao-esp32c6".to_string(),
            "esp32c6-4mb".to_string(),
            false,
        )
    }

    fn ended(outcome: ActivityOutcome) -> Input {
        Input::Event(Event::ActivityMarker {
            device: DeviceId(1),
            effect: Some(crate::event::EffectId(1)),
            marker: ActivityMarker::Ended {
                kind: ActivityKind::Flash,
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
            effect_id: crate::event::EffectId(1),
        };
        body(&mut ctx)
    }

    fn fold(evidence: &mut Evidence, now: Millis, event: Event, config: &RosterConfig) {
        let mut identity = IdentityChain::default();
        evidence.fold(now, &event, &mut identity, config);
    }

    fn opened(evidence: &mut Evidence, now: Millis, config: &RosterConfig) {
        fold(
            evidence,
            now,
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Opened {
                    info: LinkInfo::default(),
                },
            },
            config,
        );
    }

    fn hello(evidence: &mut Evidence, now: Millis, config: &RosterConfig) {
        fold(
            evidence,
            now,
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Frame(ServerFrame::hello(
                    1,
                    HelloFacts {
                        proto: config.expected_proto,
                        board_id: Some("seeed-xiao-esp32c6".to_string()),
                        ..Default::default()
                    },
                )),
            },
            config,
        );
    }

    #[test]
    fn spawn_emits_the_coarse_effect_and_a_successful_write_starts_the_ladder() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();

        let commands = with_ctx(&evidence, &config, |ctx| {
            activity.spawn_commands(Millis(0), ctx)
        });
        assert!(matches!(
            commands.as_slice(),
            [Command::RunEffect {
                effect: EffectRequest::Flash { .. },
                ..
            }]
        ));
        assert_eq!(activity.next_deadline(), None, "the effect drives");

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(30_000),
                &ended(ActivityOutcome::Succeeded {
                    summary: "written".to_string(),
                }),
                ctx,
            )
        });

        assert!(matches!(
            step,
            ActivityStep::Continue(ref commands)
                if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::Open { .. }, .. }])
        ));
        assert!(activity.next_deadline().is_some(), "the ladder is timed");
    }

    #[test]
    fn a_failed_write_ends_the_activity_with_the_effects_message() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(1_000),
                &ended(ActivityOutcome::Failed {
                    message: "chip guard: image is esp32c6, chip is esp32s3".to_string(),
                }),
                ctx,
            )
        });

        assert!(matches!(
            step,
            ActivityStep::Done {
                outcome: ActivityOutcome::Failed { ref message },
                ..
            } if message.contains("chip guard")
        ));
    }

    #[test]
    fn the_ladder_escalates_reopen_then_normal_then_both_then_drop_then_fails_honestly() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(0),
                &ended(ActivityOutcome::Succeeded {
                    summary: "written".to_string(),
                }),
                ctx,
            )
        });

        // Past the first rung: a Normal reset fires.
        let at = Millis(config.flash_rung_ms + 1);
        let step = with_ctx(&evidence, &config, |ctx| activity.handle(at, &timer(), ctx));
        assert!(
            matches!(
                step,
                ActivityStep::Continue(ref commands)
                    if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::RunReset(ResetKind::Normal), .. }])
            ),
            "{step:?}"
        );

        // Past the second rung: the CH34x fallback.
        let at = Millis(2 * config.flash_rung_ms + 2);
        let step = with_ctx(&evidence, &config, |ctx| activity.handle(at, &timer(), ctx));
        assert!(
            matches!(
                step,
                ActivityStep::Continue(ref commands)
                    if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::RunReset(ResetKind::BothThenDrop), .. }])
            ),
            "{step:?}"
        );

        // Past the last rung: honest failure that points at Reconnect.
        let at = Millis(3 * config.flash_rung_ms + 3);
        let step = with_ctx(&evidence, &config, |ctx| activity.handle(at, &timer(), ctx));
        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Failed { ref message },
                    ..
                } if message.contains("Reconnect")
            ),
            "{step:?}"
        );
    }

    #[test]
    fn a_hello_during_the_ladder_moves_to_the_board_manifest_stamp_then_succeeds() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(0),
                &ended(ActivityOutcome::Succeeded {
                    summary: "written".to_string(),
                }),
                ctx,
            )
        });

        hello(&mut evidence, Millis(2_000), &config);
        let frame = Input::link(
            LinkId(1),
            LinkEvent::Frame(ServerFrame::hello(
                1,
                HelloFacts {
                    proto: config.expected_proto,
                    ..Default::default()
                },
            )),
        );
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(2_000), &frame, ctx)
        });
        assert!(
            matches!(
                step,
                ActivityStep::Continue(ref commands)
                    if matches!(commands.as_slice(), [Command::RunEffect {
                        effect: EffectRequest::WriteBoardManifest { .. },
                        ..
                    }])
            ),
            "{step:?}"
        );

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(2_500),
                &ended(ActivityOutcome::Succeeded {
                    summary: "stamped".to_string(),
                }),
                ctx,
            )
        });
        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ..
                } if summary.contains("seeed-xiao-esp32c6")
            ),
            "{step:?}"
        );
    }

    #[test]
    fn a_failed_stamp_degrades_honestly_instead_of_failing_the_flash() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(0),
                &ended(ActivityOutcome::Succeeded {
                    summary: "written".to_string(),
                }),
                ctx,
            )
        });
        hello(&mut evidence, Millis(1_000), &config);
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(1_000), &timer(), ctx)
        });

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(1_500),
                &ended(ActivityOutcome::Failed {
                    message: "fs write refused".to_string(),
                }),
                ctx,
            )
        });

        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ..
                } if summary.contains("default pin map")
            ),
            "the flash stands: {step:?}"
        );
    }

    /// C1 (G1 bench, 2026-08-31): the stamp used to inherit the ladder's 8 s
    /// rung, which gave up ~30 s before a freshly flashed classic — still
    /// formatting its littlefs — was ever going to answer. It gets its own
    /// budget now, and the budget is the one that has to hold.
    #[test]
    fn the_stamp_waits_on_its_own_deadline_not_a_ladder_rung() {
        let config = RosterConfig::default();
        assert!(
            config.stamp_deadline_ms > config.flash_rung_ms,
            "a stamp that inherits a rung is the defect this closes"
        );
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(0),
                &ended(ActivityOutcome::Succeeded {
                    summary: "written".to_string(),
                }),
                ctx,
            )
        });
        hello(&mut evidence, Millis(1_000), &config);
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(Millis(1_000), &timer(), ctx)
        });

        // A rung past the hello, the stamp is still waiting — the board is
        // allowed to be slow.
        let a_rung_later = Millis(1_000 + config.flash_rung_ms + 1);
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(a_rung_later, &timer(), ctx)
        });
        assert_eq!(
            step,
            ActivityStep::nothing(),
            "the ladder's rung is not the stamp's deadline"
        );
        assert_eq!(
            activity.next_deadline(),
            Some(Millis(1_000 + config.stamp_deadline_ms)),
            "the stamp is timed by its own budget"
        );

        // Its OWN deadline still bounds it (I1), and the flash still stands.
        let past_the_stamp = Millis(1_000 + config.stamp_deadline_ms + 1);
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(past_the_stamp, &timer(), ctx)
        });
        assert!(
            matches!(
                step,
                ActivityStep::Done {
                    outcome: ActivityOutcome::Succeeded { ref summary },
                    ..
                } if summary.contains("default pin map")
            ),
            "{step:?}"
        );
    }

    #[test]
    fn cancel_during_the_write_is_held_then_honoured_when_the_effect_ends() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();

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
            "the write window cannot be aborted"
        );

        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(60_000),
                &ended(ActivityOutcome::Succeeded {
                    summary: "written".to_string(),
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
                Millis(60_100),
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
    fn a_close_during_the_ladder_is_not_fatal_and_the_cadence_keeps_knocking() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        opened(&mut evidence, Millis(0), &config);
        let mut activity = flash();
        with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(0),
                &ended(ActivityOutcome::Succeeded {
                    summary: "written".to_string(),
                }),
                ctx,
            )
        });

        // The re-enumerated port answers the first open with a close.
        fold(
            &mut evidence,
            Millis(500),
            Event::Link {
                link: LinkId(1),
                event: LinkEvent::Closed {
                    reason: "browser serial error".to_string(),
                },
            },
            &config,
        );
        let step = with_ctx(&evidence, &config, |ctx| {
            activity.handle(
                Millis(500),
                &Input::link(
                    LinkId(1),
                    LinkEvent::Closed {
                        reason: "browser serial error".to_string(),
                    },
                ),
                ctx,
            )
        });
        assert_eq!(step, ActivityStep::nothing());

        // The next poke retries the open (adoption will have fixed the port).
        let at = Millis(config.flash_reopen_retry_ms + 10);
        let step = with_ctx(&evidence, &config, |ctx| activity.handle(at, &timer(), ctx));
        assert!(
            matches!(
                step,
                ActivityStep::Continue(ref commands)
                    if matches!(commands.as_slice(), [Command::Link { command: LinkCommand::Open { .. }, .. }])
            ),
            "{step:?}"
        );
    }
}
