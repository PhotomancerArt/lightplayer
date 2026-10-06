//! A plain link reads a SYN's 12-byte prefix and ignores the rest and every
//! flag bit but `established` — the link's growth path once it is frozen
//! (OTA plan Part A, P1b; `one-way-doors.md` §6).
//!
//! A future host may add a link feature the way `secure` was added: a SYN
//! flag and an extension after the 12 bytes. A fielded board must still come
//! up for it. These tests play that future host: one end's SYNs are
//! rewritten on the way (extension bytes appended, unknown flag bits set,
//! the checksum recomputed) and the plain end must establish and carry
//! messages both ways. What a plain link *sends* is pinned separately by
//! `plain_bytes_golden.rs`, unchanged.

use lp_link::frame::{self, FrameKind, Header};
use lp_link::{
    CH_PROTO, CH_UPDATE, Framing, Link, LinkConfig, LinkEvent, LinkState, Micros, SelectiveRepeat,
};

#[test]
fn a_plain_link_comes_up_when_its_peer_sends_a_longer_syn() {
    for extra in [1usize, 4, 64] {
        let ext: Vec<u8> = (0..extra)
            .map(|i| (i as u8).wrapping_mul(29) ^ 0xa5)
            .collect();
        let (a, b) = exchange(LinkConfig::ws(), &ext, 0);
        assert_up_and_clean(&a, &b, &format!("{extra} extension bytes"));
    }
}

#[test]
fn a_plain_link_comes_up_when_its_peer_sets_unknown_flag_bits() {
    for bit in 2..8u8 {
        let (a, b) = exchange(LinkConfig::ws(), &[], 1 << bit);
        assert_up_and_clean(&a, &b, &format!("flag bit {bit}"));
    }
    let (a, b) = exchange(LinkConfig::ble(), &[0xee; 8], 0xfc);
    assert_up_and_clean(&a, &b, "every unknown bit and an extension, on ble()");
}

/// A build without `secure` cannot read a secure SYN's extension, so it reads
/// the prefix and answers as plain: the peer that asked for secure decides
/// whether a plain answer will do. (A `secure` build checks the secure SYN
/// first and says `secure_required`; `secure_handshake.rs` pins that.)
#[cfg(not(feature = "secure"))]
#[test]
fn a_plain_only_build_reads_a_secure_flagged_syn_as_plain() {
    let (a, b) = exchange(LinkConfig::ws(), &[0x77; 64], lp_link::frame::SYN_SECURE);
    assert_up_and_clean(&a, &b, "SYN_SECURE with an extension, plain-only build");
}

/// The update channel (3) is what a fielded core is reached over: a future
/// host whose SYN carries an extension must still move update messages.
#[test]
fn the_update_channel_works_when_the_peer_sends_a_longer_syn() {
    for cfg in [LinkConfig::ble(), LinkConfig::udp()] {
        let mut cfg = cfg;
        cfg.reliable_channels |= 1 << CH_UPDATE;
        let (a, b) = exchange_on(cfg, &[0xc3, 0x3c, 0x5a, 0xa5], 0, CH_UPDATE);
        assert_up_and_clean(&a, &b, "a 4-byte SYN extension, channel 3");
        assert_eq!(a.channels, vec![CH_UPDATE]);
        assert_eq!(b.channels, vec![CH_UPDATE]);
    }
}

#[test]
fn the_extension_has_no_effect_on_what_the_receiver_reads() {
    // Same nonces, with and without an extension: the receiving end reaches
    // the same state and sends the same bytes back.
    let plain = exchange(LinkConfig::ws(), &[], 0);
    let extended = exchange(LinkConfig::ws(), &[0x5a; 64], 0);
    assert_eq!(plain.0.sent, extended.0.sent, "A's frames moved");
}

#[test]
fn an_eleven_byte_syn_is_still_not_a_syn() {
    let cfg = LinkConfig::ws();
    let mut a = Link::<SelectiveRepeat>::new(cfg.clone(), 0x1111_1111);
    let hdr = Header {
        kind: FrameKind::Syn,
        fin: false,
        first: false,
        chan: 0,
        seq: 0,
        ack: 0,
        win: 0,
    };
    let mut raw = Vec::new();
    frame::encode_raw(cfg.crc, 0, &hdr, &[0x22; 11], &mut raw);
    let before = a.counters().bad_frames;
    a.on_datagram(0, &raw);
    assert_eq!(a.counters().bad_frames, before + 1);
    assert_ne!(a.state(), LinkState::Established);
}

