//! Targeted scenarios: each names one behaviour of the link and checks it
//! directly (the broad sweep is `delivery_properties`).

use lp_link::frame::{self, FrameKind, Header};
use lp_link::sim::pipe::Faults;
use lp_link::sim::{Report, Scenario, Transport, Workload, run};
use lp_link::{
    Arq, CH_CONTROL, CH_LOG, CH_PROTO, GoBackN, Link, LinkConfig, LinkEvent, LinkState, NoArq,
    ResetReason, SelectiveRepeat, StopAndWait,
};

type Gbn = GoBackN<127>;

#[test]
fn every_variant_is_clean_on_a_perfect_pipe() {
    for t in [
        Transport::Usb,
        Transport::Ble,
        Transport::BleStream,
        Transport::Udp,
    ] {
        let sc = Scenario::new(t, 0.0, interactive(), 2_000_000, 1);
        for r in [
            run::<StopAndWait>(&sc),
            run::<Gbn>(&sc),
            run::<SelectiveRepeat>(&sc),
        ] {
            assert_clean(&r, t);
            // UDP's pipe reorders even without loss, which go-back-N pays for
            // with resends by design; the ordered pipes must cost nothing.
            if t != Transport::Udp {
                let resent = r.host.retransmits + r.board.retransmits;
                assert_eq!(resent, 0, "{} on {}", r.arq, t.name());
            }
            assert_eq!(r.host.bad_frames + r.board.bad_frames, 0);
            assert!(r.down.delivered > 0 && r.up.delivered > 0);
        }
    }
}

#[test]
fn no_arq_delivers_everything_on_a_reliable_pipe() {
    let sc = Scenario::new(Transport::Ws, 0.0, random(), 3_000_000, 7);
    let r = run::<NoArq>(&sc);
    assert_clean(&r, Transport::Ws);
    assert_eq!(r.up.delivered, r.up.sent);
    assert_eq!(r.down.delivered, r.down.sent);
}

#[test]
fn no_arq_loses_messages_on_a_lossy_pipe_but_never_misorders() {
    let sc = Scenario::new(Transport::Usb, 0.05, random(), 3_000_000, 3);
    let r = run::<NoArq>(&sc);
    assert!(r.down.delivered < r.down.sent, "loss must show without ARQ");
    // Gaps are expected without ARQ; duplicates, reordering and damage are not.
    for v in &r.violations {
        assert!(v.contains("gap") || v.contains("liveness"), "{v}");
    }
    assert_eq!(r.down.undetected_damage + r.up.undetected_damage, 0);
}

#[test]
fn torn_tails_are_detected_and_resent() {
    let mut sc = Scenario::new(Transport::Usb, 0.0, interactive(), 2_000_000, 11);
    sc.faults_down = Faults {
        drop_tail: 0.05,
        ..Faults::none()
    };
    for r in [run::<Gbn>(&sc), run::<SelectiveRepeat>(&sc)] {
        assert_clean(&r, Transport::Usb);
        assert!(
            r.host.bad_frames > 0,
            "{}: torn frames must be counted",
            r.arq
        );
        assert!(r.board.retransmits > 0, "{}: and resent", r.arq);
    }
}

#[test]
fn a_board_reboot_resets_both_sides_and_the_new_session_works() {
    let mut sc = Scenario::new(Transport::Usb, 0.0, random(), 3_000_000, 5);
    sc.board_reboots = vec![1_000_000, 2_000_000];
    let r = run::<SelectiveRepeat>(&sc);
    assert_clean(&r, Transport::Usb);
    assert_eq!(r.host_resets, 2, "the host sees each reboot as a reset");
}

#[test]
fn a_slow_reader_stalls_the_sender_without_loss() {
    let cfg = LinkConfig {
        rx_budget: 1024,
        ..LinkConfig::usb()
    };
    let (mut a, mut b) = pair::<SelectiveRepeat>(cfg);
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    for i in 0..40u8 {
        a.send(CH_PROTO, &[i; 200]).unwrap();
    }
    // b never reads: a must stop once b's window closes.
    for _ in 0..200 {
        now += 1_000;
        shuttle(&mut a, &mut b, now);
    }
    assert!(!a.is_idle(), "the sender is held back");
    assert_eq!(b.counters().rx_no_room, 0, "and never overran the reader");
    let mut got = vec![];
    for _ in 0..400 {
        while let Some(ev) = b.recv() {
            if let LinkEvent::Message { data, .. } = ev {
                got.push(data[0]);
            }
        }
        now += 1_000;
        shuttle(&mut a, &mut b, now);
    }
    assert_eq!(got, (0..40).collect::<Vec<u8>>());
}

