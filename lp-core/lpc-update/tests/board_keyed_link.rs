//! A secure link in core-only (OTA Wi-Fi plan WD1, WD6): the core answers
//! its key lookup from the store's secrets, the key that verifies decides
//! its tier, a wrong guess shares the `L` login's backoff, and a keyed link
//! never logs in over channel 3.

mod support;

use lpc_access::{LoginMac, OpenTo, SecretEntry, Tier, derive_login_key, link_psk};
use lpc_update::board::{AccessFacts, CoreKeyAnswer, LinkId, LinkTrust, SessionConfig};
use lpc_update::code_table::CHUNK;
use lpc_update::testing::{BoardRig, FakeBoard, MODEL_REGION_START, ModelBuild};
use lpc_update::{
    BoardLoginStep, BoardMessage, HostLoginStep, HostMessage, PieceKind, ReadBackRequest, Refusal,
    tier_from_code,
};

use support::{Host, RADIO, RADIO_2, REGION, exchange, offer_of, x_and_y};

const PASSWORD: &[u8] = b"correct horse";
const ITERATIONS: u32 = 64;
const PLAY_SALT: [u8; 16] = [1; 16];
const EDIT_SALT: [u8; 16] = [2; 16];

#[test]
fn a_known_salt_is_answered_with_its_candidates_and_the_match_decides_the_tier() {
    let (x, y) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody), vec![x, y.clone()]);
    let edit = secret(Tier::Edit, 2);
    let s = session(&mut rig);
    assert_eq!(
        s.key_lookup(1, RADIO, &EDIT_SALT),
        CoreKeyAnswer::Keys(vec![link_psk(&edit.k)])
    );
    assert_eq!(s.key_authenticated(RADIO, 0), LinkTrust::Keyed(Tier::Edit));

    let play = secret(Tier::Play, 1);
    assert_eq!(
        s.key_lookup(2, RADIO_2, &PLAY_SALT),
        CoreKeyAnswer::Keys(vec![link_psk(&play.k)])
    );
    assert_eq!(
        s.key_authenticated(RADIO_2, 0),
        LinkTrust::Keyed(Tier::Play)
    );

    // A candidate it never offered, or a link it never looked up, is no
    // grant.
    assert_eq!(
        s.key_authenticated(LinkId(9), 0),
        LinkTrust::Untrusted,
        "no lookup"
    );
}

#[test]
fn the_anonymous_key_gets_the_zero_psk_and_grants_nothing() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Edit), vec![x]);
    let s = session(&mut rig);
    assert_eq!(
        s.key_lookup(1, RADIO, &[0; 16]),
        CoreKeyAnswer::Keys(vec![[0; 32]])
    );
    assert_eq!(s.key_authenticated(RADIO, 0), LinkTrust::Untrusted);
}

#[test]
fn an_unknown_salt_is_refused_and_not_charged() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody), vec![x]);
    let s = session(&mut rig);
    for now in 0..10 {
        assert_eq!(s.key_lookup(now, RADIO, &[9; 16]), CoreKeyAnswer::Unknown);
    }
    assert!(matches!(
        s.key_lookup(11, RADIO, &EDIT_SALT),
        CoreKeyAnswer::Keys(_)
    ));
}

/// Three free guesses, then the backoff refuses every lookup without
/// reading anything — and it is the `L` login's backoff: one board, one.
#[test]
fn wrong_keys_put_the_board_in_the_logins_backoff() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody), vec![x]);
    {
        let s = session(&mut rig);
        for now in 0..3 {
            assert!(matches!(
                s.key_lookup(now, RADIO, &EDIT_SALT),
                CoreKeyAnswer::Keys(_)
            ));
            s.key_wrong(now, RADIO);
        }
        assert_eq!(
            s.key_lookup(10, RADIO, &EDIT_SALT),
            CoreKeyAnswer::Keys(vec![link_psk(&secret(Tier::Edit, 2).k)]),
            "three are free"
        );
        s.key_wrong(10, RADIO);
        let CoreKeyAnswer::Backoff { retry_after_ms } = s.key_lookup(11, RADIO, &EDIT_SALT) else {
            panic!("in backoff");
        };
        assert!(retry_after_ms > 0);
        // Even an unknown or anonymous key: nothing is read in backoff.
        assert!(matches!(
            s.key_lookup(12, RADIO, &[0; 16]),
            CoreKeyAnswer::Backoff { .. }
        ));
    }
    // The core's `L` login on another link waits out the same backoff.
    rig.link_up(13, RADIO_2, LinkTrust::Untrusted);
    let out = say(&mut rig, 14, RADIO_2, &HostLoginStep::Begin.encode());
    let (tier, wait) = verdict(&out[0]);
    assert_eq!(tier, None);
    assert!(wait > 0, "the login is in the key's backoff");

    // Past it, a right key clears the count, as a login does.
    let s = session(&mut rig);
    assert!(matches!(
        s.key_lookup(60_000, RADIO, &EDIT_SALT),
        CoreKeyAnswer::Keys(_)
    ));
    assert_eq!(s.key_authenticated(RADIO, 0), LinkTrust::Keyed(Tier::Edit));
    s.key_wrong(60_001, RADIO_2);
    assert!(
        matches!(
            s.key_lookup(60_002, RADIO_2, &EDIT_SALT),
            CoreKeyAnswer::Keys(_)
        ),
        "the count started again"
    );
}

