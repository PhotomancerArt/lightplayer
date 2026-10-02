//! Targeted scenarios: each names one behaviour of the link and checks it
//! directly (the broad sweep is `delivery_properties`).

use lp_link::frame::{self, FrameKind, Header};
use lp_link::sim::pipe::Faults;
use lp_link::sim::{Report, Scenario, Transport, Workload, run};
use lp_link::{
    Arq, CH_CONTROL, CH_LOG, CH_PROTO, GoBackN, Link, LinkConfig, LinkEvent, LinkState,
    MAX_MESSAGE, NoArq, ResetReason, SelectiveRepeat, SendError, StopAndWait,
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
fn the_uart_preset_recovers_loss_with_its_narrower_windows() {
    // A stream pipe (the USB model: the framing is the same) with uart()'s
    // four-frame windows on both ends: loss is found and resent, nothing is
    // delivered twice, out of order or damaged.
    let mut sc = Scenario::new(Transport::Usb, 0.02, random(), 3_000_000, 17);
    sc.host_cfg = LinkConfig::uart();
    sc.board_cfg = LinkConfig::uart();
    let r = run::<SelectiveRepeat>(&sc);
    assert_clean(&r, Transport::Usb);
    assert!(
        r.host.retransmits + r.board.retransmits > 0,
        "loss was resent"
    );
    assert!(r.down.delivered > 0 && r.up.delivered > 0);
}

#[test]
fn a_board_reboot_resets_both_sides_and_the_new_session_works() {
    let mut sc = Scenario::new(Transport::Usb, 0.0, random(), 3_000_000, 5);
    sc.board_reboots = vec![1_000_000, 2_000_000];
    let r = run::<SelectiveRepeat>(&sc);
    assert_clean(&r, Transport::Usb);
    assert_eq!(r.host_resets, 2, "the host sees each reboot as a reset");
}

/// `syn_backoff` (ruling DD27 of the classic-UART plan): a board nobody
/// answers doubles its SYN gap up to its ceiling instead of writing a SYN
/// every `syn_interval` forever. Ten seconds with nobody there, four
/// doublings: 100, 200, 400, 800 ms, then one every 1.6 s.
#[test]
fn an_unanswered_link_backs_off_its_syns_to_the_ceiling() {
    let backed_off = LinkConfig {
        syn_backoff: 4,
        ..LinkConfig::uart()
    };
    let at = syn_times(&mut Link::<SelectiveRepeat>::new(backed_off, 7), 10_000_000);
    let gaps: Vec<u64> = at.windows(2).map(|w| (w[1] - w[0]) / 1_000).collect();
    assert_eq!(&gaps[..5], &[100, 200, 400, 800, 1_600], "{gaps:?}");
    assert!(gaps[4..].iter().all(|&g| g == 1_600), "{gaps:?}");
    assert_eq!(at.len(), 10, "SYNs in ten seconds: {at:?}");
}

/// The knob defaults to off: every preset keeps today's fixed interval.
#[test]
fn without_the_knob_syns_stay_one_interval_apart() {
    for (name, cfg) in [
        ("usb", LinkConfig::usb()),
        ("uart", LinkConfig::uart()),
        ("ble", LinkConfig::ble()),
    ] {
        assert_eq!(cfg.syn_backoff, 0, "{name}");
        let interval = cfg.syn_interval;
        let at = syn_times(&mut Link::<SelectiveRepeat>::new(cfg, 7), 2_000_000);
        assert!(
            at.windows(2).all(|w| w[1] - w[0] == interval),
            "{name}: {at:?}"
        );
    }
}

/// Any byte heard while handshaking (a host opening the port and writing
/// anything at all) brings the gap back to `syn_interval` at once.
#[test]
fn any_byte_heard_snaps_the_syn_gap_back() {
    let cfg = LinkConfig {
        syn_backoff: 4,
        ..LinkConfig::uart()
    };
    let mut board = Link::<SelectiveRepeat>::new(cfg, 7);
    let backed_off = syn_times(&mut board, 5_000_000);
    let last = *backed_off.last().unwrap();
    // Well inside a 1.6 s gap: a stray byte of text.
    let heard = last + 50_000;
    board.on_bytes(heard, b"x");
    let mut after = Vec::new();
    let mut now = heard;
    while now < heard + 1_000_000 {
        now += 1_000;
        while board.poll_transmit(now).is_some() {
            after.push(now);
        }
    }
    assert_eq!(
        after.first().copied(),
        Some(heard + 100_000),
        "the next SYN goes one syn_interval after the byte: {after:?}"
    );
    let gaps: Vec<u64> = after.windows(2).map(|w| (w[1] - w[0]) / 1_000).collect();
    assert_eq!(gaps, vec![100, 200, 400], "and backs off again from there");
}

/// The backoff never delays a host: a host that turns up after the board
/// has backed off to its ceiling is answered at once, and both ends are up
/// within one exchange.
#[test]
fn a_backed_off_board_answers_a_late_host_at_once() {
    let cfg = LinkConfig {
        syn_backoff: 4,
        ..LinkConfig::uart()
    };
    let mut board = Link::<SelectiveRepeat>::new(cfg, 0x3333_4444);
    let at = syn_times(&mut board, 7_300_000);
    let now = *at.last().unwrap() + 300_000; // mid-gap
    let mut host = Link::<SelectiveRepeat>::new(LinkConfig::uart(), 0x1111_2222);
    shuttle(&mut host, &mut board, now);
    assert_eq!(host.state(), LinkState::Established);
    assert_eq!(board.state(), LinkState::Established);
}

/// Sim: a backed-off board across two reboots and an outage long enough to
/// reset the link is still clean: nothing lost, doubled or reordered, and
/// every reboot is seen.
#[test]
fn a_backed_off_board_stays_clean_across_reboots_and_an_outage() {
    let mut sc = Scenario::new(Transport::Usb, 0.02, random(), 6_000_000, 23);
    sc.host_cfg = LinkConfig::uart();
    sc.board_cfg = LinkConfig {
        syn_backoff: 4,
        ..LinkConfig::uart()
    };
    sc.board_reboots = vec![1_500_000, 4_000_000];
    sc.outage = Some((2_000_000, 3_500_000));
    let r = run::<SelectiveRepeat>(&sc);
    assert_clean(&r, Transport::Usb);
    assert!(
        r.host_resets >= 2,
        "each reboot is a reset: {}",
        r.host_resets
    );
    assert!(r.down.delivered > 0 && r.up.delivered > 0);
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

/// main's (proto-29) firmware sends a short marker containing `0x00` when
/// the port opens. Taken as a frame start, followed by an old board's
/// continuous plain-text console (no quiet gap for `frame_abandon` to fire
/// on), that must not hold the console text for 3 s: once the misread
/// partial reaches `max_frame` it flushes as text at once.
#[test]
fn text_after_a_stray_leading_zero_is_not_held_for_frame_abandon() {
    let (_a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let now = 0;
    // The stray 0x00, then an M!-line stream well past max_frame (531 B for
    // the usb preset) with no gap, so frame_abandon never gets a chance.
    let mut stream = vec![0u8];
    for i in 0..100u32 {
        stream.extend_from_slice(format!("M!{i:05}\n").as_bytes());
    }
    b.on_bytes(now, &stream);
    let texts: Vec<String> = drain(&mut b)
        .into_iter()
        .filter_map(|e| match e {
            LinkEvent::Text(t) => Some(String::from_utf8(t).unwrap()),
            _ => None,
        })
        .collect();
    assert!(
        !texts.is_empty(),
        "an M!-line stream after a stray 0x00 must arrive as text without \
         waiting on frame_abandon, at time `now` with no idle at all"
    );
    assert!(texts.concat().starts_with("M!00000\n"), "{texts:?}");
    assert_eq!(
        b.counters().stale_partials,
        0,
        "flushed before it went stale"
    );
}

/// The older board-side convention for a panic that interrupts a frame:
/// write `0x00`, then the text. The torn frame fails, the text lands in the
/// deframer's next "frame", and once that partial has been quiet for
/// `frame_abandon` it is handed up as console text. (The product writes the
/// `0xFF` text mark instead, which needs no timeout at all.)
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
    now += LinkConfig::usb().frame_abandon + 1_000;
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

/// A busy board writes one frame in 64-byte USB packets with a render tick
/// (~80 ms) between them. The receiver must wait for the rest, not abandon
/// the partial at the text idle time: the 2026-09-27 rehearsal
/// (`silicon:esp32c6 10:bd:a3:b0:8e:30`) saw every palette cross-fade stall
/// the link for 4 s while each resend was split and abandoned again.
#[test]
fn a_frame_written_in_slow_pieces_is_received_whole() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    a.send(CH_PROTO, &[7; 200]).unwrap();
    let frame = a.poll_transmit(now).unwrap().to_vec();
    for piece in frame.chunks(64) {
        b.on_bytes(now, piece);
        now += 80_000;
        // The receiver's timers run between pieces, as they do on a host.
        while b.poll_transmit(now).is_some() {}
    }
    let msgs: Vec<Vec<u8>> = drain(&mut b)
        .into_iter()
        .filter_map(|e| match e {
            LinkEvent::Message { data, .. } => Some(data),
            _ => None,
        })
        .collect();
    assert_eq!(msgs, vec![vec![7; 200]]);
    assert_eq!(b.counters().stale_partials, 0, "no partial was abandoned");
    assert_eq!(b.counters().bad_frames, 0);
}

#[test]
fn the_rtt_accessors_report_the_round_trip_a_frame_took() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut a);
    let _ = drain(&mut b);
    let (_, before) = a.rtt_last_sample();

    // One data frame out at `now`; it arrives at once, and the peer's ACK
    // comes back 7 ms later.
    a.send(CH_PROTO, &[1; 32]).unwrap();
    let sent_at = now;
    let frame = a.poll_transmit(now).unwrap().to_vec();
    b.on_bytes(now, &frame);
    now += 7_000;
    while let Some(ack) = b.poll_transmit(now) {
        let ack = ack.to_vec();
        a.on_bytes(now, &ack);
    }

    let (last, count) = a.rtt_last_sample();
    assert_eq!(count, before.wrapping_add(1), "one new sample");
    assert_eq!(last, now - sent_at, "send to ACK");
    assert!(a.srtt() > 0 && a.srtt() <= LinkConfig::usb().initial_rto);
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

/// Two channels mid-message at once, each nearly the budget: the window must
/// stay open for both to finish (found by `decoder_fuzz`: counting partial
/// messages against the window closed it with neither able to complete).
#[test]
fn two_big_messages_on_two_channels_both_arrive() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    a.send(CH_PROTO, &[1; 12 * 1024]).unwrap();
    // Most of the proto message arrives before the control one is queued.
    let mut frames = 0;
    while frames < 40 {
        now += 100;
        if let Some(f) = a.poll_transmit(now) {
            let f = f.to_vec();
            b.on_bytes(now, &f);
            frames += 1;
        }
        while let Some(f) = b.poll_transmit(now) {
            let f = f.to_vec();
            a.on_bytes(now, &f);
        }
    }
    a.send(CH_CONTROL, &[2; MAX_MESSAGE - 500]).unwrap();
    let mut got = vec![];
    for _ in 0..500 {
        now += 1_000;
        shuttle(&mut a, &mut b, now);
        for ev in drain(&mut b) {
            if let LinkEvent::Message { channel, data } = ev {
                got.push((channel, data.len()));
            }
        }
    }
    assert_eq!(
        got,
        vec![(CH_CONTROL, MAX_MESSAGE - 500), (CH_PROTO, 12 * 1024)]
    );
    assert!(a.is_idle());
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

/// `datagram_room` is what `send` would take: a caller that pops each log
/// record out of its own ring asks it first, so a record the link would
/// refuse is never taken (the classic's `[OUT] dump` lost parts that way —
/// docs/defects/2026-09-29-the-classics-log-ring-drops-records-under-a-project-load-burst.md).
#[test]
fn datagram_room_counts_the_slots_send_would_fill() {
    let cfg = LinkConfig {
        datagram_queue: 2,
        ..LinkConfig::uart()
    };
    let (mut a, mut b) = pair::<SelectiveRepeat>(cfg);
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    assert_eq!(a.datagram_room(), 2);
    a.send(CH_LOG, b"one").unwrap();
    assert_eq!(a.datagram_room(), 1);
    a.send(CH_LOG, b"two").unwrap();
    assert_eq!(a.datagram_room(), 0);
    assert_eq!(a.send(CH_LOG, b"three"), Err(SendError::Full));
    assert_eq!(a.counters().datagrams_dropped, 1, "a refusal is counted");
    let f = a.poll_transmit(now).expect("the oldest datagram").to_vec();
    b.on_bytes(now, &f);
    assert_eq!(a.datagram_room(), 1, "sending one frees its slot");
}

/// The message budget: the wire's 16 KiB frame budget plus slack. Longer is
/// refused at `send`.
#[test]
fn a_message_past_the_budget_is_too_big_to_send() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    assert_eq!(MAX_MESSAGE, 17 * 1024);
    assert_eq!(
        a.send(CH_PROTO, &vec![0; MAX_MESSAGE + 1]),
        Err(SendError::TooBig)
    );
    assert_eq!(a.send(CH_PROTO, &vec![0; MAX_MESSAGE]), Ok(()));
}

