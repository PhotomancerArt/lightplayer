//! **Who may open a session to a board through the relay.** The one place
//! this decision lives, behind one small interface
//! ([`RouteAdmissionPolicy`]), so it can be replaced or extended without
//! touching the hub or the board.
//!
//! The rule today is **interim** ([`InterimRouteAdmission`]; Yona,
//! 2026-10-06, the plan's Q1 as answered). Relay access is meant to be
//! governed later by cloud-side access settings for each board — a board
//! registry, owner sharing, share links — which the cloud does not have yet
//! (planning notes `lp2025/2026-10-06-1500-cloud-board-access/notes.md`).
//! That check implements this trait, or wraps this one.
//!
//! The interim rule:
//!
//! 1. **A signed-in browser session is required** — an account or a guest.
//!    It costs a real user nothing (Studio always has one) and gives the
//!    limit below something to hold on to. It could be relaxed later.
//! 2. **A member** — an account the board proved it holds the key of, i.e.
//!    one that plugged the board in by USB — reaches its board. The board
//!    then grants that account key's tier.
//! 3. **Anyone else (a visitor)** may also reach any online board, by its
//!    id. The board, not the relay, decides what a visitor gets: its
//!    "Anyone" (open) setting never applies over the relay
//!    (`LinkTrust::Relayed`), so a visitor holds nothing until it logs in
//!    with the board's own Play/Author password, inside the sealed session
//!    the relay cannot read.
//! 4. **Visitors, and every try at an id that is not online, are rate
//!    limited per address** ([`VisitorRateLimit`]): ids are MACs, not
//!    secrets, so this bounds a sweep for boards and the rate of password
//!    tries; the board's own login backoff bounds them again, device-wide.
//!
//! What is *not* access, and so not here: the board's route limit
//! ([`MAX_ROUTES_PER_BOARD`](lpc_relay::MAX_ROUTES_PER_BOARD) at the hub,
//! fewer if the board holds fewer) — that is capacity, the hub's.

use std::net::IpAddr;
use std::time::Instant;

use lpc_history::PrefixedUid;
use lpc_relay::{RelayBoardId, RelayCloseCode};

use super::relay_hub::OnlineBoard;
use super::visitor_rate_limit::VisitorRateLimit;

/// Everything an admission decision may look at.
#[derive(Debug, Clone, Copy)]
pub struct RouteRequest<'a> {
    /// The browser's signed-in account (or guest), if it has a session.
    pub user: Option<PrefixedUid>,
    /// The address the browser reached the relay from.
    pub ip: Option<IpAddr>,
    /// The board it asks for.
    pub board_id: RelayBoardId,
    /// That board, if it is online.
    pub board: Option<&'a OnlineBoard>,
    /// Now, for rate limits.
    pub now: Instant,
}

/// How a route was admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The browser is signed in to an account the board registered under.
    Member,
    /// Anyone else: the board will ask it for a password.
    Visitor,
}

/// Who may open a route: the whole access decision for the relay. The
/// refusal is the close code the browser leg ends with.
pub trait RouteAdmissionPolicy: Send {
    fn admit(&mut self, request: &RouteRequest<'_>) -> Result<Admission, RelayCloseCode>;
}

/// The interim rule (module doc).
#[derive(Debug, Default)]
pub struct InterimRouteAdmission {
    visitors: VisitorRateLimit,
}

impl InterimRouteAdmission {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl RouteAdmissionPolicy for InterimRouteAdmission {
    fn admit(&mut self, request: &RouteRequest<'_>) -> Result<Admission, RelayCloseCode> {
        let user = request.user.ok_or(RelayCloseCode::SignInRequired)?;
        let member = request.board.is_some_and(|board| board.has_account(user));
        if member {
            return Ok(Admission::Member);
        }
        if !self.visitors.take(request.ip, request.now) {
            return Err(RelayCloseCode::SlowDown);
        }
        if request.board.is_none() {
            return Err(RelayCloseCode::BoardOffline);
        }
        Ok(Admission::Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::relay_hub::{BoardRegistration, RelayHub};
    use crate::relay::visitor_rate_limit::BUCKET_CAPACITY;
    use lp_cloud_domain::BoardAccounts;
    use lpc_history::UidPrefix;

    #[test]
    fn no_session_is_refused_sign_in_required() {
        let hub = online();
        let answer = InterimRouteAdmission::new().admit(&request(&hub, None, BOARD, None));
        assert_eq!(answer, Err(RelayCloseCode::SignInRequired));
    }

    #[test]
    fn a_member_and_a_visitor_are_both_admitted_and_told_apart() {
        let hub = online();
        let mut policy = InterimRouteAdmission::new();
        assert_eq!(
            policy.admit(&request(&hub, Some(user(1)), BOARD, None)),
            Ok(Admission::Member)
        );
        assert_eq!(
            policy.admit(&request(&hub, Some(user(2)), BOARD, None)),
            Ok(Admission::Visitor)
        );
    }

    #[test]
    fn an_offline_board_is_refused_and_costs_a_try() {
        let hub = online();
        let mut policy = InterimRouteAdmission::new();
        let ip = Some("203.0.113.9".parse().unwrap());
        for _ in 0..BUCKET_CAPACITY {
            assert_eq!(
                policy.admit(&request(&hub, Some(user(1)), OFFLINE, ip)),
                Err(RelayCloseCode::BoardOffline),
                "even a member's try at an id that is not online"
            );
        }
        assert_eq!(
            policy.admit(&request(&hub, Some(user(2)), BOARD, ip)),
            Err(RelayCloseCode::SlowDown),
            "a visitor from a spent address"
        );
        assert_eq!(
            policy.admit(&request(&hub, Some(user(1)), BOARD, ip)),
            Ok(Admission::Member),
            "a member reaching its own board is never limited"
        );
    }

    const BOARD: RelayBoardId = RelayBoardId([0x10, 0xbd, 0, 0, 0, 1]);
    const OFFLINE: RelayBoardId = RelayBoardId([0x10, 0xbd, 0, 0, 0, 2]);

    fn user(n: u8) -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[n; 16])
    }

    fn request(
        hub: &RelayHub,
        user: Option<PrefixedUid>,
        board_id: RelayBoardId,
        ip: Option<IpAddr>,
    ) -> RouteRequest<'_> {
        RouteRequest {
            user,
            ip,
            board_id,
            board: hub.board(board_id),
            now: Instant::now(),
        }
    }

    /// A hub with `BOARD` online under user 1.
    fn online() -> RelayHub {
        let mut hub = RelayHub::new();
        hub.register(BoardRegistration {
            id: BOARD,
            leg: 10,
            accounts: BoardAccounts {
                users: vec![user(1)],
                accounts_ok: 1,
            },
            label: "Lamp".into(),
            wire_proto: 39,
            lan: None,
            public_ip: None,
            since: 1.0,
        })
        .unwrap();
        hub
    }
}
