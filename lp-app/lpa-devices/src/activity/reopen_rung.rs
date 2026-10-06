//! The reopen rung: how an activity knocks while it waits for a board to
//! come back after a reset.
//!
//! Shared by the two activities that outlive a reset they caused: the
//! Flash's post-write ladder (its first rung, before any reset escalates)
//! and the Update's gaps between legs. Both face the same physics. A
//! USB-Serial-JTAG chip (the C6) re-enumerates on every reset, so the port
//! under the link dies exactly when the work succeeded; session adoption in
//! the platform layer re-derives the handle, so an open that fails now
//! succeeds a moment later. And a board that booted while the port was down
//! never volunteers its hello again — each knock on an open port therefore
//! *asks* for one.
//!
//! The rung is a cadence, not a loop: the caller keeps `next_poke_at` in its
//! own (serialized) phase, and [`knock_when_due`] fires at most once per
//! cadence step. All waiting is the caller's scheduled timer (I7).
//!
//! ⚠️ What answers a knock has to be NEWER than the work. The observation
//! window survives a close, so a hello heard before the reset is still in
//! the evidence while the port is being reopened — see
//! [`hello_heard_since`].

use crate::event::Command;
use crate::evidence::Evidence;
use crate::link::LinkCommand;
use crate::time::Millis;
use crate::wire::ClientFrame;

use super::activity_cell::ActivityCtx;

/// What a knock on a CLOSED port does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClosedPort {
    /// Reopen it: a serial port re-enumerated under us, and session adoption
    /// makes one of these opens succeed.
    Reopen,
    /// Leave it: the transport reconnects by itself (a Bluetooth provider's
    /// own loop), and a second open would only fight it.
    Wait,
}

/// Knock once if the cadence says so: on an open port ask for a hello, on a
/// closed one reopen it (or wait, per `closed`). Moves `next_poke_at` one
/// retry step on when it fires. `None` = not due yet.
pub(crate) fn knock_when_due(
    now: Millis,
    next_poke_at: &mut Millis,
    ctx: &ActivityCtx<'_>,
    next_request_id: &mut u32,
    closed: ClosedPort,
) -> Option<Vec<Command>> {
    if now < *next_poke_at {
        return None;
    }
    *next_poke_at = now.plus_ms(ctx.config.flash_reopen_retry_ms);
    Some(knock(ctx, next_request_id, closed))
}

/// One knock, now: ask an open, quiet port for a hello (a connect cannot
/// assume the power to cause a boot), or knock on a closed one.
pub(crate) fn knock(
    ctx: &ActivityCtx<'_>,
    next_request_id: &mut u32,
    closed: ClosedPort,
) -> Vec<Command> {
    match (ctx.evidence.presence.is_open(), closed) {
        (true, _) => ask_hello(ctx, next_request_id),
        (false, ClosedPort::Reopen) => open_port(ctx),
        (false, ClosedPort::Wait) => Vec::new(),
    }
}

/// Open the device's link at the configured baud.
pub(crate) fn open_port(ctx: &ActivityCtx<'_>) -> Vec<Command> {
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

/// Ask the board for a hello, minting the request id from the caller's
/// counter.
pub(crate) fn ask_hello(ctx: &ActivityCtx<'_>, next_request_id: &mut u32) -> Vec<Command> {
    let Some(link) = ctx.link else {
        return Vec::new();
    };
    let request_id = *next_request_id;
    *next_request_id += 1;
    vec![Command::Link {
        link,
        command: LinkCommand::SendFrame(ClientFrame::hello(request_id)),
    }]
}

/// Whether the fold heard a hello at or after `since`. `has_hello` alone is
/// not the question — the window survives a close, so a board that said
/// hello before the reset still carries it while its port is reopened
/// (bench, 2026-09-04).
pub(crate) fn hello_heard_since(evidence: &Evidence, since: Millis) -> bool {
    evidence
        .hello_heard_at()
        .is_some_and(|heard_at| heard_at >= since)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::Evidence;
    use crate::link::{LinkId, LinkInfo};
    use crate::roster::RosterConfig;

    #[test]
    fn a_closed_port_is_reopened_or_left_to_reconnect_by_itself() {
        let config = RosterConfig::default();
        let evidence = Evidence::default();
        let ctx = ctx(&evidence, &config);
        let mut ids = 1;
        assert!(matches!(
            knock(&ctx, &mut ids, ClosedPort::Reopen).as_slice(),
            [Command::Link {
                command: LinkCommand::Open { .. },
                ..
            }]
        ));
        assert!(knock(&ctx, &mut ids, ClosedPort::Wait).is_empty());
    }

    #[test]
    fn an_open_port_is_asked_for_a_hello_and_the_cadence_holds() {
        let config = RosterConfig::default();
        let mut evidence = Evidence::default();
        let mut identity = crate::identity::IdentityChain::default();
        evidence.fold(
            Millis(0),
            &crate::event::Event::Link {
                link: LinkId(1),
                event: crate::link::LinkEvent::Opened {
                    info: LinkInfo::default(),
                },
            },
            &mut identity,
            &config,
        );
        let ctx = ctx(&evidence, &config);
        let mut ids = 1;
        let mut next = Millis(100);
        assert_eq!(
            knock_when_due(Millis(50), &mut next, &ctx, &mut ids, ClosedPort::Wait),
            None
        );
        let commands =
            knock_when_due(Millis(100), &mut next, &ctx, &mut ids, ClosedPort::Wait).expect("due");
        assert!(matches!(
            commands.as_slice(),
            [Command::Link {
                command: LinkCommand::SendFrame(_),
                ..
            }]
        ));
        assert_eq!(next, Millis(100 + config.flash_reopen_retry_ms));
        assert_eq!(ids, 2, "each ask mints its own request id");
    }

    fn ctx<'a>(evidence: &'a Evidence, config: &'a RosterConfig) -> ActivityCtx<'a> {
        ActivityCtx {
            link: Some(LinkId(1)),
            evidence,
            config,
            effect_id: crate::event::EffectId(1),
        }
    }
}
