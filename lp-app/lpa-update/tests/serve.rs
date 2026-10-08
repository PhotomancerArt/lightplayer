//! Serving rules (A-P5): send-ahead, restarts, the header alone, `Z` only
//! when flagged and indexed, the flag rule, and refusals.

use lpa_update::{
    EncodedPiece, HostBuild, HostIdentity, HostRefusal, ServeConfig, ServeEvent, ServeSession,
};
use lpc_update::code_table::CHUNK;
use lpc_update::{
    BoardMessage, ChunkEncoding, HostMessage, PieceKind, Refusal, Request, encode_chunk,
};

fn identity() -> HostIdentity {
    HostIdentity {
        target: "esp32c6-4mb".into(),
        chip: "esp32c6".into(),
        version: "2026.10.06-1".into(),
        build_id: "2026.10.06-1+abcdefabcdef".into(),
        wire_proto: 36,
        layout: 1,
        min_loader: 1,
    }
}

/// Core: 8 chunks, encoded except chunk 2 (index 0). Engine: 6 chunks, no
/// encoding.
fn build() -> HostBuild {
    let core: Vec<u8> = (0..8 * 4096u32).map(|i| (i / 4096) as u8).collect();
    let engine: Vec<u8> = (0..6 * 4096u32 - 100).map(|i| (i * 3) as u8).collect();
    let chunks: Vec<u32> = (0..8).map(|i| if i == 2 { 0 } else { 10 + i }).collect();
    let stream = chunks
        .iter()
        .enumerate()
        .flat_map(|(i, &n)| vec![0xA0 + i as u8; n as usize])
        .collect();
    let encoded = EncodedPiece { stream, chunks };
    HostBuild::from_parts(identity(), core, engine, Some(encoded), None).unwrap()
}

fn r(kind: PieceKind, idx: u32, len: u32, flags: u8) -> Vec<u8> {
    Request {
        kind,
        off: idx * CHUNK,
        len,
        flags,
    }
    .encode()
}

/// `(type, kind, chunk)` of each message sent.
fn sent(out: &[Vec<u8>]) -> Vec<(u8, PieceKind, u32)> {
    out.iter()
        .map(|m| match HostMessage::decode(m) {
            Ok(HostMessage::Chunk(c)) => (c.encoding.type_byte(), c.kind, c.off / CHUNK),
            other => panic!("{other:?}"),
        })
        .collect()
}

/// One chunk per request (what `ServeConfig::USB` was before it streamed).
const ONE_AHEAD: ServeConfig = ServeConfig::ahead(1);
const C: PieceKind = PieceKind::Core;
const E: PieceKind = PieceKind::Engine;

#[test]
fn ahead_1_sends_exactly_what_is_asked() {
    let b = build();
    let mut s = ServeSession::new(ONE_AHEAD);
    for i in 0..8 {
        let out = s.on_board(&b, &r(C, i, 4096, 0));
        assert_eq!(sent(&out.send), [(b'D', C, i)]);
    }
    assert_eq!(s.counters().chunks_duplicate, 0);
    assert_eq!(s.counters().requests, 8);
}

#[test]
fn ahead_4_keeps_four_in_flight_and_tops_up_one_per_request() {
    let b = build();
    let mut s = ServeSession::new(ServeConfig::BLE);
    assert_eq!(
        sent(&s.on_board(&b, &r(C, 0, 4096, 0)).send),
        [(b'D', C, 0), (b'D', C, 1), (b'D', C, 2), (b'D', C, 3)]
    );
    assert_eq!(
        sent(&s.on_board(&b, &r(C, 1, 4096, 0)).send),
        [(b'D', C, 4)]
    );
    assert_eq!(
        sent(&s.on_board(&b, &r(C, 2, 4096, 0)).send),
        [(b'D', C, 5)]
    );
    // Near the end the window runs out of chunks.
    s.on_board(&b, &r(C, 3, 4096, 0));
    s.on_board(&b, &r(C, 4, 4096, 0));
    assert_eq!(sent(&s.on_board(&b, &r(C, 5, 4096, 0)).send), []);
    assert_eq!(s.counters().chunks_duplicate, 0, "nothing sent twice");
}

#[test]
fn a_request_behind_the_stream_restarts_it_there() {
    let b = build();
    let mut s = ServeSession::new(ServeConfig::BLE);
    s.on_board(&b, &r(C, 0, 4096, 0));
    s.on_board(&b, &r(C, 1, 4096, 0));
    // The board asks for chunk 1 again (it dropped what came after it).
    let out = s.on_board(&b, &r(C, 1, 4096, 0));
    assert_eq!(
        sent(&out.send),
        [(b'D', C, 1), (b'D', C, 2), (b'D', C, 3), (b'D', C, 4)]
    );
    assert_eq!(s.counters().chunks_duplicate, 4, "counted, not hidden");
}