/// A peer with a bigger limit sends a message past ours: it is dropped and
/// counted, its frames are still acknowledged (the sender is not stuck
/// resending), and the session and the next message carry on.
#[test]
fn an_oversize_message_is_dropped_without_a_reset() {
    let big = LinkConfig {
        max_message: 40 * 1024,
        send_budget: 48 * 1024,
        ..LinkConfig::usb()
    };
    let mut a = Link::<SelectiveRepeat>::new(big, 0x1111_2222);
    let mut b = Link::<SelectiveRepeat>::new(LinkConfig::usb(), 0x3333_4444);
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    a.send(CH_PROTO, &[5; 30 * 1024]).unwrap();
    a.send(CH_PROTO, b"after").unwrap();
    let mut got = vec![];
    for _ in 0..200 {
        now += 1_000;
        shuttle(&mut a, &mut b, now);
        got.extend(drain(&mut b));
    }
    assert_eq!(
        got,
        vec![LinkEvent::Message {
            channel: CH_PROTO,
            data: b"after".to_vec()
        }]
    );
    assert_eq!(b.counters().oversize_messages, 1);
    assert_eq!(a.counters().resets + b.counters().resets, 0);
    assert!(a.is_idle(), "everything was acknowledged");
}

/// External messages (the caller keeps the bytes) under loss and damage in
/// both directions: each arrives whole and in its channel's order among ring
/// messages, a control message still overtakes, and the caller's buffer is
/// read only while `external_in_flight` says so (it is scribbled over the
/// moment that turns false, and the resends still carry the right bytes).
#[test]
fn external_messages_arrive_whole_under_faults() {
    let board_cfg = LinkConfig {
        send_budget: 3 * 1024,
        ..LinkConfig::usb()
    };
    let mut a = Link::<SelectiveRepeat>::new(board_cfg, 0x1111_2222);
    let mut b = Link::<SelectiveRepeat>::new(LinkConfig::usb(), 0x3333_4444);
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    let mut rng = 0x2545_f491_u64;
    let mut buf = vec![0u8; MAX_MESSAGE];
    let mut want: Vec<Vec<u8>> = vec![];
    let mut got: Vec<Vec<u8>> = vec![];
    let mut controls = 0;
    let mut round = 0usize;
    while round < 12 || a.external_in_flight() || !a.is_idle() {
        assert!(
            now < 600_000_000,
            "stuck: {} of {} delivered",
            got.len(),
            want.len()
        );
        if round < 12 && !a.external_in_flight() {
            // The caller's next big reply, in its own buffer.
            let len = 1 + (round * 1_531) % (16 * 1024);
            for (i, x) in buf[..len].iter_mut().enumerate() {
                *x = (i + round * 31) as u8;
            }
            a.send_external(CH_PROTO, len).unwrap();
            assert_eq!(a.send_external(CH_PROTO, 1), Err(SendError::Full));
            want.push(buf[..len].to_vec());
            // A small ring message behind it keeps its place.
            let small = vec![round as u8; 40];
            a.send(CH_PROTO, &small).unwrap();
            want.push(small);
            if round % 3 == 0 {
                a.send(CH_CONTROL, b"ctl").unwrap();
                controls += 1;
            }
            round += 1;
        }
        now += 1_000;
        for _ in 0..16 {
            let mut moved = false;
            while let Some(f) = a.poll_transmit_with(now, &mut |off, out| {
                out.copy_from_slice(&buf[off..off + out.len()])
            }) {
                let f = f.to_vec();
                moved = true;
                if let Some(f) = mangle(&mut rng, f) {
                    b.on_bytes(now, &f);
                }
            }
            if !a.external_in_flight() {
                // Every byte is cut: the buffer is the caller's again.
                buf.fill(0xEE);
            }
            while let Some(f) = b.poll_transmit(now) {
                let f = f.to_vec();
                moved = true;
                if let Some(f) = mangle(&mut rng, f) {
                    a.on_bytes(now, &f);
                }
            }
            if !moved {
                break;
            }
        }
        for ev in drain(&mut b) {
            match ev {
                LinkEvent::Message {
                    channel: CH_PROTO,
                    data,
                } => got.push(data),
                LinkEvent::Message {
                    channel: CH_CONTROL,
                    ..
                } => controls -= 1,
                other => panic!("unexpected {other:?}"),
            }
        }
    }
    assert_eq!(got.len(), want.len());
    assert!(got == want, "every message whole and in order");
    assert_eq!(controls, 0);
    assert_eq!(a.counters().resets + b.counters().resets, 0);
    assert!(a.counters().retransmits > 0, "the faults cost resends");
}

