//! The update channel's facts, in the device model's vocabulary (DS2): a
//! board manifest (`lpc_update::BoardManifest`) → [`UpdateFacts`].
//!
//! A board speaks its manifest twice — as `M` on lp-link channel 3 (the
//! board's own answer, on link-up and when asked) and, from update protocol
//! Part B, as the hello's `firmware` field. Both are decoded here, the one
//! place this crate's wire vocabulary meets `lpa-devices`' serde-only
//! mirror ([`crate::device_link::wire`] is the other half of that seam),
//! and every channel-3 transport turns what the board sent into events with
//! [`update_events`]:
//!
//! | board message | events |
//! |---|---|
//! | `M` (a manifest that parses) | [`LinkEvent::UpdateFacts`], then [`LinkEvent::Update`] |
//! | anything else on channel 3 | [`LinkEvent::Update`] (the update host's alone) |
//!
//! The manifest's JSON rides along verbatim in
//! [`UpdateFacts::manifest_json`] for `lpa-studio-core`, which parses it for
//! the update decision.

use lpa_devices::link::LinkEvent;
use lpa_devices::{UpdateBoardState, UpdateFacts, UpdatePieceKind, UpdateTransferFacts};
use lpc_update::{BoardManifest, BoardMessage, BoardState, PieceKind};

/// The events one channel-3 message from the board is: its decoded facts
/// when it is a manifest, then the bytes themselves for the update host.
pub fn update_events(bytes: Vec<u8>) -> Vec<LinkEvent> {
    let facts = match BoardMessage::decode(&bytes) {
        Ok(BoardMessage::Manifest(json)) => update_facts_from_manifest_json(json),
        _ => None,
    };
    facts
        .map(LinkEvent::UpdateFacts)
        .into_iter()
        .chain(core::iter::once(LinkEvent::Update(bytes)))
        .collect()
}

/// The model's mirror of a manifest's JSON (`M`'s payload), the JSON kept
/// verbatim; `None` when it does not parse.
pub fn update_facts_from_manifest_json(json: &[u8]) -> Option<UpdateFacts> {
    let manifest = BoardManifest::from_json(json).ok()?;
    Some(mirror(
        &manifest,
        String::from_utf8_lossy(json).into_owned(),
    ))
}

/// The model's mirror of a manifest already decoded (the hello's
/// `firmware`), its JSON re-encoded as `M` would carry it.
pub fn update_facts_from_manifest(manifest: &BoardManifest) -> UpdateFacts {
    let json = String::from_utf8_lossy(&manifest.to_json()).into_owned();
    mirror(manifest, json)
}

fn mirror(m: &BoardManifest, manifest_json: String) -> UpdateFacts {
    UpdateFacts {
        state: match m.state {
            BoardState::Running => UpdateBoardState::Running,
            BoardState::NeedsEngine => UpdateBoardState::NeedsEngine,
            BoardState::EngineCrashing => UpdateBoardState::EngineCrashing,
            BoardState::Updating => UpdateBoardState::Updating,
            BoardState::OnTrial => UpdateBoardState::OnTrial,
            BoardState::Unknown => UpdateBoardState::Unknown,
        },
        version: Some(m.version.clone()),
        target: Some(m.target.clone()),
        build_id: Some(m.build_id.clone()),
        transfer: m.transfer.map(|t| UpdateTransferFacts {
            kind: match t.kind {
                PieceKind::Core => UpdatePieceKind::Core,
                PieceKind::Engine => UpdatePieceKind::Engine,
            },
            done: t.done,
            total: t.total,
            busy: t.busy,
            build_hash: t.build_hash,
        }),
        refused_build: m.refused_build,
        manifest_json,
    }
}

#[cfg(test)]
mod tests {
    use lpc_update::TransferView;

    use super::*;

    #[test]
    fn a_manifest_is_its_facts_then_its_bytes() {
        let manifest = example();
        let mut m = vec![b'M'];
        m.extend_from_slice(&manifest.to_json());
        let events = update_events(m.clone());
        let [LinkEvent::UpdateFacts(facts), LinkEvent::Update(bytes)] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_eq!(bytes, &m);
        assert_eq!(facts.state, UpdateBoardState::Updating);
        assert_eq!(facts.version.as_deref(), Some("2026.10.05-3"));
        assert_eq!(facts.target.as_deref(), Some("esp32c6-4mb"));
        assert_eq!(facts.build_id.as_deref(), Some("2026.10.05-3+abc123456789"));
        assert_eq!(facts.refused_build, Some(7));
        let transfer = facts.transfer.expect("a transfer");
        assert_eq!(transfer.kind, UpdatePieceKind::Engine);
        assert_eq!((transfer.done, transfer.total), (4096, 8192));
        assert!(transfer.busy);
        assert_eq!(
            BoardManifest::from_json(facts.manifest_json.as_bytes()).unwrap(),
            manifest
        );
    }

    #[test]
    fn any_other_message_is_only_its_bytes() {
        let events = update_events(vec![b'R', 1, 2, 3]);
        assert!(
            matches!(events.as_slice(), [LinkEvent::Update(bytes)] if bytes[0] == b'R'),
            "{events:?}"
        );
        // A manifest that does not parse is still the update host's bytes.
        let events = update_events(b"M{not json".to_vec());
        assert!(
            matches!(events.as_slice(), [LinkEvent::Update(_)]),
            "{events:?}"
        );
    }

    #[test]
    fn the_hellos_manifest_mirrors_the_same_as_m() {
        let manifest = example();
        assert_eq!(
            update_facts_from_manifest(&manifest),
            update_facts_from_manifest_json(&manifest.to_json()).unwrap()
        );
    }

    fn example() -> BoardManifest {
        BoardManifest {
            proto: 1,
            target: "esp32c6-4mb".to_string(),
            chip: "esp32c6".to_string(),
            version: "2026.10.05-3".to_string(),
            build_id: "2026.10.05-3+abc123456789".to_string(),
            wire_proto: 38,
            core_sha256: "11".repeat(32),
            core_len: 1_160_000,
            engine_sha256: "22".repeat(32),
            engine_len: Some(1_830_000),
            layout: 1,
            loader: 1,
            region_len: 3_375_104,
            state: BoardState::Updating,
            refused_build: Some(7),
            transfer: Some(TransferView {
                kind: PieceKind::Engine,
                done: 4096,
                total: 8192,
                busy: true,
                build_hash: 9,
            }),
        }
    }
}
