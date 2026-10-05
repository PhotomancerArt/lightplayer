//! When an update starts with no click (DS4), and when Studio only watches
//! (DS8): read off a board's standing (P4's [`update_standing`] rows), or —
//! for a link still pending adoption, which has no card to project a
//! standing from — off the same decision those rows are made of.
//!
//! | Standing | Board's decision | Here |
//! |---|---|---|
//! | `Restoring` / `Finishing`, not running | `Heal`, `ContinueUpdate` | start |
//! | `CantGetVersion` (a heal that found no engine) | `Heal` after a miss | start, unless the update host remembers the miss for this link ([`super::update_host::UpdateHost::auto_blocked`]) |
//! | `AnotherDevice`, not running | `Busy` | watch: ask `Q` every 3 s |
//! | anything else | anything else | leave it |
//!
//! **This Studio needs no build of its own to restore a board.** With none
//! installed (P8's web source is not yet), the decision is read against the
//! board's own build ([`board_build_facts`]): a board waiting for its engine
//! still decides `Heal`, and its engine comes from the cache or the store.

use lpa_devices::{DeviceId, Evidence, LinkId, UpdateFacts};
use lpa_update::{
    BoardView, Decision, HostBuildFacts, HostFacts, HostIdentity, HostPieceFacts, decide,
};
use lpc_access::Tier;

use super::device_update_standing::{UpdateStanding, wants_auto_start};
use super::update_host::UpdateHost;

/// What the controller does about a board's update with no click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoUpdate {
    /// Dispatch `Action::Update { intent: Auto }`.
    Start,
    /// Another device holds the transfer: ask the board again every 3 s.
    Watch,
    Leave,
}

/// The no-click verdict for a board with a card.
pub(crate) fn auto_update_for_standing(standing: &UpdateStanding) -> AutoUpdate {
    if wants_auto_start(standing) || matches!(standing, UpdateStanding::CantGetVersion { .. }) {
        return AutoUpdate::Start;
    }
    match standing {
        UpdateStanding::AnotherDevice { .. } if !standing.is_running() => AutoUpdate::Watch,
        _ => AutoUpdate::Leave,
    }
}

/// The no-click verdict for a link still pending adoption: the decision the
/// standing's rows are made of (see the module docs).
pub(crate) fn auto_update_for_board(
    facts: &UpdateFacts,
    own: Option<&HostBuildFacts>,
    tier: Option<Tier>,
) -> AutoUpdate {
    let Some(board) = BoardView::from_json(facts.manifest_json.as_bytes()) else {
        return AutoUpdate::Leave;
    };
    let derived;
    let build = match own {
        Some(own) => own,
        None => match board_build_facts(facts) {
            Some(facts) => {
                derived = facts;
                &derived
            }
            None => return AutoUpdate::Leave,
        },
    };
    let host = HostFacts {
        build,
        user_tier: tier,
        allow_downgrade: false,
    };
    match decide(&board, &host) {
        Decision::Heal { .. } | Decision::ContinueUpdate { .. } => AutoUpdate::Start,
        Decision::Busy { .. } => AutoUpdate::Watch,
        _ => AutoUpdate::Leave,
    }
}

/// What the controller does for one board this fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoAction {
    /// Dispatch `Action::Update { intent: Auto }` for it.
    Start(DeviceId),
    /// Ask it `Q` every 3 s over this link.
    Watch(DeviceId, LinkId),
}

/// A board's verdict made an action: a start the update host does not
/// hold off (a miss it remembers for this engine on this link window), or
/// a watch on the board's link.
pub(crate) fn auto_action(
    host: &UpdateHost,
    device: DeviceId,
    evidence: &Evidence,
    verdict: AutoUpdate,
) -> Option<AutoAction> {
    match verdict {
        AutoUpdate::Start => {
            let engine = evidence.update_facts().and_then(board_engine_sha);
            let blocked = host.auto_blocked(
                device,
                engine.as_deref(),
                evidence.link(),
                evidence.window_started_at().map(|at| at.0),
            );
            (!blocked).then_some(AutoAction::Start(device))
        }
        AutoUpdate::Watch => evidence.link().map(|link| AutoAction::Watch(device, link)),
        AutoUpdate::Leave => None,
    }
}

