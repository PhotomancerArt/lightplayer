//! The first-interrupt replay, and replay recovery.
//!
//! `start_frame` prefills both halves and plants no guard, so a late first
//! refill interrupt lets the transmitter go round the prefilled window again.
//! The re-sent bits are inserted into the stream and the frame's last bits
//! spill past its final pixel into LEDs wired after it, which nothing ever
//! addresses again: on a field ESP32-C6 they froze on pattern colours. These
//! tests run a model strip longer than the frame and look at what it holds.
//!
//! Needs the `mock` feature (on by default) for `MockRmt`/`Pump`.
#![cfg(feature = "mock")]

mod common;

use common::{expected_words, ramp_frame};
use lp_ws281x::{ChannelTiming, MockRmt, PulseCodes, Pump, Ws281xDriver};

const TIMING: ChannelTiming = ChannelTiming::WS2812;

/// The C6's two-channel window: one 48-word block, 24-word halves.
const RAM_WORDS: usize = 48;
const HALF: usize = RAM_WORDS / 2;

/// Pixels in the frame, and LEDs physically wired after them.
const PIXELS: usize = 12;
const EXTRA: usize = 6;

fn driver(recovery_tail: Option<usize>) -> Ws281xDriver<MockRmt, 1> {
    let d: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, RAM_WORDS));
    d.configure_default_clock(0, &TIMING).unwrap();
    if let Some(pixels) = recovery_tail {
        d.enable_replay_recovery(0, pixels).unwrap();
    }
    d
}

#[test]
fn without_recovery_a_late_first_interrupt_freezes_pattern_data_past_the_frame() {
    let d = driver(None);
    let frame = ramp_frame(PIXELS);
    let mut strip = Strip::new(PIXELS + EXTRA);

    send(&d, &frame, Some(HALF + 3), &mut strip);
    // A healthy frame after it addresses only its own pixels.
    send(&d, &frame, None, &mut strip);

    let wire = pixel_words(&frame);
    assert_eq!(
        &strip.pixels[..PIXELS],
        &wire[..],
        "the mapped pixels recover"
    );
    // The replayed half pushed the frame's last pixel one LED down the chain,
    // and nothing rewrites it: pattern data, frozen.
    assert_eq!(strip.pixels[PIXELS], wire[PIXELS - 1]);
    assert!(!strip.is_black(PIXELS));

    let stats = d.stats(0);
    assert_eq!(
        stats.replays, 1,
        "the replay is counted with recovery off too"
    );
    assert_eq!(stats.resends, 0);
    assert_eq!(stats.guard_trips, 0, "a replay is not a truncation");
    assert_eq!(stats.frames, 2);
}

#[test]
fn with_recovery_a_late_first_interrupt_leaves_everything_past_the_frame_black() {
    let d = driver(Some(4));
    let frame = ramp_frame(PIXELS);
    let mut strip = Strip::new(PIXELS + EXTRA);
    // What the LEDs after the frame held before: pattern colour from the
    // field bug, say.
    strip.pixels[PIXELS] = pixel_words(&frame)[0].clone();

    let transmissions = send(&d, &frame, Some(HALF + 3), &mut strip);
    assert_eq!(transmissions, 2, "the replayed frame is sent again at once");

    let wire = pixel_words(&frame);
    assert_eq!(&strip.pixels[..PIXELS], &wire[..]);
    for led in PIXELS..PIXELS + 4 {
        assert!(strip.is_black(led), "LED {led} past the frame is not black");
    }

    let stats = d.stats(0);
    assert_eq!(stats.replays, 1);
    assert_eq!(stats.resends, 1);
    assert_eq!(stats.frames, 1, "one frame, sent twice");
    assert_eq!(stats.guard_trips, 0);
}

