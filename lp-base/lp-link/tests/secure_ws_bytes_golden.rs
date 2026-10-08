//! The secure LAN link's bytes, pinned. A C6 on Wi-Fi is reached over
//! `ws://<board>/link` with `LinkConfig::ws()` and lp-link's secure channel
//! (Noise NNpsk0 inside the SYN, then every frame sealed). Once a core that
//! updates over Wi-Fi is fielded, a board in a house can be reached **only**
//! this way, so — like `plain_bytes_golden.rs` and `update_channel_golden.rs`
//! — this is a **never break** pin: a mismatch is a wire change that strands
//! fielded boards, never a golden to re-capture (OTA-over-Wi-Fi plan,
//! `lp2025/2026-10-06-2249-ota-wifi-updates`, W1 and P2).
//!
//! Each transcript is a fixed exchange between a host and a board on one
//! secure LAN link, with fixed nonces and seeded ephemerals: the SYN each
//! way carrying msg1 and msg2 of the NNpsk0 handshake, the sealed
//! key-confirmation ACK, one sealed channel-1 message each way (the wire's
//! JSON), one sealed channel-3 message each way (`Q`, and an `M`-shaped
//! answer), their ACKs, and keepalives, every frame in the order it went, as
//! hex. A is the host, B the board; the framing is Datagram (one WebSocket
//! binary message per frame), so a line is exactly one message.
//!
//! - **host**: `LinkConfig::ws()`, unchanged, as lp-cli's `lan:` and
//!   Studio's `?lan=` build it (`WireLinkPort::new_secure`);
//! - **board**: `ws()` cut to what the C6 holds. The cut lives in
//!   `lp-fw/fw-esp32-common/src/radio_link/lan_link_config.rs`
//!   (`lan_link_config()`); lp-link cannot depend on that crate, so
//!   [`board_config`] below builds an equal config, and
//!   `fw-esp32-common`'s `tests/lan_endpoint_contract.rs`
//!   (`the_boards_link_config_is_the_one_the_golden_pins`) asserts the board
//!   builds that config. The fields that reach the wire
//!   are the windows (2), `ack_every` (2) and the preset's payload and timers;
//!   the budgets (`max_message`, `rx_budget`, `send_queue`, ...) are local
//!   buffer sizes that put no byte on the wire, and are left as the preset's
//!   here.
//!
//! Two keys: a **keyed** link (a device's access entry) and the **anonymous**
//! key (an open board, the zero key id and PSK). A board with no entry for
//! the key never answers, so both are looked up and found.
//!
//! Entropy is this file's own SplitMix64 (not `lp_link::sim`'s), so the pin
//! does not move if the simulator's generator does; if a future link draws
//! entropy for something new, or in another order, these bytes move, and
//! that is a finding about the wire, not about the golden.

use std::cell::Cell;

use lp_link::secure_channel::{KeyId, Psk, SecureEvent, SecureRole};
use lp_link::{CH_PROTO, CH_UPDATE, Framing, Link, LinkConfig, LinkState, Micros, SelectiveRepeat};

type SecureLink = Link<SelectiveRepeat>;

const HOST_NONCE: u32 = 0x1111_1111;
const BOARD_NONCE: u32 = 0x2222_2222;
const ENTROPY_SEED: u64 = 0x4c41_4e5f_5345_4355;

#[test]
fn keyed_lan_link_bytes_are_unchanged() {
    check(&transcript(KeyId([0x4b; 16]), [0x91; 32]), KEYED);
}

#[test]
fn anonymous_lan_link_bytes_are_unchanged() {
    check(&transcript(KeyId::ANONYMOUS, [0; 32]), ANONYMOUS);
}

/// The pin is only a pin if two runs of one exchange agree byte for byte.
#[test]
fn the_exchange_is_deterministic() {
    let key_id = KeyId([0x4b; 16]);
    let a = transcript(key_id, [0x91; 32]);
    let b = transcript(key_id, [0x91; 32]);
    assert_eq!(a, b, "the same seeds must give the same bytes");
}