/// The board's own build, by the facts its manifest states: what the
/// decision is read against when this Studio has no build of its own (a
/// restore puts the board's own engine back, so its own build is the one in
/// question). `None` when the manifest does not parse or a hash is not hex.
pub(crate) fn board_build_facts(facts: &UpdateFacts) -> Option<HostBuildFacts> {
    let m = lpc_update::BoardManifest::from_json(facts.manifest_json.as_bytes()).ok()?;
    let identity = HostIdentity {
        target: m.target.clone(),
        chip: m.chip.clone(),
        version: m.version.clone(),
        build_id: m.build_id.clone(),
        wire_proto: m.wire_proto,
        layout: m.layout,
        min_loader: m.loader,
    };
    Some(HostBuildFacts::from_parts(
        identity,
        HostPieceFacts {
            sha256: lpc_update::sha256_from_hex(&m.core_sha256)?,
            len: m.core_len,
        },
        HostPieceFacts {
            sha256: lpc_update::sha256_from_hex(&m.engine_sha256)?,
            len: m.engine_len.unwrap_or(0),
        },
    ))
}

/// The engine the board's core needs (its manifest's `engineSha256`): what
/// a remembered miss is keyed by.
pub(crate) fn board_engine_sha(facts: &UpdateFacts) -> Option<String> {
    lpc_update::BoardManifest::from_json(facts.manifest_json.as_bytes())
        .ok()
        .map(|m| m.engine_sha256)
}

#[cfg(test)]
mod tests {
    use lpa_devices::UpdateBoardState;
    use lpc_update::{BoardManifest, BoardState, PieceKind, TransferView, sha256_to_hex};

    use super::*;
    use crate::app::devices::device_update_version::UpdateVersion;

    #[test]
    fn a_board_waiting_for_its_engine_starts_with_or_without_a_build_of_our_own() {
        let needs = facts(BoardState::NeedsEngine, None);
        assert_eq!(auto_update_for_board(&needs, None, None), AutoUpdate::Start);
        let own = board_build_facts(&facts(BoardState::Running, None)).unwrap();
        assert_eq!(
            auto_update_for_board(&needs, Some(&own), None),
            AutoUpdate::Start
        );
        let running = facts(BoardState::Running, None);
        assert_eq!(
            auto_update_for_board(&running, None, None),
            AutoUpdate::Leave
        );
        let crashing = facts(BoardState::EngineCrashing, None);
        assert_eq!(
            auto_update_for_board(&crashing, None, None),
            AutoUpdate::Leave,
            "a crash is reported, never healed on its own"
        );
    }

    #[test]
    fn another_devices_transfer_is_watched() {
        let busy = facts(
            BoardState::Updating,
            Some(TransferView {
                kind: PieceKind::Core,
                done: 4096,
                total: 20_000,
                busy: true,
                build_hash: 7,
            }),
        );
        assert_eq!(auto_update_for_board(&busy, None, None), AutoUpdate::Watch);
    }

    #[test]
    fn standings_map_onto_start_watch_and_leave() {
        let v = UpdateVersion::new("2026.10.03-1");
        let restoring = UpdateStanding::Restoring {
            board: v.clone(),
            percent: None,
            running: false,
        };
        assert_eq!(auto_update_for_standing(&restoring), AutoUpdate::Start);
        let running = UpdateStanding::Restoring {
            board: v.clone(),
            percent: None,
            running: true,
        };
        assert_eq!(auto_update_for_standing(&running), AutoUpdate::Leave);
        let missed = UpdateStanding::CantGetVersion {
            board: v.clone(),
            own: v.clone(),
            choices: vec![],
        };
        assert_eq!(auto_update_for_standing(&missed), AutoUpdate::Start);
        let other = UpdateStanding::AnotherDevice {
            board: v.clone(),
            to: None,
            percent: Some(40),
        };
        assert_eq!(auto_update_for_standing(&other), AutoUpdate::Watch);
        let available = UpdateStanding::Available {
            board: v.clone(),
            to: v,
        };
        assert_eq!(auto_update_for_standing(&available), AutoUpdate::Leave);
    }

    fn facts(state: BoardState, transfer: Option<TransferView>) -> UpdateFacts {
        let m = BoardManifest {
            proto: 1,
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.03-1".into(),
            build_id: "2026.10.03-1+aaaaaaaaaaaa".into(),
            wire_proto: 36,
            core_sha256: sha256_to_hex(&[0xaa; 32]),
            core_len: 18_000,
            engine_sha256: sha256_to_hex(&[0xae; 32]),
            engine_len: Some(38_000),
            layout: 1,
            loader: 1,
            region_len: 3_375_104,
            state,
            refused_build: None,
            transfer,
        };
        UpdateFacts {
            state: UpdateBoardState::Unknown,
            manifest_json: String::from_utf8(m.to_json()).unwrap(),
            ..UpdateFacts::default()
        }
    }
}
