//! Karn's rule for the retransmission backoff: an ACK that only covers frames
//! that were resent says nothing about the path's round trip, so it must not
//! clear the backoff (the RTT sample is already skipped for such frames). A
//! peer that acks late would otherwise make every new request start again at
//! the un-backed-off timeout, time out and resend its whole flight.

use lp_link::{CH_PROTO, Link, LinkConfig, LinkState, SelectiveRepeat};

type L = Link<SelectiveRepeat>;

#[test]
fn an_ack_of_resent_frames_keeps_the_backoff() {
    let (mut host, mut board) = established();
    let mut now = 1_000_000;
    let base = host.rto();

    // The request reaches the board, but its answer is slow.
    host.send(CH_PROTO, &[1; 40]).unwrap();
    let first = host.poll_transmit(now).expect("first send").to_vec();
    board.on_bytes(now, &first);

    // The host's timeout fires and it resends; the resend is lost.
    now += base + 1_000;
    let resend = host.poll_transmit(now).expect("resend after the timeout");
    assert!(!resend.is_empty());
    assert_eq!(host.counters().timeouts, 1);
    assert_eq!(host.rto(), base * 2, "one timeout doubles the timeout");

    // The late ACK of the first send arrives. Every frame it covers was resent,
    // so there is no round-trip sample, and the backoff must stand.
    now += 2_000;
    let ack = board.poll_transmit(now).expect("the board's ACK").to_vec();
    host.on_bytes(now, &ack);
    assert_eq!(
        host.rto(),
        base * 2,
        "an ack of resent frames cleared the backoff"
    );

    // The next request is not resent at the un-backed-off timeout.
    let before = host.counters().clone();
    host.send(CH_PROTO, &[2; 40]).unwrap();
    now += 1_000;
    host.poll_transmit(now).expect("the next request goes out");
    now += base + 1_000; // past the base timeout, inside the doubled one
    while host.poll_transmit(now).is_some() {}
    let spent = host.counters().since(&before);
    assert_eq!(spent.timeouts, 0, "resent at the un-backed-off timeout");
    assert_eq!(spent.retransmits, 0, "resent at the un-backed-off timeout");
}

#[test]
fn an_ack_of_a_frame_sent_once_clears_the_backoff() {
    let (mut host, mut board) = established();
    let mut now = 1_000_000;
    let base = host.rto();

    // Time out once, so the backoff is up.
    host.send(CH_PROTO, &[1; 40]).unwrap();
    let first = host.poll_transmit(now).expect("first send").to_vec();
    now += base + 1_000;
    host.poll_transmit(now).expect("resend");
    assert_eq!(host.counters().timeouts, 1);
    assert_eq!(host.rto(), base * 2);

    // A frame sent once and acked is a fresh sample: the backoff clears.
    host.send(CH_PROTO, &[2; 40]).unwrap();
    let mut frames = vec![first];
    while let Some(f) = host.poll_transmit(now) {
        frames.push(f.to_vec());
    }
    for f in &frames {
        board.on_bytes(now, f);
    }
    now += 5_000;
    while let Some(ack) = board.poll_transmit(now) {
        let ack = ack.to_vec();
        host.on_bytes(now, &ack);
    }
    assert!(
        host.rto() < base * 2,
        "a fresh sample must clear the backoff"
    );
}

fn established() -> (L, L) {
    let cfg = LinkConfig::usb();
    let mut host: L = Link::new(cfg.clone(), 0x1111_2222);
    let mut board: L = Link::new(cfg, 0x3333_4444);
    let mut now = 0;
    for _ in 0..10 {
        for _ in 0..64 {
            let mut moved = false;
            while let Some(f) = host.poll_transmit(now) {
                let f = f.to_vec();
                board.on_bytes(now, &f);
                moved = true;
            }
            while let Some(f) = board.poll_transmit(now) {
                let f = f.to_vec();
                host.on_bytes(now, &f);
                moved = true;
            }
            if !moved {
                break;
            }
        }
        if host.state() == LinkState::Established && board.state() == LinkState::Established {
            while host.recv().is_some() {}
            while board.recv().is_some() {}
            return (host, board);
        }
        now += 200_000;
    }
    panic!("no handshake");
}
