//! The pusher scheduler on the host: waves, cap, takeover, close, abort —
//! the real [`Pusher`] against [`MockRmt`], single-threaded and
//! deterministic. (The concurrency story is `cross_core.rs`'s; this file is
//! about *scheduling* being right.)
//!
//! The 8-wire scenarios are the design target's host validation: silicon has
//! four slots and five pads today, so two-full-waves-of-four exists only
//! here until the 8-pad rig era (deferred — see the overlap plan).

#![cfg(feature = "mock")]

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::vec::Vec;

use common::ramp_frame;
use lp_ws281x::{ChannelTiming, MockRmt, PadOps, Pusher, WireMailbox, WireOutcome, Ws281xDriver};

/// Slot channels used by most tests: all four MockRmt channels.
const SLOTS: [u8; 4] = [0, 1, 2, 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PadEvent {
    Route { slot: u8, gpio: u8 },
    Park { gpio: u8 },
}

/// A [`PadOps`] that records instead of touching a GPIO matrix.
#[derive(Clone, Default)]
struct RecorderPads(Rc<RefCell<Vec<PadEvent>>>);

impl PadOps for RecorderPads {
    fn route_to(&mut self, slot_channel: u8, gpio: u8) {
        self.0.borrow_mut().push(PadEvent::Route {
            slot: slot_channel,
            gpio,
        });
    }
    fn park(&mut self, gpio: u8) {
        self.0.borrow_mut().push(PadEvent::Park { gpio });
    }
}

fn driver() -> Ws281xDriver<MockRmt, 4> {
    let driver: Ws281xDriver<MockRmt, 4> = Ws281xDriver::new(MockRmt::new(4, 48));
    for ch in 0..4 {
        driver
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .unwrap();
    }
    driver
}

/// Advance every transmitter and dispatch the handler, then run one
/// scheduling pass — the host's stand-in for "an interrupt woke the pusher".
fn tick<const W: usize>(
    driver: &Ws281xDriver<MockRmt, 4>,
    pusher: &mut Pusher<'_, MockRmt, RecorderPads, 4, W>,
) {
    driver.hw().advance_all(4);
    driver.on_interrupt();
    pusher.service();
}

/// Which wires currently have a frame on a slot, by mailbox evidence.
fn active_wires<const W: usize>(mailboxes: &[WireMailbox; W]) -> Vec<usize> {
    (0..W)
        .filter(|&w| mailboxes[w].active_channel().is_some())
        .collect()
}

/// Post one fresh frame on every wire; returns (frames, seqs). The frames
/// must outlive the transmissions — the caller keeps the Vec alive to the
/// end of the test.
fn post_all<const W: usize>(mailboxes: &[WireMailbox; W]) -> (Vec<Box<[u8]>>, Vec<u32>) {
    let mut frames = Vec::new();
    let mut seqs = Vec::new();
    for (wire, mailbox) in mailboxes.iter().enumerate() {
        let frame: Box<[u8]> = ramp_frame(4).into_boxed_slice();
        // SAFETY: `frames` outlives every transmission — the test drains the
        // pusher before dropping it.
        let seq = unsafe { mailbox.post(wire as u8 + 10, frame.as_ptr(), frame.len()) };
        frames.push(frame);
        seqs.push(seq);
    }
    (frames, seqs)
}

/// Run ticks until every posted sequence reports an outcome (bounded).
fn drain<const W: usize>(
    driver: &Ws281xDriver<MockRmt, 4>,
    pusher: &mut Pusher<'_, MockRmt, RecorderPads, 4, W>,
    mailboxes: &[WireMailbox; W],
    seqs: &[u32],
) {
    for _ in 0..10_000 {
        if (0..W).all(|w| mailboxes[w].completed_outcome(seqs[w]).is_some()) {
            return;
        }
        tick(driver, pusher);
    }
    panic!("pusher failed to drain within the tick budget");
}

/// Eight wires over four slots at cap 4: two full waves, every wire exactly
/// once, never more than four on the wire at any instant.
#[test]
fn eight_wires_transmit_as_two_waves_of_four() {
    let driver = driver();
    let mailboxes: [WireMailbox; 8] = core::array::from_fn(|_| WireMailbox::new());
    let mut pusher = Pusher::new(&driver, &mailboxes, RecorderPads::default(), &SLOTS, 4);

    let (frames, seqs) = post_all(&mailboxes);
    pusher.service();

    // Wave 1: exactly four wires, and round-robin from wire 0.
    assert_eq!(active_wires(&mailboxes), vec![0, 1, 2, 3]);
    assert_eq!(pusher.transmitting(), 4);

    // Run until the second wave is on the wire; the cap must never be
    // exceeded at any tick in between.
    for _ in 0..10_000 {
        tick(&driver, &mut pusher);
        assert!(pusher.transmitting() <= 4, "cap must hold at every instant");
        if active_wires(&mailboxes) == vec![4, 5, 6, 7] {
            break;
        }
    }
    assert_eq!(
        active_wires(&mailboxes),
        vec![4, 5, 6, 7],
        "wave 2 must be the other four wires"
    );

    drain(&driver, &mut pusher, &mailboxes, &seqs);
    for (wire, seq) in seqs.iter().enumerate() {
        assert_eq!(
            mailboxes[wire].completed_outcome(*seq),
            Some(WireOutcome::Transmitted),
            "wire {wire} must transmit exactly its posted frame"
        );
    }
    // Two waves of four over four slots: every slot carried exactly two
    // frames.
    for ch in 0..4 {
        assert_eq!(driver.stats(ch).frames, 2, "slot {ch} carries two wires");
        assert_eq!(driver.stats(ch).guard_trips, 0);
    }
    drop(frames);
}

/// Five wires over four slots: a wave of four plus a wave of one — the Zook
/// dome shape. The fifth wire's start needs no poster-side call after the
/// post: a completion alone must trigger it.
#[test]
fn five_wires_wave_four_plus_one() {
    let driver = driver();
    let mailboxes: [WireMailbox; 5] = core::array::from_fn(|_| WireMailbox::new());
    let mut pusher = Pusher::new(&driver, &mailboxes, RecorderPads::default(), &SLOTS, 4);

    let (frames, seqs) = post_all(&mailboxes);
    pusher.service();
    assert_eq!(active_wires(&mailboxes), vec![0, 1, 2, 3]);

    drain(&driver, &mut pusher, &mailboxes, &seqs);
    for (wire, seq) in seqs.iter().enumerate() {
        assert_eq!(
            mailboxes[wire].completed_outcome(*seq),
            Some(WireOutcome::Transmitted),
            "wire {wire}"
        );
    }
    // One slot carried two frames (the waved wire), the rest one each.
    let mut per_slot: Vec<usize> = (0..4).map(|ch| driver.stats(ch).frames).collect();
    per_slot.sort_unstable();
    assert_eq!(per_slot, vec![1, 1, 1, 2], "the 2:1 muxing signature");
    drop(frames);
}

/// Cap 3 over four slots (the single-core-adjacent degradation shape used by
/// the plan's tiering): eight wires go out 3+3+2, never more than three at
/// once.
#[test]
fn cap_three_runs_eight_wires_as_three_waves() {
    let driver = driver();
    let mailboxes: [WireMailbox; 8] = core::array::from_fn(|_| WireMailbox::new());
    let mut pusher = Pusher::new(&driver, &mailboxes, RecorderPads::default(), &SLOTS, 3);

    let (frames, seqs) = post_all(&mailboxes);
    pusher.service();
    assert_eq!(active_wires(&mailboxes).len(), 3);

    let mut max_active = 0;
    for _ in 0..10_000 {
        if (0..8).all(|w| mailboxes[w].completed_outcome(seqs[w]).is_some()) {
            break;
        }
        tick(&driver, &mut pusher);
        max_active = max_active.max(pusher.transmitting());
    }
    assert_eq!(max_active, 3, "cap 3 must bind, and be reached");
    for (wire, seq) in seqs.iter().enumerate() {
        assert_eq!(
            mailboxes[wire].completed_outcome(*seq),
            Some(WireOutcome::Transmitted),
            "wire {wire}"
        );
    }
    drop(frames);
}

/// Takeover pad discipline: the displaced pad is parked *before* the slot's
/// signal is routed to the new pad, and the steady state (same wire, same
/// slot) costs zero matrix writes.
#[test]
fn takeover_parks_the_displaced_pad_before_routing() {
    let driver = driver();
    let mailboxes: [WireMailbox; 2] = core::array::from_fn(|_| WireMailbox::new());
    let pads = RecorderPads::default();
    let log = Rc::clone(&pads.0);
    // One slot, two wires: every handover is a takeover.
    let mut pusher = Pusher::new(&driver, &mailboxes, pads, &SLOTS[..1], 1);

    let frame_a: Box<[u8]> = ramp_frame(4).into_boxed_slice();
    let frame_b: Box<[u8]> = ramp_frame(4).into_boxed_slice();
    // SAFETY: both frames outlive their transmissions (drained below).
    let seq_a = unsafe { mailboxes[0].post(10, frame_a.as_ptr(), frame_a.len()) };
    let seq_b = unsafe { mailboxes[1].post(11, frame_b.as_ptr(), frame_b.len()) };

    pusher.service();
    // Fresh slot: route only, no park (nothing was displaced).
    assert_eq!(log.borrow().as_slice(), &[PadEvent::Route { slot: 0, gpio: 10 }]);

    for _ in 0..10_000 {
        if mailboxes[1].completed_outcome(seq_b).is_some() {
            break;
        }
        tick(&driver, &mut pusher);
    }
    assert_eq!(mailboxes[0].completed_outcome(seq_a), Some(WireOutcome::Transmitted));
    assert_eq!(mailboxes[1].completed_outcome(seq_b), Some(WireOutcome::Transmitted));
    assert_eq!(
        log.borrow().as_slice(),
        &[
            PadEvent::Route { slot: 0, gpio: 10 },
            PadEvent::Park { gpio: 10 },
            PadEvent::Route { slot: 0, gpio: 11 },
        ],
        "takeover parks the displaced pad first, then routes"
    );

    // Steady state: wire 1 transmits again on the slot it now owns — zero
    // further pad events.
    let events_before = log.borrow().len();
    let frame_c: Box<[u8]> = ramp_frame(4).into_boxed_slice();
    // SAFETY: drained below.
    let seq_c = unsafe { mailboxes[1].post(11, frame_c.as_ptr(), frame_c.len()) };
    for _ in 0..10_000 {
        if mailboxes[1].completed_outcome(seq_c).is_some() {
            break;
        }
        tick(&driver, &mut pusher);
    }
    assert_eq!(mailboxes[1].completed_outcome(seq_c), Some(WireOutcome::Transmitted));
    assert_eq!(
        log.borrow().len(),
        events_before,
        "an owned slot re-transmits with zero matrix writes"
    );
    drop((frame_a, frame_b, frame_c));
}

/// Close quiesces both shapes: an in-flight frame is aborted off the wire,
/// a queued frame is cancelled unstarted — and the wire is reusable after.
#[test]
fn close_aborts_in_flight_and_cancels_queued() {
    let driver = driver();
    let mailboxes: [WireMailbox; 2] = core::array::from_fn(|_| WireMailbox::new());
    // One slot: wire 0 will be in flight, wire 1 stuck queued behind it.
    let mut pusher = Pusher::new(
        &driver,
        &mailboxes,
        RecorderPads::default(),
        &SLOTS[..1],
        1,
    );

    let frame_a: Box<[u8]> = ramp_frame(8).into_boxed_slice();
    let frame_b: Box<[u8]> = ramp_frame(8).into_boxed_slice();
    // SAFETY: frame_a is quiesced by the acked close below; frame_b is
    // cancelled unstarted by its own close.
    let seq_a = unsafe { mailboxes[0].post(10, frame_a.as_ptr(), frame_a.len()) };
    let seq_b = unsafe { mailboxes[1].post(11, frame_b.as_ptr(), frame_b.len()) };
    pusher.service();
    assert_eq!(active_wires(&mailboxes), vec![0]);

    // Queued wire closes: cancelled, never started.
    let close_b = mailboxes[1].request_close();
    pusher.service();
    assert!(mailboxes[1].close_acked(close_b));
    assert_eq!(mailboxes[1].completed_outcome(seq_b), Some(WireOutcome::Cancelled));
    drop(frame_b); // Reclaimable the moment the ack lands.

    // In-flight wire closes: aborted off the wire.
    let close_a = mailboxes[0].request_close();
    pusher.service();
    assert!(mailboxes[0].close_acked(close_a));
    assert_eq!(mailboxes[0].completed_outcome(seq_a), Some(WireOutcome::Aborted));
    drop(frame_a);
    assert_eq!(pusher.transmitting(), 0, "the slot is free again");

    // The wire lives on after a close: a fresh post transmits normally.
    let frame_c: Box<[u8]> = ramp_frame(4).into_boxed_slice();
    // SAFETY: drained below.
    let seq_c = unsafe { mailboxes[0].post(10, frame_c.as_ptr(), frame_c.len()) };
    for _ in 0..10_000 {
        if mailboxes[0].completed_outcome(seq_c).is_some() {
            break;
        }
        tick(&driver, &mut pusher);
    }
    assert_eq!(mailboxes[0].completed_outcome(seq_c), Some(WireOutcome::Transmitted));
    drop(frame_c);
}

/// The poster's hang-recovery verb: an abort request disposes of an
/// in-flight frame as Aborted and a queued frame as Cancelled.
#[test]
fn abort_request_disposes_in_flight_and_queued_frames() {
    let driver = driver();
    let mailboxes: [WireMailbox; 2] = core::array::from_fn(|_| WireMailbox::new());
    let mut pusher = Pusher::new(
        &driver,
        &mailboxes,
        RecorderPads::default(),
        &SLOTS[..1],
        1,
    );

    let frame_a: Box<[u8]> = ramp_frame(8).into_boxed_slice();
    let frame_b: Box<[u8]> = ramp_frame(8).into_boxed_slice();
    // SAFETY: both frames outlive their disposal (asserted below).
    let seq_a = unsafe { mailboxes[0].post(10, frame_a.as_ptr(), frame_a.len()) };
    let seq_b = unsafe { mailboxes[1].post(11, frame_b.as_ptr(), frame_b.len()) };
    pusher.service();

    mailboxes[0].request_abort(seq_a);
    mailboxes[1].request_abort(seq_b);
    pusher.service();
    assert_eq!(mailboxes[0].completed_outcome(seq_a), Some(WireOutcome::Aborted));
    assert_eq!(mailboxes[1].completed_outcome(seq_b), Some(WireOutcome::Cancelled));
    drop((frame_a, frame_b));
    assert_eq!(pusher.transmitting(), 0);
}

/// After a wire closes, its pad's lease is the poster's to release — a
/// takeover by another wire must NOT park the closed wire's pad (the lease
/// may already belong to someone else). The close forgets the binding; the
/// next acquisition routes without parking anything.
#[test]
fn takeover_after_close_never_parks_the_closed_wires_pad() {
    let driver = driver();
    let mailboxes: [WireMailbox; 2] = core::array::from_fn(|_| WireMailbox::new());
    let pads = RecorderPads::default();
    let log = Rc::clone(&pads.0);
    let mut pusher = Pusher::new(&driver, &mailboxes, pads, &SLOTS[..1], 1);

    let frame_a: Box<[u8]> = ramp_frame(4).into_boxed_slice();
    // SAFETY: quiesced by the acked close below.
    let seq_a = unsafe { mailboxes[0].post(10, frame_a.as_ptr(), frame_a.len()) };
    pusher.service();
    let close_a = mailboxes[0].request_close();
    pusher.service();
    assert!(mailboxes[0].close_acked(close_a));
    assert_eq!(mailboxes[0].completed_outcome(seq_a), Some(WireOutcome::Aborted));
    drop(frame_a);

    log.borrow_mut().clear();
    let frame_b: Box<[u8]> = ramp_frame(4).into_boxed_slice();
    // SAFETY: drained below.
    let seq_b = unsafe { mailboxes[1].post(11, frame_b.as_ptr(), frame_b.len()) };
    for _ in 0..10_000 {
        if mailboxes[1].completed_outcome(seq_b).is_some() {
            break;
        }
        tick(&driver, &mut pusher);
    }
    assert_eq!(mailboxes[1].completed_outcome(seq_b), Some(WireOutcome::Transmitted));
    assert_eq!(
        log.borrow().as_slice(),
        &[PadEvent::Route { slot: 0, gpio: 11 }],
        "no park of the closed wire's pad — its lease has moved on"
    );
    drop(frame_b);
}

/// A slot channel the driver never configured surfaces as `StartFailed`
/// rather than a wedged wire — the defect is the poster's to report.
#[test]
fn unconfigured_slot_reports_start_failed() {
    let driver: Ws281xDriver<MockRmt, 4> = Ws281xDriver::new(MockRmt::new(4, 48));
    // Deliberately no configure_default_clock.
    let mailboxes: [WireMailbox; 1] = core::array::from_fn(|_| WireMailbox::new());
    let mut pusher = Pusher::new(
        &driver,
        &mailboxes,
        RecorderPads::default(),
        &SLOTS[..1],
        1,
    );

    let frame: Box<[u8]> = ramp_frame(4).into_boxed_slice();
    // SAFETY: disposed of (StartFailed) before the drop.
    let seq = unsafe { mailboxes[0].post(10, frame.as_ptr(), frame.len()) };
    pusher.service();
    assert_eq!(
        mailboxes[0].completed_outcome(seq),
        Some(WireOutcome::StartFailed)
    );
    drop(frame);
}