#[test]
fn the_board_config_is_the_lan_cut_of_ws() {
    let cfg = board_config();
    assert_eq!(cfg.framing, Framing::Datagram, "one message is one frame");
    assert_eq!(cfg.max_payload, 1024);
    assert_eq!((cfg.tx_window, cfg.rx_window, cfg.ack_every), (2, 2, 2));
    assert!(
        cfg.is_reliable(CH_UPDATE),
        "channel 3 is reliable on the LAN"
    );
    assert_eq!(cfg.validate(), Ok(()));
}

/// `lan_link_config()`'s wire-relevant cut of `ws()`, see the file header.
fn board_config() -> LinkConfig {
    const LAN_WINDOW: u8 = 2;
    let mut cfg = LinkConfig::ws();
    cfg.tx_window = LAN_WINDOW;
    cfg.rx_window = LAN_WINDOW;
    cfg.ack_every = cfg.ack_every.min(LAN_WINDOW);
    cfg
}

const KEYED: &[&str] = &[
    "A 030000001111111100000000060004104b4b4b4b4b4b4b4b4b4b4b4b4b4b4b4bf46d7fa8be979ce5b0ea2f60483d21ac366da685cc10b50fd3121cf526439410c04f800dfb1df7bd3daf95a047e5509cff19a77a",
    "B 0300000022222222111111110a000402873e46822252c6838cc236577ede273e74f7b4b546e1d51cee173cd009f6e66e5142fd48ddb98e8d1f3248bc51bf8143ff7bf1d81ca59187",
    "A 0200001000000000578918a6ccd754ea31f1ee2f4870d9b35a1da28f",
    "A 38000010010000001dcbc3df91a36c3df644da0dfff1cda1093fc040e137c5c93ca9264f4fcf1df4ff1b3e14327821c02bdfcf0ebaf21614484d5ddd",
    "B 3800010200000000dfcf69e9f65908bd4fc118b3a9d59e5159f0feeb1337c77cecb86c6e4533b0e899e6636d9ec7ac9e0a9080a2a322",
    "A 78010110020000009f964b49746bce1c61729a0f8c3d6dd8d462d91a5f1a",
    "B 7801020201000000c5e56d8adaced15986e9ad0d56b8a5f8dd00f969e0dc49903d61e47600daa412a129f6a3c9f8b95171e56f961b106fb384fc",
    "A 0200021003000000c334d12e3410a4753bc17f93e5393a49c334973e",
    "A 02000210040000003e53e58bb3b4df22646cf61064daf202df17d720",
    "B 02000202020000002d5f230bcd8dbd274ceb518237b04961da9fddde",
];
const ANONYMOUS: &[&str] = &[
    "A 0300000011111111000000000600041000000000000000000000000000000000f46d7fa8be979ce5b0ea2f60483d21ac366da685cc10b50fd3121cf526439410c6e181925d31df0292ff12a6f5985b4d37d99bab",
    "B 0300000022222222111111110a000402873e46822252c6838cc236577ede273e74f7b4b546e1d51cee173cd009f6e66e7ea0c268011593c135c2ab3e7c0c016e262d8595869be3a5",
    "A 0200001000000000497e61838dc257b2c11b766109871620286e26c0",
    "A 38000010010000006e5d13e11204dddfd5ad12a97cd0f612cc4d76bd51a0128aed8adbdb53a65e12f19a9af3474dd07c5f715df5210c8093e6adc238",
    "B 3800010200000000e1c1193bcb90b6e7f02b2b560b86ec2c85addf4984e2ffbba73f2b06e6a06afef338842ab4299b33b170e5917083",
    "A 7801011002000000ad62e98657e5b52a2ca41fb8519275638060af7119c6",
    "B 7801020201000000ed498755ec09aae380dfe3b4ce67a404508aad5b4b677c2c5f267862097862c548f746eceb1a97a843c7432021be49115ab2",
    "A 020002100300000049a63896f729d22fc4f5f634b92240baed4df165",
    "A 02000210040000009ba3398034532ac2d4d9cab665687bd9ca1e69ea",
    "B 020002020200000079fec19945f0b22d73a043cc01684a70256b7e48",
];

