//! The Erase activity — "Factory reset" on the card: wipe the flash, then
//! watch the board come back blank.
//!
//! A sibling of [`FlashActivity`](super::flash::FlashActivity) with the
//! second half inverted: the flasher's ladder waits for a hello, but an
//! erased board can never say hello — its ROM boot-loops printing the
//! blank-flash signature. So after the effect the reducer reopens the port
//! at the BOOTLOADER baud (the ROM prints at 115 200; the app baud would
//! turn the loop into unreadable splatter) and settles when the fold
//! classifies the board — `Blank`/`Bootloader` is the success it expects,
//! and anything else is reported as what it is.
//!
//! The effect's own verification (esptool's erase completion line) is the
//! truth about the WIPE; the observation window only refreshes the card's
//! classification so the needs-firmware face appears with the chip named by
//! the boot output. A quiet window therefore degrades to success with a
//! note, never to failure. Identity survives an erase by design — it lives
//! in the efuse (ADR 2026-08-04), so the entry and its registry row stay.
//!
//! Cancel during the wipe is held (esptool cannot abort cleanly) exactly
//! like the flash's write window; the erase grace bounds the hold (I1/I2).

use serde::{Deserialize, Serialize};

use crate::DeviceId;
use crate::activity::activity_cell::{
    ActivityCtx, ActivityKind, ActivityOutcome, ActivityReducer, ActivityStep,
};
use crate::event::{Action, ActivityMarker, Command, EffectRequest, Event, Input};
use crate::evidence::Classification;
use crate::link::LinkCommand;
use crate::time::Millis;

/// The ROM bootloader's console baud: what a blank board's boot loop prints
/// at. Opening at the app baud (921 600) would garble the very lines the
/// classifier needs.
const BOOTLOADER_BAUD: u32 = 115_200;

/// Where the erase currently is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum ErasePhase {
    /// The coarse effect owns the wire; we absorb markers.
    Erasing,
    /// The effect succeeded; listening at bootloader baud for the blank
    /// boot loop so the card's classification catches up.
    Observing {
        deadline: Millis,
        /// Next instant to retry the open (session adoption answers one of
        /// these knocks after a re-enumeration).
        next_poke_at: Millis,
    },
}

/// The Erase reducer's own state. Everything it learns lives in the fold.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EraseActivity {
    device: DeviceId,
    phase: ErasePhase,
    /// Cancel arrived during the wipe: esptool cannot stop cleanly, so the
    /// wind-down happens when the effect ends.
    cancel_after_effect: bool,
    /// Cancel wind-down in progress: the port was asked to close.
    winding_down: bool,
}

impl EraseActivity {
    pub fn new(device: DeviceId) -> Self {
        Self {
            device,
            phase: ErasePhase::Erasing,
            cancel_after_effect: false,
            winding_down: false,
        }
    }

    /// The command that starts the coarse effect. Emitted by
    /// [`Device::spawn_erase`](crate::Device::spawn_erase) at spawn.
    pub(crate) fn spawn_commands(&self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        vec![Command::RunEffect {
            device: self.device,
            link,
            effect_id: ctx.effect_id,
            effect: EffectRequest::Erase,
        }]
    }