/// Withdrawing an external message: only before its first fragment; a reset
/// drops one in flight and hands the buffer back.
#[test]
fn an_external_message_is_withdrawn_only_before_it_starts() {
    let (mut a, mut b) = pair::<SelectiveRepeat>(LinkConfig::usb());
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    let _ = drain(&mut b);
    assert_eq!(a.send_external(CH_LOG, 10), Err(SendError::BadChannel));
    assert_eq!(
        a.send_external(CH_PROTO, MAX_MESSAGE + 1),
        Err(SendError::TooBig)
    );

    a.send_external(CH_PROTO, 4_000).unwrap();
    assert_eq!(a.cancel_external(), Ok(()));
    assert!(!a.external_in_flight());
    shuttle(&mut a, &mut b, now);
    assert!(drain(&mut b).is_empty(), "nothing of it went out");

    a.send_external(CH_PROTO, 4_000).unwrap();
    let f = a
        .poll_transmit_with(now, &mut |_, out| out.fill(1))
        .map(<[u8]>::to_vec);
    assert!(f.is_some());
    assert_eq!(a.cancel_external(), Err(lp_link::ExternalStarted));
    assert!(a.external_in_flight());
    a.restart(now);
    assert!(!a.external_in_flight(), "the reset dropped it");
}

/// Without a source an external message waits; the send ring alone is
/// bounded by `send_budget`, which may be below `max_message`.
#[test]
fn a_small_send_budget_refuses_big_ring_messages_but_not_external_ones() {
    let cfg = LinkConfig {
        send_budget: 2 * 1024,
        ..LinkConfig::usb()
    };
    assert_eq!(cfg.validate(), Ok(()));
    let (mut a, mut b) = pair::<SelectiveRepeat>(cfg);
    let mut now = 0;
    handshake(&mut a, &mut b, &mut now);
    assert_eq!(a.send(CH_PROTO, &[0; 3 * 1024]), Err(SendError::TooBig));
    a.send_external(CH_PROTO, 12 * 1024).unwrap();
    now += 10_000;
    while let Some(f) = a.poll_transmit(now) {
        assert!(f.len() < 64, "no data frame without a source");
    }
    assert!(a.external_in_flight());
}

