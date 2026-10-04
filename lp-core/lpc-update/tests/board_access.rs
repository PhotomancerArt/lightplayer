//! The board session's links, access, login, ownership, manifest and
//! read-back (A-P4), against the NOR model.

mod support;

use lpc_access::{DeviceAccessFile, LoginMac, OpenTo, SecretEntry, Tier, derive_login_key};
use lpc_update::board::{
    AccessFacts, CORE_INSTALL_FOLLOWS_OPEN_TO, LinkId, LinkTrust, SessionConfig, SessionMode,
};
use lpc_update::code_table::CHUNK;
use lpc_update::testing::{BoardRig, FakeBoard, MODEL_PROGRESS, MODEL_REGION_START, ModelBuild};
use lpc_update::transfer_record::{RecordRead, RecordStage, TransferRecord};
use lpc_update::{
    BoardLoginStep, BoardMessage, BoardState, ChunkRef, HostLoginStep, HostMessage, PieceKind,
    ReadBackRequest, Refusal, tier_from_code,
};

use support::{Host, RADIO, RADIO_2, REGION, USB, drive, exchange, offer_of, rig_with, x_and_y};

const PASSWORD: &[u8] = b"correct horse";
const ITERATIONS: u32 = 64;

fn nonce(buf: &mut [u8]) {
    buf.fill(0x5a);
}

fn config() -> SessionConfig {
    SessionConfig {
        entropy: Some(nonce),
        ..SessionConfig::default()
    }
}

fn secret(tier: Tier, salt: u8) -> SecretEntry {
    SecretEntry::from_password("mine", tier, PASSWORD, [salt; 16], ITERATIONS)
}

fn access(open: OpenTo, follows: bool) -> AccessFacts {
    AccessFacts {
        secrets: vec![secret(Tier::Play, 1), secret(Tier::Edit, 2)],
        open,
        core_install_follows_open_to: follows,
    }
}

fn engine_start(core_len: usize) -> u32 {
    (MODEL_REGION_START + core_len as u32).div_ceil(CHUNK) * CHUNK
}

/// A board holding X with its engine header erased (core-only).
fn engineless_rig(access: AccessFacts, catalog: Vec<ModelBuild>) -> BoardRig {
    let x = catalog[0].clone();
    let mut board = FakeBoard::flashed_with(catalog, 0, REGION);
    board
        .flash
        .flash_image(engine_start(x.core.len()), &[0xFF; CHUNK as usize]);
    BoardRig::new(board, access, config()).unwrap()
}

/// Core-only X whose engine crashes (E10): its header is valid.
fn crashing_rig(access: AccessFacts) -> (BoardRig, ModelBuild) {
    let (x, _) = x_and_y();
    let x = x.crashing();
    let board = FakeBoard::flashed_with(vec![x.clone()], 0, REGION);
    (BoardRig::new(board, access, config()).unwrap(), x)
}

/// Every message `link` gets for `bytes`, decoded as board messages.
fn say(rig: &mut BoardRig, now: u64, link: LinkId, bytes: &[u8]) -> Vec<Vec<u8>> {
    rig.deliver(now, link, None, bytes)
        .into_iter()
        .map(|o| {
            assert_eq!(o.link, link);
            o.bytes
        })
        .collect()
}

fn refusal(bytes: &[u8]) -> Option<Refusal> {
    match BoardMessage::decode(bytes) {
        Ok(BoardMessage::Refusal(r)) => Some(r),
        _ => None,
    }
}

fn manifest(rig: &BoardRig, now: u64, link: LinkId) -> lpc_update::BoardManifest {
    rig.session.as_ref().unwrap().manifest_for(now, link)
}

/// Log in on `link` with `password`; the verdict's tier.
fn login(rig: &mut BoardRig, now: u64, link: LinkId, password: &[u8]) -> (Option<Tier>, u32) {
    let out = say(rig, now, link, &HostLoginStep::Begin.encode());
    let Ok(BoardMessage::Login(BoardLoginStep::Challenge { nonce, offers })) =
        BoardMessage::decode(&out[0])
    else {
        return verdict(&out[0]);
    };
    let macs = offers
        .iter()
        .map(|o| LoginMac::compute(&derive_login_key(password, &o.salt, o.iterations), &nonce).0)
        .collect();
    let out = say(rig, now, link, &HostLoginStep::Answer { macs }.encode());
    verdict(&out[0])
}