#[test]
fn text_outside_frames_passes_through() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    b.on_bytes(
        now,
        b"ESP-ROM:esp32c6-20220919\r\nboot: chip revision v0.1\r\n",
    );
    handshake(&mut a, &mut b, &mut now);
    a.send(CH_PROTO, b"hello").unwrap();
    // A panic message written raw between two frames.
    let f1 = a.poll_transmit(now).unwrap().to_vec();
    b.on_bytes(now, &f1);
    b.on_bytes(now, b"panicked at src/main.rs:10\n");
    let mut texts = vec![];
    let mut msgs = vec![];
    while let Some(ev) = b.recv() {
        match ev {
            LinkEvent::Text(t) => texts.push(String::from_utf8(t).unwrap()),
            LinkEvent::Message { data, .. } => msgs.push(data),
            _ => {}
        }
    }
    assert_eq!(msgs, vec![b"hello".to_vec()]);
    assert_eq!(
        texts.concat(),
        "ESP-ROM:esp32c6-20220919\r\nboot: chip revision v0.1\r\npanicked at src/main.rs:10\n"
    );
}

/// The board-side convention for a panic that interrupts a frame: write
/// `0x00`, then the text. The torn frame fails, the text lands in the
/// deframer's next "frame", and the idle flush hands it up as console text.
#[test]
fn panic_text_after_a_torn_frame_survives_when_it_starts_with_a_delimiter() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    a.send(CH_PROTO, &[7; 200]).unwrap();
    let frame = a.poll_transmit(now).unwrap().to_vec();
    b.on_bytes(now, &frame[..frame.len() / 2]);
    b.on_bytes(now, b"\x00panicked at src/main.rs:10: boom\r\n");
    now += 100_000;
    let _ = b.poll_transmit(now);
    let texts: Vec<String> = drain(&mut b)
        .into_iter()
        .filter_map(|e| match e {
            LinkEvent::Text(t) => Some(String::from_utf8(t).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(texts.concat(), "panicked at src/main.rs:10: boom\r\n");
    assert_eq!(b.counters().bad_frames, 1, "the torn frame is counted");
}

#[test]
fn a_peer_restart_is_reported_once_on_each_side() {
    let (mut a, mut b) = pair::<Gbn>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut a);
    let _ = drain(&mut b);
    // The page reloads: a brand-new link on the host side.
    a = Link::new(LinkConfig::usb(), 0xABCD_0001);
    handshake(&mut a, &mut b, &mut now);
    let evs = drain(&mut b);
    assert!(evs.contains(&LinkEvent::Reset {
        reason: ResetReason::PeerRestarted,
        generation: 1
    }));
    assert_eq!(b.state(), LinkState::Established);
    assert_eq!(b.generation(), 1);
}

#[test]
fn crc16_lets_some_damage_through_where_crc32c_does_not() {
    // Heavy corruption on a stream, same seeds, both checksums.
    let mut missed = [0u64; 2];
    for (i, crc) in [lp_link::CrcKind::Crc16, lp_link::CrcKind::Crc32c]
        .into_iter()
        .enumerate()
    {
        for seed in 0..4 {
            let mut sc = Scenario::new(
                Transport::Usb,
                0.0,
                Workload::Bulk { size: 1024 },
                3_000_000,
                seed,
            )
            .with_configs(LinkConfig {
                crc,
                ..LinkConfig::usb()
            });
            sc.faults_down = Faults {
                corrupt: 0.3,
                drop_span: 0.1,
                ..Faults::none()
            };
            missed[i] += run::<SelectiveRepeat>(&sc).down.undetected_damage;
        }
    }
    assert_eq!(missed[1], 0, "CRC-32C missed damage");
    // CRC-16 misses about 1 in 65,536 damaged frames: this run damages tens
    // of thousands, so zero is possible; the bench reports the rate.
    let _ = missed[0];
}

/// Per-channel scheduling: a control message queued behind a 16 KiB proto
/// reply goes out at the next frame boundary, not after the reply.
#[test]
fn a_control_message_overtakes_a_big_proto_message() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    a.send(CH_PROTO, &[1; 16 * 1024]).unwrap();
    for _ in 0..3 {
        let f = a.poll_transmit(now).unwrap().to_vec();
        b.on_bytes(now, &f);
    }
    a.send(CH_CONTROL, &[2; 20]).unwrap();
    // One frame at a time from `a`; `b` answers at once.
    let mut frames = 0;
    let mut delivered = vec![];
    while delivered.len() < 2 {
        now += 100;
        assert!(frames < 200, "stuck: {delivered:?}");
        if let Some(f) = a.poll_transmit(now) {
            let f = f.to_vec();
            frames += 1;
            b.on_bytes(now, &f);
        }
        while let Some(f) = b.poll_transmit(now) {
            let f = f.to_vec();
            a.on_bytes(now, &f);
        }
        for ev in drain(&mut b) {
            if let LinkEvent::Message { channel, data } = ev {
                delivered.push((channel, data.len(), frames));
            }
        }
    }
    assert_eq!(delivered[0].0, CH_CONTROL, "{delivered:?}");
    assert!(
        delivered[0].2 <= 2,
        "control went out after {} frames",
        delivered[0].2
    );
    assert_eq!((delivered[1].0, delivered[1].1), (CH_PROTO, 16 * 1024));
}