    fn open_port(&self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        vec![Command::Link {
            link,
            command: LinkCommand::Open {
                baud: BOOTLOADER_BAUD,
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

    /// What the observation window has seen, if it is conclusive.
    fn observed_outcome(&self, ctx: &ActivityCtx<'_>) -> Option<ActivityOutcome> {
        match &ctx.evidence.classification {
            Classification::Blank | Classification::Bootloader => {
                Some(ActivityOutcome::Succeeded {
                    summary: "flash erased — the board is blank".to_string(),
                })
            }
            // A hello after an erase means the wipe did not take (or the
            // wrong board answered). Say what happened, never pretend.
            Classification::LightPlayer { .. } | Classification::Incompatible { .. } => {
                Some(ActivityOutcome::Failed {
                    message: "the board still answers with firmware after the erase — \
                              the wipe did not take"
                        .to_string(),
                })
            }
            _ => None,
        }
    }

    fn handle_marker(
        &mut self,
        now: Millis,
        marker: &ActivityMarker,
        ctx: &ActivityCtx<'_>,
    ) -> ActivityStep {
        let ActivityMarker::Ended { outcome, .. } = marker else {
            return ActivityStep::nothing();
        };
        match &self.phase {
            ErasePhase::Erasing => {
                if self.cancel_after_effect {
                    // The wipe window is over; honour the held cancel now.
                    self.winding_down = true;
                    return ActivityStep::done(ActivityOutcome::Cancelled);
                }
                match outcome {
                    ActivityOutcome::Succeeded { .. } => {
                        // esptool's closing reset just rebooted the (now
                        // blank) board; knock right away and keep knocking —
                        // adoption answers one of these after a re-enum.
                        self.phase = ErasePhase::Observing {
                            deadline: now.plus_ms(8_000),
                            next_poke_at: now.plus_ms(1_000),
                        };
                        ActivityStep::Continue(self.open_port(ctx))
                    }
                    other => ActivityStep::done(ActivityOutcome::Failed {
                        message: format!("erase failed: {}", other.summary()),
                    }),
                }
            }
            ErasePhase::Observing { .. } => ActivityStep::nothing(),
        }
    }

    fn handle_timer(&mut self, now: Millis, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        if self.winding_down {
            return ActivityStep::nothing();
        }
        match self.phase.clone() {
            ErasePhase::Erasing => ActivityStep::nothing(),
            ErasePhase::Observing {
                deadline,
                next_poke_at,
            } => {
                if let Some(outcome) = self.observed_outcome(ctx) {
                    return ActivityStep::done(outcome);
                }
                if now >= deadline {
                    // The wipe itself was verified by the effect; only the
                    // classification refresh went quiet. Degrade honestly.
                    return ActivityStep::done(ActivityOutcome::Succeeded {
                        summary: "flash erased; the board hasn't shown its boot loop yet — \
                                  Identify re-checks it"
                            .to_string(),
                    });
                }
                if now >= next_poke_at && !ctx.evidence.presence.is_open() {
                    let commands = self.open_port(ctx);
                    self.phase = ErasePhase::Observing {
                        deadline,
                        next_poke_at: now.plus_ms(1_000),
                    };
                    return ActivityStep::Continue(commands);
                }
                if now >= next_poke_at {
                    self.phase = ErasePhase::Observing {
                        deadline,
                        next_poke_at: now.plus_ms(1_000),
                    };
                }
                ActivityStep::nothing()
            }
        }
    }
}

impl ActivityReducer for EraseActivity {
    fn kind(&self) -> ActivityKind {
        ActivityKind::Erase
    }

    fn handle(&mut self, now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match input {
            Input::Action(Action::CancelActivity { .. }) => {
                if self.winding_down {
                    return ActivityStep::nothing();
                }
                match self.phase {
                    ErasePhase::Erasing => {
                        self.cancel_after_effect = true;
                        ActivityStep::nothing()
                    }
                    ErasePhase::Observing { .. } => self.wind_down(ctx),
                }
            }
            Input::Action(_) => ActivityStep::nothing(),
            Input::Event(event) => match event {
                Event::ActivityMarker { marker, .. } => self.handle_marker(now, marker, ctx),
                Event::TimerFired { .. } => self.handle_timer(now, ctx),
                Event::Link { event, .. } => match event {
                    crate::link::LinkEvent::Closed { .. } if self.winding_down => {
                        ActivityStep::done(ActivityOutcome::Cancelled)
                    }
                    // Boot lines are the fold's; the timer reads the
                    // classification it produces.
                    _ => ActivityStep::nothing(),
                },
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
            ErasePhase::Erasing => None,
            ErasePhase::Observing {
                deadline,
                next_poke_at,
            } => Some((*deadline).min(*next_poke_at)),
        }
    }
}