fn verdict(bytes: &[u8]) -> (Option<Tier>, u32) {
    match BoardMessage::decode(bytes) {
        Ok(BoardMessage::Login(BoardLoginStep::Verdict {
            tier_code,
            retry_after_ms,
        })) => (tier_from_code(tier_code), retry_after_ms),
        other => panic!("not a verdict: {other:?}"),
    }
}

// ---- Access ---------------------------------------------------------------------

#[test]
fn qy2_ships_yes() {
    assert!(CORE_INSTALL_FOLLOWS_OPEN_TO);
}

#[test]
fn a_core_install_follows_open_to_under_yes_and_needs_a_login_under_no() {
    let (x, y) = x_and_y();
    for follows in [true, false] {
        let mut rig = engineless_rig(access(OpenTo::Edit, follows), vec![x.clone(), y.clone()]);
        rig.link_up(0, RADIO, LinkTrust::Untrusted);
        let out = say(&mut rig, 1, RADIO, &offer_of(&y).encode());
        if follows {
            assert!(
                refusal(&out[0]).is_none(),
                "open at Author takes a core: {out:?}"
            );
        } else {
            assert_eq!(refusal(&out[0]), Some(Refusal::Access));
            assert_eq!(login(&mut rig, 2, RADIO, PASSWORD).0, Some(Tier::Edit));
            let out = say(&mut rig, 3, RADIO, &offer_of(&y).encode());
            assert!(refusal(&out[0]).is_none(), "after a login it is accepted");
        }
        // USB is trusted under either answer, and a heal needs nothing.
        let mut rig = engineless_rig(access(OpenTo::Nobody, follows), vec![x.clone(), y.clone()]);
        rig.link_up(0, USB, LinkTrust::Trusted);
        assert!(refusal(&say(&mut rig, 1, USB, &offer_of(&y).encode())[0]).is_none());
        let mut rig = engineless_rig(access(OpenTo::Nobody, follows), vec![x.clone()]);
        rig.link_up(0, RADIO, LinkTrust::Untrusted);
        assert!(refusal(&say(&mut rig, 1, RADIO, &offer_of(&x).encode())[0]).is_none());
    }
}

#[test]
fn a_core_install_without_edit_is_refused_and_a_login_at_edit_lets_it_through() {
    let (x, y) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Play, true), vec![x, y.clone()]);
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    assert_eq!(
        refusal(&say(&mut rig, 1, RADIO, &offer_of(&y).encode())[0]),
        Some(Refusal::Access)
    );
    // A key at play is not enough either; a key at edit is.
    rig.link_up(2, RADIO_2, LinkTrust::Keyed(Tier::Play));
    assert_eq!(
        refusal(&say(&mut rig, 3, RADIO_2, &offer_of(&y).encode())[0]),
        Some(Refusal::Access)
    );
    rig.link_up(4, RADIO_2, LinkTrust::Keyed(Tier::Edit));
    assert!(refusal(&say(&mut rig, 5, RADIO_2, &offer_of(&y).encode())[0]).is_none());
}

#[test]
fn a_store_the_core_cannot_read_is_locked_yet_an_engine_install_proceeds() {
    let (x, y) = x_and_y();
    // A newer shape after a rollback, or damage: locked.
    let newer = br#"{"version": 4, "secrets": [], "open": "edit", "bleEnabled": true, "shiny": 1}"#;
    let facts = AccessFacts::from_store(Some(newer));
    assert_eq!(facts.open, DeviceAccessFile::locked().open);
    let mut rig = engineless_rig(facts.clone(), vec![x.clone(), y.clone()]);
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    assert_eq!(
        refusal(&say(&mut rig, 1, RADIO, &offer_of(&y).encode())[0]),
        Some(Refusal::Access)
    );
    let mut host = Host::new(x.clone());
    exchange(&mut rig, &mut host, RADIO, offer_of(&x).encode(), &mut 2);
    assert!(host.refusals.is_empty());
    assert!(rig.reset_pending, "the heal committed");
}

// ---- Login ------------------------------------------------------------------------