fn check(got: &[String], want: &[&str]) {
    if got != want {
        let mut printed = String::new();
        for line in got {
            printed.push_str(&format!("    \"{line}\",\n"));
        }
        panic!("the secure LAN link's bytes moved; this run produced:\n{printed}");
    }
}

/// The exchange, each frame as `"<A|B> <hex>"`. A is the host, B the board.
fn transcript(key_id: KeyId, psk: [u8; 32]) -> Vec<String> {
    seed_entropy(ENTROPY_SEED);
    let mut a = SecureLink::new_secure(
        LinkConfig::ws(),
        HOST_NONCE,
        SecureRole::Initiator {
            key_id,
            psk: Psk::new(psk),
        },
        entropy,
    );
    let mut b = SecureLink::new_secure(board_config(), BOARD_NONCE, SecureRole::Responder, entropy);
    let table = [(key_id, [Psk::new(psk)])];
    let mut out = Vec::new();
    let mut now: Micros = 0;

    let step = |now: Micros, a: &mut SecureLink, b: &mut SecureLink, out: &mut Vec<String>| {
        for _ in 0..64 {
            answer_lookups(b, &table);
            let mut moved = false;
            if let Some(f) = a.poll_transmit(now) {
                let f = f.to_vec();
                out.push(format!("A {}", hex(&f)));
                b.on_datagram(now, &f);
                moved = true;
            }
            answer_lookups(b, &table);
            if let Some(f) = b.poll_transmit(now) {
                let f = f.to_vec();
                out.push(format!("B {}", hex(&f)));
                a.on_datagram(now, &f);
                moved = true;
            }
            if !moved {
                break;
            }
        }
        while a.recv().is_some() {}
        while b.recv().is_some() {}
    };

    // The handshake: SYN each way, msg1, msg2, the sealed confirmation.
    step(now, &mut a, &mut b, &mut out);
    assert_eq!(a.state(), LinkState::Established, "host up");
    assert_eq!(b.state(), LinkState::Established, "board up");
    assert_eq!(
        b.session_auth().map(|auth| auth.key_id),
        Some(key_id),
        "the board authenticated the key"
    );

    // Channel 1: the wire's JSON, one message each way.
    a.send(CH_PROTO, b"{\"id\":1,\"msg\":{\"getProject\":{}}}")
        .unwrap();
    b.send(CH_PROTO, b"{\"id\":1,\"msg\":{\"ok\":true}}")
        .unwrap();
    step(now, &mut a, &mut b, &mut out);

    // Channel 3: `Q proto=1`, and an answer shaped like `M` (a short JSON).
    a.send(CH_UPDATE, b"Q\x01").unwrap();
    b.send(CH_UPDATE, b"M{\"proto\":1,\"state\":\"running\"}")
        .unwrap();
    step(now, &mut a, &mut b, &mut out);

    // ACKs and keepalives.
    for dt in [1_000, 5_000, 300_000, 1_100_000] {
        now += dt;
        step(now, &mut a, &mut b, &mut out);
    }
    out
}

/// The board's edge: every key lookup the link raises is answered from the
/// table, now (a harness's `RadioLinkSlot` does the same from its access
/// entries).
fn answer_lookups(board: &mut SecureLink, table: &[(KeyId, [Psk; 1])]) {
    while let Some(event) = board.poll_secure_event() {
        if let SecureEvent::KeyLookup { key_id } = event {
            let (_, psks) = table
                .iter()
                .find(|(known, _)| *known == key_id)
                .expect("the board knows the key");
            board.provide_keys(key_id, psks);
        }
    }
}

thread_local! {
    static ENTROPY: Cell<u64> = const { Cell::new(0) };
}

fn seed_entropy(seed: u64) {
    ENTROPY.with(|s| s.set(seed));
}

/// SplitMix64, this file's own: the secure link's `fn(&mut [u8])` entropy.
fn entropy(buf: &mut [u8]) {
    ENTROPY.with(|state| {
        for chunk in buf.chunks_mut(8) {
            let mut z = state.get().wrapping_add(0x9e37_79b9_7f4a_7c15);
            state.set(z);
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            chunk.copy_from_slice(&z.to_le_bytes()[..chunk.len()]);
        }
    });
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