/// One end of the run: the link, the frames it sent (as hex), and the
/// messages it received.
struct End {
    link: Link<SelectiveRepeat>,
    sent: Vec<String>,
    got: Vec<Vec<u8>>,
    /// The channel each received message came on.
    channels: Vec<u8>,
}

/// Run a plain pair to establishment and one message each way. Every SYN
/// `b` sends reaches `a` with `ext` appended and `flags` OR-ed into byte 8.
fn exchange(cfg: LinkConfig, ext: &[u8], flags: u8) -> (End, End) {
    exchange_on(cfg, ext, flags, CH_PROTO)
}

/// [`exchange`], the two messages on `channel`.
fn exchange_on(cfg: LinkConfig, ext: &[u8], flags: u8, channel: u8) -> (End, End) {
    assert_eq!(
        cfg.framing,
        Framing::Datagram,
        "this harness speaks datagrams"
    );
    let mut a = End {
        link: Link::new(cfg.clone(), 0x1111_1111),
        sent: Vec::new(),
        got: Vec::new(),
        channels: Vec::new(),
    };
    let mut b = End {
        link: Link::new(cfg.clone(), 0x2222_2222),
        sent: Vec::new(),
        got: Vec::new(),
        channels: Vec::new(),
    };
    let mut now: Micros = 0;
    let mut sent_messages = false;
    for _ in 0..200 {
        for _ in 0..16 {
            let mut moved = false;
            if let Some(f) = a.link.poll_transmit(now) {
                let f = f.to_vec();
                a.sent.push(hex(&f));
                b.link.on_datagram(now, &f);
                moved = true;
            }
            if let Some(f) = b.link.poll_transmit(now) {
                let f = rewrite_syn(&cfg, f, ext, flags);
                b.sent.push(hex(&f));
                a.link.on_datagram(now, &f);
                moved = true;
            }
            if !moved {
                break;
            }
        }
        for end in [&mut a, &mut b] {
            while let Some(ev) = end.link.recv() {
                if let LinkEvent::Message { channel, data } = ev {
                    end.got.push(data);
                    end.channels.push(channel);
                }
            }
        }
        if !sent_messages
            && a.link.state() == LinkState::Established
            && b.link.state() == LinkState::Established
        {
            a.link.send(channel, b"from a").unwrap();
            b.link.send(channel, b"from b").unwrap();
            sent_messages = true;
        }
        if !a.got.is_empty() && !b.got.is_empty() {
            break;
        }
        now += 1_000;
    }
    (a, b)
}

fn assert_up_and_clean(a: &End, b: &End, what: &str) {
    assert_eq!(
        a.link.state(),
        LinkState::Established,
        "{what}: A is not up"
    );
    assert_eq!(
        b.link.state(),
        LinkState::Established,
        "{what}: B is not up"
    );
    assert_eq!(a.got, vec![b"from b".to_vec()], "{what}: A's message");
    assert_eq!(b.got, vec![b"from a".to_vec()], "{what}: B's message");
    assert_eq!(a.link.counters().bad_frames, 0, "{what}: A refused a frame");
}

/// A SYN with `ext` after its 12 bytes and `flags` set in byte 8, checksum
/// recomputed; any other frame untouched.
fn rewrite_syn(cfg: &LinkConfig, raw: &[u8], ext: &[u8], flags: u8) -> Vec<u8> {
    let hdr = Header::parse(raw).expect("the link sent a frame");
    if hdr.kind != FrameKind::Syn {
        return raw.to_vec();
    }
    let mut body = frame::verify(cfg.crc, 0, raw)
        .expect("a SYN verifies under key 0")
        .to_vec();
    assert_eq!(body.len(), frame::SYN_LEN, "a plain link sends 12 bytes");
    body[8] |= flags;
    body.extend_from_slice(ext);
    let mut out = Vec::new();
    frame::encode_raw(cfg.crc, 0, &hdr, &body, &mut out);
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