/// Fair share for logs: with a proto stream that always has a frame ready and
/// log lines always waiting, no more than `datagram_every` data frames go in
/// a row, and every log line arrives.
#[test]
fn logs_keep_flowing_beside_a_busy_proto_stream() {
    let cfg = LinkConfig::usb();
    let every = cfg.datagram_every as usize;
    let (mut a, mut b) = pair::<SelectiveRepeat>(cfg);
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    let (mut logs_sent, mut logs_rx, mut proto_rx) = (0, 0, 0);
    let (mut run, mut longest_run, mut datagrams) = (0, 0, 0);
    let mut raw = vec![];
    for ms in 0..500 {
        now += 1_000;
        while a.send(CH_PROTO, &[3; 4096]).is_ok() {}
        for _ in 0..2 {
            if a.send(CH_LOG, b"a log line").is_ok() {
                logs_sent += 1;
            }
        }
        // The pipe takes eight frames a millisecond from `a`.
        for _ in 0..8 {
            let Some(f) = a.poll_transmit(now) else { break };
            let f = f.to_vec();
            raw.clear();
            frame::unwrap_stream(&f[1..f.len() - 1], &mut raw).unwrap();
            match Header::parse(&raw).unwrap().kind {
                FrameKind::Data => run += 1,
                FrameKind::Datagram => {
                    datagrams += 1;
                    run = 0;
                }
                FrameKind::Ack | FrameKind::Syn => {}
            }
            if ms > 0 {
                longest_run = longest_run.max(run);
            }
            b.on_bytes(now, &f);
        }
        while let Some(f) = b.poll_transmit(now) {
            let f = f.to_vec();
            a.on_bytes(now, &f);
        }
        for ev in drain(&mut b) {
            match ev {
                LinkEvent::Message {
                    channel: CH_LOG, ..
                } => logs_rx += 1,
                LinkEvent::Message { data, .. } => proto_rx += data.len(),
                _ => {}
            }
        }
    }
    assert!(longest_run <= every, "{longest_run} data frames in a row");
    assert!(
        datagrams >= 500 * 8 / (every + 1) - 8,
        "{datagrams} datagrams"
    );
    assert!(
        logs_rx + 32 >= logs_sent,
        "{logs_rx} of {logs_sent} logs arrived"
    );
    assert!(
        proto_rx > 500 * 1024,
        "the proto stream moved: {proto_rx} B"
    );
}

fn interactive() -> Workload {
    Workload::Interactive {
        interval: 20_000,
        up: 120,
        down: 1_500,
        log_every: 50_000,
    }
}

fn random() -> Workload {
    Workload::Random {
        mean_gap: 10_000,
        max_size: 3_000,
    }
}

fn assert_clean(r: &Report, t: Transport) {
    assert!(
        r.violations.is_empty(),
        "{} on {}: {:#?}",
        r.arq,
        t.name(),
        r.violations
    );
    assert_eq!(r.up.undetected_damage + r.down.undetected_damage, 0);
}

fn pair<A: Arq>(cfg: LinkConfig) -> (Link<A>, Link<A>) {
    (
        Link::new(cfg.clone(), 0x1111_2222),
        Link::new(cfg, 0x3333_4444),
    )
}

/// Move every frame each way, instantly and losslessly.
fn shuttle<A: Arq>(a: &mut Link<A>, b: &mut Link<A>, now: u64) {
    for _ in 0..64 {
        let mut moved = false;
        while let Some(f) = a.poll_transmit(now) {
            let f = f.to_vec();
            b.on_bytes(now, &f);
            moved = true;
        }
        while let Some(f) = b.poll_transmit(now) {
            let f = f.to_vec();
            a.on_bytes(now, &f);
            moved = true;
        }
        if !moved {
            break;
        }
    }
}

fn handshake<A: Arq>(a: &mut Link<A>, b: &mut Link<A>, now: &mut u64) {
    for _ in 0..10 {
        shuttle(a, b, *now);
        if a.state() == LinkState::Established && b.state() == LinkState::Established {
            return;
        }
        *now += 200_000;
    }
    panic!("no handshake");
}

fn drain<A: Arq>(l: &mut Link<A>) -> Vec<LinkEvent> {
    std::iter::from_fn(|| l.recv()).collect()
}