/// A lossy, damaging wire for the external-message scenario: drop 3%, flip a
/// byte in 2%.
fn mangle(rng: &mut u64, mut f: Vec<u8>) -> Option<Vec<u8>> {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    match *rng % 100 {
        0..=2 => None,
        3..=4 => {
            let at = (*rng as usize / 100) % f.len();
            f[at] ^= 0x5A;
            Some(f)
        }
        _ => Some(f),
    }
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

/// Step a link nobody answers in 1 ms ticks up to `until`, and return the
/// time of every frame it wrote (all SYNs: it never connects).
fn syn_times<A: Arq>(link: &mut Link<A>, until: u64) -> Vec<u64> {
    let mut at = Vec::new();
    let mut raw = Vec::new();
    let mut now = 0;
    while now <= until {
        while let Some(f) = link.poll_transmit(now) {
            let body = if f[0] == 0 {
                // Stream framing: undo the COBS.
                raw.clear();
                frame::unwrap_stream(&f[1..f.len() - 1], &mut raw).unwrap();
                &raw[..]
            } else {
                f
            };
            assert_eq!(Header::parse(body).unwrap().kind, FrameKind::Syn);
            at.push(now);
        }
        now += 1_000;
    }
    at
}

fn drain<A: Arq>(l: &mut Link<A>) -> Vec<LinkEvent> {
    std::iter::from_fn(|| l.recv()).collect()
}