#[test]
fn the_replay_is_detected_at_the_first_refill_and_the_resend_is_clean() {
    let d = driver(Some(0));
    let frame = ramp_frame(PIXELS);
    let mut strip = Strip::new(PIXELS + EXTRA);
    let codes = codes();

    // SAFETY: `frame` outlives the run, which ends with the channel complete.
    unsafe { d.start_frame(0, &frame).unwrap() };
    late_first_interrupt(&d, HALF + 3);
    assert_eq!(
        d.stats(0).replays,
        1,
        "seen at the first refill, before tx_end"
    );
    Pump::default().run(&d);
    let log = d.hw().emitted(0);
    assert!(d.is_complete(0));

    let pieces: Vec<&[u32]> = log.split_inclusive(|w| *w == codes.latch).collect();
    assert_eq!(pieces.len(), 2);
    let tail_bits = d.channel(0).unwrap().tail_bits();
    assert_eq!(tail_bits, 2 * 24, "a whole window, rounded up to pixels");

    // The first transmission: the prefill, the replayed first half, then the
    // rest — one half longer than the frame.
    let expected = black_tailed(&frame, tail_bits);
    let mut replayed = expected[..2 * HALF].to_vec();
    replayed.extend_from_slice(&expected[..HALF]);
    replayed.extend_from_slice(&expected[2 * HALF..]);
    assert_eq!(pieces[0], &replayed[..]);
    // The resend is the frame, then its black tail, exactly.
    assert_eq!(pieces[1], &expected[..]);

    strip.latch(pieces[0]);
    strip.latch(pieces[1]);
    assert!(strip.is_black(PIXELS) && strip.is_black(PIXELS + 1));
}

#[test]
fn a_whole_window_replay_is_not_seen_but_the_next_frame_blackens_its_spill() {
    // Late by two halves and more: the pointer is back in the second half,
    // where an on-time service finds it.
    let d = driver(Some(4));
    let frame = ramp_frame(PIXELS);
    let mut strip = Strip::new(PIXELS + EXTRA);

    assert_eq!(send(&d, &frame, Some(2 * HALF + 3), &mut strip), 1);
    assert_eq!(d.stats(0).replays, 0);
    assert!(
        !strip.is_black(PIXELS),
        "the spill: two halves of pattern data"
    );

    send(&d, &frame, None, &mut strip);
    for led in PIXELS..PIXELS + 4 {
        assert!(strip.is_black(led), "LED {led}");
    }
}

#[test]
fn a_frame_that_ends_inside_the_prefill_gets_no_tail() {
    let d = driver(Some(4));
    // Data, latch and a STOP word fit in 48 words: one pixel.
    let frame = ramp_frame(1);
    // SAFETY: as above.
    unsafe { d.start_frame(0, &frame).unwrap() };
    Pump::default().run(&d);
    assert_eq!(d.hw().emitted(0), expected_words(&frame, &TIMING));
    assert_eq!(d.channel(0).unwrap().total_bits(), 24);

    // Two pixels plus the latch fill the window exactly and leave no STOP:
    // that one can replay, so it is tailed.
    let d = driver(Some(4));
    let frame = ramp_frame(2);
    // SAFETY: as above.
    unsafe { d.start_frame(0, &frame).unwrap() };
    Pump::default().run(&d);
    assert_eq!(d.channel(0).unwrap().total_bits(), (2 + 4) * 24);
    assert_eq!(d.channel(0).unwrap().data_bits(), 2 * 24);
}

#[test]
fn a_healthy_run_is_the_frame_and_its_tail_and_nothing_else() {
    let d = driver(Some(3));
    for pixels in [3usize, 9, 33] {
        let frame = ramp_frame(pixels);
        d.hw().clear_emitted(0);
        // SAFETY: as above.
        unsafe { d.start_frame(0, &frame).unwrap() };
        Pump::default().run(&d);
        assert_eq!(
            d.hw().emitted(0),
            black_tailed(&frame, 3 * 24),
            "pixels={pixels}"
        );
    }
    let stats = d.stats(0);
    assert_eq!((stats.replays, stats.resends, stats.guard_trips), (0, 0, 0));
    assert_eq!(stats.frames, 3);
}

#[test]
fn a_resend_that_replays_again_completes_as_it_is() {
    let d = driver(Some(4));
    let frame = ramp_frame(PIXELS);
    // SAFETY: as above.
    unsafe { d.start_frame(0, &frame).unwrap() };
    late_first_interrupt(&d, HALF + 3);
    run_to_end(&d);
    // The resend has started; make its first interrupt late too.
    assert!(!d.is_complete(0));
    late_first_interrupt(&d, HALF + 3);
    Pump::default().run(&d);
    assert!(d.is_complete(0));
    let stats = d.stats(0);
    assert_eq!((stats.replays, stats.resends, stats.frames), (2, 1, 1));
}

