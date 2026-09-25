//! Learned tables survive loss: a board table and a host table driven over the
//! committed traffic sample, with frames the board abandons (it rolls back),
//! frames lost whole or torn in flight (the host never sees them, or sees a
//! prefix), and the host's reset request answered a few frames later.
//!
//! The invariant: every frame the host accepts decodes byte-exact, and every
//! divergence is caught by the header before a wrong decode.

#[path = "support/fixture_dictionary.rs"]
mod fixture_dictionary;

use fixture_dictionary::{fixture_dictionary, fixture_lines};
use lp_json_pack::pack_learned::{HeaderMismatch, LearnStore, LearnedTable};
use lp_json_pack::{DecodeError, Dictionary, PackEncoder, decode_learned};

static SEED: Dictionary = Dictionary::EMPTY;

#[test]
fn a_clean_link_decodes_every_frame() {
    let r = run(Loss::NONE, 1);
    assert_eq!(r.decoded, r.sent);
    assert_eq!((r.desyncs, r.wrong), (0, 0));
}

#[test]
fn a_seed_and_a_table_compose() {
    // Codes past the seed are the table's: every line decodes with a
    // non-empty seed too, and the table learns only what the seed lacks.
    let seed = fixture_dictionary();
    let mut board = LearnedTable::NEW;
    let mut host = LearnedTable::NEW;
    let mut buf = vec![0u8; 1 << 17];
    let mut out = Vec::new();
    for line in fixture_lines().iter().filter(|l| l.dir == b'<') {
        let mut enc = PackEncoder::with_learned(&mut buf, seed, &mut board);
        enc.json_value(line.json).unwrap();
        let n = enc.finish().unwrap();
        out.clear();
        decode_learned(seed, &mut host, &buf[..n], &mut out).unwrap();
        assert_eq!(out, line.json);
    }
    assert_eq!(board.mark(), host.mark());
    assert!(board.find_key(b"msg").is_none(), "a seeded key was learned");
}

#[test]
fn losses_never_decode_wrong_and_always_recover() {
    let mut total = Report::default();
    for seed in 1..=24u64 {
        let r = run(
            Loss {
                abandon_one_in: 23,
                lose_one_in: 29,
                tear_one_in: 31,
            },
            seed,
        );
        assert_eq!(r.wrong, 0, "seed {seed}: {r:?}");
        // Recovery: the last pass is clean, and within one re-ask (plus the
        // board's answer) the link decodes again, to the end.
        assert!(
            r.clean_tail_decoded + REASK_EVERY as usize + 3 >= r.clean_tail,
            "seed {seed}: {r:?}"
        );
        total.add(&r);
    }
    // Losses happened, desyncs happened, and most frames still got through.
    assert!(total.abandoned > 0 && total.lost > 0 && total.torn > 0, "{total:?}");
    assert!(total.desyncs > 0, "{total:?}");
    assert!(total.decoded * 10 > total.sent * 7, "{total:?}");
}

#[test]
fn abandoned_frames_alone_never_desync() {
    // The board knows about every abandoned write and rolls it back, so the
    // two tables never part.
    for seed in 1..=8u64 {
        let r = run(
            Loss {
                abandon_one_in: 5,
                ..Loss::NONE
            },
            seed,
        );
        assert!(r.abandoned > 0);
        assert_eq!((r.desyncs, r.wrong), (0, 0), "seed {seed}: {r:?}");
        assert_eq!(r.decoded, r.sent - r.abandoned);
    }
}