#[test]
fn a_good_mac_grants_the_secrets_tier_for_that_link_only() {
    let (x, y) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody, true), vec![x, y.clone()]);
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    rig.link_up(0, RADIO_2, LinkTrust::Untrusted);
    assert_eq!(login(&mut rig, 1, RADIO, PASSWORD), (Some(Tier::Edit), 0));
    assert_eq!(
        refusal(&say(&mut rig, 2, RADIO_2, &offer_of(&y).encode())[0]),
        Some(Refusal::Access)
    );
    assert!(refusal(&say(&mut rig, 3, RADIO, &offer_of(&y).encode())[0]).is_none());
}

#[test]
fn a_wrong_password_is_refused_with_backoff() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody, true), vec![x]);
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    let mut waits = Vec::new();
    for i in 0..5 {
        let (tier, wait) = login(&mut rig, 100 + i, RADIO, b"wrong");
        assert_eq!(tier, None);
        waits.push(wait);
    }
    // lpc-access's RateLimit: three free attempts, then 2 s; begin is then
    // refused until the wait is over.
    assert_eq!(&waits[..4], &[0, 0, 0, 2_000]);
    assert!(waits[4] > 0);
    assert_eq!(login(&mut rig, 20_000, RADIO, PASSWORD).0, Some(Tier::Edit));
}

#[test]
fn an_expired_challenge_and_a_second_begin_are_refused() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody, true), vec![x]);
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    rig.link_up(0, RADIO_2, LinkTrust::Untrusted);
    let out = say(&mut rig, 1, RADIO, &HostLoginStep::Begin.encode());
    assert!(matches!(
        BoardMessage::decode(&out[0]),
        Ok(BoardMessage::Login(BoardLoginStep::Challenge { .. }))
    ));
    // One login in flight per device: another link waits for it.
    let out = say(&mut rig, 2, RADIO_2, &HostLoginStep::Begin.encode());
    let (tier, wait) = verdict(&out[0]);
    assert_eq!(tier, None);
    assert!(wait > 0 && wait <= 30_000);
    // Another link cannot answer the first link's challenge.
    let out = say(
        &mut rig,
        3,
        RADIO_2,
        &HostLoginStep::Answer {
            macs: vec![[0; 32]; 2],
        }
        .encode(),
    );
    assert_eq!(verdict(&out[0]).0, None);
    // Answered after 30 s: expired.
    let macs = vec![[0; 32]; 2];
    let out = say(
        &mut rig,
        40_000,
        RADIO,
        &HostLoginStep::Answer { macs }.encode(),
    );
    assert_eq!(verdict(&out[0]).0, None);
}

#[test]
fn a_link_going_down_frees_its_challenge() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody, true), vec![x]);
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    rig.link_up(0, RADIO_2, LinkTrust::Untrusted);
    say(&mut rig, 1, RADIO, &HostLoginStep::Begin.encode());
    rig.link_down(2, RADIO);
    assert_eq!(login(&mut rig, 3, RADIO_2, PASSWORD).0, Some(Tier::Edit));
}

#[test]
fn with_no_entropy_every_login_is_refused() {
    let (x, _) = x_and_y();
    let board = FakeBoard::flashed_with(vec![x.clone()], 0, REGION);
    let mut rig = BoardRig::new(
        board,
        access(OpenTo::Nobody, true),
        SessionConfig::default(),
    )
    .unwrap();
    // The factory board runs its engine; make it core-only.
    rig.board
        .flash
        .flash_image(engine_start(x.core.len()), &[0xFF; CHUNK as usize]);
    rig.reboot().unwrap();
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    assert_eq!(login(&mut rig, 1, RADIO, PASSWORD), (None, 0));
}

// ---- Two links: ownership ------------------------------------------------------------

/// Start a heal on `RADIO`, answer `n` chunks, and stop answering.
fn half_a_heal(rig: &mut BoardRig, x: &ModelBuild, n: usize) {
    let mut host = Host::new(x.clone());
    let mut to_board = vec![offer_of(x).encode()];
    let mut now = 10;
    for _ in 0..=n {
        let mut next = Vec::new();
        for m in to_board.drain(..) {
            now += 1;
            for out in say(rig, now, RADIO, &m) {
                next.extend(host.on_board(&out));
            }
        }
        to_board = next;
    }
}