#[test]
fn abort_during_a_resend_leaves_the_transmitter_stopped() {
    let d = driver(Some(4));
    let frame = ramp_frame(PIXELS);
    // SAFETY: the channel is aborted below, before `frame` is dropped.
    unsafe { d.start_frame(0, &frame).unwrap() };
    late_first_interrupt(&d, HALF + 3);
    run_to_end(&d);
    assert!(d.hw().is_running(0), "the resend is on the wire");
    assert!(d.abort(0));
    assert!(!d.hw().is_running(0));
    assert!(d.is_complete(0));
}

#[test]
fn configure_turns_recovery_off_and_recovery_needs_a_configured_channel() {
    let d: Ws281xDriver<MockRmt, 2> = Ws281xDriver::new(MockRmt::new(2, RAM_WORDS));
    assert!(d.enable_replay_recovery(0, 4).is_err());
    assert!(d.enable_replay_recovery(5, 4).is_err());
    d.configure_default_clock(0, &TIMING).unwrap();
    d.enable_replay_recovery(0, 4).unwrap();
    assert_eq!(d.channel(0).unwrap().tail_bits(), 4 * 24);
    d.configure_default_clock(0, &TIMING).unwrap();
    assert_eq!(d.channel(0).unwrap().tail_bits(), 0);
}

// --- helpers -----------------------------------------------------------------

fn codes() -> PulseCodes {
    PulseCodes::at_default_clock(&TIMING).unwrap()
}

/// Send `frame`, with its first refill interrupt `late` words after it was
/// raised if given, and latch every transmission on `strip`. Returns how many
/// transmissions the frame took.
fn send(
    d: &Ws281xDriver<MockRmt, 1>,
    frame: &[u8],
    late: Option<usize>,
    strip: &mut Strip,
) -> usize {
    d.hw().clear_emitted(0);
    // SAFETY: `frame` outlives the run, which ends with the channel complete.
    unsafe { d.start_frame(0, frame).unwrap() };
    if let Some(late) = late {
        late_first_interrupt(d, late);
    }
    Pump::default().run(d);
    assert!(d.is_complete(0));
    let log = d.hw().emitted(0);
    let latch = codes().latch;
    let mut transmissions = 0;
    for piece in log.split_inclusive(|w| *w == latch) {
        strip.latch(piece);
        transmissions += 1;
    }
    transmissions
}

/// Run the transmitter to its first threshold event and on for `late` more
/// words before the handler gets to it.
fn late_first_interrupt(d: &Ws281xDriver<MockRmt, 1>, late: usize) {
    let mock = d.hw();
    while mock.peek_interrupts().threshold == 0 {
        assert_eq!(mock.advance(0, 1), 1, "no threshold event");
    }
    mock.advance(0, late);
    d.on_interrupt();
}

/// Service on time until the transmission's `tx_end` has been handled.
fn run_to_end(d: &Ws281xDriver<MockRmt, 1>) {
    let mock = d.hw();
    loop {
        mock.advance(0, 1);
        let end = mock.peek_interrupts().end != 0;
        if mock.has_pending() {
            mock.advance(0, 1);
            d.on_interrupt();
        }
        if end {
            return;
        }
    }
}

/// `frame`'s words, then `tail_bits` black bits, then the latch.
fn black_tailed(frame: &[u8], tail_bits: usize) -> Vec<u32> {
    let mut words = expected_words(frame, &TIMING);
    let latch = words.pop().unwrap();
    words.extend(std::iter::repeat_n(codes().zero, tail_bits));
    words.push(latch);
    words
}

/// `frame`'s words, one entry per pixel.
fn pixel_words(frame: &[u8]) -> Vec<Vec<u32>> {
    let mut words = expected_words(frame, &TIMING);
    words.pop();
    words.chunks(24).map(<[u32]>::to_vec).collect()
}

/// A chain of WS281x LEDs: each transmission between latches sets the pixels
/// it reaches, in order, and every LED it does not reach keeps what it held.
struct Strip {
    pixels: Vec<Vec<u32>>,
}

impl Strip {
    fn new(leds: usize) -> Self {
        Self {
            pixels: vec![vec![codes().zero; 24]; leds],
        }
    }

    fn latch(&mut self, transmission: &[u32]) {
        let latch = codes().latch;
        let bits: Vec<u32> = transmission
            .iter()
            .copied()
            .filter(|w| *w != latch)
            .collect();
        for (led, chunk) in self.pixels.iter_mut().zip(bits.chunks_exact(24)) {
            *led = chunk.to_vec();
        }
    }

    fn is_black(&self, led: usize) -> bool {
        self.pixels[led].iter().all(|w| *w == codes().zero)
    }
}
