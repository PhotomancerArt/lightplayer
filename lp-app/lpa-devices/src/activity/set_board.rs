//! The Set-board activity: save which board this is on a running board.
//!
//! An install no longer asks for the board model first (plan
//! `lp2025/2026-10-28-0025-install-without-board-pick`): it stamps the model
//! only when exactly one catalog board fits what the bootloader said. When
//! several fit, the card asks "Which board is this?" afterwards, and the
//! answer lands here — the same `/hardware.json` write the Flash activity's
//! stamp makes ([`EffectRequest::WriteBoardManifest`]), on a board already
//! running LightPlayer.
//!
//! The firmware reads its board manifest at boot, so the device restarts
//! the board once the write is confirmed (`Device`'s end-of-activity
//! handling, the same restart [`Action::ResetBoard`] sends). This reducer
//! owns only the write.
//!
//! [`Action::ResetBoard`]: crate::Action::ResetBoard

use serde::{Deserialize, Serialize};

use crate::event::{Action, ActivityMarker, Command, EffectRequest, Event, Input};
use crate::identity::DeviceId;
use crate::link::{LinkCommand, LinkEvent};
use crate::time::Millis;

use super::activity_cell::{
    ActivityCtx, ActivityKind, ActivityOutcome, ActivityReducer, ActivityStep,
};

/// The Set-board reducer's own state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SetBoardActivity {
    device: DeviceId,
    board_id: String,
    /// Cancel arrived during the write: a half-written manifest is the one
    /// state to avoid, so the wind-down waits for the effect to end.
    cancel_after_effect: bool,
    /// Cancel wind-down in progress: the port was asked to close.
    winding_down: bool,
}

impl SetBoardActivity {
    pub fn new(device: DeviceId, board_id: String) -> Self {
        Self {
            device,
            board_id,
            cancel_after_effect: false,
            winding_down: false,
        }
    }

    /// The board model being saved.
    pub fn board_id(&self) -> &str {
        &self.board_id
    }

    /// The command that starts the write. Emitted by
    /// [`Device::spawn_set_board`](crate::Device::spawn_set_board) at spawn.
    pub(crate) fn spawn_commands(&self, ctx: &ActivityCtx<'_>) -> Vec<Command> {
        let Some(link) = ctx.link else {
            return Vec::new();
        };
        vec![Command::RunEffect {
            device: self.device,
            link,
            effect_id: ctx.effect_id,
            effect: EffectRequest::WriteBoardManifest {
                board_id: self.board_id.clone(),
                layout_verified: false,
            },
        }]
    }

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

    fn handle_marker(&mut self, marker: &ActivityMarker, ctx: &ActivityCtx<'_>) -> ActivityStep {
        let ActivityMarker::Ended { outcome, .. } = marker else {
            return ActivityStep::nothing();
        };
        if self.cancel_after_effect {
            return self.wind_down(ctx);
        }
        match outcome {
            ActivityOutcome::Succeeded { .. } => ActivityStep::done(ActivityOutcome::Succeeded {
                summary: "board model saved — the board restarts to use it".to_string(),
            }),
            other => ActivityStep::done(ActivityOutcome::Failed {
                message: format!("the board model was not saved ({})", other.summary()),
            }),
        }
    }
}

impl ActivityReducer for SetBoardActivity {
    fn kind(&self) -> ActivityKind {
        ActivityKind::SetBoard
    }

    fn handle(&mut self, _now: Millis, input: &Input, ctx: &mut ActivityCtx<'_>) -> ActivityStep {
        match input {
            Input::Action(Action::CancelActivity { .. }) => {
                if !self.winding_down {
                    self.cancel_after_effect = true;
                }
                ActivityStep::nothing()
            }
            Input::Action(_) => ActivityStep::nothing(),
            Input::Event(event) => match event {
                Event::ActivityMarker { marker, .. } => self.handle_marker(marker, ctx),
                Event::Link {
                    event: LinkEvent::Closed { .. },
                    ..
                } if self.winding_down => ActivityStep::done(ActivityOutcome::Cancelled),
                _ => ActivityStep::nothing(),
            },
        }
    }

    fn next_deadline(&self) -> Option<Millis> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EffectId;
    use crate::evidence::Evidence;
    use crate::link::LinkId;
    use crate::roster::RosterConfig;

    #[test]
    fn it_writes_the_board_manifest_and_settles_on_the_write() {
        let config = RosterConfig::default();
        let evidence = Evidence::default();
        let mut activity = SetBoardActivity::new(DeviceId(1), "seeed/xiao-esp32-c6".into());
        let ctx = ctx(&evidence, &config);
        let commands = activity.spawn_commands(&ctx);
        assert!(matches!(
            commands.as_slice(),
            [Command::RunEffect {
                effect: EffectRequest::WriteBoardManifest { board_id, layout_verified: false },
                ..
            }] if board_id == "seeed/xiao-esp32-c6"
        ));
        let mut ctx = ctx;
        let step = activity.handle(
            Millis(10),
            &ended(ActivityOutcome::Succeeded {
                summary: "board manifest written".into(),
            }),
            &mut ctx,
        );
        assert!(matches!(
            step,
            ActivityStep::Done {
                outcome: ActivityOutcome::Succeeded { .. },
                ..
            }
        ));
    }

    #[test]
    fn a_failed_write_says_the_model_was_not_saved() {
        let config = RosterConfig::default();
        let evidence = Evidence::default();
        let mut activity = SetBoardActivity::new(DeviceId(1), "b".into());
        let mut ctx = ctx(&evidence, &config);
        let step = activity.handle(
            Millis(10),
            &ended(ActivityOutcome::Failed {
                message: "port lost".into(),
            }),
            &mut ctx,
        );
        let ActivityStep::Done {
            outcome: ActivityOutcome::Failed { message },
            ..
        } = step
        else {
            panic!("a failed write fails the activity");
        };
        assert!(message.contains("not saved"), "{message}");
    }

    #[test]
    fn a_cancel_waits_for_the_write_to_end() {
        let config = RosterConfig::default();
        let evidence = Evidence::default();
        let mut activity = SetBoardActivity::new(DeviceId(1), "b".into());
        let mut ctx = ctx(&evidence, &config);
        let cancel = Input::Action(Action::CancelActivity {
            device: DeviceId(1),
        });
        assert!(matches!(
            activity.handle(Millis(1), &cancel, &mut ctx),
            ActivityStep::Continue(ref c) if c.is_empty()
        ));
        let step = activity.handle(
            Millis(2),
            &ended(ActivityOutcome::Succeeded {
                summary: String::new(),
            }),
            &mut ctx,
        );
        assert!(matches!(step, ActivityStep::Continue(ref c) if matches!(
            c.as_slice(),
            [Command::Link { command: LinkCommand::Close, .. }]
        )));
    }

    fn ctx<'a>(evidence: &'a Evidence, config: &'a RosterConfig) -> ActivityCtx<'a> {
        ActivityCtx {
            link: Some(LinkId(1)),
            evidence,
            config,
            effect_id: EffectId(1),
        }
    }

    fn ended(outcome: ActivityOutcome) -> Input {
        Input::Event(Event::ActivityMarker {
            device: DeviceId(1),
            effect: Some(EffectId(1)),
            marker: ActivityMarker::Ended {
                kind: ActivityKind::SetBoard,
                outcome,
            },
        })
    }
}