#[test]
fn a_live_owner_keeps_its_transfer_and_another_link_gets_busy() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody, true), vec![x.clone()]);
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    rig.link_up(0, RADIO_2, LinkTrust::Untrusted);
    half_a_heal(&mut rig, &x, 3);
    let out = say(&mut rig, 100, RADIO_2, &offer_of(&x).encode());
    assert_eq!(
        refusal(&out[0]),
        Some(Refusal::Busy {
            done: 3 * CHUNK,
            total: x.engine.len() as u32
        })
    );
    let m = manifest(&rig, 100, RADIO_2);
    assert_eq!(m.state, BoardState::Updating);
    assert!(m.transfer.unwrap().busy, "busy as the other link sees it");
    assert!(
        !manifest(&rig, 100, RADIO).transfer.unwrap().busy,
        "not to its owner"
    );
    // Data from the other link is ignored.
    let ops = rig.board.flash.ops();
    let chunk = lpc_update::encode_chunk(
        lpc_update::ChunkEncoding::Raw,
        PieceKind::Engine,
        4 * CHUNK,
        &x.engine[4 * 4096..5 * 4096],
    );
    assert!(say(&mut rig, 101, RADIO_2, &chunk).is_empty());
    assert_eq!(rig.board.flash.ops(), ops);
}

#[test]
fn another_link_takes_over_when_the_owner_drops_or_goes_quiet() {
    let (x, _) = x_and_y();
    for drop_it in [true, false] {
        let mut rig = engineless_rig(access(OpenTo::Nobody, true), vec![x.clone()]);
        rig.link_up(0, RADIO, LinkTrust::Untrusted);
        rig.link_up(0, RADIO_2, LinkTrust::Untrusted);
        half_a_heal(&mut rig, &x, 3);
        let now = if drop_it {
            rig.link_down(50, RADIO);
            51
        } else {
            // Quiet for 15 s since its last message.
            14 + 15_000
        };
        let out = say(&mut rig, now, RADIO_2, &offer_of(&x).encode());
        let Ok(BoardMessage::Request(r)) = BoardMessage::decode(&out[0]) else {
            panic!("no takeover: {out:?}");
        };
        assert_eq!(
            (r.kind, r.off),
            (PieceKind::Engine, 4 * CHUNK),
            "at the right offset"
        );
        let mut host = Host::new(x.clone());
        exchange(
            &mut rig,
            &mut host,
            RADIO_2,
            offer_of(&x).encode(),
            &mut 100_000,
        );
        assert!(rig.reset_pending);
    }
}

#[test]
fn a_heal_during_a_pending_core_transfer_cancels_it() {
    // E2: the running engine took Y and handed over; the host that comes back
    // holds only X, and heals.
    let (x, y) = x_and_y();
    let mut rig = rig_with(
        vec![x.clone(), y.clone()],
        0,
        access(OpenTo::Nobody, true),
        config(),
    );
    rig.link_up(0, USB, LinkTrust::Trusted);
    let out = say(&mut rig, 1, USB, &offer_of(&y).encode());
    assert!(out.is_empty());
    assert!(rig.reset_pending);
    rig.reboot().unwrap();
    assert!(rig.session.as_ref().unwrap().transferring());
    let mut host = Host::new(x.clone());
    rig.link_up(2, RADIO, LinkTrust::Untrusted);
    assert_eq!(manifest(&rig, 2, RADIO).state, BoardState::Updating);
    drive(&mut rig, &mut host, RADIO, LinkTrust::Untrusted, &mut 3);
    assert_eq!(rig.board.running_build(), Some(&x));
    let at = MODEL_PROGRESS as usize;
    assert_eq!(
        TransferRecord::read(&rig.board.flash.bytes()[at..at + 4096]),
        RecordRead::Nothing,
        "the pending record is gone"
    );
}

// ---- The manifest -------------------------------------------------------------------

