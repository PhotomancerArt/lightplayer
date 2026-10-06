//! **Who may open a session to a board through the relay.** The one place
//! this rule lives; widen or narrow it here.
//!
//! The rule (Yona, 2026-10-06, the plan's Q1 as answered):
//!
//! 1. **A signed-in browser session is required** — an account or a guest.
//!    It costs a real user nothing (Studio always has one) and gives the
//!    limits below something to hold on to. It could be relaxed later.
//! 2. **A member** — an account the board proved it holds the key of, i.e.
//!    one that plugged the board in by USB — opens a route to its board.
//!    The board then grants that account key's tier.
//! 3. **Anyone else (a visitor)** may also open a route, to any online
//!    board, by its id. The board, not the relay, decides what a visitor
//!    gets: the board's "Anyone" (open) setting never applies over the
//!    relay (`LinkTrust::Relayed`), so a visitor holds nothing until it
//!    logs in with the board's own Play/Author password, inside the sealed
//!    session the relay cannot read.
//! 4. **Visitors, and every try at an id that is not online, are rate
//!    limited per address** ([`VisitorRateLimit`]): ids are MACs, not
//!    secrets, so this bounds a sweep for boards and the rate of password
//!    tries; the board's own login backoff bounds them again, device-wide.
//!
//! Then the board's route limit: [`MAX_ROUTES_PER_BOARD`](lpc_relay::MAX_ROUTES_PER_BOARD)
//! at the hub, fewer if the board holds fewer (it answers those `Busy`
//! itself).

use std::net::IpAddr;
use std::time::Instant;

use lpc_history::PrefixedUid;
use lpc_relay::{RelayBoardId, RelayCloseCode};

use super::relay_hub::{HubAction, LegId, OpenRefusal, RelayHub};
use super::visitor_rate_limit::VisitorRateLimit;

/// How a route was admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The browser is signed in to an account the board registered under.
    Member,
    /// Anyone else: the board will ask it for a password.
    Visitor,
}

/// A route the rule let through: its id, how, and the hub's actions (the
/// `Open` for the board).
#[derive(Debug)]
pub struct AdmittedRoute {
    pub route: u16,
    pub admission: Admission,
    pub actions: Vec<HubAction>,
}

/// Decide, and if yes open, a route for browser leg `browser` (signed in as
/// `user`, from `ip`) to board `board`. The refusal is the close code the
/// browser leg ends with.
pub fn admit_route(
    hub: &mut RelayHub,
    visitors: &mut VisitorRateLimit,
    browser: LegId,
    user: Option<PrefixedUid>,
    ip: Option<IpAddr>,
    board: RelayBoardId,
    now: Instant,
) -> Result<AdmittedRoute, RelayCloseCode> {
    let user = user.ok_or(RelayCloseCode::SignInRequired)?;
    let member = hub
        .board(board)
        .is_some_and(|online| online.has_account(user));
    if !member && !visitors.take(ip, now) {
        return Err(RelayCloseCode::SlowDown);
    }
    let (route, actions) = hub
        .open_route(browser, board)
        .map_err(|refusal| match refusal {
            OpenRefusal::Offline => RelayCloseCode::BoardOffline,
            OpenRefusal::Busy => RelayCloseCode::Busy,
        })?;
    Ok(AdmittedRoute {
        route,
        admission: if member {
            Admission::Member
        } else {
            Admission::Visitor
        },
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::relay_hub::BoardRegistration;
    use crate::relay::visitor_rate_limit::BUCKET_CAPACITY;
    use lp_cloud_domain::BoardAccounts;
    use lpc_history::UidPrefix;

    #[test]
    fn no_session_is_refused_sign_in_required() {
        let (mut hub, mut visitors) = online();
        let answer = admit_route(
            &mut hub,
            &mut visitors,
            100,
            None,
            None,
            BOARD,
            Instant::now(),
        );
        assert_eq!(answer.unwrap_err(), RelayCloseCode::SignInRequired);
    }

    #[test]
    fn a_member_and_a_visitor_are_both_admitted_and_told_apart() {
        let (mut hub, mut visitors) = online();
        let now = Instant::now();
        let member = admit_route(
            &mut hub,
            &mut visitors,
            100,
            Some(user(1)),
            None,
            BOARD,
            now,
        );
        assert_eq!(member.unwrap().admission, Admission::Member);
        let visitor = admit_route(
            &mut hub,
            &mut visitors,
            101,
            Some(user(2)),
            None,
            BOARD,
            now,
        );
        assert_eq!(visitor.unwrap().admission, Admission::Visitor);
    }

    #[test]
    fn visitors_and_offline_tries_are_rate_limited_and_members_are_not() {
        let (mut hub, mut visitors) = online();
        let now = Instant::now();
        let ip = Some("203.0.113.9".parse().unwrap());
        for leg in 0..u64::from(BUCKET_CAPACITY) {
            let answer = admit_route(
                &mut hub,
                &mut visitors,
                200 + leg,
                Some(user(2)),
                ip,
                OFFLINE,
                now,
            );
            assert_eq!(answer.unwrap_err(), RelayCloseCode::BoardOffline);
        }
        let answer = admit_route(&mut hub, &mut visitors, 300, Some(user(2)), ip, BOARD, now);
        assert_eq!(
            answer.unwrap_err(),
            RelayCloseCode::SlowDown,
            "a visitor from a spent address"
        );
        let answer = admit_route(&mut hub, &mut visitors, 301, Some(user(1)), ip, BOARD, now);
        assert_eq!(
            answer.unwrap().admission,
            Admission::Member,
            "a member is never limited"
        );
    }

    #[test]
    fn the_board_route_limit_answers_busy() {
        let (mut hub, mut visitors) = online();
        let now = Instant::now();
        for leg in 0..lpc_relay::MAX_ROUTES_PER_BOARD as u64 {
            admit_route(
                &mut hub,
                &mut visitors,
                100 + leg,
                Some(user(1)),
                None,
                BOARD,
                now,
            )
            .unwrap();
        }
        let answer = admit_route(
            &mut hub,
            &mut visitors,
            200,
            Some(user(1)),
            None,
            BOARD,
            now,
        );
        assert_eq!(answer.unwrap_err(), RelayCloseCode::Busy);
    }

    const BOARD: RelayBoardId = RelayBoardId([0x10, 0xbd, 0, 0, 0, 1]);
    const OFFLINE: RelayBoardId = RelayBoardId([0x10, 0xbd, 0, 0, 0, 2]);

    fn user(n: u8) -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[n; 16])
    }

    /// A hub with `BOARD` online under user 1.
    fn online() -> (RelayHub, VisitorRateLimit) {
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
        (hub, VisitorRateLimit::new())
    }
}