/// What each key may do over a keyed link: play queries and reads back,
/// edit installs a core; anyone heals the board's own engine (Y8).
#[test]
fn a_play_key_queries_and_reads_back_and_an_edit_key_installs_a_core() {
    let (x, y) = x_and_y();
    // A running engine: the read-back.
    let mut rig = running_rig(access(OpenTo::Nobody), vec![x.clone()]);
    keyed_up(&mut rig, RADIO, &PLAY_SALT);
    let g = read_back_request();
    let out = rig.deliver(2, RADIO, None, &g);
    assert!(
        matches!(
            BoardMessage::decode(&out[0].bytes),
            Ok(BoardMessage::Data(_))
        ),
        "play reads back"
    );

    // Core-only: play may query, not install a core.
    let mut rig = engineless_rig(access(OpenTo::Nobody), vec![x.clone(), y.clone()]);
    let up = keyed_up(&mut rig, RADIO, &PLAY_SALT);
    assert!(
        matches!(
            BoardMessage::decode(&up[0].bytes),
            Ok(BoardMessage::Manifest(_))
        ),
        "M on up"
    );
    let out = say(
        &mut rig,
        3,
        RADIO,
        &HostMessage::Query { proto: 1 }.encode(),
    );
    assert!(matches!(
        BoardMessage::decode(&out[0]),
        Ok(BoardMessage::Manifest(_))
    ));
    assert_eq!(
        refusal(&say(&mut rig, 4, RADIO, &offer_of(&y).encode())[0]),
        Some(Refusal::Access)
    );
    // Edit installs it, start to finish.
    keyed_up(&mut rig, RADIO_2, &EDIT_SALT);
    let mut host = Host::new(y.clone());
    let mut now = 10;
    exchange(
        &mut rig,
        &mut host,
        RADIO_2,
        offer_of(&y).encode(),
        &mut now,
    );
    assert!(host.refusals.is_empty(), "{:?}", host.refusals);
    assert!(rig.reset_pending, "the core committed");

    // A board open to nobody refuses the anonymous key's core install, and
    // still heals its own engine for it.
    let mut rig = engineless_rig(access(OpenTo::Nobody), vec![x.clone(), y.clone()]);
    keyed_up(&mut rig, RADIO, &[0; 16]);
    assert_eq!(
        refusal(&say(&mut rig, 3, RADIO, &offer_of(&y).encode())[0]),
        Some(Refusal::Access)
    );
    let mut host = Host::new(x.clone());
    exchange(&mut rig, &mut host, RADIO, offer_of(&x).encode(), &mut now);
    assert!(host.refusals.is_empty());
    assert!(rig.reset_pending, "the heal committed");

    // A board open at edit takes a core on the anonymous key (QY2 = yes).
    let mut rig = engineless_rig(access(OpenTo::Edit), vec![x, y.clone()]);
    keyed_up(&mut rig, RADIO, &[0; 16]);
    assert!(refusal(&say(&mut rig, 3, RADIO, &offer_of(&y).encode())[0]).is_none());
}