#[test]
fn the_manifest_reports_each_state_and_every_identity_field() {
    let (x, y) = x_and_y();
    // Running.
    let mut rig = rig_with(
        vec![x.clone(), y.clone()],
        0,
        access(OpenTo::Edit, true),
        config(),
    );
    let m = manifest(&rig, 0, USB);
    assert_eq!(m.state, BoardState::Running);
    assert_eq!(
        (m.proto, m.target.as_str(), m.chip.as_str()),
        (1, "esp32c6-4mb", "esp32c6")
    );
    assert_eq!(
        (m.version.as_str(), m.build_id.as_str()),
        (x.version.as_str(), x.build_id.as_str())
    );
    assert_eq!(m.core_sha256, lpc_update::sha256_to_hex(&x.core_sha256()));
    assert_eq!(m.core_len, x.core.len() as u32);
    assert_eq!(
        m.engine_sha256,
        lpc_update::sha256_to_hex(&x.engine_sha256())
    );
    assert_eq!(m.engine_len, Some(x.engine.len() as u32));
    assert_eq!(
        (m.layout, m.loader, m.wire_proto, m.region_len),
        (1, 1, 36, REGION)
    );
    assert_eq!((m.refused_build, m.transfer), (None, None));
    // `Q` answers it on any link, while the engine runs too.
    let out = say(&mut rig, 1, RADIO, &[b'Q', 1]);
    assert!(matches!(
        BoardMessage::decode(&out[0]),
        Ok(BoardMessage::Manifest(_))
    ));

    // Updating (the pending transfer), then on trial, then running Y.
    rig.link_up(1, USB, LinkTrust::Trusted);
    say(&mut rig, 1, USB, &offer_of(&y).encode());
    rig.reboot().unwrap();
    let m = manifest(&rig, 2, USB);
    assert_eq!(m.state, BoardState::Updating);
    assert_eq!(m.engine_len, None, "the header is gone");
    let t = m.transfer.unwrap();
    assert_eq!(
        (t.kind, t.done, t.total, t.busy, t.build_hash),
        (PieceKind::Core, 0, y.core.len() as u32, false, y.build_hash())
    );
    let mut host = Host::new(y.clone());
    rig.link_up(3, USB, LinkTrust::Trusted);
    exchange(&mut rig, &mut host, USB, offer_of(&y).encode(), &mut 4);
    rig.reboot().unwrap();
    assert_eq!(manifest(&rig, 5, USB).state, BoardState::OnTrial);
    let out = rig.link_up(5, USB, LinkTrust::Trusted);
    host.on_board(&out[0].bytes);
    assert_eq!(
        host.manifests.last().unwrap().state,
        BoardState::OnTrial,
        "sent as the link came up"
    );
    assert_eq!(
        manifest(&rig, 6, USB).state,
        BoardState::NeedsEngine,
        "proven, waiting for its engine"
    );

    // Engine-crashing.
    let (rig, x) = crashing_rig(access(OpenTo::Edit, true));
    assert_eq!(rig.mode(), Some(SessionMode::CoreOnly));
    let m = manifest(&rig, 0, USB);
    assert_eq!(m.state, BoardState::EngineCrashing);
    assert_eq!(m.engine_len, Some(x.engine.len() as u32));
}

// ---- Read-back -----------------------------------------------------------------------

fn read_back(rig: &mut BoardRig, link: LinkId, len: u32) -> Result<Vec<u8>, Refusal> {
    let mut bytes = Vec::new();
    let mut off = 0;
    while off < len {
        let g = HostMessage::ReadBack(ReadBackRequest {
            kind: PieceKind::Engine,
            off,
            len: CHUNK,
        });
        let out = say(rig, 1, link, &g.encode());
        if let Some(r) = refusal(&out[0]) {
            return Err(r);
        }
        let Ok(BoardMessage::Data(ChunkRef {
            off: at, payload, ..
        })) = BoardMessage::decode(&out[0])
        else {
            panic!("not data");
        };
        assert_eq!(at, off);
        bytes.extend_from_slice(payload);
        off += payload.len() as u32;
    }
    Ok(bytes)
}

#[test]
fn read_back_returns_the_engine_exactly_and_only_with_play() {
    let (x, _) = x_and_y();
    // The running engine.
    let mut rig = rig_with(vec![x.clone()], 0, access(OpenTo::Nobody, true), config());
    rig.link_up(0, USB, LinkTrust::Trusted);
    assert_eq!(
        read_back(&mut rig, USB, x.engine.len() as u32).unwrap(),
        x.engine
    );
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    assert_eq!(
        read_back(&mut rig, RADIO, 1),
        Err(Refusal::Access),
        "no tier"
    );
    // The engine's server granted play on that link.
    let g = HostMessage::ReadBack(ReadBackRequest {
        kind: PieceKind::Engine,
        off: 0,
        len: CHUNK,
    });
    let out = rig.deliver(2, RADIO, Some(Tier::Play), &g.encode());
    assert!(matches!(
        BoardMessage::decode(&out[0].bytes),
        Ok(BoardMessage::Data(_))
    ));

    // An engine-crashing board can be backed up (E10).
    let (mut rig, x) = crashing_rig(access(OpenTo::Play, true));
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    assert_eq!(
        read_back(&mut rig, RADIO, x.engine.len() as u32).unwrap(),
        x.engine
    );

    // No engine: nothing to read.
    let mut rig = engineless_rig(access(OpenTo::Edit, true), vec![x.clone()]);
    rig.link_up(0, USB, LinkTrust::Trusted);
    assert_eq!(read_back(&mut rig, USB, 1), Err(Refusal::Untrusted));
    // v1 reads back the engine only.
    let g = HostMessage::ReadBack(ReadBackRequest {
        kind: PieceKind::Core,
        off: 0,
        len: 16,
    });
    assert_eq!(
        refusal(&say(&mut rig, 1, USB, &g.encode())[0]),
        Some(Refusal::Untrusted)
    );
}