#[test]
fn the_engine_header_goes_alone() {
    let b = build();
    let mut s = ServeSession::new(ServeConfig::BLE);
    assert_eq!(sent(&s.on_board(&b, &r(E, 1, 4096, 0)).send).len(), 4);
    let out = s.on_board(&b, &r(E, 0, 4096, 0));
    assert_eq!(sent(&out.send), [(b'D', E, 0)]);
    // A short tail is served at its length.
    let out = s.on_board(&b, &r(E, 5, 4096 - 100, 0));
    assert_eq!(sent(&out.send), [(b'D', E, 5)]);
}

#[test]
fn z_only_when_flagged_and_indexed() {
    let b = build();
    let mut s = ServeSession::new(ONE_AHEAD);
    assert_eq!(
        sent(&s.on_board(&b, &r(C, 0, 4096, 1)).send),
        [(b'Z', C, 0)]
    );
    assert_eq!(
        sent(&s.on_board(&b, &r(C, 1, 4096, 0)).send),
        [(b'D', C, 1)],
        "raw fallback"
    );
    assert_eq!(
        sent(&s.on_board(&b, &r(C, 2, 4096, 1)).send),
        [(b'D', C, 2)],
        "index 0"
    );
    assert_eq!(
        sent(&s.on_board(&b, &r(E, 1, 4096, 1)).send),
        [(b'D', E, 1)],
        "no encoding"
    );
    let out = s.on_board(&b, &r(C, 3, 4096, 1));
    let Ok(HostMessage::Chunk(c)) = HostMessage::decode(&out.send[0]) else {
        panic!()
    };
    assert_eq!(
        c.payload,
        &[0xA3; 13][..],
        "cut from the stream by the index"
    );
    let k = s.counters();
    assert_eq!((k.chunks_encoded, k.chunks_raw), (2, 3));
    assert_eq!(k.bytes_encoded, 10 + 13);
}

#[test]
fn a_request_with_an_unknown_must_understand_flag_is_not_served() {
    let b = build();
    let mut s = ServeSession::new(ONE_AHEAD);
    let out = s.on_board(&b, &r(C, 0, 4096, 0x21));
    assert!(out.send.is_empty());
    assert_eq!(out.events, [ServeEvent::UnservableRequest { flags: 0x21 }]);
    // An unknown low bit is only a hint: served.
    assert_eq!(s.on_board(&b, &r(C, 0, 4096, 0x06)).send.len(), 1);
    // A request past the piece is not served either.
    let out = s.on_board(&b, &r(C, 9, 4096, 0));
    assert!(out.send.is_empty());
    assert!(matches!(out.events[..], [ServeEvent::RequestOutOfRange(_)]));
}

#[test]
fn refusals_become_typed_events_and_unknown_board_messages_are_ignored() {
    let b = build();
    let mut s = ServeSession::new(ONE_AHEAD);
    for (n, want) in [
        (
            Refusal::UnknownMessage { ty: b'G' },
            HostRefusal::BoardLacksMessage(b'G'),
        ),
        (Refusal::Access, HostRefusal::NeedsLogin),
        (
            Refusal::Busy {
                done: 4096,
                total: 9000,
            },
            HostRefusal::Busy {
                done: 4096,
                total: 9000,
            },
        ),
        (
            Refusal::FailedBuild { build_hash: 7 },
            HostRefusal::FailedBuild(7),
        ),
        (Refusal::HashMismatch, HostRefusal::HashMismatch),
        (Refusal::Untrusted, HostRefusal::Untrusted),
        (Refusal::Other { reason: b'Q' }, HostRefusal::Other(b'Q')),
    ] {
        let out = s.on_board(&b, &n.encode());
        assert_eq!(out.events, [ServeEvent::Refused(want)]);
        assert!(out.send.is_empty());
    }
    let out = s.on_board(&b, b"Kwhatever a newer board says");
    assert_eq!(out, Default::default());
    // Read-back data is not the serving session's.
    let d = encode_chunk(ChunkEncoding::Raw, E, 0, &[1, 2]);
    assert_eq!(s.on_board(&b, &d), Default::default());
    assert!(matches!(
        BoardMessage::decode(&d),
        Ok(BoardMessage::Data(_))
    ));
}

#[test]
fn the_offer_is_the_builds() {
    let b = build();
    let Ok(HostMessage::Offer(o)) = HostMessage::decode(&ServeSession::offer(&b)) else {
        panic!()
    };
    assert_eq!(o, b.offer());
    assert_eq!((o.flags, o.proto), (0, 1));
}