/// WD6: a keyed link's key is its login. `L` on one is refused with the
/// verdict any login the session will not take gets — and the same link
/// untrusted logs in as before.
#[test]
fn a_keyed_link_is_refused_the_cores_login_and_an_untrusted_one_is_not() {
    let (x, _) = x_and_y();
    let mut rig = engineless_rig(access(OpenTo::Nobody), vec![x]);
    keyed_up(&mut rig, RADIO, &PLAY_SALT);
    let out = say(&mut rig, 2, RADIO, &HostLoginStep::Begin.encode());
    assert_eq!(verdict(&out[0]), (None, 0), "no challenge on a keyed link");
    let out = say(
        &mut rig,
        3,
        RADIO,
        &HostLoginStep::Answer {
            macs: vec![[0; 32]],
        }
        .encode(),
    );
    assert_eq!(verdict(&out[0]), (None, 0));
    // It did not reach the backoff, and an untrusted link logs in.
    rig.link_up(4, RADIO_2, LinkTrust::Untrusted);
    assert_eq!(login(&mut rig, 5, RADIO_2), Some(Tier::Edit));
}

// ---- helpers ----

fn nonce(buf: &mut [u8]) {
    buf.fill(0x5a);
}

fn secret(tier: Tier, salt: u8) -> SecretEntry {
    SecretEntry::from_password("mine", tier, PASSWORD, [salt; 16], ITERATIONS)
}

fn access(open: OpenTo) -> AccessFacts {
    AccessFacts {
        secrets: vec![secret(Tier::Play, 1), secret(Tier::Edit, 2)],
        open,
        core_install_follows_open_to: true,
    }
}

fn config() -> SessionConfig {
    SessionConfig {
        entropy: Some(nonce),
        ..SessionConfig::default()
    }
}

/// A board holding X with its engine header erased (core-only).
fn engineless_rig(access: AccessFacts, catalog: Vec<ModelBuild>) -> BoardRig {
    let x = catalog[0].clone();
    let mut board = FakeBoard::flashed_with(catalog, 0, REGION);
    let engine = (MODEL_REGION_START + x.core.len() as u32).div_ceil(CHUNK) * CHUNK;
    board.flash.flash_image(engine, &[0xFF; CHUNK as usize]);
    BoardRig::new(board, access, config()).unwrap()
}

fn running_rig(access: AccessFacts, catalog: Vec<ModelBuild>) -> BoardRig {
    let board = FakeBoard::flashed_with(catalog, 0, REGION);
    BoardRig::new(board, access, config()).unwrap()
}

fn session(rig: &mut BoardRig) -> &mut lpc_update::board::BoardSession {
    rig.session.as_mut().unwrap()
}

/// Look `salt` up on `link`, take its first candidate as the match (what
/// the handshake would do), and bring the link up with the trust that
/// gives; what the board said.
fn keyed_up(rig: &mut BoardRig, link: LinkId, salt: &[u8; 16]) -> Vec<lpc_update::board::Outgoing> {
    let s = session(rig);
    assert!(matches!(
        s.key_lookup(1, link, salt),
        CoreKeyAnswer::Keys(_)
    ));
    let trust = s.key_authenticated(link, 0);
    rig.link_up(1, link, trust)
}

fn read_back_request() -> Vec<u8> {
    HostMessage::ReadBack(ReadBackRequest {
        kind: PieceKind::Engine,
        off: 0,
        len: CHUNK,
    })
    .encode()
}

fn say(rig: &mut BoardRig, now: u64, link: LinkId, bytes: &[u8]) -> Vec<Vec<u8>> {
    rig.deliver(now, link, None, bytes)
        .into_iter()
        .map(|o| o.bytes)
        .collect()
}

fn refusal(bytes: &[u8]) -> Option<Refusal> {
    match BoardMessage::decode(bytes) {
        Ok(BoardMessage::Refusal(r)) => Some(r),
        _ => None,
    }
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

/// Log in on `link` with the edit password.
fn login(rig: &mut BoardRig, now: u64, link: LinkId) -> Option<Tier> {
    let out = say(rig, now, link, &HostLoginStep::Begin.encode());
    let Ok(BoardMessage::Login(BoardLoginStep::Challenge { nonce, offers })) =
        BoardMessage::decode(&out[0])
    else {
        panic!("no challenge: {:?}", BoardMessage::decode(&out[0]));
    };
    let macs = offers
        .iter()
        .map(|o| LoginMac::compute(&derive_login_key(PASSWORD, &o.salt, o.iterations), &nonce).0)
        .collect();
    let out = say(rig, now, link, &HostLoginStep::Answer { macs }.encode());
    verdict(&out[0]).0
}