#[test]
fn queued_read_backs_are_answered_in_order() {
    let (x, _) = x_and_y();
    let mut rig = rig_with(vec![x.clone()], 0, access(OpenTo::Edit, true), config());
    let mut got = Vec::new();
    for i in 0..4 {
        let g = HostMessage::ReadBack(ReadBackRequest {
            kind: PieceKind::Engine,
            off: i * CHUNK,
            len: CHUNK,
        });
        got.extend(say(&mut rig, 1, USB, &g.encode()));
    }
    let offs: Vec<u32> = got
        .iter()
        .map(|b| match BoardMessage::decode(b) {
            Ok(BoardMessage::Data(c)) => c.off,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(offs, [0, CHUNK, 2 * CHUNK, 3 * CHUNK]);
}

// ---- The running engine ------------------------------------------------------------------

#[test]
fn the_running_engine_hands_over_pending_record_then_header_then_reset() {
    let (x, y) = x_and_y();
    let mut rig = rig_with(
        vec![x.clone(), y.clone()],
        0,
        access(OpenTo::Nobody, true),
        config(),
    );
    assert_eq!(rig.mode(), Some(SessionMode::EngineRunning));
    rig.link_up(0, RADIO, LinkTrust::Untrusted);
    // No tier from the engine's login, board closed: refused.
    let out = rig.deliver(1, RADIO, None, &offer_of(&y).encode());
    assert_eq!(refusal(&out[0].bytes), Some(Refusal::Access));
    // The engine's server granted edit on that link: accepted.
    let ops = rig.board.flash.ops();
    let out = rig.deliver(2, RADIO, Some(Tier::Edit), &offer_of(&y).encode());
    assert!(out.is_empty());
    assert!(rig.reset_pending);
    assert_eq!(
        rig.board.flash.ops() - ops,
        3,
        "record erase, record program, header erase"
    );
    assert!(!rig.board.engine_valid());
    let at = MODEL_PROGRESS as usize;
    let RecordRead::V1(r, _) = TransferRecord::read(&rig.board.flash.bytes()[at..at + 4096]) else {
        panic!("no record");
    };
    assert_eq!(
        (r.kind, r.stage, r.build),
        (PieceKind::Core, RecordStage::Pending, y.build_hash())
    );
}

#[test]
fn the_running_engine_ignores_its_own_engine_and_chunks_and_answers_unknown_with_u() {
    let (x, _) = x_and_y();
    let mut rig = rig_with(vec![x.clone()], 0, access(OpenTo::Edit, true), config());
    let before = rig.board.flash.bytes().to_vec();
    let out = say(&mut rig, 1, USB, &offer_of(&x).encode());
    assert!(
        matches!(BoardMessage::decode(&out[0]), Ok(BoardMessage::Manifest(_))),
        "a no-op: M"
    );
    let chunk = lpc_update::encode_chunk(
        lpc_update::ChunkEncoding::Raw,
        PieceKind::Engine,
        0,
        &[0; 16],
    );
    assert!(say(&mut rig, 2, USB, &chunk).is_empty());
    assert!(say(&mut rig, 3, USB, &HostLoginStep::Begin.encode()).is_empty());
    assert_eq!(
        refusal(&say(&mut rig, 4, USB, b"W")[0]),
        Some(Refusal::UnknownMessage { ty: b'W' })
    );
    assert!(rig.board.flash.bytes() == before.as_slice());
    assert!(!rig.reset_pending);
}
