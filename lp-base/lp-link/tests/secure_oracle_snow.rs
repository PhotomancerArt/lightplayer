//! Our NNpsk0 against `snow`, an independent Noise implementation, in both
//! roles: the same messages are accepted, both ends split into the same
//! keys, and the handshake hashes agree. snow is a dev-dependency oracle
//! only; nothing of it is linked into the product.
//!
//! Inputs (PSK, key id, nonces, ephemerals, msg2 payload) come from a seeded
//! generator so a failure reproduces.

use lp_link::secure_channel::{
    HandshakeError, Initiator, KeyId, MSG1_LEN, MSG2_LEN, MSG2_PAYLOAD_LEN, Psk, Responder,
    prologue,
};

const PARAMS: &str = "Noise_NNpsk0_25519_ChaChaPoly_SHA256";
const CASES: u64 = 64;

#[test]
fn our_initiator_against_snow_responder() {
    for seed in 0..CASES {
        let mut g = Gen(seed * 2 + 1);
        let (key_id, psk, nonce) = (KeyId(g.bytes()), g.bytes::<32>(), g.u32());
        let p = prologue(&key_id, nonce);
        let (e_i, e_r, payload) = (g.bytes::<32>(), g.bytes::<32>(), g.bytes::<4>());

        let ours = Initiator::new(&p, &Psk::new(psk), e_i);
        let mut snow = snow::Builder::new(PARAMS.parse().unwrap())
            .psk(0, &psk)
            .unwrap()
            .prologue(&p)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&e_r)
            .build_responder()
            .unwrap();

        let mut buf = [0u8; 128];
        let n = snow.read_message(ours.msg1(), &mut buf).unwrap();
        assert_eq!(n, 0, "msg1 carries no payload");
        let mut msg2 = [0u8; 128];
        let m = snow.write_message(&payload, &mut msg2).unwrap();
        assert_eq!(m, MSG2_LEN);

        let mut got = [0u8; MSG2_PAYLOAD_LEN];
        let keys = ours.read_msg2(&msg2[..m], &mut got).unwrap();
        assert_eq!(got, payload);
        let hash = snow.get_handshake_hash().to_vec();
        let (k1, k2) = snow.dangerously_get_raw_split();
        assert_eq!(keys.send, k1, "seed {seed}: initiator → responder key");
        assert_eq!(keys.recv, k2, "seed {seed}: responder → initiator key");
        assert_eq!(keys.handshake_hash.to_vec(), hash, "seed {seed}");
    }
}

#[test]
fn snow_initiator_against_our_responder() {
    for seed in 0..CASES {
        let mut g = Gen(seed * 2 + 2);
        let (key_id, psk, nonce) = (KeyId(g.bytes()), g.bytes::<32>(), g.u32());
        let p = prologue(&key_id, nonce);
        let (e_i, e_r, payload) = (g.bytes::<32>(), g.bytes::<32>(), g.bytes::<4>());

        let mut snow = snow::Builder::new(PARAMS.parse().unwrap())
            .psk(0, &psk)
            .unwrap()
            .prologue(&p)
            .unwrap()
            .fixed_ephemeral_key_for_testing_only(&e_i)
            .build_initiator()
            .unwrap();
        let mut msg1 = [0u8; 128];
        let n = snow.write_message(&[], &mut msg1).unwrap();
        assert_eq!(n, MSG1_LEN);

        let ready = Responder::new(&p)
            .read_msg1(&msg1[..n], &Psk::new(psk))
            .unwrap();
        let mut msg2 = [0u8; MSG2_LEN];
        let keys = ready.write_msg2(e_r, &payload, &mut msg2).unwrap();

        let mut buf = [0u8; 128];
        let got = snow.read_message(&msg2, &mut buf).unwrap();
        assert_eq!(&buf[..got], &payload);
        let hash = snow.get_handshake_hash().to_vec();
        let (k1, k2) = snow.dangerously_get_raw_split();
        assert_eq!(keys.recv, k1, "seed {seed}: initiator → responder key");
        assert_eq!(keys.send, k2, "seed {seed}: responder → initiator key");
        assert_eq!(keys.handshake_hash.to_vec(), hash, "seed {seed}");
    }
}

/// The anonymous key (zero key id, zero PSK) is ordinary NNpsk0 too.
#[test]
fn the_anonymous_key_interoperates() {
    let p = prologue(&KeyId::ANONYMOUS, 77);
    let ours = Initiator::new(&p, &Psk::ANONYMOUS, [5; 32]);
    let mut snow = snow::Builder::new(PARAMS.parse().unwrap())
        .psk(0, &[0; 32])
        .unwrap()
        .prologue(&p)
        .unwrap()
        .build_responder()
        .unwrap();
    let mut buf = [0u8; 128];
    snow.read_message(ours.msg1(), &mut buf).unwrap();
    let mut msg2 = [0u8; 128];
    let m = snow.write_message(&[1, 2, 3, 4], &mut msg2).unwrap();
    let mut got = [0u8; 4];
    let keys = ours.read_msg2(&msg2[..m], &mut got).unwrap();
    assert_eq!(keys.send, snow.dangerously_get_raw_split().0);
}

/// A msg1 written by snow under one PSK fails our responder under another,
/// and a msg2 of ours with one byte changed fails snow (and the reverse).
#[test]
fn a_wrong_psk_or_a_tampered_message_fails_both_ways() {
    let p = prologue(&KeyId([1; 16]), 5);
    let mut snow_i = snow::Builder::new(PARAMS.parse().unwrap())
        .psk(0, &[1; 32])
        .unwrap()
        .prologue(&p)
        .unwrap()
        .build_initiator()
        .unwrap();
    let mut msg1 = [0u8; 128];
    let n = snow_i.write_message(&[], &mut msg1).unwrap();
    assert_eq!(
        Responder::new(&p)
            .read_msg1(&msg1[..n], &Psk::new([2; 32]))
            .err(),
        Some(HandshakeError::BadTag)
    );

    let ready = Responder::new(&p)
        .read_msg1(&msg1[..n], &Psk::new([1; 32]))
        .unwrap();
    let mut msg2 = [0u8; MSG2_LEN];
    ready.write_msg2([9; 32], &[0; 4], &mut msg2).unwrap();
    msg2[40] ^= 1;
    let mut buf = [0u8; 128];
    assert!(snow_i.read_message(&msg2, &mut buf).is_err());

    let ours = Initiator::new(&p, &Psk::new([1; 32]), [3; 32]);
    let mut snow_r = snow::Builder::new(PARAMS.parse().unwrap())
        .psk(0, &[1; 32])
        .unwrap()
        .prologue(&p)
        .unwrap()
        .build_responder()
        .unwrap();
    snow_r.read_message(ours.msg1(), &mut buf).unwrap();
    let mut msg2 = [0u8; 128];
    let m = snow_r.write_message(&[0; 4], &mut msg2).unwrap();
    msg2[0] ^= 0x80;
    assert_eq!(
        ours.read_msg2(&msg2[..m], &mut [0; 4]).err(),
        Some(HandshakeError::BadTag)
    );
}

/// SplitMix64: reproducible inputs, no RNG dependency.
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn u32(&mut self) -> u32 {
        self.next() as u32
    }

    fn bytes<const N: usize>(&mut self) -> [u8; N] {
        core::array::from_fn(|_| self.next() as u8)
    }
}