#[test]
fn a_torn_first_sighting_is_caught_although_the_counts_agree() {
    // The board sees "AA" for the first time in a frame the host never
    // decodes. Next frame the board learns it; the host would not. Both
    // tables hold zero entries when that next frame is checked, so a
    // count-only header passes it. The state hash must not.
    let mut board = LearnedTable::NEW;
    let mut host = LearnedTable::NEW;
    let mut buf = [0u8; 64];
    let _torn = encode(&mut buf, &mut board, br#"["AA"]"#);
    assert_eq!(board.mark().values, host.mark().values);
    assert_eq!(board.mark().keys, host.mark().keys);
    let n = encode(&mut buf, &mut board, br#"["AA","x"]"#).len();
    let mut out = Vec::new();
    assert!(matches!(
        decode_learned(&SEED, &mut host, &buf[..n], &mut out),
        Err(DecodeError::Learned(HeaderMismatch::State { .. }))
    ));
    assert_eq!(host.mark(), LearnedTable::NEW.mark());
}

#[test]
fn a_decode_error_leaves_the_host_table_as_it_was() {
    let mut board = LearnedTable::NEW;
    let mut host = LearnedTable::NEW;
    let mut buf = [0u8; 256];
    let n = encode(&mut buf, &mut board, br#"{"alpha":"beta","gamma":[1,2,3]}"#).len();
    let before = host.mark();
    let mut out = Vec::new();
    assert!(decode_learned(&SEED, &mut host, &buf[..n - 3], &mut out).is_err());
    assert_eq!(host.mark(), before);
    // The whole frame still decodes afterwards: nothing half-learned.
    out.clear();
    decode_learned(&SEED, &mut host, &buf[..n], &mut out).unwrap();
    assert_eq!(out, br#"{"alpha":"beta","gamma":[1,2,3]}"#);
    assert_eq!(host.mark(), board.mark());
}

#[derive(Clone, Copy)]
struct Loss {
    /// The board abandons the write (and knows it).
    abandon_one_in: u64,
    /// The frame is lost whole in flight (the board thinks it was sent).
    lose_one_in: u64,
    /// The frame arrives short in flight (the board thinks it was sent).
    tear_one_in: u64,
}

impl Loss {
    const NONE: Loss = Loss {
        abandon_one_in: 0,
        lose_one_in: 0,
        tear_one_in: 0,
    };
}

#[derive(Debug, Default)]
struct Report {
    sent: usize,
    abandoned: usize,
    lost: usize,
    torn: usize,
    decoded: usize,
    desyncs: usize,
    wrong: usize,
    clean_tail: usize,
    clean_tail_decoded: usize,
}

impl Report {
    fn add(&mut self, r: &Report) {
        self.sent += r.sent;
        self.abandoned += r.abandoned;
        self.lost += r.lost;
        self.torn += r.torn;
        self.decoded += r.decoded;
        self.desyncs += r.desyncs;
        self.wrong += r.wrong;
    }
}

/// Frames between the host's reset requests while it stays desynced: the
/// frame-count stand-in for `PackOptIn`'s re-ask interval.
const REASK_EVERY: u64 = 12;

/// Every board→host line of the sample, three times over (so the tables fill
/// and the steady state is exercised), through one board and one host table.
/// Losses happen in the first two passes; the third is clean.
fn run(loss: Loss, seed: u64) -> Report {
    let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut board = LearnedTable::NEW;
    let mut host = LearnedTable::NEW;
    let mut buf = vec![0u8; 1 << 17];
    let mut out = Vec::new();
    let mut r = Report::default();
    // Frames until the board sees the host's reset request (`SetEncoding`).
    let mut reset_in: Option<u64> = None;
    // Frames since the host last asked, while desynced.
    let mut desynced: Option<u64> = None;
    let lines: Vec<_> = fixture_lines().iter().filter(|l| l.dir == b'<').collect();
    let lossy = lines.len() * 2;
    for (i, line) in lines.iter().cycle().take(lines.len() * 3).enumerate() {
        let clean = i >= lossy;
        if let Some(n) = desynced.as_mut() {
            *n += 1;
            if *n >= REASK_EVERY && reset_in.is_none() {
                *n = 0;
                reset_in = Some(rng.next() % 3);
            }
        }
        if let Some(n) = reset_in.as_mut() {
            if *n == 0 {
                let next = board.epoch().wrapping_add(1);
                board.reset(next);
                reset_in = None;
            } else {
                *n -= 1;
            }
        }
        let mark = board.mark();
        let frame = encode(&mut buf, &mut board, line.json).to_vec();
        r.sent += 1;
        r.clean_tail += usize::from(clean);
        if !clean && rng.one_in(loss.abandon_one_in) {
            board.truncate(mark);
            r.abandoned += 1;
            continue;
        }
        let delivered: &[u8] = if !clean && rng.one_in(loss.lose_one_in) {
            r.lost += 1;
            continue;
        } else if !clean && rng.one_in(loss.tear_one_in) {
            r.torn += 1;
            let keep = (rng.next() as usize) % frame.len();
            &frame[..keep]
        } else {
            &frame
        };
        out.clear();
        match decode_learned(&SEED, &mut host, delivered, &mut out) {
            Ok(()) => {
                if out == line.json && delivered.len() == frame.len() {
                    r.decoded += 1;
                    r.clean_tail_decoded += usize::from(clean);
                    desynced = None;
                } else {
                    r.wrong += 1;
                }
            }
            Err(DecodeError::Learned(_)) => {
                // The host drops it and asks for a reset, then again every
                // REASK_EVERY frames while it stays desynced.
                if desynced.is_none() {
                    r.desyncs += 1;
                    desynced = Some(0);
                    if reset_in.is_none() {
                        reset_in = Some(rng.next() % 3);
                    }
                }
            }
            Err(_) => {} // a torn frame: the host truncated its own tentative learning
        }
    }
    r
}

fn encode<'b>(buf: &'b mut [u8], table: &mut LearnedTable, json: &[u8]) -> &'b [u8] {
    let mut enc = PackEncoder::with_learned(buf, &SEED, table);
    enc.json_value(json).unwrap();
    let n = enc.finish().unwrap();
    &buf[..n]
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn one_in(&mut self, n: u64) -> bool {
        n != 0 && self.next() % n == 0
    }
}
